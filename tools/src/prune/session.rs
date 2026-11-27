// SPDX-License-Identifier: Apache-2.0

//! One `--apply` run: the baseline check, the bisecting search over the
//! cuts, and putting files back when the run stops.

use super::check::{self, Counts};
use super::edit::{self, Range};
use super::journal::{self, Journal};
use super::{Cut, Opts, git, git_bytes, io, safe_path, status};
use crate::Out;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::time::Duration;

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
    /// Files the check changed, recorded in the journal for `undo`.
    stopped: Vec<String>,
    /// `HEAD`'s tree when the run began, which the clean tree matched.
    tree: String,
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
        // Only changed files are parsed, and the parser's span table is freed
        // after, so memory does not grow with every trial.
        let parses = want
            .iter()
            .filter(|(p, t)| self.disk[*p] != **t)
            .all(|(_, t)| syn::parse_file(t).is_ok());
        proc_macro2::extra::invalidate_current_thread_spans();
        if !parses {
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
        let log = self.journal.log("check.log");
        let run = check::run(self.root, self.check, self.env, &log, self.limit);
        self.untouched()?;
        let run = run?;
        Ok(run.ok && check::holds(&self.base, &run.counts))
    }

    /// Stops the run when the check changed a file (one prune did not write,
    /// or one prune wrote whose bytes now differ) or removed or overwrote
    /// part of the journal.
    fn untouched(&mut self) -> Result<(), String> {
        // `true`: a file prune wrote. The journal's own files are judged by `intact`.
        let inside = format!("{}/", journal::DIR);
        let mut changed: Vec<(String, bool)> = status(self.root, self.env)?
            .into_iter()
            .filter(|p| !self.disk.contains_key(p) && !p.starts_with(&inside))
            .map(|p| (p, false))
            .collect();
        for (path, text) in &self.disk {
            let same = safe_path(self.root, path)
                .ok()
                .and_then(|p| std::fs::read(p).ok())
                .as_deref()
                == Some(text.as_bytes());
            if !same {
                changed.push((path.clone(), true));
            }
        }
        let lost = !self.journal.intact()
            || self
                .orig
                .iter()
                .any(|(p, t)| !self.journal.keeps(p, t.as_bytes(), self.disk[p].as_bytes()));
        if changed.is_empty() && !lost {
            return Ok(());
        }
        let mut names: Vec<&str> = changed.iter().map(|(p, _)| p.as_str()).collect();
        names.extend(lost.then_some(journal::DIR));
        let why = format!(
            "the check changed files itself, so the run stopped: {}",
            names.join(", ")
        );
        Err(with_record_err(why, self.stop(lost, &changed)))
    }

    /// Records in the journal, for each file the check changed, what the
    /// check left and the file's bytes before the run, so `undo` restores
    /// it. A journal the check damaged is rebuilt from memory first.
    fn stop(&mut self, lost: bool, changed: &[(String, bool)]) -> Result<(), String> {
        // Listed first, so a failure below never lets the journal go.
        self.stopped.extend(changed.iter().map(|(p, _)| p.clone()));
        record_stopped(
            (self.root, self.env, &self.tree),
            &mut self.journal,
            (&self.orig, &self.disk),
            lost,
            changed,
        )
    }

    /// After a failed run: puts back every file that still holds what prune
    /// wrote, and removes the journal when nothing else is left in it.
    fn abandon(&mut self) -> String {
        let back: Vec<(String, String)> = self
            .orig
            .iter()
            .filter(|(p, t)| !self.stopped.contains(p) && self.disk[*p] != **t)
            .map(|(p, t)| (p.clone(), t.clone()))
            .collect();
        let done = back
            .iter()
            .try_for_each(|(p, t)| self.journal.write(p, t.as_bytes()))
            .and_then(|()| {
                if self.stopped.is_empty() {
                    self.journal.discard()
                } else {
                    Ok(())
                }
            });
        match done {
            Err(e) => format!("; putting the files back failed: {e}; run `prune undo`"),
            Ok(()) if self.stopped.is_empty() => "; prune's edits were undone".into(),
            Ok(()) => {
                "; prune undid its own edits; `prune undo` puts back the files the check changed"
                    .into()
            }
        }
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

/// Original bytes of `path` before the run: prune's copy when `ours`, else
/// Git's checkout from `tree` (the index can hold the check's own staging),
/// or none if that tree lacks it.
fn orig_before(
    (root, env, tree): (&Path, &HashMap<String, String>, &str),
    orig: &BTreeMap<String, String>,
    path: &str,
    ours: bool,
) -> Result<Option<Vec<u8>>, String> {
    if ours {
        return Ok(Some(orig[path].clone().into_bytes()));
    }
    let spec = format!("{tree}:{path}");
    if git(root, env, &["cat-file", "-e", &spec]).is_ok() {
        git_bytes(root, env, &["cat-file", "--filters", &spec]).map(Some)
    } else {
        Ok(None)
    }
}

/// Records in `journal`, for each file the check changed, its bytes before
/// the run and what the check left, repairing a damaged journal first. When
/// a path passes through a symlink, its original (if tracked) is recorded as
/// absent so `undo` restores it once the link is removed.
fn record_stopped(
    repo: (&Path, &HashMap<String, String>, &str),
    journal: &mut Journal,
    maps: (&BTreeMap<String, String>, &BTreeMap<String, String>),
    lost: bool,
    changed: &[(String, bool)],
) -> Result<(), String> {
    if lost {
        journal.repair(maps.0, maps.1)?;
    }
    let mut first_err: Option<String> = None;
    for (path, ours) in changed {
        let orig = orig_before(repo, maps.0, path, *ours)?;
        match safe_path(repo.0, path) {
            Ok(full) => {
                let now = std::fs::read(&full).ok();
                journal.record(path, orig.as_deref(), now.as_deref())?;
            }
            Err(e) => {
                if let Some(bytes) = orig.as_deref() {
                    journal.record(path, Some(bytes), None)?;
                }
                first_err.get_or_insert(e);
            }
        }
    }
    first_err.map_or(Ok(()), Err)
}

fn with_record_err(msg: String, rec: Result<(), String>) -> String {
    match rec {
        Ok(()) => msg,
        Err(e) => format!("{msg}; recording them failed: {e}"),
    }
}

/// Records any files the baseline check changed into `journal`, returning
/// `(reason, keep_journal)` when the tree or journal was modified.
fn baseline_dirty(
    repo: (&Path, &HashMap<String, String>, &str),
    journal: &mut Journal,
) -> Result<Option<(String, bool)>, String> {
    let inside = format!("{}/", journal::DIR);
    let changed: Vec<(String, bool)> = status(repo.0, repo.1)?
        .into_iter()
        .filter(|p| !p.starts_with(&inside))
        .map(|p| (p, false))
        .collect();
    let lost = !journal.intact();
    let mut names: Vec<&str> = changed.iter().map(|(p, _)| p.as_str()).collect();
    names.extend(lost.then_some(journal::DIR));
    if names.is_empty() {
        return Ok(None);
    }
    let msg = format!(
        "after the baseline check, the working tree is not clean: {}",
        names.join(", ")
    );
    if changed.is_empty() {
        return Ok(Some((msg, false)));
    }
    let empty = BTreeMap::new();
    let rec = record_stopped(repo, journal, (&empty, &empty), lost, &changed);
    let msg = format!("{msg}; `prune undo` puts back the files the check changed");
    Ok(Some((with_record_err(msg, rec), true)))
}

/// Runs the check before any cut; refuses when it fails, changes the tree, or
/// prints no count above 0 without `--uncounted`. Returns `(err, keep_journal)`.
fn baseline(
    repo: (&Path, &HashMap<String, String>, &str),
    o: &Opts,
    journal: &mut Journal,
) -> Result<check::Run, (String, bool)> {
    let run = check::run(
        repo.0,
        &o.check,
        repo.1,
        &journal.log("baseline.log"),
        Duration::MAX,
    );
    // Read before any refusal, so what the check changed is named and journalled
    // even when the check removed the journal (and `baseline.log` with it).
    let dirty = baseline_dirty(repo, journal).map_err(|e| (e, false))?;
    let base = match run {
        Ok(b) => b,
        Err(e) => return Err(dirty.unwrap_or((e, false))),
    };
    if !base.ok {
        let (changed, keep) = dirty.map_or((String::new(), false), |(d, k)| (format!("; {d}"), k));
        let tail = if base.tail.is_empty() {
            String::new()
        } else {
            format!("; its output ends:\n{}", base.tail)
        };
        return Err((
            format!("the check fails before any cut{changed}{tail}"),
            keep,
        ));
    }
    if let Some(d) = dirty {
        return Err(d);
    }
    if base.counts.values().all(|&n| n == 0) && !o.uncounted {
        return Err((
            "the check prints no test count above 0 that prune can read; pass --uncounted to rely on its exit status alone".into(),
            false,
        ));
    }
    Ok(base)
}

/// Each cut file's text, refused when it differs from the text the finder
/// parsed (the baseline check regenerated it).
fn originals(
    root: &Path,
    cuts: &[Cut],
    texts: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, String> {
    let mut orig = BTreeMap::new();
    for c in cuts {
        let path = safe_path(root, &c.path)?;
        let text = std::fs::read_to_string(&path).map_err(io(&path))?;
        if texts.get(&c.path) != Some(&text) {
            return Err(format!("{}: changed after the finder read it", c.path));
        }
        orig.insert(c.path.clone(), text);
    }
    Ok(orig)
}

fn finish_apply(
    s: &mut Session<'_>,
    out: &mut Out,
    kept: &[usize],
    rejected: &[usize],
) -> Result<(), String> {
    s.set(kept)?;
    report(out, s.cuts, kept, rejected, s.runs);
    if kept.is_empty() {
        s.journal.discard()
    } else {
        s.journal.finish()
    }
}

pub(super) fn apply(
    root: &Path,
    env: &HashMap<String, String>,
    o: &Opts,
    (cuts, texts): (&[Cut], &BTreeMap<String, String>),
    mut out: Out,
) -> Result<Out, String> {
    let tree = git(root, env, &["rev-parse", "HEAD^{tree}"])?
        .trim()
        .to_string();
    let mut journal = Journal::create(root)?;
    let ready = baseline((root, env, &tree), o, &mut journal).and_then(|base| {
        originals(root, cuts, texts)
            .map(|orig| (base, orig))
            .map_err(|e| (e, false))
    });
    // Keep the journal when baseline mutations were recorded; clear it otherwise.
    let (base, orig) = match ready {
        Ok(r) => r,
        Err((e, true)) => return Err(e),
        Err((e, false)) => return journal::clear(root).and(Err(e)),
    };
    out.say(format!(
        "baseline: check passes ({})",
        check::describe(&base.counts)
    ));
    let mut s = Session {
        root,
        env,
        check: &o.check,
        cuts,
        journal,
        base: base.counts,
        limit: base.took * 3 + Duration::from_secs(60),
        disk: orig.clone(),
        orig,
        runs: 0,
        stopped: Vec::new(),
        tree,
    };
    let (kept, rejected) = match search(cuts.len(), &mut |a| s.pass(a)) {
        Ok(r) => r,
        Err(e) => return Err(format!("{e}{}", s.abandon())),
    };
    finish_apply(&mut s, &mut out, &kept, &rejected)?;
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
            stopped: Vec::new(),
            tree: String::new(),
        };
        assert_eq!((s.pass(&[0]), s.runs), (Ok(false), 0));
        assert_eq!(
            std::fs::read_to_string(d.path().join("a.rs")).expect("read"),
            "fn a() {}\n"
        );
    }
}
