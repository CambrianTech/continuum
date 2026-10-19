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

use continuum_base::time::EpochMs;
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

use crate::identity::{PeerId, PersonaRef};

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
    /// Where a room's authored manifest comes from, so each turn can be stamped with
    /// the affordances the room's RECIPE declares and the deliberation faculty can
    /// select her tool surface from them (S1). The live spawn passes
    /// [`crate::experience::source::node_experience_source`]; `None` (harnesses,
    /// a headless boot) stamps nothing and the surface is the pre-S1 one.
    pub experience: Option<crate::experience::source::SharedExperienceSource>,
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
    /// FRAMING BY IMPORTANCE, VOLATILE BY CONTENT (2026-08-22). `StandingFraming`
    /// conflated two axes: selection priority (the attention floor — correct for
    /// these sources) and PLACEMENT in the cacheable stable tier (correct only for
    /// byte-stable content). `debug/prompt-reuse` convicted active-work and
    /// room-board on live captures: their content flaps every turn ("card X
    /// [Claimed]" ↔ "[work] Your claim on card X…") at ~char 7,900 of 90–185k-char
    /// prompts — a mutating span parked at the TOP of the prompt, invalidating the
    /// KV of everything behind it (worst pairs: 4% predicted reuse). `true` keeps
    /// the salience floor but serializes the content in the volatile tail, nearest
    /// generation, where mutation costs only itself.
    pub volatile_content: bool,
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
            volatile_content: false,
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
            volatile_content: false,
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

    /// Framing whose CONTENT mutates per turn (see field doc): keeps the
    /// attention floor, loses the stable-tier placement.
    pub fn volatile_content(mut self) -> Self {
        self.volatile_content = true;
        self
    }
}

/// A fork of a citizen (an eval or a solve) acts with ITS OWN hands: the same persona,
/// gated the same way, in a fresh acting context (`ToolExecutor::in_fresh_hands`). Before
/// this a fork cloned her live executor, so its root moved her live tools and its restore
/// moved them home (docs/architecture/HER-HANDS.md).
///
/// Fails closed: hands that can't open a context of their own are never borrowed. The fork
/// then runs without hands (and without the grounding that describes them), and says so.
fn give_fork_its_own_hands(cfg: &mut PersonaBrainConfig, persona_id: Uuid) {
    let Some(live) = cfg.tool_executor.as_ref() else {
        return;
    };
    match live.in_fresh_hands(persona_id) {
        Some(own) => cfg.tool_executor = Some(own),
        None => {
            cfg.tool_executor = None;
            cfg.grounding_sources.retain(|g| !g.requires_hands);
            continuum_base::probe!(
                class = "workspace.fork_without_hands",
                persona = %persona_id,
                "these hands can't open a context of their own, so the fork runs without hands rather than borrowing hers"
            );
        }
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
    assemble_workspace_cycle(cfg, None, Capture::Recorded).0
}

/// Whether an assembled cycle's ticks and prompts are captured. Her MIND cycle is
/// `Withheld` (PRIVACY-OF-THOUGHT.md §4): it runs only her mind room's turns, so it opens no
/// prompt capture and no workspace trace at all.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Capture {
    Recorded,
    Withheld,
}

fn assemble_workspace_cycle(
    cfg: PersonaBrainConfig,
    restored: Option<PersistedVolatile>,
    capture: Capture,
) -> (WorkspaceCycle, Arc<WorkingMemory>) {
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
    // Persistence belongs to resident registration, independently of whether
    // recall runs inline or deferred. Constructing a fork grants no disk access.
    if let Some(mut persisted) = restored {
        let n = persisted.wm.entries.len();
        // Results restored from a snapshot written before results were saved (#4963) carry no
        // handle: they predate custody, they are not failed saves. Save them now, so they
        // collapse within the share like any saved result and the "save failed" probe names
        // only a real failure (BigMama's live receipt on the 5090, card df0035bf). The pin
        // holds them live until `restore` below publishes the memory's own set.
        let restore_pin = persisted.wm.retain_handleless(cfg.persona_id);
        // The previous life's handoff re-seeds this one: her ledger and card are known at
        // the NEXT seam too, without a wall read (card 49b5e806).
        if let Some(handoff) = persisted.wm.handoff.as_ref() {
            crate::cognition::handoff::reload(cfg.persona_id, handoff);
        }
        working_memory.restore(persisted.wm);
        drop(restore_pin);
        let peer = crate::identity::PeerId::from_uuid(cfg.persona_id);
        match &persisted.own_speech {
            OwnSpeechPersisted::ByRoom(by_room) => {
                for (room, utterances) in by_room {
                    for utterance in utterances {
                        super::deliberation_budget::record_own_speech(peer, *room, utterance);
                    }
                }
            }
            // Pre-room-scoping file: unattributable, so dropped rather than
            // mis-filed into a room she may never have spoken in. See
            // OwnSpeechPersisted — hydrate_speech_rings re-seeds with rooms.
            OwnSpeechPersisted::Legacy(flat) => {
                continuum_base::probe!(
                    class = "persona.volatile.own_speech_legacy_dropped",
                    persona = %cfg.persona_name,
                    dropped = flat.len(),
                    "pre-room-scoping own-speech ring carried no room — dropped, \
                     durable-transcript hydration re-seeds it"
                );
            }
        }
        continuum_base::probe!(
            class = "persona.volatile.restored",
            persona = %cfg.persona_name,
            entries = n,
            ring = persisted.own_speech.len(),
            "volatile tier restored — waking mid-work, not blank"
        );
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
    }
    // The CognitionModule tick checkpoints the registry's current residents.
    // Construction must not spawn a writer: forks share the persona UUID and
    // replaced cycles can remain alive in callers (#3918).
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
        let faculty = RagSourceFaculty::new(cfg.persona_id, g.source, g.policy)
            .with_budget(grounding_budget)
            .with_volatile_content(g.volatile_content)
            // Deferrability is also a LOUDNESS contract, not only a scheduling one
            // (#3873). ColdStartCritical means "absent here is a wrong turn", so a
            // source in this tier that delivers nothing says so in the probe
            // ledger instead of abstaining silently — which is how `room-doctrine`
            // sat at 0 bids in 873 consecutive ticks with nothing noticing.
            .cold_start_critical(matches!(g.deferrability, Deferrability::ColdStartCritical));
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
    let experience = cfg.experience.clone();
    let adapter = cfg.adapter;
    let model_binding = super::llm_deliberation_faculty::model_binding(
        Arc::clone(&adapter),
        None,
        cfg.context_window,
    );
    let mut deliberation =
        LlmDeliberationFaculty::new(cfg.persona_id, cfg.persona_name, cfg.system_prompt, adapter)
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
    // fed, what did she emit?" is answerable token-for-token. Native hosts use
    // the recorder's shared home discovery even without a shell HOME variable.
    // A persona can respawn within one process and restart its cycle counter.
    // Both capture owners receive the SAME construction session, so session +
    // persona + CycleId joins every request without a singular workspace call ID.
    let capture_session = Uuid::new_v4();
    if capture == Capture::Recorded {
        match super::prompt_capture::JsonlPromptCaptureSink::open_for_persona(cfg.persona_id) {
            Ok(sink) => {
                deliberation =
                    deliberation.with_prompt_capture(Arc::new(sink.with_session(capture_session)))
            }
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
    .with_model_binding(model_binding)
    // S1: the room's recipe reaches the mind. Without this the cycle stamps no
    // affordances and every room offers the same global surface.
    .with_experience_source(experience);

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
    let cycle = if capture == Capture::Withheld {
        cycle
    } else {
        match super::workspace_capture::JsonlWorkspaceCaptureSink::open_for_persona(cfg.persona_id)
        {
            Ok(sink) => cycle.with_capture(Arc::new(sink.with_session(capture_session))),
            Err(e) => {
                tracing::warn!(
                    persona_id = %cfg.persona_id,
                    error = %e,
                    "workspace trace capture unavailable; running with Noop capture"
                );
                cycle
            }
        }
    };
    (cycle, working_memory)
}

/// Persona-scoped registry of continuous minds. One `Arc<WorkspaceCycle>` per
/// persona; lookups by `persona_id`. `ai/should-respond` resolves the cycle here,
/// runs it over the room's consolidated burst, and reads the `Decision`.
#[derive(Default)]
pub struct PersonaWorkspaceRegistry {
    cycles: Mutex<HashMap<Uuid, ResidentWorkspace>>,
    /// Assembly opens captures and starts faculty wiring. Serialize constructors
    /// without holding the lookup lock, so a losing lazy build has no side effects.
    assembly: Mutex<()>,
    /// Serializes checkpoint admission BEFORE resolving current residents and
    /// snapshotting. Held only by blocking checkpoint work, never a turn. True
    /// closes periodic admission once shutdown starts; queued ticks cannot
    /// overwrite the acknowledged final checkpoint.
    checkpoint_stopped: Mutex<bool>,
    #[cfg(all(test, feature = "stress-tests"))]
    checkpoint_pause: Mutex<Option<CheckpointPause>>,
    /// Per-persona fork-template: a clone of the `PersonaBrainConfig` the live
    /// cycle was built from, retained so `cognition/eval` can fork an ephemeral
    /// measurement copy without touching the living persona (see
    /// [`fork_eval_cycle`](Self::fork_eval_cycle)). Cheap — every cfg field is a
    /// handle. Lock order is always `cycles` THEN `templates`, never the reverse,
    /// so the two can't deadlock.
    templates: Mutex<HashMap<Uuid, PersonaBrainConfig>>,
    /// Her MIND cycle, one per persona, built lazily from her template on her first turn in
    /// her mind room (PRIVACY-OF-THOUGHT.md §4). It owns its own working memory, so nothing
    /// crosses between her public and private turns except what she publishes. It is never in
    /// `cycles`, so the volatile checkpoint never writes it to disk in plaintext (until her
    /// sealed store exists, a restart forgets her private scratchpad, which is the honest cost).
    /// Lock order: `cycles`, then `templates`, then `mind_cycles`.
    mind_cycles: Mutex<HashMap<Uuid, Arc<WorkspaceCycle>>>,
}

struct ResidentWorkspace {
    cycle: Arc<WorkspaceCycle>,
    // Pure-cognition residents have no ActingBody but still have memory.
    working_memory: Arc<WorkingMemory>,
}

#[cfg(all(test, feature = "stress-tests"))]
struct CheckpointPause {
    admitted: tokio::sync::oneshot::Sender<()>,
    resume: std::sync::mpsc::Receiver<()>,
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
                    *persona_id,
                    root,
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
    /// Her working memory: the restore clears the receipt scope the rooting set.
    working_memory: Arc<crate::cognition::working_memory::WorkingMemory>,
    /// Which of her hands these are: `None` her live ones, `Some` an eval's or a fork's own
    /// (`ToolExecutor::hands_context`). Rooting and restore act on THESE hands only.
    context: Option<crate::code::hands::HandsContext>,
}

impl ActingHands {
    /// The hands of a cycle that has them; `None` for a pure-cognition (handless) cycle.
    pub(crate) fn of(cycle: &WorkspaceCycle) -> Option<Self> {
        cycle.acting().map(|a| Self {
            persona_id: a.persona_id,
            persona_name: a.persona_name.clone(),
            executor: a.executor.clone(),
            working_memory: a.working_memory.clone(),
            context: a.executor.hands_context(),
        })
    }

    /// The same persona's hands in `context`, a context these hands don't own (a card
    /// claim's): `None` when her executor can't act in a context.
    fn in_context(&self, context: crate::code::hands::HandsContext) -> Option<Self> {
        Some(Self {
            executor: self.executor.in_hands(self.persona_id, context)?,
            context: Some(context),
            ..self.clone()
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

/// WHERE HER HANDS STAND: the root a work turn rooted a set of her hands at (a card's
/// checkout), or nothing when they stand in her own workspace. Keyed by persona AND acting
/// context, so an eval's or a fork's place is its own and never overwrites her live one
/// (docs/architecture/HER-HANDS.md). Her live place (`None` context) is the one truth for the
/// workspace map she perceives and the scope her receipts carry; set at the rooting seam,
/// cleared on restore.
static ACTING_ROOTS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<ActingKey, ActingPlace>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// Whose place: the persona, and which of her hands (`None` = live).
type ActingKey = (uuid::Uuid, Option<crate::code::hands::HandsContext>);

/// Where she stands: the checkout root, and the card whose checkout it is when
/// a held card rooted her (None for a plain workspace_root).
#[derive(Clone, Debug)]
struct ActingPlace {
    root: std::path::PathBuf,
    card: Option<uuid::Uuid>,
}

/// The card-claim context her perception follows: the one her latest turn was bound to, or
/// none (her default hands, at home). Only WHICH hands she looks through changes; no hands'
/// place ever moves (card fc9e9fd7). Set by [`root_at_held_card`], cleared when that
/// claim's hands close.
static FOCUS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<uuid::Uuid, crate::code::hands::HandsContext>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

fn focus_of(persona_id: uuid::Uuid) -> Option<crate::code::hands::HandsContext> {
    FOCUS.lock().unwrap_or_else(|e| e.into_inner()).get(&persona_id).copied() // poisoned lock = read the last state, same policy as every lock in this crate
}

/// Point her perception at `context` (or home). The map she perceives re-renders when it moved.
fn set_focus(persona_id: uuid::Uuid, context: Option<crate::code::hands::HandsContext>) {
    let moved = {
        let mut map = FOCUS.lock().unwrap_or_else(|e| e.into_inner()); // poisoned lock = read the last state, same policy as every lock in this crate
        let before = match context {
            Some(c) => map.insert(persona_id, c),
            None => map.remove(&persona_id),
        };
        before != context
    };
    if moved {
        crate::persona::grounding_invalidation::mark_workspace_map_dirty(persona_id);
    }
    if context.is_none() {
        crate::cognition::handoff::note_unbound(persona_id);
    }
}

/// Where the hands she is focused on stand (what she perceives and what her receipts are
/// scoped to): her current card claim's checkout, or nothing when she is at home.
pub fn acting_root_of(persona_id: uuid::Uuid) -> Option<std::path::PathBuf> {
    acting_root_in(persona_id, focus_of(persona_id))
}

/// Where one set of her hands stands: her live ones (`None`) or a context's.
pub(crate) fn acting_root_in(persona_id: uuid::Uuid, context: Option<crate::code::hands::HandsContext>) -> Option<std::path::PathBuf> {
    ACTING_ROOTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&(persona_id, context))
        .map(|p| p.root.clone()) // poisoned lock = read the last state, same policy as every lock in this crate
}

/// The card her hands are rooted at, if a held card rooted them. Her work
/// receipts radiate into THAT card's activity room, not the room whose line
/// happened to trigger the turn (2026-09-04: every holder's 💭/⚙ lines landed
/// in #academy — the base room and the agents' coordination room — because the
/// trigger arrived there; the run room stayed empty for its reviewer).
pub fn acting_card_of(persona_id: uuid::Uuid) -> Option<uuid::Uuid> {
    acting_card_in(persona_id, focus_of(persona_id))
}

/// The card one set of her hands is rooted at: her live ones (`None`) or a context's.
pub(crate) fn acting_card_in(persona_id: uuid::Uuid, context: Option<crate::code::hands::HandsContext>) -> Option<uuid::Uuid> {
    ACTING_ROOTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&(persona_id, context))
        .and_then(|p| p.card) // poisoned lock = read the last state, same policy as every lock in this crate
}

fn note_acting_card_in(persona_id: uuid::Uuid, context: Option<crate::code::hands::HandsContext>, card: uuid::Uuid) {
    if let Some(place) = ACTING_ROOTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_mut(&(persona_id, context))
    {
        // poisoned lock = read the last state, same policy as every lock in this crate
        place.card = Some(card);
    }
}

fn note_acting_root_in(
    persona_id: uuid::Uuid,
    context: Option<crate::code::hands::HandsContext>,
    root: Option<std::path::PathBuf>,
) {
    {
        let mut map = ACTING_ROOTS.lock().unwrap_or_else(|e| e.into_inner()); // poisoned lock = read the last state, same policy as every lock in this crate
        match root {
            Some(r) => {
                map.insert(
                    (persona_id, context),
                    ActingPlace {
                        root: r,
                        card: None,
                    },
                );
            }
            None => {
                map.remove(&(persona_id, context));
            }
        }
    }
    // The map she perceives is the place of the hands she is focused on; an eval or a fork
    // placing its own hands changes nothing she perceives.
    let marked = context == focus_of(persona_id) && crate::persona::grounding_invalidation::mark_workspace_map_dirty(persona_id);
    continuum_base::probe!(
        class = "workspace.acting_root",
        persona_id = %persona_id,
        context = %context.map(|c| c.id().to_string()).unwrap_or_else(|| "live".to_string()), // unwrap_or_else: no context = her live hands, named
        root = %acting_root_in(persona_id, context).map(|p| p.display().to_string()).unwrap_or_default(),  // unwrap_or: nothing recorded / a pre-epoch clock = 0, never a guess
        map_marked_dirty = marked,
        "where this set of her hands stands now; her live map re-renders only when her live hands moved"
    );
}

/// What the board says became of a card she believed she held, from the ONE holder
/// projection the board itself renders (`card_holder`). PURE, so every branch is pinned:
/// a live lease held by `her` → `None` (the card IS hers — a renewal or re-claim landed
/// between the wake's two reads, nothing is gone); a live lease that is not hers →
/// `HeldBy`; a card past the claimable states → `MovedOn` (its life advanced, not a
/// lapse); otherwise `Lapsed` — the last lease, whoever's, ran out and nobody holds it.
/// `owner` is never read as authority: an expired claim still names its last holder, and
/// that holder may be her.
pub(crate) fn claim_gone_verdict(
    card: &airc_lib::WorkCard,
    now_ms: u64,
    her: uuid::Uuid,
) -> Option<crate::cognition::working_memory::ClaimGone> {
    use crate::cognition::working_memory::ClaimGone;
    use crate::persona::card_holder::{hold_of, refused_by_claim, Hold};
    if refused_by_claim(card.state) {
        return Some(ClaimGone::MovedOn {
            state: crate::modules::work::state_str(&card.state),
        });
    }
    let expired_at_ms = card.claim_expires_at_ms;
    Some(match hold_of(card, now_ms) {
        Hold::Held => match card.owner {
            Some(peer) if peer.as_uuid() == her => return None,
            Some(peer) => ClaimGone::HeldBy {
                peer: peer.as_uuid(),
                expires_at_ms: expired_at_ms,
            },
            // A live hold with no owner cannot be rendered by the board either; say
            // lapsed-and-open rather than invent a holder.
            None => ClaimGone::Lapsed { expired_at_ms },
        },
        Hold::Lapsed | Hold::Unclaimed => ClaimGone::Lapsed { expired_at_ms },
    })
}

/// Bind THIS turn to the hands of her held card's claim, standing in the card's staged
/// checkout — any turn, not only the work turn. Freya (2026-09-05) edited a file during a
/// room turn with her hands at home: her home is a copy of the continuum repo, and the edit
/// landed there, not in her card's checkout. While a citizen holds a card she works at that
/// repo. Returns the admitted claim hands for the turn's burst ([`crate::cognition::workspace::Burst::in_hands`]) and for
/// [`end_card_turn`]; nothing is restored after the turn, because nothing moved (card
/// fc9e9fd7).
pub(crate) async fn root_at_held_card(
    cycle: &WorkspaceCycle,
    peer_id: uuid::Uuid,
    conversation: &dyn crate::persona::service_loop::PersonaConversation,
) -> HeldCardTurn {
    let turn = bind_held_card(cycle, peer_id, conversation).await;
    // A turn bound to no claim's hands acts with her default hands, at home: her
    // perception follows the same hands the turn acts with.
    if turn.hands.is_none() {
        set_focus(peer_id, None);
    }
    turn
}

async fn bind_held_card(
    cycle: &WorkspaceCycle,
    peer_id: uuid::Uuid,
    conversation: &dyn crate::persona::service_loop::PersonaConversation,
) -> HeldCardTurn {
    // The card is UNKNOWN on each of these: there is no citizen, her claims could not be
    // read, or she holds nothing. Absent credit is the honest answer here — unlike the
    // workspace failures further down, where the card IS known.
    let Some(citizen) = conversation.stream_citizen() else {
        return HeldCardTurn::unheld();
    };
    let Ok(held) = citizen.active_claims().await else {
        return HeldCardTurn::unheld();
    };
    // THE WAKE'S FIRST READ OF THE BOARD answers what the checkpoint believed (card
    // c8303c32, Kimi 2026-09-21): her hands were rooted at a card when the snapshot was
    // written; if the board no longer lists it among her holds, she is told NOW — in her
    // window, before she resumes as its holder — instead of by a submit refusal hours on.
    // The verdict is the board's, through the SAME holder projection the board renders
    // (`card_holder`), never an owner field read as authority (Astra on #4315: an expired
    // claim still names its last holder). The belief is consumed only once the board has
    // ANSWERED; a card no board she stands in shows is UNKNOWN — said as such, and asked
    // again next turn — never "nobody holds it".
    if let Some(body) = cycle.acting() {
        if let Some(believed) = body.working_memory.peek_restored_acting_card() {
            if held.iter().any(|c| c.card_id.as_uuid() == believed) {
                // The board agrees with the checkpoint: nothing to say, nothing to keep.
                body.working_memory.take_restored_acting_card();
            } else {
                // The card's own row is a SECOND read; a renewal or re-claim can land
                // between it and `active_claims`, so the verdict is reconciled with HER
                // identity — a live hold that is hers is not gone (Astra, #4315 round 2).
                // Three outcomes: the row shows her holding it (settled, silent); the row
                // shows it gone (settled, said); no board she stands in shows the row
                // (unsettled — said as unknown, the belief kept for the next read).
                let now_ms = EpochMs::now().millis();
                let row = citizen.card(believed).await;
                let verdict = row.as_ref().map(|card| claim_gone_verdict(card, now_ms, peer_id));
                match verdict {
                    Some(None) => {
                        body.working_memory.take_restored_acting_card();
                    }
                    Some(Some(v)) => {
                        body.working_memory.take_restored_acting_card();
                        body.working_memory.note_claim_gone(believed, v);
                    }
                    None => body.working_memory.note_claim_gone(
                        believed,
                        crate::cognition::working_memory::ClaimGone::Unknown,
                    ),
                }
                if !matches!(verdict, Some(None)) {
                    continuum_base::probe!(
                        class = "persona.claim.gone_on_wake",
                        peer = %peer_id,
                        card = %believed,
                        verdict = ?verdict.flatten(),
                        "her checkpoint rooted her at a card the board no longer counts as hers — told in her window on the first held-card read"
                    );
                }
            }
        }
    }
    // The hands of every claim she no longer holds close, whether or not a card is
    // bound this turn.
    if let Some(live) = ActingHands::of(cycle) {
        let held_contexts: Vec<crate::code::hands::HandsContext> = held
            .iter()
            .filter_map(|c| c.claim_id.map(|claim| crate::code::hands::HandsContext::for_claim(peer_id, c.card_id.as_uuid(), claim.as_uuid())))
            .collect();
        close_ended_claims(&live, &held_contexts);
    }
    if held.is_empty() {
        return HeldCardTurn::unheld();
    }
    // ONE card per turn here too (`work_focus`): rooting on every held title
    // was ambiguous for a two-card holder, so her message turns kept her hands
    // at home while her work turns rooted (`persona.work.staged_ambiguous` ×2
    // after the focus cut, 2026-09-04).
    let Some(focus) = crate::persona::work_focus::focus_workspace_card(held.iter()) else {
        return HeldCardTurn::unheld();
    };
    bind_card(cycle, peer_id, focus).await
}

/// The activity this step perceives and acts IN: the board room of the card her hands are rooted on, by
/// the one definition of "which board holds a card" (`modules::work::room_holding_card`,
/// any activity, project or benchmark); else the room the turn was woken in. The wake room
/// is a fallback for acts with no subject yet (a help call before she has chosen a task),
/// never an override of one. It's the card of THESE hands (`context`): an eval's or a fork's
/// acts never inherit her live card's room (Astra's review of #5005).
pub(crate) async fn activity_room_for(
    persona_id: Uuid,
    context: Option<crate::code::hands::HandsContext>,
    wake_room: Uuid,
) -> Uuid {
    if crate::persona::mind_room::is_private_room(persona_id, wake_room) {
        return wake_room;
    }
    let Some(card) = acting_card_in(persona_id, context) else {
        return wake_room;
    };
    let Some(runtime) = crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()
        .and_then(|registry| registry.get(persona_id))
    else {
        return wake_room;
    };
    match crate::modules::work::room_holding_card(runtime.airc(), airc_lib::WorkCardId::from_uuid(card)).await {
        Some(room) => room.channel.as_uuid(),
        None => wake_room,
    }
}

/// Bind a turn to the hands of `card`'s claim (card fc9e9fd7): THE one binding both the
/// room turn ([`root_at_held_card`]) and the held-work turn use. Resolves the card's
/// checkout through the one owner of where a card's work lives
/// (`card_staging::checkout_path_for`), admits the turn to the claim's hands, roots them
/// once, points her perception at them and tells her where she stands when that changed.
pub(crate) async fn bind_card(
    cycle: &WorkspaceCycle,
    peer_id: uuid::Uuid,
    focus: &airc_lib::WorkCard,
) -> HeldCardTurn {
    // ── FROM HERE THE CARD IS KNOWN ───────────────────────────────────────────────
    // Everything that can still fail below is a fact about the WORKSPACE — no checkout
    // on this node, no acting body, a rooting error. NONE of them is a fact about which
    // card she is working, so none of them may erase the credit.
    //
    // Captured AT SELECTION and carried out BESIDE the hands rather than on them
    // (Astra/S6 on 799b8fe9). The first cut hung credit off `ActingHands`, so every
    // workspace failure below silently produced `credit: None` — and the training
    // producer reads `None` as "ordinary unlinked conversation" and SUBMITS IT
    // IMMEDIATELY as positive SFT, skipping the evidence floor. That is a pre-existing
    // rooting absence being reused as a different fact, which is the substrate defect
    // shape: one value, two meanings. A turn whose card is known stays STAGED even when
    // her hands never reached it.
    //
    // `from_selected_card` is PURE — it reads the card and invents nothing, so a
    // claimless card yields a credit with no receipt rather than a fabricated one.
    let credit = crate::persona::training_producer::CapturedCredit::from_selected_card(focus);
    // Resolve through the SAME authority that staged the card at claim time
    // (`card_staging::checkout_path_for`), which keys a generic repo card on its
    // CARD ID via airc's per-card worktree.
    //
    // This used to ask `staged_workspace::workspace_for_held_cards` for a match on
    // the card's DISPLAY TITLE — and every path in that module is rooted at
    // `staging_root(peer)`, the SWE-BENCH staging area. Card work does not live
    // there, so a generic repo card never matched, the resolver returned `None`,
    // and the `?` here dropped the whole function: her hands silently stayed at the
    // resident root while she believed she was working the card. Kimi's build.rs
    // edit (eb5a2606, 2026-09-08) landed in the resident workspace that way. The
    // title was the symptom; asking a benchmark-shaped resolver about an ordinary
    // repo card was the defect.
    crate::modules::card_staging::restage_held_linked_checkout(peer_id, focus).await;
    let Some(ws) = crate::modules::card_staging::checkout_path_for(&peer_id, focus) else {
        // NOT a silent `?`. The ambiguous case has always been probed; the
        // not-found case was the one that said nothing, which is why this went
        // unnoticed until a citizen's commit turned up in the wrong tree.
        continuum_base::probe!(
            class = "persona.work.checkout_unresolved",
            peer = %peer_id,
            card = %focus.card_id.as_uuid(),
            repo = %focus.repo,
            "held card has no checkout on this node — her hands stay at the resident \
             root for this turn, and anything she writes lands there, NOT in the card"
        );
        // Her hands stay home, but she is still working THIS card — the credit rides out.
        return HeldCardTurn::unrooted(credit, peer_id, focus.card_id.as_uuid());
    };
    // A pure-cognition persona has no hands at all. Still her card.
    let Some(live) = ActingHands::of(cycle) else {
        return HeldCardTurn::unrooted(credit, peer_id, focus.card_id.as_uuid());
    };
    // THE CLAIM'S OWN HANDS (card fc9e9fd7): derived from the claim, the same for every turn
    // of it and new for any other claim, rooted once at the card's checkout and never moved.
    // Her default hands stay home; no turn moves another's hands, so concurrent turns can't
    // split her shell from her code tools (Kimi #11697, card 2d58192d).
    let Some(claim) = focus.claim_id else {
        continuum_base::probe!(
            class = "persona.work.claim_unknown",
            peer = %peer_id,
            card = %focus.card_id.as_uuid(),
            "the focus card carries no claim id, so no claim's hands can be bound; nothing acts on it this turn"
        );
        return HeldCardTurn::unrooted(credit, peer_id, focus.card_id.as_uuid());
    };
    let context = crate::code::hands::HandsContext::for_claim(peer_id, focus.card_id.as_uuid(), claim.as_uuid());
    let Some(hands) = live.in_context(context) else {
        return HeldCardTurn::unrooted(credit, peer_id, focus.card_id.as_uuid());
    };
    let lease = match crate::code::hands::admit(context) {
        Ok(lease) => lease,
        Err(why) => {
            continuum_base::probe!(class = "persona.work.hands_closing", peer = %peer_id, card = %focus.card_id.as_uuid(), why = %why, "the claim's hands are closing; nothing acts on the card this turn");
            return HeldCardTurn::unrooted(credit, peer_id, focus.card_id.as_uuid());
        }
    };
    // Rooted ONCE: a context's place is fixed for its life, so a later turn finds it there.
    if acting_root_in(peer_id, Some(context)).as_deref() != Some(ws.as_path()) {
        if let Err(e) = drive_create_workspace(&hands, &ws.to_string_lossy(), &[], "bind-card-hands", false).await {
            tracing::warn!(error = %e, "could not root the card claim's hands at its checkout");
            // Nothing may act on the card, but the turn is still card-linked: it stages.
            return HeldCardTurn::unrooted(credit, peer_id, focus.card_id.as_uuid());
        }
        note_acting_root_in(peer_id, Some(context), Some(ws.clone()));
        note_acting_card_in(peer_id, Some(context), focus.card_id.as_uuid());
    }
    set_focus(peer_id, Some(context));
    // Both room and work turns stamp only after binding succeeds (81097858).
    crate::cognition::handoff::note_held(peer_id, None, focus, EpochMs::now().millis());
    if let Some(body) = cycle.acting() {
        body.working_memory.set_scope(Some(ws.to_string_lossy().to_string()));
        // SAY WHERE SHE NOW STANDS when it is not where she was last told (card 822c4253):
        // staging a card's checkout moved Kimi from the career-wrangler/ tree she had built
        // at home into a per-card worktree on a card branch, and her next code/read of
        // career-wrangler/docs/... failed with no word of why. Recorded as told only AFTER
        // the line is pinned (Cormac on #4555).
        if !already_told(peer_id, &ws) {
            let branch = crate::modules::card_staging::card_branch(focus);
            body.working_memory.pin_fact_for_turns("rooted", &rooted_notice(&focus.repo.to_string(), &ws, &branch), 3);
            mark_told(peer_id, &ws);
            continuum_base::probe!(
                class = "persona.work.root_moved",
                peer = %peer_id,
                card = %focus.card_id.as_uuid(),
                root = %ws.display(),
                branch = %branch,
                "she works a checkout she was not last told about; she is told now"
            );
        }
    }
    HeldCardTurn {
        credit: Some(credit),
        hands: Some(CardHands { context, hands, lease: std::sync::Arc::new(lease) }),
        unbound: None,
    }
}

/// The checkout each citizen was last TOLD her hands stand in (card 822c4253). Process
/// memory: after a restart she is told once more, which costs one pinned line.
static ANNOUNCED_ROOTS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<uuid::Uuid, std::path::PathBuf>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// Whether `root` is the checkout she was last told about. A per-turn rooting at the
/// same checkout says nothing.
fn already_told(persona_id: uuid::Uuid, root: &std::path::Path) -> bool {
    ANNOUNCED_ROOTS
        .lock()
        .unwrap_or_else(|e| e.into_inner()) // poisoned lock = read the last state, same policy as every lock in this crate
        .get(&persona_id)
        .is_some_and(|r| r == root)
}

/// Record `root` as told, once the line naming it is pinned in her window.
fn mark_told(persona_id: uuid::Uuid, root: &std::path::Path) {
    ANNOUNCED_ROOTS
        .lock()
        .unwrap_or_else(|e| e.into_inner()) // poisoned lock = read the last state, same policy as every lock in this crate
        .insert(persona_id, root.to_path_buf());
}

/// PURE: the line that tells her where her hands now stand and what did not move. The
/// branch is the card's EXPECTED branch (derived from the card), never asserted as the
/// checked-out HEAD, which an existing tree may have moved (Codex on #4555).
fn rooted_notice(repo: &str, root: &std::path::Path, branch: &str) -> String {
    format!(
        "[workspace] For your held card on {repo}, your hands now stand in its own checkout: \
         {root}. The card's branch is {branch}; git status shows what is actually checked \
         out. Paths in code/* are relative to it (docs/x.md, not <repo>/docs/x.md). Your \
         home workspace is unchanged: a copy of this repo you keep there is a SEPARATE tree, \
         and anything not pushed from it lives only there. Commits here stay on the checked \
         out branch until you push them where the project wants them.",
        root = root.display()
    )
}

/// What a turn knows about the card being worked, kept SEPARATE from whether her hands
/// reached it.
///
/// These are two different facts and collapsing them is what card 0d51573a's review
/// caught: a missing checkout, an absent acting body, or a rooting error all left the
/// credit `None`, and [`crate::persona::training_producer::produce`] reads `None` as
/// ordinary unlinked conversation — submitting the turn immediately as positive SFT
/// instead of staging it behind the evidence floor. A workspace failure must never be
/// readable as "she was not working a card".
pub(crate) struct HeldCardTurn {
    /// The card the work store selected for this turn, present whenever one was
    /// selected — regardless of what happened to the workspace afterwards.
    pub(crate) credit: Option<crate::persona::training_producer::CapturedCredit>,
    /// The hands of the card claim this turn was admitted to, standing in its checkout.
    /// `None` means nothing may act on the card this turn; it says nothing about which
    /// card it is.
    pub(crate) hands: Option<CardHands>,
    /// When the card is known but its hands could not be bound: the never-rooted context the
    /// turn acts with instead, so its code tools refuse rather than act at home.
    unbound: Option<crate::code::hands::HandsContext>,
}

/// A turn's admission to the hands of the card claim it works (card fc9e9fd7). The context
/// is derived from the claim, rooted ONCE at the card's checkout and never moved; the lease
/// holds the hands open until the turn ends, so closing them (the claim ended) waits the
/// turn out instead of dropping its engine or shell under it.
pub(crate) struct CardHands {
    pub(crate) context: crate::code::hands::HandsContext,
    hands: ActingHands,
    lease: std::sync::Arc<crate::code::hands::HandsLease>,
}

impl CardHands {
    /// Where these hands stand: the claim's checkout, rooted once.
    pub(crate) fn root(&self) -> Option<std::path::PathBuf> {
        acting_root_in(self.hands.persona_id, Some(self.context))
    }
}

/// The end of one turn on a card: the act's work is WIP-committed on the card's branch and
/// pushed (card 73eefbbb), so a node she is next staged on fetches it and a seat change waits
/// on it. Her hands stay where they are; nothing is restored, because nothing moved.
pub(crate) async fn end_card_turn(card: &CardHands, acts: usize) {
    let hands = &card.hands;
    // A turn that took no act changed nothing in the checkout: nothing to commit, nothing to
    // carry. Measured on the M5 (2026-10-16, lane gated at 6,144): every resident's turn
    // ended without reaching the model and the carry still ran git on each one, 374 ms
    // a time, 33 times per resident per 5 min, committed=false pushed=false throughout.
    if acts == 0 {
        continuum_base::probe!(
            class = "workspace.push.skipped_no_act",
            persona = %hands.persona_name,
            "turn took no act: the card's checkout is as it was, nothing to carry"
        );
        return;
    }
    if let (Some(root), Some(card_id)) = (acting_root_in(hands.persona_id, hands.context), acting_card_in(hands.persona_id, hands.context)) {
        // under the turn's own admission (see sync_after_act_for)
        crate::persona::workspace_transfer::sync_after_act_for(std::sync::Arc::clone(&card.lease), hands.persona_id, &hands.persona_name, root, card_id).await;
    }
    // Her other held cards' committed work rides the carry owner, not this card's turn.
    crate::modules::card_carry::request();
}

/// Close the hands of every card claim she no longer holds: their claims ended (released,
/// merged, lapsed, re-acquired as a new claim). Each closes in the background, refusing new
/// turns and waiting out admitted ones, then releases its engine and shells. A claim she
/// still holds keeps its hands.
fn close_ended_claims(live: &ActingHands, held: &[crate::code::hands::HandsContext]) {
    let ended: Vec<crate::code::hands::HandsContext> = ACTING_ROOTS
        .lock()
        .unwrap_or_else(|e| e.into_inner()) // poisoned lock = read the last state, same policy as every lock in this crate
        .iter()
        .filter(|((persona, context), place)| *persona == live.persona_id && place.card.is_some() && context.is_some_and(|c| !held.contains(&c)))
        .filter_map(|((_, context), _)| *context)
        .collect();
    for context in ended {
        let Some(hands) = live.in_context(context) else { continue };
        tokio::spawn(async move {
            // The release closes the hands to new work and waits out admitted work itself.
            let root = acting_root_in(hands.persona_id, Some(context));
            match close_acting_context(&hands).await {
                Ok(()) => {
                    if let Some(root) = root {
                        crate::persona::workspace_transfer::forget_checkout(&root);
                    }
                    if focus_of(hands.persona_id) == Some(context) {
                        set_focus(hands.persona_id, None);
                    }
                }
                Err(e) => tracing::warn!(error = %e, context = %context.id(), "a card claim's hands could not be released; they are kept closed for a retry"),
            }
        });
    }
}

impl HeldCardTurn {
    /// The hands this turn acts with: its card claim's, the card's never-rooted stand-in when
    /// those could not be bound, or her default hands when she works no card.
    pub(crate) fn turn_hands(&self) -> Option<crate::code::hands::HandsContext> {
        self.hands.as_ref().map(|h| h.context).or(self.unbound)
    }

    /// No card was selected: no citizen, unreadable claims, or nothing held.
    fn unheld() -> Self {
        Self {
            credit: None,
            hands: None,
            unbound: None,
        }
    }

    /// The card is known and her hands did not reach it.
    fn unrooted(credit: crate::persona::training_producer::CapturedCredit, persona: Uuid, card: Uuid) -> Self {
        Self {
            credit: Some(credit),
            hands: None,
            unbound: Some(crate::code::hands::HandsContext::unbound(persona, card)),
        }
    }
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
/// Callers root an acting context they opened (a fork's own hands) and end it with
/// [`end_acting_context`]; her own hands are never rooted here.
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
    // HER OWN HANDS NEVER MOVE (card fc9e9fd7): a measurement, solve or replay roots the
    // hands of the fork it opened, never hers.
    if hands.context.is_none() {
        return Err(crate::sdk_codegen::CommandError::Internal(
            "refusing to root her default hands: a measurement, solve or replay roots its own fork's hands".to_string(),
        ));
    }
    drive_create_workspace(
        &hands,
        root,
        path_prepend,
        "root-acting-workspace",
        refuse_inert_edits,
    )
    .await?;
    note_acting_root_in(hands.persona_id, hands.context, Some(std::path::PathBuf::from(root)));
    if let Some(body) = cycle.acting() {
        body.working_memory.set_scope(Some(root.to_string()));
    }
    continuum_base::probe!(
        class = "workspace.rooted",
        persona = %hands.persona_name,
        root = %root,
        "forked persona's file engine rooted at the target directory before her cycle"
    );
    Ok(())
}

/// End an acting context a measurement, a solve or a replay opened (its own hands, never
/// hers): the act's work is WIP-committed on its card's branch when a card rooted it, then
/// the context closes, awaited, before its caller removes the directory it stood in. Her own
/// hands are never moved by any of these, so there is nothing to put back (card fc9e9fd7:
/// the move-home this replaced is gone with the moves).
pub(crate) async fn end_acting_context(
    hands: &ActingHands,
) -> Result<(), crate::sdk_codegen::CommandError> {
    // THE WORKSPACE MOVES WITH THE MIND (card 73eefbbb): this is the one restore every
    // acting path goes through, and the last moment both facts still stand — WHERE her
    // hands acted and WHICH card rooted them. Before her hands leave the checkout, the
    // act's work is WIP-committed on the card's branch and pushed to origin, so a node she
    // is next staged on fetches it (`workspace_transfer::arrive`) and a seat change waits
    // on it (`move_blocker`). Bounded, probed, never the turn's failure. A root with no
    // card (an eval sandbox rooted directly) carries nothing: nothing to push.
    let Some(context) = hands.context else {
        return Err(crate::sdk_codegen::CommandError::Internal(
            "refusing to end her default hands: only an acting context a measurement, solve or replay opened ends".to_string(),
        ));
    };
    if let (Some(root), Some(card)) = (acting_root_in(hands.persona_id, hands.context), acting_card_in(hands.persona_id, hands.context)) {
        // Admitted to the context it syncs: the close below waits for the git work itself,
        // even when the sync's own timeout already returned (Astra on #5026). Hands already
        // closing admit nothing, and nothing runs in them unleased (Codex root on #5026): the
        // sync is skipped, named, and the work stays committed-or-not where it stands.
        match crate::code::hands::admit(context) {
            Ok(lease) => {
                crate::persona::workspace_transfer::sync_after_act_for(std::sync::Arc::new(lease), hands.persona_id, &hands.persona_name, root, card).await;
            }
            Err(why) => continuum_base::probe!(
                class = "workspace.sync_refused",
                persona = %hands.persona_name,
                card = %card,
                why = %why,
                "the acting context's hands are already closing, so its end-of-act sync does not run"
            ),
        }
    }
    close_acting_context(hands).await.map_err(Into::into)
}

/// Forget where an acting context stood: what a context's release on drop (the abnormal-exit
/// fallback) does once its release CONFIRMED, so no bookkeeping outlives it.
pub(crate) fn forget_acting_context(persona_id: Uuid, context: crate::code::hands::HandsContext) {
    note_acting_root_in(persona_id, Some(context), None);
}

/// Why an acting context is NOT closed: its handles and place are kept for a retry.
#[derive(Debug)]
pub(crate) struct ContextNotClosed {
    pub message: String,
    /// The pids not confirmed gone, or `None` when the release couldn't report them.
    pub unconfirmed: Option<Vec<u32>>,
}

impl ContextNotClosed {
    fn unknown(message: String) -> Self {
        Self { message, unconfirmed: None }
    }
}

impl std::fmt::Display for ContextNotClosed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<ContextNotClosed> for crate::sdk_codegen::CommandError {
    fn from(not_closed: ContextNotClosed) -> Self {
        crate::sdk_codegen::CommandError::Internal(not_closed.message)
    }
}

/// End an acting context and confirm it: `code/release-hands` through these hands (so it
/// ends THIS context and no other), awaited. Only a CONFIRMED release forgets where the
/// context stood; a release that couldn't confirm every kill keeps both the handles and the
/// record, and is returned as an error naming what's unresolved, so the caller keeps what
/// the context stood in and a retry finds it (Astra's review of #5005). This is the normal
/// teardown of an eval, a solve or a replay; the release sent when its hands drop is only
/// the fallback for an abnormal exit.
pub(crate) async fn close_acting_context(hands: &ActingHands) -> Result<(), ContextNotClosed> {
    let Some(context) = hands.context else {
        return Ok(()); // her live hands are never ended
    };
    // THE RELEASE IS THE KERNEL'S, run through the core as a RELEASE of these hands (never as
    // work admitted to them): it closes them to new work, waits out what is admitted, then
    // ends what still runs (Astra on #5026). Its typed result decides; nothing is clipped.
    let core = hands.executor.command_executor().ok_or_else(|| {
        ContextNotClosed::unknown(format!("closing acting context {}: these hands have no core to release through", context.id()))
    })?;
    let releasing = crate::routing::CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(hands.persona_id))
        .in_hands(context)
        .releasing_hands();
    let release: crate::modules::code_commands::ReleaseHandsResult = match core
        .execute_with_caller("code/release-hands", serde_json::json!({}), Some(releasing))
        .await
    {
        Ok(crate::runtime::CommandResult::Json(value)) => serde_json::from_value(value).map_err(|e| { // BOUNDARY: code/release-hands answers through the command surface's JSON result
            ContextNotClosed::unknown(format!("closing acting context {}: the release's result isn't its own shape ({e}), so nothing is confirmed", context.id()))
        })?,
        Ok(_) => return Err(ContextNotClosed::unknown(format!("closing acting context {}: the release answered in a shape other than its own JSON", context.id()))),
        Err(e) => return Err(ContextNotClosed::unknown(format!("closing acting context {}: {e}", context.id()))),
    };
    if !release.released {
        return Err(ContextNotClosed {
            message: format!(
                "acting context {} is NOT closed: pids {:?} not confirmed gone ({}); its handles and place are kept for a retry",
                context.id(),
                release.unconfirmed,
                release.errors.join("; ")
            ),
            unconfirmed: Some(release.unconfirmed),
        });
    }
    note_acting_root_in(hands.persona_id, hands.context, None);
    hands.working_memory.set_scope(None);
    continuum_base::probe!(
        class = "workspace.context_closed",
        persona = %hands.persona_name,
        context = %context.id(),
        "an acting context ended: its engine and shells released, its place forgotten"
    );
    Ok(())
}

impl PersonaWorkspaceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a persona's mind. `None` if it hasn't been registered/built yet.
    pub fn get(&self, persona_id: &Uuid) -> Option<Arc<WorkspaceCycle>> {
        self.cycles
            .lock()
            .get(persona_id)
            .map(|entry| Arc::clone(&entry.cycle))
    }

    /// The cycle a turn of `persona_id` in `room` runs on: her mind cycle in her mind room,
    /// her live cycle everywhere else. THE selection the service loop asks (one rule, two
    /// call sites: a message turn and a self/continuation turn).
    pub fn cycle_for_room(&self, persona_id: &Uuid, room: Uuid) -> Option<Arc<WorkspaceCycle>> {
        if crate::persona::mind_room::is_private_room(*persona_id, room) {
            self.mind_cycle(persona_id)
        } else {
            self.get(persona_id)
        }
    }

    /// Her mind cycle, built on first use from her template with its own working memory,
    /// no captures, and synchronous recall (a deferred recall worker would fold dispatch
    /// completions into it as well as her live memory). Before every use it mirrors her live
    /// cycle's genome, decoding and model binding, so a page-in or a re-home reaches her
    /// private turns at the next turn boundary. `None` when she has no live cycle or template.
    pub fn mind_cycle(&self, persona_id: &Uuid) -> Option<Arc<WorkspaceCycle>> {
        let live = self.get(persona_id)?;
        let cached = self.mind_cycles.lock().get(persona_id).cloned();
        let mind = match cached {
            Some(mind) => mind,
            None => {
                let _assembly = self.assembly.lock();
                // Bound to its own statement: a guard in an `if let` scrutinee would live into
                // the `else` and deadlock on the insert below (non-reentrant lock).
                let built_meanwhile = self.mind_cycles.lock().get(persona_id).cloned();
                if let Some(mind) = built_meanwhile {
                    mind
                } else {
                    let mut cfg = self.templates.lock().get(persona_id)?.clone();
                    cfg.defer_recall = false;
                    let (cycle, _working_memory) = assemble_workspace_cycle(cfg, None, Capture::Withheld);
                    let cycle = Arc::new(cycle);
                    self.mind_cycles.lock().insert(*persona_id, Arc::clone(&cycle));
                    cycle
                }
            }
        };
        mind.mirror_mind_from(&live);
        Some(mind)
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
    /// The ONE door from a caller's [`PersonaRef`] to a real [`PeerId`]. Taking the
    /// newtype rather than `&str` is what makes resolution unskippable: a param that
    /// holds a reference cannot reach a subsystem that wants an identity without
    /// coming through here.
    pub fn resolve_persona(&self, reference: &PersonaRef) -> Result<PeerId, String> {
        let id_or_name = reference.as_str();
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
        if let Ok(id) = continuum_base::id_resolve::resolve(id_or_name, &ids, "persona") {
            return Ok(PeerId::from_uuid(id));
        }

        // 2. Case-insensitive persona NAME.
        let want = id_or_name.trim();
        let name_matches: Vec<Uuid> = roster
            .iter()
            .filter(|(_, name)| name.eq_ignore_ascii_case(want))
            .map(|(id, _)| *id)
            .collect();
        match name_matches.as_slice() {
            [one] => Ok(PeerId::from_uuid(*one)),
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
    /// Checkpoint lock/read/schema failures refuse this registration and leave
    /// an existing resident unchanged. Call from a blocking worker during boot.
    pub fn register_from_cfg(
        &self,
        cfg: PersonaBrainConfig,
    ) -> std::io::Result<Arc<WorkspaceCycle>> {
        let _assembly = self.assembly.lock();
        let persona_id = cfg.persona_id;
        let template = cfg.clone();
        // Assembly restores disk state; do it outside the lookup lock.
        let restored = load_volatile(persona_id)?;
        let (cycle, working_memory) = assemble_workspace_cycle(cfg, restored, Capture::Recorded);
        let cycle = Arc::new(cycle);
        // cycles THEN templates (the one canonical lock order).
        let mut cycles = self.cycles.lock();
        self.templates.lock().insert(persona_id, template);
        cycles.insert(
            persona_id,
            ResidentWorkspace {
                cycle: Arc::clone(&cycle),
                working_memory,
            },
        );
        Ok(cycle)
    }

    /// Get the persona's mind, building + caching it from `cfg` on first access.
    /// Lazy-init so a persona's cycle is assembled once and reused across every
    /// room it services (the "one soul" invariant). Also retains the fork-template
    /// (same as [`register_from_cfg`](Self::register_from_cfg)).
    pub fn get_or_build(&self, cfg: PersonaBrainConfig) -> std::io::Result<Arc<WorkspaceCycle>> {
        let persona_id = cfg.persona_id;
        if let Some(existing) = self.get(&persona_id) {
            return Ok(existing);
        }
        let _assembly = self.assembly.lock();
        if let Some(existing) = self.get(&persona_id) {
            return Ok(existing);
        }
        let template = cfg.clone();
        let restored = load_volatile(persona_id)?;
        let (cycle, working_memory) = assemble_workspace_cycle(cfg, restored, Capture::Recorded);
        let cycle = Arc::new(cycle);
        // cycles THEN templates (the one canonical lock order).
        let mut cycles = self.cycles.lock();
        self.templates.lock().insert(persona_id, template);
        cycles.insert(
            persona_id,
            ResidentWorkspace {
                cycle: Arc::clone(&cycle),
                working_memory,
            },
        );
        Ok(cycle)
    }

    /// Periodic crash checkpoint. A busy writer coalesces this tick; no queue
    /// of blocking jobs accumulates behind slow storage.
    pub fn checkpoint_volatile_all(&self) -> Vec<(Uuid, std::io::Result<()>)> {
        let Some(stopped) = self.checkpoint_stopped.try_lock() else {
            return Vec::new();
        };
        if *stopped {
            return Vec::new();
        }
        self.write_volatile_all(SaveReason::Periodic)
    }

    /// Explicit save is repeatable and leaves periodic persistence running.
    pub fn flush_volatile_all(&self) -> Vec<(Uuid, std::io::Result<()>)> {
        let _checkpoint = self.checkpoint_stopped.lock();
        self.write_volatile_all(SaveReason::Seam)
    }

    /// Shutdown boundary: drain the admitted checkpoint, close periodic writes,
    /// then snapshot CURRENT residents. The caller must quiesce mutation first.
    /// Blocking work can outlive an async timeout; callers must receipt that as
    /// uncertain, never successful. No later periodic write can overtake this one.
    pub fn stop_volatile_checkpoints(&self) -> Vec<(Uuid, std::io::Result<()>)> {
        let mut stopped = self.checkpoint_stopped.lock();
        *stopped = true;
        self.write_volatile_all(SaveReason::Seam)
    }

    // Only called under checkpoint_stopped. The cycle lookup lock is released
    // before snapshot/serialization/IO, keeping room turns and roster reads free.
    fn write_volatile_all(&self, reason: SaveReason) -> Vec<(Uuid, std::io::Result<()>)> {
        // ONE staged-read budget for the whole seam, shared by every resident, so the save
        // phase is bounded by the phase and not by resident count; a periodic write has none.
        let staged_deadline = matches!(reason, SaveReason::Seam)
            .then(|| std::time::Instant::now() + crate::cognition::handoff::STAGED_READ_BOUND);
        let residents: Vec<_> = self
            .cycles
            .lock()
            .iter()
            .map(|(id, entry)| (*id, Arc::clone(&entry.working_memory)))
            .collect();
        #[cfg(all(test, feature = "stress-tests"))]
        {
            let pause = self.checkpoint_pause.lock().take();
            if let Some(pause) = pause {
                assert!(pause.admitted.send(()).is_ok(), "test awaits admission");
                assert!(
                    pause
                        .resume
                        .recv_timeout(std::time::Duration::from_secs(30))
                        .is_ok(),
                    "test releases checkpoint"
                );
            }
        }
        let save_one = |id: Uuid, memory: Arc<WorkingMemory>| {
            let started = std::time::Instant::now();
            let result = save_volatile(id, reason, staged_deadline, &memory);
            continuum_base::probe!(
                class = "persona.volatile.checkpoint",
                persona_id = %id,
                outcome = if result.is_ok() { "ok" } else { "error" },
                ms = started.elapsed().as_millis() as u64,
                "resident volatile checkpoint completed"
            );
            (id, result)
        };
        match reason {
            // A periodic checkpoint reads no tree and writes one small file per resident:
            // sequential on the checkpoint worker, no threads per tick.
            SaveReason::Periodic => residents
                .into_iter()
                .map(|(id, memory)| save_one(id, memory))
                .collect(),
            // THE SEAM RUNS EVERY RESIDENT AT ONCE (card 1e4d8b3b). Each resident's record
            // reads her tree (two bounded git children); sequential under one shared
            // deadline meant the first cold `git status` spent the budget and every later
            // resident wrote `Unmeasured` or, past the save phase, only a Periodic record
            // (M5, first deploy seam 2026-09-26 16:37Z: five Deploy records, two Periodic).
            // Fork/join like the runtime's own save phase: the seam's wall time is the
            // slowest resident, never the sum, and one deadline is enough for all.
            SaveReason::Seam => {
                let started = std::time::Instant::now();
                let count = residents.len();
                let save_one = &save_one;
                // A test's native-home fixture is thread-local; each seam thread carries it
                // (a fixture not carried is a real home written from a test).
                #[cfg(test)]
                let test_home = crate::paths::NativeHomeOverride::current();
                #[cfg(test)]
                let test_home = &test_home;
                let results: Vec<(Uuid, std::io::Result<()>)> = std::thread::scope(|scope| {
                    let handles: Vec<_> = residents
                        .into_iter()
                        .map(|(id, memory)| {
                            let handle = scope.spawn(move || {
                                #[cfg(test)]
                                let _home = test_home
                                    .as_deref()
                                    .map(crate::paths::NativeHomeOverride::install);
                                save_one(id, memory)
                            });
                            (id, handle)
                        })
                        .collect();
                    handles
                        .into_iter()
                        .map(|(id, handle)| {
                            handle.join().unwrap_or_else(|_| { // unwrap_or_else: a resident's save thread panicked — her record is the one thing lost, named; the seam goes on
                                (
                                    id,
                                    Err(std::io::Error::other(
                                        "the resident's seam checkpoint thread panicked",
                                    )),
                                )
                            })
                        })
                        .collect()
                });
                continuum_base::probe!(
                    class = "persona.volatile.seam",
                    residents = count,
                    failed = results.iter().filter(|(_, r)| r.is_err()).count(),
                    ms = started.elapsed().as_millis() as u64,
                    staged_budget_ms =
                        crate::cognition::handoff::STAGED_READ_BOUND.as_millis() as u64,
                    "every resident's seam record was written concurrently under one \
                     phase-derived deadline"
                );
                results
            }
        }
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
        } else {
            give_fork_its_own_hands(&mut cfg, *persona_id);
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
        } else {
            give_fork_its_own_hands(&mut cfg, *persona_id);
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
            .map(|(id, entry)| (*id, entry.cycle.acting().map(|b| b.persona_name.clone())))
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
    ) -> Option<(
        Arc<AdmissionState>,
        Arc<dyn AIProviderAdapter>,
        Option<String>,
    )> {
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
        // A citizen whose brain is assigned OFF-BOX keeps her remote binding: the
        // local lane coming up is not her model changing (card 1d2f65e7 slice 2).
        let root = crate::modules::persona_instance_manager::resolve_continuum_root();
        let remote: std::collections::HashSet<Uuid> = {
            let templates = self.templates.lock();
            templates
                .iter()
                .filter(|(_, cfg)| {
                    crate::persona::remote_lane_factory::is_remote_bound(&root, &cfg.persona_name)
                })
                .map(|(id, _)| *id)
                .collect()
        };
        let cycles = self.cycles.lock();
        let mut rehomed = 0usize;
        for (id, entry) in cycles.iter() {
            if remote.contains(id) {
                continuum_base::probe!(
                    class = "persona.rehome.kept_remote",
                    persona_id = %id,
                    "re-home skipped: her brain runs on a remote lane, the local model is not hers"
                );
                continue;
            }
            entry
                .cycle
                .rebind_model(super::llm_deliberation_faculty::ModelBinding {
                    adapter: Arc::clone(&adapter),
                    model: model.clone(),
                    context_window,
                });
            rehomed += 1;
        }
        rehomed
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
    own_speech: OwnSpeechPersisted,
}

/// Explicit offline adoption of a selected legacy checkpoint; shares this
/// owner's schema, native path, and cross-process checkpoint exclusion.
pub mod checkpoint_adoption;

/// Her own-speech rings on disk. Room-keyed since 2026-08-14 — a ring restored
/// without its room would re-create the cross-room repetition fact the keying
/// fix exists to kill.
///
/// `Legacy` is the pre-room-scoping shape (a flat `Vec<String>`) and exists so
/// an older `volatile.json` still PARSES: a torn mind-file that fails to
/// deserialize is a silent blank wake, which is strictly worse than a dropped
/// scratchpad. Legacy utterances carry no room, so they cannot be attributed
/// and are discarded on load — covered by `hydrate_speech_rings`, which
/// re-seeds from the durable transcript where every message HAS a room (#265).
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
enum OwnSpeechPersisted {
    ByRoom(Vec<(Uuid, Vec<String>)>),
    Legacy(Vec<String>),
}

impl OwnSpeechPersisted {
    /// Total utterances across every room — what the restore probe reports.
    fn len(&self) -> usize {
        match self {
            Self::ByRoom(by_room) => by_room.iter().map(|(_, u)| u.len()).sum(),
            Self::Legacy(flat) => flat.len(),
        }
    }
}

fn volatile_path(persona_id: Uuid) -> std::io::Result<std::path::PathBuf> {
    let home = crate::paths::home_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "cannot resolve persistent home directory",
        )
    })?;
    Ok(home
        .join(".continuum/personas")
        .join(persona_id.to_string())
        .join("volatile.json"))
}

pub use crate::cognition::handoff::SaveReason;

/// Persist the volatile tier — atomic tmp+rename so a crash mid-write never
/// leaves a torn file. Called under the registry checkpoint gate, on a blocking
/// worker. Failures propagate to both periodic and shutdown lifecycle receipts.
fn save_volatile(
    persona_id: Uuid,
    reason: SaveReason,
    staged_deadline: Option<std::time::Instant>,
    wm: &super::working_memory::WorkingMemory,
) -> std::io::Result<()> {
    let path = volatile_path(persona_id)?;
    let _checkpoint_lock = checkpoint_adoption::lock_checkpoint(&path, true)?;
    let mut snapshot = wm.snapshot();
    // THE HANDOFF (card 49b5e806): composed from what her turns already stamped (rooting,
    // work/note, the seam walk) plus the two things only the seam knows — what tears this
    // life, read from the deploy claim the reboot holds, and what stands on disk at her root.
    let now_ms = EpochMs::now().millis();
    let home = crate::paths::continuum_home().ok();
    let torn_by = crate::cognition::handoff::torn_by_for(reason, home.as_deref(), now_ms);
    let acting_root = acting_root_of(persona_id);
    snapshot.handoff = crate::cognition::handoff::compose(
        persona_id,
        torn_by,
        acting_root.as_deref(),
        staged_deadline,
        now_ms,
    );
    // What she believes she holds, stamped by the one writer that knows whose memory this
    // is: her next wake compares it to the board (card c8303c32). A belief the board has
    // NOT yet answered (restored, then `Unknown` on every read so far) outranks where her
    // hands stand now: the card she stands at is on the board and recoverable from it, the
    // pending comparison is recoverable from nowhere else — a checkpoint that wrote the
    // ambient root would settle it by forgetting (Astra on #4315, round 2).
    snapshot.acting_card = wm
        .peek_restored_acting_card()
        .or_else(|| acting_card_of(persona_id));
    let persisted = PersistedVolatile {
        wm: snapshot,
        own_speech: OwnSpeechPersisted::ByRoom(super::deliberation_budget::own_speech_by_room(
            crate::identity::PeerId::from_uuid(persona_id),
        )),
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    use std::io::Write;
    let mut file = std::fs::File::create(&tmp)?;
    file.write_all(&serde_json::to_vec(&persisted)?)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, &path)?;
    checkpoint_adoption::sync_parent(&path)
}

/// Load the previous life's volatile tier under the same owner lock as saves
/// and adoption. Only an absent checkpoint permits a fresh resident; inability
/// to read or decode an existing one must reach the registration caller.
/// The spill handles a persona's PERSISTED working memory still points at: its checkpoint's
/// results ring AND its collapsed pointers (each pointer is how she reopens a result that left
/// the ring, so it is a reference exactly as a ring entry is). Disk-pressure eviction protects these as well as the live minds' handles, so
/// a saved result survives the restart a deploy performs, a mind that has not restored yet,
/// and a persona that is offline (Astra on card 9197cd01). Cached per persona by the
/// checkpoint's modification time, so an eviction pass over many files reads each checkpoint
/// once and a new checkpoint is always seen. An unreadable checkpoint is an `Err`: the caller
/// fails closed and evicts nothing of that persona's.
pub(crate) fn persisted_result_handles(
    persona_id: Uuid,
) -> std::io::Result<std::sync::Arc<std::collections::HashSet<String>>> {
    result_handles_at(&volatile_path(persona_id)?)
}

/// [`persisted_result_handles`] for the checkpoint at `path` (the filesystem seam).
fn result_handles_at(
    path: &std::path::Path,
) -> std::io::Result<std::sync::Arc<std::collections::HashSet<String>>> {
    type Cached = (std::time::SystemTime, std::sync::Arc<std::collections::HashSet<String>>);
    static CACHE: std::sync::OnceLock<
        parking_lot::Mutex<std::collections::HashMap<std::path::PathBuf, Cached>>,
    > = std::sync::OnceLock::new();
    let modified = match std::fs::metadata(path) {
        Ok(meta) => meta.modified()?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Default::default());
        }
        Err(error) => return Err(error),
    };
    let cache = CACHE.get_or_init(Default::default);
    if let Some((at, handles)) = cache.lock().get(path) {
        if *at == modified {
            return Ok(std::sync::Arc::clone(handles));
        }
    }
    let handles: std::sync::Arc<std::collections::HashSet<String>> = std::sync::Arc::new(
        load_volatile_at(path)?
            .map(|persisted| {
                let wm = persisted.wm;
                crate::cognition::working_memory::reachable_handles(
                    &wm.recent_results,
                    &wm.evicted_pointers,
                    &wm.receipt_children,
                    &wm.open_views,
                )
            })
            .unwrap_or_default(),
    );
    cache.lock().insert(path.to_path_buf(), (modified, std::sync::Arc::clone(&handles)));
    Ok(handles)
}

fn load_volatile(persona_id: Uuid) -> std::io::Result<Option<PersistedVolatile>> {
    load_volatile_at(&volatile_path(persona_id)?)
}

fn load_volatile_at(path: &std::path::Path) -> std::io::Result<Option<PersistedVolatile>> {
    let _lock = checkpoint_adoption::lock_checkpoint(path, true)?;
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

// The higher-ring items this module's tests use, named ONCE here (they reach them through
// `use super::*`), so splitting the tests into their own file does not count the same upward
// dependencies a second time (layer_discipline counts per file).
#[cfg(test)]
use crate::{
    modules::{card_staging, code::{CodeModule, CodeState}},
    routing::{CallerIdentity, GridTrustAuthPolicy},
};
// The tests gate themselves (`#![cfg(test)]` in tests.rs), so the scanner reads the file as test code.
mod tests;
