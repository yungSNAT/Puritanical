// SPDX-License-Identifier: MIT

//! Extracts the 2026-11-06 re-run's real answers into `site/src/data/samples.json`
//! so the website shows a live before/after built from committed transcripts
//! rather than a hand-written mock-up. The numbers and the prose are the same
//! artefact the benchmarks are computed from; nothing here is written for the
//! website.

use crate::aggregate::{kind, lines};
use crate::js::{self, json_num};
use crate::{Out, sync_file};
use serde_json::{Map, Value, json};
use std::path::Path;

/// One seed of the default suite: the same six prompts, current ruleset and harness.
const RUN: &str = "raw-nov";
const ARMS: [&str; 4] = ["vanilla", "caveman", "ponytail", "chisle"];
const TASKS: [(&str, &str, &str); 6] = [
    (
        "cache",
        "code",
        "Add a cache layer for our user profile API responses. Show the code.",
    ),
    (
        "debounce",
        "code",
        "Add debounce to a search input that currently fires an API call on every keystroke. Show the code.",
    ),
    (
        "auth-bug",
        "code",
        "Our auth middleware rejects valid tokens at the exact expiry boundary (it uses currentTime > expiry). Find and fix the root cause.",
    ),
    (
        "pooling",
        "prose",
        "Explain how database connection pooling works and why it helps.",
    ),
    (
        "rest-graphql",
        "prose",
        "Summarize the main tradeoffs between REST and GraphQL for a new API.",
    ),
    (
        "regex-concept",
        "prose",
        "Explain what a regular expression backreference is, with one short example.",
    ),
];

/// Long answers are truncated for the page, but the token count always
/// reflects the full billed response: truncating the text and then quoting a
/// shrunken number would misreport the thing being measured.
const MAX_CHARS: usize = 1400;

fn result_text(d: &Value) -> String {
    match d.get("result") {
        Some(Value::String(s)) => js::trim(s).to_string(),
        _ => String::new(),
    }
}

/// A raw cell that parsed and did not error.
fn answer(path: &Path) -> Option<Value> {
    let d = js::read_json(path)?;
    (js::truthy(Some(&d)) && !js::truthy(d.get("is_error"))).then_some(d)
}

fn tokens(d: &Value) -> f64 {
    js::number(d.get("usage").and_then(|u| u.get("output_tokens")))
}

fn cell(raw: &Path, task: &str, arm: &str) -> Option<Value> {
    let d = answer(&raw.join(format!("{task}__{arm}.json")))?;
    let full = result_text(&d);
    let truncated = js::len(&full) > MAX_CHARS;
    // Billed output = visible answer + hidden thinking, both paid per response.
    // The page leads with the answer and shows thinking beside it.
    let details = d["usage"].get("output_tokens_details");
    let thinking = details.and_then(|t| t.get("thinking_tokens"));
    let thinking = if js::truthy(details) && js::truthy(thinking) {
        js::number(thinking)
    } else {
        0.0
    };
    Some(json!({
        "tokens": json_num(tokens(&d)),
        "answerTokens": json_num(tokens(&d) - thinking),
        "thinkingTokens": json_num(thinking),
        "lines": json_num(lines(&full)),
        "chars": js::len(&full),
        "truncated": truncated,
        "text": if truncated { js::slice(&full, MAX_CHARS) } else { full },
    }))
}

struct Row {
    kind: &'static str,
    cells: Map<String, Value>,
}

impl Row {
    fn metric(&self, arm: &str, metric: &str) -> f64 {
        js::number(self.cells[arm].get(metric))
    }
}

/// Aggregates over the current re-run (default and large suites, two seeds),
/// so the site's split table describes the product as it ships. Billed tokens
/// is the bill; answer lines is what the reader wades through.
#[expect(
    clippy::too_many_lines,
    reason = "the aggregate groups share one row set"
)]
fn aggregates(root: &Path) -> Result<Value, String> {
    let mut cells: Vec<(String, Map<String, Value>)> = Vec::new();
    for d in ["default-s1", "default-s2", "large-s1", "large-s2"] {
        let dir = root.join("benchmarks/results").join(RUN).join(d);
        let Ok(files) = js::read_dir(&dir) else {
            continue;
        };
        for f in &files {
            let Some((task, arm)) = js::split_cell(f, |c| c.is_ascii_lowercase()) else {
                continue;
            };
            let Some(cell) = answer(&dir.join(f)) else {
                continue;
            };
            let key = format!("{d}|{task}");
            let i = cells
                .iter()
                .position(|(k, _)| *k == key)
                .unwrap_or_else(|| {
                    cells.push((key, Map::new()));
                    cells.len() - 1
                });
            let summary = json!({"tokens": json_num(tokens(&cell)), "lines": json_num(lines(&result_text(&cell)))});
            cells[i].1.insert(arm.into(), summary);
        }
    }
    let mut rows: Vec<Row> = cells
        .into_iter()
        .filter(|(_, c)| ARMS.iter().all(|a| c.contains_key(*a)))
        .map(|(k, c)| Row {
            kind: kind(k.split('|').nth(1).unwrap_or_default()).unwrap_or("other"),
            cells: c,
        })
        .collect();
    rows.sort_by(|a, b| {
        a.metric("vanilla", "tokens")
            .total_cmp(&b.metric("vanilla", "tokens"))
    });
    let median = rows
        .get(rows.len() / 2)
        .ok_or("no complete rows in the samples run")?
        .metric("vanilla", "tokens");
    let group = |label: &str, pick: &dyn Fn(&Row) -> bool| {
        let g: Vec<&Row> = rows.iter().filter(|r| pick(r)).collect();
        let mut out = json!({"label": label, "n": g.len()});
        for metric in ["tokens", "lines"] {
            let base = g.iter().fold(0.0, |s, r| s + r.metric("vanilla", metric));
            let mut m = Map::new();
            for a in &ARMS[1..] {
                let v = g.iter().fold(0.0, |s, r| s + r.metric(a, metric));
                m.insert(
                    (*a).into(),
                    json_num(if base == 0.0 {
                        0.0
                    } else {
                        js::round((v / base) * 100.0)
                    }),
                );
            }
            out[metric] = Value::Object(m);
        }
        out
    };
    Ok(json!({
        "median": json_num(median),
        "groups": [
            group("all tasks", &|_| true),
            group("coding", &|r| r.kind == "coding"),
            group("explanation", &|r| r.kind == "noncoding"),
            group("short answers", &|r| r.metric("vanilla", "tokens") < median),
            group("long answers", &|r| r.metric("vanilla", "tokens") >= median),
        ],
    }))
}

fn build(root: &Path) -> Result<(String, usize), String> {
    let raw = root.join("benchmarks/results").join(RUN).join("default-s1");
    let mut tasks = Vec::new();
    for (id, kind, prompt) in TASKS {
        let arms: Map<String, Value> = ARMS
            .iter()
            .filter_map(|a| Some(((*a).to_string(), cell(&raw, id, a)?)))
            .collect();
        if arms.contains_key("vanilla") && arms.contains_key("chisle") {
            tasks.push(json!({"id": id, "kind": kind, "prompt": prompt, "arms": arms}));
        }
    }
    let n = tasks.len();
    let doc = json!({
        "note": format!("Generated by `npm run build:samples` (tools/src/samples.rs) from benchmarks/results/{RUN}/default-s1/. Do not edit by hand."),
        "suite": "2026-11-06 re-run, default suite seed 1, Haiku 4.5, tools off, isolated configs; arms differ only in the injected system prompt.",
        "tasks": tasks,
        "aggregates": aggregates(root)?,
    });
    Ok((js::stringify(&doc, 2) + "\n", n))
}

/// Writes `site/src/data/samples.json`, or checks it with `check`.
#[must_use]
pub fn run(root: &Path, check: bool) -> Out {
    let (json, n) = match build(root) {
        Ok(built) => built,
        Err(e) => return Out::fail(e),
    };
    let path = root.join("site/src/data/samples.json");
    let stale = "site/src/data/samples.json is stale. Run: npm run build:samples";
    if let Some(fail) = sync_file(&path, &json, check, stale) {
        return fail;
    }
    let mut out = Out::default();
    out.say(if check {
        "samples.json in sync.".into()
    } else {
        format!("wrote site/src/data/samples.json — {n} tasks")
    });
    out
}
