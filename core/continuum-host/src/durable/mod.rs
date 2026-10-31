//! Durable placement: a file (or directory) placed here survives a crash or power loss whole, or the call fails.
//! Every write-then-rename in the core goes through `replace` (a mutable file) or `publish_new`
//! (an immutable, content-addressed one); a caller never renames and syncs by hand.
//!
//! What differs by OS, and only that, is in `os`: how a name is moved or created durably (POSIX
//! renames and then syncs the directory; Windows moves with MOVEFILE_WRITE_THROUGH, which "does not
//! return until the file is actually moved on the disk", and has no directory sync), and which
//! errors mean "another process holds this file".

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;
use std::time::Duration;

#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod os;

#[cfg(any(test, feature = "test-fixtures"))]
pub mod fault_injection;

#[cfg(test)]
mod tests;

/// How many times a move refused because another process holds the file is asked again, and the
/// first wait (doubling). A floor: a scanner releases a file it just saw in milliseconds, and the
/// waits sum to ~1.6 s, so a holder that keeps it longer is reported, not waited on. Measured on
/// the 5090 2026-10-10 15:28Z (job 00431d4d): a footprint store's single rename failed with os
/// error 32 and the measured graph it carried was lost.
const IN_USE_RETRIES: u32 = 6;
const IN_USE_FIRST_WAIT: Duration = Duration::from_millis(25);

/// Moves `from` onto `to`, replacing `to` if it exists, so the move survives power loss. `from` is
/// a file the caller wrote and synced (or a directory it filled).
pub fn replace(from: &Path, to: &Path) -> io::Result<()> {
    let mut wait = IN_USE_FIRST_WAIT;
    let mut attempt = 0;
    loop {
        match os::move_onto(from, to) {
            Ok(()) => break,
            Err(e) if attempt < IN_USE_RETRIES && os::held_by_another_process(&e) => {
                std::thread::sleep(wait);
                wait *= 2;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
    os::sync_entries(parent(to)?)
}

/// Publishes a synced staging file at an immutable address, refusing (`AlreadyExists`) to replace
/// an object already there. On failure the caller removes any staging file left; a visible
/// destination may exist without a completed fence, so a reader of an existing one calls `fence`.
pub fn publish_new(from: &Path, to: &Path) -> io::Result<()> {
    os::create_new(from, to)?;
    let published = OpenOptions::new().read(true).write(true).open(to)?;
    fence(&published, to)
}

/// Completes the durability fence of the opened file at `path`: its contents and its name. A
/// visible name alone is not a durable receipt: a competing publisher may have created it but not
/// yet fenced it. `file` must be writable (Windows flushes through the handle).
pub fn fence(file: &File, path: &Path) -> io::Result<()> {
    #[cfg(any(test, feature = "test-fixtures"))]
    if fault_injection::fence_fails() {
        return Err(io::Error::other("injected durability fence failure"));
    }
    file.sync_all()?;
    os::sync_entries(parent(path)?)
}

/// Creates the directory `dir` (its parent must exist) so its entry survives power loss. An
/// existing directory is `AlreadyExists`, as from `std::fs::create_dir`.
pub fn create_dir(dir: &Path) -> io::Result<()> {
    std::fs::create_dir(dir)?;
    os::sync_entries(parent(dir)?)
}

/// The directory holding `path`; a bare relative name lives in the current directory.
fn parent(path: &Path) -> io::Result<&Path> {
    match path.parent() {
        Some(p) if p.as_os_str().is_empty() => Ok(Path::new(".")),
        Some(p) => Ok(p),
        None => Err(io::Error::new(io::ErrorKind::InvalidInput, format!("{} has no parent directory", path.display()))),
    }
}
