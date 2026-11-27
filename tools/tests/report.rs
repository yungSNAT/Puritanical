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

    let paths = [&big, &small, &empty].map(|p| p.to_str().expect("utf-8").to_string());
    let out = report(&paths.iter().map(String::as_str).collect::<Vec<_>>());
    let md = ok(&out);
    assert!(md.starts_with("# Puritanical report\n\n## Tool output\n"));
    // The only mention of tokens is the label saying there are none.
    assert_eq!(md.matches("token").count(), 1, "{md}");
    assert!(md.contains("Bytes, not tokens."));
    // The hook's `transform` takes this log from 15,190 to 5,044 units.
    for row in [
        format!("| `{}` | 15,190 | 5,044 | 66.7% |", paths[0]),
        format!("| `{}` | 10 | 10 | 0.0% |", paths[1]),
        format!("| `{}` | 0 | 0 | n/a |", paths[2]),
        "| All 3 | 15,200 | 5,054 | 66.7% |".to_string(),
    ] {
        assert!(md.contains(&row), "{row} in {md}");
    }
    assert!(!md.contains("## Audit"));
}

/// A line of 1,100 `x`, then `n` short lines with `spaces` trailing spaces,
/// which scrub removes.
fn padded(n: usize, spaces: usize) -> String {
    let mut s = "x".repeat(1100) + "\n";
    for i in 0..n {
        writeln!(s, "y{i}{}", " ".repeat(spaces)).expect("write");
    }
    s
}

/// Sizes the hook's own `transform(text, THRESHOLDS)` gives for the same
/// text; `tests/test_report_parity.js` pins the same figures on the hook side.
#[test]
fn sizes_match_the_hooks_transform() {
    assert_eq!(compressed(&long_log()).len(), 5044);
    // 63 units removed is under the hook's minimum win, so nothing changes.
    let text = padded(21, 3);
    assert_eq!(compressed(&text), text);
    assert_eq!(compressed(&padded(16, 4)).len(), 1155);
    // 100 lines or fewer: the first and last 4,000 units are kept.
    let mut wide = String::new();
    for i in 0..50 {
        writeln!(wide, "{i}:{}", "abcdefghij".repeat(30)).expect("write");
    }
    assert_eq!(compressed(&wide).len(), 8060);
}

#[test]
fn a_file_that_is_not_utf8_is_refused() {
    let dir = tempfile::tempdir().expect("temp dir");
    let bin = dir.path().join("x.bin");
    let bytes: Vec<u8> = (0x80..0xC0).cycle().take(9000).collect();
    fs::write(&bin, bytes).expect("write");
    let bin = bin.to_str().expect("utf-8");
    for args in [vec![bin], vec!["--audit-after", bin]] {
        let out = report(&args);
        assert_eq!(out.code, 1);
        assert_eq!(out.stderr, format!("{bin}: not UTF-8 text\n"));
    }
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
         docs/my notes.md:4  [prose]  recap → cut.\n\
         nocolon  [code]  not a finding\n\
         src/d.rs:x  [code]  not a line number\n\
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
        "| Kind | Before | After |\n|---|---:|---:|\n| code | 2 | 1 |\n| prose | 2 | 0 |\n| all | 4 | 1 |\n"
    ), "{md}");
    assert!(!md.contains("## Tool output"));

    let svg = fs::read_to_string(&card).expect("card written");
    assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(svg.contains(">Audit findings</text>"));
    assert!(svg.contains("Before: 4 findings") && svg.contains("After: 1 findings"));
    for colour in [
        "#000000", "#9B7735", "#E5E4E2", "#FFFFF0", "#66023C", "#990024",
    ] {
        assert!(svg.contains(colour), "{colour}");
    }
    assert!(svg.contains("width=\"380\"") && svg.contains("width=\"95\""));
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
fn a_zero_count_draws_no_bar() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (before, after) = (dir.path().join("b.txt"), dir.path().join("a.txt"));
    let card = dir.path().join("c.svg");
    write(&before, "README.md:9  [prose]  recap → cut.\n");
    write(&after, "");
    let p = [&before, &after, &card].map(|p| p.to_str().expect("utf-8").to_string());
    ok(&report(&[
        "--audit-before",
        &p[0],
        "--audit-after",
        &p[1],
        "--svg",
        &p[2],
    ]));
    let svg = fs::read_to_string(&card).expect("card written");
    assert!(
        svg.contains("After: 0 findings</text><rect x=\"196\" y=\"105\" width=\"0\""),
        "{svg}"
    );
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
    // The file name only: a shared report carries no home directory.
    assert!(
        md.contains("`stats.json` records 1,234,567 characters (UTF-16 code units, as the hook counts them) removed from 42 tool outputs."),
        "{md}"
    );
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
