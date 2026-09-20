//! Pure pressure-driven lease-revocation selection.
//!
//! Today `ThroughputLeaseRegistry` (who holds what), `FootprintRegistry`
//! (how many bytes each lease costs), and `PressureBroker` (when the
//! system is over budget) exist but are NOT connected: pressure evicts
//! pool pages without consulting lease policy, and leases expire only by
//! the clock. This module is the first wire between them — the pure
//! decision "under THIS pressure, which leases should we revoke to free
//! `target_bytes`, respecting each lease's revocation policy?"
//!
//! It is a **pure classifier**, deliberately mirroring the
//! `classify_peer_prune` / `plan_adaptive_throughput` shape the
//! concurrency style guide endorses: no I/O, no locks, no async, no
//! mutation. The caller (a follow-up slice in `PressureBroker::relieve`)
//! feeds it a snapshot and acts on the returned plan. Keeping the
//! decision pure means the policy is unit-testable in microseconds and
//! the broker stays the single decision-maker — no parallel "lease
//! revoker" task (a forbidden move).
//!
//! Revocation preference (least-disruptive first):
//! - **expired** (any policy) — the holder is gone; reclaiming costs
//!   nothing. Always eligible.
//! - **`Hard`** — "revoke immediately, suitable for stale frames"; cheap
//!   to drop, preferred before Graceful.
//! - **`Graceful`** — "revoke after notifying the holder"; revocable but
//!   carries a notification cost, so it goes last.
//! - **`Pinned`** — never revoked while active (only once expired).
//!
//! Pressure tier gates how far down that ladder we descend, so a mild
//! Warning never disrupts an active Graceful lease when shedding stale
//! frames would do.

use std::collections::HashMap;

use crate::cognition::{ThroughputLease, ThroughputLeaseRevocationPolicy};
use crate::paging::broker::PressureTier;

/// Disruption rank of revoking `lease` at `now_ms` — lower is revoked
/// first. `None` means "must not revoke" (an active `Pinned` lease).
///
/// Expiry is checked before policy so an expired `Pinned` lease is still
/// rank 0 (its holder is gone; the pin no longer protects anything),
/// matching [`ThroughputLease::is_reclaimable`].
///
/// This is the **single definition of the revocation ladder** (expired →
/// Hard → Graceful, never active Pinned). Both selection strategies share
/// it: `select_leases_to_revoke` (broker-style — largest-first within rank,
/// `PressureTier`-gated) and `InferenceCoordinator::evict_under_pressure`
/// (lane-style — oldest-first, ungated, lane-aware side effects). Keep the
/// ladder defined ONCE here; a consumer that re-encodes class→tier inline
/// is the duplication this `pub` exists to prevent.
pub fn disruption_rank(lease: &ThroughputLease, now_ms: u64) -> Option<u8> {
    if lease.is_expired(now_ms) {
        return Some(0);
    }
    match lease.revocation_policy {
        ThroughputLeaseRevocationPolicy::Hard => Some(1),
        ThroughputLeaseRevocationPolicy::Graceful => Some(2),
        ThroughputLeaseRevocationPolicy::Pinned => None,
    }
}

/// Highest disruption rank eligible at a given pressure tier. A higher
/// tier descends further down the revocation ladder.
fn max_eligible_rank(tier: PressureTier) -> u8 {
    match tier {
        // Comfortable — only reclaim leases whose holder already left.
        PressureTier::Normal => 0,
        // One pool tight — shed stale frames (Hard) before disrupting
        // active holders.
        PressureTier::Warning => 1,
        // Over budget — everything reclaimable is on the table (still
        // never an active Pinned lease).
        PressureTier::High | PressureTier::Critical => 2,
    }
}

/// Select leases to revoke under `pressure_tier` to free at least
/// `target_bytes`, respecting each lease's revocation policy.
///
/// `footprint_bytes_per_lease` maps `lease_id` → resident bytes (the
/// `FootprintRegistry`'s view); a lease absent from the map, or costing
/// zero bytes, is skipped (revoking it frees nothing useful).
///
/// Returns:
/// - `Some(plan)` — an ordered `(lease_id, bytes)` list that frees at
///   least `target_bytes`, least-disruptive first (expired → Hard →
///   Graceful), and within a rank the largest first so the target is met
///   with the fewest revocations. `Some(vec![])` when `target_bytes == 0`.
/// - `None` — the target is unachievable with the eligible candidates
///   (e.g. the only remaining leases are active `Pinned`). The caller
///   must NOT treat `None` as "free nothing"; it means "policy cannot
///   satisfy this demand" — a signal to escalate elsewhere, never to
///   silently revoke a pinned lease.
///
/// Pure: no I/O, no mutation, O(N log N) in the lease count. Safe to call
/// on the broker's tick.
pub fn select_leases_to_revoke(
    leases: &[ThroughputLease],
    footprint_bytes_per_lease: &HashMap<String, u64>,
    pressure_tier: PressureTier,
    now_ms: u64,
    target_bytes: u64,
) -> Option<Vec<(String, u64)>> {
    if target_bytes == 0 {
        return Some(Vec::new());
    }
    let ceiling = max_eligible_rank(pressure_tier);

    // (lease_id, bytes, rank) for every eligible candidate.
    let mut candidates: Vec<(String, u64, u8)> = leases
        .iter()
        .filter_map(|lease| {
            let bytes = *footprint_bytes_per_lease.get(&lease.lease_id)?;
            if bytes == 0 {
                return None;
            }
            let rank = disruption_rank(lease, now_ms)?;
            if rank > ceiling {
                return None;
            }
            Some((lease.lease_id.clone(), bytes, rank))
        })
        .collect();

    // Least-disruptive first; within a rank, largest first (fewest
    // revocations to hit the target); lease_id breaks ties so the plan is
    // deterministic across calls with the same snapshot.
    candidates.sort_by(|a, b| a.2.cmp(&b.2).then(b.1.cmp(&a.1)).then(a.0.cmp(&b.0)));

    let mut plan = Vec::new();
    let mut freed = 0u64;
    for (lease_id, bytes, _rank) in candidates {
        if freed >= target_bytes {
            break;
        }
        freed = freed.saturating_add(bytes);
        plan.push((lease_id, bytes));
    }

    if freed >= target_bytes {
        Some(plan)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::{ResourceClass, TargetSilicon};

    fn lease(
        lease_id: &str,
        expires_at_ms: u64,
        revocation_policy: ThroughputLeaseRevocationPolicy,
    ) -> ThroughputLease {
        ThroughputLease {
            lease_id: lease_id.to_string(),
            artifact_key: format!("artifact:{lease_id}"),
            resource_class: ResourceClass::LocalGeneration,
            target_silicon: TargetSilicon::Gpu,
            holder_id: "persona:helper".to_string(),
            cost_units: 1,
            acquired_at_ms: 100,
            expires_at_ms,
            revocation_policy,
        }
    }

    fn bytes(pairs: &[(&str, u64)]) -> HashMap<String, u64> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    /// what this catches: a zero-byte demand must short-circuit to an
    /// empty plan, never iterate or return None (which the caller reads
    /// as "policy can't satisfy" — wrong for a no-op).
    #[test]
    fn zero_target_returns_empty_plan() {
        let leases = [lease("a", 1_000, ThroughputLeaseRevocationPolicy::Hard)];
        let map = bytes(&[("a", 500)]);
        assert_eq!(
            select_leases_to_revoke(&leases, &map, PressureTier::Critical, 100, 0),
            Some(Vec::new())
        );
    }

    /// what this catches: an expired lease is rank 0 EVEN IF Pinned — its
    /// holder is gone so the pin protects nothing. Dropping the expiry
    /// check (treating Pinned as always-None) would strand reclaimable
    /// bytes and fail this.
    #[test]
    fn expired_pinned_lease_is_reclaimable_first() {
        let leases = [
            lease(
                "expired-pinned",
                150,
                ThroughputLeaseRevocationPolicy::Pinned,
            ),
            lease("fresh-hard", 9_999, ThroughputLeaseRevocationPolicy::Hard),
        ];
        let map = bytes(&[("expired-pinned", 400), ("fresh-hard", 400)]);
        // now=200 → expired-pinned is expired; under Normal tier only
        // rank-0 (expired) is eligible, so it is the sole candidate.
        let plan = select_leases_to_revoke(&leases, &map, PressureTier::Normal, 200, 400);
        assert_eq!(plan, Some(vec![("expired-pinned".to_string(), 400)]));
    }

    /// what this catches: an ACTIVE Pinned lease must never be selected.
    /// If it's the only thing that could free the bytes, the answer is
    /// None (escalate), never "revoke the pinned lease".
    #[test]
    fn active_pinned_lease_never_selected() {
        let leases = [lease(
            "pinned",
            9_999,
            ThroughputLeaseRevocationPolicy::Pinned,
        )];
        let map = bytes(&[("pinned", 1_000)]);
        assert_eq!(
            select_leases_to_revoke(&leases, &map, PressureTier::Critical, 100, 500),
            None
        );
    }

    /// what this catches: the pressure tier actually GATES policy. Under
    /// Warning, an active Graceful lease (rank 2) is NOT eligible — only
    /// stale/Hard (rank ≤1). The same lease IS eligible under High.
    /// Dropping the ceiling check would revoke active Graceful leases on
    /// the slightest pressure.
    #[test]
    fn warning_tier_excludes_active_graceful_high_includes_it() {
        let leases = [lease(
            "graceful",
            9_999,
            ThroughputLeaseRevocationPolicy::Graceful,
        )];
        let map = bytes(&[("graceful", 1_000)]);
        assert_eq!(
            select_leases_to_revoke(&leases, &map, PressureTier::Warning, 100, 500),
            None,
            "Warning must not disrupt an active Graceful lease"
        );
        assert_eq!(
            select_leases_to_revoke(&leases, &map, PressureTier::High, 100, 500),
            Some(vec![("graceful".to_string(), 1_000)]),
            "High may revoke a Graceful lease"
        );
    }

    /// what this catches: ordering is least-disruptive first — Hard
    /// (stale) is drained before Graceful (needs notify), even when the
    /// Graceful lease alone could meet the target. Sorting by bytes only
    /// (ignoring rank) would disrupt the active holder unnecessarily.
    #[test]
    fn hard_drained_before_graceful() {
        let leases = [
            lease(
                "graceful-big",
                9_999,
                ThroughputLeaseRevocationPolicy::Graceful,
            ),
            lease("hard-small", 9_999, ThroughputLeaseRevocationPolicy::Hard),
        ];
        let map = bytes(&[("graceful-big", 900), ("hard-small", 600)]);
        // target 500: hard-small (rank 1, 600B) alone satisfies it and is
        // less disruptive than the bigger graceful lease.
        let plan = select_leases_to_revoke(&leases, &map, PressureTier::High, 100, 500);
        assert_eq!(plan, Some(vec![("hard-small".to_string(), 600)]));
    }

    /// what this catches: within a rank, biggest-first meets the target
    /// with the FEWEST revocations. Smallest-first would revoke more
    /// leases than necessary.
    #[test]
    fn within_rank_largest_first_minimizes_revocations() {
        let leases = [
            lease("hard-small", 9_999, ThroughputLeaseRevocationPolicy::Hard),
            lease("hard-big", 9_999, ThroughputLeaseRevocationPolicy::Hard),
        ];
        let map = bytes(&[("hard-small", 300), ("hard-big", 800)]);
        let plan = select_leases_to_revoke(&leases, &map, PressureTier::High, 100, 500);
        assert_eq!(
            plan,
            Some(vec![("hard-big".to_string(), 800)]),
            "one big revocation beats two small ones"
        );
    }

    /// what this catches: when the eligible candidates can't cover the
    /// target, return None (unachievable) — NOT a partial plan the caller
    /// might mistake for "this frees enough".
    #[test]
    fn returns_none_when_target_unachievable() {
        let leases = [lease("hard", 9_999, ThroughputLeaseRevocationPolicy::Hard)];
        let map = bytes(&[("hard", 400)]);
        assert_eq!(
            select_leases_to_revoke(&leases, &map, PressureTier::Critical, 100, 1_000),
            None
        );
    }

    /// what this catches: a lease with no footprint entry (or zero bytes)
    /// is skipped — revoking it frees nothing, so it must not pad the
    /// plan or count toward the target.
    #[test]
    fn unknown_or_zero_footprint_leases_are_skipped() {
        let leases = [
            lease("unknown", 9_999, ThroughputLeaseRevocationPolicy::Hard),
            lease("zero", 9_999, ThroughputLeaseRevocationPolicy::Hard),
            lease("real", 9_999, ThroughputLeaseRevocationPolicy::Hard),
        ];
        let map = bytes(&[("zero", 0), ("real", 700)]);
        let plan = select_leases_to_revoke(&leases, &map, PressureTier::High, 100, 500);
        assert_eq!(plan, Some(vec![("real".to_string(), 700)]));
    }
}
