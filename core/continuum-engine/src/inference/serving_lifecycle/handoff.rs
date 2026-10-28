//! Training admission closed for a core handoff (card 91921d0f).
//!
//! A deploy reads the node's training hold (`serving/status` `held_for`) and defers while live
//! work holds the engine (card 9beab2da). That read alone left a window: a run admitted after it
//! and before the stop was killed anyway (Codex on #5551). The handoff first closes admission
//! here, under the lifecycle gate, so every admission already deciding has published its hold and
//! every later one waits; then its hold read is final. The close is reversible (a refused or
//! failed handoff reopens it) and owned by the handing-off process, so an owner that dies without
//! reopening never leaves training closed: its death, verified by pid and start time, reopens it.

use uuid::Uuid;

use crate::inference::engine_residency::{Liveness, ProcessProbe};

/// The process a close belongs to, by pid and OS start time (a reused pid is another process).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandoffOwner {
    pub pid: u32,
    pub started_s: u64,
}

impl HandoffOwner {
    /// The process `pid` as it runs now; `None` when its start time cannot be read, since a close
    /// whose owner can never be verified dead could never be released by its death.
    pub fn of(pid: u32) -> Option<Self> {
        crate::inference::engine_residency::process_start_s(pid).map(|started_s| Self { pid, started_s })
    }

    /// PURE but for `probe`: whether the owner still runs. Unreadable is not dead.
    pub fn liveness_with(&self, probe: impl Fn(u32) -> ProcessProbe) -> Liveness {
        match probe(self.pid) {
            ProcessProbe::Absent => Liveness::Dead,
            ProcessProbe::Present(s) if s == self.started_s => Liveness::Alive,
            ProcessProbe::Present(0) | ProcessProbe::Unreadable => Liveness::Unknown,
            ProcessProbe::Present(_) => Liveness::Dead, // the pid was reused by a later process
        }
    }
}

/// One handoff's close: the token that reopens it and the process it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandoffClose {
    pub token: Uuid,
    pub owner: HandoffOwner,
}

/// What an admission does about the current close, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseVerdict {
    /// No close: admit.
    Open,
    /// Closed by a live (or unverifiable) owner: wait.
    Closed,
    /// Its owner is verified dead: the close is released and the admission proceeds.
    OwnerGone,
}

/// PURE but for `probe`: the verdict for `close`.
pub fn verdict(close: Option<&HandoffClose>, probe: impl Fn(u32) -> ProcessProbe) -> CloseVerdict {
    match close {
        None => CloseVerdict::Open,
        Some(close) => match close.owner.liveness_with(probe) {
            Liveness::Dead => CloseVerdict::OwnerGone,
            Liveness::Alive | Liveness::Unknown => CloseVerdict::Closed,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card 91921d0f): a handoff close that a dead deploy process left behind
    // stopping training for good, or a reused pid or an unreadable process table reopening it while
    // its real owner still runs.
    #[test]
    fn only_a_verified_dead_owner_releases_a_close() {
        let close = HandoffClose { token: Uuid::from_u128(1), owner: HandoffOwner { pid: 4242, started_s: 1_793_160_000 } };
        assert_eq!(verdict(None, |_| ProcessProbe::Absent), CloseVerdict::Open);
        assert_eq!(verdict(Some(&close), |_| ProcessProbe::Present(1_793_160_000)), CloseVerdict::Closed, "its owner runs");
        assert_eq!(verdict(Some(&close), |_| ProcessProbe::Absent), CloseVerdict::OwnerGone, "its owner exited");
        assert_eq!(verdict(Some(&close), |_| ProcessProbe::Present(1_793_169_999)), CloseVerdict::OwnerGone, "the pid now names a later process");
        assert_eq!(verdict(Some(&close), |_| ProcessProbe::Unreadable), CloseVerdict::Closed, "an unreadable table is not a death");
        assert_eq!(verdict(Some(&close), |_| ProcessProbe::Present(0)), CloseVerdict::Closed);
    }
}
