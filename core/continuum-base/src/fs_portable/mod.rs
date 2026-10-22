//! Portable positioned file I/O — the first concrete slice of the
//! fs/platform adapter (#197), forced by #304: `std::os::unix::fs::FileExt`
//! was imported un-gated in lib code (expert_container, capacity io-probe),
//! so `continuum-core` did not BUILD on windows-msvc — which silently
//! blocked the Windows grid node (the CUDA lane) from running any core
//! test locally. One helper, two cfg arms, no behavioral difference:
//! a positioned read never moves the file cursor on either platform
//! (`seek_read` on Windows moves the HANDLE cursor, but each call seeks
//! absolutely, so sequential callers see pread semantics).

pub mod replace;

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
        let parent = to.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "rename target has no parent")
        })?;
        File::open(parent)?.sync_all()
    }
    #[cfg(windows)]
    {
        move_write_through(from, to, true)
    }
}

/// Publish a synced staging file at an immutable address, refusing to replace
/// an existing object. On failure the caller cleans any remaining staging file;
/// a visible destination may exist without a completed durability fence.
pub fn durable_publish_new(from: &std::path::Path, to: &std::path::Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        // link is an atomic create-if-absent on the same filesystem; rename
        // would overwrite an existing content address in a publisher race.
        std::fs::hard_link(from, to)?;
    }
    #[cfg(windows)]
    {
        move_write_through(from, to, false)?;
    }
    let published = std::fs::OpenOptions::new().read(true).write(true).open(to)?;
    sync_publication(&published, to)
}

/// Complete a publication fence for this opened immutable object. A visible
/// name alone is not a durable receipt: a competing publisher can have linked
/// it but not yet fenced its directory, or failed while doing so.
/// `file` must be writable for Windows FlushFileBuffers (File::sync_all).
pub fn sync_publication(file: &File, path: &std::path::Path) -> io::Result<()> {
    #[cfg(any(test, feature = "test-fixtures"))]
    if fault_injection::publication_fence_fails() {
        return Err(io::Error::other("injected publication fence failure"));
    }
    file.sync_all()?;
    #[cfg(unix)]
    {
        let parent = path.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "publication target has no parent")
        })?;
        File::open(parent)?.sync_all()?;
    }
    #[cfg(windows)]
    let _ = path; // FlushFileBuffers flushes this file's buffered information.
    Ok(())
}

#[cfg(windows)]
fn move_write_through(
    from: &std::path::Path,
    to: &std::path::Path,
    replace: bool,
) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let wide = |p: &std::path::Path| -> Vec<u16> {
        p.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };
    let (from_w, to_w) = (wide(from), wide(to));
    // SAFETY: both buffers are NUL-terminated UTF-16 that outlive the call; the
    // flags are the documented constants; no pointer escapes the call.
    let ok = unsafe {
        MoveFileExW(
            from_w.as_ptr(),
            to_w.as_ptr(),
            MOVEFILE_WRITE_THROUGH
                | if replace {
                    MOVEFILE_REPLACE_EXISTING
                } else {
                    0
                },
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(any(test, feature = "test-fixtures"))]
pub mod fault_injection;

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

        // Immutable publication must refuse an occupied address, including
        // while a reader holds it open; the original bytes remain untouched.
        let reader = File::open(&fresh).unwrap();
        std::fs::write(&from, b"different").unwrap();
        assert_eq!(
            durable_publish_new(&from, &fresh).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        let mut original = [0; 5];
        read_exact_at(&reader, &mut original, 0).unwrap();
        assert_eq!(&original, b"first");
        let published = dir.path().join("immutable.log");
        durable_publish_new(&from, &published).unwrap();
        assert_eq!(std::fs::read(&published).unwrap(), b"different");
        let _ = std::fs::remove_file(&from);
    }
}
