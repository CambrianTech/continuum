//! A file written before it is placed, the lock beside a file, and a re-fence of a name another
//! publisher made: the shapes every durable writer needs, owned once here so a store or a class
//! writer never opens, syncs, renames or removes a staging file by hand.

use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

/// The sequence number the next staging file tries first (tests plant a crash's leftovers there).
#[cfg(test)]
pub(super) fn peek_seq() -> u64 {
    SEQ.load(Ordering::Relaxed)
}

/// A staging file in a directory, exclusively created. Write into [`Staging::file`], then place it
/// with [`Staging::publish_new`] or [`Staging::replace`]; dropped unplaced, it is removed.
pub struct Staging {
    path: PathBuf,
    file: File,
}

impl Staging {
    /// A new staging file in `dir` (which exists, on the destination's filesystem). A name left by
    /// a crashed writer is skipped, never refused: the sequence restarts in every process and a
    /// container's core is PID 1 every boot, so `.1-0.staging` from the last life would otherwise
    /// fail this life's first write (Cormac on #5651). Each attempt takes a fresh sequence number,
    /// so the walk ends at the first name no file holds.
    pub fn new_in(dir: &Path) -> io::Result<Self> {
        loop {
            let path = dir.join(format!(".{}-{}.staging", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed)));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => return Ok(Staging { path, file }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
    }

    /// The file to write the content into.
    pub fn file(&mut self) -> &mut File {
        &mut self.file
    }

    /// Sync and publish at an immutable address: `AlreadyExists` if one is there (the caller that
    /// may be racing an identical publisher calls [`fence_existing`] on it).
    pub fn publish_new(self, to: &Path) -> io::Result<()> {
        self.file.sync_all()?;
        super::publish_new(&self.path, to)
        // The hard link leaves the staging name; dropping `self` removes it.
    }

    /// Sync and move over `to`, replacing what is there.
    pub fn replace(self, to: &Path) -> io::Result<()> {
        self.file.sync_all()?;
        super::replace(&self.path, to)
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        // After a placement by rename the name is gone (NotFound); after a hard link or a failure
        // it is removed here. Drop cannot return an error: the failure that matters was already
        // returned, and a leftover staging file holds no reachable state.
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Complete the durability fence of a name another publisher created (a content address it may
/// not have synced yet), so a reader never trusts an unfenced name.
pub fn fence_existing(path: &Path) -> io::Result<()> {
    let file = OpenOptions::new().read(true).write(true).open(path)?;
    super::fence(&file, path)
}

/// An exclusive lock on the file at `path`, held until dropped. The lock file is created on first
/// use and never deleted (deleting a lock file races the next locker).
pub struct Lock {
    _file: File,
}

/// Lock `path` exclusively, waiting for a holder when `wait`, else refusing with `WouldBlock`.
pub fn lock(path: &Path, wait: bool) -> io::Result<Lock> {
    if std::fs::symlink_metadata(path).is_ok_and(|meta| !meta.is_file()) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("lock {} is not a regular file", path.display())));
    }
    let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path)?;
    if wait {
        file.lock()?;
    } else {
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(io::Error::from(io::ErrorKind::WouldBlock)),
            Err(TryLockError::Error(error)) => return Err(error),
        }
    }
    Ok(Lock { _file: file })
}
