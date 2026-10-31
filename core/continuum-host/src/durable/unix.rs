//! POSIX: a rename is durable once its directory is synced.

use std::fs::File;
use std::io;
use std::path::Path;

pub(super) fn move_onto(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::rename(from, to)
}

/// A hard link is an atomic create-if-absent on one filesystem; a rename would overwrite an
/// existing content address in a publisher race.
pub(super) fn create_new(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::hard_link(from, to)
}

pub(super) fn sync_entries(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all()
}

pub(super) fn held_by_another_process(_: &io::Error) -> bool {
    false
}

/// A name placed in a held directory survives power loss once the directory itself is synced.
pub(super) fn sync_placed(dir: &cap_std::fs::Dir, _name: &Path) -> io::Result<()> {
    dir.try_clone()?.into_std_file().sync_all()
}
