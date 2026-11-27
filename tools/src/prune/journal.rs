// SPDX-License-Identifier: Apache-2.0

//! The undo journal: each file's original bytes are on disk before prune
//! first edits it, and every write is an atomic replace, so `undo` can
//! restore the original bytes after a finished, failed or crashed run.

use super::{io, safe_path};
use serde_json::{Value, json};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The journal folder, inside the repository and ignored by Git.
pub const DIR: &str = ".puritanical-prune";

/// An open journal for the current run.
pub struct Journal {
    root: PathBuf,
    dir: PathBuf,
    files: Vec<String>,
}

/// Writes `bytes` to a new file and flushes it to disk.
fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut f = File::create(path).map_err(io(path))?;
    f.write_all(bytes)
        .and_then(|()| f.sync_all())
        .map_err(io(path))
}

/// Flushes a directory entry change (a rename) to disk where the OS allows it.
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

/// Replaces `path` with `bytes` atomically, keeping its permissions: the file
/// holds either the old or the new bytes at every moment.
fn replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let perms = fs::metadata(path).map_err(io(path))?.permissions();
    let tmp = temp_of(path);
    write_synced(&tmp, bytes)?;
    fs::set_permissions(&tmp, perms).map_err(io(&tmp))?;
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
        let j = Self {
            root: root.to_path_buf(),
            dir,
            files: Vec::new(),
        };
        j.save(false)?;
        Ok(j)
    }

    /// Replaces `journal.json` atomically.
    fn save(&self, complete: bool) -> Result<(), String> {
        let text = json!({ "complete": complete, "files": self.files }).to_string();
        let path = self.dir.join("journal.json");
        let tmp = self.dir.join("journal.json.tmp");
        write_synced(&tmp, text.as_bytes())?;
        fs::rename(&tmp, &path).map_err(io(&path))?;
        sync_dir(&self.dir)
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
            let n = self.files.len();
            let orig = fs::read(&path).map_err(io(&path))?;
            let copy = self.dir.join("orig").join(n.to_string());
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&copy)
                .and_then(|mut f| f.write_all(&orig).and_then(|()| f.sync_all()))
                .map_err(io(&copy))?;
            self.files.push(rel.to_string());
            self.save(false)?;
            n
        };
        let wrote = self.dir.join("wrote").join(n.to_string());
        let next = wrote.with_extension("new");
        write_synced(&next, bytes)?;
        replace(&path, bytes)?;
        fs::rename(&next, &wrote).map_err(io(&wrote))
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
    pub fn discard(self) -> Result<(), String> {
        remove(&self.dir)
    }
}

fn remove(dir: &Path) -> Result<(), String> {
    fs::remove_dir_all(dir).map_err(io(dir))
}

/// The files a journal lists and whether its run finished, from `journal.json`.
fn listed(dir: &Path) -> Result<(Vec<String>, bool), String> {
    let path = dir.join("journal.json");
    let text = fs::read_to_string(&path).map_err(io(&path))?;
    let bad = || format!("{}: not a prune journal", path.display());
    let v: Value = serde_json::from_str(&text).map_err(|_| bad())?;
    let files = v["files"]
        .as_array()
        .ok_or_else(bad)?
        .iter()
        .map(|f| f.as_str().map(str::to_string).ok_or_else(bad))
        .collect::<Result<_, _>>()?;
    Ok((files, v["complete"] == true))
}

/// Restores every file the journal lists to its original bytes, then removes
/// the journal. Refuses, changing nothing, when a file holds bytes prune did
/// not write (edited since). Returns the number of files restored.
///
/// # Errors
/// No journal, a file edited since, or an I/O failure.
pub fn undo(root: &Path) -> Result<usize, String> {
    let dir = dir_of(root)?;
    if !exists(root) {
        return Err("nothing to undo: no prune journal in this repository".into());
    }
    let mut plan = Vec::new();
    let mut edited = Vec::new();
    for (n, rel) in listed(&dir)?.0.into_iter().enumerate() {
        let path = safe_path(root, &rel)?;
        let orig_path = dir.join("orig").join(n.to_string());
        let orig = fs::read(&orig_path).map_err(io(&orig_path))?;
        if ours(&dir, n, &orig, &path) {
            plan.push((path, orig));
        } else {
            edited.push(rel);
        }
    }
    if !edited.is_empty() {
        return Err(format!(
            "undo refused, nothing changed: edited since prune wrote them: {}",
            edited.join(", ")
        ));
    }
    for (n, (path, orig)) in plan.iter().enumerate() {
        // Checked again just before the replace, to narrow the window for an
        // edit made whilst undo runs.
        let same = ours(&dir, n, orig, path);
        same.then_some(()).ok_or(format!(
            "undo stopped: {} changed whilst undo ran",
            path.display()
        ))?;
        if fs::read(path).ok().as_ref() != Some(orig) {
            replace(path, orig)?;
        }
        let _ = fs::remove_file(temp_of(path));
    }
    remove(&dir)?;
    Ok(plan.len())
}

/// Whether `path` holds its original bytes or bytes prune wrote to it.
fn ours(dir: &Path, n: usize, orig: &[u8], path: &Path) -> bool {
    let wrote = |name: String| fs::read(dir.join("wrote").join(name)).ok();
    fs::read(path).is_ok_and(|b| {
        b == orig
            || wrote(n.to_string()).as_ref() == Some(&b)
            || wrote(format!("{n}.new")).as_ref() == Some(&b)
    })
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
    if !listed(&dir)?.1 {
        return Err("keep refused: the last run did not finish, so its edits were not all checked; run `prune undo`".into());
    }
    remove(&dir)
}

/// The journal folder for check logs.
#[must_use]
pub fn log_dir(root: &Path) -> PathBuf {
    root.join(DIR)
}
