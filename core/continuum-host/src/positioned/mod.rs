//! Positioned reads: read at an absolute offset without depending on the file cursor (pread
//! semantics), so concurrent readers of one handle never race. What differs by OS is the single
//! positioned read call; the exact-fill loop is here once.

use std::fs::File;
use std::io;

#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod os;

/// Reads exactly `buf.len()` bytes at `offset`, or `UnexpectedEof` if the file ends first.
pub fn read_exact_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<()> {
    let mut done = 0usize;
    while done < buf.len() {
        match os::read_at(file, &mut buf[done..], offset + done as u64) {
            Ok(0) => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "failed to fill whole buffer")),
            Ok(n) => done += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // what this catches: an exact fill at an offset, and UnexpectedEof (never a partial fill)
    // past the end, on whichever platform this runs.
    #[test]
    fn a_positioned_read_is_exact_and_eof_is_loud() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bank.bin");
        File::create(&path).unwrap().write_all(&[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        let f = File::open(&path).unwrap();
        let mut buf = [0u8; 4];
        read_exact_at(&f, &mut buf, 2).unwrap();
        assert_eq!(buf, [3, 4, 5, 6]);
        let mut over = [0u8; 4];
        assert_eq!(read_exact_at(&f, &mut over, 6).unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
    }
}
