// SPDX-License-Identifier: MIT

//! Maintainer tooling for Chisle: the generated files CI checks and the
//! benchmark analysis. Hooks, host plugins and the agent-launching harnesses
//! stay in their hosts' languages.

pub mod aggregate;
pub mod answers;
pub mod chart;
pub mod js;
pub mod normalize_pi;
pub mod pooled;
pub mod readme_art;
pub mod rules;
pub mod samples;
pub mod score;
pub mod stats;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const USAGE: &str = "usage: puritanical-tools <command>
  rules [--check]          write (or verify) the per-agent rule copies
  chart [--check|--json]   write (or verify) assets/benchmark*.svg
  samples [--check]        write (or verify) site/src/data/samples.json
  readme-art [--check]     write (or verify) assets/readme/
  aggregate [--json]       billed tokens per arm (RAW_DIR, default benchmarks/results/raw)
  pooled <dir>...          pooled ratios with a bootstrap interval (ARMS)
  answers <run-dir> [arm ...]  write <run-dir>/ANSWERS.md
  normalize-pi <events.jsonl>  Pi's event stream as one benchmark cell
  agentic-score [--json]   agentic suite table (RAW_DIR, ARMS)";

/// What a command prints and its exit status.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Out {
    pub stdout: String,
    pub stderr: String,
    pub code: u8,
}

impl Out {
    /// A failure with one line on stderr.
    #[must_use]
    pub fn fail(message: impl Into<String>) -> Self {
        Self {
            stderr: message.into() + "\n",
            code: 1,
            ..Self::default()
        }
    }

    /// `console.log(line)`.
    pub fn say(&mut self, line: impl AsRef<str>) {
        self.stdout.push_str(line.as_ref());
        self.stdout.push('\n');
    }

    /// `console.error(line)`.
    pub fn err(&mut self, line: impl AsRef<str>) {
        self.stderr.push_str(line.as_ref());
        self.stderr.push('\n');
    }
}

/// The repository and the environment a command reads.
pub struct Ctx {
    pub root: PathBuf,
    pub env: HashMap<String, String>,
}

impl Ctx {
    #[must_use]
    pub fn var(&self, key: &str) -> Option<&str> {
        self.env
            .get(key)
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }

    /// `RAW_DIR` when set, else `default` under the repository.
    #[must_use]
    pub fn raw_dir(&self, default: &str) -> PathBuf {
        self.var("RAW_DIR")
            .map_or_else(|| self.root.join(default), PathBuf::from)
    }
}

/// Writes `text` to `path`, creating its directory.
///
/// # Errors
/// The directory or file cannot be written.
pub fn write(path: &Path, text: &str) -> Result<(), String> {
    let fail = |e: std::io::Error| format!("{}: {e}", path.display());
    std::fs::create_dir_all(path.parent().unwrap_or(Path::new("."))).map_err(fail)?;
    std::fs::write(path, text).map_err(fail)
}

/// Compares or writes one generated file. `stale` is the `--check` failure line.
#[must_use]
pub fn sync_file(path: &Path, want: &str, check: bool, stale: &str) -> Option<Out> {
    if check {
        let have = std::fs::read_to_string(path).unwrap_or_default();
        return (have != want).then(|| Out::fail(stale));
    }
    write(path, want).err().map(Out::fail)
}

/// Runs one command line (without the program name).
#[must_use]
pub fn run(ctx: &Ctx, args: &[String]) -> Out {
    let flag = |f: &str| args.iter().skip(1).any(|a| a == f);
    match args.first().map(String::as_str) {
        Some("rules") => rules::run(&ctx.root, flag("--check")),
        Some("chart") => chart::run(&ctx.root, flag("--check"), flag("--json")),
        Some("samples") => samples::run(&ctx.root, flag("--check")),
        Some("readme-art") => readme_art::run(&ctx.root, flag("--check")),
        Some("aggregate") => aggregate::run(ctx, flag("--json")),
        Some("pooled") => pooled::run(ctx, &args[1..]),
        Some("answers") => answers::run(&ctx.root, &args[1..]),
        Some("normalize-pi") => normalize_pi::run(&args[1..]),
        Some("agentic-score") => score::run(ctx, flag("--json")),
        Some("--version" | "-V") => Out {
            stdout: format!("puritanical-tools {}\n", env!("CARGO_PKG_VERSION")),
            ..Out::default()
        },
        Some("help" | "--help" | "-h") => Out {
            stdout: format!("{USAGE}\n"),
            ..Out::default()
        },
        _ => Out {
            stderr: format!("{USAGE}\n"),
            code: 2,
            ..Out::default()
        },
    }
}
