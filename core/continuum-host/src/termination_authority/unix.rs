use super::TerminationAuthority;

pub(super) fn may_terminate(pid: u32) -> TerminationAuthority {
    // SAFETY: signal 0 performs only the permission and existence check; nothing is sent.
    if unsafe { libc::kill(pid as libc::pid_t, 0) } == 0 {
        return TerminationAuthority::Granted;
    }
    let error = std::io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::ESRCH) => TerminationAuthority::Absent,
        Some(libc::EPERM) => TerminationAuthority::Refused(error),
        _ => TerminationAuthority::Unknown(error),
    }
}
