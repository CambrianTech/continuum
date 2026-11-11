//! Whether the OS kernel itself knows a pid, asked when the process table did not list it: the
//! one question a liveness probe needs POSITIVE evidence for before it may call a process gone.
//! What differs by OS is the call: Unix asks `kill(pid, 0)`; Windows opens a query handle.

#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(not(any(unix, windows)), path = "other.rs")]
mod os;

/// What the kernel answered about one pid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// The kernel knows a process with this pid (possibly another user's).
    Present,
    /// POSITIVELY absent: the kernel said no such process, or the process has exited and only
    /// awaits its reaper.
    Absent,
    /// The kernel could not answer either way. Never read as absence.
    Unreadable,
}

/// Ask the kernel about `pid`.
pub fn kernel_says(pid: u32) -> Presence {
    os::kernel_says(pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a probe that reads its own running process as gone, or a never-used pid
    // as present, would release a live owner's resources or hold a dead one's forever.
    #[test]
    fn the_kernel_knows_this_process_and_not_an_unused_pid() {
        assert_eq!(kernel_says(std::process::id()), Presence::Present, "this process runs");
        assert_eq!(kernel_says(u32::MAX >> 2), Presence::Absent, "no process has this pid");
    }
}
