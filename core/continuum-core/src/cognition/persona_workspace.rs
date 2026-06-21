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
use std::sync::{Arc, Mutex, OnceLock};

use uuid::Uuid;

use super::embedding::{CachingEmbeddingProvider, EmbeddingProvider, LexicalEmbedder};
use super::llm_deliberation_faculty::LlmDeliberationFaculty;
use super::rag_source_faculty::{RagSourceFaculty, SaliencePolicy};
use super::recall_faculty::RecallFaculty;
use super::workspace::{Faculty, SalienceArbiter, WorkspaceCycle};
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
    /// dynamic `AiSafe` tool surface ([`ai_safe_tool_specs`]) and routes the
    /// model's tool calls through this executor (a `CommandToolExecutor` carrying
    /// the persona's identity, so the `GridTrustAuthPolicy` ACL gates execution).
    /// `None` → speak-only (no tools offered) — the safe default for harnesses and
    /// for any persona whose spawn path hasn't built an executor.
    ///
    /// [`ai_safe_tool_specs`]: super::persona_tools::ai_safe_tool_specs
    pub tool_executor: Option<Arc<dyn crate::cognition::tool_executor::ToolExecutor>>,
}

/// A grounding [`RagSource`] plus the [`SaliencePolicy`] under which it competes
/// for attention once bridged into a faculty. Classified by whoever assembles the
/// cycle (the spawn path), keeping `RagSource` itself salience-free.
pub struct GroundingSource {
    pub source: Arc<dyn RagSource>,
    pub policy: SaliencePolicy,
}

impl GroundingSource {
    /// Standing framing (roster, doctrine) — always-present structural context.
    pub fn framing(source: Arc<dyn RagSource>) -> Self {
        Self {
            source,
            policy: SaliencePolicy::StandingFraming,
        }
    }

    /// Retrieved grounding (engram, conversation) — competes on relevance.
    pub fn retrieved(source: Arc<dyn RagSource>) -> Self {
        Self {
            source,
            policy: SaliencePolicy::Retrieved,
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
    let mut faculties: Vec<Arc<dyn Faculty>> = Vec::with_capacity(2 + cfg.grounding_sources.len());

    // Relevance recall ON by default. The embedder comes from the spawn path
    // (`resolve_recall_embedder`): neural when the embed model serves, lexical
    // otherwise — already wrapped in the process-global content-addressed cache
    // so a message is embedded ONCE and shared across every persona (never 14× for
    // 14 personas). `None` (harnesses) falls back to the lexical bootstrap, which
    // works on any machine with no model. Relevance > recency either way.
    let embedder = cfg.embedder.unwrap_or_else(|| {
        Arc::new(CachingEmbeddingProvider::new(Arc::new(LexicalEmbedder::new())))
    });
    faculties.push(Arc::new(
        RecallFaculty::new(cfg.persona_id, cfg.admission).with_embedder(embedder),
    ));

    // Bridge each grounding source into a perception-tier faculty under its
    // salience policy. Standing-framing (roster, doctrine) bids at a high floor so
    // the top-k arbiter never evicts the room's rules under attention pressure.
    for g in cfg.grounding_sources {
        faculties.push(Arc::new(RagSourceFaculty::new(
            cfg.persona_id,
            g.source,
            g.policy,
        )));
    }

    // The reasoner runs in phase 2 over everything the perception tier surfaced.
    // With a tool executor (the persona's HANDS), it's offered the dynamic AiSafe
    // tool surface and can ACT, not just speak — every tool call routed through
    // the persona's identity-bearing executor, so the ACL gates what it may do.
    // Without one, it's speak-only (the safe default). The tool SURFACE is the
    // single source of truth (`command_registry × AiSafe`), never hardcoded.
    let mut deliberation = LlmDeliberationFaculty::new(
        cfg.persona_id,
        cfg.persona_name,
        cfg.system_prompt,
        cfg.adapter,
    );
    if let Some(executor) = cfg.tool_executor {
        deliberation = deliberation
            .with_tools(super::persona_tools::ai_safe_tool_specs())
            .with_tool_executor(executor);
    }
    faculties.push(Arc::new(deliberation));

    let cycle = WorkspaceCycle::new(
        faculties,
        Arc::new(SalienceArbiter),
        cfg.capacity.unwrap_or(DEFAULT_WORKSPACE_CAPACITY),
    );

    // Make the LIVE brain observable: capture every tick's full competition (all
    // bids incl. losers, the assembled context the decider saw, the decision) to
    // a per-persona JSONL. The always-on recorder watches the legacy respond()
    // path; THIS is what instruments the path that actually runs. Best-effort —
    // if the fixtures dir can't be opened we log and run with Noop capture; a
    // persona's mind never fails to assemble over an observability hiccup.
    match std::env::var("HOME").map(|h| {
        std::path::Path::new(&h).join(".continuum/fixtures/workspace-traces")
    }) {
        Ok(dir) => match super::workspace_capture::JsonlWorkspaceCaptureSink::open(
            &dir,
            cfg.persona_id,
        ) {
            Ok(sink) => cycle.with_capture(Arc::new(sink)),
            Err(e) => {
                tracing::warn!(
                    persona_id = %cfg.persona_id,
                    error = %e,
                    "workspace trace capture unavailable; running with Noop capture"
                );
                cycle
            }
        },
        Err(_) => cycle, // HOME unset — opt-out, no capture (no warning spam)
    }
}

/// Persona-scoped registry of continuous minds. One `Arc<WorkspaceCycle>` per
/// persona; lookups by `persona_id`. `ai/should-respond` resolves the cycle here,
/// runs it over the room's consolidated burst, and reads the `Decision`.
#[derive(Default)]
pub struct PersonaWorkspaceRegistry {
    cycles: Mutex<HashMap<Uuid, Arc<WorkspaceCycle>>>,
}

impl PersonaWorkspaceRegistry {
    pub fn new() -> Self {
        Self {
            cycles: Mutex::new(HashMap::new()),
        }
    }

    /// Look up a persona's mind. `None` if it hasn't been registered/built yet.
    pub fn get(&self, persona_id: &Uuid) -> Option<Arc<WorkspaceCycle>> {
        self.cycles.lock().unwrap().get(persona_id).cloned()
    }

    /// Register a pre-built cycle for a persona (overwrites any existing).
    pub fn register(&self, persona_id: Uuid, cycle: Arc<WorkspaceCycle>) {
        self.cycles.lock().unwrap().insert(persona_id, cycle);
    }

    /// Get the persona's mind, building + caching it from `cfg` on first access.
    /// Lazy-init so a persona's cycle is assembled once and reused across every
    /// room it services (the "one soul" invariant).
    pub fn get_or_build(&self, cfg: PersonaBrainConfig) -> Arc<WorkspaceCycle> {
        let persona_id = cfg.persona_id;
        let mut cycles = self.cycles.lock().unwrap();
        if let Some(existing) = cycles.get(&persona_id) {
            return existing.clone();
        }
        let cycle = Arc::new(build_workspace_cycle(cfg));
        cycles.insert(persona_id, cycle.clone());
        cycle
    }

    /// How many persona minds are resident.
    pub fn len(&self) -> usize {
        self.cycles.lock().unwrap().len()
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
        let first = Arc::new(build_workspace_cycle(cfg_for(persona)));
        registry.register(persona, first.clone());
        let second = Arc::new(build_workspace_cycle(cfg_for(persona)));
        registry.register(persona, second.clone());
        let got = registry.get(&persona).expect("registered");
        assert!(
            Arc::ptr_eq(&got, &second),
            "respawn must resolve to the FRESH mind"
        );
        assert!(!Arc::ptr_eq(&got, &first), "the prior lifetime's mind is replaced");
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
        let adapter: Arc<dyn AIProviderAdapter> =
            Arc::new(LlamaCppAdapter::new().with_context_length(8192).with_n_seq_max(1));
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
        let faculties: Vec<Arc<dyn Faculty>> =
            vec![Arc::new(RecallFaculty::new(persona, admission)), delib.clone()];

        // The EXISTING capture harness: a WorkspaceCaptureSink records every phase
        // of the tick (all bids incl. losers, the assembled context, the decision)
        // so we can diagnose cognition at any phase — record + recreate, not guess.
        #[derive(Default)]
        struct CapturingSink(std::sync::Mutex<Vec<WorkspaceTrace>>);
        impl WorkspaceCaptureSink for CapturingSink {
            fn record(&self, t: &WorkspaceTrace) {
                self.0.lock().unwrap().push(t.clone());
            }
        }
        let sink = Arc::new(CapturingSink::default());

        let cycle =
            WorkspaceCycle::new(faculties, Arc::new(SalienceArbiter), DEFAULT_WORKSPACE_CAPACITY)
                .with_capture(sink.clone());

        let burst = "general room:\n\
            Joel: morning all\n\
            teammate: the deploy from yesterday — did we ever figure out what broke it?\n\
            teammate: ivar you were looking at it right?";

        let ws = cycle.run(burst).await;

        // ---- Glass box: diagnose cognition at EVERY phase ----
        let trace = sink.0.lock().unwrap().pop().expect("a tick was recorded");
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
            eprintln!("  [{}] {}", c.faculty.as_str(), c.content.replace('\n', " / "));
        }

        // ---- EXACTLY what the LLM was fed (reconstruct the pre-deliberation ws) ----
        let context_ws = Workspace {
            world_state: burst.to_string(),
            room_id: trace.room_id,
            broadcast: trace.context_broadcast.clone(),
        };
        let view = delib.prompt_view(&context_ws);
        eprintln!("\n--------------- WHAT THE LLM WAS FED ---------------");
        eprintln!("[SYSTEM]\n{}\n", view.system);
        eprintln!("[USER]\n{}", view.user);

        eprintln!("\n--------------- Ivar's DECISION ---------------");
        match ws.decision() {
            Some(Decision::Speak { text }) => eprintln!("Ivar SPEAKS:\n{text}"),
            Some(Decision::RaiseUnprompted { text }) => eprintln!("Ivar RAISES:\n{text}"),
            Some(Decision::Pass) | None => eprintln!("Ivar chose silence (PASS)."),
        }
        eprintln!("=================================================\n");

        assert!(
            ws.decision().is_some(),
            "the persona's mind must reach a decision through the real model"
        );
    }
}
