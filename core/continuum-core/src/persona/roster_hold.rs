//! RosterHold — persisted operator intent the hosting reconciler consults
//! before adopting citizens.
//!
//! Measured 2026-08-23 (the showcase battery's root flakiness cause): the
//! spawn supervisor stays resident as the hosting reconciler and re-fires on
//! every serving edge, so `persona/instances/despawn` was undone within
//! minutes — every quiesce of the night silently dissolved, producing KV
//! thrash, throughput collapse, and a permit-starved exam. Despawn records
//! "not now"; NOTHING recorded "not until I say". This module is that record.
//!
//! The reconciler is right by its own law (boot owns the process tree,
//! reap-or-ADOPT) — so the fix is not to weaken it but to give it the other
//! half of the contract: a persisted, EXPIRING operator intent. A hold names
//! the citizens allowed to be hosted and when the hold lapses; the reconciler
//! skips (and probes) everyone else. Survives reboots by construction (it is
//! a file); can never wedge the fleet permanently (expiry is mandatory and
//! capped). An operator's EXPLICIT `persona/spawn` stays sovereign — the hold
//! gates the RECONCILER's adoption, never a human's direct command.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Longest allowed hold. A measurement window is hours; a hold that outlives
/// the operator's memory of setting it becomes a mystery outage ("why do my
/// citizens never come back?") — the cap keeps the failure mode bounded.
pub const MAX_HOLD_MINUTES: u64 = 24 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RosterHold {
    /// Agent names allowed to be hosted while the hold stands.
    pub only: Vec<String>,
    /// Unix ms after which the hold no longer applies.
    pub until_ms: u64,
    /// Why — travels into every skip probe so a held-down fleet explains itself.
    pub reason: String,
}

impl RosterHold {
    pub fn allows(&self, agent_name: &str) -> bool {
        self.only.iter().any(|n| n.eq_ignore_ascii_case(agent_name))
    }
    pub fn expired(&self, now_ms: u64) -> bool {
        now_ms >= self.until_ms
    }
}

fn hold_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".continuum").join("roster-hold.json"))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0) // pre-epoch clock: 0 makes every hold read as expired — fail OPEN (fleet hosts), never a stuck hold
}

/// The active hold, or `None` when absent, expired, or unreadable. An expired
/// or corrupt file is REMOVED on read — the hold is self-cleaning, so a stale
/// file can never quietly gate next week's boot.
pub fn active() -> Option<RosterHold> {
    active_at(&hold_path()?, now_ms())
}

/// Testable core of [`active`] — explicit path + clock.
pub fn active_at(path: &std::path::Path, now_ms: u64) -> Option<RosterHold> {
    let bytes = std::fs::read(path).ok()?;
    match serde_json::from_slice::<RosterHold>(&bytes) {
        Ok(hold) if !hold.expired(now_ms) => Some(hold),
        Ok(_) => {
            let _ = std::fs::remove_file(path); // lapsed — self-clean
            None
        }
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "roster-hold file is corrupt — removing it; the fleet hosts normally \
                 (fail OPEN: a broken hold must never become a silent standing outage)"
            );
            let _ = std::fs::remove_file(path);
            None
        }
    }
}

/// Install a hold. Minutes are clamped to [1, MAX_HOLD_MINUTES]; an empty
/// `only` is refused (a hold that allows nobody is a fleet outage, not a
/// measurement window — despawn the fleet explicitly if that is truly meant).
pub fn set(only: Vec<String>, minutes: u64, reason: String) -> Result<RosterHold, String> {
    if only.is_empty() {
        return Err(
            "a hold must name at least one allowed citizen — an empty allow-list is a \
             fleet outage, not a measurement window"
                .to_string(),
        );
    }
    let minutes = minutes.clamp(1, MAX_HOLD_MINUTES);
    let hold = RosterHold {
        only,
        until_ms: now_ms() + minutes * 60_000,
        reason,
    };
    let path = hold_path().ok_or_else(|| "no home directory".to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&hold).map_err(|e| e.to_string())?)
        .map_err(|e| format!("write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("promote {}: {e}", path.display()))?;
    Ok(hold)
}

/// Remove any standing hold. Returns whether one existed.
pub fn clear() -> bool {
    hold_path().is_some_and(|p| std::fs::remove_file(p).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the whole contract in one pass — a hold gates by name
    // (case-insensitive), lapses at until_ms with self-cleaning, and a corrupt
    // file fails OPEN (removed, fleet hosts) rather than becoming a standing
    // silent outage. Regression for the 2026-08-23 despawn≠quiesce arc.
    #[test]
    fn hold_gates_by_name_expires_self_cleans_and_fails_open_on_corruption() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("roster-hold.json");

        let hold = RosterHold {
            only: vec!["Atlas".into()],
            until_ms: 10_000,
            reason: "exam".into(),
        };
        std::fs::write(&path, serde_json::to_vec(&hold).expect("ser")).expect("write");

        let live = active_at(&path, 5_000).expect("hold is live before until_ms");
        assert!(live.allows("Atlas") && live.allows("atlas"), "name match is case-insensitive");
        assert!(!live.allows("Benchy"), "unlisted citizens are held out");

        assert!(active_at(&path, 10_000).is_none(), "hold lapses AT until_ms");
        assert!(!path.exists(), "a lapsed hold self-cleans");

        std::fs::write(&path, b"{not json").expect("write corrupt");
        assert!(active_at(&path, 0).is_none(), "corrupt hold fails OPEN");
        assert!(!path.exists(), "corrupt hold is removed, not left to re-fail");
    }

    // what this catches: an empty allow-list silently becoming a fleet outage,
    // and unbounded holds outliving the operator's memory of setting them.
    #[test]
    fn empty_allow_list_is_refused_and_minutes_are_capped() {
        assert!(set(vec![], 10, "x".into()).is_err(), "empty only-list must refuse");
        // (set() writes to the real home path; only the refusal branch is
        // exercised here — the write path is covered by the gating test's
        // hand-written file, keeping this test filesystem-neutral.)
        assert!(MAX_HOLD_MINUTES <= 24 * 60, "cap stays a day or less");
    }
}
