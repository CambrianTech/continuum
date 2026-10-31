//! A root held open as a directory handle, and everything below it reached through that handle.
//! A lexical check (no `..`) is not confinement, and neither is checking a path's components and
//! then opening the path: a concurrent writer can swap a checked directory for a link before the
//! open (Codex on #5651). `cap_std::fs::Dir` resolves every component against the held handle,
//! never following a link out of it, so a create, open, rename or listing below the root cannot
//! land outside it however the tree changes meanwhile.
//!
//! The root itself may be a redirect (a home relocated to another volume is placed there on
//! purpose); it is resolved once, when it is opened.

use std::io;
use std::path::{Component, Path};

pub use cap_std::fs::{Dir, OpenOptions};

#[cfg(test)]
mod tests;

/// `relative`, refused unless it is plain names (no root, `..`, `.`), and not empty.
pub fn plain(relative: &Path) -> io::Result<&Path> {
    let mut parts = relative.components().peekable();
    if parts.peek().is_none() || !parts.all(|part| matches!(part, Component::Normal(_))) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("{} is not a path of plain names", relative.display())));
    }
    Ok(relative)
}

/// The root an owner chose, created if missing and held open.
pub fn open_root(root: &Path) -> io::Result<Dir> {
    std::fs::create_dir_all(root)?;
    Dir::open_ambient_dir(root, cap_std::ambient_authority())
}

/// The directory `relative` below `dir` (empty: `dir` itself), created as needed, held open.
pub fn subdir(dir: &Dir, relative: &Path) -> io::Result<Dir> {
    if relative.as_os_str().is_empty() {
        return dir.try_clone();
    }
    let relative = plain(relative)?;
    dir.create_dir_all(relative)?;
    dir.open_dir(relative)
}
