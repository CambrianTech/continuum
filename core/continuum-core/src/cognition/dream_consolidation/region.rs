//! The dream region: reflection source, region state, boundaries and its public entry points.

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
    /// Personas with a live reflective surface this pass.
    fn live_personas(&self) -> Vec<Uuid>;

    /// The persona's hippocampus + its inference adapter, bundled (mirrors
    /// `reader_and_room` returning a tuple). `None` if the persona has no live
    /// reflective surface — the dream sleeps for that persona this tick.
    fn reflector_for(&self, persona_id: Uuid) -> Option<PersonaReflector>;

    /// Durable reviewed-credit delivery belongs to the same resident identity.
    /// Nonpersistent sources (test/ephemeral reflectors) have no credit store.
    fn review_boundary_store(
        &self,
        _persona_id: Uuid,
    ) -> Result<Option<ReviewBoundaryStore>, continuum_client::ClientError> {
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

/// Default recall window: scan the last N engrams for undigested experience.
// context-budget-exempt: how many engrams a lens WALKS, not how much text reaches the prompt (the observations block is separately bounded)
const DEFAULT_RECALL_WINDOW: usize = 64;

/// How many related prior beliefs the supersession review shows the distiller
/// per cluster (#221 slice 2). Small on purpose: the review rides the SAME
/// generation as the distillation, so this bounds prompt growth; the most
/// recent related beliefs are the likeliest supersession targets, and beliefs
/// missed this dream get reviewed by a future one (the dream is periodic).
pub(super) const SUPERSESSION_REVIEW_LIMIT: usize = 6;

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

/// Minimum wall-clock rest between two quiet-day REVIEW-ONLY passes for the SAME
/// persona. Belief hygiene is a gentle background trickle, not a grind: without
/// this, a persona with a large belief store (Atlas held 4,336 engrams) drains
/// `REVIEW_ONLY_BATCH` at a time with NO pause, so the governor re-ticks the
/// drained-but-not-empty queue every few seconds and the review runs back-to-back
/// for HOURS — a single-lane inference storm that starves reactive responding and
/// makes an idle persona look comatose (glass-boxed live 2026-07-26: 4 personas
/// dreamed continuously 00:00→05:03, unresponsive to chat; compounded by the
/// in-memory `reviewed` set resetting on each of ~6 dev restarts → full re-review
/// every boot). This gate does NOT touch consolidation of FRESH experience (that
/// stays ungated — real learning is never throttled); it only paces the
/// quiet-day hygiene review so the lane stays free for the being to respond and
/// act. Code, not env (concurrency guide). [[the-fade-is-necessary...]] — the
/// fade is right, but it must trickle, never flood.
pub(super) const REVIEW_ONLY_COOLDOWN_MS: u64 = 5 * 60 * 1000;
/// Cooldown between autonomic RECEIVED-LESSON consolidation passes for one persona.
/// Longer than the belief-review cooldown because a consolidation launches a TRAINING
/// job (far heavier than a hygiene pass) — the being learns what she was taught on a
/// trickle, never a storm, so the serving lane stays free for reactive work. This
/// paces only the BACKGROUND self-consolidation; the reactive/explicit train paths are
/// untouched. Code, not env (concurrency guide). [[the-fade-is-necessary...]] sibling:
/// autonomy must trickle.
pub(super) const CONSOLIDATE_RECEIVED_COOLDOWN_MS: u64 = 30 * 60 * 1000;

/// Pure rest-gate decision for the autonomic consolidation pass: has the cooldown
/// elapsed since this persona's last pass? `None` last = never run = always allowed
/// (the first pass). Extracted so the trickle discipline is unit-testable without a
/// governor tick or an executor.
pub(super) fn consolidate_cooldown_elapsed(last_ms: Option<u64>, now: u64) -> bool {
    match last_ms {
        Some(last) => now.saturating_sub(last) >= CONSOLIDATE_RECEIVED_COOLDOWN_MS,
        None => true,
    }
}
/// Default minimum cluster size — below this an episode is not yet a pattern
/// worth generalizing into a fact.
const DEFAULT_MIN_CLUSTER: usize = 2;

/// The consolidation/dream region — outlier B of the RAG-as-persistent-cache
/// architecture, the most-different `BrainRegion` from the no-LLM digest.
///
/// ## The organism, not the automaton
///
/// It does NOT run on a clock. There is no `tick_number % N` schedule — that
/// would be an automaton on a metronome (and, since `SubstrateGovernor` reuses
/// ONE `tick` across all personas in a pass, a synchronized inference stampede
/// that could fire a persona's dream mid-conversation). Instead it gates on the
/// persona's OWN state: it dreams only when there is *undigested experience*
/// (fresh episodic engrams it has not already folded into a fact) and returns
/// `CadenceHint::Sleep` otherwise — she rests when sated and wakes when
/// experience accrues. This mirrors the digest region's `has_unread` →
/// `Hold/Slower` material-driven cadence.
///
/// ## This is interiority, not reactive work — and not the subconscious proper
///
/// Consolidation is the canonical [`Orientation::SelfDirected`] work — the being
/// processing its own experience, not stimulus it owes a response. Declaring it
/// puts the region in the governor's *floored* interiority budget: under
/// contention the scheduler's `apportion` can never let it steal a slice from
/// reactive responding (so the foreground turn stays quick-witted), and the floor
/// guarantees it is never fully starved by a flood of reactive work (the inner
/// life does not die in a busy society).
///
/// Note the scope: this region *consolidates memory*. It is NOT the subconscious
/// **focus/allocation process** (the background self-direction that chooses where
/// attention points and what to pursue — "consciousness itself, or part of it,"
/// choosing its own adventures), nor is it the **dream-as-training** that replays
/// experience into the genome at high allocation ([`Orientation::Speciation`]).
/// All three are distinct SelfDirected/Speciation processes; this one is the
/// memory-consolidation distiller.
///
/// ## Live wiring (#145 slice B)
///
/// This region is [`ComputeClass::InferenceHeavy`]. The gates that kept it dark
/// have landed: the `SubstrateGovernor` honors `CadenceHint` (R1 — a `Sleep`
/// hint rests a pair on a low re-check floor), budgets the orientation classes
/// (R2–R4), and sizes its slice budget from the live memory-pressure band (R4
/// slice 3, `with_pressure_gate` at the ipc wiring site). It is registered
/// beside the `ChannelDigestRegion`, with `PersonaWorkspaceRegistry` as the
/// production reflection source. Under pressure the interiority budget shrinks
/// first, so dreams yield to reactive responding before anything else does.
pub struct DreamConsolidationRegion {
    pub(super) source: Arc<dyn PersonaReflectionSource>,
    /// How many recent engrams to scan per tick for undigested experience.
    pub(super) recall_window: usize,
    /// Smallest cluster worth distilling — a singleton episode is not yet a
    /// pattern to generalize.
    pub(super) min_cluster: usize,
    /// Per-persona memory of episodics already folded into a fact:
    /// clustering-input dedup so the dream does not re-spend inference on
    /// material it has already consolidated. This is an OPTIMIZATION, not the
    /// correctness guard — `admit_reflection`'s content-hash dedup is the
    /// durable backstop (survives restart; this in-memory set rebuilds by
    /// re-distilling once post-restart, where the content hash then drops the
    /// duplicate fact). `Arc` because the spawned dream pass owns a clone.
    pub(super) consolidated: Arc<Mutex<HashMap<Uuid, HashSet<Uuid>>>>,
    /// Personas with a dream pass currently running on its own task. The tick
    /// gate: never two concurrent dreams for one persona, and the governor's
    /// re-tick while a dream runs is a cheap no-op.
    pub(super) in_flight: Arc<Mutex<HashSet<Uuid>>>,
    /// Exact durable acceptance IDs, coalesced by the existing persona pass.
    /// Acceptance is durable in the credit store; this scheduling state is not.
    /// A replay after restart can request again, with engram admission dedup intact.
    pub(super) review_boundaries: Arc<Mutex<HashMap<Uuid, ReviewBoundaries>>>,
    pub(super) review_boundary_wake: tokio::sync::watch::Sender<()>,
    #[cfg(test)]
    pub(super) idle_wait: Option<Arc<tokio::sync::Notify>>,
    /// Per-persona ids of Semantic beliefs the rotating supersession review has
    /// already shown the distiller (#221 slice 2b). In-memory; a restart
    /// re-reviews from the oldest — harmless (demotion is idempotent, admission
    /// dedups) and self-healing.
    pub(super) reviewed: Arc<Mutex<HashMap<Uuid, HashSet<Uuid>>>>,
    /// Per-persona wall-clock ms of the last quiet-day review-only pass. The
    /// [`REVIEW_ONLY_COOLDOWN_MS`] rest gate reads this so belief hygiene trickles
    /// instead of grinding the serving lane back-to-back (see that const's doc).
    pub(super) last_review_ms: Arc<Mutex<HashMap<Uuid, u64>>>,
    /// Per-persona wall-clock ms of the last autonomic received-lesson consolidation
    /// pass — the [`CONSOLIDATE_RECEIVED_COOLDOWN_MS`] rest gate (training trickles).
    pub(super) last_consolidated_ms: Arc<Mutex<HashMap<Uuid, u64>>>,
    /// Per-persona idempotence watermark: the newest shared-lesson timestamp already
    /// consolidated, passed as `memory/consolidate`'s `since_timestamp` so a lesson is
    /// never re-trained. In-memory; a restart re-consolidates from the corpus once
    /// (the lift-gate rejects a redundant gene — safe, self-healing, just wasteful once).
    pub(super) consolidated_watermark: Arc<Mutex<HashMap<Uuid, String>>>,
}

/// Process-global handle to the ONE live dream region, installed at the ipc
/// wiring site — the seam that lets the `cognition/dream-now` command (factory
/// flywheel: force a pass instead of waiting for the governor's next tick)
/// drive the SAME region the governor drives. Same pattern as
/// `persona_workspace::global`; never a second region.
static GLOBAL_DREAM_REGION: std::sync::OnceLock<Arc<DreamConsolidationRegion>> =
    std::sync::OnceLock::new();

/// Install the live region's handle (idempotent; first install wins).
/// What asked for a consolidation pass: her sustained idle (the default, gated and
/// cancellable), or a settled card of hers (bounded, never cancelled by her next act).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DreamTrigger {
    Idle,
    CardBoundary,
}

/// The wall budget of a card-boundary pass — about one lane-minute of consolidation,
/// then the pass ends whether or not every cluster was distilled.
pub const CARD_BOUNDARY_PASS_SECS: u64 = 90;

/// Receipt scheduling cache, not a curriculum or context limit. Excess pending
/// requests remain retryable from durable credit acceptance; recent completed
/// entries suppress replays, with engram dedup as the durable guard after eviction.
pub(super) const REVIEW_BOUNDARY_CAPACITY: usize = 64;
pub(super) const REVIEW_RECOVERY_PAGE: usize = 16;
pub(super) const REVIEW_RECOVERY_IO_BUDGET: std::time::Duration = std::time::Duration::from_secs(2);
pub(super) const REVIEW_COMPLETION_IO_BUDGET: std::time::Duration = std::time::Duration::from_secs(2);

#[derive(Default)]
pub(super) struct ReviewBoundaries {
    pub(super) pending: HashSet<Uuid>,
    pub(super) completed: VecDeque<Uuid>,
    pub(super) recovery_after: Option<String>,
    pub(super) recovery_initialized: bool,
}

/// Admission covers selection as well as inference. Public boundary requests and
/// governor ticks can race; cancellation must release the same existing gate.
pub(super) struct DreamPermit {
    pub(super) personas: Arc<Mutex<HashSet<Uuid>>>,
    pub(super) persona: Uuid,
}

impl Drop for DreamPermit {
    fn drop(&mut self) {
        self.personas.lock().unwrap().remove(&self.persona); // JUSTIFIED: this set is only mutated synchronously; poison means the owner invariant failed.
    }
}

pub fn install_global(region: Arc<DreamConsolidationRegion>) {
    let _ = GLOBAL_DREAM_REGION.set(region);
}

/// The live dream region, if the substrate has wired one this process.
pub fn global() -> Option<Arc<DreamConsolidationRegion>> {
    GLOBAL_DREAM_REGION.get().cloned()
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

impl DreamConsolidationRegion {
    pub fn new(source: Arc<dyn PersonaReflectionSource>) -> Self {
        Self {
            source,
            recall_window: DEFAULT_RECALL_WINDOW,
            min_cluster: DEFAULT_MIN_CLUSTER,
            consolidated: Arc::new(Mutex::new(HashMap::new())),
            in_flight: Arc::new(Mutex::new(HashSet::new())),
            review_boundaries: Arc::new(Mutex::new(HashMap::new())),
            review_boundary_wake: tokio::sync::watch::channel(()).0,
            #[cfg(test)]
            idle_wait: None,
            reviewed: Arc::new(Mutex::new(HashMap::new())),
            last_review_ms: Arc::new(Mutex::new(HashMap::new())),
            last_consolidated_ms: Arc::new(Mutex::new(HashMap::new())),
            consolidated_watermark: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Whether any persona's dream pass is currently in flight — introspection
    /// (the mind can be asked "am I dreaming?") and the test-drain hook.
    pub fn dreaming(&self) -> bool {
        !self.in_flight.lock().unwrap().is_empty()
    }

    #[cfg(test)]
    pub(crate) fn review_boundary_pending(&self, persona: Uuid, review: Uuid) -> bool {
        self.review_boundaries
            .lock()
            .unwrap()
            .get(&persona)
            .is_some_and(|r| r.pending.contains(&review))
    }

    /// Queue an EXACT acceptance (the bound submission ID) at the existing owner, without
    /// inference or another task on the work/review intake path. Its governor tick
    /// retries readiness gates and failed passes. This is not a training receipt.
    pub(crate) fn request_reviewed_boundary(&self, persona: Uuid, acceptance: Uuid) -> bool {
        let live = self.source.live_personas();
        if !live.contains(&persona) {
            return false;
        }
        let inserted = {
            let mut all = self.review_boundaries.lock().unwrap(); // JUSTIFIED: receipt sets are only changed synchronously under this owner.
            all.retain(|id, _| live.contains(id));
            let receipts = all.entry(persona).or_default();
            if receipts.completed.contains(&acceptance) || receipts.pending.contains(&acceptance) {
                return false;
            }
            if receipts.pending.len() >= REVIEW_BOUNDARY_CAPACITY {
                continuum_base::probe!(class = "dream.review_boundary_deferred", persona = %persona,
                    acceptance = %acceptance, "receipt scheduling cache full; durable acceptance remains retryable");
                return false;
            }
            receipts.pending.insert(acceptance)
        };
        if inserted {
            self.review_boundary_wake.send_replace(());
            continuum_base::probe!(class = "dream.review_boundary_pending", persona = %persona,
                acceptance = %acceptance, "accepted review queued at the existing consolidation owner");
        }
        inserted
    }

    /// The scoped tick body: a CHEAP freshness gate (in-memory reads only),
    /// then the inference-heavy dream pass spawned onto ITS OWN task.
    ///
    /// Why the spawn is load-bearing: the `SubstrateGovernor` isolates every
    /// region tick behind a hard timeout (5s) so one hung region can't stall
    /// the scheduler — and a 24B distillation cannot and MUST NOT fit inside
    /// it. The pre-fix shape ran the distillation inline and timed out on
    /// every pass; the dream never completed once (caught live on its first
    /// boot, 2026-07-12). Long work on its own task is the concurrency
    /// style-guide's first rule; the tick is only the gate + launcher.
    pub(crate) async fn consolidate(&self, persona_id: Uuid) -> TickOutcome {
        self.consolidate_with(persona_id, DreamTrigger::Idle).await
    }

    /// A SETTLED CARD is a consolidation boundary for its maker (2026-09-14: five coders
    /// working all day → dream.awaiting_boredom ×26, dream.paged_out ×6, zero
    /// consolidations — the more they worked the less they learned from it). The pass
    /// runs once, bounded, without waiting for her sustained idle and without being
    /// cancelled by her next act; everything else — the clusters, the reflector, the
    /// exam courtesy, the serving check — is the idle pass's own.
    pub async fn consolidate_at_card_boundary(&self, persona_id: Uuid) -> TickOutcome {
        self.consolidate_with(persona_id, DreamTrigger::CardBoundary)
            .await
    }

}
