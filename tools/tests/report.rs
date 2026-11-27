// SPDX-License-Identifier: Apache-2.0

//! `report`: measured figures only, labelled bytes, lines or characters.

mod support;

use puritanical_tools::report::compressed;
use std::fmt::Write as _;
use std::fs;
use support::{cmd, ok, repo, write};

fn report(args: &[&str]) -> puritanical_tools::Out {
    cmd(&repo(), &[], &[&["report"], args].concat())
}

/// `n` with thousands separators.
fn sep(n: usize) -> String {
    let d = n.to_string();
    let mut out = String::new();
    for (i, c) in d.chars().enumerate() {
        if i > 0 && (d.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A test log the hook would elide: 300 lines, about 15,000 bytes.
fn long_log() -> String {
    let mut log = String::new();
    for i in 0..300 {
        writeln!(log, "step {i:03}: compiling module number {i} of the build").expect("write");
    }
    log
}

#[test]
fn usage_errors_exit_2() {
    for args in [&[][..], &["--bogus"], &["--stats"], &["--svg", "card.svg"]] {
        let out = report(args);
        assert_eq!(out.code, 2, "{args:?}");
        assert!(out.stderr.starts_with("usage: puritanical-tools report"));
    }
}

#[test]
fn an_unreadable_input_fails_with_its_path() {
    let dir = tempfile::tempdir().expect("temp dir");
    let missing = dir.path().join("missing.log");
    let missing = missing.to_str().expect("utf-8");
    for args in [
        vec![missing],
        vec!["--audit-before", missing],
        vec!["--stats", missing],
    ] {
        let out = report(&args);
        assert_eq!(out.code, 1);
        assert!(out.stderr.starts_with(missing), "{}", out.stderr);
    }
}

#[test]
fn tool_output_rows_give_bytes_before_and_after_the_hook() {
    let dir = tempfile::tempdir().expect("temp dir");
    let big = dir.path().join("big.log");
    let small = dir.path().join("small.txt");
    let empty = dir.path().join("empty.txt");
    let log = long_log();
    write(&big, &log);
    write(&small, "0123456789");
    write(&empty, "");
    let binary = dir.path().join("binary.bin");
    fs::write(&binary, [0xff, 0xfe, b'x']).expect("write");
    let after = compressed(&log).len();
    assert!(after < log.len() / 2, "{after} of {}", log.len());

    let paths = [&big, &small, &empty, &binary].map(|p| p.to_str().expect("utf-8").to_string());
    let out = report(&paths.iter().map(String::as_str).collect::<Vec<_>>());
    let md = ok(&out);
    assert!(md.starts_with("# Puritanical report\n\n## Tool output\n"));
    assert!(md.contains("Bytes, not tokens."));
    let big_name = &paths[0];
    let pct = format!(
        "{:.1}%",
        (log.len() - after) as f64 * 100.0 / log.len() as f64
    );
    let row = format!(
        "| `{big_name}` | {} | {} | {pct} |",
        sep(log.len()),
        sep(after)
    );
    assert!(md.contains(&row), "{md}");
    assert!(
        md.contains(&format!("| `{}` | 10 | 10 | 0.0% |", paths[1])),
        "{md}"
    );
    assert!(
        md.contains(&format!("| `{}` | 0 | 0 | 0.0% |", paths[2])),
        "{md}"
    );
    assert!(
        md.contains(&format!(
            "| All 4 | {} | {} |",
            sep(log.len() + 13),
            sep(after + 13)
        )),
        "{md}"
    );
    assert!(
        md.contains(&format!("| `{}` | 3 | 3 | 0.0% |", paths[3])),
        "{md}"
    );
    assert!(!md.contains('~'), "no estimates: {md}");
    assert!(!md.contains("## Audit"));
}

#[test]
fn a_small_win_leaves_the_output_unchanged() {
    let text = "x".repeat(1500);
    assert_eq!(compressed(&text), text);
}

#[test]
fn audit_findings_are_counted_before_and_after_with_a_card() {
    let dir = tempfile::tempdir().expect("temp dir");
    let before = dir.path().join("before.txt");
    let after = dir.path().join("after.txt");
    let card = dir.path().join("out").join("card.svg");
    write(
        &before,
        "src/a.rs:3  [code]  wrapper → inline it. (~4 lines)\n\
         README.md:9  [prose]  recap → cut. (~2 lines)\n\
         lib/b.js:1  [code]  dead branch → delete. (~3 lines)\n\
         nocolon  [code]  not a finding\n\
         src/c.rs:2  [other]  not a kind\n\
         \n\
         3 findings: 2 code, 1 prose. Est. removable: ~7 lines code, ~2 lines prose.\n",
    );
    write(&after, "src/a.rs:3  [code]  wrapper → inline it.\n");
    let out = report(&[
        "--audit-before",
        before.to_str().expect("utf-8"),
        "--audit-after",
        after.to_str().expect("utf-8"),
        "--svg",
        card.to_str().expect("utf-8"),
    ]);
    let md = ok(&out);
    assert!(md.contains(
        "| Kind | Before | After |\n|---|---:|---:|\n| code | 2 | 1 |\n| prose | 1 | 0 |\n| all | 3 | 1 |\n"
    ), "{md}");
    assert!(!md.contains("## Tool output"));

    let svg = fs::read_to_string(&card).expect("card written");
    assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(svg.contains(">Audit findings</text>"));
    assert!(svg.contains("Before: 3 lines") && svg.contains("After: 1 lines"));
    for colour in [
        "#000000", "#9B7735", "#E5E4E2", "#FFFFF0", "#66023C", "#990024",
    ] {
        assert!(svg.contains(colour), "{colour}");
    }
    assert!(svg.contains("width=\"380\"") && svg.contains("width=\"127\""));
}

#[test]
fn one_audit_gives_one_column_and_no_card_bars() {
    let dir = tempfile::tempdir().expect("temp dir");
    let after = dir.path().join("after.txt");
    let card = dir.path().join("card.svg");
    write(&after, "README.md:9  [prose]  recap → cut.\n");
    let out = report(&[
        "--audit-after",
        after.to_str().expect("utf-8"),
        "--svg",
        card.to_str().expect("utf-8"),
    ]);
    assert!(
        ok(&out)
            .contains("| Kind | After |\n|---|---:|\n| code | 0 |\n| prose | 1 |\n| all | 1 |\n")
    );
    let svg = fs::read_to_string(&card).expect("card written");
    assert!(!svg.contains("Audit findings"));
}

#[test]
fn the_ledger_line_reports_characters_and_the_card_shows_it() {
    let dir = tempfile::tempdir().expect("temp dir");
    let stats = dir.path().join("stats.json");
    let log = dir.path().join("run.log");
    let card = dir.path().join("card.svg");
    write(&stats, r#"{"savedChars":1234567,"events":42}"#);
    write(&log, &long_log());
    let stats = stats.to_str().expect("utf-8");
    let out = report(&[
        "--stats",
        stats,
        "--svg",
        card.to_str().expect("utf-8"),
        log.to_str().expect("utf-8"),
    ]);
    let md = ok(&out);
    assert!(md.contains(&format!(
        "`{stats}` records 1,234,567 characters (UTF-16 code units, as the hook counts them) removed from 42 tool outputs."
    )));
    let svg = fs::read_to_string(&card).expect("card written");
    assert!(svg.contains("Tool output: "));
    assert!(svg.contains(&format!("Before: {} bytes", sep(long_log().len()))));
    assert!(svg.contains("Compressor ledger: 1,234,567 characters removed from 42 outputs"));
}

#[test]
fn a_bad_ledger_or_card_path_fails() {
    let dir = tempfile::tempdir().expect("temp dir");
    let stats = dir.path().join("stats.json");
    for text in [
        "not json",
        r#"{"savedChars":-1,"events":2}"#,
        r#"{"events":2}"#,
    ] {
        write(&stats, text);
        let out = report(&["--stats", stats.to_str().expect("utf-8")]);
        assert_eq!(out.code, 1);
        assert!(
            out.stderr.contains("not a compressor ledger"),
            "{}",
            out.stderr
        );
    }

    write(&stats, r#"{"savedChars":1,"events":1}"#);
    let blocked = dir.path().join("stats.json").join("card.svg");
    let out = report(&[
        "--stats",
        stats.to_str().expect("utf-8"),
        "--svg",
        blocked.to_str().expect("utf-8"),
    ]);
    assert_eq!(out.code, 1);
    assert_eq!(out.stdout, "");
}
