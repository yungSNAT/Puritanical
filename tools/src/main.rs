// SPDX-License-Identifier: MIT

use puritanical_tools::{Ctx, js, run};
use std::io::{ErrorKind, Write};
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
    let mut out = run(&ctx, &args);
    let mut code = out.code;
    if let Err(e) = emit(&mut std::io::stdout(), &out.stdout) {
        out.err(format!("puritanical-tools: cannot write output: {e}"));
        code = code.max(1);
    }
    if emit(&mut std::io::stderr(), &out.stderr).is_err() {
        code = code.max(1);
    }
    ExitCode::from(code)
}

/// Writes `text` as Node does, a lone surrogate as U+FFFD. A closed pipe
/// (`| head`) is not an error; any other failure is.
fn emit(w: &mut impl Write, text: &str) -> std::io::Result<()> {
    match w
        .write_all(js::lossy(text).as_bytes())
        .and_then(|()| w.flush())
    {
        Err(e) if e.kind() != ErrorKind::BrokenPipe => Err(e),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::emit;
    use std::io::{Error, ErrorKind, Write};

    /// A writer that fails every write with `kind`.
    struct Fails(ErrorKind);

    impl Write for Fails {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(Error::from(self.0))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn only_a_closed_pipe_is_ignored() {
        assert!(emit(&mut Fails(ErrorKind::BrokenPipe), "x").is_ok());
        assert!(emit(&mut Fails(ErrorKind::StorageFull), "x").is_err());
        let mut buf = Vec::new();
        emit(&mut buf, "ok").expect("written");
        assert_eq!(buf, b"ok");
        assert!(Fails(ErrorKind::Other).flush().is_ok());
    }
}
