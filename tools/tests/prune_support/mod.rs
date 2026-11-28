// SPDX-License-Identifier: Apache-2.0

//! Fixture repositories and a `prune` runner for the `prune` test binaries.
#![allow(dead_code, reason = "each test binary uses part of this module")]

use crate::support::write;
use puritanical_tools::prune::journal;
use puritanical_tools::{Ctx, Out, run};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

/// The environment minus what the outer `cargo test` or coverage run set, so
/// nested cargo builds use the fixture's own target folder.
pub fn env() -> HashMap<String, String> {
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

pub fn git(dir: &Path, args: &[&str]) -> String {
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

pub fn commit(dir: &Path) {
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
pub fn repo(files: &[(&str, &str)]) -> TempDir {
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
pub fn prune(dir: &Path, args: &[&str]) -> Out {
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

pub fn read(dir: &Path, rel: &str) -> String {
    fs::read_to_string(dir.join(rel)).expect("read")
}

pub fn clean(dir: &Path) {
    assert!(!dir.join(journal::DIR).exists());
    assert_eq!(git(dir, &["status", "--porcelain"]), "");
    let diff = Command::new("git")
        .args(["diff", "--exit-code"])
        .current_dir(dir)
        .status()
        .expect("git diff");
    assert!(diff.success());
}

pub const MANIFEST: &str =
    "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[features]\nextra = []\n";
pub const IGNORE: &str = "/target\nCargo.lock\n";
