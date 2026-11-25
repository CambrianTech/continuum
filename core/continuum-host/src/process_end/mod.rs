//! Ending a process: the one place the kill differs by OS. Unix sends `SIGKILL`. Windows pins the
//! incarnation with a handle, checks its creation time, and terminates that handle, never a second
//! PID lookup; a best-effort kill ends the child tree with `taskkill /F /T`.

#[cfg_attr(any(target_os = "macos", target_os = "linux"), path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(not(any(target_os = "macos", target_os = "linux", windows)), path = "other.rs")]
mod os;

/// One incarnation of a process: its pid and its start time (Unix seconds), as the process table
/// recorded it. A pid alone can name a later process; the pair cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Incarnation {
    pub pid: u32,
    pub start_time: u64,
}

/// Force-end `pid` (on Windows, with its child tree). Best effort: one that already exited is fine.
pub fn kill(pid: u32) {
    os::kill(pid);
}

/// End exactly `incarnation`. `Ok(true)`: the kill was sent. `Ok(false)`: it is already gone, and
/// whatever holds its pid now is left alone. `still_present` is the caller's identity check,
/// run where the OS has no handle that pins an incarnation (Unix); Windows pins it with a handle
/// and checks its creation time itself, so it never reads the process table for this.
pub fn end(incarnation: Incarnation, still_present: impl FnOnce(Incarnation) -> bool) -> Result<bool, Refusal> {
    os::end(incarnation, still_present)
}

/// Why ending an incarnation was refused. A refusal never means it ended.
#[derive(Debug)]
pub enum Refusal {
    /// The OS would not open, inspect or signal it (access denied, a fault): which call, and its error.
    Os { call: &'static str, incarnation: Incarnation, source: std::io::Error },
    /// Its creation time precedes the Unix epoch: no time this record could name.
    BeforeEpoch { incarnation: Incarnation },
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Os { call, incarnation, source } => write!(f, "{call} pid={} started={}: {source}", incarnation.pid, incarnation.start_time),
            Self::BeforeEpoch { incarnation } => write!(f, "pid={}: its creation time precedes the Unix epoch", incarnation.pid),
        }
    }
}

impl std::error::Error for Refusal {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Os { source, .. } => Some(source),
            Self::BeforeEpoch { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: an end that acts on a pid the caller's identity check says is no longer the
    // recorded incarnation (on Unix the check is the caller's; on Windows the handle's creation time).
    #[test]
    fn an_incarnation_that_is_gone_is_never_signalled() {
        let gone = Incarnation { pid: i32::MAX as u32, start_time: 1 };
        assert!(matches!(end(gone, |_| false), Ok(false)), "a gone incarnation is reported gone, never signalled");
    }
}
