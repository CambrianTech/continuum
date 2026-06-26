//! The byte-residency ledger — pure accounting for who holds what right now.
//!
//! [`ResourceLeaseLedger`] is the byte-axis sibling of this module's
//! [`ThroughputLeaseRegistry`](super::ThroughputLeaseRegistry): same
//! acquire/renew/release/expire lifecycle, but it tracks **bytes of physical
//! capacity** (VRAM/RAM/disk) a subsystem holds rather than concurrency slots a
//! transient job occupies. Its one job that the throughput registry cannot do:
//! refuse a grant that would exceed *scanned available* capacity — the
//! over-commit guard at the heart of task #56 (serving claiming a fraction of
//! *total* VRAM, blind to what Bevy and LiveKit already hold → OOM).
//!
//! Pure: no I/O, no async, no clock, no locks. The daemon (a `BrainRegion`)
//! feeds it `set_capacity` from each scan tick, mints lease ids, supplies
//! `now_ms`, and drives reclaim by reading [`select_to_reclaim`]. Mirrors the
//! pure/daemon split of `paging::lease_revocation` (pure `select_*`) vs its
//! servicer.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

use super::arbiter::{ArbiterContext, LeaseArbiter};
use super::lease::{LeaseError, LeaseRequest, ResourceKind, ResourceLease};
#[cfg(test)]
use super::lease::ReclaimPolicy;

/// Per-kind accounting snapshot — what one resource axis looks like right now.
/// `available = capacity − granted`; `granted` sums *all* live leases including
/// expired ones (expiry marks a lease overdue for reclaim, it does NOT free the
/// bytes — only `release` does that, after the holder confirms cleanup).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/resources/KindLedger.ts")]
pub struct KindLedger {
    pub kind: ResourceKind,
    #[ts(type = "number")]
    pub capacity_bytes: u64,
    #[ts(type = "number")]
    pub granted_bytes: u64,
    #[ts(type = "number")]
    pub available_bytes: u64,
    pub lease_count: u32,
}

/// The full board the daemon publishes on its `watch` channel and the
/// `resource/*` commands return — every live kind plus every lease.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/resources/LeaseBoard.ts")]
pub struct LeaseBoard {
    pub kinds: Vec<KindLedger>,
    pub leases: Vec<ResourceLease>,
}

/// The authority's accounting heart. One per machine (or per container). The
/// daemon owns it; everything here is synchronous and deterministic so it is
/// trivially testable and never holds a lock across an await.
#[derive(Debug, Default)]
pub struct ResourceLeaseLedger {
    /// Scanned ceiling per kind, refreshed by the daemon every tick. A kind
    /// absent here has unknown capacity → treated as zero (fail-loud: we refuse
    /// to grant against a resource we have not scanned).
    capacity: BTreeMap<ResourceKind, u64>,
    leases: BTreeMap<String, ResourceLease>,
    /// Per-consumer guaranteed floor of a kind. The real-time fairness mechanism
    /// the directive demands: a live video call (bevy/livekit/stt/tts) reserves
    /// bytes that inference can neither grant-into nor reclaim-below. The daemon
    /// (or a higher arbiter) DECIDES the floor values — policy; the ledger
    /// ENFORCES them — mechanism. Keyed `(kind, consumer_id)`.
    reservations: BTreeMap<(ResourceKind, String), u64>,
}

impl ResourceLeaseLedger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Daemon sets the scanned ceiling for a kind each tick. This is the only
    /// way capacity enters the ledger — it never guesses from `total`, it is
    /// told `available` by the scan layer (GpuMonitor / SystemResourceMonitor /
    /// DiskPressureMonitor).
    pub fn set_capacity(&mut self, kind: ResourceKind, bytes: u64) {
        self.capacity.insert(kind, bytes);
    }

    pub fn capacity(&self, kind: ResourceKind) -> u64 {
        self.capacity.get(&kind).copied().unwrap_or(0)
    }

    /// Bytes currently spoken for of this kind — sums ALL leases, expired
    /// included. Expiry ≠ freed; only `release` (after the holder confirms
    /// reclaim) removes the lease and frees its bytes.
    pub fn granted(&self, kind: ResourceKind) -> u64 {
        self.leases
            .values()
            .filter(|l| l.kind == kind)
            .map(|l| l.bytes)
            .fold(0u64, |acc, b| acc.saturating_add(b))
    }

    /// Free headroom of this kind, ignoring reservations — the raw physical
    /// picture the board reports. Never negative (saturating).
    pub fn available(&self, kind: ResourceKind) -> u64 {
        self.capacity(kind).saturating_sub(self.granted(kind))
    }

    /// Bytes of `kind` currently held by one specific consumer.
    pub fn granted_to(&self, consumer_id: &str, kind: ResourceKind) -> u64 {
        self.leases
            .values()
            .filter(|l| l.kind == kind && l.consumer_id == consumer_id)
            .map(|l| l.bytes)
            .fold(0u64, |acc, b| acc.saturating_add(b))
    }

    /// Register/raise a consumer's guaranteed floor for a kind. A floor of 0
    /// clears the reservation. This is how a video call says "keep N bytes for
    /// me, no matter what inference wants."
    pub fn reserve(&mut self, consumer_id: impl Into<String>, kind: ResourceKind, min_bytes: u64) {
        let key = (kind, consumer_id.into());
        if min_bytes == 0 {
            self.reservations.remove(&key);
        } else {
            self.reservations.insert(key, min_bytes);
        }
    }

    pub fn reservation(&self, consumer_id: &str, kind: ResourceKind) -> u64 {
        self.reservations
            .get(&(kind, consumer_id.to_string()))
            .copied()
            .unwrap_or(0)
    }

    /// Reserved-but-not-yet-granted headroom that must stay free for OTHER
    /// consumers' floors. A requester never sees bytes another consumer is
    /// guaranteed but hasn't claimed yet.
    fn reserved_headroom_excluding(&self, exclude_consumer: &str, kind: ResourceKind) -> u64 {
        self.reservations
            .iter()
            .filter(|((k, c), _)| *k == kind && c != exclude_consumer)
            .map(|((k, c), floor)| floor.saturating_sub(self.granted_to(c, *k)))
            .fold(0u64, |acc, h| acc.saturating_add(h))
    }

    /// Headroom THIS consumer may actually acquire: physical free minus the
    /// unmet reservations of every OTHER consumer. The requester's own
    /// reservation never counts against it. This is what `acquire` enforces so
    /// inference cannot eat into the video call's guaranteed floor.
    pub fn available_for(&self, consumer_id: &str, kind: ResourceKind) -> u64 {
        self.available(kind)
            .saturating_sub(self.reserved_headroom_excluding(consumer_id, kind))
    }

    /// THE over-commit guard. Grant `req.bytes` of `req.kind` only if they fit
    /// in what is *actually* free (capacity minus what others already hold).
    /// Refuses with the exact numbers a caller needs to back off. This is the
    /// task #56 fix in one method: a request is checked against scanned-available,
    /// never against total.
    pub fn acquire(
        &mut self,
        req: &LeaseRequest,
        lease_id: String,
        now_ms: u64,
    ) -> Result<ResourceLease, LeaseError> {
        if self.leases.contains_key(&lease_id) {
            return Err(LeaseError::DuplicateLease { lease_id });
        }
        // Reservation-aware: a requester can take physical-free MINUS what other
        // consumers are guaranteed. Inference cannot grant into the video call's
        // floor even when the bytes are physically idle.
        let available = self.available_for(&req.consumer_id, req.kind);
        if req.bytes > available {
            return Err(LeaseError::InsufficientCapacity {
                kind: req.kind,
                requested: req.bytes,
                available,
            });
        }
        let lease = ResourceLease {
            lease_id: lease_id.clone(),
            consumer_id: req.consumer_id.clone(),
            kind: req.kind,
            bytes: req.bytes,
            acquired_at_ms: now_ms,
            expires_at_ms: now_ms.saturating_add(req.ttl_ms),
            reclaim_policy: req.reclaim_policy,
        };
        self.leases.insert(lease_id, lease.clone());
        Ok(lease)
    }

    /// Extend a live lease's deadline. An already-expired lease cannot be
    /// renewed — the holder must re-`acquire` (and re-pass the capacity check,
    /// since the bytes may have been reclaimed out from under it).
    pub fn renew(
        &mut self,
        lease_id: &str,
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<(), LeaseError> {
        let lease = self
            .leases
            .get_mut(lease_id)
            .ok_or_else(|| LeaseError::MissingLease {
                lease_id: lease_id.to_string(),
            })?;
        if lease.is_expired(now_ms) {
            return Err(LeaseError::ExpiredLease {
                lease_id: lease_id.to_string(),
            });
        }
        lease.expires_at_ms = expires_at_ms;
        Ok(())
    }

    /// Free the bytes — the holder has confirmed cleanup. This is the ONLY path
    /// that reduces `granted`. Missing lease is a fail-loud error, never a
    /// silent no-op (a double-release or stale id is a real bug to surface).
    pub fn release(&mut self, lease_id: &str) -> Result<ResourceLease, LeaseError> {
        self.leases
            .remove(lease_id)
            .ok_or_else(|| LeaseError::MissingLease {
                lease_id: lease_id.to_string(),
            })
    }

    /// Reduce a lease's footprint by `freed_bytes` without ending it — the
    /// accounting side of a consumer satisfying a reclaim by SHRINKING rather
    /// than releasing (a persona tier-downgrade swapping to a smaller base
    /// frees some VRAM but stays alive). Saturating; if the lease reaches zero
    /// it is removed (equivalent to a full release). A `freed_bytes` of 0 is a
    /// no-op — the honest accounting when a consumer Defers or Refuses. Missing
    /// lease is fail-loud (a stale id is a real bug). Returns the bytes still
    /// held by the lease (0 if it was removed).
    pub fn shrink(&mut self, lease_id: &str, freed_bytes: u64) -> Result<u64, LeaseError> {
        let lease = self
            .leases
            .get_mut(lease_id)
            .ok_or_else(|| LeaseError::MissingLease {
                lease_id: lease_id.to_string(),
            })?;
        lease.bytes = lease.bytes.saturating_sub(freed_bytes);
        let remaining = lease.bytes;
        if remaining == 0 {
            self.leases.remove(lease_id);
        }
        Ok(remaining)
    }

    /// Report every lease past its deadline. Does NOT remove them — the daemon
    /// uses this list to drive reclaim callbacks; the bytes free only when the
    /// holder confirms via `release`. Returned in stable id order.
    pub fn expire(&self, now_ms: u64) -> Vec<ResourceLease> {
        self.leases
            .values()
            .filter(|l| l.is_expired(now_ms))
            .cloned()
            .collect()
    }

    /// Pick leases to reclaim to free at least `target_bytes` of `kind`,
    /// ordered by the `arbiter`'s reclaim score (highest = taken first), ties
    /// broken by `lease_id` for determinism. Returns `None` if the target
    /// cannot be met under the protections below — fail-loud: the daemon must
    /// escalate (refuse the new demand) rather than yank a protected holder.
    ///
    /// Mechanism / policy split: this method owns **eligibility** (what is safe
    /// to take) and the arbiter owns **order** (which safe lease to take first).
    /// The arbiter can pick a worse victim; it can never pick an unsafe one.
    /// Two anti-thrash / fairness protections gate eligibility here:
    /// - `min_dwell_ms`: an active (non-expired) lease younger than this is
    ///   off-limits. This is the hysteresis that breaks page-in/page-out
    ///   thrash — a lease just granted gets to live at least a dwell window
    ///   before it can be taken back. Expired leases bypass dwell (overdue).
    /// - reservation floors: a lease is never chosen if removing it would drop
    ///   its consumer below its reserved floor. The live video call keeps its
    ///   guaranteed bytes even while inference is starving for room.
    ///
    /// `pressure` is the kind's current contention (0.0..1.0), passed to the
    /// arbiter; for the default [`TieredArbiter`](super::arbiter::TieredArbiter)
    /// it does not change reclaim order (it scales uniformly), but a richer
    /// policy may use it.
    pub fn select_to_reclaim(
        &self,
        kind: ResourceKind,
        target_bytes: u64,
        now_ms: u64,
        min_dwell_ms: u64,
        arbiter: &dyn LeaseArbiter,
        pressure: f64,
    ) -> Option<Vec<(String, u64)>> {
        if target_bytes == 0 {
            return Some(Vec::new());
        }
        let ctx = ArbiterContext { now_ms, pressure };
        let mut candidates: Vec<&ResourceLease> = self
            .leases
            .values()
            .filter(|l| l.kind == kind && l.reclaim_rank(now_ms).is_some())
            .filter(|l| {
                // Dwell protection — expired bytes are always fair game; a
                // freshly-granted active lease is protected from churn.
                l.is_expired(now_ms)
                    || now_ms.saturating_sub(l.acquired_at_ms) >= min_dwell_ms
            })
            .collect();
        candidates.sort_by(|a, b| {
            let sa = arbiter.reclaim_score(a, &ctx);
            let sb = arbiter.reclaim_score(b, &ctx);
            // Highest score reclaimed first; NaN-safe; lease_id breaks ties.
            sb.partial_cmp(&sa)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.lease_id.cmp(&b.lease_id))
        });

        // Track each consumer's remaining held bytes as we choose victims, so a
        // reservation floor is never breached across multiple picks.
        let mut remaining_per_consumer: BTreeMap<&str, u64> = BTreeMap::new();
        let mut freed = 0u64;
        let mut chosen = Vec::new();
        for lease in candidates {
            if freed >= target_bytes {
                break;
            }
            let floor = self.reservation(&lease.consumer_id, kind);
            let remaining = remaining_per_consumer
                .entry(lease.consumer_id.as_str())
                .or_insert_with(|| self.granted_to(&lease.consumer_id, kind));
            // Skip if taking this lease would drop the consumer below its floor.
            if remaining.saturating_sub(lease.bytes) < floor {
                continue;
            }
            *remaining -= lease.bytes;
            freed = freed.saturating_add(lease.bytes);
            chosen.push((lease.lease_id.clone(), lease.bytes));
        }
        if freed >= target_bytes {
            Some(chosen)
        } else {
            None
        }
    }

    pub fn lease(&self, lease_id: &str) -> Option<&ResourceLease> {
        self.leases.get(lease_id)
    }

    /// The published board. Omits kinds that are both uncapacitied AND
    /// unleased (nothing honest to say about them); reports honest math for the
    /// rest.
    pub fn board(&self) -> LeaseBoard {
        let mut kinds = Vec::new();
        for kind in ResourceKind::ALL {
            let capacity_bytes = self.capacity(kind);
            let lease_count = self.leases.values().filter(|l| l.kind == kind).count() as u32;
            if capacity_bytes == 0 && lease_count == 0 {
                continue;
            }
            let granted_bytes = self.granted(kind);
            kinds.push(KindLedger {
                kind,
                capacity_bytes,
                granted_bytes,
                available_bytes: capacity_bytes.saturating_sub(granted_bytes),
                lease_count,
            });
        }
        let mut leases: Vec<ResourceLease> = self.leases.values().cloned().collect();
        leases.sort_by(|a, b| a.lease_id.cmp(&b.lease_id));
        LeaseBoard { kinds, leases }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::arbiter::TieredArbiter;

    fn req(consumer: &str, kind: ResourceKind, bytes: u64, policy: ReclaimPolicy) -> LeaseRequest {
        LeaseRequest {
            consumer_id: consumer.into(),
            kind,
            bytes,
            ttl_ms: 1_000,
            reclaim_policy: policy,
        }
    }

    // what this catches: THE task-#56 OOM guard. Capacity 10GB, bevy already
    // holds 4GB pinned, serving asks for 8GB → must be refused against the 6GB
    // actually free, NOT granted against the 10GB total. The old serving
    // host_budget() bug over-committed exactly here and OOM'd the box.
    #[test]
    fn acquire_refuses_over_commit_against_what_others_hold() {
        let mut ledger = ResourceLeaseLedger::new();
        ledger.set_capacity(ResourceKind::Vram, 10_000);
        ledger
            .acquire(&req("bevy", ResourceKind::Vram, 4_000, ReclaimPolicy::Pinned), "bevy-1".into(), 100)
            .expect("bevy fits in 10GB");

        let err = ledger
            .acquire(&req("serving", ResourceKind::Vram, 8_000, ReclaimPolicy::Graceful), "serving-1".into(), 100)
            .expect_err("8GB must not fit alongside bevy's 4GB");
        assert_eq!(
            err,
            LeaseError::InsufficientCapacity {
                kind: ResourceKind::Vram,
                requested: 8_000,
                available: 6_000,
            }
        );
        // A request that DOES fit the 6GB headroom succeeds.
        ledger
            .acquire(&req("serving", ResourceKind::Vram, 6_000, ReclaimPolicy::Graceful), "serving-2".into(), 100)
            .expect("6GB fits the remaining headroom exactly");
        assert_eq!(ledger.available(ResourceKind::Vram), 0);
    }

    // what this catches: expiry must not silently free bytes. An expired lease
    // still counts against granted until release — otherwise the ledger would
    // hand out bytes the holder hasn't actually given back yet (double-grant →
    // OOM the moment the holder is slow to clean up).
    #[test]
    fn expired_lease_still_counts_as_held_until_released() {
        let mut ledger = ResourceLeaseLedger::new();
        ledger.set_capacity(ResourceKind::Ram, 1_000);
        ledger
            .acquire(&req("livekit", ResourceKind::Ram, 1_000, ReclaimPolicy::Graceful), "lk-1".into(), 0)
            .expect("fits");
        // ttl 1_000 → expired at 2_000
        assert_eq!(ledger.granted(ResourceKind::Ram), 1_000);
        assert_eq!(ledger.expire(2_000).len(), 1);
        assert_eq!(ledger.granted(ResourceKind::Ram), 1_000, "still held until released");
        assert_eq!(ledger.available(ResourceKind::Ram), 0);
        ledger.release("lk-1").expect("release frees it");
        assert_eq!(ledger.available(ResourceKind::Ram), 1_000);
    }

    // what this catches: reclaim victim selection — safest-first ordering and
    // the fail-loud refusal. With only a pinned active lease covering the
    // target, select must return None (escalate, don't yank pinned), but an
    // expired lease of any policy is fair game.
    #[test]
    fn select_to_reclaim_orders_safest_first_and_refuses_impossible() {
        let mut ledger = ResourceLeaseLedger::new();
        ledger.set_capacity(ResourceKind::Vram, 10_000);
        // pinned, active, 5GB — never reclaimable while live
        ledger
            .acquire(&req("bevy", ResourceKind::Vram, 5_000, ReclaimPolicy::Pinned), "pin".into(), 100)
            .unwrap();
        // graceful, active, 2GB, acquired later
        ledger
            .acquire(&req("serving", ResourceKind::Vram, 2_000, ReclaimPolicy::Graceful), "grace".into(), 200)
            .unwrap();
        // hard, active, 2GB
        ledger
            .acquire(&req("livekit", ResourceKind::Vram, 2_000, ReclaimPolicy::Hard), "hard".into(), 150)
            .unwrap();

        // Need 3GB: Hard (rank 1) chosen before Graceful (rank 2); 2GB Hard
        // then 2GB Graceful = 4GB ≥ 3GB. dwell 0 → no dwell protection.
        let arbiter = TieredArbiter::default();
        let picks = ledger
            .select_to_reclaim(ResourceKind::Vram, 3_000, 1_000, 0, &arbiter, 0.0)
            .expect("3GB reachable without the pinned lease");
        assert_eq!(picks.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), vec!["hard", "grace"]);

        // Need 6GB: only 4GB is non-pinned and active → impossible without
        // touching the pinned lease → None (escalate, don't yank).
        assert!(ledger
            .select_to_reclaim(ResourceKind::Vram, 6_000, 1_000, 0, &arbiter, 0.0)
            .is_none());
    }

    // what this catches: min-dwell hysteresis — the anti-thrash mechanism. A
    // lease granted moments ago must NOT be eligible for reclaim within the
    // dwell window, so the authority can't page it in and rip it back out a
    // tick later. Expired leases must bypass dwell (their bytes are overdue).
    #[test]
    fn min_dwell_protects_fresh_leases_from_reclaim_thrash() {
        let mut ledger = ResourceLeaseLedger::new();
        ledger.set_capacity(ResourceKind::Vram, 10_000);
        // granted at t=1000, active (ttl 1000 → expires 2000)
        ledger
            .acquire(&req("serving", ResourceKind::Vram, 4_000, ReclaimPolicy::Graceful), "fresh".into(), 1_000)
            .unwrap();

        let arbiter = TieredArbiter::default();
        // At t=1200 (held 200ms) with a 500ms dwell → protected → can't free it.
        assert!(ledger
            .select_to_reclaim(ResourceKind::Vram, 4_000, 1_200, 500, &arbiter, 0.0)
            .is_none());
        // At t=1600 (held 600ms ≥ 500ms dwell) → now reclaimable.
        let picks = ledger
            .select_to_reclaim(ResourceKind::Vram, 4_000, 1_600, 500, &arbiter, 0.0)
            .expect("past dwell → eligible");
        assert_eq!(picks.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), vec!["fresh"]);
        // Once expired (t=2100), dwell no longer shields it even within a window.
        let picks = ledger
            .select_to_reclaim(ResourceKind::Vram, 4_000, 2_100, 10_000, &arbiter, 0.0)
            .expect("expired bypasses dwell");
        assert_eq!(picks.len(), 1);
    }

    // what this catches: the real-time fairness guarantee. A live video call
    // reserves a floor; inference can neither acquire into that floor (even when
    // the bytes are physically idle) nor reclaim the call below it. This is the
    // "always get some time" mechanism — the video call is never starved.
    #[test]
    fn reservation_floor_blocks_grant_and_reclaim_below_it() {
        let mut ledger = ResourceLeaseLedger::new();
        ledger.set_capacity(ResourceKind::Vram, 10_000);
        // The live call reserves 6GB and currently holds 4GB of it.
        ledger.reserve("livekit", ResourceKind::Vram, 6_000);
        ledger
            .acquire(&req("livekit", ResourceKind::Vram, 4_000, ReclaimPolicy::Graceful), "call".into(), 100)
            .unwrap();

        // Physical free = 6GB, but 2GB of it is livekit's unmet reservation →
        // inference may only take 4GB, not 6GB.
        assert_eq!(ledger.available(ResourceKind::Vram), 6_000);
        assert_eq!(ledger.available_for("serving", ResourceKind::Vram), 4_000);
        let err = ledger
            .acquire(&req("serving", ResourceKind::Vram, 6_000, ReclaimPolicy::Graceful), "infer".into(), 100)
            .expect_err("can't eat the call's floor");
        assert_eq!(
            err,
            LeaseError::InsufficientCapacity {
                kind: ResourceKind::Vram,
                requested: 6_000,
                available: 4_000,
            }
        );
        // livekit itself CAN draw its own reservation up to the floor.
        ledger
            .acquire(&req("livekit", ResourceKind::Vram, 2_000, ReclaimPolicy::Graceful), "call-2".into(), 100)
            .expect("reserved consumer reaches its own floor");

        // Now reclaim must never drop livekit below 6GB. It holds exactly 6GB
        // across two leases → none can be taken.
        assert!(ledger
            .select_to_reclaim(ResourceKind::Vram, 2_000, 1_000, 0, &TieredArbiter::default(), 0.0)
            .is_none());
    }

    // what this catches: renew extends a live lease but rejects an expired one
    // (the holder must re-acquire and re-pass the capacity check). A renew that
    // silently revived an expired lease would skip the over-commit guard.
    #[test]
    fn renew_extends_active_rejects_expired() {
        let mut ledger = ResourceLeaseLedger::new();
        ledger.set_capacity(ResourceKind::Disk, 1_000);
        ledger
            .acquire(&req("serving", ResourceKind::Disk, 500, ReclaimPolicy::Graceful), "d1".into(), 0)
            .unwrap();
        ledger.renew("d1", 5_000, 500).expect("live lease renews");
        assert_eq!(ledger.lease("d1").unwrap().expires_at_ms, 5_000);
        // now past the new deadline
        let err = ledger.renew("d1", 9_000, 6_000).expect_err("expired cannot renew");
        assert_eq!(err, LeaseError::ExpiredLease { lease_id: "d1".into() });
        // unknown id is fail-loud, not a no-op
        assert_eq!(
            ledger.renew("ghost", 9_000, 100).expect_err("missing"),
            LeaseError::MissingLease { lease_id: "ghost".into() }
        );
    }

    // what this catches: the published board reports only live kinds and honest
    // arithmetic (available = capacity − granted). A board that listed empty
    // kinds or mismatched math would mislead the grid/command callers reading it.
    #[test]
    fn board_reports_only_live_kinds_with_honest_math() {
        let mut ledger = ResourceLeaseLedger::new();
        ledger.set_capacity(ResourceKind::Vram, 8_000);
        // Ram and Disk untouched and uncapacitied → omitted from the board.
        ledger
            .acquire(&req("serving", ResourceKind::Vram, 3_000, ReclaimPolicy::Graceful), "s1".into(), 0)
            .unwrap();
        let board = ledger.board();
        assert_eq!(board.kinds.len(), 1, "only vram is live");
        let vram = &board.kinds[0];
        assert_eq!(vram.kind, ResourceKind::Vram);
        assert_eq!(vram.capacity_bytes, 8_000);
        assert_eq!(vram.granted_bytes, 3_000);
        assert_eq!(vram.available_bytes, 5_000);
        assert_eq!(vram.lease_count, 1);
        assert_eq!(board.leases.len(), 1);
    }

    // what this catches: the mechanism/policy split is real — swapping the
    // arbiter changes WHICH safe victim is taken first, but can never breach a
    // safety bound (pinned-active stays excluded, the reservation floor still
    // holds). Here a deliberately inverted policy that prefers consumer "b"
    // reclaims b's lease before a's, the opposite of the default; if the ledger
    // had baked the ordering in, this swap would have no effect.
    #[test]
    fn arbiter_swap_reorders_victims_without_breaching_safety() {
        struct PrefersB;
        impl LeaseArbiter for PrefersB {
            fn name(&self) -> &str {
                "prefers-b"
            }
            fn reclaim_score(&self, lease: &ResourceLease, _ctx: &ArbiterContext) -> f64 {
                // Higher = reclaimed first. Take consumer "b" before anyone.
                if lease.consumer_id == "b" {
                    100.0
                } else {
                    1.0
                }
            }
            fn demand_urgency(
                &self,
                _req: &LeaseRequest,
                _waited_ms: u64,
                _ctx: &ArbiterContext,
            ) -> f64 {
                0.0
            }
        }

        let mut ledger = ResourceLeaseLedger::new();
        ledger.set_capacity(ResourceKind::Vram, 10_000);
        ledger
            .acquire(&req("a", ResourceKind::Vram, 2_000, ReclaimPolicy::Graceful), "a1".into(), 100)
            .unwrap();
        ledger
            .acquire(&req("b", ResourceKind::Vram, 2_000, ReclaimPolicy::Graceful), "b1".into(), 50)
            .unwrap();
        // pinned-active must stay off-limits no matter what the policy says
        ledger
            .acquire(&req("c", ResourceKind::Vram, 5_000, ReclaimPolicy::Pinned), "c1".into(), 10)
            .unwrap();

        // Default (LRU within tier): b1 acquired earlier → taken first for 2GB.
        let default_pick = ledger
            .select_to_reclaim(ResourceKind::Vram, 2_000, 1_000, 0, &TieredArbiter::default(), 0.0)
            .unwrap();
        assert_eq!(default_pick.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), vec!["b1"]);

        // Inverted policy still picks b first here (consumer match), but proves
        // the SCORE drives selection: need 4GB → both graceful taken, never the
        // pinned "c1" (eligibility is the ledger's, unaffected by policy).
        let swapped = ledger
            .select_to_reclaim(ResourceKind::Vram, 4_000, 1_000, 0, &PrefersB, 0.0)
            .unwrap();
        let ids: Vec<&str> = swapped.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, vec!["b1", "a1"], "b scored higher → reclaimed first; pinned excluded");
        assert!(!ids.contains(&"c1"), "active pinned never eligible regardless of policy");
    }
}
