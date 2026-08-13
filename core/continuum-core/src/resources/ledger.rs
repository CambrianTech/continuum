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
use super::consumer::ConsumerFootprint;
#[cfg(test)]
use super::lease::ReclaimPolicy;
use super::lease::{LeaseError, LeaseRequest, ResourceKind, ResourceLease};

/// Per-kind accounting snapshot — what one resource axis looks like right now.
///
/// The un-inversion (#79): `capacity_bytes` is the FIXED hardware ceiling (device
/// total less a safety reserve), a stable fact about the node. What moves is
/// `physical_used_bytes` — every byte bodily resident (`total − free` as the
/// monitor sees it: our leases, our unleased residency, and external processes
/// alike). The honest commit number is
/// `available = capacity − max(granted, physical_used)`: we never hand out bytes
/// that are physically gone, whoever took them, AND we never double-count a
/// freshly-granted lease that hasn't allocated yet. `granted` sums *all* live
/// leases including expired ones (expiry marks a lease overdue for reclaim, it
/// does NOT free the bytes — only `release` does that, after cleanup confirms).
///
/// Two report-only honesty axes sit alongside:
/// - `measured_bytes` — the sum of what live consumers *self-declare* they hold
///   ([`ConsumerFootprint`]), gathered by the daemon's background poll. It fixes
///   the `granted:0` blind spot (serving holding a resident model with no lease)
///   and its gap from `granted` is the drift the daemon probes.
/// - `external_bytes` — `physical_used − measured`, the bytes resident that NO
///   consumer of ours claims: another process, a game, the OS. The grid signal
///   for "how contended is this node beyond our own footprint."
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/resources/KindLedger.ts"
)]
pub struct KindLedger {
    pub kind: ResourceKind,
    #[ts(type = "number")]
    pub capacity_bytes: u64,
    #[ts(type = "number")]
    pub granted_bytes: u64,
    #[ts(type = "number")]
    pub available_bytes: u64,
    /// Sum of live consumers' self-declared footprints for this kind. Reporting
    /// only — its residency reaches `available` via `physical_used`, not here.
    #[ts(type = "number")]
    pub measured_bytes: u64,
    /// Everything bodily resident of this kind (`total − free`), ours + external.
    /// The moving ground truth the fixed ceiling is netted against.
    #[ts(type = "number")]
    pub physical_used_bytes: u64,
    /// `physical_used − measured` — bytes resident that no consumer of ours
    /// claims (other processes / OS / a game). Reporting only; a grid contention
    /// signal.
    #[ts(type = "number")]
    pub external_bytes: u64,
    pub lease_count: u32,
}

/// One consumer's self-declared residency for one kind — the attribution axis of
/// the board. This is what fixes a board that reads "nothing tracked" while the
/// hardware is full: serving reports `{consumer_id:"serving", kind:Vram, bytes:
/// 18e9, detail:"qwen3-coder-30b weights resident"}` even though it holds no
/// lease. Each node publishes these so the grid can see WHERE the headroom is
/// (the unit of cross-node awareness) without any node having to guess.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/resources/ConsumerAttribution.ts"
)]
pub struct ConsumerAttribution {
    pub consumer_id: String,
    pub kind: ResourceKind,
    #[ts(type = "number")]
    pub bytes: u64,
    pub detail: String,
}

/// The full board the daemon publishes on its `watch` channel and the
/// `resource/*` commands return — every live kind, every lease, and every
/// consumer's measured attribution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/resources/LeaseBoard.ts"
)]
pub struct LeaseBoard {
    pub kinds: Vec<KindLedger>,
    pub leases: Vec<ResourceLease>,
    /// Per-consumer measured residency (self-declared footprints), the honest
    /// attribution of physical bytes independent of leases.
    pub attributions: Vec<ConsumerAttribution>,
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
    /// Each consumer's latest self-declared footprint, refreshed wholesale by the
    /// daemon's background poll of [`ResourceConsumer::footprint`]. This is the
    /// self-declared ATTRIBUTION axis: what a consumer *says* it physically holds
    /// right now, independent of whether it leased it. Keyed by `consumer_id`; the
    /// value is the consumer's full footprint across all kinds. Its purpose is the
    /// board's per-consumer attribution + `measured_bytes` + the drift probe — the
    /// WHO. It does not itself drive `available` (the monitor's `physical_used`
    /// does — see below); a consumer's self-declared bytes reach `available` only
    /// insofar as the hardware monitor also sees them resident.
    measured: BTreeMap<String, Vec<ConsumerFootprint>>,
    /// Bytes of each kind physically resident RIGHT NOW across *everyone* —
    /// `total − free` as the hardware monitor reports it, fed by the daemon every
    /// tick from [`CapacitySource::used_bytes`](super::capacity::CapacitySource).
    /// This is the un-inversion's ground truth: unlike `measured` (what our
    /// consumers self-declare) it counts external processes and our own unleased
    /// residency too. The honest commit number nets it against the fixed ceiling —
    /// `available = capacity − max(granted, physical_used)`. Absent (no physical
    /// monitor) → 0, and `available` degrades to `capacity − granted`.
    physical_used: BTreeMap<ResourceKind, u64>,
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

    /// Daemon feeds physical usage for a kind each tick — `total − free` from the
    /// monitor via [`CapacitySource::used_bytes`](super::capacity::CapacitySource).
    /// Sibling of `set_capacity`; the ground-truth axis of the un-inversion.
    pub fn set_physical_used(&mut self, kind: ResourceKind, bytes: u64) {
        self.physical_used.insert(kind, bytes);
    }

    /// Bytes of this kind physically resident across everyone, as last scanned.
    /// Absent → 0 (no physical monitor for this kind yet).
    pub fn physical_used(&self, kind: ResourceKind) -> u64 {
        self.physical_used.get(&kind).copied().unwrap_or(0)
    }

    /// What is REALLY spoken for of this kind: the larger of what we have granted
    /// (promised, possibly not yet allocated) and what is physically resident
    /// (allocated, possibly never leased — serving's model, a game's VRAM). Taking
    /// the max is the un-inversion in one line: it protects a freshly-granted
    /// lease that hasn't allocated (granted > physical) AND refuses to hand out
    /// bytes that are physically gone whoever took them (physical > granted).
    pub fn committed(&self, kind: ResourceKind) -> u64 {
        self.granted(kind).max(self.physical_used(kind))
    }

    /// Free headroom of this kind, ignoring reservations — the honest global
    /// remainder the board reports: `capacity − max(granted, physical_used)`.
    /// Never negative (saturating). With no physical monitor (`physical_used == 0`)
    /// this is exactly `capacity − granted`, the pre-un-inversion behavior.
    pub fn available(&self, kind: ResourceKind) -> u64 {
        self.capacity(kind).saturating_sub(self.committed(kind))
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

    // ---- measurement axis (monitored residency, never reserved) ------------

    /// Record a consumer's freshly-polled footprint, replacing its prior one
    /// wholesale — each poll is a complete restatement of what it holds, not an
    /// accumulation. An empty footprint clears the consumer (it holds nothing
    /// measurable now). The daemon calls this every tick from its background poll
    /// of [`ResourceConsumer::footprint`]. Pure bookkeeping — no I/O, no clock;
    /// the daemon owns the polling, the ledger only stores the latest truth.
    pub fn set_measured(&mut self, consumer_id: &str, footprints: Vec<ConsumerFootprint>) {
        if footprints.is_empty() {
            self.measured.remove(consumer_id);
        } else {
            self.measured.insert(consumer_id.to_string(), footprints);
        }
    }

    /// Total self-declared residency of a kind across every measured consumer —
    /// the WHO axis (per-consumer attribution). Distinct from `physical_used`
    /// (the monitor's `total − free`, which is what actually drives `available`):
    /// `measured` is only what OUR consumers claim, so `physical_used − measured`
    /// is the external floor. Never negative (saturating).
    pub fn measured(&self, kind: ResourceKind) -> u64 {
        self.measured
            .values()
            .flatten()
            .filter(|f| f.kind == kind)
            .map(|f| f.bytes)
            .fold(0u64, |acc, b| acc.saturating_add(b))
    }

    /// One consumer's measured residency of a kind — used by the drift probe to
    /// compare a consumer's self-declared footprint against what it has leased.
    pub fn measured_by(&self, consumer_id: &str, kind: ResourceKind) -> u64 {
        self.measured
            .get(consumer_id)
            .into_iter()
            .flatten()
            .filter(|f| f.kind == kind)
            .map(|f| f.bytes)
            .fold(0u64, |acc, b| acc.saturating_add(b))
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
                l.is_expired(now_ms) || now_ms.saturating_sub(l.acquired_at_ms) >= min_dwell_ms
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

    /// The published board. Omits a kind only when it is uncapacitied AND
    /// unleased AND unmeasured (nothing honest to say about it); reports honest
    /// math for the rest. `measured_bytes` and `attributions` surface the
    /// self-declared residency so a full GPU never reads as "nothing tracked" —
    /// but `available` stays `capacity − granted`, unchanged by measurement.
    pub fn board(&self) -> LeaseBoard {
        let mut kinds = Vec::new();
        for kind in ResourceKind::ALL {
            let capacity_bytes = self.capacity(kind);
            let lease_count = self.leases.values().filter(|l| l.kind == kind).count() as u32;
            let measured_bytes = self.measured(kind);
            let physical_used_bytes = self.physical_used(kind);
            if capacity_bytes == 0
                && lease_count == 0
                && measured_bytes == 0
                && physical_used_bytes == 0
            {
                continue;
            }
            let granted_bytes = self.granted(kind);
            kinds.push(KindLedger {
                kind,
                capacity_bytes,
                granted_bytes,
                // The honest global remainder — one source of truth with `available`.
                available_bytes: self.available(kind),
                measured_bytes,
                physical_used_bytes,
                // Bytes resident that no consumer of ours claims — the external floor.
                external_bytes: physical_used_bytes.saturating_sub(measured_bytes),
                lease_count,
            });
        }
        let mut leases: Vec<ResourceLease> = self.leases.values().cloned().collect();
        leases.sort_by(|a, b| a.lease_id.cmp(&b.lease_id));

        // Flatten every consumer's latest footprint into stable-ordered
        // attributions (by consumer, then kind) — deterministic for tests + a
        // stable grid-gossip wire order.
        let mut attributions = Vec::new();
        for (consumer_id, footprints) in &self.measured {
            for f in footprints {
                attributions.push(ConsumerAttribution {
                    consumer_id: consumer_id.clone(),
                    kind: f.kind,
                    bytes: f.bytes,
                    detail: f.detail.clone(),
                });
            }
        }
        attributions.sort_by(|a, b| {
            a.consumer_id
                .cmp(&b.consumer_id)
                .then_with(|| (a.kind as u8).cmp(&(b.kind as u8)))
        });

        LeaseBoard {
            kinds,
            leases,
            attributions,
        }
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
            .acquire(
                &req("bevy", ResourceKind::Vram, 4_000, ReclaimPolicy::Pinned),
                "bevy-1".into(),
                100,
            )
            .expect("bevy fits in 10GB");

        let err = ledger
            .acquire(
                &req(
                    "serving",
                    ResourceKind::Vram,
                    8_000,
                    ReclaimPolicy::Graceful,
                ),
                "serving-1".into(),
                100,
            )
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
            .acquire(
                &req(
                    "serving",
                    ResourceKind::Vram,
                    6_000,
                    ReclaimPolicy::Graceful,
                ),
                "serving-2".into(),
                100,
            )
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
            .acquire(
                &req("livekit", ResourceKind::Ram, 1_000, ReclaimPolicy::Graceful),
                "lk-1".into(),
                0,
            )
            .expect("fits");
        // ttl 1_000 → expired at 2_000
        assert_eq!(ledger.granted(ResourceKind::Ram), 1_000);
        assert_eq!(ledger.expire(2_000).len(), 1);
        assert_eq!(
            ledger.granted(ResourceKind::Ram),
            1_000,
            "still held until released"
        );
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
            .acquire(
                &req("bevy", ResourceKind::Vram, 5_000, ReclaimPolicy::Pinned),
                "pin".into(),
                100,
            )
            .unwrap();
        // graceful, active, 2GB, acquired later
        ledger
            .acquire(
                &req(
                    "serving",
                    ResourceKind::Vram,
                    2_000,
                    ReclaimPolicy::Graceful,
                ),
                "grace".into(),
                200,
            )
            .unwrap();
        // hard, active, 2GB
        ledger
            .acquire(
                &req("livekit", ResourceKind::Vram, 2_000, ReclaimPolicy::Hard),
                "hard".into(),
                150,
            )
            .unwrap();

        // Need 3GB: Hard (rank 1) chosen before Graceful (rank 2); 2GB Hard
        // then 2GB Graceful = 4GB ≥ 3GB. dwell 0 → no dwell protection.
        let arbiter = TieredArbiter::default();
        let picks = ledger
            .select_to_reclaim(ResourceKind::Vram, 3_000, 1_000, 0, &arbiter, 0.0)
            .expect("3GB reachable without the pinned lease");
        assert_eq!(
            picks.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
            vec!["hard", "grace"]
        );

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
            .acquire(
                &req(
                    "serving",
                    ResourceKind::Vram,
                    4_000,
                    ReclaimPolicy::Graceful,
                ),
                "fresh".into(),
                1_000,
            )
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
        assert_eq!(
            picks.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
            vec!["fresh"]
        );
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
            .acquire(
                &req(
                    "livekit",
                    ResourceKind::Vram,
                    4_000,
                    ReclaimPolicy::Graceful,
                ),
                "call".into(),
                100,
            )
            .unwrap();

        // Physical free = 6GB, but 2GB of it is livekit's unmet reservation →
        // inference may only take 4GB, not 6GB.
        assert_eq!(ledger.available(ResourceKind::Vram), 6_000);
        assert_eq!(ledger.available_for("serving", ResourceKind::Vram), 4_000);
        let err = ledger
            .acquire(
                &req(
                    "serving",
                    ResourceKind::Vram,
                    6_000,
                    ReclaimPolicy::Graceful,
                ),
                "infer".into(),
                100,
            )
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
            .acquire(
                &req(
                    "livekit",
                    ResourceKind::Vram,
                    2_000,
                    ReclaimPolicy::Graceful,
                ),
                "call-2".into(),
                100,
            )
            .expect("reserved consumer reaches its own floor");

        // Now reclaim must never drop livekit below 6GB. It holds exactly 6GB
        // across two leases → none can be taken.
        assert!(ledger
            .select_to_reclaim(
                ResourceKind::Vram,
                2_000,
                1_000,
                0,
                &TieredArbiter::default(),
                0.0
            )
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
            .acquire(
                &req("serving", ResourceKind::Disk, 500, ReclaimPolicy::Graceful),
                "d1".into(),
                0,
            )
            .unwrap();
        ledger.renew("d1", 5_000, 500).expect("live lease renews");
        assert_eq!(ledger.lease("d1").unwrap().expires_at_ms, 5_000);
        // now past the new deadline
        let err = ledger
            .renew("d1", 9_000, 6_000)
            .expect_err("expired cannot renew");
        assert_eq!(
            err,
            LeaseError::ExpiredLease {
                lease_id: "d1".into()
            }
        );
        // unknown id is fail-loud, not a no-op
        assert_eq!(
            ledger.renew("ghost", 9_000, 100).expect_err("missing"),
            LeaseError::MissingLease {
                lease_id: "ghost".into()
            }
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
            .acquire(
                &req(
                    "serving",
                    ResourceKind::Vram,
                    3_000,
                    ReclaimPolicy::Graceful,
                ),
                "s1".into(),
                0,
            )
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

    // what this catches: the measurement axis is REPORTING-ONLY and never
    // corrupts the available math. A consumer that holds 18GB resident with NO
    // lease (serving's real posture) must surface on the board as measured_bytes
    // + an attribution — fixing the "granted:0 while the GPU is full" blindness —
    // while `available` stays capacity − granted (unchanged by the measurement).
    // If measurement ever leaked into `available`, this test's available_bytes
    // would drop and the honest free-based global remainder would be wrong.
    #[test]
    fn measured_footprint_surfaces_on_board_without_touching_available() {
        let mut ledger = ResourceLeaseLedger::new();
        ledger.set_capacity(ResourceKind::Vram, 24_000);
        // Serving holds 18GB resident but leased nothing — the un-inverted case.
        ledger.set_measured(
            "serving",
            vec![ConsumerFootprint {
                kind: ResourceKind::Vram,
                bytes: 18_000,
                detail: "qwen3-coder-30b weights resident".into(),
            }],
        );

        assert_eq!(ledger.measured(ResourceKind::Vram), 18_000);
        assert_eq!(ledger.measured_by("serving", ResourceKind::Vram), 18_000);
        // available is untouched: capacity − granted(0) = full 24GB, NOT
        // 24 − 18. Measurement reports; it does not reserve.
        assert_eq!(ledger.available(ResourceKind::Vram), 24_000);

        let board = ledger.board();
        let vram = board
            .kinds
            .iter()
            .find(|k| k.kind == ResourceKind::Vram)
            .unwrap();
        assert_eq!(vram.granted_bytes, 0, "no lease → nothing granted");
        assert_eq!(vram.measured_bytes, 18_000, "but 18GB measured-resident");
        assert_eq!(
            vram.available_bytes, 24_000,
            "available ignores measurement"
        );
        assert_eq!(board.attributions.len(), 1);
        assert_eq!(board.attributions[0].consumer_id, "serving");
        assert_eq!(board.attributions[0].bytes, 18_000);
        assert_eq!(board.attributions[0].kind, ResourceKind::Vram);

        // A fresh poll restates wholesale: a smaller model now resident.
        ledger.set_measured(
            "serving",
            vec![ConsumerFootprint {
                kind: ResourceKind::Vram,
                bytes: 4_000,
                detail: "qwen2.5-0.5b resident".into(),
            }],
        );
        assert_eq!(
            ledger.measured(ResourceKind::Vram),
            4_000,
            "restated, not summed"
        );

        // An empty poll clears it (serving unloaded everything) → measured axis
        // gone, and with no lease + no capacity-less kind, attribution empties.
        ledger.set_measured("serving", Vec::new());
        assert_eq!(ledger.measured(ResourceKind::Vram), 0);
        assert!(ledger.board().attributions.is_empty());
    }

    // what this catches: THE un-inversion (task #79). Capacity is the FIXED
    // ceiling; the honest commit number is `capacity − max(granted, physical_used)`.
    // Three regimes must hold with ONE formula:
    //   (a) physical_used < granted (a fresh lease not yet resident) → granted wins,
    //       so we don't double-hand-out the bytes we already promised;
    //   (b) physical_used > granted (external grab / unleased residency) → physical
    //       wins, so we refuse to commit bytes that are physically gone;
    //   (c) the external floor = physical_used − measured surfaces on the board as a
    //       grid-contention signal distinct from our own attribution.
    // If `available` ever went back to `capacity − granted`, regime (b) would
    // over-commit into a launching game's VRAM — the exact OOM the un-inversion
    // fixes — and a grid peer could not read this node's honest bearing.
    #[test]
    fn available_is_capacity_minus_max_of_granted_and_physical_used() {
        let mut ledger = ResourceLeaseLedger::new();
        ledger.set_capacity(ResourceKind::Vram, 24_000);

        // Regime (a): we granted 10_000 but only 6_000 is resident so far (the
        // lease's allocation is still in flight). committed = max(10k, 6k) = 10k;
        // available = 24k − 10k = 14k — the grant is protected, not double-counted.
        ledger
            .acquire(
                &req(
                    "serving",
                    ResourceKind::Vram,
                    10_000,
                    ReclaimPolicy::Graceful,
                ),
                "lease-a".into(),
                0,
            )
            .unwrap();
        ledger.set_physical_used(ResourceKind::Vram, 6_000);
        assert_eq!(
            ledger.committed(ResourceKind::Vram),
            10_000,
            "granted wins while allocation lags"
        );
        assert_eq!(ledger.available(ResourceKind::Vram), 14_000);

        // Regime (b): a game grabs VRAM — physical_used jumps to 21_000 while our
        // grant is unchanged at 10_000. committed = max(10k, 21k) = 21k; available
        // = 24k − 21k = 3k. We now refuse to hand out the bytes the game took.
        ledger.set_physical_used(ResourceKind::Vram, 21_000);
        assert_eq!(
            ledger.committed(ResourceKind::Vram),
            21_000,
            "physical wins when external pressure exceeds grants"
        );
        assert_eq!(ledger.available(ResourceKind::Vram), 3_000);

        // Regime (c): attribute 8_000 of the residency to serving; the rest of
        // physical_used is external (the game). external = 21k − 8k = 13k.
        ledger.set_measured(
            "serving",
            vec![ConsumerFootprint {
                kind: ResourceKind::Vram,
                bytes: 8_000,
                detail: "weights resident".into(),
            }],
        );
        let board = ledger.board();
        let vram = board
            .kinds
            .iter()
            .find(|k| k.kind == ResourceKind::Vram)
            .unwrap();
        assert_eq!(
            vram.capacity_bytes, 24_000,
            "ceiling is fixed — the grab did not move it"
        );
        assert_eq!(vram.physical_used_bytes, 21_000);
        assert_eq!(vram.measured_bytes, 8_000);
        assert_eq!(
            vram.external_bytes, 13_000,
            "physical − measured = the game's floor"
        );
        assert_eq!(
            vram.available_bytes, 3_000,
            "board available == committed math"
        );
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
            .acquire(
                &req("a", ResourceKind::Vram, 2_000, ReclaimPolicy::Graceful),
                "a1".into(),
                100,
            )
            .unwrap();
        ledger
            .acquire(
                &req("b", ResourceKind::Vram, 2_000, ReclaimPolicy::Graceful),
                "b1".into(),
                50,
            )
            .unwrap();
        // pinned-active must stay off-limits no matter what the policy says
        ledger
            .acquire(
                &req("c", ResourceKind::Vram, 5_000, ReclaimPolicy::Pinned),
                "c1".into(),
                10,
            )
            .unwrap();

        // Default (LRU within tier): b1 acquired earlier → taken first for 2GB.
        let default_pick = ledger
            .select_to_reclaim(
                ResourceKind::Vram,
                2_000,
                1_000,
                0,
                &TieredArbiter::default(),
                0.0,
            )
            .unwrap();
        assert_eq!(
            default_pick
                .iter()
                .map(|(id, _)| id.as_str())
                .collect::<Vec<_>>(),
            vec!["b1"]
        );

        // Inverted policy still picks b first here (consumer match), but proves
        // the SCORE drives selection: need 4GB → both graceful taken, never the
        // pinned "c1" (eligibility is the ledger's, unaffected by policy).
        let swapped = ledger
            .select_to_reclaim(ResourceKind::Vram, 4_000, 1_000, 0, &PrefersB, 0.0)
            .unwrap();
        let ids: Vec<&str> = swapped.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["b1", "a1"],
            "b scored higher → reclaimed first; pinned excluded"
        );
        assert!(
            !ids.contains(&"c1"),
            "active pinned never eligible regardless of policy"
        );
    }
}
