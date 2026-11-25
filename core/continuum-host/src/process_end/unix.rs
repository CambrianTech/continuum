use super::{Incarnation, Refusal};

pub(super) fn kill(pid: u32) {
    let Ok(pid) = libc::pid_t::try_from(pid) else { return };
    // SAFETY: a signal to a pid; a race where it already exited returns ESRCH, which is fine here.
    unsafe {
        libc::kill(pid, libc::SIGKILL);
    }
}

/// Unix has no handle that pins an incarnation, so the caller's process-table check is the identity
/// check, made immediately before the signal.
pub(super) fn end(incarnation: Incarnation, still_present: impl FnOnce(Incarnation) -> bool) -> Result<bool, Refusal> {
    if !still_present(incarnation) {
        return Ok(false);
    }
    let Ok(pid) = libc::pid_t::try_from(incarnation.pid) else { return Ok(false) };
    // SAFETY: a signal to the incarnation the caller verified immediately above.
    if unsafe { libc::kill(pid, libc::SIGKILL) } == 0 {
        return Ok(true);
    }
    let source = std::io::Error::last_os_error();
    if source.raw_os_error() == Some(libc::ESRCH) {
        return Ok(false); // it exited between the check and the signal
    }
    Err(Refusal::Os { call: "SIGKILL", incarnation, source })
}
