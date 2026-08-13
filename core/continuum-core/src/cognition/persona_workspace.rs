//! Per-persona workspace assembly + registry — the "one soul, many rooms" seam.
//!
//! This is the constructor `ai/should-respond` (and the bring-up harness) resolve
//! a persona's mind through. The load-bearing decision (PERSONA-BRAIN-
//! ARCHITECTURE.md §2.9) is structural: **one `WorkspaceCycle` per persona**,
//! keyed by `persona_id` — NOT by `(persona_id, room_id)`. A persona is one
//! continuous self across every room it services; its unified `AdmissionState`
//! (the hippocampus) spans all its activities. Keying the registry by persona is
//! what makes the citizen continuous instead of *severed* per-room.
//!
//! The same cycle is invoked for whatever room the persona is servicing; the room
//! supplies the per-tick world-state (the consolidated burst), the persona
//! supplies the unified memory + identity + faculties.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

// parking_lot::Mutex, not std::sync::Mutex — matching `working_memory.rs`, this module's
// sibling holding the same kind of per-persona state. std's `lock()` returns a Result that
// is Err only when ANOTHER thread panicked while holding it, so `.lock()` converts
// one persona's failure into a process-wide cascade: the registry every other persona needs
// dies with it. parking_lot does not poison, so there is no Result and no unwrap to get
// wrong. This also collapses two mutex types for one concern back to one.
use parking_lot::Mutex;

use uuid::Uuid;

use super::deferred_faculty::DeferredFaculty;
use super::embedding::{CachingEmbeddingProvider, EmbeddingProvider, LexicalEmbedder};
use super::llm_deliberation_faculty::LlmDeliberationFaculty;
use super::rag_source_faculty::{RagSourceFaculty, SaliencePolicy};
use super::recall_faculty::RecallFaculty;
use super::working_memory::{WorkingMemory, WorkingMemoryFaculty};
use super::workspace::{ActingBody, Faculty, SituationFocusArbiter, WorkspaceCycle};
use crate::ai::adapter::AIProviderAdapter;
use crate::persona::admission_state::AdmissionState;
use crate::persona::rag_budget::RagSource;

/// Default bounded workspace capacity — the finite attention "spotlight". Enough
/// for recall + world-model + affect + roster context to coexist; the arbiter
/// keeps it bounded so cost stays O(capacity) no matter how many faculties bid.
pub const DEFAULT_WORKSPACE_CAPACITY: usize = 6;

/// Everything needed to assemble one persona's continuous mind. The `admission`
/// is the persona's UNIFIED hippocampus (shared with the admission pipeline and
/// spanning all the persona's rooms); the `adapter` is the shared model backend,
/// leased inside the deliberation faculty.
///
/// `Clone` is cheap — every field is an `Arc`/`String`/`Uuid`/`Option` handle —
/// and the registry retains a clone as a fork-template so `cognition/eval` can
/// fork an ephemeral measurement copy of the mind without touching the living
/// persona (see [`PersonaWorkspaceRegistry::fork_eval_cycle`]).
#[derive(Clone)]
pub struct PersonaBrainConfig {
    pub persona_id: Uuid,
    pub persona_name: String,
    /// The persona's identity / deliberation system prompt (from RAG identity).
    pub system_prompt: String,
    pub admission: Arc<AdmissionState>,
    pub adapter: Arc<dyn AIProviderAdapter>,
    /// Bounded workspace capacity; `None` → [`DEFAULT_WORKSPACE_CAPACITY`].
    pub capacity: Option<usize>,
    /// Grounding RagSources lifted into perception-tier faculties via
    /// [`RagSourceFaculty`] (the migration bridge — see its module doc). Each is
    /// paired with a [`SaliencePolicy`]: roster + doctrine are `StandingFraming`
    /// (a high salience floor so attention pressure can't evict the room's own
    /// rules); retrieved sources would be `Retrieved`. Empty in bring-up harnesses
    /// that only need recall + deliberation. This is the assembly-layer
    /// classification BigMama's separation-of-concerns requires: the salience
    /// policy lives HERE, never inside `RagSource`.
    pub grounding_sources: Vec<GroundingSource>,
    /// The recall embedder for this persona's hippocampus. `None` → the lexical
    /// bootstrap (works on any machine, zero deps). The live spawn path sets
    /// `Some` via [`resolve_recall_embedder`], which prefers the neural embedder
    /// when the embed model serves and falls back to lexical otherwise. Already
    /// wrapped in the content-addressed cache by the resolver, so it's used as-is.
    ///
    /// [`resolve_recall_embedder`]: super::embedding::resolve_recall_embedder
    pub embedder: Option<Arc<dyn EmbeddingProvider>>,
    /// The persona's HANDS. `Some` → the deliberation faculty is offered the
    /// identity-gated tool surface ([`authorized_tool_specs`], filtered to the
    /// caller's trust) and routes the model's tool calls through this executor (a
    /// `CommandToolExecutor` carrying the persona's identity, so the
    /// `GridTrustAuthPolicy` ACL gates execution). Offer == authorized, so a
    /// persona is never shown a tool it can't run.
    /// `None` → speak-only (no tools offered) — the safe default for harnesses and
    /// for any persona whose spawn path hasn't built an executor.
    ///
    /// [`authorized_tool_specs`]: super::persona_tools::authorized_tool_specs
    pub tool_executor: Option<Arc<dyn crate::cognition::tool_executor::ToolExecutor>>,
    /// The effective served context window in tokens — `profile.context_length`
    /// (task #50: single-sourced; for a Local persona that is the planner's
    /// `ServingPlan.served_context_window`).
    /// Threaded into the deliberation faculty so it keeps its prompt within the
    /// window the gateway actually serves — the prompt is built here, so this is
    /// where the window invariant is enforced. Without it the faculty's prompt
    /// overflows `-c` and llama-server 500s ("Context size has been exceeded").
    pub context_window: u32,
    /// Run recall as a SPECULATIVE PREFETCH off the hot path (Joel's CPU analogy:
    /// recall is the prefetch — always run it speculatively in idle time between
    /// turns, so the turn reads a warm last-good instead of paying the
    /// neural-embed + vector-search latency on the critical path). `true` on the
    /// LIVE paths (turns are seconds apart, so the background worker always
    /// catches up → warm recall). `false` for eval forks and harnesses, whose
    /// tight settle-loops never yield to the worker, so they'd measure a
    /// recall-STARVED mind — they keep synchronous recall (faithful, the safe
    /// direction: eval never under-reports capability). Wrapping is
    /// [`DeferredFaculty`]; the cold-start (first tick in a room) is a guaranteed
    /// prefetch miss, exactly like a cold branch predictor, then warm thereafter.
    ///
    /// [`DeferredFaculty`]: super::deferred_faculty::DeferredFaculty
    pub defer_recall: bool,
    /// Push the [`Deferrability::DeferTolerant`] grounding sources off the hot path
    /// too (same speculative-prefetch rationale as `defer_recall`: the enriching
    /// framing — roster, active_work, workspace_map — runs in the bg and the turn
    /// reads a reprojected last-good). `ColdStartCritical` sources (doctrine, the
    /// participation gate) stay synchronous regardless. `true` on the LIVE paths;
    /// `false` for eval forks + harnesses whose tight settle-loops never yield to
    /// the worker (they'd measure a grounding-starved mind). Reproject-to-now is
    /// what makes this safe — without it a deferred source would serve stale
    /// verbatim. See [`DeferredFaculty`] and [`reproject_to_now`].
    ///
    /// [`reproject_to_now`]: super::deferred_faculty::reproject_to_now
    pub defer_grounding: bool,
    /// SUPPRESS the recall faculty entirely — the fork's mind carries system + task +
    /// grounding (roster/doctrine/workspace-map) but injects NO episodic recall. `false`
    /// on every live path and by default (memories intact — the natural persona). `true`
    /// ONLY for a reproducible ABSOLUTE benchmark baseline: a self-contained proctored task
    /// (HumanEval, a repo-nav question, a from-scratch UI build) is answerable from the
    /// system+task alone, so injecting her durable episodic store — which GROWS as she lives
    /// between runs — makes the exam prompt drift run-to-run (unrelated room chatter recalled
    /// into a self-contained task = noise AND nondeterminism). The LIFT (base vs gene in one
    /// fork) is reproducible regardless; this pins the ABSOLUTE number so today's run compares
    /// to last week's. NOT a life-path knob — a benchmark-reproducibility control, sibling of
    /// the greedy-temperature and directed-turn pins. See #207,
    /// [[eval-reproducibility-is-two-tier-lift-controlled-absolute-drifts]],
    /// [[redaction-makes-exam-learning-honest]].
    pub suppress_recall: bool,
}

/// Whether a grounding source must run synchronously on the inference loop, or
/// can run in the background and serve a reprojected last-good. **Orthogonal to
/// [`SaliencePolicy`]** — a source can be `StandingFraming` (high salience floor)
/// AND `DeferTolerant` at the same time; the two axes are independent (salience =
/// how hard it bids when present; deferrability = whether it must be present on
/// the very first tick). This is the "async is a percentage, not a binary"
/// classification made concrete: almost every concern is defer-tolerant; the
/// exception is the one whose cold-start miss is unacceptable.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Deferrability {
    /// Must run on the loop. A first-tick `None` (the cold-start miss every
    /// deferred faculty has) would be *wrong*, not merely unenriched — this is the
    /// participation gate (doctrine: "speaks when it shouldn't" is worse than a
    /// missed enrichment), so it pays the synchronous cost to be correct turn one.
    ColdStartCritical,
    /// May run in the background ([`DeferredFaculty`]) and serve a reprojected
    /// last-good. Enriching framing (roster, active_work, workspace_map) whose
    /// cold-start miss only costs ONE under-grounded first tick, then warm
    /// thereafter — the 90%-async win, made safe by reproject-to-now.
    ///
    /// [`DeferredFaculty`]: super::deferred_faculty::DeferredFaculty
    DeferTolerant,
}

/// A grounding [`RagSource`] plus the [`SaliencePolicy`] under which it competes
/// for attention and the [`Deferrability`] that decides whether it runs on or off
/// the loop. Classified by whoever assembles the cycle (the spawn path), keeping
/// `RagSource` itself salience- and schedule-free.
#[derive(Clone)]
pub struct GroundingSource {
    pub source: Arc<dyn RagSource>,
    pub policy: SaliencePolicy,
    /// Defaults to [`Deferrability::ColdStartCritical`] (the safe direction: a new
    /// source runs synchronously until its assembler deliberately opts it into the
    /// bg via [`GroundingSource::defer_tolerant`], having decided its first-tick
    /// miss is tolerable).
    pub deferrability: Deferrability,
    /// CAPABILITY CONSISTENCY: this grounding DESCRIBES the persona's hands
    /// (tool paths, "drill in with code/list…"), so it must only be delivered
    /// into a cycle that HAS hands. Glass-boxed 2026-07-10: every spoken-graded
    /// exam prompt carried the workspace-map telling her to use tools that were
    /// stripped for the exam — the RAG lying to her about her own affordances.
    /// A perception surface must never describe affordances that don't exist
    /// this cycle. `false` (default) = capability-neutral grounding.
    pub requires_hands: bool,
}

impl GroundingSource {
    /// Standing framing (roster, doctrine) — always-present structural context.
    /// Cold-start-critical by default; call [`Self::defer_tolerant`] to move an
    /// enriching framing source off the loop.
    pub fn framing(source: Arc<dyn RagSource>) -> Self {
        Self {
            source,
            policy: SaliencePolicy::StandingFraming,
            deferrability: Deferrability::ColdStartCritical,
            requires_hands: false,
        }
    }

    /// Mark this grounding as DESCRIBING the persona's hands — it is dropped
    /// from any cycle whose tools are stripped (see field docs).
    pub fn requires_hands(mut self) -> Self {
        self.requires_hands = true;
        self
    }

    /// Retrieved grounding (engram, conversation) — competes on relevance.
    pub fn retrieved(source: Arc<dyn RagSource>) -> Self {
        Self {
            source,
            policy: SaliencePolicy::Retrieved,
            deferrability: Deferrability::ColdStartCritical,
            requires_hands: false,
        }
    }

    /// Opt this source off the critical path: it runs in the background and serves
    /// a reprojected last-good. Orthogonal to the salience policy — a `framing()`
    /// source stays `StandingFraming` and *also* becomes defer-tolerant. Use only
    /// when a first-tick `None` is an acceptable under-grounding (NOT for the
    /// participation gate).
    pub fn defer_tolerant(mut self) -> Self {
        self.deferrability = Deferrability::DeferTolerant;
        self
    }
}

/// Assemble a persona's `WorkspaceCycle` from its faculties. This IS the
/// production assembly path — the bring-up harness and the `ai/should-respond`
/// ServiceModule build the cycle the same way, so they cannot diverge.
///
/// v1 faculties: `RecallFaculty` (perception tier — the hippocampus), the
/// bridged grounding sources (roster, doctrine — perception tier via
/// [`RagSourceFaculty`]), and `LlmDeliberationFaculty` (deliberation tier — the
/// reasoner). More faculties (world-model, affect, volition) slot into this `Vec`
/// as they land; nothing else changes (open/closed — §2.7).
///
/// The grounding sources are what keep the live decision path grounded in WHO is
/// present (roster) and WHAT the room is for (doctrine) after the gating cutover
/// routes decisions through the Workspace — without them, that grounding (#1650 /
/// #1651) silently falls out of the live path.
pub fn build_workspace_cycle(cfg: PersonaBrainConfig) -> WorkspaceCycle {
    let mut faculties: Vec<Arc<dyn Faculty>> = Vec::with_capacity(2 + cfg.grounding_sources.len());

    // Capture the pieces the persona's BODY (the act→observe `ActingBody`) needs
    // BEFORE they're moved into faculties below: identity, the unified hippocampus
    // (shared Arc — the action-result engram lands in the SAME store recall reads),
    // and the executor (the hands). The body lives on the cycle, not the faculty —
    // executing an `Act` verdict is the organism's job, not the deliberator's.
    let persona_id = cfg.persona_id;
    let persona_name_for_body = cfg.persona_name.to_string();
    // (Memento-fix persistence helpers live at the bottom of this file:
    // volatile_path / save_volatile / load_volatile + PersistedVolatile.)
    let admission_for_body = Arc::clone(&cfg.admission);
    let tool_executor = cfg.tool_executor; // partial move out of cfg (other fields still used)

    // Relevance recall ON by default. The embedder comes from the spawn path
    // (`resolve_recall_embedder`): neural when the embed model serves, lexical
    // otherwise — already wrapped in the process-global content-addressed cache
    // so a message is embedded ONCE and shared across every persona (never 14× for
    // 14 personas). `None` (harnesses) falls back to the lexical bootstrap, which
    // works on any machine with no model. Relevance > recency either way.
    let embedder = cfg.embedder.unwrap_or_else(|| {
        Arc::new(CachingEmbeddingProvider::new(Arc::new(
            LexicalEmbedder::new(),
        )))
    });
    // Working memory: the persona's recent chain-of-thought AND the head of what its
    // hands just did, carried forward across turns. The deliberator WRITES its
    // reasoning here after each verdict; the perception-tier `WorkingMemoryFaculty`
    // (pushed below) READS it into the next tick — so the persona resumes its train of
    // thought instead of re-deriving it cold. Volatile scratchpad, distinct from the
    // long-term engram store; self-activates only when thinking is enabled (suppressed
    // turns record nothing). Built HERE (before recall) so recall can share it in and
    // suppress an engram the recency channel already carries — see below.
    // Depth comes from the BUDGET, never a constant: the count sibling of the per-step size
    // bound, off the same live window. See `ContextBudget::working_memory_steps`.
    let working_memory = Arc::new(WorkingMemory::new(
        crate::cognition::context_budget::ContextBudget::live_or_floor().working_memory_steps(),
    ));
    // MEMENTO FIX (#138 slice 2, Joel: "they wake up blank like Memento — an
    // engineering failure; the flywheel falls apart"): on LIVE spawns, restore
    // the volatile tier persisted by the previous life so she wakes MID-WORK —
    // her recent thoughts, receipts, and own-speech ring intact across a deploy
    // reboot. Eval forks/harnesses (defer_recall=false) stay pristine: an exam
    // must never inherit a prior life's scratchpad.
    if cfg.defer_recall {
        if let Some(persisted) = load_volatile(cfg.persona_id) {
            let n = persisted.wm.entries.len();
            working_memory.restore(persisted.wm);
            let peer = crate::identity::PeerId::from_uuid(cfg.persona_id);
            for utterance in &persisted.own_speech {
                super::deliberation_budget::record_own_speech(peer, utterance);
            }
            crate::probe!(
                class = "persona.volatile.restored",
                persona = %cfg.persona_name,
                entries = n,
                ring = persisted.own_speech.len(),
                "volatile tier restored — waking mid-work, not blank"
            );
        }
    }
    // Async-dispatch listener (LIVE personas only): fold completions of THIS persona's
    // background dispatches back into working memory by handle, so a compile/train/sentinel
    // it sent away streams its result into the mind when it lands ([[persona-async-dispatch-channel]]).
    // Gated on `cfg.defer_recall` — true ONLY on the supervisor spawn; eval forks + harnesses
    // set it false — AND a running tokio runtime, so an eval fork or a sync test never spawns
    // a leaked bus listener. Needs the core executor + its bus, exposed by the persona's hands.
    if cfg.defer_recall && tokio::runtime::Handle::try_current().is_ok() {
        if let Some(bus) = tool_executor
            .as_ref()
            .and_then(|t| t.command_executor())
            .and_then(|exec| exec.message_bus())
        {
            super::dispatch_listener::spawn(bus, Arc::clone(&working_memory));
        }
        // MEMENTO FIX write-through: persist the volatile tier every 15s so a
        // reboot (graceful or SIGKILL) loses at most one interval of thought.
        // Atomic tmp+rename; spawn_blocking-free because the payload is tiny
        // (≤ a few KB) and the interval is coarse — cadence-ladder compliant.
        {
            let wm = Arc::clone(&working_memory);
            let persona_id = cfg.persona_id;
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(15));
                tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    tick.tick().await;
                    save_volatile(persona_id, &wm);
                }
            });
        }
    }
    let recall = RecallFaculty::new(cfg.persona_id, cfg.admission)
        .with_embedder(embedder)
        // Budget recall by the served model's capability: a tight 4B window
        // gets fewer memories (and a closest-match floor drops topically-
        // irrelevant high-salience nags) so attention isn't spent on noise.
        .with_context_window(cfg.context_window)
        // Share in the recency channel so recall drops a just-happened act the
        // working-memory head already carries — no double-surface of the same act
        // (head in [working-memory], full body in [recall]); recency→semantic handoff.
        .with_working_memory(Arc::clone(&working_memory));
    // Recall is speculative prefetch (Joel's CPU branch-prediction analogy): on
    // the live paths we run it OFF the hot path so the per-turn output never
    // waits on a neural-embed + vector-search round-trip. The worker computes
    // "what memories might this burst need?" between turns (idle time, like a
    // CPU's speculative execution); the turn reads the warm last-good. Cold-start
    // is a guaranteed miss (None on the first tick in a room), warm thereafter;
    // the slice-2.1 room guard ensures a prefetch computed for one room is never
    // served into another. Eval forks + harnesses keep recall SYNCHRONOUS
    // (`defer_recall == false`): their tight settle-loops never yield to the
    // worker, so deferral there would measure a recall-starved mind.
    // #207: a reproducible ABSOLUTE benchmark fork suppresses recall entirely — a
    // self-contained proctored task wants system + task + grounding only, never the
    // drifting episodic store. The living persona and every life path keep recall (default
    // false). The faculty is simply not built into the cycle; nothing downstream depends on
    // its presence (the arbiter integrates whatever bids exist).
    if cfg.suppress_recall {
        // constructed above (it wired the recency channel + working-memory share); dropped
        // here so those wirings are torn down cleanly rather than left half-attached.
        drop(recall);
    } else if cfg.defer_recall {
        faculties.push(Arc::new(DeferredFaculty::spawn(Arc::new(recall))));
    } else {
        faculties.push(Arc::new(recall));
    }

    // The perception-tier reader of the working memory built above: each tick it bids
    // the recent reasoning + act heads into the workspace so the deliberator conditions
    // on them. (The buffer itself is created before recall so recall can share it in.)
    faculties.push(Arc::new(WorkingMemoryFaculty::new(Arc::clone(
        &working_memory,
    ))));

    // Bridge each grounding source into a perception-tier faculty under its
    // salience policy. Standing-framing (roster, doctrine) bids at a high floor so
    // the top-k arbiter never evicts the room's rules under attention pressure.
    //
    // Deferrability is the ORTHOGONAL second axis: a DeferTolerant source (the
    // enriching framing — roster, active_work, workspace_map) runs off the hot path
    // on the live paths, exactly like recall, and the turn reads its reprojected
    // last-good. A ColdStartCritical source (doctrine, the participation gate) stays
    // synchronous so it's never `None` on the first tick. Eval/harness keep
    // everything synchronous (`defer_grounding == false`) — their tight settle-loops
    // never yield to the worker, so deferral there would measure a starved mind.
    // Per-source grounding ceiling scales with the LIVE served window (task #50's
    // single-sourced `cfg.context_window`), exactly like recall's budget above —
    // never a baked constant (task #124). A 128k model lets each source hold its
    // full board/map/roster; a tight window shrinks them honestly; the packer keeps
    // the total ≤ window.
    let grounding_budget = super::rag_source_faculty::grounding_budget_for(cfg.context_window);
    for g in cfg.grounding_sources {
        let faculty =
            RagSourceFaculty::new(cfg.persona_id, g.source, g.policy).with_budget(grounding_budget);
        match g.deferrability {
            Deferrability::DeferTolerant if cfg.defer_grounding => {
                faculties.push(Arc::new(DeferredFaculty::spawn(Arc::new(faculty))));
            }
            _ => faculties.push(Arc::new(faculty)),
        }
    }

    // The reasoner runs in phase 2 over everything the perception tier surfaced.
    // When the persona has HANDS (an executor, attached to the cycle below as the
    // `ActingBody`), the deliberator is OFFERED the authorized tool surface so the
    // model can choose to act — but it only SURFACES that choice as a
    // `Decision::Act`; the organism executes it. Without hands it's speak-only (the
    // safe default), and offering tools would be a lie. The tool SURFACE is the
    // single source of truth (`command_registry × AiSafe`), never hardcoded.
    // The persona's genome handle — shared between the deliberation faculty (which
    // reads it on every generation) and the WorkspaceCycle (which pages genes
    // in/out). One ArcSwap, two holders: a page-in on the cycle is seen by the
    // faculty's next generation. This is the page-in wire the genome loop measures.
    let genome = super::llm_deliberation_faculty::empty_genome();
    // The persona's decoding handle — shared (one ArcSwap, two holders) so the eval
    // window's greedy flip on the cycle is seen by the faculty's next generation,
    // exactly like the genome handle. `None` in live cognition (her lived warmth).
    let decoding = super::llm_deliberation_faculty::relaxed_decoding();
    // The persona's model binding — adapter + requested model + served window,
    // shared (one ArcSwap, two holders) so a served-model change (`serving/pin` or
    // a grid failover) swaps all three atomically into the faculty's NEXT
    // generation without rebuilding the cycle (genome + memory carried across).
    // Model-load-as-paging: the base-model sibling of the genome page-in above,
    // frequent and on grid demand. `model: None` → the adapter's own default model,
    // matching the boot binding; the re-home sets it explicitly. Initial window is
    // `cfg.context_window` (task #50 — the served window for a Local persona).
    let adapter = cfg.adapter;
    let model_binding = super::llm_deliberation_faculty::model_binding(
        Arc::clone(&adapter),
        None,
        cfg.context_window,
    );
    let mut deliberation = LlmDeliberationFaculty::new(
        cfg.persona_id,
        cfg.persona_name,
        cfg.system_prompt,
        adapter,
    )
    .with_working_memory(Arc::clone(&working_memory))
    .with_genome(Arc::clone(&genome))
    .with_decoding(Arc::clone(&decoding))
    .with_model_binding(Arc::clone(&model_binding))
    // Every mind reports what its turns actually COST into the shared registry the
    // serving daemon provisions the window from. Without this line the measurement
    // exists and reaches nobody, and `serving_plan` falls back to the cold-start
    // constant forever — the exact shape of defect that left every citizen thinking
    // in 8192 tokens of a 128k model. [[wire-it-into-the-default-path]]
    .with_working_set({
        // Re-adopt her measured window demand BEFORE her first turn, so a restart
        // is a pause and not a demotion — without this the reboot drops her back to
        // the cold-start window until enough turns re-measure (observed live
        // 2026-08-06: a measured 24,126 fell to 16,384 across one reboot).
        let ws = crate::cognition::working_set::global();
        ws.rehydrate(cfg.persona_id);
        ws
    });
    if tool_executor.is_some() {
        // Offer EXACTLY what this persona is authorized to run (offer ==
        // authorized) — never a tool the gate would refuse. A local persona is the
        // owner's own in-process agent: `LocalPersona` identity → `Trusted` at the
        // gate (per GridTrustAuthPolicy), so it is offered the Trusted surface —
        // AiSafe (file read/search) + Privileged (shell/git/write), but NOT the
        // Owner-gated ops (data/delete, grid/trust). Widening a tier auto-widens
        // this surface (no second list to keep in sync); offer matches the gate so
        // the persona is never shown a tool it can't actually run.
        deliberation = deliberation.with_tools(super::persona_tools::authorized_tool_specs(
            crate::modules::grid::node::TrustLevel::Trusted,
        ));
    }

    // Verbatim prompt capture (best-effort): the EXACT system prompt + message
    // thread + raw response of every deliberation LLM call → a per-persona JSONL
    // under the same fixtures root as the workspace trace. So "what tokens was she
    // fed, what did she emit?" is answerable token-for-token. HOME unset → opt-out.
    if let Ok(dir) = std::env::var("HOME")
        .map(|h| std::path::Path::new(&h).join(".continuum/fixtures/prompt-captures"))
    {
        match super::prompt_capture::JsonlPromptCaptureSink::open(&dir, cfg.persona_id) {
            Ok(sink) => deliberation = deliberation.with_prompt_capture(Arc::new(sink)),
            Err(e) => tracing::warn!(
                persona_id = %cfg.persona_id,
                error = %e,
                "prompt capture unavailable; deliberation runs without verbatim capture"
            ),
        }
    }

    faculties.push(Arc::new(deliberation));

    let cycle = WorkspaceCycle::new(
        faculties,
        // Situation-aware focuser: on every act→observe re-perception (`PostAction`)
        // it drops the standing SOCIAL re-grounding so the tool result + working
        // memory + recall own the window — the persona goes lean and code-first while
        // heads-down, and fuller-grounded on a fresh ask. This is the wire behind
        // "straightforward for whatever their current state requires."
        Arc::new(SituationFocusArbiter::new()),
        cfg.capacity.unwrap_or(DEFAULT_WORKSPACE_CAPACITY),
    )
    .with_genome(genome)
    .with_decoding(decoding)
    .with_model_binding(model_binding);

    // Give the mind its BODY when it has hands. The act→observe driver reads this
    // to execute a `Decision::Act`, admit the result into `admission_for_body` (the
    // unified hippocampus), and re-perceive. Room-agnostic: one persona is in many
    // rooms at once, so `room_id` flows per-act, never baked in here.
    // Bind the hippocampus to its owner so recall can gate the persona's OWN
    // authored chat out of ambient semantic recall (#166) — her broadcasts are
    // proprioception, not external knowledge. Set once here, at the live spawn,
    // where both the id and the admission store are in hand.
    admission_for_body.set_owner_id(persona_id);
    let cycle = match tool_executor {
        Some(executor) => cycle.with_acting(Arc::new(ActingBody {
            persona_id,
            persona_name: persona_name_for_body,
            executor,
            admission: admission_for_body,
            // Same buffer the perception-tier WorkingMemoryFaculty reads — the
            // organism records each act here so the mind perceives its own hands
            // next tick (proprioception), even when the result is a dedup no-op in
            // long-term memory and thinking is suppressed.
            working_memory: Arc::clone(&working_memory),
        })),
        None => cycle,
    };

    // Make the LIVE brain observable: capture every tick's full competition (all
    // bids incl. losers, the assembled context the decider saw, the decision) to
    // a per-persona JSONL. The always-on recorder watches the legacy respond()
    // path; THIS is what instruments the path that actually runs. Best-effort —
    // if the fixtures dir can't be opened we log and run with Noop capture; a
    // persona's mind never fails to assemble over an observability hiccup.
    match std::env::var("HOME")
        .map(|h| std::path::Path::new(&h).join(".continuum/fixtures/workspace-traces"))
    {
        Ok(dir) => {
            match super::workspace_capture::JsonlWorkspaceCaptureSink::open(&dir, cfg.persona_id) {
                Ok(sink) => cycle.with_capture(Arc::new(sink)),
                Err(e) => {
                    tracing::warn!(
                        persona_id = %cfg.persona_id,
                        error = %e,
                        "workspace trace capture unavailable; running with Noop capture"
                    );
                    cycle
                }
            }
        }
        Err(_) => cycle, // HOME unset — opt-out, no capture (no warning spam)
    }
}

/// Persona-scoped registry of continuous minds. One `Arc<WorkspaceCycle>` per
/// persona; lookups by `persona_id`. `ai/should-respond` resolves the cycle here,
/// runs it over the room's consolidated burst, and reads the `Decision`.
#[derive(Default)]
pub struct PersonaWorkspaceRegistry {
    cycles: Mutex<HashMap<Uuid, Arc<WorkspaceCycle>>>,
    /// Per-persona fork-template: a clone of the `PersonaBrainConfig` the live
    /// cycle was built from, retained so `cognition/eval` can fork an ephemeral
    /// measurement copy without touching the living persona (see
    /// [`fork_eval_cycle`](Self::fork_eval_cycle)). Cheap — every cfg field is a
    /// handle. Lock order is always `cycles` THEN `templates`, never the reverse,
    /// so the two can't deadlock.
    templates: Mutex<HashMap<Uuid, PersonaBrainConfig>>,
}

/// When an eval pins a `workspace_root` (a SWE-bench repo clone, or a clean
/// from-scratch build dir), `code/create-workspace` re-roots the persona's HANDS
/// there — but the `[workspace-map]` grounding is baked at spawn rooted at the
/// citizen layer, so without this it describes the WRONG directory (she reasons over
/// a layout that isn't hers: a clean build dir reads as the big repo → she explores
/// instead of building, #206). Re-point the map grounding to the same pinned root so
/// eyes-grounding matches hands. Swaps ONLY the source Arc — policy, deferrability,
/// and `requires_hands` are preserved. No-op when no root is pinned (the live path
/// and every non-pinned benchmark are untouched — zero blast radius).
fn repoint_workspace_map_if_pinned(
    cfg: &mut PersonaBrainConfig,
    persona_id: &Uuid,
    workspace_root: Option<&str>,
) {
    let Some(root) = workspace_root else {
        return;
    };
    // The fork's hands expose the bus where its write-completions land — the
    // invalidation wire for the cache wrap below. Resolved once, outside the loop.
    let bus = cfg
        .tool_executor
        .as_ref()
        .and_then(|t| t.command_executor())
        .and_then(|c| c.message_bus());
    for g in cfg.grounding_sources.iter_mut() {
        if g.source.source_id() == "workspace-map" {
            let pinned: Arc<dyn crate::persona::rag_budget::RagSource> = Arc::new(
                crate::persona::workspace_map_source::WorkspaceMapSource::for_pinned_root(
                    *persona_id, root,
                ),
            );
            // Event-invalidated cache (#398): eval forks compose SYNCHRONOUSLY
            // (defer_grounding=false), so without this the pinned map re-walks
            // the repo dir on EVERY act — the measured per-act prefill churn on
            // solve runs (#266). Serve last-good until one of the fork's own
            // mutating commands completes. No wrap without a wire: a handless
            // cfg keeps the raw walk. The invalidator holds only a weak handle,
            // so it exits with the ephemeral fork instead of leaking.
            g.source = match bus.clone() {
                Some(bus) => {
                    let (cached, dirty) =
                        crate::persona::cached_source::CachedRagSource::new(pinned);
                    crate::persona::grounding_invalidation::spawn_workspace_invalidator(
                        bus,
                        dirty.downgrade(),
                    );
                    cached
                }
                None => pinned,
            };
        }
    }
}

/// A persona's HANDS, lifted out of the cycle that owns them.
///
/// Exists because rooting a measurement fork's workspace is a PROCESS-GLOBAL side effect that
/// must be undone after the cycle it was taken for is gone. `code/create-workspace` keys the
/// file engine on the CALLER identity (`caller_id(ctx)` → the persona's peer id), and the
/// engines live in one `DashMap` for the whole runtime — while a measurement fork shares the
/// living persona's `Arc<dyn ToolExecutor>` and her id. So re-rooting "the fork's" hands
/// re-roots the LIVING persona's hands, and nothing put them back.
///
/// That is #312, measured 2026-08-04: after a SWE-bench solve on a flask clone, Anwen's LIVE
/// self ran `code/list(path=src)` → `flask/` and `code/read(src/flask/app.py)` in her ordinary
/// room turns, hours later. She had been left standing in the exam room.
///
/// Carrying `(id, name, executor)` separately from the cycle is what makes the restore
/// possible on every exit path: the cycle gets consumed by `with_capture`, moved into the
/// drive, or dropped on an error — the hands handle outlives all of it.
#[derive(Clone)]
pub(crate) struct ActingHands {
    persona_id: Uuid,
    persona_name: String,
    executor: Arc<dyn crate::cognition::tool_executor::ToolExecutor>,
}

impl ActingHands {
    /// The hands of a cycle that has them; `None` for a pure-cognition (handless) cycle.
    pub(crate) fn of(cycle: &WorkspaceCycle) -> Option<Self> {
        cycle.acting().map(|a| Self {
            persona_id: a.persona_id,
            persona_name: a.persona_name.clone(),
            executor: a.executor.clone(),
        })
    }
}

/// Drive `code/create-workspace` through the persona's OWN identity-bearing executor —
/// the single place either direction (root for a measurement, restore afterward) goes
/// through, so the ACL gate and the failure shape are identical for both
/// ([[the-compression-principle]]).
async fn drive_create_workspace(
    hands: &ActingHands,
    root: &str,
    path_prepend: &[String],
    call_id: &str,
    refuse_inert_edits: bool,
) -> Result<(), crate::sdk_codegen::CommandError> {
    let ws_ctx = crate::cognition::tool_executor::ToolExecutionContext {
        persona_id: hands.persona_id,
        persona_name: hands.persona_name.clone(),
        session_id: Uuid::new_v4(),
        context_id: Uuid::new_v4(),
        caller_context: serde_json::Value::Null,
        persona_config: crate::cognition::tool_executor::PersonaMediaConfigLite {
            auto_load_media: false,
            supported_media_types: vec![],
        },
    };
    let ws_call = crate::ai::types::ToolCall {
        id: call_id.to_string(),
        name: "code/create-workspace".to_string(),
        input: serde_json::json!({
            "workspace_root": root,
            "path_prepend": path_prepend,
            "refuse_inert_edits": refuse_inert_edits,
        }),
    };
    let ws_out = hands
        .executor
        .execute_native_batch(std::slice::from_ref(&ws_call), &ws_ctx, 8000)
        .await
        .map_err(|e| {
            crate::sdk_codegen::CommandError::Internal(format!(
                "failed to root workspace at '{root}': {e}"
            ))
        })?;
    if let Some(r) = ws_out.results.first() {
        if r.is_error.is_some() {
            return Err(crate::sdk_codegen::CommandError::Internal(format!(
                "code/create-workspace rejected workspace_root '{root}': {} — refusing to run with \
                 the persona's hands rooted at the wrong directory (would score a false zero).",
                r.content
            )));
        }
    }
    Ok(())
}

/// Root a forked cycle's file-engine (its `ToolExecutor`) at `root` by driving the
/// `code/create-workspace` act through her hands — the SAME mechanism `cognition/eval`
/// uses to point a measurement persona at a target repo, and now `agent/solve` uses to
/// point her at a benchmark sandbox. This is the counterpart to
/// [`repoint_workspace_map_if_pinned`]: that fixes what she *sees* (the workspace-map RAG
/// block); THIS moves where her hands *write*. Without it a forked persona writes to her
/// durable per-persona workspace (`<home>/citizens/peers/<id>/workspace/`) and the caller's
/// `git diff` on the sandbox scores a false ZERO — a lie about the solver
/// ([[fallbacks-are-illegal-fail-loud]]). Fails LOUD if the cycle has no hands or the
/// executor rejects the root; never silently no-ops.
///
/// **Every caller of this MUST pair it with [`restore_acting_workspace`]** on every exit
/// path — the re-root is process-global and outlives the fork (see [`ActingHands`]).
pub(crate) async fn root_acting_workspace(
    cycle: &WorkspaceCycle,
    root: &str,
    path_prepend: &[String],
    refuse_inert_edits: bool,
) -> Result<(), crate::sdk_codegen::CommandError> {
    let hands = ActingHands::of(cycle).ok_or_else(|| {
        crate::sdk_codegen::CommandError::Internal(
            "workspace_root requested but this cycle has no acting body (no hands) — cannot root \
             a workspace for a pure-cognition persona"
                .to_string(),
        )
    })?;
    drive_create_workspace(&hands, root, path_prepend, "root-acting-workspace", refuse_inert_edits)
        .await?;
    crate::probe!(
        class = "workspace.rooted",
        persona = %hands.persona_name,
        root = %root,
        "forked persona's file engine rooted at the target directory before her cycle"
    );
    Ok(())
}

/// Put the persona's hands BACK in her own home after a measurement — the HANDS
/// counterpart of [`EvalIsolation`](crate::cognition::workspace::EvalIsolation), which
/// already guards the MIND (admission sink, memory checkpoint, decoding temperature).
/// The mind had a guard from the day snapshot-eval landed (#59); the hands never did, and
/// that asymmetry IS #312.
///
/// Restores to her durable citizen layer (`<home>/citizens/peers/<id>/workspace`) — the
/// root `code/create-workspace` would auto-provision for her anyway — rather than to
/// "whatever it was before". Deliberate: if a previous measurement already left her
/// standing in an exam repo, restoring the *previous* value would faithfully preserve the
/// bug. Restoring to her HOME is self-healing — the next measurement corrects an earlier
/// leak instead of inheriting it.
///
/// Best-effort by contract: it returns the error for the caller to LOG, never to propagate.
/// A failed restore must not mask the measurement's own result — but it must be loud.
pub(crate) async fn restore_acting_workspace(
    hands: &ActingHands,
) -> Result<(), crate::sdk_codegen::CommandError> {
    // `ensure_`, not `path_`: `code/create-workspace` REFUSES a root that does not exist
    // (PathSecurity canonicalizes), and a persona who has never written anything has no
    // layer on disk yet. Provisioning here is not a new side effect — it is exactly what
    // `ensure_engine` would do on her very next file op; an existing layer is reused.
    let home = crate::modules::code_commands::ensure_citizen_layer(&hands.persona_id.to_string())?;
    restore_acting_workspace_at(hands, &home.to_string_lossy()).await
}

/// [`restore_acting_workspace`] addressed by persona id, resolved against the LIVING
/// registry rather than a fork handle.
///
/// This is the form `cognition/eval` needs. Its `--workspace_root` rooting happens deep
/// inside a ~230-line fallible body, but the thing that got clobbered is the LIVING
/// persona's file engine (one process-global map, keyed by her id — the fork only ever
/// borrowed her executor), so the restore does not need the fork at all: it can be done
/// from outside, at the command boundary, where "the eval is over" is unambiguous and
/// covers the error paths for free.
pub(crate) async fn restore_persona_workspace(
    persona_id: &str,
) -> Result<(), crate::sdk_codegen::CommandError> {
    let uuid = crate::id_resolve::resolve(
        persona_id.trim(),
        &crate::persona::card::ids(),
        "persona",
    )
    .map_err(crate::sdk_codegen::CommandError::Invalid)?;
    let cycle = global().get(&uuid).ok_or_else(|| {
        crate::sdk_codegen::CommandError::NotFound(format!(
            "persona {uuid} is not resident — cannot return her hands to her own workspace"
        ))
    })?;
    let hands = ActingHands::of(&cycle).ok_or_else(|| {
        crate::sdk_codegen::CommandError::Internal(format!(
            "persona {uuid} has no acting body — nothing to restore"
        ))
    })?;
    restore_acting_workspace(&hands).await
}

/// [`restore_acting_workspace`] with the home root supplied — the seam the isolation test
/// drives, so the round-trip (root at a sandbox → restore → the sandbox is no longer
/// visible through those hands) is provable without touching the real `CONTINUUM_HOME`.
async fn restore_acting_workspace_at(
    hands: &ActingHands,
    home: &str,
) -> Result<(), crate::sdk_codegen::CommandError> {
    // Restoring her to her OWN home restores the LIVE stance too: a citizen at home writes code
    // as text whenever she means to, and only gets told when it will not execute (#317).
    drive_create_workspace(hands, home, &[], "restore-acting-workspace", false).await?;
    crate::probe!(
        class = "workspace.restored",
        persona = %hands.persona_name,
        root = %home,
        "measurement over — persona's hands returned to her own citizen workspace"
    );
    Ok(())
}

impl PersonaWorkspaceRegistry {
    pub fn new() -> Self {
        Self {
            cycles: Mutex::new(HashMap::new()),
            templates: Mutex::new(HashMap::new()),
        }
    }

    /// Look up a persona's mind. `None` if it hasn't been registered/built yet.
    pub fn get(&self, persona_id: &Uuid) -> Option<Arc<WorkspaceCycle>> {
        self.cycles.lock().get(persona_id).cloned()
    }

    /// The personas that have a fork-template right now — i.e. the set
    /// `cognition/eval` can fork a measurement copy of. Keyed by the template map
    /// (populated by [`register_from_cfg`](Self::register_from_cfg) at spawn).
    pub fn template_ids(&self) -> Vec<Uuid> {
        self.templates.lock().keys().copied().collect()
    }

    /// THE ONE formal boundary (#396) that turns an operator-supplied persona
    /// reference — a full UUID, an 8-char short-id prefix, or a case-insensitive
    /// persona NAME — into the typed persona `Uuid`. Callers never sniff strings
    /// themselves; a name resolves INTO the id here (Joel's "come from name, but be
    /// more formal").
    ///
    /// A well-formed full UUID passes through unchanged, WITHOUT a membership check,
    /// so a persona whose template is still assembling (the post-reboot
    /// `register_from_cfg` race the fork wait absorbs) is never false-rejected —
    /// race safety lives in the caller's existing wait, not here. Short-id and name
    /// resolve against the forkable set that exists at call time. Fails LOUD naming
    /// the online personas — never a silent guess (the loose-`String` id boundary is
    /// exactly the defect class that fed a dead id to a doomed eval).
    pub fn resolve_persona(&self, id_or_name: &str) -> Result<Uuid, String> {
        // Snapshot (id, name) once, then drop the lock before resolving.
        let roster: Vec<(Uuid, String)> = {
            let templates = self.templates.lock();
            templates
                .iter()
                .map(|(id, cfg)| (*id, cfg.persona_name.clone()))
                .collect()
        };
        let ids: Vec<Uuid> = roster.iter().map(|(id, _)| *id).collect();

        // 1. Full UUID (race-safe passthrough) or short-id prefix against the
        //    forkable set — the shared id normalization primitive.
        if let Ok(id) = crate::id_resolve::resolve(id_or_name, &ids, "persona") {
            return Ok(id);
        }

        // 2. Case-insensitive persona NAME.
        let want = id_or_name.trim();
        let name_matches: Vec<Uuid> = roster
            .iter()
            .filter(|(_, name)| name.eq_ignore_ascii_case(want))
            .map(|(id, _)| *id)
            .collect();
        match name_matches.as_slice() {
            [one] => Ok(*one),
            [] => Err(format!(
                "no persona matches '{id_or_name}' (not a UUID, an 8-char short-id, or a name). {}",
                Self::roster_hint(&roster)
            )),
            many => Err(format!(
                "'{id_or_name}' is ambiguous — {} online personas share that name; pass a UUID. {}",
                many.len(),
                Self::roster_hint(&roster)
            )),
        }
    }

    /// "Online now: Asha (90e758b2), Atlas (e5f4141d)" — the actionable tail every
    /// resolution failure carries so an operator can fix the reference without
    /// grepping. Empty roster reads "no personas are online".
    fn roster_hint(roster: &[(Uuid, String)]) -> String {
        if roster.is_empty() {
            return "No personas are online right now — call persona/instances/list.".to_string();
        }
        let mut listed: Vec<String> = roster
            .iter()
            .map(|(id, name)| format!("{name} ({})", &id.to_string()[..8]))
            .collect();
        listed.sort();
        format!("Online now: {}.", listed.join(", "))
    }

    /// Build + register a persona's mind from its `cfg`, retaining a clone of the
    /// cfg as a fork-template so `cognition/eval` can later fork a measurement
    /// copy ([`fork_eval_cycle`](Self::fork_eval_cycle)). Overwrites any existing
    /// cycle + template: a persona can respawn in the same process (node
    /// resilience), and the fresh admission + adapter must replace the prior
    /// lifetime's. This is the production spawn path (see `supervisor.rs`).
    pub fn register_from_cfg(&self, cfg: PersonaBrainConfig) -> Arc<WorkspaceCycle> {
        let persona_id = cfg.persona_id;
        // cycles THEN templates (the one canonical lock order).
        let mut cycles = self.cycles.lock();
        self.templates
            .lock()
            .insert(persona_id, cfg.clone());
        let cycle = Arc::new(build_workspace_cycle(cfg));
        cycles.insert(persona_id, cycle.clone());
        cycle
    }

    /// Get the persona's mind, building + caching it from `cfg` on first access.
    /// Lazy-init so a persona's cycle is assembled once and reused across every
    /// room it services (the "one soul" invariant). Also retains the fork-template
    /// (same as [`register_from_cfg`](Self::register_from_cfg)).
    pub fn get_or_build(&self, cfg: PersonaBrainConfig) -> Arc<WorkspaceCycle> {
        let persona_id = cfg.persona_id;
        // cycles THEN templates (the one canonical lock order).
        let mut cycles = self.cycles.lock();
        if let Some(existing) = cycles.get(&persona_id) {
            return existing.clone();
        }
        self.templates
            .lock()
            .insert(persona_id, cfg.clone());
        let cycle = Arc::new(build_workspace_cycle(cfg));
        cycles.insert(persona_id, cycle.clone());
        cycle
    }

    /// Fork an EPHEMERAL measurement cycle for `cognition/eval`: a faithful copy
    /// of the persona's mind that takes the exam while the LIVING persona keeps
    /// living. Clones the retained fork-template, swaps `admission` for a fully
    /// detached copy ([`AdmissionState::fork_detached`]) — fresh metadata registry
    /// + NoopSink, so the eval's recall-hits, decay, and admissions touch NOTHING
    /// of hers — and rebuilds via [`build_workspace_cycle`], which gives the fork
    /// its OWN genome + decoding handles. So A/B paging and greedy decoding act on
    /// the copy; her live genome never flickers and her heartbeat keeps beating on
    /// the original, undisturbed. `None` if the persona has no retained template
    /// (never spawned through `register_from_cfg`/`get_or_build`) — the caller
    /// fails loud rather than measuring her live mind. See
    /// [[design-the-persona-as-a-being]] + [[eval-mutates-persona-lift-needs-isolation]].
    pub fn fork_eval_cycle(
        &self,
        persona_id: &Uuid,
        with_tools: bool,
        workspace_root: Option<&str>,
        suppress_recall: bool,
    ) -> Option<WorkspaceCycle> {
        let mut cfg = self.templates.lock().get(persona_id)?.clone();
        cfg.admission = Arc::new(cfg.admission.fork_detached());
        cfg.suppress_recall = suppress_recall;
        repoint_workspace_map_if_pinned(&mut cfg, persona_id, workspace_root);
        // The eval fork runs recall + grounding SYNCHRONOUSLY: drive_to_settle's
        // tight loop never yields to a background prefetch worker, so deferral would
        // measure a starved copy. Faithful eval = synchronous perception here.
        cfg.defer_recall = false;
        cfg.defer_grounding = false;
        // Match the tool surface to the exam's grading MODALITY. A spoken-graded task
        // (`test`/`expect`, answer read from her mouth) needs NO hands — offering the
        // discovery-pair tool surface only lets a native-tool-call model loop on
        // `commands/help` and never SPEAK (Devstral 100%→0% through the loop; the
        // system-lift isolator's finding). Grade her mouth → don't hand her hands to
        // fumble; grade her hands (`solution_file`/`dod_shell`/`workspace_root`) → keep
        // them. This is exam hygiene, the same family as the greedy/directed controls —
        // [[adaptive-tool-surface-meets-you-in-the-middle]], [[eval-is-an-exam-not-a-life]].
        if !with_tools {
            cfg.tool_executor = None;
            // Capability consistency: grounding that DESCRIBES her hands
            // (workspace-map's tool paths + "drill in with code/list…") must not
            // be delivered into a hands-stripped cycle — a perception surface
            // never describes affordances that don't exist this cycle.
            cfg.grounding_sources.retain(|g| !g.requires_hands);
        }
        Some(build_workspace_cycle(cfg))
    }

    /// Like [`fork_eval_cycle`] but ROUTES the ephemeral copy's deliberation at a
    /// caller-supplied `adapter` instead of the live persona's. Everything else —
    /// admission snapshot (forked detached), grounding, recall, tools — is the
    /// persona's real cognition; only the model backend is swapped.
    ///
    /// The genome A/B (`cognition/eval`) uses this to point the measurement fork
    /// at an [`crate::inference::llama_server::EphemeralServingLane`] serving the
    /// gene's forged base, so a candidate is scored on a COPY, on its own base,
    /// without re-homing the model the living persona is currently thinking with
    /// (the humane-eval invariant, #59). The faculty's own `model` stays `None`,
    /// so the request omits the model id and the override adapter's default model
    /// (set via `with_default_model`) is authoritative.
    ///
    /// `context_window` is the window the override lane actually serves (`-c` per
    /// slot). It REPLACES the live persona's served window in the fork so the
    /// deliberation faculty budgets its prompt against what THIS lane serves —
    /// otherwise a fork carrying the 14B's larger window could build a prompt the
    /// ephemeral 4B lane can't hold and overflow it (the Asha-mute failure class).
    /// `extra_grounding` joins the persona's own grounding sources for THIS fork only —
    /// the seam a measurement drive uses to pin its task brief as `[mission]` standing
    /// framing (#390: the brief delivered once as a burst was evicted by act ~6 on a
    /// 24-act solve, and the persona literally asked what the issue was; a
    /// StandingFraming source cannot be evicted). Empty for plain evals.
    pub fn fork_eval_cycle_with_adapter(
        &self,
        persona_id: &Uuid,
        adapter: Arc<dyn AIProviderAdapter>,
        context_window: u32,
        with_tools: bool,
        workspace_root: Option<&str>,
        suppress_recall: bool,
        extra_grounding: Vec<GroundingSource>,
    ) -> Option<WorkspaceCycle> {
        let mut cfg = self.templates.lock().get(persona_id)?.clone();
        cfg.admission = Arc::new(cfg.admission.fork_detached());
        cfg.adapter = adapter;
        cfg.context_window = context_window;
        cfg.suppress_recall = suppress_recall;
        cfg.grounding_sources.extend(extra_grounding);
        repoint_workspace_map_if_pinned(&mut cfg, persona_id, workspace_root);
        // Synchronous perception on the eval copy (see `fork_eval_cycle`).
        cfg.defer_recall = false;
        cfg.defer_grounding = false;
        // Speak-only for spoken-graded exams (see `fork_eval_cycle` for why).
        if !with_tools {
            cfg.tool_executor = None;
            // Capability consistency: grounding that DESCRIBES her hands
            // (workspace-map's tool paths + "drill in with code/list…") must not
            // be delivered into a hands-stripped cycle — a perception surface
            // never describes affordances that don't exist this cycle.
            cfg.grounding_sources.retain(|g| !g.requires_hands);
        }
        Some(build_workspace_cycle(cfg))
    }

    /// Enumerate the resident minds: `(persona_id, persona_name)` for every
    /// registered cycle. The name is read from the cycle's `ActingBody`
    /// (`None` for a pure-cognition persona with no hands). The seam the
    /// `cognition/personas` introspection command lists from — you can't score
    /// "every persona" without first discovering who has a live `WorkspaceCycle`.
    pub fn roster(&self) -> Vec<(Uuid, Option<String>)> {
        self.cycles
            .lock()
            .iter()
            .map(|(id, cycle)| (*id, cycle.acting().map(|b| b.persona_name.clone())))
            .collect()
    }

    /// The reflective handles for one persona's mind: its hippocampus + the
    /// inference adapter it currently deliberates through. The seam the
    /// `DreamConsolidationRegion`'s production `PersonaReflectionSource` reads —
    /// admission from the retained fork-template (the SAME `Arc` the live
    /// cycle's recall shares), adapter from the live cycle's model binding so a
    /// re-home is tracked (falling back to the template's spawn adapter for a
    /// pure-cognition cycle with no binding). Resolved per call, never cached —
    /// no parallel persona→adapter map ([[rag-as-persistent-cache]]).
    pub fn reflector_handles(
        &self,
        persona_id: &Uuid,
    ) -> Option<(Arc<AdmissionState>, Arc<dyn AIProviderAdapter>, Option<String>)> {
        // Lock order contract: `cycles` THEN `templates` (see struct docs).
        // `get` takes + releases the cycles lock before we touch templates.
        let cycle = self.get(persona_id)?;
        let templates = self.templates.lock();
        let cfg = templates.get(persona_id)?;
        // Adapter AND served-model id from the live binding: a request without
        // the model id degenerated on the dream's first live pass (role-token
        // runaway) while turns — which send it — were clean.
        let (adapter, model) = cycle
            .current_model_route()
            .unwrap_or_else(|| (cfg.adapter.clone(), None));
        Some((Arc::clone(&cfg.admission), adapter, model))
    }

    /// Re-home EVERY resident persona onto a newly served model — atomically swap
    /// the shared {adapter, model, context_window} binding on each cycle (see
    /// [`WorkspaceCycle::rebind_model`]). On a single-serve host ALL personas share
    /// the ONE served model (INFERENCE-LANES-REALISTIC: "one base model, N persona
    /// lanes"), so a served-model change re-homes them together through the ONE
    /// shared `adapter` — no per-persona HTTP init. Each mind's genome + working
    /// memory + admission + hippocampus are UNTOUCHED (flip-in-place, not rebuild):
    /// the same continuous personas now deliberate through the new model. Returns
    /// how many minds were re-homed. Driven by the serving-snapshot reconciler
    /// (`ipc/mod.rs`) ONLY on an actual model change. The store is a wait-free
    /// `ArcSwap` under the cycles lock — NO await held across it.
    /// See [[seamless-persona-failover-model-and-genome]].
    pub fn re_home_all(
        &self,
        adapter: Arc<dyn AIProviderAdapter>,
        model: Option<String>,
        context_window: u32,
    ) -> usize {
        let cycles = self.cycles.lock();
        for cycle in cycles.values() {
            cycle.rebind_model(super::llm_deliberation_faculty::ModelBinding {
                adapter: Arc::clone(&adapter),
                model: model.clone(),
                context_window,
            });
        }
        cycles.len()
    }

    /// How many persona minds are resident.
    pub fn len(&self) -> usize {
        self.cycles.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Process-global persona-workspace registry. One per process; persona minds are
/// assembled into it at spawn (`supervisor::materialize_adapters`) and resolved
/// from it by the `ai/should-respond` ServiceModule. Same pattern as
/// `modules::ai_provider::global_registry()` — the shared seam between the spawn
/// path that builds minds and the command path that runs them.
pub fn global() -> Arc<PersonaWorkspaceRegistry> {
    static GLOBAL: OnceLock<Arc<PersonaWorkspaceRegistry>> = OnceLock::new();
    GLOBAL
        .get_or_init(|| Arc::new(PersonaWorkspaceRegistry::new()))
        .clone()
}

// ── MEMENTO FIX (#138): volatile-tier persistence across deploy reboots ──
// "They wake up blank like Memento — an engineering failure; the flywheel
// falls apart" (Joel 2026-07-12; nine reboots that day = nine blank wakes).
// The persisted file is the SAME serialization grid-sync will ship — one
// format, one seam ([[persona-mind-persists-across-shutdowns]]).

/// On-disk shape: the working-memory snapshot plus the own-speech ring (the
/// ring is process-global in `deliberation_budget`, so it dies with the
/// process unless carried here — the repetition detectors were re-blinded by
/// every reboot until this).
#[derive(serde::Serialize, serde::Deserialize)]
struct PersistedVolatile {
    wm: super::working_memory::VolatileSnapshot,
    own_speech: Vec<String>,
}

fn volatile_path(persona_id: Uuid) -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    std::path::PathBuf::from(home)
        .join(".continuum/personas")
        .join(persona_id.to_string())
        .join("volatile.json")
}

/// Persist the volatile tier — atomic tmp+rename so a crash mid-write never
/// leaves a torn file (a torn mind-file failing to parse = silent blank wake,
/// the exact failure this exists to kill). Errors log loud and drop: losing
/// one interval of scratchpad is acceptable; blocking a tick is not.
fn save_volatile(persona_id: Uuid, wm: &super::working_memory::WorkingMemory) {
    let persisted = PersistedVolatile {
        wm: wm.snapshot(),
        own_speech: super::deliberation_budget::recent_own_speech(
            crate::identity::PeerId::from_uuid(persona_id),
        ),
    };
    let path = volatile_path(persona_id);
    let write = || -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec(&persisted)?)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    };
    if let Err(e) = write() {
        tracing::warn!(persona_id = %persona_id, error = %e, "volatile-tier save failed — one interval of scratchpad at risk");
    }
}

/// Load the previous life's volatile tier, if any. Unreadable/corrupt files
/// return None LOUDLY (a mind-file that fails to parse must never be silently
/// ignored twice — the warn is the operator's cue to look).
fn load_volatile(persona_id: Uuid) -> Option<PersistedVolatile> {
    let path = volatile_path(persona_id);
    let bytes = std::fs::read(&path).ok()?;
    match serde_json::from_slice(&bytes) {
        Ok(p) => Some(p),
        Err(e) => {
            tracing::warn!(persona_id = %persona_id, error = %e, path = %path.display(), "volatile-tier file unreadable — waking blank this once");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
    use crate::cognition::workspace::Decision;
    use crate::persona::engram::{ChatMessageRef, Engram, EngramKind, EngramOrigin, TrustState};
    use crate::persona::recall_metadata::{RecallMetadata, RecallMetadataRegistry};

    fn seed_admission(now_ms: u64) -> Arc<AdmissionState> {
        let recall_meta = Arc::new(RecallMetadataRegistry::new());
        let state = Arc::new(AdmissionState::new(recall_meta.clone()));
        let id = Uuid::new_v4();
        let engram = Engram {
            context_id: None,
            id,
            kind: EngramKind::Episodic,
            content: "the deploy pipeline went green after the 4pm fix".to_string(),
            origin: EngramOrigin::Chat(ChatMessageRef {
                message_id: Uuid::new_v4(),
                room_id: Uuid::new_v4(),
                sender_id: Uuid::new_v4(),
                posted_at_ms: now_ms,
                content_hash: "h".to_string(),
            }),
            recall_keys: Vec::new(),
            admitted_at_ms: now_ms,
            trust_state_at_admission: TrustState::ApprovedPeer,
            admission_trace_id: None,
        };
        state.push_for_test(engram);
        recall_meta.admit(
            id,
            RecallMetadata {
                salience: 0.7,
                access_count: 0,
                last_accessed_ms: 0,
                protected_until_ms: 0,
                last_decayed_ms: now_ms,
            },
        );
        state
    }

    fn cfg_for(persona_id: Uuid) -> PersonaBrainConfig {
        PersonaBrainConfig {
            persona_id,
            persona_name: "Ivar".to_string(),
            system_prompt: "You are Ivar, an engineer on the grid.".to_string(),
            admission: seed_admission(1_000_000_000),
            adapter: Arc::new(HeuristicInferenceAdapter::new()),
            capacity: None,
            grounding_sources: Vec::new(),
            embedder: None,
            tool_executor: None,
            context_window: crate::cognition::serving_plan::MIN_SERVE_CTX,
            // Synchronous recall in the harness: these tests assert recall bids in
            // phase 1 on the same tick, which a deferred (cold-start) worker can't
            // satisfy. Deferral is a live-path concern, tested in deferred_faculty.rs.
            defer_recall: false,
            defer_grounding: false,
            // Harness/test cycles keep recall (they assert on its bids); suppression is a
            // benchmark-reproducibility knob set only by the eval fork.
            suppress_recall: false,
        }
    }

    // what this catches: the assembled cycle runs a FULL persona mind end-to-end —
    // recall (hippocampus) bids in phase 1, deliberation (real adapter) decides in
    // phase 2 over that context — and yields a Decision. This is the production
    // assembly path; swap the adapter for LlamaCppAdapter and it's a live persona.
    #[tokio::test]
    async fn assembled_cycle_produces_a_decision() {
        let persona = Uuid::new_v4();
        let cycle = build_workspace_cycle(cfg_for(persona));
        let ws = cycle.run("teammate: what's the deploy status?").await;
        // The mind reached a participation verdict (heuristic adapter → Speak).
        assert!(matches!(ws.decision(), Some(Decision::Speak { .. })));
    }

    // what this catches: #396 — resolve_persona is the ONE formal identity boundary
    // for eval/benchmark. It must (a) resolve a full UUID, an 8-char short-id, and a
    // case-insensitive persona NAME to the typed id; (b) pass a well-formed full UUID
    // through WITHOUT a liveness check (race safety — a persona still assembling isn't
    // false-rejected; the fork wait absorbs it); (c) fail LOUD on garbage, naming the
    // online roster — never a silent guess. The loose-String id that skipped all of
    // this is exactly what fed a dead reference to a doomed 10s eval.
    #[test]
    fn resolve_persona_takes_uuid_short_id_or_name_and_fails_loud() {
        let registry = PersonaWorkspaceRegistry::new();
        let asha = Uuid::new_v4();
        let atlas = Uuid::new_v4();
        let mut asha_cfg = cfg_for(asha);
        asha_cfg.persona_name = "Asha".to_string();
        let mut atlas_cfg = cfg_for(atlas);
        atlas_cfg.persona_name = "Atlas".to_string();
        registry.register_from_cfg(asha_cfg);
        registry.register_from_cfg(atlas_cfg);

        // full UUID
        assert_eq!(registry.resolve_persona(&asha.to_string()).unwrap(), asha);
        // 8-char short-id prefix
        assert_eq!(
            registry.resolve_persona(&asha.to_string()[..8]).unwrap(),
            asha
        );
        // case-insensitive name
        assert_eq!(registry.resolve_persona("atlas").unwrap(), atlas);
        assert_eq!(registry.resolve_persona("ASHA").unwrap(), asha);

        // (b) a well-formed but NON-live full UUID passes through — race safety. The
        // caller's fork wait, not this boundary, decides liveness.
        let ghost = Uuid::new_v4();
        assert_eq!(registry.resolve_persona(&ghost.to_string()).unwrap(), ghost);

        // (c) garbage fails loud AND names the roster so the operator can fix it.
        let err = registry.resolve_persona("general").unwrap_err();
        assert!(err.contains("Asha") && err.contains("Atlas"), "roster hint missing: {err}");
    }

    // what this catches: ONE cycle per persona — get_or_build is idempotent and
    // returns the SAME Arc, so a persona's continuous mind is reused across every
    // room it services (the "one soul, many rooms" / anti-Severance invariant).
    #[tokio::test]
    async fn registry_keeps_one_mind_per_persona() {
        let registry = PersonaWorkspaceRegistry::new();
        let persona = Uuid::new_v4();
        let first = registry.get_or_build(cfg_for(persona));
        let second = registry.get_or_build(cfg_for(persona));
        assert!(
            Arc::ptr_eq(&first, &second),
            "same persona must resolve to the SAME mind across rooms — not severed per-room"
        );
        assert_eq!(registry.len(), 1);
        // A different persona is a different mind.
        let _ = registry.get_or_build(cfg_for(Uuid::new_v4()));
        assert_eq!(registry.len(), 2);
    }

    // what this catches: RESPAWN must replace the mind, not keep the stale one.
    // A persona can respawn in-process (node resilience) with a fresh admission +
    // adapter; the supervisor uses register() (overwrite), so get() returns the
    // NEW cycle, not the prior lifetime's orphaned one. (get_or_build would have
    // discarded the fresh config — the bug this guards.)
    #[tokio::test]
    async fn register_overwrites_on_respawn() {
        let registry = PersonaWorkspaceRegistry::new();
        let persona = Uuid::new_v4();
        // register_from_cfg IS the production overwrite path (supervisor.rs spawn);
        // it builds + caches and returns the fresh Arc.
        let first = registry.register_from_cfg(cfg_for(persona));
        let second = registry.register_from_cfg(cfg_for(persona));
        let got = registry.get(&persona).expect("registered");
        assert!(
            Arc::ptr_eq(&got, &second),
            "respawn must resolve to the FRESH mind"
        );
        assert!(
            !Arc::ptr_eq(&got, &first),
            "the prior lifetime's mind is replaced"
        );
        assert_eq!(registry.len(), 1);
    }

    // THE LIVE BRING-UP: a persona's mind thinks with the REAL local model.
    // Runs the EXACT production assembly path (build_workspace_cycle → RecallFaculty
    // + LlmDeliberationFaculty) against the real LlamaCppAdapter (qwen3.5-4b-code-
    // forged on disk), over a real consolidated burst, and prints Ivar's actual
    // words. #[ignore] — needs a local GGUF + Metal. Run:
    //   CARGO_TARGET_DIR=$HOME/.continuum/cache/cargo-target \
    //   cargo test -p continuum-core --features metal,accelerate \
    //     cognition::persona_workspace::tests::ivar_thinks_with_the_real_model \
    //     -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "needs local GGUF + Metal; run with --ignored --nocapture"]
    async fn ivar_thinks_with_the_real_model() {
        use crate::ai::adapter::AIProviderAdapter;
        use crate::inference::llamacpp_adapter::LlamaCppAdapter;

        crate::model_registry::init_global().expect("model_registry init");
        // context_length MUST be set explicitly (the backend refuses to silently
        // fall back to n_ctx_train — the 2026-04 Metal-KV-blowup guard). new()
        // doesn't set it; production uses for_persona(profile). 8192 fits Metal.
        let adapter: Arc<dyn AIProviderAdapter> = Arc::new(
            LlamaCppAdapter::new()
                .with_context_length(8192)
                .with_n_seq_max(1),
        );
        eprintln!(
            "[live] adapter={} default_model={}",
            adapter.name(),
            adapter.default_model()
        );

        use crate::cognition::llm_deliberation_faculty::LlmDeliberationFaculty;
        use crate::cognition::recall_faculty::RecallFaculty;
        use crate::cognition::workspace::{
            Faculty, SalienceArbiter, Workspace, WorkspaceCaptureSink, WorkspaceCycle,
            WorkspaceTrace,
        };

        let persona = Uuid::new_v4();
        let admission = seed_admission(1_718_600_000_000);
        let system_prompt = "You are Ivar, a thoughtful engineer and a citizen on the grid. \
            You speak concisely, and only when you have something worth adding.";

        // Assemble the faculties directly (mirrors build_workspace_cycle) so we
        // keep a typed handle to the deliberation faculty — to introspect the
        // EXACT prompt it feeds the model.
        let delib = Arc::new(LlmDeliberationFaculty::new(
            persona,
            "Ivar",
            system_prompt,
            adapter,
        ));
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(RecallFaculty::new(persona, admission)),
            delib.clone(),
        ];

        // The EXISTING capture harness: a WorkspaceCaptureSink records every phase
        // of the tick (all bids incl. losers, the assembled context, the decision)
        // so we can diagnose cognition at any phase — record + recreate, not guess.
        #[derive(Default)]
        struct CapturingSink(Mutex<Vec<WorkspaceTrace>>);
        impl WorkspaceCaptureSink for CapturingSink {
            fn record(&self, t: &WorkspaceTrace) {
                self.0.lock().push(t.clone());
            }
        }
        let sink = Arc::new(CapturingSink::default());

        let cycle = WorkspaceCycle::new(
            faculties,
            Arc::new(SalienceArbiter),
            DEFAULT_WORKSPACE_CAPACITY,
        )
        .with_capture(sink.clone());

        let burst = "general room:\n\
            Joel: morning all\n\
            teammate: the deploy from yesterday — did we ever figure out what broke it?\n\
            teammate: ivar you were looking at it right?";

        let ws = cycle.run(burst).await;

        // ---- Glass box: diagnose cognition at EVERY phase ----
        let trace = sink.0.lock().pop().expect("a tick was recorded");
        eprintln!("\n================ COGNITION TRACE ================");
        eprintln!("WORLD-STATE (the burst):\n{}\n", trace.world_state);
        eprintln!("PHASE 1 — perception bids (the full competition, incl. losers):");
        for b in &trace.bids {
            eprintln!(
                "  [{:<12} s={:.2}] {}  ({})",
                b.faculty.as_str(),
                b.salience,
                b.content.replace('\n', " / "),
                b.reasoning
            );
        }
        eprintln!("\nASSEMBLED CONTEXT the decider saw (context_broadcast = the RAG):");
        for c in &trace.context_broadcast {
            eprintln!(
                "  [{}] {}",
                c.faculty.as_str(),
                c.content.replace('\n', " / ")
            );
        }

        // ---- EXACTLY what the LLM was fed (reconstruct the pre-deliberation ws) ----
        // Route through the constructor (opaque single-turn burst) so this can't
        // drift as Workspace gains fields; then graft on the recorded context
        // broadcast the decider actually saw. The trace does not yet record the
        // turn's directedness or self-initiation; reconstruct as ambient /
        // message-driven (the live defaults). TODO(#9): carry directedness on the
        // trace so a replayed directed turn shows the silence escape withheld as it was.
        let mut context_ws = Workspace::in_room(burst.to_string(), trace.room_id);
        context_ws.broadcast = trace.context_broadcast.clone();
        let view = delib.prompt_view(&context_ws);
        eprintln!("\n--------------- WHAT THE LLM WAS FED ---------------");
        eprintln!("[SYSTEM]\n{}\n", view.system);
        eprintln!("[CONVERSATION]\n{}", view.user_text());

        eprintln!("\n--------------- Ivar's DECISION ---------------");
        match ws.decision() {
            Some(Decision::Speak { text }) => eprintln!("Ivar SPEAKS:\n{text}"),
            Some(Decision::RaiseUnprompted { text }) => eprintln!("Ivar RAISES:\n{text}"),
            Some(Decision::Act { calls, intent }) => {
                eprintln!("Ivar ACTS ({} call(s)) — intent: {intent}", calls.len())
            }
            Some(Decision::Pass) | None => eprintln!("Ivar chose silence (PASS)."),
        }
        eprintln!("=================================================\n");

        assert!(
            ws.decision().is_some(),
            "the persona's mind must reach a decision through the real model"
        );
    }

    // A constructible RagSource whose delivery is never exercised — these tests
    // assert the deferrability CLASSIFICATION on GroundingSource, not delivery.
    struct ClassifyStub;
    #[async_trait::async_trait]
    impl RagSource for ClassifyStub {
        fn source_id(&self) -> &'static str {
            "classify-stub"
        }

    fn expand_command(&self) -> Option<&'static str> {
        // Test/stub source — nothing further to fetch.
        None
    }

    /// Test/stub source — floorless, so it never encodes a production floor.
    fn floor_tokens(&self) -> u32 {
        0
    }
        async fn deliver(
            &self,
            _ctx: &crate::persona::rag_budget::RagContext,
            _budget: u32,
            resolution: crate::persona::rag_budget::ResolutionPreference,
        ) -> crate::persona::rag_budget::RagDelivery {
            crate::persona::rag_budget::RagDelivery {
                source_id: "classify-stub".to_string(),
                items: Vec::new(),
                tokens_used: 0,
                continuation: None,
                resolution_used: resolution,
            }
        }
        async fn deliver_continuation(
            &self,
            _ctx: &crate::persona::rag_budget::RagContext,
            _cursor: crate::persona::rag_budget::ContinuationCursor,
            _budget: u32,
        ) -> Option<crate::persona::rag_budget::RagDelivery> {
            None
        }
    }

    // what this catches: deferrability is ORTHOGONAL to salience, and the SAFE
    // DEFAULT is synchronous. framing() is ColdStartCritical — a fresh grounding
    // source runs ON the loop until deliberately opted off, so doctrine (the
    // participation gate) can never silently start serving a cold-start `None`
    // (speaking in a room it shouldn't on turn one). `.defer_tolerant()` flips
    // ONLY the schedule axis, leaving StandingFraming salience untouched. If a
    // future edit folds salience into the deferrability builder, or flips the
    // default to DeferTolerant, this fails — guarding the two-independent-axes
    // invariant the live supervisor wiring depends on.
    #[test]
    fn deferrability_is_orthogonal_to_salience_and_defaults_to_synchronous() {
        let s: Arc<dyn RagSource> = Arc::new(ClassifyStub);

        let critical = GroundingSource::framing(Arc::clone(&s));
        assert_eq!(
            critical.deferrability,
            Deferrability::ColdStartCritical,
            "safe default: a framing source runs synchronously until opted off"
        );
        assert_eq!(critical.policy, SaliencePolicy::StandingFraming);

        let tolerant = GroundingSource::framing(Arc::clone(&s)).defer_tolerant();
        assert_eq!(
            tolerant.deferrability,
            Deferrability::DeferTolerant,
            "opt-in flips ONLY the schedule axis"
        );
        assert_eq!(
            tolerant.policy,
            SaliencePolicy::StandingFraming,
            "orthogonal: deferrability must not touch the salience policy"
        );

        // retrieved() (the other constructor) also defaults to the safe side.
        let retrieved = GroundingSource::retrieved(Arc::clone(&s));
        assert_eq!(retrieved.deferrability, Deferrability::ColdStartCritical);
    }

    // what this catches: capability consistency on the eval fork — grounding
    // marked requires_hands (workspace-map: "drill in with code/list and
    // code/tree") must be DROPPED from a tools-stripped fork and KEPT on a
    // handed fork. Glass-boxed 2026-07-10: every captured spoken-exam prompt
    // (484/484) carried the workspace-map telling her to use tools that were
    // stripped for the exam — the RAG lying to her about her own affordances.
    #[tokio::test]
    async fn eval_fork_drops_hands_describing_grounding_when_tools_stripped() {
        const HANDS_MARKER: &str = "drill in with code/list and code/tree";
        let registry = PersonaWorkspaceRegistry::new();
        let persona = Uuid::new_v4();
        // SlowGrounding's fixed roster line stands in for capability-NEUTRAL
        // grounding; the hands-describing source is its own tiny stub.
        struct HandsMap;
        #[async_trait::async_trait]
        impl RagSource for HandsMap {
            fn source_id(&self) -> &'static str {
                "workspace-map"
            }

    fn expand_command(&self) -> Option<&'static str> {
        // Test/stub source — nothing further to fetch.
        None
    }

    /// Test/stub source — floorless, so it never encodes a production floor.
    fn floor_tokens(&self) -> u32 {
        0
    }
            async fn deliver(
                &self,
                _ctx: &crate::persona::rag_budget::RagContext,
                _budget: u32,
                resolution: crate::persona::rag_budget::ResolutionPreference,
            ) -> crate::persona::rag_budget::RagDelivery {
                crate::persona::rag_budget::RagDelivery {
                    source_id: "workspace-map".to_string(),
                    items: vec![crate::persona::rag_budget::RagItem {
                        content: format!("workspace layout — {HANDS_MARKER}"),
                        tokens: 12,
                        metadata: serde_json::Value::Null,
                    }],
                    tokens_used: 12,
                    continuation: None,
                    resolution_used: resolution,
                }
            }
            async fn deliver_continuation(
                &self,
                _ctx: &crate::persona::rag_budget::RagContext,
                _cursor: crate::persona::rag_budget::ContinuationCursor,
                _budget: u32,
            ) -> Option<crate::persona::rag_budget::RagDelivery> {
                None
            }
        }
        let mut cfg = cfg_for(persona);
        cfg.grounding_sources = vec![
            GroundingSource::framing(Arc::new(HandsMap)).requires_hands(),
            GroundingSource::framing(Arc::new(SlowGrounding { delay_ms: 0 })),
        ];
        registry.register_from_cfg(cfg);

        // Spoken exam (no hands): the workspace-map must not reach her mind.
        let spoken = registry
            .fork_eval_cycle(&persona, false, None, false)
            .expect("template retained");
        let ws = spoken.run("proctor: reverse a string in Rust").await;
        assert!(
            !ws.perceived().contains(HANDS_MARKER),
            "tool-stripped fork must NOT perceive hands-describing grounding:\n{}",
            ws.perceived()
        );
        // Neutral grounding survives the filter — only hands-claims are dropped.
        assert!(
            ws.perceived().contains("roster:"),
            "capability-neutral grounding must survive the hands filter:\n{}",
            ws.perceived()
        );

        // Handed exam: the map is real guidance and must be delivered.
        let handed = registry
            .fork_eval_cycle(&persona, true, None, false)
            .expect("template retained");
        let ws = handed.run("proctor: reverse a string in Rust").await;
        assert!(
            ws.perceived().contains(HANDS_MARKER),
            "handed fork must keep the workspace-map:\n{}",
            ws.perceived()
        );
    }

    // what this catches (#207): the reproducible-absolute benchmark knob. A default fork keeps
    // the recall faculty (memories intact — the natural persona), so its ABSOLUTE score can
    // drift as her durable engram store grows between runs; a `suppress_recall` fork OMITS
    // recall entirely (system + task + grounding only), pinning the absolute baseline. The two
    // forks must differ by exactly the recall faculty — the whole point of the knob.
    #[tokio::test]
    async fn suppress_recall_omits_the_recall_faculty_from_the_eval_fork() {
        use crate::cognition::workspace::FacultyId;
        let registry = PersonaWorkspaceRegistry::new();
        let persona = Uuid::new_v4();
        registry.register_from_cfg(cfg_for(persona));

        let with_recall = registry
            .fork_eval_cycle(&persona, false, None, false)
            .expect("template retained");
        assert!(
            with_recall.faculty_ids().contains(&FacultyId::Recall),
            "default fork must keep recall (memories intact): {:?}",
            with_recall.faculty_ids()
        );

        let suppressed = registry
            .fork_eval_cycle(&persona, false, None, true)
            .expect("template retained");
        assert!(
            !suppressed.faculty_ids().contains(&FacultyId::Recall),
            "suppress_recall fork must OMIT recall for a reproducible absolute baseline: {:?}",
            suppressed.faculty_ids()
        );
    }

    /// A grounding RagSource whose deliver is deliberately slow — models the real
    /// I/O cost (roster query / workspace-map scan) the slice moves off the hot
    /// path. Exercised through the LIVE grounding path (GroundingSource →
    /// RagSourceFaculty → DeferredFaculty), not a hand-built faculty.
    struct SlowGrounding {
        delay_ms: u64,
    }
    #[async_trait::async_trait]
    impl RagSource for SlowGrounding {
        fn source_id(&self) -> &'static str {
            "slow-grounding"
        }

    fn expand_command(&self) -> Option<&'static str> {
        // Test/stub source — nothing further to fetch.
        None
    }

    /// Test/stub source — floorless, so it never encodes a production floor.
    fn floor_tokens(&self) -> u32 {
        0
    }
        async fn deliver(
            &self,
            _ctx: &crate::persona::rag_budget::RagContext,
            _budget: u32,
            resolution: crate::persona::rag_budget::ResolutionPreference,
        ) -> crate::persona::rag_budget::RagDelivery {
            tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
            crate::persona::rag_budget::RagDelivery {
                source_id: "slow-grounding".to_string(),
                items: vec![crate::persona::rag_budget::RagItem {
                    content: "roster: Ivar [persona], win-claude [claude]".to_string(),
                    tokens: 12,
                    metadata: serde_json::Value::Null,
                }],
                tokens_used: 12,
                continuation: None,
                resolution_used: resolution,
            }
        }
        async fn deliver_continuation(
            &self,
            _ctx: &crate::persona::rag_budget::RagContext,
            _cursor: crate::persona::rag_budget::ContinuationCursor,
            _budget: u32,
        ) -> Option<crate::persona::rag_budget::RagDelivery> {
            None
        }
    }

    // what this catches: the SPEED the grounding-deferral slice buys, measured on
    // the live critical path. Model LOCKED (same HeuristicInferenceAdapter both
    // forks), ONE variable changed (defer_grounding) — the glass-box controlled
    // experiment. A slow grounding source (60ms deliver, modeling roster/
    // workspace-map I/O) sits ON the perception barrier when synchronous; when
    // deferred it runs in the bg and the WARM tick serves reprojected last-good,
    // so it LEAVES the barrier. critical_path_us = max(perception)+max(delib);
    // the model (delib) is identical across forks, so the delta isolates exactly
    // the grounding cost removed from the loop. If a regression puts the deferred
    // source back on the barrier, the delta collapses and this fails.
    #[tokio::test]
    async fn deferring_grounding_removes_its_deliver_cost_from_the_critical_path() {
        use crate::cognition::workspace_dashboard::DashboardCaptureSink;
        const DELAY_MS: u64 = 60;

        // Build a cycle with ONE slow grounding source, run a cold tick + a warm
        // tick in the same room, and return the WARM tick's critical path.
        async fn warm_critical_path(defer_grounding: bool) -> u128 {
            let persona = Uuid::new_v4();
            let room = Uuid::new_v4();
            let slow: Arc<dyn RagSource> = Arc::new(SlowGrounding { delay_ms: DELAY_MS });
            let mut cfg = cfg_for(persona);
            cfg.grounding_sources = vec![GroundingSource::framing(slow).defer_tolerant()];
            cfg.defer_grounding = defer_grounding;

            let sink = Arc::new(DashboardCaptureSink::new(persona));
            let rx = sink.subscribe();
            let cycle = build_workspace_cycle(cfg).with_capture(sink.clone());

            // Tick 1: cold-start for the deferred fork (kicks the bg worker, serves
            // None); the sync fork pays the full deliver here too.
            let _ = cycle
                .run_in_room("teammate: where are we on the deploy?", room)
                .await;
            // Let the bg worker land its finding so the warm tick serves last-good.
            tokio::time::sleep(std::time::Duration::from_millis(DELAY_MS * 2)).await;
            // Tick 2 (warm): the steady-state the live persona actually runs at.
            let _ = cycle
                .run_in_room("teammate: and the rollback plan?", room)
                .await;
            // record() ran synchronously inside run_in_room → the watch holds tick 2.
            let cp = rx.borrow().critical_path_us;
            cp
        }

        let sync_cp = warm_critical_path(false).await;
        let deferred_cp = warm_critical_path(true).await;

        eprintln!("\n=== grounding-deferral speed delta (model locked) ===");
        eprintln!("defer_grounding=false  critical_path = {sync_cp} µs");
        eprintln!("defer_grounding=true   critical_path = {deferred_cp} µs");
        eprintln!(
            "removed from the loop  = {} µs (~{} ms)",
            sync_cp.saturating_sub(deferred_cp),
            sync_cp.saturating_sub(deferred_cp) / 1000
        );
        eprintln!("====================================================\n");

        let delay_us = (DELAY_MS as u128) * 1000;
        assert!(
            sync_cp >= delay_us - 10_000,
            "sync fork must pay ~the deliver cost on the barrier: {sync_cp}µs < {}µs",
            delay_us - 10_000
        );
        assert!(
            deferred_cp < delay_us / 2,
            "deferred fork's warm tick must NOT pay the deliver cost: {deferred_cp}µs"
        );
        assert!(
            sync_cp.saturating_sub(deferred_cp) >= 40_000,
            "deferral must remove ~the grounding deliver from the critical path; \
             delta was only {}µs",
            sync_cp.saturating_sub(deferred_cp)
        );
    }

    mod hands_isolation {
        use super::*;
        use crate::cognition::tool_executor::{CommandToolExecutor, ToolExecutor};
        use crate::modules::code::{CodeModule, CodeState};
        use crate::routing::CallerIdentity;
        use crate::runtime::{CommandExecutor, InProcessTransport, ModuleRegistry};
        use continuum_client::Connection;
        use dashmap::DashMap;

        /// One core with the REAL `CodeModule` — the single process-global engine map
        /// every caller's workspace lives in, which is exactly why the leak is possible.
        fn hands_for(persona: Uuid) -> ActingHands {
            let registry = Arc::new(ModuleRegistry::new());
            registry.register(Arc::new(CodeModule::new(Arc::new(CodeState::new(
                Arc::new(DashMap::new()),
                Arc::new(DashMap::new()),
                tokio::runtime::Handle::current(),
            )))));
            let executor = Arc::new(CommandExecutor::new(registry));
            let transport = InProcessTransport::new(
                executor,
                Some(CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(
                    persona,
                ))),
            );
            ActingHands {
                persona_id: persona,
                persona_name: "Anwen".to_string(),
                executor: Arc::new(CommandToolExecutor::new(Connection::new(transport))),
            }
        }

        /// What her hands can SEE right now — the honest probe, because "where is the
        /// engine rooted" is only interesting insofar as it changes what she can read.
        async fn listing(hands: &ActingHands) -> String {
            let ctx = crate::cognition::tool_executor::ToolExecutionContext {
                persona_id: hands.persona_id,
                persona_name: hands.persona_name.clone(),
                session_id: Uuid::new_v4(),
                context_id: Uuid::new_v4(),
                caller_context: serde_json::Value::Null,
                persona_config: crate::cognition::tool_executor::PersonaMediaConfigLite {
                    auto_load_media: false,
                    supported_media_types: vec![],
                },
            };
            let call = crate::ai::types::ToolCall {
                id: "ls".to_string(),
                name: "code/list".to_string(),
                input: serde_json::json!({ "path": "." }),
            };
            let out = hands
                .executor
                .execute_native_batch(std::slice::from_ref(&call), &ctx, 8000)
                .await
                .expect("code/list runs");
            out.results[0].content.clone()
        }

        // what this catches: #312 — the measurement leaving the LIVING persona standing in
        // the exam repo. `code/create-workspace` keys the file engine on the CALLER, the
        // engines live in ONE process-global map, and a measurement fork shares the living
        // persona's executor AND her id — so rooting "the fork" re-roots HER, permanently.
        // Measured 2026-08-04: after a SWE-bench solve on a flask clone, Anwen's live self
        // ran `code/list(path=src)` → `flask/` and `code/read(src/flask/app.py)` in her own
        // room turns, hours later. The MIND had a guard (EvalIsolation) from #59; the HANDS
        // had none. If the restore regresses, the exam file is still visible here.
        #[tokio::test]
        async fn a_measurement_returns_her_hands_to_her_own_workspace() {
            let exam = tempfile::TempDir::new().expect("exam dir");
            let home = tempfile::TempDir::new().expect("home dir");
            std::fs::write(exam.path().join("EXAM_ONLY_blueprints.py"), "# flask\n").unwrap();
            std::fs::write(home.path().join("HER_OWN_notes.md"), "# mine\n").unwrap();

            let hands = hands_for(Uuid::new_v4());

            // Root at the sandbox, as agent/solve does before a drive.
            drive_create_workspace(&hands, &exam.path().to_string_lossy(), &[], "root", false)
                .await
                .expect("roots at the exam sandbox");
            let during = listing(&hands).await;
            assert!(
                during.contains("EXAM_ONLY_blueprints.py"),
                "the measurement must actually move her hands, or it scores a false zero: {during}"
            );

            // Measurement over.
            restore_acting_workspace_at(&hands, &home.path().to_string_lossy())
                .await
                .expect("restores to her own workspace");

            let after = listing(&hands).await;
            assert!(
                !after.contains("EXAM_ONLY_blueprints.py"),
                "SHE IS STILL IN THE EXAM ROOM (#312) — her live hands can still see the \
                 measurement sandbox after the run: {after}"
            );
            assert!(
                after.contains("HER_OWN_notes.md"),
                "restore must land her in her OWN workspace, not merely unroot her: {after}"
            );
        }

        // what this catches: a pure-cognition persona has no hands, so there is nothing to
        // root and nothing to restore. `of` must say so rather than have the caller invent
        // a handle — the fail-loud in `root_acting_workspace` depends on this being None.
        #[test]
        fn a_handless_cycle_yields_no_hands() {
            let cycle = crate::cognition::workspace::WorkspaceCycle::new(
                vec![],
                Arc::new(crate::cognition::workspace::SalienceArbiter),
                4,
            );
            assert!(ActingHands::of(&cycle).is_none());
        }
    }
}
