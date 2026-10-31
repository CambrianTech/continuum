//! POSIX: `O_NOFOLLOW` makes opening a symlink fail with `ELOOP`.

use std::fs::OpenOptions;
use std::io;
use std::os::unix::fs::OpenOptionsExt;

pub(super) fn no_follow(options: &mut OpenOptions) {
    options.custom_flags(libc::O_NOFOLLOW);
}

pub(super) fn is_redirect_refusal(error: &io::Error) -> bool {
    error.raw_os_error() == Some(libc::ELOOP)
}
