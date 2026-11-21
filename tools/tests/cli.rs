// SPDX-License-Identifier: MIT

//! The binary: usage, version, exit codes and the repository root.

mod support;

use std::process::Command;
use support::repo;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_puritanical-tools"))
}

#[test]
fn usage_help_and_version() {
    let out = bin().output().expect("run");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).starts_with("usage: puritanical-tools <command>"));

    let out = bin().arg("--help").output().expect("run");
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("readme-art [--check]"));

    let out = bin().arg("--version").output().expect("run");
    let version = String::from_utf8_lossy(&out.stdout).into_owned();
    assert_eq!(
        version,
        format!("puritanical-tools {}\n", env!("CARGO_PKG_VERSION"))
    );

    let out = bin().arg("frobnicate").output().expect("run");
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn checks_run_against_the_crate_repository_or_puritanical_root() {
    let out = bin()
        .args(["rules", "--check"])
        .env_remove("PURITANICAL_ROOT")
        .output()
        .expect("run");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "All rule copies in sync.\n"
    );

    let empty = tempfile::tempdir().expect("temp dir");
    let out = bin()
        .args(["rules", "--check"])
        .env("PURITANICAL_ROOT", empty.path())
        .output()
        .expect("run");
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .ends_with("6 file(s) out of sync. Run: npm run build:rules\n")
    );

    let out = bin()
        .args(["chart", "--check"])
        .current_dir(empty.path())
        .env("PURITANICAL_ROOT", repo())
        .output();
    assert_eq!(out.expect("run").status.code(), Some(0));
}
