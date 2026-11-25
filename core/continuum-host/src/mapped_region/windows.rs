//! The four calls on Windows: a file mapping object and a writable view (`CreateFileMappingW`,
//! `MapViewOfFile`), `FlushViewOfFile` then `FlushFileBuffers` to write back, `VirtualUnlock` on the
//! unlocked view to drop its pages from the working set (documented: on pages that are not locked it
//! removes them from the working set, and fails with ERROR_NOT_LOCKED, which is the expected answer),
//! and `PrefetchVirtualMemory` to read ahead.

use std::fs::File;
use std::io;
use std::os::windows::io::AsRawHandle;
use std::ptr::NonNull;

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_NOT_LOCKED, HANDLE};
use windows_sys::Win32::Storage::FileSystem::FlushFileBuffers;
use windows_sys::Win32::System::Memory::{
    CreateFileMappingW, FlushViewOfFile, MapViewOfFile, PrefetchVirtualMemory, UnmapViewOfFile, VirtualUnlock, FILE_MAP_READ, FILE_MAP_WRITE,
    MEMORY_MAPPED_VIEW_ADDRESS, PAGE_READWRITE, WIN32_MEMORY_RANGE_ENTRY,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

/// The file mapping object, closed after the view is unmapped.
pub(super) struct Map(HANDLE);

pub(super) fn map(file: &File, len: usize) -> io::Result<(Map, NonNull<u8>)> {
    let size = len as u64;
    // SAFETY: a mapping object over an open file handle, sized to `len`.
    let object = unsafe { CreateFileMappingW(file.as_raw_handle() as HANDLE, std::ptr::null(), PAGE_READWRITE, (size >> 32) as u32, size as u32, std::ptr::null()) };
    if object.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a view of the whole mapping object just made.
    let view = unsafe { MapViewOfFile(object, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, len) };
    match NonNull::new(view.Value.cast::<u8>()) {
        Some(base) => Ok((Map(object), base)),
        None => {
            let err = io::Error::last_os_error();
            // SAFETY: the object is ours and has no view.
            unsafe { CloseHandle(object) };
            Err(err)
        }
    }
}

pub(super) fn flush(_map: &Map, file: &File, base: NonNull<u8>, len: usize) -> io::Result<()> {
    // SAFETY: `base..base+len` is a live view owned by the caller.
    if unsafe { FlushViewOfFile(base.as_ptr().cast(), len) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: an open file handle.
    if unsafe { FlushFileBuffers(file.as_raw_handle() as HANDLE) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub(super) fn release(base: NonNull<u8>, len: usize) -> io::Result<()> {
    // SAFETY: as above; the pages were just written back, so dropping them loses nothing.
    if unsafe { VirtualUnlock(base.as_ptr().cast(), len) } == 0 {
        let err = io::Error::last_os_error();
        if err.raw_os_error() != Some(ERROR_NOT_LOCKED as i32) {
            return Err(err);
        }
    }
    Ok(())
}

pub(super) fn prefetch(base: NonNull<u8>, len: usize) -> io::Result<()> {
    let range = WIN32_MEMORY_RANGE_ENTRY { VirtualAddress: base.as_ptr().cast(), NumberOfBytes: len };
    // SAFETY: one range of a live view of this process.
    if unsafe { PrefetchVirtualMemory(GetCurrentProcess(), 1, &range, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub(super) fn unmap(map: &Map, base: NonNull<u8>, _len: usize) {
    // SAFETY: the view and the object are owned by the region being dropped; nothing reads them after.
    unsafe {
        UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS { Value: base.as_ptr().cast() });
        CloseHandle(map.0);
    }
}
