//! Portable positioned file I/O — the first concrete slice of the
//! fs/platform adapter (#197), forced by #304: `std::os::unix::fs::FileExt`
//! was imported un-gated in lib code (expert_container, capacity io-probe),
//! so `continuum-core` did not BUILD on windows-msvc — which silently
//! blocked the Windows grid node (the CUDA lane) from running any core
//! test locally. One helper, two cfg arms, no behavioral difference:
//! a positioned read never moves the file cursor on either platform
//! (`seek_read` on Windows moves the HANDLE cursor, but each call seeks
//! absolutely, so sequential callers see pread semantics).

use std::fs::File;
use std::io;

/// Read exactly `buf.len()` bytes at absolute `offset` — pread semantics
/// on every platform. Errors with `UnexpectedEof` if the file ends short,
/// matching unix `read_exact_at`.
pub fn read_exact_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        file.read_exact_at(buf, offset)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        let mut done = 0usize;
        while done < buf.len() {
            let n = file.seek_read(&mut buf[done..], offset + done as u64)?;
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "failed to fill whole buffer",
                ));
            }
            done += n;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // what this catches: pread semantics on the CURRENT platform — exact
    // fill at an offset, UnexpectedEof (not a partial fill) past the end.
    // The same test compiles and passes on unix and windows, which is the
    // whole point of the helper.
    #[test]
    fn positioned_read_is_exact_and_eof_loud() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("bank.bin");
        let mut f = File::create(&path).expect("create");
        f.write_all(&[1, 2, 3, 4, 5, 6, 7, 8]).expect("write");
        drop(f);

        let f = File::open(&path).expect("open");
        let mut buf = [0u8; 4];
        read_exact_at(&f, &mut buf, 2).expect("mid-file read");
        assert_eq!(buf, [3, 4, 5, 6]);

        let mut over = [0u8; 4];
        let err = read_exact_at(&f, &mut over, 6).expect_err("past-end read");
        assert_eq!(err.kind(), std::io::ErrorKind::UnexpectedEof);
    }
}
