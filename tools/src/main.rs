// SPDX-License-Identifier: MIT

use puritanical_tools::{Ctx, run};
use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

/// The repository root: the parent of this crate, or `PURITANICAL_ROOT`.
fn root() -> PathBuf {
    std::env::var_os("PURITANICAL_ROOT").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".."),
        PathBuf::from,
    )
}

fn main() -> ExitCode {
    // nosemgrep: rust.lang.security.args-os.args-os -- names a command and its paths; nothing trusts them
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let ctx = Ctx {
        root: root(),
        env: std::env::vars_os()
            .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
            .collect(),
    };
    let out = run(&ctx, &args);
    // A closed pipe (`| head`) is not an error worth a panic.
    let _ = std::io::stdout().write_all(out.stdout.as_bytes());
    let _ = std::io::stderr().write_all(out.stderr.as_bytes());
    ExitCode::from(out.code)
}
