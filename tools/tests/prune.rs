// SPDX-License-Identifier: Apache-2.0

//! `prune` end to end on fixture repositories built here: exact deletions,
//! the proof model's refusals, and undo back to a clean `git diff`.

mod prune_support;
mod support;

use prune_support::{IGNORE, MANIFEST, clean, commit, git, prune, read, repo};
use puritanical_tools::prune::journal;
use std::path::Path;
use support::write;

const LIB_A: &str = r#"//! Fixture.

macro_rules! call {
    ($f:ident) => {
        $f()
    };
}

mod util;

pub fn api() -> u32 {
    used() + util::helper()
}

fn used() -> u32 {
    call!(by_macro)
}

fn by_macro() -> u32 {
    1
}

/// Dead, with a doc comment.
fn dead() -> u32 {
    2
}

#[cfg(feature = "extra")]
pub fn extra() -> u32 {
    feature_only()
}

fn feature_only() -> u32 {
    3
}

fn guarded() {}

struct Gone;

impl Gone {
    fn new() -> Self {
        Gone
    }
}

impl std::fmt::Debug for Gone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Gone")
    }
}

fn test_helper() -> u32 {
    4
}

#[cfg(test)]
mod tests {
    #[test]
    fn api_works() {
        assert_eq!(super::api(), 2);
    }

    #[test]
    fn helper() {
        assert_eq!(super::test_helper(), 4);
    }

    #[test]
    fn source_keeps_guarded() {
        assert!(include_str!("lib.rs").contains(concat!("fn guar", "ded() {}")));
    }
}
"#;

const UTIL_A: &str = "pub fn helper() -> u32 {\n    1\n}\n\npub fn used() -> u32 {\n    5\n}\n\npub fn dead() -> u32 {\n    6\n}\n";

#[test]
fn cuts_exactly_the_dead_items_the_tests_allow_and_undo_restores_every_byte() {
    let d = repo(&[
        ("Cargo.toml", MANIFEST),
        (".gitignore", IGNORE),
        ("src/lib.rs", LIB_A),
        ("src/util.rs", UTIL_A),
    ]);
    let dir = d.path();

    let dry = prune(dir, &[]);
    assert_eq!(dry.code, 0, "{}", dry.stderr);
    assert_eq!(
        dry.stdout,
        "src/lib.rs:24 function `dead`\nsrc/lib.rs:37 function `guarded`\nsrc/lib.rs:39 struct `Gone` and 2 impl blocks\n\
         src/util.rs:5 function `used`\nsrc/util.rs:9 function `dead`\n\
         5 candidate(s), 0 risky. Dry run: nothing changed. To cut: puritanical-tools prune --apply -- <check command>\n"
    );
    clean(dir);
    assert!(!dir.join(journal::DIR).exists());

    let out = prune(dir, &["--apply", "--", "cargo", "test", "--quiet"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(
        out.stdout
            .starts_with("baseline: check passes (libtest 3)\n"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout
            .contains("kept src/lib.rs:37 function `guarded` (the check failed without it)\n"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.ends_with("4 of 5 cut after 6 check run(s); review with `git diff`; `prune undo` restores, `prune keep` accepts\n"));
    assert_eq!(
        read(dir, "src/lib.rs"),
        LIB_A.replace(dead_fn(), "").replace(gone(), "")
    );
    assert_eq!(
        read(dir, "src/util.rs"),
        "pub fn helper() -> u32 {\n    1\n}\n"
    );
    assert_eq!(
        git(dir, &["status", "--porcelain"]),
        " M src/lib.rs\n M src/util.rs\n"
    );

    undo_refuses_an_edited_file_then_restores_every_byte(dir);
}

fn dead_fn() -> &'static str {
    "/// Dead, with a doc comment.\nfn dead() -> u32 {\n    2\n}\n\n"
}

fn gone() -> &'static str {
    &LIB_A[LIB_A.find("struct Gone;").expect("Gone")..LIB_A.find("fn test_helper").expect("helper")]
}

/// Continues from the run above, with its journal in place.
fn undo_refuses_an_edited_file_then_restores_every_byte(dir: &Path) {
    // A new run refuses whilst the journal is there; undo refuses a file edited since.
    assert!(
        prune(dir, &[])
            .stderr
            .contains("a journal from an earlier run")
    );
    let cut = read(dir, "src/util.rs");
    write(&dir.join("src/util.rs"), "// edited\n");
    let refused = prune(dir, &["undo"]);
    assert_eq!(
        (
            refused.code,
            refused
                .stderr
                .contains("edited since prune wrote them: src/util.rs")
        ),
        (1, true)
    );
    // Nothing changed: the other file still holds prune's cut.
    assert_eq!(read(dir, "src/util.rs"), "// edited\n");
    assert_eq!(
        read(dir, "src/lib.rs"),
        LIB_A.replace(dead_fn(), "").replace(gone(), "")
    );
    write(&dir.join("src/util.rs"), &cut);

    let undo = prune(dir, &["undo"]);
    assert_eq!(
        undo.stdout,
        "restored 2 file(s) to their bytes before the run\n"
    );
    clean(dir);
    assert!(!dir.join(journal::DIR).exists());
    assert!(prune(dir, &["undo"]).stderr.contains("nothing to undo"));
    assert!(prune(dir, &["keep"]).stderr.contains("nothing to keep"));
}

const LIB_B: &str = "\u{feff}//! Fixture B.

/// test result: ok. 1 passed;
pub fn live() {}

/// test result: ok. 1 passed;
fn counted() {}

fn plain() {}

mod inner {
    pub fn reached() {}

    fn dead_inner() {}
}

pub use inner::reached;

mod win;

pub use win::kept;

#[repr(C)]
struct Ffi {
    x: u8,
}
";

const WIN_B: &str = "pub fn kept() {}\r\n\r\nfn gone() {}\r\n";

#[test]
fn a_cut_that_lowers_the_test_count_stays_out_and_risky_items_wait_for_the_flag() {
    let d = repo(&[
        ("Cargo.toml", MANIFEST),
        (".gitignore", IGNORE),
        ("src/lib.rs", LIB_B),
        ("src/win.rs", WIN_B),
    ]);
    let dir = d.path();
    let out = prune(dir, &COUNTS);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "src/lib.rs:24 struct `Ffi` (risky: behind attribute #[repr]; cut only with --risky)\n\
         baseline: check passes (libtest 2)\n\
         cut src/lib.rs:9 function `plain`\ncut src/lib.rs:14 function `dead_inner`\ncut src/win.rs:3 function `gone`\n\
         kept src/lib.rs:7 function `counted` (the check failed without it)\n\
         3 of 4 cut after 6 check run(s); review with `git diff`; `prune undo` restores, `prune keep` accepts\n"
    );
    let want = LIB_B.replace("fn plain() {}\n\n", "").replace(
        "    pub fn reached() {}\n\n    fn dead_inner() {}\n",
        "    pub fn reached() {}\n",
    );
    assert_eq!(read(dir, "src/lib.rs"), want);
    assert_eq!(read(dir, "src/win.rs"), "pub fn kept() {}\r\n");

    assert_eq!(
        prune(dir, &["keep"]).stdout,
        "kept the cuts; journal removed\n"
    );
    assert!(!dir.join(journal::DIR).exists());
    commit(dir);

    count_less_checks_need_uncounted(dir);
}

/// Continues from the committed cuts above.
fn count_less_checks_need_uncounted(dir: &Path) {
    let quiet = ["--apply", "--risky", "--", "git", "status", "--porcelain"];
    let refused = prune(dir, &quiet);
    assert!(
        refused.stderr.contains("prints no test count"),
        "{}",
        refused.stderr
    );
    clean(dir);
    let out = prune(
        dir,
        &[
            "--apply",
            "--risky",
            "--uncounted",
            "--",
            "git",
            "status",
            "--porcelain",
        ],
    );
    assert!(
        out.stdout
            .contains("baseline: check passes (no test count)\n"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("2 of 2 cut after 1 check run(s)"),
        "{}",
        out.stdout
    );
    assert!(
        !read(dir, "src/lib.rs").contains("Ffi") && !read(dir, "src/lib.rs").contains("counted")
    );
}

/// A check that prints one libtest count line per marker comment in the sources.
const COUNTS: [&str; 10] = [
    "--apply",
    "--",
    "git",
    "grep",
    "-h",
    "-o",
    "-e",
    "test result: ok. [0-9]* passed;",
    "--",
    "src",
];

const LIB_C: &str = "pub fn live() {}\n\nfn plain() {}\n";

#[test]
fn a_failing_baseline_shows_its_output_and_zero_counts_need_uncounted() {
    let d = repo(&[
        ("Cargo.toml", MANIFEST),
        (".gitignore", IGNORE),
        ("src/lib.rs", LIB_C),
    ]);
    let dir = d.path();
    let alias = |script: &str| {
        let alias = format!("alias.chk=!{script}");
        prune(dir, &["--apply", "--", "git", "-c", &alias, "chk"])
    };
    assert_eq!(
        alias("echo one; echo broken; exit 1").stderr,
        "prune: the check fails before any cut; its output ends:\none\nbroken\n"
    );
    // A crate with no tests prints only zero counts: no test guards a cut.
    assert!(
        alias("echo 'test result: ok. 0 passed;'")
            .stderr
            .contains("prints no test count above 0")
    );
    // What a refused baseline changed is named, so the user can put it back.
    assert_eq!(
        alias("rm src/lib.rs; exit 1").stderr,
        "prune: the check fails before any cut; after the baseline check, the working tree is not clean: src/lib.rs\n"
    );
    git(dir, &["checkout", "--", "src/lib.rs"]);
    assert_eq!(
        alias("echo z > new.txt; echo 'test result: ok. 0 passed;'").stderr,
        "prune: after the baseline check, the working tree is not clean: new.txt\n"
    );
    std::fs::remove_file(dir.join("new.txt")).expect("rm");
    clean(dir);
}

#[test]
fn refuses_a_dirty_tree() {
    let d = repo(&[
        ("Cargo.toml", MANIFEST),
        (".gitignore", IGNORE),
        ("src/lib.rs", LIB_C),
        ("other.txt", "x\n"),
    ]);
    let dir = d.path();
    git(dir, &["mv", "other.txt", "moved.txt"]);
    write(&dir.join("junk.txt"), "j");
    let dirty = prune(dir, &["--apply", "--uncounted", "--", "git", "status"]).stderr;
    assert!(
        dirty.starts_with("prune: before the run, the working tree is not clean: "),
        "{dirty}"
    );
    assert!(
        dirty.contains("moved.txt") && dirty.contains("junk.txt") && dirty.contains("other.txt"),
        "{dirty}"
    );
}

#[test]
fn refuses_failing_checks_and_keeps_nothing_when_every_cut_fails() {
    let d = repo(&[
        ("Cargo.toml", MANIFEST),
        (".gitignore", IGNORE),
        ("src/lib.rs", LIB_C),
    ]);
    let dir = d.path();
    let grep = |word: &str| {
        [
            "--apply",
            "--uncounted",
            "--",
            "git",
            "grep",
            "-q",
            "-e",
            word,
            "--",
            "src",
        ]
        .map(str::to_string)
    };
    let p = |args: &[String]| prune(dir, &args.iter().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(
        p(&grep("no_such_word")).stderr,
        "prune: the check fails before any cut\n"
    );
    clean(dir);

    // Every cut fails the check: nothing changes and no journal stays.
    let none = p(&grep("fn plain"));
    assert!(
        none.stdout
            .ends_with("0 of 1 cut after 1 check run(s); nothing changed\n"),
        "{}",
        none.stdout
    );
    clean(dir);
}

#[cfg(unix)]
#[test]
fn a_path_through_a_symbolic_link_is_refused() {
    let d = tempfile::tempdir().expect("temp dir");
    std::os::unix::fs::symlink(d.path(), d.path().join("link")).expect("symlink");
    assert!(
        puritanical_tools::prune::safe_path(d.path(), "link/a.rs")
            .unwrap_err()
            .contains("symbolic link")
    );
}
