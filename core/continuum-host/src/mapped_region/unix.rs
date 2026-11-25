//! The four calls on Unix: `mmap` shared, `msync` synchronously, `madvise(MADV_DONTNEED)` to drop
//! clean pages (a shared file mapping reads them back from the file on the next touch), and
//! `madvise(MADV_WILLNEED)` to read ahead.

use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;
use std::ptr::NonNull;

/// Nothing beyond the address is needed to unmap on Unix.
pub(super) struct Map;

pub(super) fn map(file: &File, len: usize) -> io::Result<(Map, NonNull<u8>)> {
    // SAFETY: a fresh shared mapping of an open file descriptor; the kernel chooses the address.
    let at = unsafe { libc::mmap(std::ptr::null_mut(), len, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED, file.as_raw_fd(), 0) };
    if at == libc::MAP_FAILED {
        return Err(io::Error::last_os_error());
    }
    NonNull::new(at.cast::<u8>()).map(|base| (Map, base)).ok_or_else(|| io::Error::other("mmap returned a null address"))
}

pub(super) fn flush(_map: &Map, _file: &File, base: NonNull<u8>, len: usize) -> io::Result<()> {
    // SAFETY: `base..base+len` is a live mapping owned by the caller.
    check(unsafe { libc::msync(base.as_ptr().cast(), len, libc::MS_SYNC) })
}

pub(super) fn release(base: NonNull<u8>, len: usize) -> io::Result<()> {
    // SAFETY: as above; the pages were just written back, so dropping them loses nothing.
    check(unsafe { libc::madvise(base.as_ptr().cast(), len, libc::MADV_DONTNEED) })
}

pub(super) fn prefetch(base: NonNull<u8>, len: usize) -> io::Result<()> {
    // SAFETY: as above; advice only.
    check(unsafe { libc::madvise(base.as_ptr().cast(), len, libc::MADV_WILLNEED) })
}

pub(super) fn unmap(_map: &Map, base: NonNull<u8>, len: usize) {
    // SAFETY: the mapping is owned by the region being dropped; nothing reads it after.
    unsafe { libc::munmap(base.as_ptr().cast(), len) };
}

fn check(rc: libc::c_int) -> io::Result<()> {
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
