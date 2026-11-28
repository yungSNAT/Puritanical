// SPDX-License-Identifier: Apache-2.0

//! The undo journal: each file's original bytes are on disk before prune
//! first edits it, and every write is an atomic replace, so `undo` can
//! restore the original bytes after a finished, failed or crashed run.

use super::{io, safe_path};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

/// The journal folder, inside the repository and ignored by Git.
pub const DIR: &str = ".puritanical-prune";

/// An open journal for the current run.
pub struct Journal {
    root: PathBuf,
    dir: PathBuf,
    files: Vec<String>,
    /// Entries whose file did not exist before the run (the check made it).
    created: Vec<usize>,
    /// Entries whose file the check removed.
    deleted: Vec<usize>,
}

/// Whether `path` is a plain folder (never following a symbolic link).
fn is_plain_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_dir())
}

/// Reads `path` only when it is a plain file (never following a symbolic link).
fn read_plain(path: &Path) -> Option<Vec<u8>> {
    fs::symlink_metadata(path)
        .ok()
        .filter(fs::Metadata::is_file)?;
    fs::read(path).ok()
}

/// Refuses `path` when it exists and is not a plain folder (e.g. a symlink).
fn plain_dir(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(m) if !m.is_dir() => Err(format!(
            "{} is not a plain folder; prune will not use it",
            path.display()
        )),
        _ => Ok(()),
    }
}

/// Refuses `path` when it exists and is not a plain file (e.g. a symlink).
fn plain_file(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(m) if !m.is_file() => Err(format!(
            "{} is not a plain file; prune will not use it",
            path.display()
        )),
        _ => Ok(()),
    }
}

/// Unlinks a file or symbolic link entry without following it.
fn remove_entry(path: &Path, m: &fs::Metadata) -> Result<(), String> {
    #[cfg(windows)]
    if m.file_type().is_symlink() && fs::remove_dir(path).is_ok() {
        return Ok(());
    }
    let _ = m;
    fs::remove_file(path).map_err(io(path))
}

/// Ensures `dir` is a plain folder; if a check replaced it with a symlink or
/// file, unlinks that entry without following it and creates a plain folder.
fn ensure_plain_dir(dir: &Path) -> Result<(), String> {
    match fs::symlink_metadata(dir) {
        Ok(m) if m.is_dir() => Ok(()),
        Ok(m) => {
            remove_entry(dir, &m)?;
            fs::create_dir(dir).map_err(io(dir))
        }
        Err(_) => fs::create_dir(dir).map_err(io(dir)),
    }
}

/// Writes `bytes` to a brand-new plain file and flushes it to disk (unlinking
/// any pre-existing file or symlink first so a planted symlink is never followed).
fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Ok(m) = fs::symlink_metadata(path) {
        remove_entry(path, &m)?;
    }
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io(path))?;
    f.write_all(bytes)
        .and_then(|()| f.sync_all())
        .map_err(io(path))
}

/// Flushes a directory entry change (a rename) to disk where the OS allows it.
#[cfg_attr(not(unix), allow(clippy::unnecessary_wraps))]
fn sync_dir(dir: &Path) -> Result<(), String> {
    #[cfg(unix)]
    fs::File::open(dir)
        .and_then(|d| d.sync_all())
        .map_err(io(dir))?;
    let _ = dir;
    Ok(())
}

/// The temporary sibling an atomic replace of `path` writes first.
fn temp_of(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    path.with_file_name(format!(".{name}.puritanical-prune.tmp"))
}

/// Replaces (or creates) `path` with `bytes` atomically, keeping its
/// permissions: the file holds either the old or the new bytes at every moment.
/// Never follows a symlink at `path` or its temp sibling.
fn replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let meta = fs::symlink_metadata(path).ok();
    let perms = meta
        .as_ref()
        .filter(|m| m.is_file())
        .map(fs::Metadata::permissions);
    let tmp = temp_of(path);
    write_synced(&tmp, bytes)?;
    if let Some(p) = perms {
        fs::set_permissions(&tmp, p).map_err(io(&tmp))?;
    }
    if let Some(m) = meta.filter(|m| m.file_type().is_symlink()) {
        remove_entry(path, &m)?;
    }
    fs::rename(&tmp, path).map_err(io(path))?;
    sync_dir(path.parent().unwrap_or(Path::new(".")))
}

/// The journal folder of `root`, refused when it or its subfolders are not plain folders.
fn dir_of(root: &Path) -> Result<PathBuf, String> {
    let dir = root.join(DIR);
    plain_dir(&dir)?;
    for sub in ["orig", "wrote"] {
        plain_dir(&dir.join(sub))?;
    }
    Ok(dir)
}

/// Whether `root` holds a journal from an earlier run.
#[must_use]
pub fn exists(root: &Path) -> bool {
    fs::symlink_metadata(root.join(DIR)).is_ok()
}

impl Journal {
    /// Starts a journal for a new run; refuses when one exists.
    ///
    /// # Errors
    /// A journal exists or the folder cannot be written.
    pub fn create(root: &Path) -> Result<Self, String> {
        let dir = dir_of(root)?;
        fs::create_dir(&dir).map_err(io(&dir))?;
        for sub in ["orig", "wrote"] {
            fs::create_dir(dir.join(sub)).map_err(io(&dir))?;
        }
        write_synced(&dir.join(".gitignore"), b"*\n")?;
        sync_dir(&dir)?;
        sync_dir(root)?;
        let j = Self {
            root: root.to_path_buf(),
            dir,
            files: Vec::new(),
            created: Vec::new(),
            deleted: Vec::new(),
        };
        j.save(false)?;
        Ok(j)
    }

    /// The exact JSON expected on disk for `journal.json`.
    fn index_json(&self, complete: bool) -> String {
        json!({
            "complete": complete,
            "files": self.files,
            "created": self.created,
            "deleted": self.deleted,
        })
        .to_string()
    }

    /// Whether the journal's folders, `.gitignore` and `journal.json` are still
    /// plain entries on disk holding the exact expected bytes.
    #[must_use]
    pub fn intact(&self) -> bool {
        is_plain_dir(&self.dir)
            && is_plain_dir(&self.dir.join("orig"))
            && is_plain_dir(&self.dir.join("wrote"))
            && read_plain(&self.dir.join(".gitignore")).as_deref() == Some(b"*\n")
            && read_plain(&self.dir.join("journal.json")).as_deref()
                == Some(self.index_json(false).as_bytes())
    }

    /// Whether the journal's copies of `rel` are plain files still holding its
    /// bytes before the run, `orig`, and what prune last wrote, `now` (true
    /// when it has no entry for `rel`): a check may overwrite or symlink them.
    #[must_use]
    pub fn keeps(&self, rel: &str, orig: &[u8], now: &[u8]) -> bool {
        self.files.iter().position(|f| f == rel).is_none_or(|n| {
            let holds = |sub: &str, want: &[u8]| {
                read_plain(&self.dir.join(sub).join(n.to_string())).as_deref() == Some(want)
            };
            holds("orig", orig) && holds("wrote", now)
        })
    }

    /// A path in the journal folder for a check log.
    #[must_use]
    pub fn log(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// Replaces `journal.json` atomically without following a symlink.
    fn save(&self, complete: bool) -> Result<(), String> {
        replace(
            &self.dir.join("journal.json"),
            self.index_json(complete).as_bytes(),
        )
    }

    fn write_orig(&self, n: usize, bytes: &[u8]) -> Result<(), String> {
        let copy = self.dir.join("orig").join(n.to_string());
        write_synced(&copy, bytes)?;
        sync_dir(&self.dir.join("orig"))
    }

    fn write_wrote(&self, n: usize, bytes: &[u8]) -> Result<(), String> {
        let wrote = self.dir.join("wrote");
        let next = wrote.join(format!("{n}.new"));
        write_synced(&next, bytes)?;
        fs::rename(&next, wrote.join(n.to_string())).map_err(io(&next))?;
        sync_dir(&wrote)
    }

    fn record_now(&mut self, n: usize, now: Option<&[u8]>) -> Result<(), String> {
        if let Some(bytes) = now {
            self.write_wrote(n, bytes)
        } else {
            self.deleted.push(n);
            self.save(false)
        }
    }

    /// Records a file a check changed: its bytes before the run (`orig`, `None`
    /// if the check created it) and what the check left (`now`, `None` if
    /// removed or replaced by a symlink). For a new entry, both copies hit disk
    /// before `journal.json` publishes it.
    ///
    /// # Errors
    /// The journal cannot be written.
    pub fn record(
        &mut self,
        rel: &str,
        orig: Option<&[u8]>,
        now: Option<&[u8]>,
    ) -> Result<(), String> {
        if let Some(n) = self.files.iter().position(|f| f == rel) {
            return self.record_now(n, now);
        }
        let n = self.files.len();
        if let Some(bytes) = orig {
            self.write_orig(n, bytes)?;
        } else {
            self.created.push(n);
        }
        if let Some(bytes) = now {
            self.write_wrote(n, bytes)?;
        } else {
            self.deleted.push(n);
        }
        self.files.push(rel.to_string());
        self.save(false)
    }

    /// Writes `bytes` to the repository file `rel`, journalling its original
    /// bytes first when this run has not touched it before.
    ///
    /// # Errors
    /// The path is unsafe or a write fails.
    pub fn write(&mut self, rel: &str, bytes: &[u8]) -> Result<(), String> {
        let path = safe_path(&self.root, rel)?;
        let n = if let Some(n) = self.files.iter().position(|f| f == rel) {
            n
        } else {
            let orig = fs::read(&path).map_err(io(&path))?;
            let n = self.files.len();
            self.write_orig(n, &orig)?;
            self.files.push(rel.to_string());
            self.save(false)?;
            n
        };
        let wrote = self.dir.join("wrote");
        let next = wrote.join(format!("{n}.new"));
        write_synced(&next, bytes)?;
        replace(&path, bytes)?;
        fs::rename(&next, wrote.join(n.to_string())).map_err(io(&next))?;
        sync_dir(&wrote)
    }

    /// Writes again, in place, every part of the journal a check removed,
    /// overwrote or replaced with a symlink: its folders, `.gitignore`, each
    /// entry's bytes before the run (from `orig`) and what prune wrote (from
    /// `now`), then the index. Never follows a symlink and removes no plain
    /// folder first, so a crash part way loses no intact copy.
    ///
    /// # Errors
    /// The journal cannot be written.
    pub fn repair(
        &self,
        orig: &BTreeMap<String, String>,
        now: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        ensure_plain_dir(&self.dir)?;
        for sub in ["orig", "wrote"] {
            ensure_plain_dir(&self.dir.join(sub))?;
        }
        sync_dir(&self.dir)?;
        sync_dir(&self.root)?;
        replace(&self.dir.join(".gitignore"), b"*\n")?;
        for (n, rel) in self.files.iter().enumerate() {
            let at = |sub: &str| self.dir.join(sub).join(n.to_string());
            replace(&at("orig"), orig[rel].as_bytes())?;
            replace(&at("wrote"), now[rel].as_bytes())?;
        }
        self.save(false)
    }

    /// Marks the run finished.
    ///
    /// # Errors
    /// The journal cannot be written.
    pub fn finish(&self) -> Result<(), String> {
        self.save(true)
    }

    /// Removes the journal folder (nothing left to undo).
    ///
    /// # Errors
    /// The folder cannot be removed.
    pub fn discard(&self) -> Result<(), String> {
        remove(&self.dir)
    }
}

/// Removes the journal folder `dir` and flushes the removal to disk.
fn remove(dir: &Path) -> Result<(), String> {
    fs::remove_dir_all(dir).map_err(io(dir))?;
    sync_dir(dir.parent().unwrap_or(Path::new(".")))
}

/// What `journal.json` lists.
#[derive(Default)]
struct Entries {
    files: Vec<String>,
    created: Vec<usize>,
    deleted: Vec<usize>,
    complete: bool,
}

/// The entries of the journal in `dir`. A folder with no `journal.json` and
/// no original copied yet (a crash as the run began) lists nothing.
fn listed(dir: &Path) -> Result<Entries, String> {
    let path = dir.join("journal.json");
    plain_file(&path)?;
    let none_copied = || fs::read_dir(dir.join("orig")).map_or(true, |mut d| d.next().is_none());
    let text = match fs::read_to_string(&path) {
        Err(e) if e.kind() == ErrorKind::NotFound && none_copied() => return Ok(Entries::default()),
        r => r.map_err(io(&path))?,
    };
    let bad = || format!("{}: not a prune journal", path.display());
    let v: Value = serde_json::from_str(&text).map_err(|_| bad())?;
    let files = v["files"]
        .as_array()
        .ok_or_else(bad)?
        .iter()
        .map(|f| f.as_str().map(str::to_string).ok_or_else(bad))
        .collect::<Result<_, _>>()?;
    let nums = |k: &str| -> Vec<usize> {
        let all = v[k].as_array().map_or(&[][..], Vec::as_slice);
        all.iter()
            .filter_map(Value::as_u64)
            .map(|n| n as usize)
            .collect()
    };
    Ok(Entries {
        files,
        created: nums("created"),
        deleted: nums("deleted"),
        complete: v["complete"] == true,
    })
}

/// Entry `n`'s bytes before the run, `None` when the file did not exist.
fn original(dir: &Path, e: &Entries, n: usize) -> Result<Option<Vec<u8>>, String> {
    if e.created.contains(&n) {
        return Ok(None);
    }
    let path = dir.join("orig").join(n.to_string());
    plain_file(&path)?;
    fs::read(&path).map(Some).map_err(io(&path))
}

/// Restores every file the journal lists to its bytes before the run (or
/// removes it when it did not exist), then removes the journal. Refuses,
/// changing nothing, when a file holds bytes neither prune nor its stopped
/// check left (edited since). Returns the number of files restored.
///
/// # Errors
/// No journal, a file edited since, or an I/O failure.
pub fn undo(root: &Path) -> Result<usize, String> {
    let dir = dir_of(root)?;
    if !exists(root) {
        return Err("nothing to undo: no prune journal in this repository".into());
    }
    let e = listed(&dir)?;
    let mut plan = Vec::new();
    let mut edited = Vec::new();
    for (n, rel) in e.files.iter().enumerate() {
        let path = safe_path(root, rel)?;
        let orig = original(&dir, &e, n)?;
        if ours(&dir, &e, n, orig.as_deref(), &path) {
            plan.push((n, path, orig));
        } else {
            edited.push(rel.as_str());
        }
    }
    if !edited.is_empty() {
        return Err(format!(
            "undo refused, nothing changed: edited since prune wrote them: {}",
            edited.join(", ")
        ));
    }
    // Remove check-created files first, so a file that replaced a tracked
    // directory is gone before its children are restored.
    plan.sort_by_key(|(_, _, orig)| orig.is_some());
    for (n, path, orig) in &plan {
        // Checked again just before the replace, to narrow the window for an
        // edit made whilst undo runs.
        let same = ours(&dir, &e, *n, orig.as_deref(), path);
        same.then_some(()).ok_or(format!(
            "undo stopped: {} changed whilst undo ran",
            path.display()
        ))?;
        put_back(path, orig.as_deref())?;
    }
    remove(&dir)?;
    Ok(plan.len())
}

/// Gives `path` the bytes `orig`, or removes it when `orig` is `None`.
fn put_back(path: &Path, orig: Option<&[u8]>) -> Result<(), String> {
    match orig {
        Some(o) if fs::read(path).ok().as_deref() != Some(o) => {
            fs::create_dir_all(path.parent().unwrap_or(Path::new("."))).map_err(io(path))?;
            replace(path, o)?;
        }
        Some(_) => {}
        // Flushed before the journal goes, so a crash never brings the file back without it.
        None => {
            absent_ok(fs::remove_file(path)).map_err(io(path))?;
            sync_dir(path.parent().unwrap_or(Path::new(".")))?;
        }
    }
    let _ = fs::remove_file(temp_of(path));
    Ok(())
}

/// Whether `path` holds its original bytes, bytes prune wrote to it, or what
/// the stopped check left there (absence included).
fn ours(dir: &Path, e: &Entries, n: usize, orig: Option<&[u8]>, path: &Path) -> bool {
    let wrote = |name: String| read_plain(&dir.join("wrote").join(name));
    match fs::read(path) {
        Ok(b) => {
            Some(&b[..]) == orig
                || wrote(n.to_string()).as_ref() == Some(&b)
                || wrote(format!("{n}.new")).as_ref() == Some(&b)
        }
        Err(err) => {
            matches!(err.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory)
                && (orig.is_none() || e.deleted.contains(&n))
        }
    }
}

/// Accepts the last run's cuts by removing its journal.
///
/// # Errors
/// No journal, or it cannot be removed.
pub fn keep(root: &Path) -> Result<(), String> {
    let dir = dir_of(root)?;
    if !exists(root) {
        return Err("nothing to keep: no prune journal in this repository".into());
    }
    if !listed(&dir)?.complete {
        return Err("keep refused: the last run did not finish, so its edits were not all checked; run `prune undo`".into());
    }
    remove(&dir)
}

/// A removal that found nothing to remove counts as done.
fn absent_ok(r: std::io::Result<()>) -> std::io::Result<()> {
    r.or_else(|e| (e.kind() == ErrorKind::NotFound).then_some(()).ok_or(e))
}

/// Removes what is left of a journal folder a check damaged.
///
/// # Errors
/// The folder cannot be removed.
pub fn clear(root: &Path) -> Result<(), String> {
    let dir = dir_of(root)?;
    absent_ok(fs::remove_dir_all(&dir)).map_err(io(&dir))?;
    sync_dir(root)
}
