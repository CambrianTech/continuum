use super::TerminationAuthority;
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER};
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE};

pub(super) fn may_terminate(pid: u32) -> TerminationAuthority {
    // SAFETY: OpenProcess is a capability query here; the handle is closed at once and nothing
    // is terminated by this call.
    let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
    if !handle.is_null() {
        // SAFETY: `handle` came from a successful OpenProcess and is closed once, here.
        unsafe { CloseHandle(handle) };
        return TerminationAuthority::Granted;
    }
    let error = std::io::Error::last_os_error();
    match error.raw_os_error() {
        // Windows spells absence as an invalid pid parameter where Unix answers ESRCH.
        Some(code) if code == ERROR_INVALID_PARAMETER as i32 => TerminationAuthority::Absent,
        Some(code) if code == ERROR_ACCESS_DENIED as i32 => TerminationAuthority::Refused(error),
        _ => TerminationAuthority::Unknown(error),
    }
}
