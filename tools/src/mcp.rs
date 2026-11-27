// SPDX-License-Identifier: Apache-2.0

//! `mcp`: a stdio MCP server with read-only tools. Newline-delimited JSON-RPC
//! for clients of the stateless 2026-07-28 revision (`server/discover`, the
//! version in each request's `_meta`) and of the 2025-06-18 and 2025-11-25 revisions that open
//! with `initialize`.
//!
//! Hostile input: a message over `MAX_MESSAGE` bytes is refused unread, every
//! path must resolve inside the working root, a file is read up to `MAX_FILE`
//! bytes, and no tool starts a process or writes a file.

use crate::report::{self, Args};
use serde_json::{Value, json};
use std::io::{BufRead, Read, Write};
use std::path::{Component, Path, PathBuf};

/// The largest message accepted, newline included.
pub const MAX_MESSAGE: usize = 1 << 20;
/// The largest file a tool reads.
pub const MAX_FILE: u64 = 16 << 20;
const MODERN: &str = "2026-07-28";
// Not 2025-03-26: it requires JSON-RPC batches, which this server refuses.
const LEGACY: [&str; 2] = ["2025-11-25", "2025-06-18"];
const VERSION_KEY: &str = "io.modelcontextprotocol/protocolVersion";
const AUDIT: &str = include_str!("../../skills/puritanical-audit/SKILL.md");

/// A JSON-RPC error: code, message and optional data.
type Fail = (i64, String, Option<Value>);

/// The `mcp` command: serves `input` from the working root `cwd`. Returns the
/// exit status.
pub fn main(
    cwd: std::io::Result<PathBuf>,
    input: impl BufRead,
    output: impl Write,
    mut stderr: impl Write,
) -> u8 {
    match cwd.and_then(|root| serve(&root, input, output)) {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(stderr, "puritanical-tools mcp: {e}");
            1
        }
    }
}

/// Answers each request in `input` until it ends.
///
/// # Errors
/// The root cannot be resolved, or reading or writing fails.
pub fn serve(root: &Path, mut input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
    let root = root.canonicalize()?;
    loop {
        let mut line = Vec::new();
        let limit = MAX_MESSAGE as u64 + 1;
        if input.by_ref().take(limit).read_until(b'\n', &mut line)? == 0 {
            return Ok(());
        }
        let reply = if line.len() > MAX_MESSAGE {
            if line.last() != Some(&b'\n') {
                skip_line(&mut input)?;
            }
            Some(reply(
                &Value::Null,
                Err((-32600, "message too large".into(), None)),
            ))
        } else {
            handle(&root, &line)
        };
        if let Some(r) = reply {
            writeln!(output, "{r}")?;
            output.flush()?;
        }
    }
}

/// Discards the rest of an oversized line without holding it.
fn skip_line(input: &mut impl BufRead) -> std::io::Result<()> {
    let mut sink = Vec::new();
    loop {
        sink.clear();
        let n = input.by_ref().take(1 << 16).read_until(b'\n', &mut sink)?;
        if n == 0 || sink.last() == Some(&b'\n') {
            return Ok(());
        }
    }
}

fn reply(id: &Value, result: Result<Value, Fail>) -> String {
    match result {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
        Err((code, message, data)) => {
            let mut error = json!({"code": code, "message": message});
            if let Some(data) = data {
                error["data"] = data;
            }
            json!({"jsonrpc": "2.0", "id": id, "error": error})
        }
    }
    .to_string()
}

/// One message: a reply for a request, none for a notification.
fn handle(root: &Path, line: &[u8]) -> Option<String> {
    if line.trim_ascii().is_empty() {
        return None;
    }
    let Ok(msg) = serde_json::from_slice::<Value>(line) else {
        return Some(reply(
            &Value::Null,
            Err((-32700, "parse error".into(), None)),
        ));
    };
    // A response from the client is never answered (this server sends no
    // requests, so it has nothing to match it with).
    if msg.get("method").is_none() && (msg.get("result").is_some() || msg.get("error").is_some()) {
        return None;
    }
    let id = msg.get("id").cloned();
    let Some(method) = msg.get("method").and_then(Value::as_str) else {
        let id = id.unwrap_or(Value::Null);
        return Some(reply(&id, Err((-32600, "invalid request".into(), None))));
    };
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    Some(reply(&id?, respond(root, method, &params)))
}

fn info() -> Value {
    json!({"name": "puritanical-tools", "version": env!("CARGO_PKG_VERSION")})
}

fn respond(root: &Path, method: &str, params: &Value) -> Result<Value, Fail> {
    let requested = params.pointer("/_meta").and_then(|m| m.get(VERSION_KEY));
    if let Some(v) = requested.filter(|v| v.as_str() != Some(MODERN)) {
        let data = json!({"supported": [MODERN], "requested": v});
        return Err((-32022, "Unsupported protocol version".into(), Some(data)));
    }
    Ok(match method {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(Value::as_str);
            let version = LEGACY.into_iter().find(|v| Some(*v) == asked);
            json!({
                "protocolVersion": version.unwrap_or(LEGACY[0]),
                "capabilities": {"tools": {}},
                "serverInfo": info(),
            })
        }
        "server/discover" => json!({
            "resultType": "complete",
            "supportedVersions": [MODERN],
            "capabilities": {"tools": {}},
            "_meta": {"io.modelcontextprotocol/serverInfo": info()},
            "ttlMs": 0,
            "cacheScope": "public",
        }),
        "ping" => json!({"resultType": "complete"}),
        "tools/list" => json!({
            "resultType": "complete",
            "tools": tools(),
            "ttlMs": 3_600_000,
            "cacheScope": "public",
        }),
        "tools/call" => call(root, params)?,
        _ => return Err((-32601, format!("method not found: {method}"), None)),
    })
}

fn path_schema(what: &str) -> Value {
    json!({"type": "string", "description": format!("{what}, relative to the working root")})
}

fn tools() -> Value {
    let read_only = json!({"readOnlyHint": true, "openWorldHint": false});
    let one_file = |what: &str| json!({"type": "object", "properties": {"path": path_schema(what)}, "required": ["path"]});
    json!([
        {
            "name": "report",
            "description": "Measured before-and-after report as Markdown: tool outputs' bytes before and after the compressor, audit finding lines before and after, and the compressor's savings ledger. No token estimates.",
            "inputSchema": {"type": "object", "properties": {
                "outputs": {"type": "array", "items": path_schema("A saved tool output")},
                "audit_before": path_schema("A puritanical-audit output before the change"),
                "audit_after": path_schema("A puritanical-audit output after the change"),
                "stats": path_schema("The compressor's .puritanical-compress-stats.json"),
            }},
            "annotations": read_only,
        },
        {
            "name": "compress",
            "description": "A file's text after the compressor's generic tiers (hook defaults), with its bytes before and after.",
            "inputSchema": one_file("The file to compress"),
            "annotations": read_only,
        },
        {
            "name": "audit_findings",
            "description": "Counts the finding lines (path:line  [code|prose]) in a saved puritanical-audit or review output.",
            "inputSchema": one_file("The saved audit or review output"),
            "annotations": read_only,
        },
        {
            "name": "audit_instructions",
            "description": "The puritanical-audit instructions, for running the audit in a host without Puritanical's skills.",
            "inputSchema": {"type": "object", "properties": {}},
            "annotations": read_only,
        },
    ])
}

fn call(root: &Path, params: &Value) -> Result<Value, Fail> {
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return Err((-32602, "tools/call needs a tool name".into(), None));
    };
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let read = |p: &str| file(root, p);
    let out = match name {
        "report" => tool_report(&args, &read),
        "compress" => text_arg(&args, "path").and_then(|p| {
            let text = report::read_text(&read, p)?;
            let out = report::compressed(&text);
            Ok(format!(
                "{p}: {} bytes before, {} bytes after the compressor's generic tiers (hook defaults).\n\n{out}",
                text.len(),
                out.len()
            ))
        }),
        "audit_findings" => text_arg(&args, "path").and_then(|p| {
            let [code, prose] = report::findings(&report::read_text(&read, p)?);
            Ok(format!("{p}: {code} code and {prose} prose finding lines."))
        }),
        "audit_instructions" => Ok(AUDIT.to_string()),
        _ => return Err((-32602, format!("unknown tool: {name}"), None)),
    };
    let (text, is_error) = match out {
        Ok(text) => (text, false),
        Err(text) => (text, true),
    };
    Ok(json!({
        "resultType": "complete",
        "content": [{"type": "text", "text": text}],
        "isError": is_error,
    }))
}

fn text_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("argument '{key}' must be a string"))
}

fn tool_report(
    args: &Value,
    read: &dyn Fn(&str) -> Result<Vec<u8>, String>,
) -> Result<String, String> {
    // A null optional argument counts as absent.
    let opt = |key| {
        args.get(key)
            .filter(|v| !v.is_null())
            .map(|_| text_arg(args, key))
            .transpose()
    };
    let outputs = match args.get("outputs") {
        None | Some(Value::Null) => Vec::new(),
        Some(v) => v
            .as_array()
            .and_then(|a| a.iter().map(Value::as_str).collect::<Option<Vec<_>>>())
            .ok_or("argument 'outputs' must be an array of strings")?,
    };
    let r = Args {
        audit_before: opt("audit_before")?,
        audit_after: opt("audit_after")?,
        stats: opt("stats")?,
        svg: None,
        outputs,
    };
    if r.outputs.is_empty() && r.audit_before.or(r.audit_after).or(r.stats).is_none() {
        return Err("report needs outputs, audit_before, audit_after or stats".into());
    }
    Ok(report::markdown(&report::measure(&r, read)?))
}

/// Reads `rel` if it resolves inside `root` (symbolic links followed) and is
/// at most `MAX_FILE` bytes.
fn file(root: &Path, rel: &str) -> Result<Vec<u8>, String> {
    // Refused by form, before any filesystem call: on Windows, resolving a
    // drive or UNC path opens it, which can reach a host the client names,
    // and resolving `..` would reveal whether a file outside the root exists.
    let escapes = |c| {
        matches!(
            c,
            Component::Prefix(_) | Component::RootDir | Component::ParentDir
        )
    };
    if Path::new(rel).components().any(escapes) {
        return Err(format!("{rel}: outside the working root"));
    }
    let path = root
        .join(rel)
        .canonicalize()
        .map_err(|e| format!("{rel}: {e}"))?;
    if !path.starts_with(root) {
        return Err(format!("{rel}: outside the working root"));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(&path)
        .and_then(|f| f.take(MAX_FILE + 1).read_to_end(&mut bytes))
        .map_err(|e| format!("{rel}: {e}"))?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(format!("{rel}: larger than {MAX_FILE} bytes"));
    }
    Ok(bytes)
}
