//! The Global Workspace — the brain's integration core.
//!
//! See `docs/architecture/PERSONA-BRAIN-ARCHITECTURE.md`. Cognition is a
//! federation of **faculties** (swappable ML adapters). Each service tick (over
//! a *consolidated burst*, never per-event) the faculties bid in parallel into
//! a bounded **Workspace**; a pluggable **Arbiter** integrates their bids
//! (attention), and the winners are broadcast. The persona's participation
//! **Decision** is the *output of the deliberation faculty's thinking* over that
//! workspace — never a heuristic gate, never an `@`-trigger, never a sender caste.
//!
//! This is built ALONGSIDE the live loop; it does not yet replace
//! `calculate_priority`/`fast_path`. Cut-over lands once it's tested + the
//! recipe-executor (the servicing substrate) calls into it.
//!
//! ## Interface contract (for the recipe-executor `ai/should-respond` step)
//! - `Faculty::contribute` is **async** (backends do inference/IPC).
//! - It consumes `&Workspace` (the consolidated world-state + current broadcast).
//! - It returns `Option<Contribution>` (`None` = abstain this tick).
//! - Faculties are **per-persona instances** (each mind owns its faculties);
//!   model *backends* may be shared. Look one up by `FacultyId`.
//! - The participation result is a typed [`Decision`] carried by the
//!   deliberation faculty's contribution.

use std::sync::Arc;

use async_trait::async_trait;
use futures_util::future::join_all;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::llm_deliberation_faculty::{empty_genome, GenomeHandle};
use crate::ai::types::{ActiveAdapterRequest, ToolCall};

/// Identifier for a cognitive faculty — a *structural name* (like a brain
/// region), NOT a cognition decision. `Custom` keeps the set open so new
/// faculties (incl. sentinel-ai-forged ones) need no enum edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FacultyId {
    /// Sophisticated learned recall over engrams (hippocampal relevance).
    Recall,
    /// Generative model of the (multimodal, channel/recipe-shaped) world.
    WorldModel,
    /// Affect / arousal — neuromodulatory gain.
    Affect,
    /// Self-generated goals + curiosity (active inference policy proposals).
    Volition,
    /// The reasoner: produces the participation [`Decision`]. LLM-grade floor.
    Deliberation,
    /// Optional fast pre-attention salience (scheduling, never the decider).
    Salience,
    /// Open extension point — sentinel-ai faculties, future regions.
    Custom(String),
}

impl FacultyId {
    pub fn as_str(&self) -> &str {
        match self {
            FacultyId::Recall => "recall",
            FacultyId::WorldModel => "world-model",
            FacultyId::Affect => "affect",
            FacultyId::Volition => "volition",
            FacultyId::Deliberation => "deliberation",
            FacultyId::Salience => "salience",
            FacultyId::Custom(s) => s,
        }
    }
}

/// The persona's participation decision — the OUTPUT of the deliberation
/// faculty thinking over the consolidated burst. This is what a recipe
/// `ai/should-respond` step returns. It is a *thought's result*, not a gate:
/// silence (`Pass`) and unprompted initiative (`RaiseUnprompted`) are
/// first-class, equal to `Speak`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Decision {
    /// Respond to the thread with this content.
    Speak { text: String },
    /// Raise something no one asked for — initiative, not reaction.
    RaiseUnprompted { text: String },
    /// **Act on the world.** The mind reached for its hands — run code, search
    /// the web, read a file, drive its own avatar. This is a first-class verdict,
    /// peer to `Speak`: the deliberation faculty emits it when the model emitted
    /// tool calls. The driver executes the `calls` through the persona's
    /// identity-bearing `ToolExecutor` (the ACL gate decides what's allowed) and
    /// — crucially — the RESULT re-enters as an Episodic engram next tick, so the
    /// mind *perceives* what its hands did. It is NOT a synchronous call whose
    /// return value a faculty consumes inside one tick (that was the textbook
    /// inner loop we deleted); it is an action whose effect becomes memory.
    ///
    /// `intent` is the mind's own words for WHY it acted ("run the failing test
    /// to see the traceback"). It is captured into the observation engram so next
    /// tick she remembers the *reason*, not just the *result*. See
    /// `docs/cognition/ACTING-ORGANISM.md`.
    Act {
        calls: Vec<ToolCall>,
        intent: String,
    },
    /// Nothing worth adding this turn (the persona's own judgment, not a gate).
    /// Together with `Speak`/`RaiseUnprompted`, this is how the organism SETTLES:
    /// the absence of an `Act` bid is the mind's judgment that the work is done.
    Pass,
}

/// What a faculty surfaces into the workspace this service tick.
#[derive(Debug, Clone)]
pub struct Contribution {
    pub faculty: FacultyId,
    /// Human/LLM-readable content the faculty surfaces (recalled memory, a
    /// predicted world-state, an affect signal, a proposed utterance).
    pub content: String,
    /// **ML-derived** salience the faculty assigns its OWN contribution
    /// (`0.0..=1.0`): how much its model thinks this matters now. The arbiter
    /// integrates these ML scores; it never invents salience itself.
    pub salience: f32,
    /// Why — for audit/replay; the brain is observable.
    pub reasoning: String,
    /// Set by the deliberation faculty: the participation decision. Other
    /// faculties leave this `None` (they contribute context, not the verdict).
    pub decision: Option<Decision>,
}

impl Contribution {
    /// A context contribution (no decision) — recall, world-model, affect, etc.
    pub fn context(
        faculty: FacultyId,
        content: impl Into<String>,
        salience: f32,
        reasoning: impl Into<String>,
    ) -> Self {
        Self {
            faculty,
            content: content.into(),
            salience: salience.clamp(0.0, 1.0),
            reasoning: reasoning.into(),
            decision: None,
        }
    }

    /// The deliberation faculty's verdict contribution.
    pub fn verdict(decision: Decision, salience: f32, reasoning: impl Into<String>) -> Self {
        let content = match &decision {
            Decision::Speak { text } | Decision::RaiseUnprompted { text } => text.clone(),
            // The mind's narration of WHY it's acting — surfaced/audited like any
            // contribution content; the calls themselves live on the decision.
            Decision::Act { intent, .. } => intent.clone(),
            Decision::Pass => String::new(),
        };
        Self {
            faculty: FacultyId::Deliberation,
            content,
            salience: salience.clamp(0.0, 1.0),
            reasoning: reasoning.into(),
            decision: Some(decision),
        }
    }
}

/// The bounded global workspace: the consolidated world-state being reasoned
/// over (channel/recipe-shaped — a text thread, a game space + player
/// positions, an AR scene, a code diff) plus what won attention and is
/// broadcast back to all faculties.
#[derive(Debug, Clone)]
pub struct Workspace {
    /// The consolidated burst / world-state at service time. Opaque to the
    /// core — the channel/recipe adapter shapes it.
    pub world_state: String,
    /// The CONTEXT this tick reasons within — the room/conversation the turn is
    /// for (the third ID tier, contextId; see
    /// docs/architecture/IDENTITY-SCOPE-PEER-LIVENESS-MODEL.md Part A). Faculties
    /// scope their actions to it: the deliberation faculty stamps tool calls with
    /// this room so a persona's hands act in the SAME room the turn is about, not
    /// a phantom `nil` room. `Uuid::nil()` only in faculty-isolation tests that
    /// don't run in a room. NEVER a session id — context is durable, session is
    /// ephemeral and never load-bearing for where an action lands.
    pub room_id: Uuid,
    /// What entered the bounded workspace and is broadcast (the persona's "now").
    pub broadcast: Vec<Contribution>,
}

impl Workspace {
    pub fn new(world_state: impl Into<String>) -> Self {
        Self::in_room(world_state, Uuid::nil())
    }

    /// Construct scoped to a specific room/context (the contextId the turn acts
    /// within). The live persona path always uses this; `new` is the nil-room
    /// shorthand for faculty-isolation tests.
    pub fn in_room(world_state: impl Into<String>, room_id: Uuid) -> Self {
        Self {
            world_state: world_state.into(),
            room_id,
            broadcast: Vec::new(),
        }
    }

    /// The participation decision that won attention this tick, if any. It is
    /// the highest-salience contribution that carries a [`Decision`] — i.e. the
    /// deliberation faculty's verdict, if it made it into the bounded workspace.
    pub fn decision(&self) -> Option<&Decision> {
        self.broadcast
            .iter()
            .filter(|c| c.decision.is_some())
            .max_by(|a, b| {
                a.salience
                    .partial_cmp(&b.salience)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .and_then(|c| c.decision.as_ref())
    }
}

/// A cognitive faculty — a swappable ML adapter. The brain never knows whether
/// the backend is an LLM, a custom/sentinel-ai-forged specialist, or a
/// composite. Async because backends do real inference/IPC.
#[async_trait]
pub trait Faculty: Send + Sync {
    fn id(&self) -> FacultyId;
    /// Bid into the workspace given the current state. `None` = abstain.
    ///
    /// Called once in the faculty's phase (see [`Faculty::reacts_to_broadcast`]).
    /// Perception faculties see an empty `ws.broadcast` (they react to the raw
    /// world-state); deliberation faculties see the *assembled context* that won
    /// attention in phase 1. A faculty's intelligence is entirely here — the
    /// arbiter only integrates the salience it returns.
    async fn contribute(&self, ws: &Workspace) -> Option<Contribution>;

    /// The faculty's **dependency / phase** in the staged cycle — the reactive
    /// "what do I fire on" declaration (a faculty is a React hook; this is its
    /// dependency array). This is the direct analog of cbar's `needsRealTime()`
    /// bool that split real-time motion from delayed scene understanding: a
    /// *structural* scheduling declaration, **not** a cognition gate.
    ///
    /// - `false` (default) — **perception tier**: reacts to the raw world-state,
    ///   bids in phase 1 (recall, world-model, affect, salience, roster…).
    /// - `true` — **deliberation tier**: reacts to the *assembled broadcast* (the
    ///   context that won attention), bids in phase 2 so it can condition its
    ///   [`Decision`] on what recall/world-model/affect actually surfaced.
    ///
    /// This is what makes "pull relevant memory, *then* decide" expressible: the
    /// decider runs after, over the assembled context — cbar's lines→planes, GWT's
    /// broadcast-then-rebid. It does NOT enumerate faculties or privilege a
    /// decider; any faculty may be either tier.
    fn reacts_to_broadcast(&self) -> bool {
        false
    }
}

/// Attention: selects which contributions enter the bounded workspace.
/// Pluggable so a *learned* arbiter (itself a faculty) can replace the
/// bootstrap. The bootstrap is a top-k over the faculties' OWN ML-derived
/// salience — mechanical integration of ML scores (like attention's softmax
/// top-k), NOT a hand-coded cognition rule. The intelligence lives in the
/// faculties; the arbiter only integrates it.
pub trait Arbiter: Send + Sync {
    fn select(&self, candidates: Vec<Contribution>, capacity: usize) -> Vec<Contribution>;
}

/// Top-k by ML salience within the workspace's bounded capacity.
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
    fn select(&self, mut candidates: Vec<Contribution>, capacity: usize) -> Vec<Contribution> {
        candidates.sort_by(|a, b| {
            b.salience
                .partial_cmp(&a.salience)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        candidates.truncate(capacity);
        candidates
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
    pub world_state: String,
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
    pub admission: Arc<crate::persona::admission_state::AdmissionState>,
}

/// One service-tick of cognition over a CONSOLIDATED burst (never per-event):
/// every faculty bids in parallel over the same world-state, the arbiter
/// integrates the bids into the bounded workspace, the workspace broadcasts.
/// The participation [`Decision`] is then read from the broadcast.
pub struct WorkspaceCycle {
    faculties: Vec<Arc<dyn Faculty>>,
    arbiter: Arc<dyn Arbiter>,
    /// Bound on how many contributions can hold the workspace at once — the
    /// finite "spotlight" of attention.
    capacity: usize,
    /// Replay/logging seam — every tick is recorded here. Default `Noop`
    /// (zero hot-path cost); swap in a recording sink to make the mind a
    /// glass box for debugging, tuning, and test benches.
    capture: Arc<dyn WorkspaceCaptureSink>,
    /// The persona's hands + hippocampus + identity for the act→observe driver.
    /// `None` → no hands (pure cognition). See [`ActingBody`].
    acting: Option<Arc<ActingBody>>,
    /// The persona's paged-in genome — the LoRA layers active for generation. The
    /// deliberation faculty shares this exact handle and reads it wait-free on
    /// every generation; [`page_in`](Self::page_in)/[`page_out`](Self::page_out)
    /// swap which gene is active (virtual memory for skill). Empty → base model.
    /// This is the seam `cognition/eval` A/Bs base vs a candidate gene over.
    genome: GenomeHandle,
}

/// RAII guard for a memory-isolated measurement window over a cycle's
/// hippocampus — see [`WorkspaceCycle::isolate_for_eval`]. Holds the persona's
/// `AdmissionState`, the frame to rewind to, and the real persistence sink to
/// restore. All `Option` so a pure-cognition cycle (no hands) yields a benign
/// no-op guard. Not `Clone` — the restore must happen exactly once, on drop.
pub struct EvalIsolation {
    admission: Option<Arc<crate::persona::admission_state::AdmissionState>>,
    checkpoint: Option<crate::persona::admission_state::AdmissionCheckpoint>,
    real_sink:
        Option<Arc<dyn crate::persona::admission_persistence::AdmissionPersistenceSink>>,
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
        let Some(admission) = &self.admission else { return };
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

impl WorkspaceCycle {
    pub fn new(
        faculties: Vec<Arc<dyn Faculty>>,
        arbiter: Arc<dyn Arbiter>,
        capacity: usize,
    ) -> Self {
        Self {
            faculties,
            arbiter,
            capacity: capacity.max(1),
            capture: Arc::new(NoopWorkspaceCaptureSink),
            acting: None,
            genome: empty_genome(),
        }
    }

    /// Share the genome handle the deliberation faculty reads — call with the SAME
    /// [`GenomeHandle`] passed to [`LlmDeliberationFaculty::with_genome`] so a
    /// page-in on the cycle takes effect on the faculty's next generation.
    pub fn with_genome(mut self, genome: GenomeHandle) -> Self {
        self.genome = genome;
        self
    }

    /// Page a gene (set of LoRA layers) into the persona's genome — the next
    /// generation runs the base model adapted by these layers. Wait-free swap.
    /// This is the measured page-in: the genome loop pages in a freshly forged
    /// gene here and `cognition/eval` measures the lift it produced.
    pub fn page_in(&self, adapters: Vec<ActiveAdapterRequest>) {
        self.genome.store(Arc::new(adapters));
    }

    /// Page out all genes — the persona reverts to the base model (no LoRA). The
    /// baseline arm of an A/B, and the clean state to leave a persona in.
    pub fn page_out(&self) {
        self.genome.store(Arc::new(Vec::new()));
    }

    /// The persona's currently paged-in genome (a snapshot).
    pub fn genome(&self) -> Vec<ActiveAdapterRequest> {
        self.genome.load().as_ref().clone()
    }

    /// Begin a memory-isolated measurement window over this cycle's hippocampus.
    ///
    /// `cognition/eval` drives the persona's REAL admission as it grades her, so
    /// the act-observations a run admits would otherwise (1) drift her absolute
    /// score run-to-run, (2) order-bias a paired A/B (the second arm inherits the
    /// first arm's writes), and (3) pollute her durable sqlite. While the returned
    /// guard is alive, admission STILL fires — the measured memory motion is
    /// identical to a real turn, which is what keeps the measurement valid — but
    /// the persistence sink is muted (nothing reaches disk) and the in-memory
    /// admission frame is checkpointed. Call [`EvalIsolation::rewind`] between A/B
    /// arms so both start from identical memory; dropping the guard restores the
    /// memory and the real sink. A pure-cognition cycle (no hands → nothing is
    /// admitted) yields a no-op guard. See
    /// [[eval-mutates-persona-lift-needs-isolation]].
    pub fn isolate_for_eval(&self) -> EvalIsolation {
        let Some(acting) = &self.acting else {
            return EvalIsolation { admission: None, checkpoint: None, real_sink: None };
        };
        let admission = acting.admission.clone();
        let checkpoint = admission.checkpoint();
        let real_sink = admission
            .swap_persistence(crate::persona::admission_persistence::NoopSink::arc());
        EvalIsolation {
            admission: Some(admission),
            checkpoint: Some(checkpoint),
            real_sink: Some(real_sink),
        }
    }

    /// Install a capture sink (recording / on-disk replay / in-flight inspection).
    pub fn with_capture(mut self, capture: Arc<dyn WorkspaceCaptureSink>) -> Self {
        self.capture = capture;
        self
    }

    /// Give this mind a body — the hands + hippocampus + identity the act→observe
    /// driver uses to execute an [`Decision::Act`], remember its result, and
    /// re-perceive. Without it the cycle is pure cognition (no acting). See
    /// [`ActingBody`].
    pub fn with_acting(mut self, body: Arc<ActingBody>) -> Self {
        self.acting = Some(body);
        self
    }

    /// The persona's body, if it has hands. The act→observe driver reads this;
    /// `None` → pure-cognition mind (no tools were offered, so no `Act` arises).
    pub fn acting(&self) -> Option<&Arc<ActingBody>> {
        self.acting.as_ref()
    }

    /// Run one cognition tick over the consolidated `world_state`, recording the
    /// full trace (every bid incl. losers, what won, the decision) for replay.
    ///
    /// **Staged assembly** (cbar lines→planes / GWT broadcast-then-rebid):
    /// 1. **Perception phase** — faculties with `reacts_to_broadcast() == false`
    ///    bid in parallel over the raw world-state (broadcast still empty). The
    ///    arbiter routes the salient subset into the broadcast: this is the
    ///    *assembled context* (the "RAG" the decider will read).
    /// 2. **Deliberation phase** — faculties with `reacts_to_broadcast() == true`
    ///    bid in parallel over the workspace whose broadcast now holds the
    ///    assembled context, so the [`Decision`] is conditioned on what recall /
    ///    world-model / affect actually surfaced. Their bids append to the
    ///    broadcast.
    ///
    /// This is what makes "pull relevant memory, *then* decide" real: the decider
    /// is never blind to recall. Still one tick over the consolidated burst, still
    /// `O(capacity)` for the bounded context — no per-event slowdown.
    pub async fn run(&self, world_state: impl Into<String>) -> Workspace {
        self.run_in_room(world_state, Uuid::nil()).await
    }

    /// Same as [`run`](Self::run) but scoped to a room/context (the contextId the
    /// turn acts within). The live persona path uses THIS so the deliberation
    /// faculty stamps tool calls with the real room — `run` is the nil-room
    /// shorthand for tests that aren't room-scoped.
    pub async fn run_in_room(&self, world_state: impl Into<String>, room_id: Uuid) -> Workspace {
        let mut ws = Workspace::in_room(world_state, room_id);

        // --- Phase 1: perception. Context faculties react to the raw world-state. ---
        let perception: Vec<&Arc<dyn Faculty>> = self
            .faculties
            .iter()
            .filter(|f| !f.reacts_to_broadcast())
            .collect();
        let context_bids: Vec<Contribution> =
            join_all(perception.iter().map(|f| f.contribute(&ws)))
                .await
                .into_iter()
                .flatten()
                .collect();
        // Route the salient subset into the bounded workspace — the arbiter is the
        // attention ROUTER over information flow, not a gate. The winners are the
        // assembled context the deliberation faculty reasons over.
        ws.broadcast = self.arbiter.select(context_bids.clone(), self.capacity);
        let context_broadcast = ws.broadcast.clone();

        // --- Phase 2: deliberation. Reacts to the assembled broadcast (it can now
        // see what recall/world-model/affect surfaced) and emits the verdict. ---
        let deliberation: Vec<&Arc<dyn Faculty>> = self
            .faculties
            .iter()
            .filter(|f| f.reacts_to_broadcast())
            .collect();
        let decision_bids: Vec<Contribution> =
            join_all(deliberation.iter().map(|f| f.contribute(&ws)))
                .await
                .into_iter()
                .flatten()
                .collect();
        // The deliberation output is the RESULT of attending to the context, not a
        // competitor for the bounded context spotlight — append it to the broadcast.
        ws.broadcast.extend(decision_bids.iter().cloned());

        // Capture the FULL competition: every bid across both phases (incl. the
        // losers you need to debug "why didn't X win?"), the assembled context the
        // decider saw, the final broadcast, and the decision.
        let mut all_bids = context_bids;
        all_bids.extend(decision_bids);
        self.capture.record(&WorkspaceTrace {
            world_state: ws.world_state.clone(),
            room_id: ws.room_id,
            bids: all_bids,
            context_broadcast,
            broadcast: ws.broadcast.clone(),
            decision: ws.decision().cloned(),
        });
        ws
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A canned faculty for tests — fixed contribution + salience.
    struct FixedFaculty(Contribution);
    #[async_trait]
    impl Faculty for FixedFaculty {
        fn id(&self) -> FacultyId {
            self.0.faculty.clone()
        }
        async fn contribute(&self, _ws: &Workspace) -> Option<Contribution> {
            Some(self.0.clone())
        }
    }

    /// A faculty that abstains this tick.
    struct AbstainFaculty(FacultyId);
    #[async_trait]
    impl Faculty for AbstainFaculty {
        fn id(&self) -> FacultyId {
            self.0.clone()
        }
        async fn contribute(&self, _ws: &Workspace) -> Option<Contribution> {
            None
        }
    }

    /// A deliberation faculty that CONDITIONS its decision on the assembled
    /// broadcast: it speaks (referencing what it saw) only if recall surfaced
    /// something; otherwise it passes. This is the probe for staged assembly —
    /// under the old single-pass join_all it would always see an empty broadcast
    /// and could never be informed by recall.
    struct ConditionalDeliberation;
    #[async_trait]
    impl Faculty for ConditionalDeliberation {
        fn id(&self) -> FacultyId {
            FacultyId::Deliberation
        }
        fn reacts_to_broadcast(&self) -> bool {
            true
        }
        async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
            // Look at the assembled context that won attention in phase 1.
            let recalled = ws.broadcast.iter().find(|c| c.faculty == FacultyId::Recall);
            match recalled {
                Some(mem) => Some(Contribution::verdict(
                    Decision::Speak {
                        text: format!("informed by recall: {}", mem.content),
                    },
                    0.9,
                    "conditioned the reply on the recalled context",
                )),
                None => Some(Contribution::verdict(
                    Decision::Pass,
                    0.5,
                    "blind — no context in the broadcast",
                )),
            }
        }
    }

    fn cycle(faculties: Vec<Arc<dyn Faculty>>, capacity: usize) -> WorkspaceCycle {
        WorkspaceCycle::new(faculties, Arc::new(SalienceArbiter), capacity)
    }

    // what this catches: the cycle's genome page-in/page-out round-trips through the
    // shared handle — page_in publishes a gene the next generation reads, page_out
    // reverts to the clean base. This is the A/B's lever (eval pages a gene in/out
    // around two passes); if it didn't round-trip, the candidate and base arms
    // would measure the same genome and every lift would read as zero.
    #[test]
    fn genome_page_in_and_out_round_trips() {
        let c = cycle(vec![], 4);
        assert!(c.genome().is_empty(), "a fresh cycle starts on the base model");

        c.page_in(vec![ActiveAdapterRequest {
            name: "coder-0p5b".to_string(),
            path: "/genes/coder.gguf".to_string(),
            domain: String::new(),
            scale: 0.8,
        }]);
        let paged = c.genome();
        assert_eq!(paged.len(), 1, "the gene is now the active genome");
        assert_eq!(paged[0].name, "coder-0p5b");
        assert_eq!(paged[0].scale, 0.8);

        c.page_out();
        assert!(c.genome().is_empty(), "page_out reverts to the clean base");
    }

    // what this catches: attention is a competition over ML-derived salience —
    // the highest-salience faculty bids win the bounded workspace. This is the
    // ML-integration replacement for calculate_priority's hand-weights.
    #[tokio::test]
    async fn arbiter_selects_top_k_by_ml_salience() {
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::Recall,
                "low",
                0.2,
                "weak recall",
            ))),
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::WorldModel,
                "high",
                0.9,
                "strong signal",
            ))),
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::Affect,
                "mid",
                0.5,
                "some arousal",
            ))),
        ];
        let ws = cycle(faculties, 2).run("the consolidated thread").await;
        assert_eq!(ws.broadcast.len(), 2, "bounded capacity");
        assert_eq!(ws.broadcast[0].content, "high", "highest ML salience wins");
        assert_eq!(ws.broadcast[1].content, "mid");
    }

    // what this catches: ACTING-ORGANISM step 1 — the mind can express "I want to
    // act on the world" as a first-class verdict. A deliberation faculty emits
    // Decision::Act{calls,intent}; the arbiter routes it like any decision, and
    // decision() returns it carrying the calls + the mind's narrated intent. This
    // is the vocabulary the genome learns to use (the disposition is trained, never
    // hardcoded); the executor that runs the calls and re-enters the result as a
    // memory is steps 3–4. Act is peer to Speak, not a special case.
    #[tokio::test]
    async fn deliberation_can_emit_an_act_decision() {
        let call = ToolCall {
            id: "toolu_run_1".to_string(),
            name: "code/run".to_string(),
            input: serde_json::json!({ "lang": "rust", "code": "fn main() { println!(\"{}\", (0..5).sum::<i32>()); }" }),
        };
        let act = Decision::Act {
            calls: vec![call.clone()],
            intent: "run my solution to see what it actually prints".to_string(),
        };
        let faculties: Vec<Arc<dyn Faculty>> = vec![Arc::new(FixedFaculty(Contribution::verdict(
            act,
            0.95,
            "the model emitted a tool call",
        )))];
        let ws = cycle(faculties, 4).run("peer: does your sum work?").await;
        match ws.decision() {
            Some(Decision::Act { calls, intent }) => {
                assert_eq!(calls.as_slice(), std::slice::from_ref(&call));
                assert!(
                    intent.contains("run my solution"),
                    "the mind's narrated intent rides on the Act decision"
                );
            }
            other => panic!("expected an Act verdict to route through the arbiter, got {other:?}"),
        }
        // verdict() surfaces the intent as the contribution content (audited like
        // any bid) while the calls live on the decision.
        assert_eq!(
            ws.broadcast[0].content,
            "run my solution to see what it actually prints"
        );
    }

    // what this catches: EQUAL CITIZENS — a persona-sent message with high
    // relevance beats a human-sent one with low relevance. Salience (ML) decides,
    // never the sender's rank. This is the death of the Human=1.0/Persona=0.3
    // caste: there is nowhere in this core to encode it.
    #[tokio::test]
    async fn salience_decides_not_sender_caste() {
        // Two world-model bids; the "from a persona" one is more relevant.
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::WorldModel,
                "human said hi (low value)",
                0.3,
                "pleasantry",
            ))),
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::WorldModel,
                "peer persona flagged a real blocker (high value)",
                0.85,
                "actionable",
            ))),
        ];
        let ws = cycle(faculties, 1).run("burst").await;
        assert_eq!(ws.broadcast.len(), 1);
        assert!(
            ws.broadcast[0].content.contains("peer persona"),
            "the relevant peer-persona content wins on salience, not the human's rank"
        );
    }

    // what this catches: the participation Decision is the OUTPUT of the
    // deliberation faculty's thought, surfaced from the workspace — not a gate.
    // Speak / RaiseUnprompted / Pass are all first-class.
    #[tokio::test]
    async fn decision_is_the_deliberation_faculty_output() {
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::Recall,
                "context",
                0.4,
                "recalled",
            ))),
            Arc::new(FixedFaculty(Contribution::verdict(
                Decision::RaiseUnprompted {
                    text: "blocker on the deploy".into(),
                },
                0.95,
                "high epistemic value — no one raised it",
            ))),
        ];
        let ws = cycle(faculties, 5).run("coordination thread").await;
        match ws.decision() {
            Some(Decision::RaiseUnprompted { text }) => {
                assert_eq!(text, "blocker on the deploy")
            }
            other => panic!("expected unprompted initiative, got {other:?}"),
        }
    }

    // what this catches: silence (Pass) is a first-class judgment, and abstaining
    // faculties simply don't contribute — no panic, no gate.
    #[tokio::test]
    async fn pass_and_abstain_are_first_class() {
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(AbstainFaculty(FacultyId::Recall)),
            Arc::new(FixedFaculty(Contribution::verdict(
                Decision::Pass,
                0.6,
                "nothing worth adding",
            ))),
        ];
        let ws = cycle(faculties, 5).run("idle chatter").await;
        assert_eq!(
            ws.broadcast.len(),
            1,
            "the abstaining faculty added nothing"
        );
        assert_eq!(ws.decision(), Some(&Decision::Pass));
    }

    // what this catches: one cycle runs over a CONSOLIDATED burst (the whole
    // world-state at once), not per-event — the efficiency spine.
    #[tokio::test]
    async fn runs_once_over_a_consolidated_burst() {
        let consolidated = "msg1\nmsg2\nmsg3\nmsg4 (many events, one unit)";
        let faculties: Vec<Arc<dyn Faculty>> = vec![Arc::new(FixedFaculty(Contribution::verdict(
            Decision::Speak {
                text: "one reply to the whole thread".into(),
            },
            0.8,
            "caught up on the backlog",
        )))];
        let ws = cycle(faculties, 5).run(consolidated).await;
        assert!(ws.world_state.contains("many events, one unit"));
        assert!(matches!(ws.decision(), Some(Decision::Speak { .. })));
    }

    /// In-memory capture sink — the test-bench / replay primitive.
    #[derive(Default)]
    struct RecordingSink(std::sync::Mutex<Vec<WorkspaceTrace>>);
    impl WorkspaceCaptureSink for RecordingSink {
        fn record(&self, trace: &WorkspaceTrace) {
            self.0.lock().unwrap().push(trace.clone());
        }
    }

    // what this catches: every tick is replayable — the trace captures the FULL
    // competition incl. the LOSER bid (the bit you need to debug "why didn't it
    // win?"), what won, and the decision. This is the glass box that makes
    // working on the mind debuggable + test-benchable, not guesswork.
    #[tokio::test]
    async fn capture_records_the_full_tick_including_losers() {
        let sink = Arc::new(RecordingSink::default());
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::Recall,
                "loser bid",
                0.1,
                "weak — should lose attention",
            ))),
            Arc::new(FixedFaculty(Contribution::verdict(
                Decision::Speak {
                    text: "winner".into(),
                },
                0.9,
                "high value",
            ))),
        ];
        // capacity 1 → only the winner is broadcast, but the trace keeps both.
        let _ws = WorkspaceCycle::new(faculties, Arc::new(SalienceArbiter), 1)
            .with_capture(sink.clone())
            .run("thread")
            .await;

        let traces = sink.0.lock().unwrap();
        assert_eq!(traces.len(), 1, "one tick recorded");
        let t = &traces[0];
        assert_eq!(t.bids.len(), 2, "trace keeps ALL bids — winner AND loser");
        assert_eq!(t.broadcast.len(), 1, "only the winner held attention");
        assert!(
            t.bids.iter().any(|b| b.content == "loser bid"),
            "the loser bid + its salience + reasoning are replayable for debugging"
        );
        assert_eq!(
            t.decision,
            Some(Decision::Speak {
                text: "winner".into()
            })
        );
    }

    // what this catches: STAGED ASSEMBLY — the deliberation faculty conditions its
    // Decision on what recall surfaced in phase 1. This is the coherence fix: "pull
    // relevant memory, THEN decide." Under the old single-pass join_all the decider
    // bid over an EMPTY broadcast and could never be informed by recall.
    #[tokio::test]
    async fn deliberation_sees_the_recall_that_won_phase_one() {
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            // Phase 1 (perception): recall surfaces a memory with strong salience.
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::Recall,
                "deploy pipeline is red",
                0.8,
                "recalled the open blocker",
            ))),
            // Phase 2 (deliberation): reacts to the assembled broadcast.
            Arc::new(ConditionalDeliberation),
        ];
        let ws = cycle(faculties, 5).run("what's the status?").await;
        match ws.decision() {
            Some(Decision::Speak { text }) => assert!(
                text.contains("deploy pipeline is red"),
                "the decider must condition on recall it saw, got: {text}"
            ),
            other => {
                panic!("expected an informed Speak, got {other:?} — decider was blind to recall")
            }
        }
    }

    // what this catches: the trace exposes the ASSEMBLED CONTEXT the decider saw
    // (context_broadcast) separately from the final broadcast — the glass-box
    // answer to "what RAG did the mind reason over?" — and the deliberation output
    // is appended, not competing for the bounded context spotlight.
    #[tokio::test]
    async fn trace_separates_assembled_context_from_deliberation_output() {
        let sink = Arc::new(RecordingSink::default());
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::Recall,
                "deploy pipeline is red",
                0.8,
                "recalled",
            ))),
            Arc::new(ConditionalDeliberation),
        ];
        let _ws = WorkspaceCycle::new(faculties, Arc::new(SalienceArbiter), 5)
            .with_capture(sink.clone())
            .run("status?")
            .await;

        let traces = sink.0.lock().unwrap();
        let t = &traces[0];
        // The assembled context is exactly the phase-1 recall winner.
        assert_eq!(t.context_broadcast.len(), 1);
        assert_eq!(t.context_broadcast[0].faculty, FacultyId::Recall);
        // The final broadcast holds the context PLUS the deliberation verdict.
        assert_eq!(
            t.broadcast.len(),
            2,
            "assembled context + deliberation output"
        );
        assert!(
            t.broadcast.iter().any(|c| c.decision.is_some()),
            "verdict appended"
        );
        // Both phases' bids are in the full competition record.
        assert_eq!(t.bids.len(), 2);
    }

    // what this catches: a perception faculty does NOT bid in the deliberation
    // phase (reacts_to_broadcast == false), and a deliberation faculty does NOT
    // bid in the perception phase — each faculty fires once, in its tier. This is
    // the cbar needsRealTime() split: no double inference.
    #[tokio::test]
    async fn faculties_fire_only_in_their_declared_phase() {
        // A faculty that records how many times it was asked to contribute, and in
        // what broadcast state, so we can prove single-phase firing.
        struct CountingDeliberation(std::sync::Arc<std::sync::atomic::AtomicUsize>);
        #[async_trait]
        impl Faculty for CountingDeliberation {
            fn id(&self) -> FacultyId {
                FacultyId::Deliberation
            }
            fn reacts_to_broadcast(&self) -> bool {
                true
            }
            async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                // If we are ever called, the broadcast must be populated (phase 2).
                assert!(
                    !ws.broadcast.is_empty(),
                    "deliberation must only fire after phase 1 assembled context"
                );
                Some(Contribution::verdict(Decision::Pass, 0.5, "noted"))
            }
        }
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::WorldModel,
                "ctx",
                0.7,
                "perception",
            ))),
            Arc::new(CountingDeliberation(calls.clone())),
        ];
        let _ws = cycle(faculties, 5).run("burst").await;
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "deliberation faculty fired exactly once, in its phase"
        );
    }
}
