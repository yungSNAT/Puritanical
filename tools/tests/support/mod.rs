// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code, reason = "each test binary uses part of this module")]

use puritanical_tools::{Ctx, Out, run};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The repository this crate lives in.
pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Runs one command against `root` with the given environment.
pub fn cmd(root: &Path, env: &[(&str, &str)], args: &[&str]) -> Out {
    let ctx = Ctx {
        root: root.to_path_buf(),
        env: env
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect::<HashMap<_, _>>(),
    };
    run(
        &ctx,
        &args.iter().map(|a| (*a).to_string()).collect::<Vec<_>>(),
    )
}

pub fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().expect("parent")).expect("create dir");
    fs::write(path, text).expect("write");
}

pub fn ok(out: &Out) -> &str {
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    &out.stdout
}
