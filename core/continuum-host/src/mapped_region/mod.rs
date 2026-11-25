//! Mapped regions: a file mapped readable, writable and shared, so its pages are the memory a consumer
//! reads and writes, and the OS pager moves them between memory and the drive (UNIFIED-PAGING.md, the
//! `Mapping` link). Demoting a region writes its dirty pages back and lets the OS drop them; promoting
//! asks the OS to read it ahead. Nothing is copied by the caller and nothing is encoded: the file's
//! bytes are the region's bytes. What differs by OS is the four calls (map, flush, release, prefetch);
//! the contract is here once.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;
use std::ptr::NonNull;

#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod os;

/// A file mapped shared and writable. Its memory lives as long as the region; dropping it unmaps.
pub struct MappedRegion {
    base: NonNull<u8>,
    len: usize,
    map: os::Map,
    file: File,
}

// SAFETY: the region is plain memory owned by this value. It holds no thread-bound state, and every
// access goes through `&self` (read) or `&mut self` (write), so Rust's borrow rules order them.
unsafe impl Send for MappedRegion {}
unsafe impl Sync for MappedRegion {}

impl MappedRegion {
    /// Map `len` bytes of the file at `path`, created if absent and extended to `len` if shorter (the
    /// extension is sparse: no byte is written, and an untouched page costs neither memory nor drive).
    pub fn create(path: &Path, len: usize) -> io::Result<Self> {
        if len == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "a mapped region of zero bytes"));
        }
        let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path)?;
        if file.metadata()?.len() < len as u64 {
            file.set_len(len as u64)?;
        }
        let (map, base) = os::map(&file, len)?;
        Ok(Self { base, len, map, file })
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The region's first byte, for a consumer that hands the memory to a device (a ggml buffer built
    /// over host memory). The pointer is valid while the region lives.
    pub fn as_ptr(&self) -> NonNull<u8> {
        self.base
    }

    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: `base` maps `len` readable bytes for as long as `self` lives.
        unsafe { std::slice::from_raw_parts(self.base.as_ptr(), self.len) }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        // SAFETY: `base` maps `len` writable bytes for as long as `self` lives, and `&mut self` is the
        // only access.
        unsafe { std::slice::from_raw_parts_mut(self.base.as_ptr(), self.len) }
    }

    /// Write every dirty page to the drive, then let the OS drop the pages from memory. The region stays
    /// mapped: its next touch reads back from the drive, so demoting never loses a byte.
    pub fn demote(&self) -> io::Result<()> {
        os::flush(&self.map, &self.file, self.base, self.len)?;
        os::release(self.base, self.len)
    }

    /// Ask the OS to read the whole region ahead of use. Returns once the request is made; the pages
    /// arrive on the OS's own IO.
    pub fn promote(&self) -> io::Result<()> {
        os::prefetch(self.base, self.len)
    }

    /// Write every dirty page to the drive, keeping the pages in memory.
    pub fn flush(&self) -> io::Result<()> {
        os::flush(&self.map, &self.file, self.base, self.len)
    }
}

impl Drop for MappedRegion {
    fn drop(&mut self) {
        os::unmap(&self.map, self.base, self.len);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a mapped region that is not the file's own bytes (a copy, or a private
    // mapping), a demote that loses dirty pages, and a promote or reopen that reads anything but what
    // was written.
    #[test]
    fn a_region_is_the_file_and_survives_demote_and_reopen() {
        let dir = tempfile::tempdir().expect("test: a temp dir");
        let path = dir.path().join("mind.kv");
        let len = 3 * 1024 * 1024 + 17;
        {
            let mut region = MappedRegion::create(&path, len).expect("test: a region");
            for (i, byte) in region.as_mut_slice().iter_mut().enumerate() {
                *byte = (i % 251) as u8;
            }
            region.demote().expect("test: demote");
            region.promote().expect("test: promote");
            assert!(region.as_slice().iter().enumerate().all(|(i, &b)| b == (i % 251) as u8), "back after demote");
        }
        assert_eq!(std::fs::metadata(&path).expect("test: the file").len(), len as u64);
        let reopened = MappedRegion::create(&path, len).expect("test: reopened");
        assert!(reopened.as_slice().iter().enumerate().all(|(i, &b)| b == (i % 251) as u8), "the file holds the region's bytes");
        assert!(MappedRegion::create(&dir.path().join("none"), 0).is_err(), "zero bytes is refused");
    }
}
