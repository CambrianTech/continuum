//! Windows pins by share mode: every pin omits FILE_SHARE_DELETE, so neither the input nor a
//! directory on its path can be renamed, deleted or replaced while held. The input shares READ
//! only; a directory also shares WRITE, because a create, rename or delete INSIDE a directory
//! opens it for write.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;

use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

use super::Component;

pub(super) const FORBIDS_REPLACEMENT: bool = true;

pub(super) fn open_file(path: &Path) -> io::Result<File> {
    OpenOptions::new().read(true).share_mode(FILE_SHARE_READ).open(path)
}

pub(super) fn hold_component(path: &Path, role: Component) -> io::Result<File> {
    let share = match role {
        Component::Input => FILE_SHARE_READ,
        Component::Ancestor => FILE_SHARE_READ | FILE_SHARE_WRITE,
    };
    OpenOptions::new()
        .read(true)
        .share_mode(share)
        // Hold links and directories themselves, never what a link points at.
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
}
