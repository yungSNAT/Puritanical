// SPDX-License-Identifier: Apache-2.0

//! `mcp`: both protocol eras, the read-only tools and each safety property.

use puritanical_tools::mcp::{MAX_FILE, MAX_MESSAGE, main, serve};
use serde_json::{Value, json};
use std::fmt::Write as _;
use std::io::{Cursor, Write};
use std::path::Path;
use std::process::{Command, Stdio};

/// The replies `serve` writes for `input`, one JSON value each.
fn session(root: &Path, input: &[u8]) -> Vec<Value> {
    let mut out = Vec::new();
    serve(root, Cursor::new(input), &mut out).expect("served");
    String::from_utf8(out)
        .expect("utf-8")
        .lines()
        .map(|l| serde_json::from_str(l).expect("one JSON message a line"))
        .collect()
}

fn request(id: u32, method: &str, params: &Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string() + "\n"
}

/// One tool call's text and error flag.
fn call(root: &Path, name: &str, args: &Value) -> (String, bool) {
    let r = session(
        root,
        request(1, "tools/call", &json!({"name": name, "arguments": args})).as_bytes(),
    );
    let result = &r[0]["result"];
    assert_eq!(result["resultType"], "complete");
    (
        result["content"][0]["text"]
            .as_str()
            .expect("text")
            .to_string(),
        result["isError"].as_bool().expect("flag"),
    )
}

fn root() -> tempfile::TempDir {
    tempfile::tempdir().expect("temp dir")
}

#[test]
fn legacy_clients_initialise_and_list_the_tools() {
    let dir = root();
    let mut input = request(1, "initialize", &json!({"protocolVersion": "2025-06-18"}));
    input += &request(2, "initialize", &json!({"protocolVersion": "1999-01-01"}));
    input += "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n\n";
    input += &request(3, "tools/list", &Value::Null);
    input += &request(4, "ping", &json!({}));
    let r = session(dir.path(), input.as_bytes());
    assert_eq!(r.len(), 4, "a notification and a blank line get no reply");
    assert_eq!(r[0]["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(r[1]["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(r[0]["result"]["serverInfo"]["name"], "puritanical-tools");
    let names: Vec<&str> = r[2]["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|t| t["name"].as_str().expect("name"))
        .collect();
    assert_eq!(
        names,
        ["report", "compress", "audit_findings", "audit_instructions"]
    );
    for t in r[2]["result"]["tools"].as_array().expect("tools") {
        assert_eq!(t["annotations"]["readOnlyHint"], true);
    }
    assert_eq!(
        r[3],
        json!({"jsonrpc": "2.0", "id": 4, "result": {"resultType": "complete"}})
    );
}

#[test]
fn modern_clients_discover_and_get_version_errors() {
    let dir = root();
    let meta = |v: &str| json!({"_meta": {"io.modelcontextprotocol/protocolVersion": v}});
    let mut input = request(1, "server/discover", &meta("2026-07-28"));
    input += &request(2, "tools/list", &meta("2026-07-28"));
    input += &request(3, "tools/list", &meta("2099-01-01"));
    input += &request(4, "resources/list", &meta("2026-07-28"));
    let r = session(dir.path(), input.as_bytes());
    assert_eq!(r[0]["result"]["supportedVersions"], json!(["2026-07-28"]));
    assert_eq!(r[0]["result"]["capabilities"], json!({"tools": {}}));
    assert_eq!(r[1]["result"]["cacheScope"], "public");
    assert_eq!(r[1]["result"]["ttlMs"], 3_600_000);
    assert_eq!(r[2]["error"]["code"], -32022);
    assert_eq!(
        r[2]["error"]["data"],
        json!({"supported": ["2026-07-28"], "requested": "2099-01-01"})
    );
    assert_eq!(r[3]["error"]["code"], -32601);
}

#[test]
fn malformed_messages_get_json_rpc_errors() {
    let dir = root();
    let input = "not json\n{\"jsonrpc\":\"2.0\",\"id\":7}\n{\"jsonrpc\":\"2.0\",\"id\":8,\"method\":\"tools/call\",\"params\":{}}\n";
    let r = session(dir.path(), input.as_bytes());
    assert_eq!(r[0]["error"]["code"], -32700);
    assert_eq!(r[1]["error"]["code"], -32600);
    assert_eq!(r[1]["id"], 7);
    assert_eq!(r[2]["error"]["code"], -32602);
    let r = session(
        dir.path(),
        request(9, "tools/call", &json!({"name": "rm"})).as_bytes(),
    );
    assert_eq!(r[0]["error"]["message"], "unknown tool: rm");
}

#[test]
fn the_read_only_tools_measure_files_in_the_root() {
    let dir = root();
    // Varied lines: scrub collapses identical ones before elision is reached.
    let mut log = String::new();
    for i in 0..300 {
        writeln!(log, "step {i:03}: compiling module number {i} of the build").expect("write");
    }
    std::fs::write(dir.path().join("build.log"), &log).expect("write");
    std::fs::write(
        dir.path().join("audit.txt"),
        "src/a.rs:3  [code]  wrapper → inline.\nREADME.md:1  [prose]  recap → cut.\n",
    )
    .expect("write");

    let (text, err) = call(dir.path(), "compress", &json!({"path": "build.log"}));
    assert!(!err);
    assert!(
        text.starts_with(&format!("build.log: {} bytes before, ", log.len())),
        "{text}"
    );
    assert!(text.contains("puritanical: elided"), "{text}");

    let (text, err) = call(dir.path(), "audit_findings", &json!({"path": "audit.txt"}));
    assert!(!err);
    assert_eq!(text, "audit.txt: 1 code and 1 prose finding lines.");

    let args = json!({"outputs": ["build.log"], "audit_after": "audit.txt"});
    let (text, err) = call(dir.path(), "report", &args);
    assert!(!err);
    assert!(
        text.contains("## Tool output") && text.contains("## Audit findings"),
        "{text}"
    );
    assert!(!dir.path().join("report.svg").exists());

    let (text, err) = call(dir.path(), "audit_instructions", &json!({}));
    assert!(!err && text.contains("name: puritanical-audit"));
}

#[test]
fn tool_argument_errors_are_tool_results() {
    let dir = root();
    std::fs::create_dir(dir.path().join("sub")).expect("mkdir");
    for (name, args, want) in [
        ("compress", json!({}), "argument 'path' must be a string"),
        ("compress", json!({"path": "missing.log"}), "missing.log: "),
        // A directory resolves inside the root but cannot be read as a file.
        ("compress", json!({"path": "sub"}), "sub: "),
        ("report", json!({}), "report needs outputs"),
        (
            "report",
            json!({"outputs": [1]}),
            "must be an array of strings",
        ),
        (
            "report",
            json!({"stats": 5}),
            "argument 'stats' must be a string",
        ),
        (
            "audit_findings",
            json!({"path": "missing.txt"}),
            "missing.txt: ",
        ),
    ] {
        let (text, err) = call(dir.path(), name, &args);
        assert!(err, "{name} {args}");
        assert!(text.contains(want), "{text}");
    }
    let r = session(
        dir.path(),
        request(1, "tools/call", &json!({"name": "audit_instructions"})).as_bytes(),
    );
    assert_eq!(
        r[0]["result"]["isError"], false,
        "arguments may be left out"
    );
}

#[test]
fn safety_no_path_outside_the_working_root() {
    let outer = root();
    let inner = outer.path().join("work");
    std::fs::create_dir(&inner).expect("dir");
    let secret = outer.path().join("secret.txt");
    std::fs::write(&secret, "src/a.rs:1  [code]  x\n").expect("write");
    for path in ["../secret.txt", secret.to_str().expect("utf-8")] {
        let (text, err) = call(&inner, "audit_findings", &json!({"path": path}));
        assert!(err);
        assert_eq!(text, format!("{path}: outside the working root"));
        let (text, err) = call(&inner, "report", &json!({"stats": path}));
        assert!(err && text.ends_with("outside the working root"), "{text}");
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&secret, inner.join("link.txt")).expect("symlink");
        let (text, err) = call(&inner, "audit_findings", &json!({"path": "link.txt"}));
        assert!(err && text.ends_with("outside the working root"), "{text}");
    }
}

#[test]
fn safety_file_reads_are_capped() {
    let dir = root();
    let big = std::fs::File::create(dir.path().join("big.log")).expect("create");
    big.set_len(MAX_FILE + 1).expect("size");
    let (text, err) = call(dir.path(), "compress", &json!({"path": "big.log"}));
    assert!(err);
    assert_eq!(text, format!("big.log: larger than {MAX_FILE} bytes"));
}

#[test]
fn safety_no_tool_runs_a_command() {
    let dir = root();
    for path in ["x; echo pwned > pwned", "$(touch pwned)", "`touch pwned`"] {
        let (_, err) = call(dir.path(), "compress", &json!({"path": path}));
        assert!(err);
    }
    let left: Vec<_> = std::fs::read_dir(dir.path()).expect("dir").collect();
    assert!(left.is_empty(), "nothing was created: {left:?}");
}

#[test]
fn safety_oversized_messages_are_refused_and_serving_continues() {
    let dir = root();
    let mut input = vec![b'x'; MAX_MESSAGE + 10];
    input.push(b'\n');
    input.extend_from_slice(&vec![b'y'; MAX_MESSAGE]);
    input.push(b'\n');
    input.extend_from_slice(request(5, "ping", &json!({})).as_bytes());
    let mut huge = vec![b'z'; MAX_MESSAGE * 2];
    huge.push(b'\n');
    input.extend_from_slice(&huge);
    input.extend_from_slice(&vec![b'w'; MAX_MESSAGE + 1]);
    let r = session(dir.path(), &input);
    assert_eq!(r.len(), 5);
    assert_eq!(r[0]["error"]["message"], "message too large");
    assert_eq!(r[1]["error"]["message"], "message too large");
    assert_eq!(r[2]["id"], 5);
    assert_eq!(r[3]["error"]["message"], "message too large");
    assert_eq!(r[4]["error"]["message"], "message too large");
}

/// A writer that refuses every write.
struct Closed;

impl Write for Closed {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn main_reports_a_missing_root_or_a_closed_output() {
    let dir = root();
    let mut err = Vec::new();
    let missing = Err(std::io::ErrorKind::NotFound.into());
    assert_eq!(main(missing, Cursor::new(""), Vec::new(), &mut err), 1);
    assert!(String::from_utf8_lossy(&err).starts_with("puritanical-tools mcp: "));

    let ping = request(1, "ping", &json!({}));
    assert_eq!(
        main(Ok(dir.path().into()), Cursor::new(ping), Closed, Vec::new()),
        1
    );
    let mut out = Vec::new();
    assert_eq!(
        main(Ok(dir.path().into()), Cursor::new(""), &mut out, Vec::new()),
        0
    );
    assert_eq!(out, b"");
    Closed.flush().expect("flush");
}

#[test]
fn the_binary_serves_stdio_from_the_current_directory() {
    let dir = root();
    std::fs::write(dir.path().join("a.txt"), "README.md:1  [prose]  x\n").expect("write");
    let mut child = Command::new(env!("CARGO_BIN_EXE_puritanical-tools"))
        .arg("mcp")
        .current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn");
    let call = request(
        1,
        "tools/call",
        &json!({"name": "audit_findings", "arguments": {"path": "a.txt"}}),
    );
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(call.as_bytes())
        .expect("write");
    let out = child.wait_with_output().expect("run");
    assert_eq!(out.status.code(), Some(0));
    let reply: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(
        reply["result"]["content"][0]["text"],
        "a.txt: 0 code and 1 prose finding lines."
    );
}
