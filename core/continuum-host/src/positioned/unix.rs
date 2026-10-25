use std::fs::File;
use std::io;
use std::os::unix::fs::FileExt;

pub(super) fn read_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    file.read_at(buf, offset)
}
