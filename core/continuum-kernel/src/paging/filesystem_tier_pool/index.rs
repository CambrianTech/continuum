//! A directory's files, kept current by the kernel's change events instead of re-walked per read.
//!
//! The pool's broker reads usage every tick; a walk per read was `readdir` + `stat` of every file every
//! tick (measured on the M5, 2026-11-17: ~615 of ~6,900 active core samples under
//! `FilesystemTierPool::walk`, from `PressureBroker::relieve`). The index is walked ONCE when the pool is
//! made, then each change the kernel reports updates only the paths it names: usage is a counter, and the
//! eviction order is an ordered set by mtime, so the oldest file is found without a sort.

use std::collections::BTreeMap;
use std::fs;
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

impl DirIndex {
    /// Every file under `dir`, recursively (an absent or unreadable dir is empty).
    pub(super) fn seeded(dir: &Path) -> Self {
        let mut index = Self::default();
        index.observe_tree(dir);
        index
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
    /// subtree re-read, so a child removed without its own event is dropped too; a path that is gone
    /// is dropped with everything recorded under it.
    pub(super) fn observe(&mut self, path: &Path) {
        match fs::metadata(path) {
            Ok(meta) if meta.is_dir() => {
                self.forget_under(path);
                self.observe_tree(path);
            }
            Ok(meta) => self.record(path, meta.len(), meta.modified().unwrap_or(SystemTime::UNIX_EPOCH)), // unwrap_or: a file system with no mtime orders it first
            Err(_) => self.forget_under(path),
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

    fn observe_tree(&mut self, dir: &Path) {
        let Ok(entries) = fs::read_dir(dir) else {
            return; // absent or unreadable: nothing to record
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                self.observe_tree(&path);
            } else {
                self.record(&path, meta.len(), meta.modified().unwrap_or(SystemTime::UNIX_EPOCH)); // unwrap_or: a file system with no mtime orders it first
            }
        }
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
        let mut index = DirIndex::seeded(dir.path());
        assert_eq!(index.total(), 1_000);

        // b.log goes with no event of its own; the kernel names only its directory.
        fs::remove_file(sub.join("b.log")).expect("test: remove");
        index.observe(&sub);
        assert_eq!(index.total(), 700, "the directory event dropped the child removed without its own event");

        // a.log is removed and recreated (another writer) before the remover reconciles it.
        fs::remove_file(sub.join("a.log")).expect("test: remove");
        fs::write(sub.join("a.log"), vec![b'.'; 50]).expect("test: recreate");
        index.observe(&sub.join("a.log"));
        assert_eq!(index.total(), 50, "the recreated file is what is accounted, never erased");

        // The whole directory goes: everything under it is dropped as one range.
        fs::remove_dir_all(&sub).expect("test: remove dir");
        index.observe(&sub);
        assert_eq!(index.total(), 0);
        assert!(index.oldest_first().is_empty());
    }
}
