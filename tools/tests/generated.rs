// SPDX-License-Identifier: Apache-2.0

//! The committed generated files are the oracle: `--check` passes on the
//! repository, a fresh write reproduces every byte, and any drift fails.

mod support;

use puritanical_tools::{readme_art, rules};
use std::fs;
use std::path::Path;
use support::{cmd, copy_dir, ok, repo, write};

const COMMANDS: [&str; 3] = ["rules", "chart", "readme-art"];

/// Every generated file, relative to the repository root.
fn outputs() -> Vec<String> {
    let mut all: Vec<String> = rules::TARGETS
        .iter()
        .map(|(rel, _)| (*rel).to_string())
        .collect();
    for name in [
        "benchmark",
        "benchmark-light",
        "benchmark-terminal",
        "benchmark-terminal-light",
    ] {
        all.push(format!("assets/{name}.svg"));
    }
    all.extend(
        readme_art::files()
            .into_iter()
            .map(|(rel, _)| format!("assets/readme/{rel}")),
    );
    all
}

/// A scratch root holding only the benchmark results the generators read.
fn inputs() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("temp dir");
    for run in ["raw-314-sidebyside", "raw-314-commands"] {
        let rel = Path::new("benchmarks/results").join(run);
        copy_dir(&repo().join(&rel), &tmp.path().join(rel));
    }
    tmp
}

#[test]
fn committed_outputs_are_in_sync() {
    for c in COMMANDS {
        let out = cmd(&repo(), &[], &[c, "--check"]);
        assert!(ok(&out).ends_with("in sync.\n"), "{c}: {}", out.stdout);
        assert_eq!(out.stderr, "", "{c}");
    }
}

#[test]
fn a_fresh_write_reproduces_every_committed_byte() {
    let tmp = inputs();
    for c in COMMANDS {
        assert!(ok(&cmd(tmp.path(), &[], &[c])).starts_with("wrote "), "{c}");
    }
    let files = outputs();
    assert_eq!(files.len(), 31);
    for rel in &files {
        let want = fs::read(repo().join(rel)).expect("committed file");
        assert!(
            fs::read(tmp.path().join(rel)).expect("written file") == want,
            "{rel} differs"
        );
    }
    for c in COMMANDS {
        ok(&cmd(tmp.path(), &[], &[c, "--check"]));
    }
}

#[test]
fn drift_fails_the_check() {
    let tmp = inputs();
    for c in COMMANDS {
        ok(&cmd(tmp.path(), &[], &[c]));
    }
    let root = tmp.path();
    fs::remove_file(root.join(".kiro/steering/puritanical.md")).expect("remove");
    fs::write(root.join(".clinerules/puritanical.md"), "edited by hand\n").expect("edit");
    let out = cmd(root, &[], &["rules", "--check"]);
    assert_eq!(out.code, 1);
    assert_eq!(
        out.stderr,
        "OUT OF SYNC: .clinerules/puritanical.md\nOUT OF SYNC: .kiro/steering/puritanical.md\n\n2 file(s) out of sync. Run: npm run build:rules\n"
    );

    let mutate = |rel: &str| {
        let path = root.join(rel);
        let mut bytes = fs::read(&path).expect("read");
        bytes.push(b' ');
        fs::write(path, bytes).expect("write");
    };
    mutate("assets/benchmark-terminal-light.svg");
    let out = cmd(root, &[], &["chart", "--check"]);
    assert_eq!(
        (out.code, out.stderr.as_str()),
        (
            1,
            "assets/benchmark-terminal-light.svg is stale. Run: npm run build:chart\n"
        )
    );

    mutate("assets/readme/race-light.svg");
    fs::remove_file(root.join("assets/readme/line.svg")).expect("remove");
    let out = cmd(root, &[], &["readme-art", "--check"]);
    assert_eq!(out.code, 1);
    assert_eq!(out.stdout, "");
    assert_eq!(
        out.stderr,
        "assets/readme/line.svg is stale. Run: npm run build:readme-art\nassets/readme/race-light.svg is stale. Run: npm run build:readme-art\n"
    );
}

#[test]
fn chart_json_prints_both_charts_figures() {
    let out = cmd(&repo(), &[], &["chart", "--json"]);
    let stats: serde_json::Value = serde_json::from_str(ok(&out)).expect("JSON");
    let keys = |i: usize| {
        stats[i]
            .as_object()
            .expect("object")
            .keys()
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(
        keys(0),
        ["n", "caveman", "ponytail", "chisle-3.13", "chisle"]
    );
    assert_eq!(
        keys(1),
        ["n", "held", "caveman", "ponytail", "rtk", "chisle"]
    );
}

#[test]
fn missing_inputs_fail_cleanly() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path();
    let out = cmd(root, &[], &["chart"]);
    assert_eq!(out.code, 1);
    assert!(out.stderr.starts_with("cannot read "), "{}", out.stderr);
    write(
        &root.join("benchmarks/results/raw-314-commands/commands.json"),
        "{}",
    );
    assert_eq!(
        cmd(root, &[], &["chart"]).stderr,
        "commands.json is not an array\n"
    );
    write(
        &root.join("benchmarks/results/raw-314-commands/commands.json"),
        "[]",
    );
    // No side-by-side cells: the bars fall back to 0%.
    let out = cmd(root, &[], &["chart", "--json"]);
    assert!(ok(&out).contains("\"chisle-3.13\": 0"), "{}", out.stdout);
}

#[test]
fn unwritable_targets_report_the_path() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path();
    // A file where a directory must go makes every write fail.
    for blocker in [".cursor", "assets"] {
        write(&root.join(blocker), "");
    }
    let bad = |args: &[&str]| {
        let out = cmd(root, &[], args);
        assert_eq!(out.code, 1, "{args:?}");
        out.stderr
    };
    assert!(bad(&["rules"]).contains("puritanical.mdc"));
    assert!(bad(&["readme-art"]).contains("line.svg"));
    let tmp2 = inputs();
    write(&tmp2.path().join("assets"), "");
    assert!(
        cmd(tmp2.path(), &[], &["chart"])
            .stderr
            .contains("benchmark.svg")
    );
}

#[test]
fn incomplete_cells_are_left_out() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path();
    write(
        &root.join("benchmarks/results/raw-314-commands/commands.json"),
        "[]",
    );
    let side = root.join("benchmarks/results/raw-314-sidebyside");
    write(&side.join("ANSWERS.md"), "");
    write(&side.join("s1/notes.txt"), "");
    write(&side.join("s1/x__vanilla.json"), "{");
    write(
        &side.join("s1/t__vanilla.json"),
        r#"{"usage": {"output_tokens": 10}}"#,
    );
    write(
        &side.join("s1/u__vanilla.json"),
        r#"{"result": "no usage"}"#,
    );
    let out = cmd(root, &[], &["chart", "--json"]);
    assert!(
        ok(&out).starts_with("[\n  {\n    \"n\": 0,"),
        "{}",
        out.stdout
    );
}
