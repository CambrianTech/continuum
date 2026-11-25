//! Copies of us on one node (UNIFIED-PAGING.md, "Other copies of us: in parallel, or gone"). Each core
//! that maps blocks joins a shared root under a lease: an exclusive OS lock on `<root>/<instance>.lease`,
//! held for as long as the core runs and released by the OS the moment it dies, however it dies. The
//! lock is the liveness truth, never a pid or a heartbeat. Each instance maps only inside its own
//! `<root>/<instance>/`, so two copies never write one file; a directory whose lease another core can
//! take belonged to a copy that is gone, and its live memory is reclaimed.

use std::io;
use std::path::{Path, PathBuf};

use continuum_host::durable::{lock_path, Lock};

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
}

impl Instance {
    /// Join `root` as `name` (unique per run): take this instance's lease, make its directory, and
    /// reclaim every directory whose lease no live core holds.
    pub fn join(root: &Path, name: &str) -> io::Result<(Self, Reclaimed)> {
        std::fs::create_dir_all(root)?;
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
/// A lease held by a live core answers `WouldBlock` and is left alone.
fn reclaim(root: &Path, own: &str) -> io::Result<Reclaimed> {
    let mut reclaimed = Reclaimed::default();
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        let Some(name) = path.file_stem().and_then(|stem| stem.to_str()).map(str::to_owned) else { continue };
        if name == own || path.extension().and_then(|ext| ext.to_str()) != Some("lease") {
            continue;
        }
        match lock_path(&path, false) {
            Ok(held) => {
                let dir = root.join(&name);
                reclaimed.bytes += disk_bytes(&dir);
                if dir.exists() {
                    std::fs::remove_dir_all(&dir)?;
                }
                drop(held);
                std::fs::remove_file(&path)?;
                reclaimed.instances += 1;
            }
            Err(alive) if alive.kind() == io::ErrorKind::WouldBlock => {}
            Err(other) => return Err(other),
        }
    }
    Ok(reclaimed)
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
        assert_eq!(reclaimed, Reclaimed { instances: 1, bytes: 8192 }, "the gone copy's file is reclaimed");
        assert!(!root.path().join("gone").exists() && !root.path().join("gone.lease").exists());
        assert_eq!(std::fs::read(running.dir().join("mind.kv")).expect("test: still there"), vec![7u8; 4096], "the running copy is untouched");
    }
}
