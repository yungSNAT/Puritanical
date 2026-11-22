// SPDX-License-Identifier: Apache-2.0 AND MIT
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

pub fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create dir");
    for entry in fs::read_dir(from).expect("list") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("copy");
        }
    }
}

pub fn ok(out: &Out) -> &str {
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    &out.stdout
}
