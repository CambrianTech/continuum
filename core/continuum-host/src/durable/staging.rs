//! Placing a file durably inside a held directory: a staging file written before it is placed, the
//! lock beside a file, and a re-fence of a name another publisher made. Every step is relative to
//! one held `Dir` (the stage is created in the target's own directory and renamed there), so no
//! path is re-walked between staging and placement for a concurrent writer to redirect (Codex and
//! Cormac on #5651). The path functions below open that directory and delegate: one implementation.
//!
//! What differs by OS is how a placement is made durable (`os::sync_placed`): POSIX syncs the
//! directory; Windows flushes the placed file, which forces the NTFS log that holds the rename.

use std::fs::TryLockError;
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use cap_std::fs::{Dir, File, OpenOptions};

static SEQ: AtomicU64 = AtomicU64::new(0);

/// The sequence number the next staging file tries first (tests plant a crash's leftovers there).
#[cfg(test)]
pub(super) fn peek_seq() -> u64 {
    SEQ.load(Ordering::Relaxed)
}

/// A staging file in a held directory, exclusively created. Write into [`Staging::file`], then
/// place it in the same directory with [`Staging::publish_new`] or [`Staging::replace`]; dropped
/// unplaced, it is removed.
pub struct Staging {
    dir: Dir,
    name: String,
    file: File,
}

impl Staging {
    /// A new staging file in `dir`. A name left by a crashed writer is skipped, never refused: the
    /// sequence restarts in every process and a container's core is PID 1 every boot (Cormac on
    /// #5651); each attempt takes a fresh number, so the walk ends at the first free name.
    pub fn new(dir: &Dir) -> io::Result<Self> {
        let dir = dir.try_clone()?;
        loop {
            let name = format!(".{}-{}.staging", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed));
            match dir.open_with(&name, OpenOptions::new().write(true).create_new(true)) {
                Ok(file) => return Ok(Staging { dir, name, file }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
    }

    /// A new staging file in the directory at `path` (opened and held for the placement).
    pub fn new_in(path: &Path) -> io::Result<Self> {
        Self::new(&Dir::open_ambient_dir(path, cap_std::ambient_authority())?)
    }

    /// The file to write the content into.
    pub fn file(&mut self) -> &mut File {
        &mut self.file
    }

    /// Write `bytes` whole into the staging file.
    pub fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.file.write_all(bytes)
    }

    /// Sync and move over `name` in the same directory, replacing what is there.
    pub fn replace(self, name: &Path) -> io::Result<()> {
        self.file.sync_all()?;
        super::retry_while_held(|| self.dir.rename(&self.name, &self.dir, name))?;
        super::os::sync_placed(&self.dir, name)
    }

    /// Sync and publish at `name` in the same directory: `AlreadyExists` if it is taken (a racing
    /// identical publisher calls [`fence_existing`] on it), never an overwrite.
    pub fn publish_new(self, name: &Path) -> io::Result<()> {
        self.file.sync_all()?;
        self.dir.hard_link(&self.name, &self.dir, name)?;
        super::os::sync_placed(&self.dir, name)
        // The hard link leaves the staging name; dropping `self` removes it.
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        // After a rename the name is gone (NotFound); after a hard link or a failure it is removed
        // here. Drop cannot return an error: the failure that matters was already returned, and a
        // leftover staging file holds no reachable state (the next life skips its name).
        let _ = self.dir.remove_file(&self.name);
    }
}

/// Complete the durability fence of `name` in `dir`, a name another publisher created and may not
/// have synced yet, so a reader never trusts an unfenced name.
pub fn fence_existing(dir: &Dir, name: &Path) -> io::Result<()> {
    dir.open_with(name, OpenOptions::new().read(true).write(true))?.sync_all()?;
    super::os::sync_placed(dir, name)
}

/// An exclusive lock on a file, held until dropped. The lock file is created on first use and
/// never deleted (deleting a lock file races the next locker).
pub struct Lock {
    _file: std::fs::File,
}

/// Lock `name` in `dir` exclusively, waiting for a holder when `wait`, else refusing with
/// `WouldBlock`.
pub fn lock(dir: &Dir, name: &Path, wait: bool) -> io::Result<Lock> {
    let file = dir.open_with(name, OpenOptions::new().read(true).write(true).create(true).truncate(false))?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("lock {} is not a regular file", name.display())));
    }
    let file = file.into_std();
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

/// Lock the file at `path` (its directory opened and held).
pub fn lock_path(path: &Path, wait: bool) -> io::Result<Lock> {
    let (dir, name) = split(path)?;
    lock(&Dir::open_ambient_dir(dir, cap_std::ambient_authority())?, Path::new(name), wait)
}

fn split(path: &Path) -> io::Result<(&Path, &std::ffi::OsStr)> {
    match (path.parent(), path.file_name()) {
        (Some(dir), Some(name)) => Ok((if dir.as_os_str().is_empty() { Path::new(".") } else { dir }, name)),
        _ => Err(io::Error::new(io::ErrorKind::InvalidInput, format!("{} names no file in a directory", path.display()))),
    }
}
