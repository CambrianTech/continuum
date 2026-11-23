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
    /// (CONTINUUM-ENGINE section 5, ONE-BUDGET; a top-k spotlight was a cut outside it).
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
