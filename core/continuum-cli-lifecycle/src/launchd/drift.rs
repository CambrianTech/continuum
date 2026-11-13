//! The read half of the macOS supervisor arm: how the registered job departs from the
//! contract `render_plist` writes.

use super::*;

/// One way the registered supervision differs from the contract — the read half of the
/// macOS supervisor arm of `continuum install` (Joel: one idempotent verb; each arm
/// reads, changes only what drifted, says so). The write half is the register + hand-off;
/// the verb re-reads after it and demands this list empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacDrift {
    /// No launchd job at all.
    Absent,
    /// Registered in the other domain than asked for (agent vs daemon).
    Domain { have: Domain, want: Domain },
    /// The plist execs a bash wrapper, not the binary (audit item 7).
    WrapperCommand,
    /// The plist lacks `AbandonProcessGroup`: a kickstart or heal would kill the lanes.
    KillsLanes,
    /// The plist's `ProcessType` is not [`PROCESS_TYPE`]: Background throttles the engine.
    Throttled,
    /// The job exists but the core on the socket is not its pid (or nothing answers).
    NotOwned {
        job_pid: Option<u32>,
        core_pid: Option<u32>,
    },
    /// The agent's domain is on-demand-only: registered, cannot heal.
    CannotHeal,
}

/// The pure rule over what the node can read: the job (if any) and its plist text, the
/// pids, launchd's word on the domain. `want` is the domain asked for.
pub fn mac_drift(
    have: Option<(&Domain, &str)>,
    want: &Domain,
    job_pid: Option<u32>,
    core_pid: Option<u32>,
    on_demand_only: bool,
) -> Vec<MacDrift> {
    let Some((domain, plist)) = have else {
        return vec![MacDrift::Absent];
    };
    let mut out = Vec::new();
    if domain != want {
        out.push(MacDrift::Domain {
            have: domain.clone(),
            want: want.clone(),
        });
    }
    if plist.contains("<string>/bin/bash</string>") || plist.contains("<string>/bin/sh</string>") {
        out.push(MacDrift::WrapperCommand);
    }
    if !plist.contains("<key>AbandonProcessGroup</key><true/>") {
        out.push(MacDrift::KillsLanes);
    }
    if !plist.contains(PROCESS_TYPE) {
        out.push(MacDrift::Throttled);
    }
    match supervision_verdict(Some(domain.clone()), job_pid, core_pid, on_demand_only) {
        SupervisionVerdict::Supervised { .. } => {}
        SupervisionVerdict::OwnedButCannotHeal { .. } => out.push(MacDrift::CannotHeal),
        SupervisionVerdict::JobPresentCoreOrphaned { job_pid, core_pid } => {
            out.push(MacDrift::NotOwned { job_pid, core_pid })
        }
        SupervisionVerdict::Unsupervised => out.push(MacDrift::Absent),
    }
    out
}
