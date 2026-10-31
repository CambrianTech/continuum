use super::*;

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
    /// [`resolve_recall_embedder`]: crate::ai::embedding::resolve_recall_embedder
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
    /// A measurement FORK of the mind (`fork_eval_cycle`): its turns warm their own KV
    /// page per room and never queue behind the living mind's act, since it acts beside
    /// her (`slots::PageKey::Fork`). `false` for every resident.
    pub forked: bool,
    /// Where her sent prompts' read receipts go: her airc source, which advances her room
    /// cursor through what each sent prompt rendered. `None` (a fork, an eval, a harness)
    /// marks nothing read.
    pub read_receipt: Option<Arc<dyn super::super::llm_deliberation_faculty::ReadReceipt>>,
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
pub(super) fn give_fork_its_own_hands(cfg: &mut PersonaBrainConfig, persona_id: Uuid) {
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
pub(super) enum Capture {
    Recorded,
    Withheld,
}

pub(super) fn assemble_workspace_cycle(
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
    // reasoning here after each verdict; her view's thoughts and actions sources READ it
    // into the next tick — so the persona resumes its train of
    // thought instead of re-deriving it cold. Volatile scratchpad, distinct from the
    // long-term engram store; self-activates only when thinking is enabled (suppressed
    // turns record nothing). Built HERE (before recall) so recall can share it in and
    // suppress an engram the recency channel already carries — see below.
    // Depth comes from the BUDGET, never a constant: the count sibling of the per-step size
    // bound, a ratio of two window fractions and so the same on every window: no live window is
    // read to build it. See `ContextBudget::WORKING_MEMORY_STEPS`.
    let working_memory = Arc::new(WorkingMemory::new(
        crate::cognition::context_budget::ContextBudget::WORKING_MEMORY_STEPS,
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
        // Results the old window share evicted carry no stored size: price each from its saved
        // output, so her view can show it whole again (FOCUS-CONTRACT rule 7).
        let persona = cfg.persona_id;
        persisted.wm.price_evicted_wholes(|handle| {
            let path = crate::cognition::tool_executor::spill::resolve(persona, handle).ok()?;
            std::fs::read_to_string(path).ok().map(|body| body.trim().chars().count())
        });
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
                        super::super::deliberation_budget::record_own_speech(peer, *room, utterance);
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
            super::super::dispatch_listener::spawn(bus, Arc::clone(&working_memory));
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
    // No per-source ceiling: each source delivers every item at its levels and her prompt
    // view's one allocation decides what she sees (FLOW-IMPLEMENTATION-PLAN §2d).
    for g in cfg.grounding_sources {
        let faculty = RagSourceFaculty::new(cfg.persona_id, g.source, g.policy)
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
    let genome = super::super::llm_deliberation_faculty::empty_genome();
    // The persona's decoding handle — shared (one ArcSwap, two holders) so the eval
    // window's greedy flip on the cycle is seen by the faculty's next generation,
    // exactly like the genome handle. `None` in live cognition (her lived warmth).
    let decoding = super::super::llm_deliberation_faculty::relaxed_decoding();
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
    let model_binding = super::super::llm_deliberation_faculty::model_binding(
        Arc::clone(&adapter),
        None,
        cfg.context_window,
    );
    let deliberation =
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
    let mut deliberation = if cfg.forked { deliberation.forked() } else { deliberation };
    // A fork copies the living config but never marks her rooms read: it acts beside her.
    if let Some(receipt) = cfg.read_receipt.clone().filter(|_| !cfg.forked) {
        deliberation = deliberation.with_read_receipt(receipt);
    }
    if tool_executor.is_some() {
        // Offer EXACTLY what this persona is authorized to run (offer ==
        // authorized) — never a tool the gate would refuse. A local persona is the
        // owner's own in-process agent: `LocalPersona` identity → `Trusted` at the
        // gate (per GridTrustAuthPolicy), so it is offered the Trusted surface —
        // AiSafe (file read/search) + Privileged (shell/git/write), but NOT the
        // Owner-gated ops (data/delete, grid/trust). Widening a tier auto-widens
        // this surface (no second list to keep in sync); offer matches the gate so
        // the persona is never shown a tool it can't actually run.
        deliberation = deliberation.with_tools(super::super::persona_tools::authorized_tool_specs(
            crate::routing::trust_level::TrustLevel::Trusted,
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
        match super::super::prompt_capture::JsonlPromptCaptureSink::open_for_persona(cfg.persona_id) {
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
        // Situation-aware focuser: every bid is forwarded, ordered by salience; how much
        // of each she sees is her view's one allocation.
        Arc::new(SituationFocusArbiter::new()),
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
            // Same buffer her view's sources read — the
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
        match super::super::workspace_capture::JsonlWorkspaceCaptureSink::open_for_persona(cfg.persona_id)
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
    pub(super) cycles: Mutex<HashMap<Uuid, ResidentWorkspace>>,
    /// Assembly opens captures and starts faculty wiring. Serialize constructors
    /// without holding the lookup lock, so a losing lazy build has no side effects.
    pub(super) assembly: Mutex<()>,
    /// Serializes checkpoint admission BEFORE resolving current residents and
    /// snapshotting. Held only by blocking checkpoint work, never a turn. True
    /// closes periodic admission once shutdown starts; queued ticks cannot
    /// overwrite the acknowledged final checkpoint.
    pub(super) checkpoint_stopped: Mutex<bool>,
    #[cfg(all(test, feature = "stress-tests"))]
    pub(super) checkpoint_pause: Mutex<Option<CheckpointPause>>,
    /// Per-persona fork-template: a clone of the `PersonaBrainConfig` the live
    /// cycle was built from, retained so `cognition/eval` can fork an ephemeral
    /// measurement copy without touching the living persona (see
    /// [`fork_eval_cycle`](Self::fork_eval_cycle)). Cheap — every cfg field is a
    /// handle. Lock order is always `cycles` THEN `templates`, never the reverse,
    /// so the two can't deadlock.
    pub(super) templates: Mutex<HashMap<Uuid, PersonaBrainConfig>>,
    /// Her MIND cycle, one per persona, built lazily from her template on her first turn in
    /// her mind room (PRIVACY-OF-THOUGHT.md §4). It owns its own working memory, so nothing
    /// crosses between her public and private turns except what she publishes. It is never in
    /// `cycles`, so the volatile checkpoint never writes it to disk in plaintext (until her
    /// sealed store exists, a restart forgets her private scratchpad, which is the honest cost).
    /// Lock order: `cycles`, then `templates`, then `mind_cycles`. Its working memory is held
    /// beside it so the memory owner accounts and sheds her private results too.
    pub(super) mind_cycles: Mutex<HashMap<Uuid, ResidentWorkspace>>,
}

pub(super) struct ResidentWorkspace {
    pub(super) cycle: Arc<WorkspaceCycle>,
    // Pure-cognition residents have no ActingBody but still have memory.
    pub(super) working_memory: Arc<WorkingMemory>,
}

#[cfg(all(test, feature = "stress-tests"))]
pub(super) struct CheckpointPause {
    pub(super) admitted: tokio::sync::oneshot::Sender<()>,
    pub(super) resume: std::sync::mpsc::Receiver<()>,
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
pub(super) fn repoint_workspace_map_if_pinned(
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
            // The map source is persona's (its layout readers); the slot builds it at `root`.
            let Some(pinned) =
                crate::cognition::extensions::pinned_workspace_map(*persona_id, std::path::Path::new(root))
            else {
                continue;
            };
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
                        crate::cognition::cached_source::CachedRagSource::new(pinned);
                    crate::cognition::grounding_invalidation::spawn_workspace_invalidator(
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
