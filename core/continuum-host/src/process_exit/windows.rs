use super::{Armless, Unobservable};
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, HANDLE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject, INFINITE, PROCESS_SYNCHRONIZE};

/// Windows has no zombie: a retained terminated handle is the presence probe's to read.
pub(super) fn exited_unreaped(_pid: u32) -> bool {
    false
}


/// A synchronize-only process handle: bound to the process that held the pid when it was opened.
pub(super) struct Armed {
    handle: HANDLE,
    pid: u32,
}

// SAFETY: a process handle is a kernel object reference usable from any thread; it is waited on and closed by
// whichever one thread owns this value.
unsafe impl Send for Armed {}

impl Drop for Armed {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful OpenProcess and is closed once, here.
        unsafe { CloseHandle(self.handle) };
    }
}

/// Windows: open the handle WITHOUT waiting; a pid no process holds answers ERROR_INVALID_PARAMETER, which is
/// that exit.
pub(super) fn arm(pid: u32) -> Result<Armed, Armless> {
    // SAFETY: a synchronize-only handle for exactly this pid, closed by Armed's Drop.
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        let error = std::io::Error::last_os_error();
        return Err(if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
            Armless::AlreadyExited
        } else {
            Armless::Unobservable(Unobservable::Refused { pid, error: error.to_string() })
        });
    }
    Ok(Armed { handle, pid })
}

/// Block until the armed process ends (its handle is signalled).
pub(super) fn wait(armed: Armed) -> Result<(), Unobservable> {
    // SAFETY: waits on the valid handle Armed owns until the process ends.
    let state = unsafe { WaitForSingleObject(armed.handle, INFINITE) };
    if state == WAIT_OBJECT_0 {
        Ok(())
    } else {
        Err(Unobservable::Refused { pid: armed.pid, error: std::io::Error::last_os_error().to_string() })
    }
}
