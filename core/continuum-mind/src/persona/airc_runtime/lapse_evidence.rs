//! On-disk evidence re-check for lapsed holds (card 13d7e47a, gap D of
//! 1fc73f18). The in-memory pulse says she was idle; disk may disagree — a
//! restart wipes the stamp while her commits stay. What we read here is what
//! an incident reader needs to separate "truly idle" from "evidence wiped by
//! restart": does the checkout exist, how old is its last commit relative to
//! one lease length, and is there uncommitted work?
//!
//! [`lapse_evidence_recheck`] is BLOCKING (two bounded git reads) — callers on
//! an async path run it via `tokio::task::spawn_blocking`. A hang is reported
//! as unreadable evidence, never waited out.

use continuum_base::time::EpochMs;
use uuid::Uuid;

/// On-disk evidence for one lapsed hold.
pub(super) struct LapseEvidence {
    /// The card's staged checkout exists on this node;
    /// None = the check itself could not run (a spawn_blocking join failure):
    /// "could not check" must never print as "no checkout".
    pub(super) checkout_present: Option<bool>,
    /// Age of the checkout's HEAD commit in ms; None = no commit or unreadable.
    pub(super) head_commit_age_ms: Option<u64>,
    /// Uncommitted changes (porcelain lines); None = status unreadable — a failed
    /// read must not print as "clean".
    pub(super) dirty_files: Option<usize>,
    /// The disambiguator: a commit within one lease length of now.
    pub(super) recent_within_ttl: bool,
}

impl LapseEvidence {
    /// The check ran and found no staged checkout for the card on this node.
    pub(super) const fn absent() -> Self {
        Self {
            checkout_present: Some(false),
            head_commit_age_ms: None,
            dirty_files: None,
            recent_within_ttl: false,
        }
    }

    /// The check itself could not run (a spawn_blocking join failure): every field
    /// reads "unknown" — a failed check must never print as an absent checkout.
    pub(super) const fn unchecked() -> Self {
        Self {
            checkout_present: None,
            head_commit_age_ms: None,
            dirty_files: None,
            recent_within_ttl: false,
        }
    }
}

/// Pure half of the evidence fold (testable without git): from a HEAD commit
/// time and the clock, what does the disk say about recency?
pub(super) fn lapse_recency(
    head_commit_ts_secs: Option<u64>,
    now_ms: u64,
    ttl_ms: u64,
) -> (Option<u64>, bool) {
    let age = head_commit_ts_secs.map(|ts| now_ms.saturating_sub(ts.saturating_mul(1000)));
    (age, age.is_some_and(|a| a < ttl_ms))
}

/// The plumbing half: resolve the card's checkout and fold two bounded git reads
/// into [`LapseEvidence`]. Blocking — run via `spawn_blocking`.
pub(super) fn lapse_evidence_recheck(peer: &Uuid, card: &airc_lib::WorkCard) -> LapseEvidence {
    use crate::system_resources::bounded_command::{probe, Probed};

    let Some(root) = crate::persona::card_checkout::checkout_path_for(peer, card) else {
        return LapseEvidence::absent(); // no staged checkout for this card — nothing to re-check
    };
    if !root.is_dir() {
        return LapseEvidence::absent();
    }
    let root_s = root.to_string_lossy().to_string();
    let now_ms = EpochMs::now().millis(); // pre-1970 reads 0: a pre-epoch clock reads 0, never a guess
    let bound = crate::persona::work_board::advance::CHECKOUT_READ_BOUND;

    let head_ts: Option<u64> =
        match probe("git", &["-C", &root_s, "log", "-1", "--format=%ct"], bound) {
            Probed::Exited { stdout, success } if success => stdout.trim().parse::<u64>().ok(),
            // no commit yet / git failed: the evidence is "no commits", not a number
            _ => None,
        };
    let (head_commit_age_ms, recent_within_ttl) = lapse_recency(
        head_ts,
        now_ms,
        crate::cognition::card_facts::DEFAULT_CLAIM_TTL_MS,
    );
    let dirty_files = match probe("git", &["-C", &root_s, "status", "--porcelain"], bound) {
        Probed::Exited { stdout, success } if success => Some(
            stdout
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count(),
        ),
        _ => None,
    };

    LapseEvidence {
        checkout_present: Some(true),
        head_commit_age_ms,
        dirty_files,
        recent_within_ttl,
    }
}

#[cfg(test)]
mod tests {
    use super::lapse_recency;

    #[test]
    // what this catches: the recency disambiguator — a commit inside one lease length
    // of now must read as work ("she was demonstrably working"), never as idle.
    fn a_commit_within_one_lease_length_is_recent() {
        let now = 30u64 * 60 * 1000; // arbitrary clock: thirty minutes, in ms
        let ttl = 30 * 60 * 1000;
        let five_min_ago_secs = (now - 5 * 60 * 1000) / 1000;
        let (age, recent) = lapse_recency(Some(five_min_ago_secs), now, ttl);
        assert!(recent, "a commit from five minutes ago must read as work");
        assert_eq!(age.map(|a| a / 1000), Some(5 * 60));
    }

    #[test]
    // what this catches: staleness masquerading as recency — a HEAD older than the
    // lease window must not read as work evidence for the lapsed episode.
    fn a_commit_older_than_one_lease_length_is_not_recent() {
        let now_ms = 7_200_000u64; // two hours, in ms
        let ttl = 30 * 60 * 1000;
        let old_commit_secs = 100; // essentially at epoch — far older than one lease
        let (age, recent) = lapse_recency(Some(old_commit_secs), now_ms, ttl);
        assert!(
            !recent,
            "a commit from before the lease window must not read as work"
        );
        assert!(age.is_some_and(|a| a > ttl));
    }

    #[test]
    // what this catches: an absent HEAD printing as a number or as recent — no commit
    // is "no evidence", never recency.
    fn no_commit_is_never_recent() {
        let (age, recent) = lapse_recency(None, 123_456, 1_800_000);
        assert_eq!(age, None);
        assert!(!recent);
    }

    #[test]
    // what this catches: clock skew — a future-dated commit must clamp to zero age and
    // still read recent, never a negative age or "no evidence".
    fn a_future_commit_clamps_to_zero_age_and_reads_recent() {
        // clock skew must not print a negative age or read as "no evidence"
        let (age, recent) = lapse_recency(Some(9_999_999), 100_000, 1_800_000);
        assert_eq!(age, Some(0));
        assert!(recent);
    }
}
