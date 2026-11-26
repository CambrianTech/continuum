//! What her dream reads her through (the reflection seam), what a pass is asked by, and the
//! bounds its passes keep.

use super::*;

/// The seam through which the dream reaches each live persona's hippocampus.
///
/// Mirrors `PersonaChannelReader` (channel_digest_region.rs): the region depends
/// on a TRAIT, not a concrete registry, so it is unit-testable against a stub.
/// The production impl is `PersonaWorkspaceRegistry` itself (below): admission
/// from the retained fork-template, adapter from the live cycle's model binding
/// (re-home-safe). It resolves the adapter per tick, never stores a parallel
/// persona→adapter map (compression — `[[rag-as-persistent-cache]]`).
pub trait PersonaReflectionSource: Send + Sync {
    /// The persona's hippocampus + its inference adapter, bundled (mirrors
    /// `reader_and_room` returning a tuple). `None` if the persona has no live
    /// reflective surface — the dream sleeps for that persona this tick.
    fn reflector_for(&self, persona_id: Uuid) -> Option<PersonaReflector>;

    /// Durable reviewed-credit delivery belongs to the same resident identity.
    /// Nonpersistent sources (test/ephemeral reflectors) have no credit store.
    fn review_boundary_store(
        &self,
        _persona_id: Uuid,
    ) -> Result<Option<Arc<dyn ReviewBoundaryLedger>>, continuum_client::ClientError> {
        Ok(None)
    }
}

/// What a [`PersonaReflectionSource`] hands the region for one persona: the
/// hippocampus to read episodics from + admit facts into, and the adapter to
/// distill with. Both are `Arc` — shared, never owned by the region.
#[derive(Clone)]
pub struct PersonaReflector {
    pub admission: Arc<AdmissionState>,
    pub adapter: Arc<dyn AIProviderAdapter>,
    /// The served-model id the adapter must be asked for (the persona's live
    /// binding model). `None` only when the adapter has one implicit model.
    pub model: Option<String>,
}

/// How many of her OLDEST not-yet-reviewed beliefs each dream pass adds to the
/// supersession review beyond the lexically-matched ones (#221 slice 2b) —
/// the rotating window that guarantees eventual coverage of the whole belief
/// store, a few beliefs per dream.
pub(super) const ROTATING_REVIEW_PER_PASS: usize = 4;

/// Batch size for a REVIEW-ONLY dream pass (#221 slice 2c) — how many of her
/// oldest unreviewed beliefs a quiet-day dream re-examines at once.
pub(super) const REVIEW_ONLY_BATCH: usize = 6;

/// A belief must be at least this old before the review will re-examine it —
/// re-judging a conclusion made minutes ago is churn, not hygiene. Mechanical
/// age gate (selection), never judgment.
pub(super) const REVIEW_MIN_AGE_MS: u64 = 10 * 60 * 1000;

/// What asked for a consolidation pass: her experience or her rest (the default), or a settled card
/// or an accepted review of hers (its boundary is finished by the pass).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DreamTrigger {
    Idle,
    CardBoundary,
}

/// Accepted reviews are paged in from durable credit a page per pass; a processed one is
/// recorded durably (`ReviewBoundaryLedger::finish`), which is what suppresses its replay.
pub(super) const REVIEW_RECOVERY_PAGE: usize = 16;
pub(super) const REVIEW_RECOVERY_IO_BUDGET: std::time::Duration = std::time::Duration::from_secs(2);
pub(super) const REVIEW_COMPLETION_IO_BUDGET: std::time::Duration = std::time::Duration::from_secs(2);

#[derive(Default)]
pub(super) struct ReviewBoundaries {
    pub(super) pending: HashSet<Uuid>,
    pub(super) recovery_after: Option<String>,
    pub(super) recovery_initialized: bool,
}

/// The substrate-wired `CommandExecutor` the autonomic consolidation pass dispatches
/// `memory/consolidate` through — the SAME executor `training_producer` + the memory
/// commands use. Late-bound because the region is constructed before the executor
/// exists; absent before install (tests, cold boot) makes the pass a quiet no-op.
pub(super) static CONSOLIDATE_EXECUTOR: crate::runtime::LateBound<crate::runtime::CommandExecutor> =
    crate::runtime::LateBound::new("dream_consolidation::consolidate_executor");

/// Install the executor the dream's autonomic consolidation pass dispatches through.
/// Called once at boot, beside `training_producer::install_executor` (the L2 producer's
/// install site) — same wired executor, so the background pass is gated identically.
pub fn install_consolidate_executor(executor: Arc<crate::runtime::CommandExecutor>) {
    CONSOLIDATE_EXECUTOR.install(executor);
}

