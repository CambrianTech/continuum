//! Benchmark suite staging as a GOVERNED consumer — it plans against a budget, and it gives
//! bytes back when a peer needs them.
//!
//! # Why this exists (Joel, 2026-08-19)
//!
//! > *"The idea of crushing a benchmark while simultaneously in a video call with several
//! > persona can ONLY work if the intelligent systems have handles to the interface/trait
//! > driven consumers of resources. These independent concerns cannot override or we will
//! > fail. Conversely, the concerns themselves need to decide how to fit within budget much of
//! > the time."*
//!
//! Staging allocated an entire dataset with no regard for anyone else on the box. It OOMed the
//! machine the same day. The reflex fix was a `drop(rows)` — which removes a gratuitous
//! doubling and changes NOTHING structural: a large enough suite still evicts a live call by
//! winning a race against `malloc`. A consumer that does not lease cannot be arbitrated, and an
//! unarbitrated consumer is not a peer, it is a hazard.
//!
//! # The shape, and why it is a genuinely new one for this trait
//!
//! Serving (#79), Bevy, and Voice are all **fat and holding**: they own expensive residency for
//! as long as they are up, and reclaim means degrading something a human can perceive. Staging
//! is the opposite on every axis, which is what makes it worth implementing rather than
//! special-casing:
//!
//! | | serving / bevy / voice | benchmark staging |
//! |---|---|---|
//! | lifetime | as long as the subsystem is up | only during a fetch + projection |
//! | reclaim cost | a tier-down, a frozen avatar, a dropped call | a re-read from a file that is already on disk |
//! | when idle | still holding | holding nothing at all |
//! | under pressure | must weigh refusing | should ALWAYS yield |
//!
//! That last row is the point. Staging is the **ideal reclaim victim**: its entire state is
//! reconstructible from the on-disk cache, so releasing costs latency and nothing else. Encoding
//! that as `Released` (never `Refused`) is what lets a video call take bytes back from a
//! benchmark mid-round instead of the two racing each other into an OOM.
//!
//! # Plan before you allocate — the half the other three do not exercise
//!
//! [`ResourceConsumer`] is the give-back half. The ASK half already exists too and staging was
//! blind to it: [`available_for`](crate::resources::ResourceDaemon::available_for) reports the
//! headroom THIS consumer may plan against — global available minus every other consumer's
//! unmet floor — and its own doc records why it exists (#225: serving planned from
//! reservation-blind `available`, grew its window over the embed lane's floor, and embedding
//! went dead).
//!
//! So [`StagingPlan::decide`] runs BEFORE the allocation, not after the failure. Refusing with a
//! named shortfall is strictly better than an OOM that takes the citizens down with it — Joel's
//! *"otherwise broken json and other things make the system completely degrade"*.
//!
//! # What is deliberately NOT here yet
//!
//! There is an obvious third state: when the budget is too small to hold a suite, STREAM it —
//! page from the HF cursor (or the disk cache) and project row-by-row, so peak stays one page
//! regardless of suite size. That upgrades a refusal into an adaptation, and it is the better
//! system.
//!
//! It is not in this enum, because a variant nothing can construct is a lie about what the
//! system does. Real streaming needs the pager inverted (a per-page callback instead of an
//! accumulate-then-return) and the on-disk cache moved from one JSON array to JSONL so it can
//! be read incrementally. That is a real change and it gets its own slice, at which point
//! `Refuse` narrows to "not even one page fits" and everything else adapts.

use crate::resources::{
    ConsumerFootprint, ReclaimOutcome, ReclaimReason, ReclaimRequest, ResourceConsumer,
    ResourceKind,
};
use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};

/// The id staging leases under. Matches the `consumer_id` on its leases and its footprint rows.
pub const CONSUMER_ID: &str = "benchmark-staging";

/// How staging will read a suite, decided against the budget BEFORE any allocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StagingPlan {
    /// Enough plannable headroom: hold the rows for the projection pass.
    Resident {
        /// What staging expects to hold, and therefore what it should lease.
        bytes: u64,
    },
    /// Not enough. Refuse and NAME the shortfall — never allocate hopefully and let the
    /// allocator arbitrate, which is how a benchmark takes a live call down with it.
    Refuse {
        needed: u64,
        available: u64,
    },
}

impl StagingPlan {
    /// The rule alone, with the governor and the network taken out of it.
    ///
    /// `estimated_bytes` is what the suite is expected to occupy; `available` is
    /// `available_for(CONSUMER_ID, Ram)`. A zero estimate is treated as Resident with zero
    /// bytes — an empty suite legitimately needs nothing, and refusing it would turn a
    /// harmless no-op into an error.
    pub fn decide(estimated_bytes: u64, available: u64) -> Self {
        if estimated_bytes <= available {
            StagingPlan::Resident {
                bytes: estimated_bytes,
            }
        } else {
            StagingPlan::Refuse {
                needed: estimated_bytes,
                available,
            }
        }
    }

    /// The operator-facing sentence for a refusal, naming the shortfall AND what changes it.
    /// A gate that blocks without saying why relocates the archaeology instead of ending it.
    pub fn explain_refusal(&self) -> Option<String> {
        match self {
            StagingPlan::Resident { .. } => None,
            StagingPlan::Refuse { needed, available } => Some(format!(
                "benchmark staging needs ~{needed} bytes of RAM but the governor can only plan \
                 {available} for `{CONSUMER_ID}` right now — another consumer (a serving lane, \
                 a live call) holds the difference. Staging REFUSES rather than allocating \
                 hopefully, because winning a race against the allocator here takes the \
                 citizens down with it. Retry when the box is quieter, or free a lane."
            )),
        }
    }
}

/// What staging currently holds, and the lever to let it go.
///
/// Abstracted as a trait for the same reason [`RenderSurface`](crate::modules::bevy_consumer)
/// is: a unit test proves the reclaim disposition without a process-global staging area or a
/// live governor.
pub trait StagingResidency: Send + Sync {
    /// Bytes staging believes it is holding right now. Zero when idle, which is most of the time.
    fn held_bytes(&self) -> u64;
    /// Release the held rows and return what was freed. Safe by construction: every staged row
    /// is already on disk, so this costs a re-read and nothing else.
    fn release(&self) -> u64;
}

/// The default residency: a process-wide byte counter staging updates as it holds and drops.
///
/// A counter rather than the rows themselves because the rows live in the request that fetched
/// them — this type is the governor's HANDLE onto that, not its owner. `release` is therefore a
/// request to the holder, published as a flag the fetch path checks between pages.
#[derive(Debug, Default)]
pub struct StagingArea {
    held: AtomicU64,
    /// Set when the governor has asked for bytes back. The fetch path reads this and abandons,
    /// which is why release is always honest: nothing keeps holding after the ask.
    yielded: AtomicU64,
}

impl StagingArea {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that staging now holds `bytes` (called when a suite lands in memory).
    pub fn hold(&self, bytes: u64) {
        self.held.store(bytes, Ordering::SeqCst);
    }

    /// Record that staging let go (called when the rows are dropped — including the normal,
    /// non-pressure path, so the footprint returns to zero the moment a fetch completes).
    pub fn drop_all(&self) {
        self.held.store(0, Ordering::SeqCst);
    }

    /// How many times the governor has asked staging to yield. Read by the fetch path so a
    /// long staging pass can abandon mid-flight rather than finish and only then release.
    pub fn yield_requests(&self) -> u64 {
        self.yielded.load(Ordering::SeqCst)
    }
}

impl StagingResidency for StagingArea {
    fn held_bytes(&self) -> u64 {
        self.held.load(Ordering::SeqCst)
    }

    fn release(&self) -> u64 {
        self.yielded.fetch_add(1, Ordering::SeqCst);
        self.held.swap(0, Ordering::SeqCst)
    }
}

/// So the governor's registered consumer and the fetch path that allocates are looking at the
/// SAME counter. Without this they would each hold their own and the footprint would be fiction.
impl StagingResidency for std::sync::Arc<StagingArea> {
    fn held_bytes(&self) -> u64 {
        (**self).held_bytes()
    }
    fn release(&self) -> u64 {
        (**self).release()
    }
}

/// THE staging area — one per process, because there is one pool of host RAM.
///
/// A global rather than a threaded-through handle for the same reason the other consumers read
/// process-global subsystem state: the governor holds `Arc<dyn ResourceConsumer>` and the
/// command path is a stateless `ActionCommand`; there is no shared owner to thread it through.
/// The `StagingResidency` trait is what keeps the tests off it.
pub fn staging_area() -> std::sync::Arc<StagingArea> {
    static AREA: std::sync::OnceLock<std::sync::Arc<StagingArea>> = std::sync::OnceLock::new();
    AREA.get_or_init(|| std::sync::Arc::new(StagingArea::new()))
        .clone()
}

/// Plan a staging pass against the LIVE governor, or fall back to Resident when no governor is
/// running (a unit test, a CLI invocation before boot). The fallback is deliberate and narrow:
/// with no governor there is no peer to starve, so refusing would block work for nobody's
/// benefit — but it is stated here rather than hidden as an `unwrap_or`.
pub fn plan_against_governor(estimated_bytes: u64) -> StagingPlan {
    let Some(daemon) = crate::resources::ResourceDaemon::global() else {
        return StagingPlan::Resident {
            bytes: estimated_bytes,
        };
    };
    let available = daemon.available_for(CONSUMER_ID, ResourceKind::Ram);
    StagingPlan::decide(estimated_bytes, available)
}

/// Staging's face to the governor.
pub struct StagingConsumer<R: StagingResidency> {
    residency: R,
}

impl<R: StagingResidency> StagingConsumer<R> {
    pub fn new(residency: R) -> Self {
        Self { residency }
    }
}

#[async_trait]
impl<R: StagingResidency + 'static> ResourceConsumer for StagingConsumer<R> {
    fn consumer_id(&self) -> &str {
        CONSUMER_ID
    }

    fn footprint(&self) -> Vec<ConsumerFootprint> {
        let bytes = self.residency.held_bytes();
        vec![ConsumerFootprint {
            kind: ResourceKind::Ram,
            bytes,
            detail: if bytes == 0 {
                "benchmark staging: idle (holds rows only during a fetch + projection)".into()
            } else {
                format!("benchmark staging: {bytes} bytes of suite rows, re-readable from the on-disk cache")
            },
        }]
    }

    async fn reclaim(&self, request: ReclaimRequest) -> ReclaimOutcome {
        // Staging NEVER refuses, on ANY reason — Pressure, Rebalance, or Shutdown. Every byte
        // it holds is reconstructible from a file that is already on disk, so yielding costs a
        // re-read and nothing a human or citizen can perceive. It is the cheapest victim on the
        // box and should always be taken before serving tiers down or an avatar freezes.
        //
        // The only honest complication is the RAM/other-kind case: an ask for VRAM cannot be
        // satisfied by dropping host rows, and reporting freed bytes for the wrong kind would
        // corrupt the ledger.
        if request.kind != ResourceKind::Ram {
            return ReclaimOutcome::refused(format!(
                "benchmark staging holds only RAM; it cannot free {:?}",
                request.kind
            ));
        }
        let freed = self.residency.release();
        crate::probe!(
            class = "benchmark.staging.reclaim",
            freed_bytes = freed,
            target_bytes = request.target_bytes,
            reason = ?request.reason,
            "benchmark staging yielded to a peer (always — its state is on disk)",
        );
        // Reporting `Released` with freed == 0 is correct and NOT a silent zero: staging is
        // idle most of the time, and "I hold nothing, take it from someone else" is the true
        // answer. The daemon reconciles against the hardware scan either way.
        ReclaimOutcome::released(freed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// what this catches: staging allocating hopefully and letting `malloc` arbitrate. That is
    /// what OOMed the box on 2026-08-19 — a benchmark and the rest of the system racing, with
    /// the loser being whoever asked second. A plan that REFUSES with a named shortfall is
    /// strictly better than an allocation that wins and takes the citizens down.
    #[test]
    fn a_suite_larger_than_the_budget_refuses_and_names_the_shortfall() {
        let plan = StagingPlan::decide(8_000, 5_000);
        assert_eq!(
            plan,
            StagingPlan::Refuse {
                needed: 8_000,
                available: 5_000
            }
        );
        let why = plan.explain_refusal().expect("a refusal must explain itself");
        assert!(
            why.contains("8000") && why.contains("5000"),
            "the shortfall must be NAMED, not described as 'insufficient': {why}"
        );
        assert!(
            why.contains(CONSUMER_ID),
            "and say which consumer was budgeted: {why}"
        );
    }

    /// what this catches: an off-by-one at the boundary refusing a suite that exactly fits, and
    /// an empty suite being refused — an empty pull needs nothing and erroring on it would turn
    /// a harmless no-op into a failure.
    #[test]
    fn a_suite_that_exactly_fits_is_resident_and_an_empty_one_always_is() {
        assert_eq!(
            StagingPlan::decide(5_000, 5_000),
            StagingPlan::Resident { bytes: 5_000 }
        );
        assert_eq!(
            StagingPlan::decide(0, 0),
            StagingPlan::Resident { bytes: 0 }
        );
        assert!(StagingPlan::decide(5_000, 5_000).explain_refusal().is_none());
    }

    /// what this catches: THE property that makes staging safe to run beside a video call.
    /// Serving weighs a tier-down and Bevy refuses outright during a live call, because their
    /// bytes are load-bearing for something a human perceives. Staging's are not — every row is
    /// on disk — so it must yield on EVERY reason, including Rebalance. If this ever starts
    /// refusing, a benchmark can starve a call, which is the exact failure this consumer exists
    /// to make impossible.
    #[tokio::test]
    async fn staging_yields_on_every_reason_because_its_state_is_on_disk() {
        for reason in [
            ReclaimReason::Pressure,
            ReclaimReason::Rebalance,
            ReclaimReason::Shutdown,
        ] {
            let area = StagingArea::new();
            area.hold(4_096);
            let consumer = StagingConsumer::new(area);
            let out = consumer
                .reclaim(ReclaimRequest {
                    kind: ResourceKind::Ram,
                    target_bytes: 1_024,
                    deadline_ms: 100,
                    reason,
                })
                .await;
            assert_eq!(out.freed_bytes, 4_096, "must yield ALL of it on {reason:?}");
            assert_eq!(
                out.status,
                crate::resources::ReclaimStatus::Released,
                "staging must never refuse a RAM ask ({reason:?}) — its bytes are re-readable"
            );
        }
    }

    /// what this catches: reporting freed bytes for a kind staging cannot free. An ask for VRAM
    /// answered with "released N" would corrupt the governor's ledger into believing device
    /// memory came back when only host rows were dropped.
    #[tokio::test]
    async fn an_ask_for_a_kind_staging_cannot_free_is_refused_not_faked() {
        let area = StagingArea::new();
        area.hold(4_096);
        let consumer = StagingConsumer::new(area);
        let out = consumer
            .reclaim(ReclaimRequest {
                kind: ResourceKind::Vram,
                target_bytes: 1_024,
                deadline_ms: 100,
                reason: ReclaimReason::Pressure,
            })
            .await;
        assert_eq!(out.freed_bytes, 0);
        assert_eq!(out.status, crate::resources::ReclaimStatus::Refused);
        assert!(out.detail.unwrap_or_default().contains("RAM"));
    }

    /// what this catches: the footprint lying while idle. Staging holds nothing between fetches,
    /// and a consumer that reports phantom bytes makes the governor evict a REAL holder to
    /// recover memory nobody has.
    #[test]
    fn an_idle_staging_area_reports_zero_and_says_it_is_idle() {
        let consumer = StagingConsumer::new(StagingArea::new());
        let fp = consumer.footprint();
        assert_eq!(fp.len(), 1);
        assert_eq!(fp[0].bytes, 0);
        assert_eq!(fp[0].kind, ResourceKind::Ram);
        assert!(fp[0].detail.contains("idle"), "{}", fp[0].detail);
    }

    /// what this catches: a long staging pass finishing its work AFTER being asked to yield.
    /// The ask has to be observable mid-flight, or "released" means "released eventually",
    /// which is the `Deferred` contract wearing a `Released` label.
    #[test]
    fn a_yield_request_is_observable_so_a_pass_in_flight_can_abandon() {
        let area = StagingArea::new();
        area.hold(1_000);
        assert_eq!(area.yield_requests(), 0);
        assert_eq!(area.release(), 1_000);
        assert_eq!(area.yield_requests(), 1, "the ask must be visible to the holder");
        assert_eq!(area.held_bytes(), 0);
    }
}
