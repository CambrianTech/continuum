use super::Unobservable;
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject, INFINITE, PROCESS_SYNCHRONIZE};

/// Windows has no zombie: a retained terminated handle is the presence probe's to read.
pub(super) fn exited_unreaped(_pid: u32) -> bool {
    false
}


/// Windows: the process handle is signalled when the process ends; a pid no process holds
/// answers ERROR_INVALID_PARAMETER, which is that exit.
pub(super) fn wait_exit(pid: u32) -> Result<(), Unobservable> {
    // SAFETY: a synchronize-only handle for exactly this pid, closed once below.
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        let error = std::io::Error::last_os_error();
        return if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
            Ok(())
        } else {
            Err(Unobservable::Refused { pid, error: error.to_string() })
        };
    }
    // SAFETY: waits on the valid handle above until the process ends.
    let state = unsafe { WaitForSingleObject(handle, INFINITE) };
    let error = std::io::Error::last_os_error();
    // SAFETY: `handle` came from a successful OpenProcess and is closed once, here.
    unsafe { CloseHandle(handle) };
    if state == WAIT_OBJECT_0 {
        Ok(())
    } else {
        Err(Unobservable::Refused { pid, error: error.to_string() })
    }
}
