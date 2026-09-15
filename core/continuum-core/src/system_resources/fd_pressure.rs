//! The process descriptor table as a pressure tier with an OWNER that acts.
//!
//! 2026-09-12: `process.open_fds.high` fired and nothing followed. The gauge is
//! [`fd_gauge`](super::fd_gauge); this is the actor. It sits on the broker as the
//! `process-fds` tier beside `sys-memory` and `disk-root`, and when the broker
//! turns to it, it restarts the airc daemon this core spawned — the one action
//! measured that night to bring a poisoned client/daemon pair back to a flat
//! count — then names the outcome. Once per cooldown: a relief that fires every
//! tick is a new storm.
//!
//! The pool's "bytes" are descriptors: the broker's arithmetic is unit-agnostic
//! (usage/capacity), and a descriptor freed is the quantity this tier owns.
use std::sync::atomic::{AtomicU64, Ordering};

use crate::paging::pool::{ResourcePool, ResourcePoolEntry};
use crate::system_resources::fd_gauge::{fd_soft_limit, open_fds};

/// The budget when the soft limit is unlimited or unreadable: the ceiling this
/// class hits in practice (the 2026-09-12 core plateaued at ~20.4k twice).
pub const DEFAULT_FD_BUDGET: u64 = 20_480;
/// Minimum spacing between two actions.
pub const RELIEF_COOLDOWN_MS: u64 = 10 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReliefOutcome {
    pub action: &'static str,
    pub freed: u64,
}

/// What the tier does when the broker turns to it. Behind a trait so the pool's
/// gating is testable without a daemon.
pub trait FdRelief: Send + Sync {
    fn relieve(&self) -> ReliefOutcome;
}

/// Restart the airc daemon this core spawned; the freed count is measured, not
/// assumed.
pub struct DaemonRestartRelief;

impl FdRelief for DaemonRestartRelief {
    fn relieve(&self) -> ReliefOutcome {
        let before = open_fds().unwrap_or(0); // unwrap_or: an unreadable table reads as 0 freed, never as a gain
        match crate::airc::daemon_supervisor::restart_if_owned() {
            crate::airc::daemon_supervisor::Restart::Restarted { old, new } => {
                std::thread::sleep(std::time::Duration::from_secs(1));
                let after = open_fds().unwrap_or(before); // unwrap_or: unreadable after = nothing freed
                let freed = before.saturating_sub(after) as u64;
                crate::probe!(
                    class = "process.fds.owner_acted",
                    action = "daemon_restart",
                    old_pid = old,
                    new_pid = new,
                    before,
                    after,
                    freed,
                    "descriptor pressure: the daemon this core spawned was restarted"
                );
                ReliefOutcome { action: "daemon_restart", freed }
            }
            crate::airc::daemon_supervisor::Restart::NotOurs => {
                crate::probe!(
                    class = "process.fds.owner_exhausted",
                    reason = "daemon not ours",
                    open = before,
                    "descriptor pressure with no action left: the daemon was adopted, not spawned — an operator restart is owed"
                );
                ReliefOutcome { action: "none", freed: 0 }
            }
            crate::airc::daemon_supervisor::Restart::Failed(e) => {
                crate::probe!(
                    class = "process.fds.owner_exhausted",
                    reason = %e,
                    open = before,
                    "descriptor pressure: the daemon restart did not come back"
                );
                ReliefOutcome { action: "daemon_restart_failed", freed: 0 }
            }
        }
    }
}

pub struct FdPressurePool {
    relief: Box<dyn FdRelief>,
    capacity: u64,
    cooldown_ms: u64,
    last_acted_ms: AtomicU64,
}

impl FdPressurePool {
    /// The production pool: the soft limit (or the default budget) and the daemon restart.
    pub fn with_daemon_relief() -> Self {
        Self::new(Box::new(DaemonRestartRelief), fd_soft_limit().unwrap_or(DEFAULT_FD_BUDGET), RELIEF_COOLDOWN_MS) // unwrap_or: unlimited/unreadable = the measured practical ceiling
    }

    pub fn new(relief: Box<dyn FdRelief>, capacity: u64, cooldown_ms: u64) -> Self {
        Self { relief, capacity: capacity.max(1), cooldown_ms, last_acted_ms: AtomicU64::new(0) }
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0) // unwrap_or: a pre-epoch clock reads as 0 — the cooldown then always allows, the safe side
    }
}

impl ResourcePool for FdPressurePool {
    fn tier_name(&self) -> &str {
        "process-fds"
    }
    fn capacity_bytes(&self) -> u64 {
        self.capacity
    }
    fn usage_bytes(&self) -> u64 {
        open_fds().map_or(0, |n| n as u64)
    }
    fn evict_at_least(&self, _want: u64) -> u64 {
        let now = Self::now_ms();
        let last = self.last_acted_ms.load(Ordering::SeqCst);
        if last != 0 && now.saturating_sub(last) < self.cooldown_ms {
            crate::probe!(
                class = "process.fds.owner_cooling",
                since_ms = now.saturating_sub(last),
                cooldown_ms = self.cooldown_ms,
                "descriptor pressure persists inside the cooldown — the last action has not had its effect yet"
            );
            return 0;
        }
        self.last_acted_ms.store(now, Ordering::SeqCst);
        self.relief.relieve().freed
    }
    fn snapshot(&self) -> Vec<ResourcePoolEntry> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    struct Counting(AtomicUsize);
    impl FdRelief for Counting {
        fn relieve(&self) -> ReliefOutcome {
            self.0.fetch_add(1, Ordering::SeqCst);
            ReliefOutcome { action: "counted", freed: 7 }
        }
    }

    // what this catches: the actor firing on every broker tick (a relief storm on
    // top of the pressure it exists to end) or never (the cooldown never expiring),
    // and the tier's arithmetic drifting from usage/capacity.
    #[test]
    fn the_owner_acts_once_per_cooldown_and_reads_its_table() {
        let pool = FdPressurePool::new(Box::new(Counting(AtomicUsize::new(0))), 100, 60_000);
        assert_eq!(pool.evict_at_least(1), 7, "first call acts");
        assert_eq!(pool.evict_at_least(1), 0, "second call inside the cooldown does not");
        let hot = FdPressurePool::new(Box::new(Counting(AtomicUsize::new(0))), 100, 0);
        assert_eq!(hot.evict_at_least(1), 7);
        assert_eq!(hot.evict_at_least(1), 7, "a zero cooldown acts every time");
        assert!(pool.usage_bytes() >= 3, "this process holds at least stdio");
        assert_eq!(pool.tier_name(), "process-fds");
        assert_eq!(FdPressurePool::new(Box::new(Counting(AtomicUsize::new(0))), 0, 0).capacity_bytes(), 1, "capacity never zero");
    }
}
