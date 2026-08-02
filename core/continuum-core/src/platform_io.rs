//! Cross-platform positioned file I/O.
//!
//! Unix has `std::os::unix::fs::FileExt::read_exact_at`; Windows has
//! `std::os::windows::fs::FileExt::seek_read`, which (like `read`) may return short, so on Windows we
//! loop to fill the whole buffer. This is the ONE place the platform split lives - callers use
//! [`pread_exact`] and never reach for `os::unix`/`os::windows` directly (the un-cfg-gated
//! `os::unix::fs::FileExt` import is exactly what broke the windows-msvc build).

use std::fs::File;
use std::io;

/// Read exactly `buf.len()` bytes at absolute `offset`, without moving the file cursor. Portable
/// equivalent of Unix `read_exact_at`.
pub(crate) fn pread_exact(file: &File, buf: &mut [u8], offset: u64) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        file.read_exact_at(buf, offset)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        let mut buf = buf;
        let mut off = offset;
        while !buf.is_empty() {
            match file.seek_read(buf, off) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "failed to fill whole buffer",
                    ))
                }
                Ok(n) => {
                    let tmp = buf;
                    buf = &mut tmp[n..];
                    off += n as u64;
                }
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (file, buf, offset);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "positioned reads unsupported on this platform",
        ))
    }
}
