use super::Unobservable;

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

/// Closes a descriptor on every path.
struct Fd(libc::c_int);

impl Drop for Fd {
    fn drop(&mut self) {
        // SAFETY: the descriptor came from a successful kqueue() and is closed once, here.
        unsafe { libc::close(self.0) };
    }
}

/// macOS: a kqueue `EVFILT_PROC` filter with `NOTE_EXIT`; registration on a pid that already
/// exited answers ESRCH, which is that exit.
pub(super) fn wait_exit(pid: u32) -> Result<(), Unobservable> {
    // SAFETY: kqueue() takes no arguments and returns a new descriptor or -1.
    let kq = unsafe { libc::kqueue() };
    if kq < 0 {
        return Err(refused(pid, std::io::Error::last_os_error()));
    }
    let _kq = Fd(kq);
    let change = libc::kevent {
        ident: pid as libc::uintptr_t,
        filter: libc::EVFILT_PROC,
        flags: libc::EV_ADD | libc::EV_ONESHOT,
        fflags: libc::NOTE_EXIT,
        data: 0,
        udata: std::ptr::null_mut(),
    };
    loop {
        // SAFETY: an all-zero kevent is a valid value for the kernel to overwrite.
        let mut fired: libc::kevent = unsafe { std::mem::zeroed() };
        // SAFETY: one change and one event slot, both valid for the call; no timeout (blocks).
        let n = unsafe { libc::kevent(kq, &change, 1, &mut fired, 1, std::ptr::null()) };
        if n >= 1 {
            if fired.flags & libc::EV_ERROR == 0 {
                return Ok(());
            }
            // A registration error is reported in-band: `data` holds its errno.
            let errno = fired.data as i32;
            return if errno == libc::ESRCH { Ok(()) } else { Err(refused(pid, std::io::Error::from_raw_os_error(errno))) };
        }
        let error = std::io::Error::last_os_error();
        match error.raw_os_error() {
            Some(libc::EINTR) => continue, // a signal woke the wait; re-adding the filter is idempotent
            Some(libc::ESRCH) => return Ok(()),
            _ => return Err(refused(pid, error)),
        }
    }
}

fn refused(pid: u32, error: std::io::Error) -> Unobservable {
    Unobservable::Refused { pid, error: error.to_string() }
}
