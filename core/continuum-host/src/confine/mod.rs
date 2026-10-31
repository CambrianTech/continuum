//! Paths that stay under a root. Every component below the root is a real directory, and the final
//! file is opened without following a redirect, so bytes written through it land where the root's
//! owner measures them. A lexical check (no `..`, no absolute path) is not confinement: a symlink
//! or junction inside the root still sends a write outside it (Codex on #5651). The walk and its
//! refusals are here once; `unix.rs` / `windows.rs` hold only how to open a final name without
//! following it.
//!
//! The root itself may be a redirect (a home relocated to another volume is placed there on
//! purpose); what lies below it may not.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Component, Path, PathBuf};

#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod os;

#[cfg(test)]
mod tests;

fn refused(path: &Path, why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, format!("{} {why}: a confined path never leaves its root", path.display()))
}

/// The components of `relative`, refused unless each is a plain name (no root, `..`, `.`).
fn names(relative: &Path) -> io::Result<Vec<&std::ffi::OsStr>> {
    relative
        .components()
        .map(|part| match part {
            Component::Normal(name) => Ok(name),
            _ => Err(refused(relative, "is not a path of plain names")),
        })
        .collect()
}

/// A directory that is really one, not a redirect to somewhere else.
fn check_dir(path: &Path) -> io::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(refused(path, "is a symlink or junction")),
        Ok(meta) if meta.is_dir() => Ok(true),
        Ok(_) => Err(refused(path, "is not a directory")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// `root/relative` as a directory, creating what is missing, every component below `root` checked
/// to be a real directory (and checked again after creating it, so a redirect swapped in between
/// is caught).
pub fn dirs_within(root: &Path, relative: &Path) -> io::Result<PathBuf> {
    // The root is the owner's choice and may itself be a redirect (a relocated home); it is
    // created as given, so a fresh install's empty class still takes its first write (Cormac on
    // #5651: the walk created only what lay below it).
    std::fs::create_dir_all(root)?;
    let mut at = root.to_path_buf();
    for name in names(relative)? {
        at.push(name);
        if !check_dir(&at)? {
            match std::fs::create_dir(&at) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
            if !check_dir(&at)? {
                return Err(refused(&at, "vanished while it was created"));
            }
        }
    }
    Ok(at)
}

/// Open `root/relative` with `options`: its directories confined by [`dirs_within`], the file
/// itself opened without following a redirect (refused if it is one).
pub fn open_within(root: &Path, relative: &Path, options: &OpenOptions) -> io::Result<File> {
    let parts = names(relative)?;
    let (file, dirs) = parts.split_last().ok_or_else(|| refused(relative, "names no file"))?;
    let dir = dirs_within(root, &dirs.iter().collect::<PathBuf>())?;
    open_no_follow(&dir.join(file), options)
}

/// Open `path` without following it if it is a redirect: the final name is refused, never
/// resolved. Its directories are the caller's to have confined.
pub fn open_no_follow(path: &Path, options: &OpenOptions) -> io::Result<File> {
    let mut options = options.clone();
    os::no_follow(&mut options);
    let file = options.open(path).map_err(|error| if os::is_redirect_refusal(&error) { refused(path, "is a symlink or junction") } else { error })?;
    if file.metadata()?.file_type().is_symlink() {
        return Err(refused(path, "is a symlink or junction"));
    }
    Ok(file)
}
