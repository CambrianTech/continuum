use super::{terminate_windows_handle, windows_handle_has_exited};
use continuum_kernel::system_resources::process_tree::ProcessIdentity;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::process::{Child, Stdio};
use windows_sys::Win32::Foundation::WAIT_TIMEOUT;
use windows_sys::Win32::System::Threading::{
    OpenProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
};

fn waiting_child() -> std::io::Result<Child> {
    airc_core::process::background("cmd.exe")
        .args(["/D", "/C", "set /p fixture_input="])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

// A real child exits after the caller's live check but before the native kill.
// Its retained handle, not a second PID lookup, is the proof it is already gone.
#[test]
fn exit_between_preflight_and_termination_is_already_gone() -> Result<(), Box<dyn std::error::Error>>
{
    let mut child = waiting_child()?;
    let who = ProcessIdentity {
        pid: child.id(),
        start_time: 0,
    };
    // SAFETY: Child owns this valid process handle throughout the test.
    let before = unsafe { WaitForSingleObject(child.as_raw_handle(), 0) };
    drop(child.stdin.take());
    child.wait()?;
    assert_eq!(before, WAIT_TIMEOUT, "child was live at preflight");
    assert_eq!(terminate_windows_handle(&child, who), Ok(false));
    Ok(())
}

// A live process with no termination right must still refuse. A waitable handle
// alone never grants termination authority or counts as proof of exit.
#[test]
fn live_process_access_denial_remains_a_refusal() -> Result<(), Box<dyn std::error::Error>> {
    let mut child = waiting_child()?;
    let who = ProcessIdentity {
        pid: child.id(),
        start_time: 0,
    };
    // SAFETY: scalar flags/PID; deliberately omit PROCESS_TERMINATE.
    let raw = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            child.id(),
        )
    };
    let observed = if raw.is_null() {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        // SAFETY: the successful OpenProcess call transferred one owned handle.
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        terminate_windows_handle(&handle, who)
    };
    let still_live = child.try_wait()?.is_none();
    drop(child.stdin.take());
    child.wait()?;
    assert!(!raw.is_null(), "fixture must acquire a waitable handle");
    assert!(observed
        .is_err_and(|error| error.contains("TerminateProcess") && error.contains("os error 5")));
    assert!(still_live, "denial must leave the live child intact");
    Ok(())
}

#[test]
fn observation_without_synchronize_right_is_unknown() -> Result<(), Box<dyn std::error::Error>> {
    let mut child = waiting_child()?;
    // SAFETY: query-only access deliberately omits the right required to wait.
    let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, child.id()) };
    let observed = if raw.is_null() {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        // SAFETY: successful OpenProcess transferred one owned handle.
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        windows_handle_has_exited(&handle)
    };
    let still_live = child.try_wait()?.is_none();
    drop(child.stdin.take());
    child.wait()?;
    assert!(!raw.is_null(), "fixture must acquire a query-only handle");
    assert!(observed.is_err_and(|error| error.contains("WaitForSingleObject")));
    assert!(still_live);
    Ok(())
}
