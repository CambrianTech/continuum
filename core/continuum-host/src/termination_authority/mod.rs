//! Whether this process may terminate a pid, asked before a stop drains anything: a caller that
//! could not end the process afterwards must refuse first. What differs by OS is the call: Unix
//! sends signal 0; Windows opens a terminate handle. Only positive evidence proves a process gone.

#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(not(any(unix, windows)), path = "other.rs")]
mod os;

/// What the OS answered about terminating one pid.
#[derive(Debug)]
pub enum TerminationAuthority {
    /// This process may terminate it.
    Granted,
    /// POSITIVELY absent: the OS said no such process (Unix `ESRCH`, Windows
    /// `ERROR_INVALID_PARAMETER`). Nothing to terminate.
    Absent,
    /// The process exists and this caller may not terminate it.
    Refused(std::io::Error),
    /// The OS answered something else; neither authority nor absence is proven.
    Unknown(std::io::Error),
}

/// Ask whether this process may terminate `pid`: one process, never a group, by its type.
pub fn may_terminate(pid: crate::process_id::ProcessId) -> TerminationAuthority {
    os::may_terminate(pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (Codex on #6042): a probe that reads an unknown OS answer as "gone", or an
    // unused pid as terminable. This process may terminate itself; no process has the unused pid.
    #[test]
    fn this_process_is_terminable_and_an_unused_pid_is_absent() {
        use crate::process_id::ProcessId;
        let this = ProcessId::try_from(std::process::id()).expect("this process's id is a pid"); // expect: the running process has a positive pid
        let unused = ProcessId::new(i32::MAX >> 1).expect("a positive literal"); // expect: positive by construction
        assert!(matches!(may_terminate(this), TerminationAuthority::Granted));
        assert!(matches!(may_terminate(unused), TerminationAuthority::Absent));
    }
}
