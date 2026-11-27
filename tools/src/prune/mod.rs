// SPDX-License-Identifier: Apache-2.0

//! `prune`: removes the dead code a compiler proves dead, keeping each cut
//! only while the user's own check still passes, with an undo journal.

mod check;
mod edit;
pub mod journal;
mod rust;

use crate::{Ctx, Out};
use check::Counts;
use edit::Range;
use journal::Journal;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

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

fn parse(rest: &[String]) -> Option<(&str, Opts)> {
    let (sub, mut rest) = match rest.first().map(String::as_str) {
        Some(s @ ("undo" | "keep")) => (s, &rest[1..]),
        _ => ("", rest),
    };
    let mut o = Opts::default();
    while let Some((a, tail)) = rest.split_first() {
        match a.as_str() {
            "--repo" => o.repo = Some(PathBuf::from(tail.first()?)),
            "--apply" if sub.is_empty() => o.apply = true,
            "--risky" if sub.is_empty() => o.risky = true,
            "--uncounted" if sub.is_empty() => o.uncounted = true,
            "--" if sub.is_empty() => {
                o.check = tail.to_vec();
                break;
            }
            _ => return None,
        }
        rest = if a == "--repo" { &tail[1..] } else { tail };
    }
    (o.check.is_empty() != o.apply).then_some((sub, o))
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
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
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

/// Paths `git status` lists, untracked files included.
fn status(root: &Path, env: &HashMap<String, String>) -> Result<Vec<String>, String> {
    let out = git(
        root,
        env,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    let mut paths = Vec::new();
    let mut entries = out.split('\0').filter(|e| e.len() > 3);
    while let Some(e) = entries.next() {
        if e.starts_with(['R', 'C']) {
            entries.next();
        }
        paths.push(e[3..].to_string());
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
    let found = rust::find(root, env)?;
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
    apply(root, env, o, &cuts, out)
}

/// The check, the baseline it must hold to, and the files being cut.
struct Session<'a> {
    root: &'a Path,
    env: &'a HashMap<String, String>,
    check: &'a [String],
    cuts: &'a [Cut],
    journal: Journal,
    base: Counts,
    limit: Duration,
    orig: BTreeMap<String, String>,
    disk: BTreeMap<String, String>,
    runs: usize,
}

impl Session<'_> {
    /// Writes each file as its original minus the `active` cuts. False, with
    /// nothing written, when a result would not parse.
    fn set(&mut self, active: &[usize]) -> Result<bool, String> {
        let mut want = BTreeMap::new();
        for (path, text) in &self.orig {
            let ranges: Vec<Range> = active
                .iter()
                .filter(|&&i| self.cuts[i].path == *path)
                .flat_map(|&i| self.cuts[i].ranges.iter().copied())
                .collect();
            want.insert(path.clone(), edit::apply(text, &ranges));
        }
        if want.values().any(|t| syn::parse_file(t).is_err()) {
            return Ok(false);
        }
        for (path, text) in want {
            if self.disk[&path] != text {
                self.journal.write(&path, text.as_bytes())?;
                self.disk.insert(path, text);
            }
        }
        Ok(true)
    }

    /// Whether the check passes with the `active` cuts.
    fn pass(&mut self, active: &[usize]) -> Result<bool, String> {
        if !self.set(active)? {
            return Ok(false);
        }
        self.runs += 1;
        let log = journal::log_dir(self.root).join("check.log");
        let run = check::run(self.root, self.check, self.env, &log, self.limit)?;
        self.untouched()?;
        Ok(run.ok && check::holds(&self.base, &run.counts))
    }

    /// Refuses when the check changed a file: one prune did not write, or one
    /// prune wrote whose bytes now differ.
    fn untouched(&self) -> Result<(), String> {
        let mut changed: Vec<String> = status(self.root, self.env)?
            .into_iter()
            .filter(|p| !self.disk.contains_key(p))
            .collect();
        for (path, text) in &self.disk {
            if std::fs::read(safe_path(self.root, path)?).ok().as_deref() != Some(text.as_bytes()) {
                changed.push(path.clone());
            }
        }
        if changed.is_empty() {
            return Ok(());
        }
        Err(format!(
            "the check changed files itself, so the run stopped: {}",
            changed.join(", ")
        ))
    }
}

/// Tries `batch` on top of `kept`; on failure splits it in halves until each
/// failing cut stands alone.
fn bisect(
    batch: &[usize],
    kept: &mut Vec<usize>,
    rejected: &mut Vec<usize>,
    pass: &mut dyn FnMut(&[usize]) -> Result<bool, String>,
) -> Result<(), String> {
    let trial: Vec<usize> = kept.iter().chain(batch).copied().collect();
    if pass(&trial)? {
        kept.extend_from_slice(batch);
        return Ok(());
    }
    if let [one] = batch {
        rejected.push(*one);
        return Ok(());
    }
    let (a, b) = batch.split_at(batch.len() / 2);
    bisect(a, kept, rejected, pass)?;
    bisect(b, kept, rejected, pass)
}

/// Cuts `0..n`, retrying the rejected ones after any pass that kept a cut.
/// Returns the kept and rejected indices.
fn search(
    n: usize,
    pass: &mut dyn FnMut(&[usize]) -> Result<bool, String>,
) -> Result<(Vec<usize>, Vec<usize>), String> {
    let mut kept = Vec::new();
    let mut pending: Vec<usize> = (0..n).collect();
    loop {
        let before = kept.len();
        let mut rejected = Vec::new();
        bisect(&pending, &mut kept, &mut rejected, pass)?;
        if rejected.is_empty() || kept.len() == before {
            return Ok((kept, rejected));
        }
        pending = rejected;
    }
}

/// Runs the check before any cut; refuses (removing the new journal) when it
/// fails, prints no count without `--uncounted`, or changes the tree.
fn baseline(
    root: &Path,
    env: &HashMap<String, String>,
    o: &Opts,
    journal: Journal,
) -> Result<(check::Run, Journal), String> {
    let base = check::run(
        root,
        &o.check,
        env,
        &journal::log_dir(root).join("baseline.log"),
        Duration::MAX,
    )?;
    let refuse = if !base.ok {
        Some("the check fails before any cut".to_string())
    } else if base.counts.is_empty() && !o.uncounted {
        Some("the check prints no test count prune can read; pass --uncounted to rely on its exit status alone".into())
    } else {
        clean(root, env, "after the baseline check").err()
    };
    match refuse {
        Some(why) => journal.discard().and(Err(why)),
        None => Ok((base, journal)),
    }
}

fn apply(
    root: &Path,
    env: &HashMap<String, String>,
    o: &Opts,
    cuts: &[Cut],
    mut out: Out,
) -> Result<Out, String> {
    let (base, journal) = baseline(root, env, o, Journal::create(root)?)?;
    out.say(format!(
        "baseline: check passes ({})",
        check::describe(&base.counts)
    ));
    let mut orig = BTreeMap::new();
    for c in cuts {
        let path = safe_path(root, &c.path)?;
        let text = std::fs::read_to_string(&path).map_err(io(&path))?;
        orig.insert(c.path.clone(), text);
    }
    let limit = base.took * 3 + Duration::from_secs(60);
    let disk = orig.clone();
    let mut s = Session {
        root,
        env,
        check: &o.check,
        cuts,
        journal,
        base: base.counts,
        limit,
        orig,
        disk,
        runs: 0,
    };
    let (kept, rejected) = match search(cuts.len(), &mut |a| s.pass(a)) {
        Ok(r) => r,
        Err(e) => {
            let undone = journal::undo(root).map_or_else(
                |u| format!("; undo failed: {u}"),
                |_| "; prune's edits were undone".into(),
            );
            return Err(format!("{e}{undone}"));
        }
    };
    s.set(&kept)?;
    report(&mut out, cuts, &kept, &rejected, s.runs);
    if kept.is_empty() {
        s.journal.discard()?;
    } else {
        s.journal.finish()?;
    }
    Ok(out)
}

fn report(out: &mut Out, cuts: &[Cut], kept: &[usize], rejected: &[usize], runs: usize) {
    let mut kept = kept.to_vec();
    kept.sort_unstable();
    for &i in &kept {
        out.say(format!("cut {}", cuts[i].label()));
    }
    for &i in rejected {
        out.say(format!(
            "kept {} (the check failed without it)",
            cuts[i].label()
        ));
    }
    let next = if kept.is_empty() {
        "nothing changed"
    } else {
        "review with `git diff`; `prune undo` restores, `prune keep` accepts"
    };
    out.say(format!(
        "{} of {} cut after {runs} check run(s); {next}",
        kept.len(),
        cuts.len()
    ));
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
        assert_eq!(parse(&args(&[])).expect("ok").0, "");
        for bad in [
            &["--apply"][..],
            &["--", "x"],
            &["undo", "--apply"],
            &["--repo"],
            &["--frob"],
            &["keep", "--", "x"],
        ] {
            assert!(parse(&args(bad)).is_none(), "{bad:?}");
        }
    }

    /// `pass` that fails while any of `bad` is active, or when `needs` is
    /// active without the cut it depends on.
    fn oracle<'a>(
        bad: &'static [usize],
        needs: Option<(usize, usize)>,
        runs: &'a mut usize,
    ) -> impl FnMut(&[usize]) -> Result<bool, String> + 'a {
        move |a| {
            *runs += 1;
            let dep = needs.is_some_and(|(x, y)| a.contains(&x) && !a.contains(&y));
            Ok(!dep && !a.iter().any(|i| bad.contains(i)))
        }
    }

    #[test]
    fn bisection_rejects_only_the_cut_that_breaks_the_check() {
        let mut runs = 0;
        let (kept, rejected) = search(4, &mut oracle(&[2], None, &mut runs)).expect("search");
        assert_eq!((kept, rejected), (vec![0, 1, 3], vec![2]));
        // all 4, then [0,1] passes, [2,3] fails, [2] fails, [3] passes; then 2 again.
        assert_eq!(runs, 6);
    }

    #[test]
    fn a_cut_rejected_for_order_is_retried() {
        let mut runs = 0;
        // Cut 0 (a struct) fails whilst cut 1 (its only user) stays, and cut 2
        // always fails, so the first pass splits 0 from 1.
        let (mut kept, rejected) =
            search(3, &mut oracle(&[2], Some((0, 1)), &mut runs)).expect("search");
        kept.sort_unstable();
        assert_eq!((kept, rejected, runs), (vec![0, 1], vec![2], 9));
        let mut fails = |_: &[usize]| Err::<bool, String>("stop".into());
        assert_eq!(search(1, &mut fails).unwrap_err(), "stop");
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
    fn a_cut_set_that_would_not_parse_writes_nothing_and_runs_no_check() {
        let d = tempfile::tempdir().expect("temp dir");
        std::fs::write(d.path().join("a.rs"), "fn a() {}\n").expect("write");
        let cuts = [Cut {
            path: "a.rs".into(),
            ranges: vec![(0, 4)],
            kind: "function",
            name: "a".into(),
            line: 1,
            risky: None,
        }];
        let orig = BTreeMap::from([("a.rs".to_string(), "fn a() {}\n".to_string())]);
        let env = HashMap::new();
        let journal = Journal::create(d.path()).expect("journal");
        let mut s = Session {
            root: d.path(),
            env: &env,
            check: &[],
            cuts: &cuts,
            journal,
            base: Counts::new(),
            limit: Duration::ZERO,
            disk: orig.clone(),
            orig,
            runs: 0,
        };
        assert_eq!((s.pass(&[0]), s.runs), (Ok(false), 0));
        assert_eq!(
            std::fs::read_to_string(d.path().join("a.rs")).expect("read"),
            "fn a() {}\n"
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
