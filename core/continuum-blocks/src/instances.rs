//! Copies of us on one node (UNIFIED-PAGING.md, "Other copies of us: in parallel, or gone"). Each core
//! that maps blocks joins a shared root under a lease: an exclusive OS lock on `<root>/<instance>.lease`,
//! held for as long as the core runs and released by the OS the moment it dies, however it dies. The
//! lock is the liveness truth, never a pid or a heartbeat. Each instance maps only inside its own
//! `<root>/<instance>/`, so two copies never write one file; a directory whose lease another core can
//! take belonged to a copy that is gone, and its live memory is reclaimed.

use std::io;
use std::path::{Path, PathBuf};

use continuum_host::durable::{lock_existing, lock_path, Lock};

/// A running core's place under a shared root: its own directory, and the lease that says it is alive.
/// Dropping it releases the lease; its directory is left for the next joiner to reclaim.
#[derive(Debug)]
pub struct Instance {
    dir: PathBuf,
    _lease: Lock,
}

/// What joining found of copies that are gone.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Reclaimed {
    /// Their directories, removed.
    pub instances: usize,
    /// The bytes those directories held on disk.
    pub bytes: u64,
    /// Gone instances that could not be removed now: left for the next joiner, never failing this one.
    pub skipped: usize,
}

impl Instance {
    /// Join `root` as `name` (unique per run): take this instance's lease, make its directory, and
    /// reclaim every directory whose lease no live core holds.
    pub fn join(root: &Path, name: &str) -> io::Result<(Self, Reclaimed)> {
        std::fs::create_dir_all(root)?;
        // her own lease is taken under the one reclaim lock, so no reclaimer ever opens it between its
        // creation and its lock and takes a starting copy for a gone one (canary f6648967d, CI run
        // 45940039043: 13 reclaimed of 12 gone, Cormac)
        let _one_at_a_time = lock_path(&root.join(".reclaim"), true)?;
        let lease = lock_path(&root.join(format!("{name}.lease")), false)?;
        let dir = root.join(name);
        std::fs::create_dir_all(&dir)?;
        let reclaimed = reclaim(root, name)?;
        Ok((Self { dir, _lease: lease }, reclaimed))
    }

    /// Where this instance maps its blocks.
    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

/// Every other instance under `root` whose lease can be taken is gone: its directory and lease file go.
/// One joiner reclaims at a time, under `<root>/.reclaim` (a lock outside the scanned leases, taken by the
/// caller before its own lease and held through this scan), so no joiner opens a lease another is deleting
/// or one still being taken: on Windows that open is denied while the
/// delete is pending (Codex: 12 joiners started together against 12 gone leases, error 5 in round 0).
/// Another instance's lease is opened only if it exists, never created, so a reclaimed lease is never
/// resurrected. A lease held by a live core is left alone. Reclaiming is best effort per instance: one
/// that cannot be removed now is counted as skipped and left for the next joiner, and never fails this
/// instance's own join.
fn reclaim(root: &Path, own: &str) -> io::Result<Reclaimed> {
    let mut reclaimed = Reclaimed::default();
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        let Some(name) = path.file_stem().and_then(|stem| stem.to_str()).map(str::to_owned) else { continue };
        if name == own || path.extension().and_then(|ext| ext.to_str()) != Some("lease") {
            continue;
        }
        match lock_existing(&path) {
            Ok(Some(held)) => match remove_gone(root, &name, &path, held) {
                Ok(bytes) => {
                    reclaimed.instances += 1;
                    reclaimed.bytes += bytes;
                }
                Err(_) => reclaimed.skipped += 1,
            },
            Ok(None) => {} // already reclaimed
            Err(alive) if alive.kind() == io::ErrorKind::WouldBlock => {}
            Err(_) => reclaimed.skipped += 1,
        }
    }
    Ok(reclaimed)
}

/// A gone instance's directory, then its lease file (the lock released first: Windows refuses to
/// delete an open file). Returns the bytes its files held.
fn remove_gone(root: &Path, name: &str, lease: &Path, held: Lock) -> io::Result<u64> {
    let dir = root.join(name);
    let bytes = disk_bytes(&dir);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    drop(held);
    match std::fs::remove_file(lease) {
        Ok(()) => Ok(bytes),
        Err(gone) if gone.kind() == io::ErrorKind::NotFound => Ok(bytes),
        Err(other) => Err(other),
    }
}

/// The bytes a gone instance's files held: their length (a sparse file's untouched pages included,
/// so this is the most it could have held). A directory that is not there held nothing.
fn disk_bytes(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries.flatten().filter_map(|entry| entry.metadata().ok()).map(|meta| meta.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (UNIFIED-PAGING.md, other copies of us): a starting core deleting a running
    // copy's live files (the slice-2 driver emptied the whole live directory at start), or leaking a
    // dead copy's files forever.
    #[test]
    fn a_live_copy_is_left_alone_and_a_gone_copy_is_reclaimed() {
        let root = tempfile::tempdir().expect("test: a root");
        let (running, _) = Instance::join(root.path(), "running").expect("test: the running copy joins");
        std::fs::write(running.dir().join("mind.kv"), [7u8; 4096]).expect("test: a live file");
        {
            let (gone, _) = Instance::join(root.path(), "gone").expect("test: a copy that will go");
            std::fs::write(gone.dir().join("mind.kv"), [9u8; 8192]).expect("test: its file");
        } // the copy ends: its lease is released, its directory stays
        let (_starting, reclaimed) = Instance::join(root.path(), "starting").expect("test: a new copy joins");
        assert_eq!(reclaimed, Reclaimed { instances: 1, bytes: 8192, skipped: 0 }, "the gone copy's file is reclaimed");
        assert!(!root.path().join("gone").exists() && !root.path().join("gone.lease").exists());
        assert_eq!(std::fs::read(running.dir().join("mind.kv")).expect("test: still there"), vec![7u8; 4096], "the running copy is untouched");
    }

    // what this catches (Codex, the 5090, 2026-11-25: 12 joiners started together against 12 gone
    // leases failed in round 0 with Windows error 5): joiners racing to reclaim the same gone copies, one
    // opening a lease another is deleting, or resurrecting one another already removed. Every join
    // succeeds, and every gone copy is reclaimed exactly once across all of them.
    #[test]
    fn many_copies_starting_together_all_join_and_reclaim_each_gone_copy_once() {
        const N: usize = 12;
        let root = tempfile::tempdir().expect("test: a root");
        let gone: Vec<Instance> = (0..N)
            .map(|i| {
                let (copy, _) = Instance::join(root.path(), &format!("gone-{i}")).expect("test: a copy that will go");
                std::fs::write(copy.dir().join("mind.kv"), [1u8; 1024]).expect("test: its file");
                copy
            })
            .collect(); // all alive at once, so none reclaims another
        drop(gone); // then all gone together: 12 released leases, 12 directories left behind
        let start = std::sync::Arc::new(std::sync::Barrier::new(N));
        let joiners: Vec<_> = (0..N)
            .map(|i| {
                let (root, start) = (root.path().to_path_buf(), std::sync::Arc::clone(&start));
                std::thread::spawn(move || {
                    start.wait();
                    Instance::join(&root, &format!("starting-{i}"))
                })
            })
            .collect();
        let mut joined = Vec::new();
        let mut reclaimed = Reclaimed::default();
        for joiner in joiners {
            let (instance, found) = joiner.join().expect("test: a joiner thread").expect("test: every concurrent join succeeds");
            reclaimed.instances += found.instances;
            reclaimed.skipped += found.skipped;
            joined.push(instance);
        }
        assert_eq!((reclaimed.instances, reclaimed.skipped), (N, 0), "each gone copy reclaimed once, none skipped");
        assert!((0..N).all(|i| !root.path().join(format!("gone-{i}")).exists() && !root.path().join(format!("gone-{i}.lease")).exists()));
        assert!(joined.iter().all(|instance| instance.dir().exists()), "every new copy holds its place");
    }
}
