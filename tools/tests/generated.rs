// SPDX-License-Identifier: Apache-2.0

//! The committed generated files are the oracle: `--check` passes on the
//! repository, a fresh write reproduces every byte, and any drift fails.

mod support;

use puritanical_tools::{readme_art, rules};
use std::fs;
use support::{cmd, ok, repo, write};

const COMMANDS: [&str; 2] = ["rules", "readme-art"];

/// Every generated file, relative to the repository root.
fn outputs() -> Vec<String> {
    let mut all: Vec<String> = rules::TARGETS
        .iter()
        .map(|(rel, _)| (*rel).to_string())
        .collect();
    all.extend(
        readme_art::files()
            .into_iter()
            .map(|(rel, _)| format!("assets/readme/{rel}")),
    );
    all
}

/// A scratch root holding only the clone marker: the generators read no
/// repository files.
fn inputs() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("temp dir");
    write(
        &tmp.path().join("package.json"),
        r#"{"name":"puritanical"}"#,
    );
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
    assert_eq!(files.len(), 23);
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
    mutate("assets/readme/trim-light.svg");
    fs::remove_file(root.join("assets/readme/line.svg")).expect("remove");
    let out = cmd(root, &[], &["readme-art", "--check"]);
    assert_eq!(out.code, 1);
    assert_eq!(out.stdout, "");
    assert_eq!(
        out.stderr,
        "assets/readme/line.svg is stale. Run: npm run build:readme-art\nassets/readme/trim-light.svg is stale. Run: npm run build:readme-art\n"
    );
}

#[test]
fn unwritable_targets_report_the_path() {
    let tmp = inputs();
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
}
