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

/// Rename `from` onto `to` so that the RENAME itself survives hard power loss —
/// the second slice of the fs/platform adapter, forced by the Windows grid node
/// (2026-09-21): `persona/seed.rs` made a rename durable the POSIX way (rename,
/// then `fsync` the parent directory opened read-only), and on Windows opening
/// a directory as a `File` is `AccessDenied`, so every seed persistence test
/// failed there on a production path.
///
/// The obvious fix — `#[cfg(unix)]` around the directory fsync — is the wrong
/// one: it hands Windows nodes WEAKER durability than POSIX nodes for persona
/// seeds and says nothing (the quiet-degrade class). The guarantee has a
/// portable equivalent, it just lives in a different call: on POSIX the rename
/// is journaled by the parent-directory fsync; on Windows `MoveFileExW` with
/// `MOVEFILE_WRITE_THROUGH` does not return until the rename is on disk. One
/// promise, two implementations, no arm that skips.
///
/// `to` is replaced if it exists, on both platforms (`rename(2)` semantics).
pub fn durable_rename(from: &std::path::Path, to: &std::path::Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::rename(from, to)?;
        let parent = to
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "rename target has no parent"))?;
        File::open(parent)?.sync_all()
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let wide = |p: &std::path::Path| -> Vec<u16> {
            p.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
        };
        let (from_w, to_w) = (wide(from), wide(to));
        // SAFETY: both buffers are NUL-terminated UTF-16 that outlive the call; the
        // flags are the documented constants; no pointer escapes the call.
        let ok = unsafe {
            MoveFileExW(
                from_w.as_ptr(),
                to_w.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
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

    // what this catches: the rename must land on every platform with `rename(2)`
    // semantics — the target is replaced, the source is gone, the bytes are the new
    // ones — through the arm that also makes it durable. On Windows this is the
    // exact call the seed tests failed on (a directory opened as a File), so a
    // green run there is the receipt that the quiet-skip fix was not taken.
    #[test]
    fn durable_rename_replaces_the_target_on_this_platform() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("seed.json.tmp");
        let to = dir.path().join("seed.json");
        std::fs::write(&to, b"old").unwrap();
        std::fs::write(&from, b"new").unwrap();
        durable_rename(&from, &to).unwrap();
        assert_eq!(std::fs::read(&to).unwrap(), b"new");
        assert!(!from.exists(), "the source is consumed by the rename");
        // A first write (no existing target) is the same call.
        let fresh = dir.path().join("fresh.json");
        std::fs::write(&from, b"first").unwrap();
        durable_rename(&from, &fresh).unwrap();
        assert_eq!(std::fs::read(&fresh).unwrap(), b"first");
    }
}
