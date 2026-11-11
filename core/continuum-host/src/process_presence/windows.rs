use super::Presence;
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE};

pub(super) fn kernel_says(pid: u32) -> Presence {
    // SAFETY: a query-only handle for exactly this pid, closed once below.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE, 0, pid) };
    if !handle.is_null() {
        // A retained handle can keep a terminated process object open after its PID
        // disappears from the process table. Handle existence alone is not liveness.
        // SAFETY: query the valid process handle without waiting or changing it.
        let state = unsafe { WaitForSingleObject(handle, 0) };
        // SAFETY: `handle` came from a successful OpenProcess and is closed once, here.
        unsafe { CloseHandle(handle) };
        return match state {
            WAIT_OBJECT_0 => Presence::Absent,
            WAIT_TIMEOUT => Presence::Present,
            _ => Presence::Unreadable,
        };
    }
    // SAFETY: reads this thread's last error, set by the failed OpenProcess above.
    if unsafe { GetLastError() } == ERROR_INVALID_PARAMETER {
        Presence::Absent
    } else {
        Presence::Unreadable
    }
}
