// SPDX-License-Identifier: Apache-2.0

//! `prune`: removes the dead code a compiler proves dead, keeping each cut
//! only while the user's own check still passes, with an undo journal.

mod check;
mod edit;
pub mod journal;
mod rust;
mod session;

use crate::{Ctx, Out};
use edit::Range;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const USAGE: &str = "usage: puritanical-tools prune [--repo <dir>] [--apply] [--risky] [--uncounted] [-- <check command>...]
       puritanical-tools prune undo|keep [--repo <dir>]";

/// Formats an I/O error with the path it concerns.
pub(crate) fn io(what: &Path) -> impl Fn(std::io::Error) -> String + '_ {
    move |e| format!("{}: {e}", what.display())
}

/// Formats a failure to start `program`.
pub(crate) fn cannot(program: &str) -> impl Fn(std::io::Error) -> String + '_ {
    move |e| format!("cannot run {program}: {e}")
}

/// One deletion: the ranges of one item (and the impl blocks cut with it).
#[derive(Debug)]
pub struct Cut {
    pub path: String,
    pub ranges: Vec<Range>,
    pub kind: &'static str,
    pub name: String,
    pub line: usize,
    pub risky: Option<String>,
}

impl Cut {
    fn label(&self) -> String {
        let with = match self.ranges.len() {
            1 => String::new(),
            2 => " and 1 impl block".into(),
            n => format!(" and {} impl blocks", n - 1),
        };
        format!(
            "{}:{} {} `{}`{with}",
            self.path, self.line, self.kind, self.name
        )
    }
}

/// Drops each cut that lies wholly inside another cut of the same file.
#[must_use]
pub fn outermost(cuts: Vec<Cut>) -> Vec<Cut> {
    let inside = |c: &Cut, o: &Cut| {
        c.path == o.path
            && c.ranges
                .iter()
                .all(|&(s, e)| o.ranges.iter().any(|&(os, oe)| os <= s && e <= oe))
    };
    let keep: Vec<bool> = cuts
        .iter()
        .enumerate()
        .map(|(i, c)| !cuts.iter().enumerate().any(|(j, o)| i != j && inside(c, o)))
        .collect();
    cuts.into_iter()
        .zip(keep)
        .filter_map(|(c, k)| k.then_some(c))
        .collect()
}

/// `rel` (a `/`- or `\`-separated path) inside `root`, refused when it
/// leaves the root, names `.git` or passes through a symbolic link.
///
/// # Errors
/// The path is not a plain path inside the root.
pub fn safe_path(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let mut path = root.to_path_buf();
    for part in rel.split(['/', '\\']) {
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.contains(':')
            || part.eq_ignore_ascii_case(".git")
        {
            return Err(format!("{rel}: not a plain path inside the repository"));
        }
        path.push(part);
        if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(format!("{rel}: passes through a symbolic link"));
        }
    }
    Ok(path)
}

#[derive(Default)]
struct Opts {
    repo: Option<PathBuf>,
    apply: bool,
    risky: bool,
    uncounted: bool,
    check: Vec<String>,
}

/// The subcommand (`undo`, `keep` or empty) and options; `undo` and `keep`
/// may come before or after `--repo <dir>`.
fn parse(mut rest: &[String]) -> Option<(&str, Opts)> {
    let mut sub = "";
    let mut o = Opts::default();
    while let Some((a, tail)) = rest.split_first() {
        match a.as_str() {
            "--repo" => o.repo = Some(PathBuf::from(tail.first()?)),
            s @ ("undo" | "keep") if sub.is_empty() => sub = s,
            "--apply" => o.apply = true,
            "--risky" => o.risky = true,
            "--uncounted" => o.uncounted = true,
            "--" if sub.is_empty() => {
                o.check = tail.to_vec();
                break;
            }
            _ => return None,
        }
        rest = if a == "--repo" { &tail[1..] } else { tail };
    }
    let ok = if sub.is_empty() {
        o.check.is_empty() != o.apply
    } else {
        !(o.apply || o.risky || o.uncounted)
    };
    ok.then_some((sub, o))
}

/// Runs `prune` with the arguments after the command name.
#[must_use]
pub fn run(ctx: &Ctx, rest: &[String]) -> Out {
    let Some((sub, o)) = parse(rest) else {
        return Out {
            stderr: format!("{USAGE}\n"),
            code: 2,
            ..Out::default()
        };
    };
    let done = root(&o, &ctx.env).and_then(|root| match sub {
        "undo" => journal::undo(&root).map(|n| {
            say(format!(
                "restored {n} file(s) to their bytes before the run"
            ))
        }),
        "keep" => journal::keep(&root).map(|()| say("kept the cuts; journal removed")),
        _ => prune(&root, &ctx.env, &o),
    });
    done.unwrap_or_else(|e| Out::fail(format!("prune: {e}")))
}

fn say(line: impl AsRef<str>) -> Out {
    let mut out = Out::default();
    out.say(line);
    out
}

fn git(root: &Path, env: &HashMap<String, String>, args: &[&str]) -> Result<String, String> {
    git_bytes(root, env, args).map(|b| String::from_utf8_lossy(&b).into_owned())
}

fn git_bytes(root: &Path, env: &HashMap<String, String>, args: &[&str]) -> Result<Vec<u8>, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .env_clear()
        .envs(env)
        .output()
        .map_err(cannot("git"))?;
    if !out.status.success() {
        return Err(format!(
            "`git {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(out.stdout)
}

/// The repository root: `--repo` or the current folder, which must be the
/// top of a Git work tree.
fn root(o: &Opts, env: &HashMap<String, String>) -> Result<PathBuf, String> {
    let dir = o
        .repo
        .clone()
        .map_or_else(std::env::current_dir, Ok)
        .map_err(io(Path::new(".")))?;
    let dir = dir.canonicalize().map_err(io(&dir))?;
    let top = git(&dir, env, &["rev-parse", "--show-toplevel"])?;
    if Path::new(top.trim()).canonicalize().ok().as_deref() != Some(&dir) {
        return Err(format!(
            "{} is not the top folder of a Git work tree",
            dir.display()
        ));
    }
    Ok(dir)
}

/// Paths `git status` lists, untracked files included; both paths of a
/// rename or copy.
fn status(root: &Path, env: &HashMap<String, String>) -> Result<Vec<String>, String> {
    let out = git(
        root,
        env,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    let mut paths = Vec::new();
    // Only the trailing field is empty; a rename's original path, which
    // follows its entry bare, can be 1 byte.
    let mut entries = out.split('\0').filter(|e| !e.is_empty());
    while let Some(e) = entries.next() {
        paths.push(e[3..].to_string());
        // `R` or `C` in either column: a staged or a work-tree rename or copy.
        if e[..2].contains(['R', 'C']) {
            paths.extend(entries.next().map(str::to_string));
        }
    }
    Ok(paths)
}

fn clean(root: &Path, env: &HashMap<String, String>, when: &str) -> Result<(), String> {
    let changed = status(root, env)?;
    if changed.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{when}, the working tree is not clean: {}",
        changed.join(", ")
    ))
}

fn prune(root: &Path, env: &HashMap<String, String>, o: &Opts) -> Result<Out, String> {
    if journal::exists(root) {
        return Err(format!(
            "a journal from an earlier run is in {}: run `prune undo` or `prune keep` first",
            journal::DIR
        ));
    }
    if o.apply {
        clean(root, env, "before the run")?;
    }
    if !root.join("Cargo.toml").is_file() {
        return Ok(say(
            "no finder for this repository: prune finds dead Rust code in a Cargo workspace",
        ));
    }
    let mut found = rust::find(root, env)?;
    untracked_risky(root, env, &mut found.cuts)?;
    let mut out = Out::default();
    for n in &found.notes {
        out.say(format!("note: {n}"));
    }
    let (cuts, held): (Vec<Cut>, Vec<Cut>) = found
        .cuts
        .into_iter()
        .partition(|c| o.risky || c.risky.is_none());
    for c in &held {
        out.say(format!(
            "{} (risky: {}; cut only with --risky)",
            c.label(),
            c.risky.as_deref().unwrap_or_default()
        ));
    }
    if !o.apply {
        for c in &cuts {
            out.say(c.label());
        }
        out.say(format!(
            "{} candidate(s), {} risky. Dry run: nothing changed. To cut: puritanical-tools prune --apply -- <check command>",
            cuts.len() + held.len(),
            held.len()
        ));
        return Ok(out);
    }
    if cuts.is_empty() {
        out.say("nothing to cut");
        return Ok(out);
    }
    clean(root, env, "after the finder's build")?;
    session::apply(root, env, o, (&cuts, &found.texts), out)
}

/// Makes risky each cut in a file Git does not track: Git holds no copy of
/// it and `git diff` does not show its cuts.
fn untracked_risky(
    root: &Path,
    env: &HashMap<String, String>,
    cuts: &mut [Cut],
) -> Result<(), String> {
    let tracked = git(root, env, &["ls-files", "-z"])?;
    let tracked: Vec<&str> = tracked.split('\0').collect();
    for c in cuts {
        if !tracked.contains(&c.path.as_str()) {
            c.risky.get_or_insert_with(|| "not tracked by Git".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn parses_the_subcommands_and_refuses_the_rest() {
        let all = args(&[
            "--repo",
            "r",
            "--apply",
            "--risky",
            "--uncounted",
            "--",
            "cargo",
            "test",
        ]);
        let (sub, o) = parse(&all).expect("ok");
        assert_eq!((sub, o.apply, o.risky, o.uncounted), ("", true, true, true));
        assert_eq!(
            (o.repo, o.check),
            (Some(PathBuf::from("r")), args(&["cargo", "test"]))
        );
        assert_eq!(
            parse(&args(&["undo", "--repo", "r"])).expect("ok").0,
            "undo"
        );
        // The order the docs give: the subcommand after `--repo <dir>`.
        let keep = args(&["--repo", "r", "keep"]);
        let (sub, o) = parse(&keep).expect("ok");
        assert_eq!((sub, o.repo), ("keep", Some(PathBuf::from("r"))));
        assert_eq!(parse(&args(&[])).expect("ok").0, "");
        for bad in [
            &["--apply"][..],
            &["--", "x"],
            &["undo", "--apply"],
            &["--repo"],
            &["--frob"],
            &["keep", "--", "x"],
            &["--repo", "r", "undo", "--risky"],
            &["undo", "--uncounted"],
            &["undo", "keep"],
        ] {
            assert!(parse(&args(bad)).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn nested_cuts_and_labels() {
        let cut = |path: &str, ranges: Vec<Range>| Cut {
            path: path.into(),
            ranges,
            kind: "struct",
            name: "S".into(),
            line: 3,
            risky: None,
        };
        let kept = outermost(vec![
            cut("a.rs", vec![(0, 10), (20, 40)]),
            cut("a.rs", vec![(25, 30)]),
            cut("b.rs", vec![(25, 30)]),
            cut("a.rs", vec![(0, 10)]),
        ]);
        let labels: Vec<_> = kept.iter().map(Cut::label).collect();
        assert_eq!(
            labels,
            ["a.rs:3 struct `S` and 1 impl block", "b.rs:3 struct `S`"]
        );
        assert_eq!(
            cut("c.rs", vec![(0, 1), (2, 3), (4, 5)]).label(),
            "c.rs:3 struct `S` and 2 impl blocks"
        );
    }

    #[test]
    fn paths_stay_inside_the_root() {
        let root = Path::new("r");
        assert_eq!(
            safe_path(root, "src/a.rs").expect("ok"),
            Path::new("r").join("src").join("a.rs")
        );
        for bad in [
            "../x",
            "/etc/x",
            "C:\\x",
            "src/.git/config",
            ".GIT/x",
            "a//b",
            "./a",
        ] {
            assert!(safe_path(root, bad).is_err(), "{bad}");
        }
    }
}
