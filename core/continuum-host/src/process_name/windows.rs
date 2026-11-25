use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};

/// `pid`'s executable path from the kernel, through a query-only handle (no `tasklist` spawned).
pub(super) fn name_of(pid: u32) -> Option<String> {
    // SAFETY: a query-only handle for exactly this pid, closed once below.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let mut path = [0u16; 32_768]; // the longest Win32 path, so a full image name always fits
    let mut len = path.len() as u32;
    // SAFETY: `handle` is valid; the call writes at most `len` UTF-16 units into `path` and sets `len`.
    let ok = unsafe { QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, path.as_mut_ptr(), &mut len) };
    // SAFETY: `handle` came from a successful OpenProcess and is closed once, here.
    unsafe { CloseHandle(handle) };
    (ok != 0).then(|| String::from_utf16_lossy(&path[..len as usize]))
}
