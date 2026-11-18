//! A directory's files, kept current by the kernel's change events instead of re-walked per read.
//!
//! The pool's broker reads usage every tick; a walk per read was `readdir` + `stat` of every file every
//! tick (measured on the M5, 2026-11-17: ~615 of ~6,900 active core samples under
//! `FilesystemTierPool::walk`, from `PressureBroker::relieve`). The index is walked ONCE when the pool is
//! made, then each change the kernel reports updates only the paths it names: usage is a counter, and the
//! eviction order is an ordered set by mtime, so the oldest file is found without a sort.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Default)]
pub(super) struct DirIndex {
    /// Ordered by path, so everything under a directory is one contiguous range.
    by_path: BTreeMap<PathBuf, (u64, SystemTime)>,
    /// Ordered by mtime, each with its size: the eviction order, read without a lookup.
    by_age: BTreeMap<(SystemTime, PathBuf), u64>,
    total: u64,
}

/// Whether a reconcile could read what it was asked about. `Unreadable` (a permission or I/O error,
/// never absence) kept the last known accounting: the owner rescans on its next read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub(super) enum Seen {
    Current,
    Unreadable,
}

impl DirIndex {
    /// Replace the whole index with what is under `dir` now, recursively. An absent `dir` is empty;
    /// an unreadable one keeps what was known and says so.
    pub(super) fn reseed(&mut self, dir: &Path) -> Seen {
        let mut fresh = Self::default();
        match fresh.read_tree(dir) {
            Ok(()) => {
                *self = fresh;
                Seen::Current
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                *self = Self::default();
                Seen::Current
            }
            Err(_) => Seen::Unreadable,
        }
    }

    pub(super) fn total(&self) -> u64 {
        self.total
    }

    /// The files oldest-first by mtime, with their sizes: the eviction order.
    pub(super) fn oldest_first(&self) -> Vec<(PathBuf, u64)> {
        self.by_age.iter().map(|((_, path), size)| (path.clone(), *size)).collect()
    }

    /// What the kernel says changed at `path`, reconciled against what is there now: a file is recorded
    /// at its current size and mtime; a directory (a coalesced event may name only it) has its whole
    /// subtree re-read into a fresh index and replaces what was known only when the whole read
    /// succeeded; a path CONFIRMED absent is dropped with everything under it. Any other error keeps
    /// the last known accounting and reports `Unreadable` (Codex on #6113: a permission or I/O error
    /// is not a deletion).
    pub(super) fn observe(&mut self, path: &Path) -> Seen {
        match fs::metadata(path) {
            Ok(meta) if meta.is_dir() => {
                let mut fresh = Self::default();
                match fresh.read_tree(path) {
                    Ok(()) => {
                        self.forget_under(path);
                        for (known, (size, mtime)) in fresh.by_path {
                            self.record(&known, size, mtime);
                        }
                        Seen::Current
                    }
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {
                        self.forget_under(path);
                        Seen::Current
                    }
                    Err(_) => Seen::Unreadable,
                }
            }
            Ok(meta) => {
                self.record(path, meta.len(), meta.modified().unwrap_or(SystemTime::UNIX_EPOCH)); // unwrap_or: a file system with no mtime orders it first
                Seen::Current
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                self.forget_under(path);
                Seen::Current
            }
            Err(_) => Seen::Unreadable,
        }
    }

    fn forget(&mut self, path: &Path) {
        if let Some((size, mtime)) = self.by_path.remove(path) {
            self.by_age.remove(&(mtime, path.to_path_buf()));
            self.total = self.total.saturating_sub(size);
        }
    }

    fn record(&mut self, path: &Path, size: u64, mtime: SystemTime) {
        self.forget(path);
        self.by_path.insert(path.to_path_buf(), (size, mtime));
        self.by_age.insert((mtime, path.to_path_buf()), size);
        self.total = self.total.saturating_add(size);
    }

    fn forget_under(&mut self, path: &Path) {
        // Paths under `path` sort right after it: one range, never a scan of every file.
        let gone: Vec<PathBuf> = self.by_path.range(path.to_path_buf()..).map(|(known, _)| known).take_while(|known| known.starts_with(path)).cloned().collect();
        for known in gone {
            self.forget(&known);
        }
    }

    /// Record every file under `dir`, recursively. `dir` absent is `NotFound`; a child removed while
    /// the walk runs is skipped (confirmed absent); any other error stops the read, so a caller
    /// never takes a partial read for the whole.
    fn read_tree(&mut self, dir: &Path) -> io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let meta = match entry.metadata() {
                Ok(meta) => meta,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            };
            if meta.is_dir() {
                match self.read_tree(&path) {
                    Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                    other => other?,
                }
            } else {
                self.record(&path, meta.len(), meta.modified().unwrap_or(SystemTime::UNIX_EPOCH)); // unwrap_or: a file system with no mtime orders it first
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (Codex on #6113 73aa929b4): a coalesced event that names only a directory kept a
    // child deleted without its own event counted forever, and a reconcile after an eviction erased the
    // accounting of a file recreated at the same path.
    #[test]
    fn a_directory_event_reconciles_its_subtree_and_a_recreated_file_keeps_its_bytes() {
        let dir = tempfile::tempdir().expect("test: a temp dir");
        let sub = dir.path().join("persona");
        fs::create_dir(&sub).expect("test: a subdir");
        fs::write(sub.join("a.log"), vec![b'.'; 700]).expect("test: a file");
        fs::write(sub.join("b.log"), vec![b'.'; 300]).expect("test: a file");
        let mut index = DirIndex::default();
        assert_eq!(index.reseed(dir.path()), Seen::Current);
        assert_eq!(index.total(), 1_000);

        // b.log goes with no event of its own; the kernel names only its directory.
        fs::remove_file(sub.join("b.log")).expect("test: remove");
        assert_eq!(index.observe(&sub), Seen::Current);
        assert_eq!(index.total(), 700, "the directory event dropped the child removed without its own event");

        // a.log is removed and recreated (another writer) before the remover reconciles it.
        fs::remove_file(sub.join("a.log")).expect("test: remove");
        fs::write(sub.join("a.log"), vec![b'.'; 50]).expect("test: recreate");
        assert_eq!(index.observe(&sub.join("a.log")), Seen::Current);
        assert_eq!(index.total(), 50, "the recreated file is what is accounted, never erased");

        // The whole directory goes: everything under it is dropped as one range.
        fs::remove_dir_all(&sub).expect("test: remove dir");
        assert_eq!(index.observe(&sub), Seen::Current);
        assert_eq!(index.total(), 0);
        assert!(index.oldest_first().is_empty());
    }

    // what this catches (Codex on #6113 d9e1af314): a permission or I/O error read as a deletion, so a
    // directory the pool could not read erased its known usage. Unreadable keeps the last known
    // accounting and says so; once readable, the reconcile is current again.
    #[cfg(unix)]
    #[test]
    fn an_unreadable_directory_keeps_its_accounting_until_it_reads_again() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("test: a temp dir");
        let sub = dir.path().join("persona");
        fs::create_dir(&sub).expect("test: a subdir");
        fs::write(sub.join("a.log"), vec![b'.'; 400]).expect("test: a file");
        let mut index = DirIndex::default();
        assert_eq!(index.reseed(dir.path()), Seen::Current);
        assert_eq!(index.total(), 400);

        fs::set_permissions(&sub, fs::Permissions::from_mode(0o000)).expect("test: make it unreadable");
        let unreadable = index.observe(&sub);
        fs::set_permissions(&sub, fs::Permissions::from_mode(0o755)).expect("test: make it readable again");
        assert_eq!(unreadable, Seen::Unreadable, "an unreadable directory is unknown, never deleted");
        assert_eq!(index.total(), 400, "its known usage is kept");

        fs::write(sub.join("b.log"), vec![b'.'; 100]).expect("test: a new file");
        assert_eq!(index.observe(&sub), Seen::Current, "readable again: reconciled");
        assert_eq!(index.total(), 500);
    }
}
