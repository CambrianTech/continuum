//! A pid that names exactly one process. Zero and negative ids name process groups on Unix,
//! and an id past `pid_t` wraps into one when narrowed, so neither can be built: every OS call
//! that takes a `ProcessId` addresses one process by construction.

use std::num::NonZeroI32;

/// One process's id: positive and representable as the OS's `pid_t`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcessId(NonZeroI32);

impl ProcessId {
    /// The id, when it names one process.
    pub fn new(pid: i32) -> Option<Self> {
        NonZeroI32::new(pid).filter(|pid| pid.get() > 0).map(Self)
    }

    /// The id as the OS's signed `pid_t`.
    pub fn get(self) -> i32 {
        self.0.get()
    }

    /// The id as an unsigned process number (Windows, process tables). Positive by construction.
    pub fn unsigned(self) -> u32 {
        self.0.get().unsigned_abs()
    }
}

/// Why an id names no single process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum NotAProcess {
    /// Zero or negative: a process group, never one process.
    #[error("pid {0} names a process group, not one process")]
    GroupId(i64),
    /// Past `pid_t`: narrowing it would name another process or group.
    #[error("pid {0} does not fit the OS's pid_t")]
    OutOfRange(u32),
}

impl TryFrom<i32> for ProcessId {
    type Error = NotAProcess;
    fn try_from(pid: i32) -> Result<Self, Self::Error> {
        Self::new(pid).ok_or(NotAProcess::GroupId(i64::from(pid)))
    }
}

impl TryFrom<u32> for ProcessId {
    type Error = NotAProcess;
    fn try_from(pid: u32) -> Result<Self, Self::Error> {
        let signed = i32::try_from(pid).map_err(|_| NotAProcess::OutOfRange(pid))?;
        Self::try_from(signed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (Codex on #6042): an id that addresses a process group (0, negatives) or
    // wraps into one when narrowed (past i32::MAX) reaching an OS call meant for one process.
    #[test]
    fn only_positive_pid_t_ids_name_a_process() {
        assert!(ProcessId::new(0).is_none());
        assert!(ProcessId::new(-42).is_none());
        assert_eq!(ProcessId::new(4_242).map(ProcessId::get), Some(4_242));
        assert_eq!(ProcessId::try_from(0_u32), Err(NotAProcess::GroupId(0)));
        assert_eq!(ProcessId::try_from(1_u32 << 31), Err(NotAProcess::OutOfRange(1 << 31)));
        assert_eq!(ProcessId::try_from(u32::MAX), Err(NotAProcess::OutOfRange(u32::MAX)));
        assert_eq!(ProcessId::try_from(-1_i32), Err(NotAProcess::GroupId(-1)));
        assert_eq!(ProcessId::try_from(std::process::id()).map(ProcessId::unsigned), Ok(std::process::id()));
    }
}
