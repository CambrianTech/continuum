//! Whether this process may terminate a pid, asked before a stop drains anything: a caller that
//! could not end the process afterwards must refuse first. What differs by OS is the call: Unix
//! sends signal 0; Windows opens a terminate handle. Only positive evidence proves a process gone.

#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(not(any(unix, windows)), path = "other.rs")]
mod os;

#[cfg_attr(windows, path = "held_windows.rs")]
#[cfg_attr(not(windows), path = "held_other.rs")]
mod held;

/// An individual process held across inspection and termination. Acquiring this
/// capability never drains or terminates anything; callers own that policy.
pub struct HeldProcess(held::Handle);

impl HeldProcess {
    pub fn take(pid: u32) -> std::io::Result<Self> {
        validate_pid(pid)?;
        held::Handle::take(pid).map(Self)
    }

    pub fn image(&self) -> std::io::Result<std::path::PathBuf> {
        self.0.image()
    }

    /// Terminate through the same handle used by `image`, never by a second pid lookup.
    pub fn terminate(self, exit_code: u32) -> std::io::Result<()> {
        self.0.terminate(exit_code)
    }
}

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

/// Ask whether this process may terminate `pid`.
pub fn may_terminate(pid: u32) -> TerminationAuthority {
    if let Err(error) = validate_pid(pid) {
        return TerminationAuthority::Unknown(error);
    }
    os::may_terminate(pid)
}

fn validate_pid(pid: u32) -> std::io::Result<()> {
    // Zero names a process group on Unix, and a narrowing cast of a larger value
    // can name another group. This capability answers only for individual pids.
    if pid == 0 || i32::try_from(pid).is_err() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "termination authority requires a positive process id representable by pid_t",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_group_and_overflow_ids_never_prove_authority_or_absence() {
        for pid in [0, 1_u32 << 31, u32::MAX] {
            assert!(matches!(may_terminate(pid), TerminationAuthority::Unknown(error)
                if error.kind() == std::io::ErrorKind::InvalidInput));
        }
    }

    // what this catches (Codex on #6042): a probe that reads an unknown OS answer as "gone", or an
    // unused pid as terminable. This process may terminate itself; no process has the unused pid.
    #[test]
    fn this_process_is_terminable_and_an_unused_pid_is_absent() {
        assert!(matches!(may_terminate(std::process::id()), TerminationAuthority::Granted));
        assert!(matches!(may_terminate(u32::MAX >> 2), TerminationAuthority::Absent));
    }
}
