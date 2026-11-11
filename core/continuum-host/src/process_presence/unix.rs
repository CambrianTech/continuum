use super::Presence;

pub(super) fn kernel_says(pid: u32) -> Presence {
    // SAFETY: signal 0 performs only the existence and permission check; nothing is sent.
    let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if rc == 0 {
        // a zombie answers kill(0) too: exited, only awaiting its reaper (card e4ef7780)
        return if crate::process_exit::exited_unreaped(pid) { Presence::Absent } else { Presence::Present };
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::ESRCH) => Presence::Absent,
        Some(libc::EPERM) => Presence::Present, // another user's process
        _ => Presence::Unreadable,
    }
}
