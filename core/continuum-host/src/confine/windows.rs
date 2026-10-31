//! Windows: `FILE_FLAG_OPEN_REPARSE_POINT` opens a symlink or junction itself instead of its
//! target; the opened handle's metadata then says it is a redirect, and the caller refuses it.

use std::fs::OpenOptions;
use std::io;
use std::os::windows::fs::OpenOptionsExt;

pub(super) fn no_follow(options: &mut OpenOptions) {
    options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
}

pub(super) fn is_redirect_refusal(_: &io::Error) -> bool {
    false
}
