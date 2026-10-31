//! Windows: a move with MOVEFILE_WRITE_THROUGH is durable when it returns; a directory cannot be
//! opened as a file to sync, and needs no sync once every entry in it was placed that way.

use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};

pub(super) fn move_onto(from: &Path, to: &Path) -> io::Result<()> {
    move_write_through(from, to, MOVEFILE_REPLACE_EXISTING)
}

pub(super) fn create_new(from: &Path, to: &Path) -> io::Result<()> {
    move_write_through(from, to, 0)
}

pub(super) fn sync_entries(_: &Path) -> io::Result<()> {
    Ok(())
}

/// Sharing violation (32) or access denied (5): usually a scanner reading a file just written.
pub(super) fn held_by_another_process(e: &io::Error) -> bool {
    matches!(e.raw_os_error(), Some(32) | Some(5))
}

fn move_write_through(from: &Path, to: &Path, flags: u32) -> io::Result<()> {
    let wide = |p: &Path| p.as_os_str().encode_wide().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (from_w, to_w) = (wide(from), wide(to));
    // SAFETY: both buffers are NUL-terminated UTF-16 that outlive the synchronous call; the flags
    // are documented constants; no pointer escapes the call.
    if unsafe { MoveFileExW(from_w.as_ptr(), to_w.as_ptr(), MOVEFILE_WRITE_THROUGH | flags) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// A directory cannot be opened as a file to sync; flushing the placed file forces the NTFS log,
/// which holds the rename or link that placed it.
pub(super) fn sync_placed(dir: &cap_std::fs::Dir, name: &Path) -> io::Result<()> {
    dir.open_with(name, cap_std::fs::OpenOptions::new().read(true).write(true))?.sync_all()
}
