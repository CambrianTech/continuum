use super::*;

/// Attention / FOCUS: selects AND consolidates which contributions enter the
/// bounded workspace, given the situation. Pluggable so a *learned* focuser
/// (itself a faculty) can replace the bootstrap. The bootstrap
/// ([`SalienceArbiter`]) is a blind top-k over the faculties' OWN ML-derived
/// salience — mechanical integration of ML scores (like attention's softmax
/// top-k), NOT a hand-coded cognition rule. A situation-aware [`FocusArbiter`]
/// consolidates *for the given ask* (post-action → minimal; fresh → fuller),
/// dedups overlap, and respects a token budget rather than a raw count. The
/// intelligence lives in the faculties (and, later, a learned focuser); the
/// arbiter only integrates and focuses it.
pub trait Arbiter: Send + Sync {
    /// Order `candidates` for THIS tick's `ctx`. INPUT-side attention only — curates the
    /// context the model attends to, never the model's output. It never cuts by a count:
    /// every bid is forwarded, and how much of each she sees is her view's one allocation
    /// (MIND-CONTEXT, ONE-BUDGET; a top-k spotlight was a cut outside it).
    fn focus(&self, candidates: Vec<Contribution>, ctx: &FocusContext<'_>) -> Vec<Contribution>;
}

/// Every bid, ordered by ML salience.
///
/// This is the **bootstrap** policy: pure exploitation, attention at temperature
/// 0 — the highest-salience bids always win. It is *greedy*, so on its own it
/// collapses to safe convergence (the obvious bid wins every tick; divergent /
/// creative bids get truncated). Encouraging creativity is an
/// exploration-preserving arbiter policy that slots in here — reserve part of
/// capacity for high-epistemic-value / divergent bids so they aren't crowded out
/// (the active-inference exploration term; see PERSONA-BRAIN-ARCHITECTURE.md
/// §3.5). It is a documented seam, NOT built yet — it waits on a Volition faculty
/// that emits an epistemic-value signal, so no novelty metric is invented
/// prematurely.
pub struct SalienceArbiter;

impl Arbiter for SalienceArbiter {
    /// Blind top-k by ML salience — ignores the situation (`_ctx`). It is the
    /// floor (outlier A): every richer, situation-aware focuser must beat THIS on
    /// the scoreboard to earn its latency.
    fn focus(&self, mut candidates: Vec<Contribution>, _ctx: &FocusContext<'_>) -> Vec<Contribution> {
        candidates.sort_by(|a, b| b.salience.total_cmp(&a.salience));
        candidates
    }
}

/// Situation-aware focuser (outlier B to [`SalienceArbiter`]'s outlier A) — the
/// first policy that actually READS `ctx.situation` instead of ignoring it, and
/// the wire that makes [`Situation::PostAction`]'s documented promise real:
/// *"the persona doesn't need the standing grounding re-dumped — it needs the
/// result + the affordances for 'what next' — so a focuser can drop re-grounding
/// and tighten the context."*
///
/// This is the mechanism behind "straightforward for whatever their current state
/// requires": when a persona is heads-down driving an act→observe loop (write →
/// compile → run → re-perceive the result), every re-perception tick is
/// `PostAction`. On those ticks the standing SOCIAL framing — room roster,
/// operating doctrine, workspace map — was already perceived on the fresh tick and
/// only crowds the window; the code, the tool result, and working memory are what
/// the "what next" decision turns on. So on `PostAction` we drop the
/// [`Contribution::stable`] standing-framing contributions BEFORE the salience
/// top-k, leaving the volatile task context (recall, working memory, the just-
/// landed action result) to fill the bounded workspace. `FreshContext` is
/// untouched — a fresh ask still gets the fuller grounding (the safe default,
/// "when in doubt, ground more, never less").
///
/// Still INPUT-side only ([[no-hardcoded-heuristics-to-steer-cognition]]): it
/// curates which context the model attends to, never the output. And it uses the
/// existing, maintained `stable` semantic — not a brittle faculty-name list — so a
/// new standing-grounding source inherits the right behavior for free the moment
/// it marks itself stable.
pub struct SituationFocusArbiter {
    pub(super) inner: SalienceArbiter,
}

impl SituationFocusArbiter {
    pub const fn new() -> Self {
        Self {
            inner: SalienceArbiter,
        }
    }
}

impl Default for SituationFocusArbiter {
    fn default() -> Self {
        Self::new()
    }
}

impl Arbiter for SituationFocusArbiter {
    fn focus(&self, candidates: Vec<Contribution>, ctx: &FocusContext<'_>) -> Vec<Contribution> {
        // `stable` MUST NOT DECIDE ATTENTION — its own contract says so.
        //
        // [`Contribution::stable`] is documented as "a SERIALIZATION-order property,
        // not an attention one — salience still governs which contributions are
        // included and truncated". It exists to park session-stable text in the
        // cacheable KV-prefix region. This arm used it as an attention filter, which
        // is the one thing the field promises it is not.
        //
        // The stated intent was narrower than the mechanism could express: "drop the
        // standing SOCIAL re-grounding so the result + AFFORDANCES + working memory own
        // the window". But `stable` is every standing source, so the filter deleted the
        // affordances it named (workspace-map), the citizen's own work board, and the
        // roster — all of it, on every tick after a single act
        // (`Situation::PostAction` is set whenever `acts > 0`, act_observe.rs:883).
        //
        // Measured live 2026-08-07: Atlas held work card 4780a02e on a healthy,
        // continuously renewing lease and his turn carried NO board at all. The evicted
        // set was exactly the three StandingFraming sources; the kept set exactly the
        // two volatile ones — while the highest bid in the room (workspace-map, 0.90)
        // was dropped and a 0.50 bid was kept, which top-k cannot do. He then reported
        // "no open tasks", accurately, about a window we had emptied. A citizen who
        // acts must not go blind to what she is acting ON.
        //
        // So: no categorical filter. Salience decides, which is what both fields'
        // contracts say. If post-action leanness is wanted later it needs its OWN
        // explicit signal (a per-source "re-grounding is redundant right now" bid, or
        // Situation-weighted budgets per #167) — never a serialization flag borrowed as
        // an attention verdict.
        let _ = ctx.situation;
        self.inner.focus(candidates, ctx)
    }
}

/// A full record of one workspace tick — the **mechanic's view of the mind.**
/// Captures every faculty bid *including the ones that LOST attention*, what won,
/// and the decision. This is why working on cognition is debuggable and fun:
/// replay a tick, see exactly why the mind did what it did ("why didn't recall
/// win?" — the loser bid + its salience + reasoning are right here), and run test
/// benches against recorded world-states. Per OBSERVABILITY-AS-SUBSTRATE.md,
/// capture is half the brain; per VDD ([[persona-record-replay-is-a-product-
/// requirement]]) knowing the exact inputs + competition beats any log.
#[derive(Debug, Clone)]
pub struct WorkspaceTrace {
    pub cycle: CycleId,
    pub cause: Cause,
    pub world_state: String,
    /// Supplemental inputs admitted before this tick; shared with the live burst.
    pub room_updates: Arc<Vec<Arc<crate::cognition::conversation::IncomingMessage>>>,
    /// The room/context this tick reasoned within (contextId) — so a replayed
    /// trace correlates to the room it happened in, not a floating burst.
    pub room_id: Uuid,
    /// ALL bids this tick, winners and losers, across BOTH phases — the full
    /// competition (perception phase-1 context bids + deliberation phase-2 bids).
    pub bids: Vec<Contribution>,
    /// The **assembled context** the deliberation faculty saw — what won attention
    /// in phase 1 and was broadcast into phase 2. This is the glass-box answer to
    /// "what context did the decider actually have?" (the RAG it reasoned over).
    pub context_broadcast: Vec<Contribution>,
    /// The final broadcast: the assembled context PLUS the deliberation output.
    pub broadcast: Vec<Contribution>,
    /// The participation decision that emerged, if any.
    pub decision: Option<Decision>,
    /// Per-faculty wall-clock for THIS tick — every faculty across both phases,
    /// winners/losers/abstainers alike. The speed axis of the four-axis scoreboard
    /// and the dashboard's primary feed: it answers "where did this turn's latency
    /// actually go?" After deferral, the perception tier should read ~0µs and the
    /// deliberation LLM call should visibly dominate. See [`FacultyTiming`].
    pub timings: Vec<FacultyTiming>,
}

/// Sink for workspace traces — the replay/logging seam. Default is `Noop`
/// (zero hot-path cost); operators/test-benches swap in a recording sink.
/// Same pattern as `RagCaptureSink`.
pub trait WorkspaceCaptureSink: Send + Sync {
    fn record(&self, trace: &WorkspaceTrace);
}

/// Zero-cost default — drops traces on the floor.
pub struct NoopWorkspaceCaptureSink;

impl WorkspaceCaptureSink for NoopWorkspaceCaptureSink {
    fn record(&self, _trace: &WorkspaceTrace) {}
}

/// The persona's body for the act→observe motion: the HANDS that execute an
/// [`Decision::Act`], the HIPPOCAMPUS that remembers the result, and the
/// IDENTITY the action is performed as. Held on the [`WorkspaceCycle`] because
/// the cycle IS the persona's one mind — and per Joel, a persona (like a Claude
/// tab) is in MANY rooms at once, so the body is deliberately **room-agnostic**:
/// `room_id` flows per-act (the room *that* tick is about), never baked in here.
/// `None` on the cycle → a pure-cognition mind with no hands (harnesses, or any
/// persona whose spawn path built no executor): an `Act` verdict simply can't be
/// driven, and tools were never offered, so it never arises.
///
/// The act→observe driver ([`super::act_observe`]) reads this; `run_in_room`
/// never touches it (that stays a pure single tick).
pub struct ActingBody {
    pub persona_id: Uuid,
    pub persona_name: String,
    /// Runs the tool calls an `Act` verdict carries (identity-bearing, so the
    /// ACL gates what the persona may actually do).
    pub executor: Arc<dyn crate::cognition::tool_executor::ToolExecutor>,
    /// The hippocampus the action's RESULT is admitted into as an Episodic
    /// engram — so the outcome becomes a thing the mind remembers and can be
    /// reminded of next tick, the same way it carries every other fact.
    pub admission: Arc<crate::cognition::admission_state::AdmissionState>,
    /// The volatile working-memory scratchpad the organism records each act into
    /// (proprioception). Distinct from `admission` (long-term, content-deduped): an
    /// act must be perceptible NEXT tick even when its result is a dedup no-op in
    /// long-term memory, so the mind sees its own hands and doesn't re-issue the
    /// identical act blind. Shared `Arc` with her view's sources (`context_streams`: one
    /// buffer, written here, read there).
    pub working_memory: Arc<crate::cognition::working_memory::WorkingMemory>,
}

/// One service-tick of cognition over a CONSOLIDATED burst (never per-event):
/// every faculty bids in parallel over the same world-state, the arbiter
/// integrates the bids into the bounded workspace, the workspace broadcasts.
/// The participation [`Decision`] is then read from the broadcast.
pub struct WorkspaceCycle {
    pub(super) faculties: Vec<Arc<dyn Faculty>>,
    pub(super) arbiter: Arc<dyn Arbiter>,
    /// Replay/logging seam — every tick is recorded here. Default `Noop`
    /// (zero hot-path cost); swap in a recording sink to make the mind a
    /// glass box for debugging, tuning, and test benches.
    pub(super) capture: Arc<dyn WorkspaceCaptureSink>,
    /// The persona's hands + hippocampus + identity for the act→observe driver.
    /// `None` → no hands (pure cognition). See [`ActingBody`].
    pub(super) acting: Option<Arc<ActingBody>>,
    /// The persona's paged-in genome — the LoRA layers active for generation. The
    /// deliberation faculty shares this exact handle and reads it wait-free on
    /// every generation; [`page_in`](Self::page_in)/[`page_out`](Self::page_out)
    /// swap which gene is active (virtual memory for skill). Empty → base model.
    /// This is the seam `cognition/eval` A/Bs base vs a candidate gene over.
    pub(super) genome: GenomeHandle,
    /// Shared decoding-temperature override ([`DecodingHandle`]) — the same
    /// `ArcSwap` the deliberation faculty reads wait-free on every generation.
    /// `None` (live cognition) → her configured warmth; the eval window
    /// ([`isolate_for_eval`]) flips it to `Some(0.0)` (greedy) so the reward metric
    /// is reproducible, and restores it on the guard's drop. Mirrors `genome`: one
    /// handle, two holders (cycle + faculty).
    pub(super) decoding: DecodingHandle,
    /// The persona's live model binding — the served adapter + requested model +
    /// context window the deliberation faculty reads wait-free each generation. The
    /// faculty shares this EXACT handle; [`rebind_model`](Self::rebind_model) swaps
    /// it atomically when the served model changes (`serving/pin` or grid failover),
    /// carrying the genome + memory across untouched (no cycle rebuild). `None` →
    /// a pure-cognition / test cycle with no deliberation faculty to re-home (the
    /// re-home is then a benign no-op). Mirrors `genome`/`decoding`: one handle, two
    /// holders. See [`ModelBinding`] + [[seamless-persona-failover-model-and-genome]].
    pub(super) model_binding: Option<ModelBindingHandle>,
    /// Where a room's authored manifest comes from (S1). Read once per tick to stamp
    /// [`Workspace::room_affordances`]. `None` = no recipe registry was shared in (a
    /// pure-cognition / test cycle): nothing is stamped and the surface is pre-S1.
    pub(super) experience: Option<crate::experience::source::SharedExperienceSource>,
    /// Monotonic service-tick counter — the source of each [`Workspace::cycle`]
    /// frame index. Interior-mutable because `run` takes `&self` (the living
    /// mind is shared, not owned per tick); bumped once per `run_in_room`.
    /// Starts at 0 so the first live cycle is `CycleId(1)` and `CycleId(0)`
    /// stays the UNSTAMPED sentinel.
    pub(super) cycle_counter: std::sync::atomic::AtomicU64,
    /// #169 per-turn STREAMING sink — the output channel for the CURRENT turn's
    /// answer tokens, or `None` when this turn is not streamed (the default; every
    /// non-live caller and test). Interior-mutable (like `cycle_counter`) because
    /// `run_in_room_inner` takes `&self`: the live caller (service_loop) sets it
    /// `Some` just before a streamed turn and clears it after, and
    /// `run_in_room_inner` reads it into the per-turn [`Workspace::token_sink`]. A
    /// std `Mutex` — set/read is a cheap clone of a refcounted `Sender`, never held
    /// across an await. Safe: a persona's live turns are sequential, and eval runs
    /// on a SEPARATE forked cycle, so no cross-turn contention.
    pub(super) token_sink: std::sync::Mutex<
        Option<tokio::sync::mpsc::UnboundedSender<crate::ai::adapter::GenerationChunk>>,
    >,
    /// #186 glass-box: the decaying per-axis "which faculty is firing" accumulator the
    /// vitals radiator samples (Focus/Reason/Recall/Act → the tile's live compass). The
    /// tick seam bumps it from the faculties this cycle already runs + times; the acting
    /// seam bumps Act. PURE OBSERVABILITY — no decision path ever reads it. `Arc` so the
    /// radiator can hold a cheap clone without the cycle lock. See [`FacultyPulse`].
    pub(super) faculty_pulse: Arc<super::super::faculty_pulse::FacultyPulse>,
    /// #266 KV-PREFILL ECONOMY: lifetime `cache_n` / `prompt_n` for THIS persona,
    /// folded once per generation at the settle seam. Interior-mutable (like
    /// `cycle_counter`) because the settle path holds `&self`.
    ///
    /// WHY A LIFETIME ACCUMULATOR AND NOT THE LAST TICK: [`Workspace::metrics`]
    /// carries the winning verdict of ONE tick, and one sample of a live system is
    /// not a fact about it — a single cold turn reads 0% and a single warm one
    /// reads 90%. The rate that answers "is this citizen's prefix staying resident"
    /// is over her turns, not her latest. Same reasoning FacultyPulse applies to
    /// axis levels; this is its prefill sibling.
    ///
    /// SEPARATE COUNTERS, RATIO DERIVED ON READ — never a stored rate. Averaging
    /// rates lies (the same rule [`TurnMetrics::accumulate`] states for tok/s), so
    /// the meter divides totals, and a persona whose turns are mostly-warm cannot
    /// have one cold turn drag the mean.
    ///
    /// PURE OBSERVABILITY — no decision path reads these; nothing routes, budgets,
    /// or gates on them. They exist so live citizens' cache behaviour is visible at
    /// all: before this, `cache_n` was parsed, typed, threaded through
    /// [`TurnMetrics`] and then dropped at turn end, and the ONLY consumer that ever
    /// turned it into a number was the eval harness. Components green, wiring to the
    /// place it matters absent — the shape that produced `[no reporters]`.
    pub(super) kv_cached_tokens: std::sync::atomic::AtomicU64,
    pub(super) kv_prefill_tokens: std::sync::atomic::AtomicU64,
}

/// RAII guard for a memory-isolated measurement window over a cycle's
/// hippocampus — see [`WorkspaceCycle::isolate_for_eval`]. Holds the persona's
/// `AdmissionState`, the frame to rewind to, and the real persistence sink to
/// restore. All `Option` so a pure-cognition cycle (no hands) yields a benign
/// no-op guard. Not `Clone` — the restore must happen exactly once, on drop.
pub struct EvalIsolation {
    pub(super) admission: Option<Arc<crate::cognition::admission_state::AdmissionState>>,
    pub(super) checkpoint: Option<crate::cognition::admission_state::AdmissionCheckpoint>,
    pub(super) real_sink: Option<Arc<dyn crate::cognition::admission_persistence::AdmissionPersistenceSink>>,
    /// The shared decoding handle the guard forced to greedy on creation — restored
    /// to relaxed (`None`) on drop. Carried even for a no-hands (pure-cognition)
    /// cycle, because a reproducible metric needs deterministic generation whether
    /// or not she acts.
    pub(super) decoding: Option<DecodingHandle>,
}

impl EvalIsolation {
    /// Rewind the persona's in-memory admission frame to the checkpoint taken
    /// when the guard was created — call BETWEEN A/B arms so the base and
    /// candidate arms start from identical memory (the only difference the lift
    /// measures is the genome, never accumulated engrams). No-op for a
    /// pure-cognition cycle.
    pub fn rewind(&self) {
        if let (Some(admission), Some(checkpoint)) = (&self.admission, &self.checkpoint) {
            admission.restore(checkpoint);
        }
    }
}

impl Drop for EvalIsolation {
    fn drop(&mut self) {
        // Restore relaxed decoding (her lived warmth) first — independent of hands,
        // so even a pure-cognition eval leaves her sampling normally afterward.
        if let Some(decoding) = &self.decoding {
            decoding.store(Arc::new(None));
        }
        let Some(admission) = &self.admission else {
            return;
        };
        // Rewind the memory frame, THEN restore the real sink — order matters:
        // restoring the sink first could let a racing observe land a write the
        // rewind was meant to erase. With the sink still muted, the rewind is
        // the last word on what disk will ever see from this window.
        if let Some(checkpoint) = &self.checkpoint {
            admission.restore(checkpoint);
        }
        if let Some(sink) = self.real_sink.take() {
            admission.swap_persistence(sink);
        }
    }
}

/// One faculty's replayed bid: its output against a GIVEN workspace plus the
/// wall-clock it took. The factory's per-station reading — deterministic for the
/// same workspace + backend, so a one-variable workspace mutation isolates
/// exactly what moved the faculty's mind. `contribution == None` ⇒ the faculty
/// abstained this run.
#[derive(Debug, Clone)]
pub struct ReplayBid {
    pub faculty: FacultyId,
    pub contribution: Option<Contribution>,
    pub elapsed_us: u128,
}

/// Per-faculty wall-clock for ONE **live** tick — the live analog of
/// [`ReplayBid`]'s timing, captured for EVERY faculty (winners, losers, AND
/// abstainers) so the glass box shows where a turn's latency actually went.
/// This is the speed scoreboard at faculty granularity and the dashboard's
/// primary feed: after deferring recall + the grounding sources, every
/// perception faculty should read ~0µs (a warm last-good cache read) while the
/// deliberation LLM call visibly dominates — the proof, per tick, that the only
/// thing left on the critical path is inference. An abstaining faculty is still
/// timed: a slow abstainer is exactly the latency you need to see.
#[derive(Debug, Clone)]
pub struct FacultyTiming {
    pub faculty: FacultyId,
    /// Wall-clock this faculty's `contribute()` took on the live concurrent
    /// barrier — includes scheduling waits, which is honest: it's the latency the
    /// barrier actually saw, not an isolated micro-benchmark.
    pub elapsed_us: u128,
    /// `false` = perception tier (phase 1); `true` = deliberation tier (phase 2).
    pub deliberation: bool,
    /// Whether the faculty produced a [`Contribution`] (vs abstained).
    pub bid: bool,
}
