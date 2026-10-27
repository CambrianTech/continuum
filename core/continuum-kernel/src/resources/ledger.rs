//! The byte-residency ledger — pure accounting for who holds what right now.
//!
//! [`ResourceLeaseLedger`] is the byte-axis sibling of this module's
//! [`ThroughputLeaseRegistry`](crate::resources::throughput_lease::ThroughputLeaseRegistry): same
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
    ///
    /// A floor is met by what its holder is GRANTED or by what it is measured to HOLD,
    /// whichever is more: measured residency is already in `physical_used`, so subtracting
    /// the whole floor on top of it counted those bytes twice (Codex, 2026-09-27: a warm
    /// build reserving 10.5 GiB and resident at 6 GiB with no grant cost serving 16.5 GiB,
    /// not 10.5). Grant, measured residency and declared floor stay three facts; only the
    /// unmet part of the floor is withheld.
    fn reserved_headroom_excluding(&self, exclude_consumer: &str, kind: ResourceKind) -> u64 {
        self.reservations
            .iter()
            .filter(|((k, c), _)| *k == kind && c != exclude_consumer)
            .map(|((k, c), floor)| {
                floor.saturating_sub(self.granted_to(c, *k).max(self.measured_by(c, *k)))
            })
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

    /// What `consumer_id` could have **if it released what it is already holding** —
    /// the only honest budget for a consumer deciding what to REPLACE itself with.
    ///
    /// # The ratchet this ends (measured 2026-08-19)
    ///
    /// [`available_for`](Self::available_for) is consumer-aware about *reservations*
    /// (it excludes other consumers' floors, #225) and consumer-BLIND about
    /// *residency*: `committed = max(granted, physical_used)` counts the asking
    /// consumer's own resident bytes against the asking consumer's own budget. For
    /// a lease request that is correct — you cannot spend what you already spent.
    /// For a REPLACEMENT decision it is circular, and the numbers show it:
    ///
    /// ```text
    /// vram capacity              55,125,917,696
    /// serving's own residency    30,253,811,702   ← subtracted from serving
    /// available_for(serving)     16,954,408,960   ← what the planner was handed
    /// + own residency          = 47,208,220,662   ← what it could actually have
    /// ```
    ///
    /// A 27B fits in 47.2 GB and does not fit in 16.9 GB. So the planner **refused to
    /// keep the model it was already running**, stepped down to a 7B, and the next
    /// decision started from a lower ceiling still — a one-way ratchet ending at a
    /// 0.5B on a 64 GB machine. That is #438 and #214 seen from two ends, and it is
    /// also why lanes thrash: the budget MOVES as the incumbent loads, so the planner
    /// re-decides mid-load and respawns.
    ///
    /// # The invariant it pins
    ///
    /// *A machine already serving model X can always re-plan model X.* Anything else
    /// can only decay, and no learned policy layered on top can recover — it would be
    /// training against a signal that is wrong by the size of the resident model.
    ///
    /// # What it does NOT do
    ///
    /// It adds back only the caller's OWN residency, never anyone else's, so it can
    /// never plan into a peer's bytes. Other consumers' reservation floors are still
    /// excluded, exactly as in `available_for`. Use this ONLY for replace-myself
    /// planning; `acquire` must keep using `available_for`, or a consumer could grant
    /// itself bytes it has not yet released.
    pub fn budget_for_replacing(&self, consumer_id: &str, kind: ResourceKind) -> u64 {
        // Never above the device. The add-back is the caller's DECLARED footprint, and
        // a declaration can exceed what the device holds: the 5090, 2026-09-17 — serving
        // declared `peak_resident_bytes(144640 ctx)` = 67.5 GB (an f16-KV estimate) on a
        // 33.6 GB card, the board's available fell to 2.3 GB, and this sum handed the
        // planner `usable_gb=54` — a budget bigger than the card, which then sized a
        // window the card could not hold. A replace-myself budget is bounded by
        // capacity like every other number the ledger speaks.
        self.available_for(consumer_id, kind)
            .saturating_add(self.measured_by(consumer_id, kind))
            .min(self.capacity(kind))
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
mod tests;
