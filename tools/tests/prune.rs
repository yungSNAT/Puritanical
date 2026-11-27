// SPDX-License-Identifier: Apache-2.0

//! `prune` end to end on fixture repositories built here: exact deletions,
//! the proof model's refusals, and undo back to a clean `git diff`.

mod support;

use puritanical_tools::prune::journal::{self, Journal};
use puritanical_tools::{Ctx, Out, run};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use support::write;
use tempfile::TempDir;

/// The environment minus what the outer `cargo test` or coverage run set, so
/// nested cargo builds use the fixture's own target folder.
fn env() -> HashMap<String, String> {
    std::env::vars()
        .filter(|(k, _)| {
            k == "CARGO_HOME"
                || !(k.starts_with("CARGO_")
                    || k.starts_with("RUSTFLAGS")
                    || k.starts_with("LLVM_PROFILE")
                    || k == "RUSTDOCFLAGS")
        })
        .collect()
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn commit(dir: &Path) {
    git(dir, &["add", "-A"]);
    git(
        dir,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            "x",
        ],
    );
}

/// A committed Git repository holding `files`.
fn repo(files: &[(&str, &str)]) -> TempDir {
    let d = tempfile::tempdir().expect("temp dir");
    for (p, t) in files {
        write(&d.path().join(p), t);
    }
    git(d.path(), &["init", "-q"]);
    // Bytes as written on every system: no line-ending conversion.
    git(d.path(), &["config", "core.autocrlf", "false"]);
    commit(d.path());
    d
}

/// `prune --repo <dir> <args>`: `undo` and `keep` after `--repo`, as the docs give them.
fn prune(dir: &Path, args: &[&str]) -> Out {
    let mut all = vec![
        "prune".to_string(),
        "--repo".into(),
        dir.display().to_string(),
    ];
    all.extend(args.iter().map(|a| (*a).to_string()));
    run(
        &Ctx {
            root: dir.to_path_buf(),
            env: env(),
        },
        &all,
    )
}

fn read(dir: &Path, rel: &str) -> String {
    fs::read_to_string(dir.join(rel)).expect("read")
}

fn clean(dir: &Path) {
    assert!(!dir.join(journal::DIR).exists());
    assert_eq!(git(dir, &["status", "--porcelain"]), "");
    let diff = Command::new("git")
        .args(["diff", "--exit-code"])
        .current_dir(dir)
        .status()
        .expect("git diff");
    assert!(diff.success());
}

const MANIFEST: &str =
    "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[features]\nextra = []\n";
const IGNORE: &str = "/target\nCargo.lock\n";

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
    clean(dir);
}

#[test]
fn refuses_dirty_trees_failing_checks_and_checks_that_edit_files() {
    let d = repo(&[
        ("Cargo.toml", MANIFEST),
        (".gitignore", IGNORE),
        ("src/lib.rs", LIB_C),
        ("other.txt", "x\n"),
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

    git(dir, &["mv", "other.txt", "moved.txt"]);
    write(&dir.join("junk.txt"), "j");
    let dirty = p(&grep("live")).stderr;
    assert!(
        dirty.starts_with("prune: before the run, the working tree is not clean: "),
        "{dirty}"
    );
    assert!(
        dirty.contains("moved.txt") && dirty.contains("junk.txt") && dirty.contains("other.txt"),
        "{dirty}"
    );
    git(dir, &["mv", "moved.txt", "other.txt"]);
    fs::remove_file(dir.join("junk.txt")).expect("rm");

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

    // The check removes the journal's index, or all of it: prune rebuilds it
    // from memory.
    for script in [
        "rm .puritanical-prune/journal.json",
        "rm .puritanical-prune/.gitignore",
        "rm -r .puritanical-prune",
    ] {
        let lost = once_cut(dir, script);
        let want = stopped(".puritanical-prune", "prune's edits were undone");
        assert_eq!(lost.stderr, want);
        clean(dir);
    }
}

#[test]
fn undo_restores_what_the_run_began_with_whatever_the_check_staged() {
    let d = repo_d();
    let dir = d.path();
    for script in [
        "echo y >> other.txt; git add other.txt",
        "git rm -q other.txt",
        "git mv other.txt moved.txt",
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
    Command::new("chmod")
        .args(["u+w", ".puritanical-prune/wrote"])
        .current_dir(dir)
        .status()
        .expect("chmod");
    assert_eq!(prune(dir, &["undo"]).code, 0);
    clean(dir);
}

#[test]
fn a_cfg_with_not_around_a_feature_makes_its_package_risky() {
    // `helper` is dead with the default features and with all of them, yet a
    // build without `std` uses it.
    let manifest = "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[features]\ndefault = [\"std\"]\nstd = []\n";
    let lib = "#[cfg(not(feature = \"std\"))]\npub fn alloc_free() {\n    helper();\n}\n\nfn helper() {}\n";
    let d = repo(&[
        ("Cargo.toml", manifest),
        (".gitignore", IGNORE),
        ("src/lib.rs", lib),
    ]);
    assert_eq!(
        prune(d.path(), &[]).stdout,
        "src/lib.rs:6 function `helper` (risky: package has a cfg its builds do not cover; cut only with --risky)\n\
         1 candidate(s), 1 risky. Dry run: nothing changed. To cut: puritanical-tools prune --apply -- <check command>\n"
    );
}

#[test]
fn a_file_2_crates_compile_is_cut_only_where_both_call_it_dead() {
    let d = repo(&[
        ("Cargo.toml", MANIFEST),
        (".gitignore", "/target\nCargo.lock\nsrc/made.rs\n"),
        (
            "src/lib.rs",
            "mod made;\nmod util;\n\npub fn api() -> u32 {\n    util::f()\n}\n",
        ),
        ("src/main.rs", "mod util;\n\nfn main() {}\n"),
        (
            "src/util.rs",
            "pub fn f() -> u32 {\n    1\n}\n\npub fn g() {}\n",
        ),
        ("src/made.rs", "fn made_dead() {}\n"),
        ("tests/it.rs", "#[test]\nfn it() {}\n"),
    ]);
    let dir = d.path();
    // `f` is dead in the bin but used by the lib; `src/made.rs` is ignored by Git.
    assert_eq!(
        prune(dir, &[]).stdout,
        "src/made.rs:1 function `made_dead` (risky: not tracked by Git; cut only with --risky)\n\
         src/util.rs:5 function `g`\n\
         2 candidate(s), 1 risky. Dry run: nothing changed. To cut: puritanical-tools prune --apply -- <check command>\n"
    );
    // A baseline check that rewrites a file after the finder read it.
    let alias = "alias.chk=!echo >> src/made.rs; echo 'test result: ok. 1 passed;'";
    let out = prune(
        dir,
        &["--apply", "--risky", "--", "git", "-c", alias, "chk"],
    );
    assert_eq!(
        out.stderr,
        "prune: src/made.rs: changed after the finder read it\n"
    );
    assert!(!dir.join(journal::DIR).exists());
}

#[test]
fn path_modules_and_include_make_their_package_risky() {
    let hidden = repo(&[
        ("Cargo.toml", MANIFEST),
        (".gitignore", IGNORE),
        (
            "src/lib.rs",
            "include!(\"inc.rs\");\n\npub fn api() {}\n\nfn dead() {}\n",
        ),
        ("src/inc.rs", "pub fn from_inc() {}\n"),
        (
            "src/main.rs",
            "#[path = \"other.rs\"]\nmod x;\n\nfn main() {\n    x::y();\n}\n",
        ),
        ("src/other.rs", "pub fn y() {}\n\npub fn z() {}\n"),
    ]);
    let why = "(risky: package has a #[path] module or an include! item; cut only with --risky)";
    assert_eq!(
        prune(hidden.path(), &[]).stdout,
        format!(
            "src/lib.rs:5 function `dead` {why}\nsrc/other.rs:3 function `z` {why}\n\
                 2 candidate(s), 2 risky. Dry run: nothing changed. To cut: puritanical-tools prune --apply -- <check command>\n"
        )
    );
}

#[test]
fn platform_cfgs_failing_feature_sets_and_outside_paths_make_candidates_risky() {
    let tmp = tempfile::tempdir().expect("temp dir");
    write(
        &tmp.path().join("dep/Cargo.toml"),
        "[package]\nname = \"dep\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    );
    write(&tmp.path().join("dep/src/lib.rs"), "fn outside() {}\n");
    let dir = tmp.path().join("repo");
    let manifest = "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\ndep = { path = \"../dep\" }\n";
    let lib = "#[cfg(unix)]\npub fn only_unix() {}\n\nfn plain() {}\n\nmod tests;\n\npub use tests::reached;\n";
    for (p, t) in [
        ("Cargo.toml", manifest),
        (".gitignore", "/target\n"),
        ("src/lib.rs", lib),
        (
            "src/tests/mod.rs",
            "pub fn reached() {}\n\nfn hidden() {}\n",
        ),
    ] {
        write(&dir.join(p), t);
    }
    git(&dir, &["init", "-q"]);
    commit(&dir);

    let dry = prune(&dir, &[]);
    let lines: Vec<&str> = dry.stdout.lines().collect();
    assert_eq!(lines.len(), 3, "{}{}", dry.stdout, dry.stderr);
    let note = lines[0].replace('\\', "/");
    let outside = "dep/src/lib.rs: outside the repository";
    assert!(
        note.starts_with("note: skipped ") && note.ends_with(outside),
        "{note}"
    );
    assert_eq!(
        lines[1],
        "src/lib.rs:4 function `plain` (risky: package has a cfg its builds do not cover; cut only with --risky)"
    );
    assert_eq!(
        lines[2],
        "1 candidate(s), 1 risky. Dry run: nothing changed. To cut: puritanical-tools prune --apply -- <check command>"
    );
    // Cargo.lock is not ignored here, so the finder's build dirties the tree.
    let apply = ["--apply", "--uncounted", "--", "git", "status"];
    let mut risky = apply.to_vec();
    risky.insert(1, "--risky");
    fs::remove_file(dir.join("Cargo.lock")).expect("rm");
    assert_eq!(
        prune(&dir, &risky).stderr,
        "prune: after the finder's build, the working tree is not clean: Cargo.lock\n"
    );
    write(&dir.join(".git/info/exclude"), "Cargo.lock\n");
    assert!(prune(&dir, &apply).stdout.ends_with("nothing to cut\n"));
}

#[test]
fn a_failing_all_features_build_makes_every_candidate_risky() {
    let exclusive = "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[features]\na = []\nb = []\n";
    let lib = "#[cfg(all(feature = \"a\", feature = \"b\"))]\ncompile_error!(\"a and b exclude each other\");\n\nfn plain() {}\n";
    let e = repo(&[
        ("Cargo.toml", exclusive),
        (".gitignore", IGNORE),
        ("src/lib.rs", lib),
    ]);
    assert_eq!(
        prune(e.path(), &[]).stdout,
        "note: the --all-features build fails, so code behind features was not seen: every candidate is risky\n\
         src/lib.rs:4 function `plain` (risky: features not checked; cut only with --risky)\n\
         1 candidate(s), 1 risky. Dry run: nothing changed. To cut: puritanical-tools prune --apply -- <check command>\n"
    );
}

#[test]
fn needs_a_building_cargo_workspace_at_the_top_of_a_git_work_tree() {
    let broken = repo(&[
        ("Cargo.toml", MANIFEST),
        (".gitignore", IGNORE),
        ("src/lib.rs", "fn (\n"),
    ]);
    assert!(
        prune(broken.path(), &[])
            .stderr
            .contains("prune needs a crate that builds")
    );
    let bad = repo(&[("Cargo.toml", "[package\n")]);
    assert_eq!(
        prune(bad.path(), &[]).stderr,
        "prune: no Cargo workspace here\n"
    );
}

#[test]
fn needs_the_top_of_a_git_work_tree() {
    let plain = repo(&[("README.md", "x\n"), ("sub/a.txt", "a\n")]);
    let out = prune(plain.path(), &[]);
    assert_eq!(
        out.stdout,
        "no finder for this repository: prune finds dead Rust code in a Cargo workspace\n"
    );
    assert!(
        prune(&plain.path().join("sub"), &[])
            .stderr
            .contains("is not the top folder of a Git work tree")
    );
    assert_eq!(prune(&plain.path().join("none"), &[]).code, 1);
    let bare = tempfile::tempdir().expect("temp dir");
    assert!(
        prune(bare.path(), &[])
            .stderr
            .contains("`git rev-parse --show-toplevel` failed")
    );

    // The binary: the current folder is the default repository; bad usage exits 2.
    let bin = || {
        let mut c = Command::new(env!("CARGO_BIN_EXE_puritanical-tools"));
        c.env_clear().envs(env());
        c
    };
    let out = bin()
        .arg("prune")
        .current_dir(plain.path())
        .output()
        .expect("run");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "no finder for this repository: prune finds dead Rust code in a Cargo workspace\n"
    );
    let usage = prune(plain.path(), &["--frob"]);
    assert_eq!(usage.code, 2);
    assert!(usage.stderr.starts_with("usage: puritanical-tools prune"));
    let out = bin().args(["prune", "--frob"]).output().expect("run");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).starts_with("usage: puritanical-tools prune"));

    write(&plain.path().join(journal::DIR), "not a folder");
    assert!(
        prune(plain.path(), &["undo"])
            .stderr
            .contains("is not a plain folder")
    );
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
