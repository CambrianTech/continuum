/// A zombie still answers `kill(pid, 0)`, but `proc_pidinfo(PROC_PIDTBSDINFO)` fails for it with
/// `ESRCH`. Both are required: `ESRCH` alone also describes a pid that never existed, and any
/// other failure (another user's process: `EPERM`) is not proof of exit.
pub(super) fn exited_unreaped(pid: u32) -> bool {
    // SAFETY: signal 0 performs only the existence and permission check; nothing is sent.
    if unsafe { libc::kill(pid as libc::pid_t, 0) } != 0 {
        return false;
    }
    // SAFETY: proc_bsdinfo is plain old data; proc_pidinfo writes at most `size` bytes into it.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    // SAFETY: a read-only query for exactly this pid into a buffer of the stated size.
    let n = unsafe { libc::proc_pidinfo(pid as libc::c_int, libc::PROC_PIDTBSDINFO, 0, std::ptr::addr_of_mut!(info).cast(), size) };
    n <= 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
}
