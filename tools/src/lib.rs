// SPDX-License-Identifier: Apache-2.0 AND MIT

//! Maintainer tooling for Puritanical: the generated files CI checks and the
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
pub mod score;
pub mod stats;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const USAGE: &str = "usage: puritanical-tools <command>
  rules [--check]          write (or verify) the per-agent rule copies
  chart [--check|--json]   write (or verify) assets/benchmark*.svg
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

/// Writes `text` to `path`, creating its directory. A lone surrogate becomes
/// U+FFFD, as `fs.writeFileSync` writes it.
///
/// # Errors
/// The directory or file cannot be written.
pub fn write(path: &Path, text: &str) -> Result<(), String> {
    let fail = |e: std::io::Error| format!("{}: {e}", path.display());
    std::fs::create_dir_all(path.parent().unwrap_or(Path::new("."))).map_err(fail)?;
    std::fs::write(path, js::lossy(text)).map_err(fail)
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
    let (cmd, rest) = args
        .split_first()
        .map_or(("", &[][..]), |(c, rest)| (c.as_str(), rest));
    generate(&ctx.root, cmd, rest)
        .or_else(|| analyse(ctx, cmd, rest))
        .unwrap_or_else(|| about(cmd))
}

fn has(rest: &[String], flag: &str) -> bool {
    rest.iter().any(|a| a == flag)
}

/// The generated-file commands.
fn generate(root: &Path, cmd: &str, rest: &[String]) -> Option<Out> {
    let check = has(rest, "--check");
    Some(match cmd {
        "rules" => rules::run(root, check),
        "chart" => chart::run(root, check, has(rest, "--json")),
        "readme-art" => readme_art::run(root, check),
        _ => return None,
    })
}

/// The benchmark analysis commands.
fn analyse(ctx: &Ctx, cmd: &str, rest: &[String]) -> Option<Out> {
    Some(match cmd {
        "aggregate" => aggregate::run(ctx, has(rest, "--json")),
        "pooled" => pooled::run(ctx, rest),
        "answers" => answers::run(&ctx.root, rest),
        "normalize-pi" => normalize_pi::run(rest),
        "agentic-score" => score::run(ctx, has(rest, "--json")),
        _ => return None,
    })
}

/// Version, help, or usage for anything else.
fn about(cmd: &str) -> Out {
    match cmd {
        "--version" | "-V" => Out {
            stdout: format!("puritanical-tools {}\n", env!("CARGO_PKG_VERSION")),
            ..Out::default()
        },
        "help" | "--help" | "-h" => Out {
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
