<<<<<<< HEAD
use super::{Armless, Unobservable};

=======
>>>>>>> origin/main
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
<<<<<<< HEAD

/// Closes a descriptor on every path.
struct Fd(libc::c_int);

impl Drop for Fd {
    fn drop(&mut self) {
        // SAFETY: the descriptor came from a successful kqueue() and is closed once, here.
        unsafe { libc::close(self.0) };
    }
}

/// A kqueue already holding the `EVFILT_PROC` / `NOTE_EXIT` registration for one process: bound to the process
/// that held the pid when it was armed, whatever later holds the number.
pub(super) struct Armed {
    kq: Fd,
    pid: u32,
}

/// macOS: register `NOTE_EXIT` for `pid` WITHOUT waiting. `EV_RECEIPT` makes the kernel answer the registration
/// itself as one event (`EV_ERROR` with `data` = 0 on success, or its errno), so the binding is acknowledged
/// before it is trusted. ESRCH (a pid with no process) is that process's exit.
pub(super) fn arm(pid: u32) -> Result<Armed, Armless> {
    // SAFETY: kqueue() takes no arguments and returns a new descriptor or -1.
    let kq = unsafe { libc::kqueue() };
    if kq < 0 {
        return Err(Armless::Unobservable(refused(pid, std::io::Error::last_os_error())));
    }
    let kq = Fd(kq);
    let change = libc::kevent {
        ident: pid as libc::uintptr_t,
        filter: libc::EVFILT_PROC,
        flags: libc::EV_ADD | libc::EV_ONESHOT | libc::EV_RECEIPT,
        fflags: libc::NOTE_EXIT,
        data: 0,
        udata: std::ptr::null_mut(),
    };
    let now = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    loop {
        // SAFETY: an all-zero kevent is a valid value for the kernel to overwrite.
        let mut receipt: libc::kevent = unsafe { std::mem::zeroed() };
        // SAFETY: one change and one receipt slot, both valid for the call; a zero timeout never blocks.
        let n = unsafe { libc::kevent(kq.0, &change, 1, &mut receipt, 1, &now) };
        if n < 0 {
            let error = std::io::Error::last_os_error();
            match error.raw_os_error() {
                Some(libc::EINTR) => continue,
                Some(libc::ESRCH) => return Err(Armless::AlreadyExited),
                _ => return Err(Armless::Unobservable(refused(pid, error))),
            }
        }
        if n == 0 || receipt.flags & libc::EV_ERROR == 0 {
            return Err(Armless::Unobservable(refused(pid, std::io::Error::other("the kernel acknowledged no registration"))));
        }
        return match receipt.data as i32 {
            0 => Ok(Armed { kq, pid }),
            libc::ESRCH => Err(Armless::AlreadyExited),
            errno => Err(Armless::Unobservable(refused(pid, std::io::Error::from_raw_os_error(errno)))),
        };
    }
}

/// Block until the armed process exits, consuming the existing registration (no second `EV_ADD`).
pub(super) fn wait(armed: Armed) -> Result<(), Unobservable> {
    loop {
        // SAFETY: an all-zero kevent is a valid value for the kernel to overwrite.
        let mut fired: libc::kevent = unsafe { std::mem::zeroed() };
        // SAFETY: no changes, one event slot; no timeout (blocks until the registered exit).
        let n = unsafe { libc::kevent(armed.kq.0, std::ptr::null(), 0, &mut fired, 1, std::ptr::null()) };
        if n >= 1 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::EINTR) {
            return Err(refused(armed.pid, error));
        }
    }
}

fn refused(pid: u32, error: std::io::Error) -> Unobservable {
    Unobservable::Refused { pid, error: error.to_string() }
}
=======
>>>>>>> origin/main
