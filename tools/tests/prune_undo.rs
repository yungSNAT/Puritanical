// SPDX-License-Identifier: Apache-2.0

//! `prune` when its check changes files: the run stops, prune puts back
//! its own edits, and `undo` restores the rest; and the journal after a crash.

mod prune_support;
mod support;

use prune_support::{IGNORE, MANIFEST, clean, commit, git, prune, read, repo};
use puritanical_tools::Out;
use puritanical_tools::prune::journal::{self, Journal};
use std::fs;
use std::path::Path;
use support::write;
use tempfile::TempDir;

const LIB_D: &str = "mod util;\n\npub fn live() -> u32 {\n    util::used()\n}\n\nfn plain() {}\n";
const UTIL_D: &str = "pub fn used() -> u32 {\n    1\n}\n\npub fn dead() {}\n";

/// 2 files with a dead item each, and a file prune never cuts.
fn repo_d() -> TempDir {
    repo(&[
        ("Cargo.toml", MANIFEST),
        (".gitignore", IGNORE),
        ("src/lib.rs", LIB_D),
        ("src/util.rs", UTIL_D),
        ("other.txt", "x\n"),
    ])
}

/// `prune --apply` with a check (a Git alias, so it runs on every system)
/// that runs `script` once `plain` is cut, then prints a libtest count.
fn once_cut(dir: &Path, script: &str) -> Out {
    let alias = format!(
        "alias.chk=!grep -q 'fn plain' src/lib.rs || {{ {script}; }}; echo 'test result: ok. 1 passed;'"
    );
    prune(dir, &["--apply", "--", "git", "-c", &alias, "chk"])
}

fn stopped(paths: &str, tail: &str) -> String {
    format!("prune: the check changed files itself, so the run stopped: {paths}; {tail}\n")
}

const FOR_UNDO: &str =
    "prune undid its own edits; `prune undo` puts back the files the check changed";

#[test]
fn a_check_that_edits_a_cut_file_stops_the_run_and_undo_puts_back_every_byte() {
    let d = repo_d();
    let dir = d.path();
    let made = prune(
        dir,
        &[
            "--apply",
            "--",
            "git",
            "-c",
            "alias.chk=!echo z > new.txt; echo 'test result: ok. 1 passed;'",
            "chk",
        ],
    );
    assert_eq!(
        made.stderr,
        "prune: after the baseline check, the working tree is not clean: new.txt\n"
    );
    fs::remove_file(dir.join("new.txt")).expect("rm");

    let out = once_cut(dir, "echo y >> src/lib.rs");
    assert_eq!(out.stderr, stopped("src/lib.rs", FOR_UNDO));
    // The other file prune cut is back; the one the check edited waits for undo.
    assert_eq!(read(dir, "src/util.rs"), UTIL_D);
    let left = LIB_D.replace("\nfn plain() {}\n", "") + "y\n";
    assert_eq!(read(dir, "src/lib.rs"), left);
    assert!(prune(dir, &["keep"]).stderr.contains("did not finish"));

    // An edit made after the stop is never overwritten.
    write(&dir.join("src/lib.rs"), "// mine\n");
    let refused = prune(dir, &["undo"]);
    assert!(
        refused
            .stderr
            .contains("edited since prune wrote them: src/lib.rs"),
        "{}",
        refused.stderr
    );
    assert_eq!(
        (read(dir, "src/lib.rs"), read(dir, "src/util.rs")),
        ("// mine\n".into(), UTIL_D.into())
    );
    write(&dir.join("src/lib.rs"), &left);
    assert_eq!(prune(dir, &["undo"]).code, 0);
    clean(dir);
}

#[test]
fn a_check_that_deletes_or_makes_files_or_harms_the_journal_is_undone_too() {
    let d = repo_d();
    let dir = d.path();
    let gone = once_cut(dir, "rm other.txt");
    assert_eq!(gone.stderr, stopped("other.txt", FOR_UNDO));
    assert_eq!(
        (read(dir, "src/lib.rs"), read(dir, "src/util.rs")),
        (LIB_D.into(), UTIL_D.into())
    );
    assert!(!dir.join("other.txt").exists());
    assert_eq!(prune(dir, &["undo"]).code, 0);
    clean(dir);
    assert_eq!(read(dir, "other.txt"), "x\n");

    let made = once_cut(dir, "echo z > made.txt");
    assert_eq!(made.stderr, stopped("made.txt", FOR_UNDO));
    // A file the check made and the user then removed: undo removes it anyway.
    fs::remove_file(dir.join("made.txt")).expect("rm");
    assert_eq!(prune(dir, &["undo"]).code, 0);
    clean(dir);

    // The check removes or overwrites part of the journal, or all of it:
    // prune rebuilds it from memory.
    for script in [
        "echo bad > .puritanical-prune/orig/0",
        "rm .puritanical-prune/orig/0",
        "rm .puritanical-prune/journal.json",
        "rm .puritanical-prune/.gitignore",
        "rm -r .puritanical-prune",
    ] {
        let lost = once_cut(dir, script);
        let want = stopped(".puritanical-prune", "prune's edits were undone");
        assert_eq!(lost.stderr, want);
        clean(dir);
    }
    // The rebuilt journal also records what else the check changed.
    let both = once_cut(dir, "rm -r .puritanical-prune; echo y >> other.txt");
    assert_eq!(
        both.stderr,
        stopped("other.txt, .puritanical-prune", FOR_UNDO)
    );
    assert_eq!(prune(dir, &["undo"]).code, 0);
    clean(dir);
    assert_eq!(read(dir, "other.txt"), "x\n");
}

#[test]
fn undo_restores_what_the_run_began_with_whatever_the_check_staged() {
    let d = repo_d();
    let dir = d.path();
    // With `-z`, a rename's original path has no status prefix: a short one
    // must not be taken for padding.
    write(&dir.join("ab"), "s\n");
    commit(dir);
    for script in [
        "echo y >> other.txt; git add other.txt",
        "git rm -q other.txt",
        "git mv other.txt moved.txt",
        "git mv ab ba; echo y >> other.txt",
    ] {
        let out = once_cut(dir, script);
        assert!(
            out.stderr.ends_with(&format!("{FOR_UNDO}\n")),
            "{script}: {}",
            out.stderr
        );
        assert_eq!(prune(dir, &["undo"]).code, 0, "{script}");
        // Undo puts back the files; the check's staging is the user's to drop.
        git(dir, &["reset", "-q"]);
        assert_eq!(read(dir, "other.txt"), "x\n", "{script}");
        assert_eq!(read(dir, "ab"), "s\n", "{script}");
        clean(dir);
    }
}

#[cfg(unix)]
#[test]
fn a_file_prune_cannot_record_is_named_and_the_journal_kept() {
    let d = repo_d();
    let dir = d.path();
    let out = once_cut(dir, "ln -s other.txt link.txt");
    assert!(
        out.stderr
            .contains("; recording them failed: link.txt: passes through a symbolic link"),
        "{}",
        out.stderr
    );
    assert!(dir.join(journal::DIR).exists());
    fs::remove_file(dir.join("link.txt")).expect("rm");
    assert_eq!(prune(dir, &["undo"]).code, 0);
    clean(dir);
}

#[cfg(unix)]
#[test]
fn what_a_passing_check_leaves_running_is_stopped_with_it() {
    let d = repo_d();
    let dir = d.path();
    let check = "(sleep 1; echo late >> other.txt) & echo 'test result: ok. 1 passed;'";
    let out = prune(dir, &["--apply", "--", "sh", "-c", check]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    std::thread::sleep(std::time::Duration::from_secs(2));
    assert_eq!(read(dir, "other.txt"), "x\n");
}

#[cfg(unix)]
#[test]
fn when_prune_cannot_put_its_files_back_undo_can() {
    let d = repo_d();
    let dir = d.path();
    // The first trial fails and locks the journal, so the next write fails.
    let out = once_cut(dir, "chmod a-w .puritanical-prune/wrote; exit 1");
    assert!(
        out.stderr.contains("; putting the files back failed: ")
            && out.stderr.ends_with("; run `prune undo`\n"),
        "{}",
        out.stderr
    );
    std::process::Command::new("chmod")
        .args(["u+w", ".puritanical-prune/wrote"])
        .current_dir(dir)
        .status()
        .expect("chmod");
    assert_eq!(prune(dir, &["undo"]).code, 0);
    clean(dir);
}

#[test]
fn the_journal_restores_original_bytes_after_a_crash_at_any_step() {
    let d = tempfile::tempdir().expect("temp dir");
    let root = d.path();
    let j_dir = root.join(journal::DIR);
    write(&root.join("a.rs"), "orig\r\n\u{e9}");
    write(&root.join("b.rs"), "b");

    let mut j = Journal::create(root).expect("create");
    assert!(Journal::create(root).is_err());
    assert!(j.write("../x", b"no").is_err());
    j.write("a.rs", b"v1").expect("write");
    j.write("a.rs", b"v2").expect("write again");
    j.write("b.rs", b"b").expect("unchanged bytes");
    // Crash after the replace, before `wrote/0.new` became `wrote/0`, with a temp file left.
    write(&j_dir.join("wrote/0.new"), "v3");
    write(&root.join("a.rs"), "v3");
    write(&root.join(".a.rs.puritanical-prune.tmp"), "partial");
    assert!(journal::keep(root).unwrap_err().contains("did not finish"));
    assert_eq!(journal::undo(root), Ok(2));
    assert_eq!(
        fs::read(root.join("a.rs")).expect("a"),
        "orig\r\n\u{e9}".as_bytes()
    );
    assert!(!root.join(".a.rs.puritanical-prune.tmp").exists() && !j_dir.exists());

    let mut j = Journal::create(root).expect("create");
    j.write("a.rs", b"v1").expect("write");
    j.finish().expect("finish");
    fs::remove_file(root.join("a.rs")).expect("rm");
    assert!(
        journal::undo(root)
            .unwrap_err()
            .contains("edited since prune wrote them: a.rs")
    );
    write(&root.join("a.rs"), "v1");
    assert_eq!(journal::keep(root), Ok(()));
    assert_eq!(fs::read_to_string(root.join("a.rs")).expect("a"), "v1");

    // A crash as the run began: a journal folder with no index and no copy.
    fs::create_dir_all(j_dir.join("orig")).expect("dir");
    assert_eq!(journal::undo(root), Ok(0));
    assert!(!j_dir.exists());

    let j = Journal::create(root).expect("create");
    j.discard().expect("discard");
    assert!(!j_dir.exists());
    Journal::create(root).expect("create");
    write(&j_dir.join("journal.json"), "{\"files\": [1]}");
    assert!(
        journal::keep(root)
            .unwrap_err()
            .contains("not a prune journal")
    );
}
