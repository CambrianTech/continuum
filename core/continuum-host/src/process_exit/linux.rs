use super::{Armless, Unobservable};

/// Linux keeps a zombie in /proc, where the process table reader sees its status: nothing to add.
pub(super) fn exited_unreaped(_pid: u32) -> bool {
    false
}


/// Closes a descriptor on every path.
struct Fd(libc::c_int);

impl Drop for Fd {
    fn drop(&mut self) {
        // SAFETY: the descriptor came from a successful pidfd_open and is closed once, here.
        unsafe { libc::close(self.0) };
    }
}

/// A pidfd: bound to the process that held the pid when it was opened, whatever later holds the number.
pub(super) struct Armed {
    fd: Fd,
    pid: u32,
}

/// Linux: open the pidfd WITHOUT waiting; a pid with no process answers ESRCH, which is that exit.
pub(super) fn arm(pid: u32) -> Result<Armed, Armless> {
    // SAFETY: pidfd_open(pid, 0) only creates a descriptor referring to the process.
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid as libc::pid_t, 0) };
    if fd < 0 {
        let error = std::io::Error::last_os_error();
        return Err(match error.raw_os_error() {
            Some(libc::ESRCH) => Armless::AlreadyExited,
            Some(libc::ENOSYS) => Armless::Unobservable(Unobservable::Unsupported),
            _ => Armless::Unobservable(Unobservable::Refused { pid, error: error.to_string() }),
        });
    }
    Ok(Armed { fd: Fd(fd as libc::c_int), pid })
}

/// Block until the pidfd turns readable: its process exited (zombie included).
pub(super) fn wait(armed: Armed) -> Result<(), Unobservable> {
    let pid = armed.pid;
    let mut watched = libc::pollfd { fd: armed.fd.0, events: libc::POLLIN, revents: 0 };
    loop {
        // SAFETY: one valid pollfd; -1 waits for the event with no timeout.
        let n = unsafe { libc::poll(&mut watched, 1, -1) };
        if n >= 1 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::EINTR) {
            return Err(Unobservable::Refused { pid, error: error.to_string() });
        }
    }
}
