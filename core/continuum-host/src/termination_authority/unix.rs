use super::TerminationAuthority;

pub(super) fn may_terminate(pid: crate::process_id::ProcessId) -> TerminationAuthority {
    // SAFETY: signal 0 performs only the permission and existence check; nothing is sent. The id
    // is positive by its type, so it addresses one process, never a group.
    if unsafe { libc::kill(pid.get(), 0) } == 0 {
        return TerminationAuthority::Granted;
    }
    let error = std::io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::ESRCH) => TerminationAuthority::Absent,
        Some(libc::EPERM) => TerminationAuthority::Refused(error),
        _ => TerminationAuthority::Unknown(error),
    }
}
