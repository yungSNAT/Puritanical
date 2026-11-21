// SPDX-License-Identifier: MIT

//! The benchmark analysis commands on small hand-made runs.

mod support;

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use support::{cmd, ok, write};

fn cell(dir: &Path, name: &str, v: &Value) {
    write(&dir.join(name), &v.to_string());
}

fn utf8(p: &Path) -> &str {
    p.to_str().expect("UTF-8 path")
}

/// A raw directory with each kind of task, an unknown one and unreadable cells.
fn aggregate_run(raw: &Path) {
    let cells = [
        (
            "cache__vanilla.json",
            json!({"usage": {"output_tokens": 100}, "result": "one  two\nthree"}),
        ),
        (
            "cache__chisle.json",
            json!({"usage": {"output_tokens": 50}, "result": "x"}),
        ),
        (
            "cache__rtk.json",
            json!({"usage": {"output_tokens": 1}, "result": "x"}),
        ),
        (
            "pooling__vanilla.json",
            json!({"usage": {"output_tokens": 200}, "result": "  "}),
        ),
        ("pooling__chisle.json", json!({"result": "abcdefgh"})),
        (
            "mystery__vanilla.json",
            json!({"usage": {"output_tokens": 10}, "result": "q"}),
        ),
        ("err__vanilla.json", json!({"is_error": true})),
    ];
    for (name, v) in &cells {
        cell(raw, name, v);
    }
    write(&raw.join("broken__vanilla.json"), "not json");
    write(&raw.join("notes.txt"), "");
}

#[test]
fn aggregate_prints_the_tables() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let raw = tmp.path().join("raw");
    aggregate_run(&raw);
    let out = cmd(tmp.path(), &[("RAW_DIR", utf8(&raw))], &["aggregate"]);
    let text = ok(&out);
    for line in [
        "| mystery | undefined | 10 | — | — | — |",
        "| cache | coding | 100 | — | — | 50 |",
        "| pooling | noncoding | 200 | — | — | 0 |",
        "| all | 100% | 0% | 0% | 16% |",
        "| noncoding | 0% | 0% | 0% | 0% |",
        "| all | 310t/3L (n=3) | 0t/0L (n=0) | 0t/0L (n=0) | 50t/2L (n=2) |",
    ] {
        assert!(text.contains(line), "missing {line:?} in\n{text}");
    }
    let order: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with("| ") && l.contains(" | 10"))
        .collect();
    assert_eq!(
        order[0], "| mystery | undefined | 10 | — | — | — |",
        "unknown kinds sort first"
    );

    let empty = cmd(tmp.path(), &[], &["aggregate"]);
    assert_eq!(
        (empty.code, empty.stderr.as_str()),
        (1, "no raw results yet — run: bash benchmarks/run-live.sh\n")
    );
}

#[test]
fn aggregate_json_lists_the_cells_and_sums() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let raw = tmp.path().join("raw");
    aggregate_run(&raw);
    let out = cmd(
        tmp.path(),
        &[("RAW_DIR", utf8(&raw))],
        &["aggregate", "--json"],
    );
    let v: Value = serde_json::from_str(ok(&out)).expect("JSON");
    assert_eq!(v["cells"].as_array().expect("cells").len(), 6);
    assert_eq!(
        v["cells"][0],
        json!({"task": "cache", "arm": "chisle", "kind": "coding", "out": 50, "ans": 0, "lines": 1})
    );
    assert_eq!(
        v["sum"]["vanilla"]["all"],
        json!({"out": 310, "ans": 3, "lines": 3, "n": 3})
    );
}

#[test]
fn aggregate_keeps_a_missing_token_count_undefined() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let raw = tmp.path().join("raw");
    cell(
        &raw,
        "cache__vanilla.json",
        &json!({"usage": {"input_tokens": 9}, "result": "x"}),
    );
    cell(
        &raw,
        "cache__chisle.json",
        &json!({"usage": {"output_tokens": 50}, "result": "x"}),
    );
    cell(
        &raw,
        "debounce__chisle.json",
        &json!({"usage": {"output_tokens": null}, "result": "x"}),
    );
    let env = [("RAW_DIR", utf8(&raw))];
    let out = cmd(tmp.path(), &env, &["aggregate"]);
    let text = ok(&out);
    for line in [
        "| cache | coding | undefined | — | — | 50 |",
        "| debounce | coding | — | — | — | null |",
        // A NaN baseline is falsy, so every arm reads 0%.
        "| all | 0% | 0% | 0% | 0% |",
        "| all | NaNt/1L (n=1) | 0t/0L (n=0) | 0t/0L (n=0) | 50t/2L (n=2) |",
    ] {
        assert!(text.contains(line), "missing {line:?} in\n{text}");
    }

    let v: Value =
        serde_json::from_str(ok(&cmd(tmp.path(), &env, &["aggregate", "--json"]))).expect("JSON");
    assert_eq!(
        v["cells"][1],
        json!({"task": "cache", "arm": "vanilla", "kind": "coding", "ans": 0, "lines": 1}),
        "JSON.stringify drops the undefined field"
    );
    assert_eq!(v["cells"][2]["out"], Value::Null);
    assert_eq!(v["sum"]["vanilla"]["all"]["out"], Value::Null);
}

#[test]
fn aggregate_reads_a_non_string_result_as_empty() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let raw = tmp.path().join("raw");
    cell(
        &raw,
        "cache__vanilla.json",
        &json!({"usage": {"output_tokens": 10}, "result": 5}),
    );
    cell(
        &raw,
        "debounce__vanilla.json",
        &json!({"usage": {"output_tokens": 20}, "result": "a"}),
    );
    let out = cmd(tmp.path(), &[("RAW_DIR", utf8(&raw))], &["aggregate"]);
    let text = ok(&out);
    let cache = text.find("| cache | coding | 10 |").expect("cache row");
    let debounce = text
        .find("| debounce | coding | 20 |")
        .expect("debounce row");
    assert!(cache < debounce, "same kind: by task name");
}

/// A pooled run: a fix that misses, a cache answer written as a tool call,
/// a clarifying question and a task only vanilla answered.
fn pooled_run(d: &Path) {
    let cells = [
        (
            "auth-bug__vanilla.json",
            json!({"result": "use >= here", "usage": {"output_tokens": 100}, "modelUsage": {"m": {"thinkingTokens": 10}}}),
        ),
        (
            "auth-bug__chisle.json",
            json!({"result": "no fix", "usage": {"output_tokens": 50}}),
        ),
        (
            "cache__vanilla.json",
            json!({"result": "```js\nx\n```", "usage": {"output_tokens": 150}}),
        ),
        (
            "cache__chisle.json",
            json!({"result": "<parameter name=\"content\">x", "usage": {"output_tokens": 60}}),
        ),
        (
            "pooling__vanilla.json",
            json!({"result": "x".repeat(200), "usage": {"output_tokens": 200}}),
        ),
        (
            "pooling__chisle.json",
            json!({"result": format!("What stack? {}", "y".repeat(200)), "usage": {"output_tokens": 20}}),
        ),
        (
            "lonely__vanilla.json",
            json!({"result": "x", "usage": {"output_tokens": 1}}),
        ),
    ];
    for (name, v) in &cells {
        cell(d, name, v);
    }
    write(&d.join("readme.txt"), "");
}

#[test]
fn pooled_ratios_and_non_answers() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let d = tmp.path().join("s1");
    pooled_run(&d);
    let env = [("ARMS", "vanilla,chisle")];
    let out = cmd(tmp.path(), &env, &["pooled", utf8(&d)]);
    let text = ok(&out);
    assert!(text.starts_with("\nall cells (n=3)\narm        answer len        CI  lines  billed        CI  think  backfires\n"), "{text}");
    let row = text.lines().nth(3).expect("chisle row");
    assert!(
        row.starts_with("chisle") && row.contains(" 29% ") && row.ends_with("0/3"),
        "{row}"
    );
    assert!(text.contains("\nevery arm delivered (n=1)\n"), "{text}");
    assert!(
        text.ends_with("\nnon-answers: vanilla 0, chisle 2\n"),
        "{text}"
    );
    let again = cmd(tmp.path(), &env, &["pooled", utf8(&d)]);
    assert_eq!(again.stdout, out.stdout, "the bootstrap is seeded");

    let missing = cmd(tmp.path(), &[], &["pooled", "no-such-dir"]);
    assert_eq!(missing.code, 1);
    assert!(
        missing.stderr.starts_with("no-such-dir: "),
        "{}",
        missing.stderr
    );
}

#[test]
fn pooled_counts_a_file_written_by_a_tool_call_as_delivered() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let d = tmp.path().join("p");
    cell(
        &d,
        "debounce__vanilla.json",
        &json!({"result": "x", "usage": {"output_tokens": 10}}),
    );
    cell(
        &d,
        "debounce__chisle.json",
        &json!({"result": "<parameter name=\"contents\">x", "usage": {"output_tokens": 5}}),
    );
    let out = cmd(
        tmp.path(),
        &[("ARMS", "vanilla,chisle")],
        &["pooled", utf8(&d)],
    );
    assert!(
        ok(&out).ends_with("non-answers: vanilla 1, chisle 0\n"),
        "{}",
        out.stdout
    );
}

const LIVE: &str = "#!/usr/bin/env bash\r\nTASKS=$(cat <<'EOF'\r\ncache\tcoding\tAdd a cache.\r\npooling\tnoncoding\tExplain pooling.\r\nEOF\r\n)\r\nTASKS_LARGE=$(cat <<'EOF'\r\nbig\tcoding\r\nEOF\r\n)\r\n";

/// `<root>/run-x`: one run with answered, unanswered and unprompted cells.
fn answers_run(root: &Path) -> PathBuf {
    write(&root.join("benchmarks/run-live.sh"), LIVE);
    let run = root.join("run-x");
    let cells = [
        (
            "cache__vanilla.json",
            json!({"result": " has ~~~~ tildes ", "usage": {"output_tokens": 30}, "modelUsage": {"m": {"thinkingTokens": 5}}}),
        ),
        (
            "cache__chisle.json",
            json!({"result": "short", "usage": {"output_tokens": 3}}),
        ),
        ("pooling__chisle.json", json!({"result": "no usage"})),
        (
            "big__vanilla.json",
            json!({"result": "b", "usage": {"output_tokens": 1}}),
        ),
        (
            "unknown__vanilla.json",
            json!({"result": "u", "usage": {"output_tokens": 1}}),
        ),
    ];
    for (name, v) in &cells {
        cell(&run.join("s1"), name, v);
    }
    write(&run.join("top.json"), "{}");
    run
}

#[test]
fn answers_writes_every_prompt_and_answer() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let run = answers_run(tmp.path());
    let out = cmd(tmp.path(), &[], &["answers", utf8(&run)]);
    assert!(
        ok(&out).ends_with(": 1 runs, arms vanilla, chisle\n"),
        "{}",
        out.stdout
    );
    let md = std::fs::read_to_string(run.join("ANSWERS.md")).expect("ANSWERS.md");
    let want = "# Every prompt and answer: `run-x`\n\n\
        Generated by `puritanical-tools answers` from the raw cells in this directory. Each answer is verbatim; \"answer\" tokens are billed output minus hidden reasoning. Arms: vanilla, chisle.\n\n\
        ## s1 / big\n\n> undefined\n\n### vanilla: 1 answer tokens, 1 billed, 1 lines\n\n~~~\nb\n~~~\n\n### chisle\n\n_No answer recorded._\n\n\
        ## s1 / cache\n\n> Add a cache.\n\n### vanilla: 25 answer tokens, 30 billed, 1 lines\n\n~~~~~\nhas ~~~~ tildes\n~~~~~\n\n\
        ### chisle: 3 answer tokens, 3 billed, 1 lines\n\n~~~\nshort\n~~~\n\n\
        ## s1 / pooling\n\n> Explain pooling.\n\n### vanilla\n\n_No answer recorded._\n\n### chisle\n\n_No answer recorded._\n";
    assert_eq!(md, want);

    ok(&cmd(tmp.path(), &[], &["answers", utf8(&run), "chisle"]));
    let md = std::fs::read_to_string(run.join("ANSWERS.md")).expect("ANSWERS.md");
    assert!(
        md.contains("Arms: chisle.") && !md.contains("### vanilla"),
        "{md}"
    );
}

#[test]
fn answers_reports_what_it_cannot_read() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path();
    let run = answers_run(root);
    let run_dir = utf8(&run);
    let fail = |args: &[&str], root: &Path| {
        let out = cmd(root, &[], args);
        assert_eq!(out.code, 1, "{args:?}");
        out.stderr
    };
    assert_eq!(
        fail(&["answers"], root),
        "usage: puritanical-tools answers <run-dir> [arm ...]\n"
    );
    assert!(fail(&["answers", "no-such-run"], root).starts_with("no-such-run: "));
    let bare = tempfile::tempdir().expect("temp dir");
    assert!(fail(&["answers", run_dir], bare.path()).contains("run-live.sh"));
    write(&bare.path().join("benchmarks/run-live.sh"), "#!/bin/sh\n");
    assert_eq!(
        fail(&["answers", run_dir], bare.path()),
        "run-live.sh has no TASKS heredocs\n"
    );
}

#[test]
fn answers_ignores_a_cell_with_no_arm() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path();
    write(&root.join("benchmarks/run-live.sh"), LIVE);
    cell(&root.join("run/s1"), "x__.json", &json!({}));
    let out = cmd(root, &[], &["answers", utf8(&root.join("run"))]);
    assert!(ok(&out).ends_with("arms vanilla\n"), "{}", out.stdout);
}

#[test]
fn normalize_pi_builds_one_cell() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let events = tmp.path().join("events.jsonl");
    let lines = [
        json!({"type": "session", "version": 3}).to_string(),
        json!({"type": "message_end", "message": {"role": "assistant", "content": [{"type": "text", "text": "answer"}, {"type": "tool", "text": "x"}, {"type": "text", "text": 5}, {"type": "text"}], "usage": {"output": 42}, "stopReason": "stop"}}).to_string(),
        "not json".into(),
        json!({"type": "message_end", "message": {"role": "user", "content": "ignored"}}).to_string(),
        json!({"type": "message_end", "message": null}).to_string(),
        json!({"type": "message_end", "message": {"role": "assistant", "content": {"a": 1}, "usage": {"output": "9"}}}).to_string(),
        String::new(),
        json!({"type": "message_end", "message": {"role": "assistant", "content": "final", "usage": {"output": 8}}}).to_string(),
    ];
    write(&events, &lines.join("\n"));
    let out = cmd(tmp.path(), &[], &["normalize-pi", utf8(&events)]);
    assert_eq!(
        ok(&out),
        "{\n  \"harness\": \"pi\",\n  \"is_error\": false,\n  \"result\": \"final\",\n  \"usage\": {\n    \"output_tokens\": 50\n  }\n}\n"
    );
}

#[test]
fn normalize_pi_marks_errors_and_usage() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let events = tmp.path().join("events.jsonl");
    let path = utf8(&events);
    let normalised = || -> Value {
        serde_json::from_str(ok(&cmd(tmp.path(), &[], &["normalize-pi", path]))).expect("JSON")
    };
    write(&events, &json!({"type": "message_end", "message": {"role": "assistant", "content": [], "stopReason": "error"}}).to_string());
    assert_eq!(
        normalised(),
        json!({"harness": "pi", "is_error": true, "result": "", "usage": {"output_tokens": 0}})
    );
    write(&events, &json!({"type": "message_end", "message": {"role": "assistant", "content": null, "stopReason": "stop"}}).to_string());
    assert_eq!(
        normalised()["is_error"],
        json!(true),
        "an empty answer is an error"
    );

    let bs = char::from(0x5C_u8);
    let event = |content: &str| {
        format!(
            r#"{{"type":"message_end","message":{{"role":"assistant","content":"{content}","usage":{{"output":3}}}}}}"#
        )
    };
    write(&events, &event(&format!("a{bs}ud800")));
    let lone = ok(&cmd(tmp.path(), &[], &["normalize-pi", path])).to_owned();
    assert!(
        lone.contains(&format!(r#""result": "a{bs}ud800""#))
            && lone.contains(r#""output_tokens": 3"#),
        "an escaped lone surrogate is kept, as JSON.parse keeps it: {lone}"
    );
    for reserved in [
        char::from_u32(0x10_F800).expect("plane 16").to_string(),
        format!("{bs}udbfe{bs}udc00"),
    ] {
        write(&events, &event(&reserved));
        assert_eq!(
            cmd(tmp.path(), &[], &["normalize-pi", path]).code,
            1,
            "a stand-in code point is refused, raw or escaped"
        );
    }

    let usage = cmd(tmp.path(), &[], &["normalize-pi"]);
    assert_eq!(
        (usage.code, usage.stderr.as_str()),
        (2, "usage: puritanical-tools normalize-pi <events.jsonl>\n")
    );
    assert_eq!(
        cmd(tmp.path(), &[], &["normalize-pi", "missing.jsonl"]).code,
        1
    );
}

fn scored(rows: &[Value]) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("temp dir");
    for (i, r) in rows.iter().enumerate() {
        cell(tmp.path(), &format!("r{i:02}.json"), r);
    }
    write(&tmp.path().join("bad.json"), "{");
    write(&tmp.path().join("notes.txt"), "");
    tmp
}

/// Two fixtures: vanilla passes, chisle fails, the hook arm compresses, and one cell errored.
fn agentic_run() -> tempfile::TempDir {
    scored(&[
        json!({"fixture": "f1", "arm": "vanilla", "seed": 1, "pass": true, "files_created": ["a"], "loc_delta": 2, "deps_added": [], "out_tokens": 100, "input_tokens": 1000, "turns": 3, "cost_usd": 0.01}),
        json!({"fixture": "f1", "arm": "chisle", "seed": 1, "pass": false, "test_output": "FAIL a|b\nmore", "files_created": "ab"}),
        json!({"fixture": "f1", "arm": "chisle-hook", "seed": 1, "pass": true, "compressed_chars": 500, "compress_events": 2, "files_created": 5}),
        json!({"fixture": "f2", "arm": "vanilla", "seed": 1, "pass": true}),
        json!({"error": "boom", "arm": "vanilla"}),
    ])
}

/// The report for `arms` (the default pair when `None`).
fn agentic(tmp: &tempfile::TempDir, arms: Option<&str>) -> String {
    let raw = utf8(tmp.path());
    let mut env = vec![("RAW_DIR", raw)];
    env.extend(arms.map(|a| ("ARMS", a)));
    ok(&cmd(tmp.path(), &env, &["agentic-score"])).to_string()
}

#[test]
fn agentic_score_reports_every_section() {
    let tmp = agentic_run();
    let text = agentic(&tmp, None);
    for line in [
        "| fixture | vanilla | chisle | Fisher p |",
        "|---------|--------:|-------:|---------:|",
        "| f1 | 1/1 | 0/1 | 1.000 |",
        "| vanilla | 1 | 1 | 100.0% | 1.00 | 2.0 | 0.00 | 100 | 1000 | 3.00 | 0.0100 |",
        "| chisle | 1 | 0 | 0.0% | 2.00 | 0.0 | 0.00 | 0 | 0 | 0.00 | 0.0000 |",
        "chisle  0/1, 95% CI 0.0%–79.3%",
        "Fisher exact, two-sided: p = 1.000 — not separable from noise at this cell count.",
        "| chisle | f1 | 1 | FAIL a\\|b |",
        "_1 cell(s) excluded: no counterpart in the other arm, so the seed sets stay matched._",
        "_1 cell(s) errored and are excluded._",
    ] {
        assert!(text.contains(line), "missing {line:?} in\n{text}");
    }
    let text = agentic(&tmp, Some("vanilla, chisle ,chisle-hook,"));
    for line in [
        "| fixture | vanilla | chisle | chisle-hook | p chisle | p chisle-hook |",
        "| chisle-hook | 1 | 1 | 100.0% | NaN | 0.0 |",
        "chisle-hook: compressor elided 500 chars/run across 2.00 outputs/run.",
        "Fisher exact (chisle vs vanilla), two-sided",
        "no counterpart in every arm",
    ] {
        assert!(text.contains(line), "missing {line:?} in\n{text}");
    }
}

#[test]
fn agentic_score_without_vanilla_or_failures() {
    let tmp = agentic_run();
    let text = agentic(&tmp, Some("chisle-hook,chisle"));
    assert!(
        text.contains("| f1 | 1/1 | 0/1 | — | — |") && !text.contains("## Significance"),
        "{text}"
    );
    let text = agentic(&tmp, Some("vanilla,ghost"));
    assert!(text.contains("## Every failed cell\n\n_None._"), "{text}");
}

#[test]
fn agentic_score_json_and_empty_runs() {
    let tmp = agentic_run();
    let raw = utf8(tmp.path());
    let out = cmd(
        tmp.path(),
        &[("RAW_DIR", raw)],
        &["agentic-score", "--json"],
    );
    let rows: Value = serde_json::from_str(ok(&out)).expect("JSON");
    assert_eq!(rows.as_array().expect("rows").len(), 5);
    assert!(
        out.stdout.starts_with("[\n {\n  \"fixture\": \"f1\""),
        "indent of 1"
    );

    let empty = tempfile::tempdir().expect("temp dir");
    let none = cmd(
        empty.path(),
        &[("RAW_DIR", utf8(empty.path()))],
        &["agentic-score"],
    );
    assert_eq!(
        (none.code, none.stderr.as_str()),
        (1, "no results — run: node benchmarks/agentic/run.js\n")
    );
}

#[test]
fn agentic_score_flags_a_real_difference() {
    let mut rows = Vec::new();
    for seed in 0..10 {
        rows.push(json!({"fixture": "f", "arm": "vanilla", "seed": seed, "pass": false, "test_output": "x"}));
        rows.push(json!({"fixture": "f", "arm": "chisle", "seed": seed, "pass": true}));
    }
    let tmp = scored(&rows);
    let text = agentic(&tmp, None);
    assert!(
        text.contains("Fisher exact, two-sided: p = <0.001\n"),
        "{text}"
    );
    assert!(text.contains("| f | 0/10 | 10/10 | <0.001 |"), "{text}");
}

#[test]
fn agentic_score_prints_a_non_string_test_output_as_blank() {
    let rows = scored(&[
        json!({"fixture": "f", "arm": "vanilla", "seed": 1, "pass": true}),
        json!({"fixture": "f", "arm": "chisle", "seed": 1, "pass": false, "test_output": 5}),
    ]);
    let text = agentic(&rows, None);
    assert!(text.contains("| chisle | f | 1 |  |"), "{text}");
}
