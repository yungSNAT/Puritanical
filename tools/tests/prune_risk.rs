// SPDX-License-Identifier: Apache-2.0

//! `prune`'s finder on fixture workspaces: what makes a candidate risky,
//! and the workspaces it refuses.

mod prune_support;
mod support;

use prune_support::{IGNORE, MANIFEST, commit, env, git, prune, repo};
use puritanical_tools::prune::journal;
use std::process::Command;
use support::write;

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
    let why = "(risky: package has a #[path] module, attribute macro or macro call; cut only with --risky)";
    assert_eq!(
        prune(hidden.path(), &[]).stdout,
        format!(
            "src/lib.rs:5 function `dead` {why}\nsrc/other.rs:3 function `z` {why}\n\
                 2 candidate(s), 2 risky. Dry run: nothing changed. To cut: puritanical-tools prune --apply -- <check command>\n"
        )
    );
}

/// A repository (`repo/` in the returned folder) whose package has a
/// platform cfg and a path dependency outside it, with no ignored `Cargo.lock`.
fn platform_repo() -> tempfile::TempDir {
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
    tmp
}

#[test]
fn platform_cfgs_and_outside_paths_make_candidates_risky() {
    let tmp = platform_repo();
    let dry = prune(&tmp.path().join("repo"), &[]);
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
}

#[test]
fn a_build_that_dirties_the_tree_stops_the_run() {
    let tmp = platform_repo();
    let dir = tmp.path().join("repo");
    // Cargo.lock is not ignored here, so the finder's build dirties the tree.
    let apply = ["--apply", "--uncounted", "--", "git", "status"];
    let mut risky = apply.to_vec();
    risky.insert(1, "--risky");
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

const PM_LIB: &str = r##"use proc_macro::{Delimiter, Group, TokenStream, TokenTree};

#[proc_macro_attribute]
pub fn keep_on_freebsd(_: TokenStream, item: TokenStream) -> TokenStream {
    let extra: TokenStream = "#[cfg(target_os = \"freebsd\")] fn _u() { dead_in_ext(); }"
        .parse()
        .expect("tokens");
    item.into_iter()
        .map(|tt| match tt {
            TokenTree::Group(g) if g.delimiter() == Delimiter::Brace => {
                let mut body = g.stream();
                body.extend(extra.clone());
                let mut out = Group::new(Delimiter::Brace, body);
                out.set_span(g.span());
                TokenTree::Group(out)
            }
            other => other,
        })
        .collect()
}

#[proc_macro]
pub fn freebsd_use(_: TokenStream) -> TokenStream {
    "{ #[cfg(target_os = \"freebsd\")] { dead_in_fn(); } }"
        .parse()
        .expect("tokens")
}
"##;

/// A repository (`repo/`) depending on a sibling proc-macro crate `pm/`.
fn proc_macro_repo(lib: &str, extra: &[(&str, &str)]) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("temp dir");
    write(
        &tmp.path().join("pm/Cargo.toml"),
        "[package]\nname = \"pm\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\nproc-macro = true\n",
    );
    write(&tmp.path().join("pm/src/lib.rs"), PM_LIB);
    let dir = tmp.path().join("repo");
    let manifest = "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\npm = { path = \"../pm\" }\n";
    for (p, t) in [
        ("Cargo.toml", manifest),
        (".gitignore", IGNORE),
        ("src/lib.rs", lib),
    ]
    .into_iter()
    .chain(extra.iter().copied())
    {
        write(&dir.join(p), t);
    }
    git(&dir, &["init", "-q"]);
    git(&dir, &["config", "core.autocrlf", "false"]);
    commit(&dir);
    tmp
}

#[test]
fn an_attributed_external_module_makes_its_package_risky() {
    let tmp = proc_macro_repo(
        "#[pm::keep_on_freebsd]\nmod ext;\n",
        &[("src/ext.rs", "fn dead_in_ext() {}\n")],
    );
    let dry = prune(&tmp.path().join("repo"), &[]);
    assert_eq!(dry.code, 0, "{}", dry.stderr);
    assert!(
        dry.stdout
            .contains("src/ext.rs:1 function `dead_in_ext` (risky:")
            && dry.stdout.contains("1 candidate(s), 1 risky."),
        "{}",
        dry.stdout
    );
}

#[test]
fn a_function_body_proc_macro_makes_its_package_risky() {
    for lib in [
        "pub fn api() {\n    pm::freebsd_use!();\n}\n\nfn dead_in_fn() {}\n",
        "pub fn api() {\n    assert!({ pm::freebsd_use!(); true });\n}\n\nfn dead_in_fn() {}\n",
    ] {
        let tmp = proc_macro_repo(lib, &[]);
        let dry = prune(&tmp.path().join("repo"), &[]);
        assert_eq!(dry.code, 0, "{}", dry.stderr);
        assert!(
            dry.stdout
                .contains("src/lib.rs:5 function `dead_in_fn` (risky:")
                && dry.stdout.contains("1 candidate(s), 1 risky."),
            "{lib}\n{}",
            dry.stdout
        );
    }
}
