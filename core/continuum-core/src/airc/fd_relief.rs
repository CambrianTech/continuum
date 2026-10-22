//! airc's answer to descriptor pressure: restart the daemon this core spawned. The
//! kernel's `process-fds` tier owns the contract ([`FdRelief`]); airc owns the action,
//! so the kernel never names airc.

use super::daemon_supervisor::{restart_through_airc, Restart};
use crate::system_resources::fd_gauge::open_fds;
use crate::system_resources::fd_pressure::{FdRelief, ReliefOutcome};

/// Restart the airc daemon through airc's own lifecycle when it offers a transient
/// restart; the freed count is measured, not assumed.
pub struct DaemonRestartRelief;

impl FdRelief for DaemonRestartRelief {
    fn relieve(&self) -> ReliefOutcome {
        let before = open_fds().unwrap_or(0); // unwrap_or: an unreadable table reads as 0 freed, never as a gain
        match restart_through_airc() {
            Restart::Restarted => {
                std::thread::sleep(std::time::Duration::from_secs(1));
                let after = open_fds().unwrap_or(before); // unwrap_or: unreadable after = nothing freed
                let freed = before.saturating_sub(after) as u64;
                continuum_base::probe!(
                    class = "process.fds.owner_acted",
                    action = "daemon_restart",
                    before,
                    after,
                    freed,
                    "descriptor pressure: the airc daemon was restarted through airc's own stop and start"
                );
                ReliefOutcome { action: "daemon_restart", freed }
            }
            Restart::Failed(e) => {
                continuum_base::probe!(
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
