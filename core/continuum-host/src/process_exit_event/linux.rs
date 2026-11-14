use super::ExitWaitError;

/// Closes a descriptor on every path.
struct Fd(libc::c_int);

impl Drop for Fd {
    fn drop(&mut self) {
        // SAFETY: the descriptor came from a successful pidfd_open and is closed once, here.
        unsafe { libc::close(self.0) };
    }
}

/// Linux: a pidfd turns readable when its process exits (zombie included); a pid already gone
/// answers ESRCH, which is that exit.
pub(super) fn wait_exit(pid: u32) -> Result<(), ExitWaitError> {
    // SAFETY: pidfd_open(pid, 0) only creates a descriptor referring to the process.
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid as libc::pid_t, 0) };
    if fd < 0 {
        let error = std::io::Error::last_os_error();
        return match error.raw_os_error() {
            Some(libc::ESRCH) => Ok(()),
            Some(libc::ENOSYS) => Err(ExitWaitError::Unsupported),
            _ => Err(ExitWaitError::Refused { pid, error: error.to_string() }),
        };
    }
    let fd = fd as libc::c_int;
    let _fd = Fd(fd);
    let mut watched = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
    loop {
        // SAFETY: one valid pollfd; -1 waits for the event with no timeout.
        let n = unsafe { libc::poll(&mut watched, 1, -1) };
        if n >= 1 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::EINTR) {
            return Err(ExitWaitError::Refused { pid, error: error.to_string() });
        }
    }
}
