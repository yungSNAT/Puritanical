// SPDX-License-Identifier: Apache-2.0

//! The undo journal: each file's original bytes are on disk before prune
//! first edits it, and every write is an atomic replace, so `undo` can
//! restore the original bytes after a finished, failed or crashed run.

use super::{io, safe_path};
use serde_json::{Value, json};
use std::fs::{self, File, OpenOptions};
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

/// Writes `bytes` to a new file and flushes it to disk.
fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut f = File::create(path).map_err(io(path))?;
    f.write_all(bytes)
        .and_then(|()| f.sync_all())
        .map_err(io(path))
}

/// Flushes a directory entry change (a rename) to disk where the OS allows it.
#[cfg_attr(not(unix), allow(clippy::unnecessary_wraps))]
fn sync_dir(dir: &Path) -> Result<(), String> {
    #[cfg(unix)]
    File::open(dir)
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
fn replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let perms = fs::metadata(path).ok().map(|m| m.permissions());
    let tmp = temp_of(path);
    write_synced(&tmp, bytes)?;
    if let Some(p) = perms {
        fs::set_permissions(&tmp, p).map_err(io(&tmp))?;
    }
    fs::rename(&tmp, path).map_err(io(path))?;
    sync_dir(path.parent().unwrap_or(Path::new(".")))
}

/// The journal folder of `root`, refused when it is a symbolic link.
fn dir_of(root: &Path) -> Result<PathBuf, String> {
    let dir = root.join(DIR);
    match fs::symlink_metadata(&dir) {
        Ok(m) if !m.is_dir() => Err(format!(
            "{} is not a plain folder; prune will not use it",
            dir.display()
        )),
        _ => Ok(dir),
    }
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

    /// Whether the journal's files are still on disk (a check may remove them).
    #[must_use]
    pub fn intact(&self) -> bool {
        ["journal.json", "orig", "wrote", ".gitignore"]
            .iter()
            .all(|p| self.dir.join(p).exists())
    }

    /// A path in the journal folder for a check log.
    #[must_use]
    pub fn log(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// Replaces `journal.json` atomically.
    fn save(&self, complete: bool) -> Result<(), String> {
        let text = json!({
            "complete": complete,
            "files": self.files,
            "created": self.created,
            "deleted": self.deleted,
        })
        .to_string();
        let path = self.dir.join("journal.json");
        let tmp = self.dir.join("journal.json.tmp");
        write_synced(&tmp, text.as_bytes())?;
        fs::rename(&tmp, &path).map_err(io(&path))?;
        sync_dir(&self.dir)
    }

    /// The entry of the repository file `rel`, added with its bytes before
    /// the run (`None`: it did not exist) when the journal lacks one.
    ///
    /// # Errors
    /// The journal cannot be written.
    pub fn track(&mut self, rel: &str, orig: Option<&[u8]>) -> Result<usize, String> {
        if let Some(n) = self.files.iter().position(|f| f == rel) {
            return Ok(n);
        }
        let n = self.files.len();
        if let Some(bytes) = orig {
            let copy = self.dir.join("orig").join(n.to_string());
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&copy)
                .and_then(|mut f| f.write_all(bytes).and_then(|()| f.sync_all()))
                .map_err(io(&copy))?;
            sync_dir(&self.dir.join("orig"))?;
        } else {
            self.created.push(n);
        }
        self.files.push(rel.to_string());
        self.save(false)?;
        Ok(n)
    }

    /// Records what the check left in entry `n` (`None`: it removed the
    /// file), so `undo` puts the original back over those bytes.
    ///
    /// # Errors
    /// The journal cannot be written.
    pub fn seen(&mut self, n: usize, now: Option<&[u8]>) -> Result<(), String> {
        let Some(bytes) = now else {
            self.deleted.push(n);
            return self.save(false);
        };
        let wrote = self.dir.join("wrote");
        let next = wrote.join(format!("{n}.new"));
        write_synced(&next, bytes)?;
        fs::rename(&next, wrote.join(n.to_string())).map_err(io(&next))?;
        sync_dir(&wrote)
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
            self.track(rel, Some(&orig))?
        };
        let wrote = self.dir.join("wrote");
        let next = wrote.join(format!("{n}.new"));
        write_synced(&next, bytes)?;
        replace(&path, bytes)?;
        fs::rename(&next, wrote.join(n.to_string())).map_err(io(&next))?;
        sync_dir(&wrote)
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

fn remove(dir: &Path) -> Result<(), String> {
    fs::remove_dir_all(dir).map_err(io(dir))
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
        None => absent_ok(fs::remove_file(path)).map_err(io(path))?,
    }
    let _ = fs::remove_file(temp_of(path));
    Ok(())
}

/// Whether `path` holds its original bytes, bytes prune wrote to it, or what
/// the stopped check left there (absence included).
fn ours(dir: &Path, e: &Entries, n: usize, orig: Option<&[u8]>, path: &Path) -> bool {
    let wrote = |name: String| fs::read(dir.join("wrote").join(name)).ok();
    match fs::read(path) {
        Ok(b) => {
            Some(&b[..]) == orig
                || wrote(n.to_string()).as_ref() == Some(&b)
                || wrote(format!("{n}.new")).as_ref() == Some(&b)
        }
        Err(err) => err.kind() == ErrorKind::NotFound && (orig.is_none() || e.deleted.contains(&n)),
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
    absent_ok(fs::remove_dir_all(&dir)).map_err(io(&dir))
}
