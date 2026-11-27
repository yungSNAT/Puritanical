// SPDX-License-Identifier: Apache-2.0

//! The binary: usage, version, exit codes and the repository root.

mod support;

use std::fs;
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

    // A root that is not a clone (as a release binary's built-in one is not
    // on the user's machine) is refused, and nothing is written into it.
    let empty = tempfile::tempdir().expect("temp dir");
    let other = tempfile::tempdir().expect("temp dir");
    fs::write(other.path().join("package.json"), r#"{"name":"other"}"#).expect("write");
    for (c, root) in [
        ("rules", &empty),
        ("readme-art", &empty),
        ("rules", &other),
        ("readme-art", &other),
    ] {
        let out = bin()
            .arg(c)
            .env("PURITANICAL_ROOT", root.path())
            .output()
            .expect("run");
        assert_eq!(out.status.code(), Some(1), "{c}");
        assert!(
            String::from_utf8_lossy(&out.stderr)
                .ends_with(": not a Puritanical clone; set PURITANICAL_ROOT to one\n"),
            "{c}"
        );
    }
    assert_eq!(fs::read_dir(empty.path()).expect("read dir").count(), 0);
    assert_eq!(fs::read_dir(other.path()).expect("read dir").count(), 1);

    let out = bin()
        .args(["readme-art", "--check"])
        .current_dir(empty.path())
        .env("PURITANICAL_ROOT", repo())
        .output();
    assert_eq!(out.expect("run").status.code(), Some(0));
}

/// `/dev/full` fails every write with ENOSPC, unlike a closed pipe.
#[cfg(target_os = "linux")]
#[test]
fn a_failed_write_is_an_error() {
    let full = || std::fs::File::create("/dev/full").expect("/dev/full");
    let out = bin().arg("--help").stdout(full()).output().expect("run");
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .starts_with("puritanical-tools: cannot write output: "),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = bin().arg("--version").stderr(full()).output().expect("run");
    assert_eq!(out.status.code(), Some(0), "nothing went to stderr");
    let out = bin()
        .arg("frobnicate")
        .stderr(full())
        .output()
        .expect("run");
    assert_eq!(out.status.code(), Some(2));
}
