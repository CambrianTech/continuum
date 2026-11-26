use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::CommandExt;

use windows_sys::Win32::Foundation::{ERROR_INVALID_PARAMETER, FILETIME, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, TerminateProcess, WaitForSingleObject, CREATE_NO_WINDOW, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
};

use super::{Incarnation, Refusal};

/// No SIGKILL on Windows: `taskkill /F /T` ends the process and its child tree. Best effort.
pub(super) fn kill(pid: u32) {
    let _ = std::process::Command::new("taskkill").args(["/F", "/T", "/PID", &pid.to_string()]).creation_flags(CREATE_NO_WINDOW).output();
}

/// Pin the incarnation before inspecting or signalling it: a PID lookup could otherwise hit a
/// replacement between the check and the kill. The handle's creation time is the identity check, so
/// the caller's process-table check is not needed here.
pub(super) fn end(incarnation: Incarnation, _still_present: impl FnOnce(Incarnation) -> bool) -> Result<bool, Refusal> {
    // SAFETY: scalar PID and access flags; the returned handle is owned below.
    let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, 0, incarnation.pid) };
    if raw.is_null() {
        let source = std::io::Error::last_os_error();
        if source.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
            return Ok(false);
        }
        return Err(Refusal::Os { call: "OpenProcess", incarnation, source });
    }
    // SAFETY: OpenProcess returned a new owned handle above.
    let process = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut creation = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let (mut exit, mut kernel, mut user) = (creation, creation, creation);
    // SAFETY: owned live handle and four valid writable FILETIME pointers.
    if unsafe { GetProcessTimes(process.as_raw_handle(), &mut creation, &mut exit, &mut kernel, &mut user) } == 0 {
        return Err(Refusal::Os { call: "GetProcessTimes", incarnation, source: std::io::Error::last_os_error() });
    }
    let ticks = (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime);
    let started = (ticks / 10_000_000).checked_sub(11_644_473_600).ok_or(Refusal::BeforeEpoch { incarnation })?;
    // SAFETY: the owned handle stays open through the nonblocking wait.
    if started != incarnation.start_time || unsafe { WaitForSingleObject(process.as_raw_handle(), 0) } == WAIT_OBJECT_0 {
        return Ok(false);
    }
    terminate(&process, incarnation)
}

/// Terminate the held, creation-time-verified incarnation. It can exit between the preflight wait and
/// the kill: only a signalled handle proves that race; a live or failed wait keeps the refusal,
/// including genuine access denial. Never re-open a PID.
fn terminate(process: &impl AsRawHandle, incarnation: Incarnation) -> Result<bool, Refusal> {
    // SAFETY: terminate only this held incarnation.
    if unsafe { TerminateProcess(process.as_raw_handle(), 1) } == 0 {
        let source = std::io::Error::last_os_error();
        // SAFETY: the same owned handle remains open through this wait.
        if unsafe { WaitForSingleObject(process.as_raw_handle(), 0) } == WAIT_OBJECT_0 {
            return Ok(false);
        }
        return Err(Refusal::Os { call: "TerminateProcess", incarnation, source });
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Child, Stdio};
    use windows_sys::Win32::Foundation::WAIT_TIMEOUT;

    fn waiting_child() -> std::io::Result<Child> {
        std::process::Command::new("cmd.exe")
            .args(["/D", "/C", "set /p fixture_input="])
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    }

    // what this catches: a child that exits after the caller's live check but before the native kill.
    // Its retained handle, not a second PID lookup, is the proof it is already gone.
    #[test]
    fn exit_between_preflight_and_termination_is_already_gone() -> Result<(), Box<dyn std::error::Error>> {
        let mut child = waiting_child()?;
        let incarnation = Incarnation { pid: child.id(), start_time: 0 };
        // SAFETY: Child owns this valid process handle throughout the test.
        let before = unsafe { WaitForSingleObject(child.as_raw_handle(), 0) };
        drop(child.stdin.take());
        child.wait()?;
        assert_eq!(before, WAIT_TIMEOUT, "child was live at preflight");
        assert!(matches!(terminate(&child, incarnation), Ok(false)));
        Ok(())
    }

    // what this catches: a live process with no termination right read as ended. A waitable handle
    // alone never grants termination authority or counts as proof of exit.
    #[test]
    fn live_process_access_denial_remains_a_refusal() -> Result<(), Box<dyn std::error::Error>> {
        let mut child = waiting_child()?;
        let incarnation = Incarnation { pid: child.id(), start_time: 0 };
        // SAFETY: scalar flags and PID; deliberately omit PROCESS_TERMINATE.
        let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE, 0, child.id()) };
        let observed = if raw.is_null() {
            None
        } else {
            // SAFETY: the successful OpenProcess call transferred one owned handle.
            let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
            Some(terminate(&handle, incarnation))
        };
        let still_live = child.try_wait()?.is_none();
        drop(child.stdin.take());
        child.wait()?;
        assert!(!raw.is_null(), "fixture must acquire a waitable handle");
        assert!(matches!(observed, Some(Err(Refusal::Os { call: "TerminateProcess", ref source, .. })) if source.raw_os_error() == Some(5)));
        assert!(still_live, "denial must leave the live child intact");
        Ok(())
    }
}
