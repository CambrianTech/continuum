//! seek_read moves the handle's cursor, but every call seeks absolutely, so callers still see
//! pread semantics.

use std::fs::File;
use std::io;
use std::os::windows::fs::FileExt;

pub(super) fn read_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    file.seek_read(buf, offset)
}
