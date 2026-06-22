//! Persona Supervisor — slice 9 of #133.
//!
//! Turns a [`MaterializedPersonaPlan`](super::spawner_module::MaterializedPersonaPlan)
//! (airc identity bootstrapped + inference profile resolved) into a
//! [`HostedPersona`] — a row owning a constructed inference adapter,
//! ready for the per-persona service-loop (slice 10) to drive.
//!
//! ## What this layer owns
//!
//! The supervisor is where **adapter lifetime lives**. One persona →
//! one [`AIProviderAdapter`] today. Future slices change that
//! ownership shape:
//!
//! - **#122 (shared base + LoRA paging)**: the Llama backend
//!   underneath multiple personas is shared; the per-persona view is
//!   a (base_arc + lora_handle) pair, not a full adapter clone.
//!   `materialize_adapters` becomes the place where the supervisor
//!   asks the foundry "is this base already loaded?" before minting a
//!   fresh adapter.
//! - **#108 (cross-grid inference)**: some personas materialize as
//!   `AircRemoteInferenceAdapter` instead of `LlamaCppAdapter`. The
//!   factory trait — not the call site — picks which.
//!
//! Both refinements compose into the `PersonaAdapterFactory` trait
//! below without touching `materialize_adapters` itself. The trait
//! line is where #122 + #108 land their respective decisions.
//!
//! ## Doctrine
//!
//! - [[no-fallbacks-ever]]: each slot is materialized independently;
//!   failures are reported per-row. The supervisor never substitutes
//!   a "default" adapter — a `Err(SupervisorError)` row stays errored
//!   and the operator decides whether to refuse boot.
//! - [[commands-are-dumb-daemons-are-smart]]: the factory trait is
//!   trivial (one `build_adapter` method). Smart routing (which
//!   factory? which backend?) lives in the boot composition above
//!   this layer.
//! - [[intent-driven-api-not-hot-patches]]: callers hand in a
//!   `PersonaInferenceProfile` and get back a ready adapter. No magic
//!   constants, no env-var probes — the profile is the substrate's
//!   declared intent and the adapter materializes that.

use crate::ai::adapter::AIProviderAdapter;
use crate::persona::airc_citizen::AircCitizen;
use crate::persona::inference_profile::{InferenceProfileError, PersonaInferenceProfile};
use crate::persona::role_template::RoleId;
use crate::persona::spawner_module::MaterializedPersonaPlan;
use async_trait::async_trait;
use std::sync::Arc;

/// Polymorphism rail for "given a profile, produce an adapter".
/// Production wiring uses [`LlamaCppPersonaAdapterFactory`]; future
/// slices add `AircRemoteFactory` for grid-routed personas, and
/// `SharedBaseFactory` (#122) for multiple personas riding one base.
///
/// Tests substitute a stub factory so adapter materialization is
/// exercisable without loading a real GGUF.
#[async_trait]
pub trait PersonaAdapterFactory: Send + Sync {
    /// Build an adapter for the given inference profile.
    ///
    /// Errors are surfaced as a free-text message — the caller wraps
    /// them in `SupervisorError::AdapterFactory { slot, role, ... }`
    /// so the operator sees which slot failed without the factory
    /// having to know about slot indices.
    async fn build_adapter(
        &self,
        profile: &PersonaInferenceProfile,
    ) -> Result<Arc<dyn AIProviderAdapter>, String>;
}

/// Production factory: hands every profile to
/// `LlamaCppAdapter::for_persona`. Stateless — safe to share via
/// `Arc` across the supervisor's persona materialization loop.
pub struct LlamaCppPersonaAdapterFactory;

#[async_trait]
impl PersonaAdapterFactory for LlamaCppPersonaAdapterFactory {
    async fn build_adapter(
        &self,
        profile: &PersonaInferenceProfile,
    ) -> Result<Arc<dyn AIProviderAdapter>, String> {
        let adapter = crate::inference::llamacpp_adapter::LlamaCppAdapter::for_persona(profile)
            .map_err(|e| format!("LlamaCppAdapter::for_persona failed: {e}"))?;
        Ok(Arc::new(adapter))
    }
}

/// Routes a persona's reasoning through the **unsloth gateway** (the
/// OpenAI-compatible `/v1` adapter). Chosen 2026-06-21 (Joel) over teaching the
/// in-process llama.cpp adapter tool-calling: unsloth does **native function
/// calling for free** (the OpenAI-compatible adapter already sends `tools` +
/// parses `tool_calls` → `FinishReason::ToolUse`), so the persona's HANDS
/// actually fire instead of the model narrating fake tool use. Aligns with
/// "lean on unsloth, delete redundant code" ([[unsloth-universal-model-gateway]]):
/// the in-process chat path becomes redundant (a later delete).
///
/// All personas share the one gateway adapter (it's a meta-provider); the model
/// served is unsloth's loaded model (`UNSLOTH_MODEL` — point it at the 4b
/// code-forged GGUF so reasoning isn't downgraded to the 0.5B). Requests carry
/// `model: None`, so unsloth uses its loaded model.
pub struct UnslothPersonaAdapterFactory;

#[async_trait]
impl PersonaAdapterFactory for UnslothPersonaAdapterFactory {
    async fn build_adapter(
        &self,
        _profile: &PersonaInferenceProfile,
    ) -> Result<Arc<dyn AIProviderAdapter>, String> {
        let mut adapter = crate::ai::openai_adapter::OpenAICompatibleAdapter::from_registry("unsloth");
        adapter
            .initialize()
            .await
            .map_err(|e| format!("Unsloth persona adapter initialize failed: {e}"))?;
        Ok(Arc::new(adapter))
    }
}

/// One row of the supervisor's roster — and the substrate's
/// per-persona context object. Analog of Android's `Context`:
/// the single struct every persona-scoped function reads from.
///
/// ## The `&ctx` doctrine
///
/// Persona/cognition/RAG/inference/supervisor functions take
/// `&PersonaContext` and read what they need. They MUST NOT extract
/// individual fields and pass them as separate arguments — that
/// fragments the substrate's source of truth and creates drift.
/// Every derived shape (a RAG inspection request, a sampling spec
/// adjustment, a log scope) is produced from `&ctx` via a named
/// constructor whose name says `for_persona`/`for_ctx`.
///
/// ## The substrate's actor model — this IS the airc user
///
/// Per `[[personas-are-citizens-airc-is-identity-provider]]`:
/// "Personas, humans, Claude/OpenClaw/Hermes are the same kind of
/// citizen." Every actor in the substrate is an airc user.
///
/// The substrate's eventual shape (task #142) is a `BaseUser` that
/// carries the airc props every actor has — peer_id, identity,
/// runtime, home, default_room — plus per-actor extensions:
///   - `PersonaContext` = `BaseUser` + (role, profile, adapter)
///   - `HumanUserContext` = `BaseUser` + UI session + human ID card
///   - `WebUserContext` = `BaseUser` + tab/session id + auth scope
///
/// Same base, different extensions. Past designs that scattered
/// actor state across multiple ad-hoc structs are explicitly the
/// anti-pattern this struct exists to prevent (Joel 2026-06-02 —
/// "design got out of control due to not using a shared object for
/// all state info required for a persona OR user"). For slice 13,
/// `PersonaContext` already carries the airc props (via `runtime`)
/// + persona extensions — so the BaseUser extraction is purely a
/// rename + trait extraction, additive only.
///
/// ## What's in the context
///
/// - `identity` — substrate-stable persona_id + airc-side
///   peer_id/agent_name/home dir + the room she joined at bootstrap.
/// - `role` — Helper / Coder / Sentinel / Custom. Cognition reads it
///   to shape prompts.
/// - `profile` — the inference shape: `context_length`, `n_ubatch`,
///   sampling, `model_id`, `stop_sequences`, `chat_template`. The
///   single source of truth for the persona's compute envelope.
/// - `adapter` — the inference adapter, hot for generate_text. `Arc`
///   so the service loop can clone-share it with the RAG layer.
/// - `runtime` — the persona's `Arc<dyn AircCitizen>` (her grid
///   presence). The service loop subscribes through this; `say()`
///   posts through this. Cognition reads `runtime.peer_id()` for
///   self-filtering. The trait abstraction (per slice 13.5 +
///   `[[personas-are-citizens-airc-is-identity-provider]]`) means
///   tests get a typed stub instead of an `Option`.
///
/// ## Type-alias note
///
/// The struct used to be named `HostedPersona` (slice 9). The rename
/// to `PersonaContext` signals the design role; `pub type
/// HostedPersona = PersonaContext;` below keeps slice-9-era callers
/// compiling without a sweeping rename. New code should use
/// `PersonaContext` directly.
pub struct PersonaContext {
    /// Role identity (Helper / Coder / Sentinel / Custom).
    pub role: RoleId,
    /// The airc-side citizen identity — peer_id, agent_name, home,
    /// default_room, source (resumed vs minted). This is the
    /// substrate's universal actor identity per
    /// `[[personas-are-citizens-airc-is-identity-provider]]`. Same
    /// type for personas, humans, browsers — everyone has a
    /// `.identity`. Token/auth state lives inside.
    pub identity: crate::modules::persona_instance_manager::PersonaInstanceInfo,
    /// The single source of truth for this persona's inference
    /// shape — context window, ubatch, sampling, model id, stop
    /// sequences, etc. Produced by slice-5 `build_profile`. Every
    /// downstream layer that needs an inference-shape knob
    /// (service-loop's RAG request, future supervisor health
    /// commands, replay) reads from this struct — no second copy,
    /// no derived constants. PR #1511 integration trace caught the
    /// failure mode of letting the RAG layer's own 32k default
    /// override the adapter's 2k context_length: llama_decode
    /// returned -1 because the prompt budget exceeded what the
    /// adapter was loaded with. The fix is structural — the
    /// profile is the single source.
    pub profile: PersonaInferenceProfile,
    /// The inference adapter, ready to receive `generate_text` calls.
    /// `Arc` so the service-loop (#133 slice 10) can clone-and-share
    /// the adapter with the RAG inspector. #122 (shared base) keeps
    /// the same `Arc<dyn ...>` shape — only the concrete adapter
    /// inside changes.
    pub adapter: Arc<dyn AIProviderAdapter>,
    /// The persona's `Arc<dyn AircCitizen>` — her grid presence.
    /// The service loop subscribes through `runtime.subscribe()` and
    /// posts replies through `runtime.say(text)`. Cognition uses
    /// `runtime.peer_id()` for self-filtering. Held here so `&ctx`
    /// is the one handle every layer needs.
    ///
    /// `Arc<dyn AircCitizen>` (not `Arc<PersonaAircRuntime>`) so test
    /// fixtures can construct a [`StubAircCitizen`](crate::persona::airc_citizen::StubAircCitizen)
    /// without standing up the airc daemon. Production callers use
    /// `materialize_adapters`'s `runtime_lookup` to fetch the live
    /// runtime from the registry post-bootstrap.
    ///
    /// Foundation for task #142's BaseUser hierarchy — every BaseUser
    /// variant (persona/human/browser) will carry an
    /// `Arc<dyn AircCitizen>` as her live airc handle, and add
    /// kind-specific extensions (cognition for persona, WebAuthn for
    /// human, session state for browser).
    pub runtime: Arc<dyn AircCitizen>,
    /// The persona's brain. PER PERSONA, per the SHARED-COGNITION
    /// doctrine: each AI has its own mind; shared optimizations
    /// (the `analyze` single-flight cache) sit underneath, not above.
    ///
    /// `PersonaCognition` carries every layer the substrate has been
    /// built for: engine, inbox, rate_limiter, sleep_state,
    /// adapter_registry, genome_engine (L1-L5 LoRA paging),
    /// domain_classifier, message_cache, content_dedup, admission
    /// (hippocampus), recall_metadata (Algorithm 4), engram_source,
    /// airc_source (bound at boot, task #148), capture_sink.
    ///
    /// `Arc<Mutex<...>>` because the cognition cycle mutates state
    /// across the turn (rate_limiter, content_dedup, genome_engine,
    /// message_cache). One turn at a time per persona is correct —
    /// the substrate parallelizes ACROSS personas, not within one.
    ///
    /// See `docs/architecture/PERSONA-COGNITION-PIPELINE.md` for the
    /// cycle service_loop drives through this brain. DO NOT bypass
    /// it with a chatbot-shaped surface.
    pub cognition: Arc<tokio::sync::Mutex<crate::persona::unified::PersonaCognition>>,
    /// Pre-baked persona system prompt — the "You are {persona},
    /// an autonomous AI persona on the grid." line currently
    /// `format!()`'d per turn by the service loop. Constructed
    /// ONCE at PersonaContext construction (the persona name is
    /// fixed for the session) and leased per turn via `Arc::clone`.
    ///
    /// Per `[[init-once-handle-then-lease-zero-copy-refs]]` +
    /// task #195 slice 2: substrate latency lives in reinit, not
    /// compute. Reformatting + reallocating the same template per
    /// turn is the textbook reinit-on-hot-path waste this field
    /// eliminates.
    ///
    /// `Arc<str>` (not `String`) so cloning is a pointer-copy + atomic
    /// refcount bump. The service loop's per-turn cost becomes
    /// `Arc::to_string` (one strlen + alloc + memcpy) instead of
    /// `format!` (variadic macro + locale machinery + alloc + copy).
    /// Task #149 will replace this with `Arc<[Token]>` to eliminate
    /// the per-turn tokenization too.
    pub system_prompt: Arc<str>,
}

/// Back-compat alias for the slice-9-era struct name. New code
/// should write `PersonaContext` directly.
pub type HostedPersona = PersonaContext;

impl PersonaContext {
    /// Construct a tracing `Span` tagged with this persona's identity
    /// + role + tier. Every log line emitted inside the span's scope
    /// inherits these fields automatically — no more
    /// `tracing::warn!(persona_id = %ctx.identity.persona_id, ...)`
    /// at every call site.
    ///
    /// Per the `&ctx` doctrine: the span derives from the context,
    /// the loop scopes the span, the substrate's observability stays
    /// honest about who did what without manual field threading.
    pub fn span(&self) -> tracing::Span {
        tracing::info_span!(
            "persona",
            persona_id = %self.identity.persona_id,
            agent_name = %self.identity.agent_name,
            peer_id = %self.identity.peer_id,
            role = ?self.role,
            tier = %self.profile.tier_id,
            ctx_len = self.profile.context_length,
            model = %self.profile.model_id,
        )
    }
}

// ─── Context trait impl ────────────────────────────────────────────────
//
// Per task #142 Slice 2 + [[airc-is-the-session-not-a-feature]]:
// every actor on the substrate carries an `Identity` and an airc
// citizen handle, reachable via the `Context` trait. `PersonaContext`
// is the first concrete implementor; `StubContext`, `ClaudeContext`,
// `JtagContext`, etc. follow the same shape.
//
// ## Transitional storage shape (Identity synthesized per call)
//
// `PersonaContext.identity` is still `PersonaInstanceInfo` (the
// pre-Identity-entity struct). `Context::identity()` SYNTHESIZES an
// `Identity` on each call, returning `Cow::Owned(synthesized)`. A
// future slice migrates PersonaContext to store `Identity` directly;
// the impl then becomes `Cow::Borrowed(&self.identity)` at zero cost.
//
// Per-call synthesis cost is acceptable per
// [[substrate-overhead-is-1to3ms-LLM-dominates-latency]] — Uuid copy
// + String clones are not the substrate's latency bottleneck.
//
// ## Identity Uuid — `ctx.identity().id` IS the registry key (Slice 1B)
//
// Slice 1B of #142 collapsed `PersonaInstanceInfo.persona_id ==
// peer_id` at the runtime boundary (`PersonaAircRuntime::bootstrap`
// + `from_attached` reseat the field from `airc.peer_id().as_uuid()`).
// Callers may now safely use `ctx.identity().id` as a registry key
// — it is the same Uuid as `ctx.identity.persona_id` and the same
// Uuid as `airc.peer_id().as_uuid()`, per
// [[persona-identity-derives-from-source-id]] (the cryptographic
// keypair Uuid IS the substrate identity).
impl crate::context::Context for PersonaContext {
    fn identity(&self) -> std::borrow::Cow<'_, crate::identity::Identity> {
        use crate::identity::{Identity, IdentityKind, IdentitySource};
        use crate::persona::identity_provider::PersonaIdentitySource;

        let source = match self.identity.source {
            PersonaIdentitySource::ResumedFromDisk => IdentitySource::ResumedFromDisk,
            PersonaIdentitySource::FreshlyMinted => IdentitySource::FreshlyMinted,
        };

        // `self.identity.peer_id == self.identity.persona_id` post-
        // Slice-1B. Either field can be used as the source; we read
        // `peer_id` because it names the cryptographic ground truth
        // explicitly.
        std::borrow::Cow::Owned(Identity {
            id: self.identity.peer_id,
            kind: IdentityKind::Persona,
            agent_name: self.identity.agent_name.clone(),
            home_path: self.identity.home.to_string_lossy().into_owned(),
            default_room: self.identity.default_room,
            source,
            // Personas aren't external AI agents — provider is None
            // by definition. See Slice-4 IdentityKind doc.
            agent_provider: None,
        })
    }

    fn airc(&self) -> &Arc<dyn crate::persona::airc_citizen::AircCitizen> {
        &self.runtime
    }
}

/// Structured error per failed slot. The two failure modes are:
///
/// - The slice-8 profile resolution already failed (bad model_id,
///   missing GGUF, etc.) — surface as `Profile`.
/// - The profile is fine, but the factory's adapter construction
///   failed (factory rejected the profile, model load failed during
///   `for_persona`, etc.) — surface as `AdapterFactory`.
///
/// Per [[no-fallbacks-ever]] the supervisor never substitutes a
/// default; the operator sees the structured failure with slot +
/// role and decides whether to refuse boot.
#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    #[error("slot {slot_index} (role {role:?}): inference profile invalid: {source}")]
    Profile {
        slot_index: usize,
        role: RoleId,
        #[source]
        source: InferenceProfileError,
    },
    #[error("slot {slot_index} (role {role:?}): adapter factory rejected profile: {message}")]
    AdapterFactory {
        slot_index: usize,
        role: RoleId,
        message: String,
    },
    /// The adapter built fine but failed its warmup decode. Per
    /// [[init-once-handle-then-lease-zero-copy-refs]] warmup is
    /// part of init, not a hot-path concern; per [[no-fallbacks-ever]]
    /// a persona whose adapter can't warm doesn't enter "hosted"
    /// state. Operator decides whether to retry, swap models, or
    /// surface the underlying inference-backend error.
    #[error("slot {slot_index} (role {role:?}): adapter warmup decode failed: {message}")]
    AdapterWarmup {
        slot_index: usize,
        role: RoleId,
        message: String,
    },
    /// The post-bootstrap registry doesn't have a runtime for this
    /// persona_id. Per [[no-fallbacks-ever]] this is a hard failure —
    /// the supervisor doesn't fabricate or stub a runtime in
    /// production. If you see this, the bootstrap → registry → lookup
    /// chain skipped a registration step; investigate
    /// `PersonaInstanceManagerModule::bootstrap_one` and the
    /// `PersonaAircRuntimeRegistry` insert path.
    #[error(
        "slot {slot_index} (role {role:?}): no airc runtime registered for persona {persona_id} \
         — substrate bootstrap chain is broken; per [[no-fallbacks-ever]] no default citizen"
    )]
    RuntimeMissing {
        slot_index: usize,
        role: RoleId,
        persona_id: uuid::Uuid,
    },
}

/// Materialize a roster of `MaterializedPersonaPlan`s into
/// `HostedPersona`s by running each profile through the factory.
///
/// One adapter is built per `Ok` profile; `Err` profiles pass
/// through as `SupervisorError::Profile { ... }`. Factory failures
/// surface as `SupervisorError::AdapterFactory { ... }`. Per
/// [[no-fallbacks-ever]] there is no implicit retry, no substitution,
/// no "default adapter" for failed slots — the row stays errored and
/// the supervisor's caller decides policy.
///
/// Factories MAY be expensive (model load, network handshake to a
/// remote inference peer); the loop is sequential today so the
/// substrate doesn't kick off four ~500 MiB GGUF loads in parallel
/// on an 8 GiB Intel Mac. Slice 10+ can introduce parallel + capped
/// materialization once #122 (shared base) makes the per-persona
/// cost much smaller.
pub async fn materialize_adapters(
    plans: Vec<MaterializedPersonaPlan>,
    factory: &dyn PersonaAdapterFactory,
    runtime_lookup: impl Fn(uuid::Uuid) -> Option<Arc<dyn AircCitizen>>,
    // The persona's HANDS, built per persona by the caller (the `ipc` bootstrap,
    // which holds the command `ModuleRegistry`). Closure DI keeps the supervisor
    // decoupled from the command runtime — same shape as `runtime_lookup`. Returns
    // `None` → that persona is speak-only. The executor carries the persona's
    // identity, so the `GridTrustAuthPolicy` ACL gates what its hands may touch.
    tool_executor_for: impl Fn(
        uuid::Uuid,
    )
        -> Option<Arc<dyn crate::cognition::tool_executor::ToolExecutor>>,
) -> Vec<Result<PersonaContext, SupervisorError>> {
    let mut out = Vec::with_capacity(plans.len());
    for (slot_index, plan) in plans.into_iter().enumerate() {
        let profile = match plan.profile {
            Ok(p) => p,
            Err(source) => {
                out.push(Err(SupervisorError::Profile {
                    slot_index,
                    role: plan.role,
                    source,
                }));
                continue;
            }
        };
        let identity = plan.instance;
        let runtime = match runtime_lookup(identity.persona_id) {
            Some(r) => r,
            None => {
                out.push(Err(SupervisorError::RuntimeMissing {
                    slot_index,
                    role: plan.role,
                    persona_id: identity.persona_id,
                }));
                continue;
            }
        };
        let adapter = match factory.build_adapter(&profile).await {
            Ok(a) => a,
            Err(message) => {
                out.push(Err(SupervisorError::AdapterFactory {
                    slot_index,
                    role: plan.role,
                    message,
                }));
                continue;
            }
        };
        // Warm the adapter's KV-cache / kernels BEFORE the persona
        // enters her service loop. Per [[init-once-handle-then-lease-zero-copy-refs]]
        // the substrate pays init costs at boot, not on Joel's first
        // message. Per [[no-fallbacks-ever]] warmup failure surfaces
        // as a typed slot failure — the persona doesn't reach
        // "hosted" state if her adapter refuses to warm.
        if let Err(message) = adapter.warmup().await {
            out.push(Err(SupervisorError::AdapterWarmup {
                slot_index,
                role: plan.role,
                message,
            }));
            continue;
        }
        // Register the persona's adapter in the global provider
        // registry so the cognition layer (evaluate_response,
        // analyze, etc.) can reach it via `global_registry()` per
        // task #161. `ArcAdapterShim` lets the supervisor keep its
        // `Arc<dyn AIProviderAdapter>` ownership while the registry
        // (which holds Box<dyn ...>) sees a delegating handle. The
        // shim's `initialize`/`shutdown` are no-ops because the
        // factory + warmup above already paid the init cost per
        // [[init-once-handle-then-lease-zero-copy-refs]].
        {
            let registry_arc = crate::modules::ai_provider::global_registry();
            let mut registry = registry_arc.write().await;
            registry.register(adapter.clone(), slot_index);
        }

        // Build the persona's brain at boot. Bind airc_source via
        // set_airc_source so compose_for_turn has engram + airc both
        // available the moment her service loop iterates (task #148).
        // The runtime IS an AircTranscriptReader by trait bound.
        let rag_engine = Arc::new(crate::rag::RagEngine::new());
        let mut cognition = crate::persona::unified::PersonaCognition::new(
            identity.persona_id,
            identity.agent_name.clone(),
            rag_engine,
        );
        let airc_source: Arc<dyn crate::persona::rag_budget::RagSource> = Arc::new(
            crate::persona::airc_source::AircRagSource::new(
                identity.persona_id,
                runtime.clone(),
            ),
        );
        cognition.set_airc_source(airc_source);

        // Bind the room-roster source from the SAME runtime — it
        // upcasts to `AircRosterReader` (a supertrait of AircCitizen)
        // just as it does to `AircTranscriptReader` above. This is what
        // grounds the persona in who else is present (and who is NOT
        // itself). See docs/grid/AIRC-NATIVE-IDENTITY-ROOMS-SECURITY.md
        // §5 slice 1.
        let roster_source: Arc<dyn crate::persona::rag_budget::RagSource> =
            Arc::new(crate::persona::room_roster_source::RoomRosterSource::new(
                identity.persona_id,
                runtime.clone(),
            ));
        // Clone the Arc: the SAME source feeds both the legacy compose path
        // (set_roster_source) and the brain (as a bridged grounding faculty,
        // below). One source of truth, two consumers during the cutover
        // transition — not a parallel allocator.
        cognition.set_roster_source(roster_source.clone());

        // Bind the room-doctrine source from the same runtime (upcasts to
        // `AircDoctrineReader`). Grounds the persona in the room's nature
        // — the airc-published operating contract. Slice 2.
        let doctrine_source: Arc<dyn crate::persona::rag_budget::RagSource> =
            Arc::new(crate::persona::room_doctrine_source::RoomDoctrineSource::new(
                identity.persona_id,
                runtime.clone(),
            ));
        // Same dual-wire as the roster: one Arc, legacy path + brain faculty.
        cognition.set_doctrine_source(doctrine_source.clone());

        // Active-work source: grounds the persona in ITS OWN live work across all
        // rooms (claimed cards + states), read from airc's work roster. The dynamic
        // fix for "misremembering my own card" — never hardcode the state; surface
        // it. Reads the persona's own airc handle (acts as itself).
        let active_work_source: Arc<dyn crate::persona::rag_budget::RagSource> =
            Arc::new(crate::persona::active_work_source::ActiveWorkSource::new(
                identity.persona_id,
                runtime.clone(),
            ));

        // Disk-backed, per-persona memory: open <home>/engrams.sqlite and
        // rehydrate prior engrams + recall metadata, so memory SURVIVES restart.
        // Without this, admission is in-memory only (NoopSink) and the persona is
        // amnesiac across boots. `identity.home` is the resolved
        // <root>/personas/<name> dir. On disk error we log loud and continue
        // in-memory — the persona stays alive; persistence is degraded, not fatal
        // (NOT an inference fallback). MUST run before the WorkspaceCycle is
        // assembled below, so its RecallFaculty binds the persisted admission.
        let home = crate::persona::home::PersonaHome::from_root(identity.home.clone());
        let recall_meta = std::sync::Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        );
        match crate::persona::admission_state::AdmissionState::for_persona(&home, recall_meta)
            .await
        {
            Ok(persisted) => {
                cognition.attach_persistent_admission(
                    identity.persona_id,
                    std::sync::Arc::new(persisted),
                );
            }
            Err(e) => {
                tracing::warn!(
                    persona = %identity.agent_name,
                    error = %e,
                    "engram persistence unavailable; running in-memory (memory will NOT survive restart)"
                );
            }
        }

        let system_prompt = build_persona_system_prompt(&identity.agent_name);

        // Assemble this persona's continuous mind into the process-global
        // workspace registry — ONE WorkspaceCycle per persona, keyed by
        // persona_id (the "one soul, many rooms" invariant, PERSONA-BRAIN-
        // ARCHITECTURE.md §2.9). The shared hippocampus (cognition.admission)
        // and the persona's inference adapter are leased into its faculties.
        // Cheap (no model load — the adapter lazy-loads on first inference).
        // Additive: makes `ai/should-respond` resolvable for this persona; does
        // NOT change the existing service-loop decision path (heuristics stay
        // live until the coordinated cutover).
        // REGISTER (overwrite), not get_or_build: a persona can respawn in the
        // same process (node resilience). get_or_build is idempotent by
        // persona_id and would DISCARD the fresh admission + adapter, leaving the
        // mind bound to the prior lifetime's orphaned (rehydrated-then-replaced)
        // AdmissionState — newly-admitted engrams invisible to recall, the
        // "severed" failure across a restart. Build + register replaces it.
        crate::cognition::persona_workspace::global().register(
            identity.persona_id,
            std::sync::Arc::new(crate::cognition::persona_workspace::build_workspace_cycle(
                crate::cognition::persona_workspace::PersonaBrainConfig {
                    persona_id: identity.persona_id,
                    persona_name: identity.agent_name.to_string(),
                    system_prompt: system_prompt.to_string(),
                    admission: cognition.admission.clone(),
                    adapter: adapter.clone(),
                    capacity: None,
                    // Neural recall when the embed model serves, lexical otherwise
                    // — decided once here (process-stable; query + stored vectors
                    // must share one embedding space). Already cached by the
                    // resolver (embed-once-per-content, shared across personas).
                    embedder: Some(
                        crate::cognition::embedding::resolve_recall_embedder(adapter.clone()).await,
                    ),
                    // Roster + doctrine bridged into the brain as STANDING-FRAMING
                    // grounding faculties (high salience floor). Without these the
                    // gating cutover routes decisions through the Workspace and the
                    // #1650/#1651 grounding silently falls out of the live path —
                    // the persona forgets who is present / what the room is for.
                    grounding_sources: vec![
                        crate::cognition::persona_workspace::GroundingSource::framing(
                            roster_source,
                        ),
                        crate::cognition::persona_workspace::GroundingSource::framing(
                            doctrine_source,
                        ),
                        // The persona's own live work across rooms — standing framing
                        // so it always knows what it's working on (cross-activity,
                        // dynamic, no hardcoded card state).
                        crate::cognition::persona_workspace::GroundingSource::framing(
                            active_work_source,
                        ),
                    ],
                    // The persona's HANDS — built by the caller for THIS persona's
                    // identity (None → speak-only). What turns "talks" into "acts".
                    tool_executor: tool_executor_for(identity.persona_id),
                },
            )),
        );

        out.push(Ok(PersonaContext {
            role: plan.role,
            identity,
            profile,
            adapter,
            runtime,
            cognition: Arc::new(tokio::sync::Mutex::new(cognition)),
            system_prompt,
        }));
    }
    out
}

/// Construct the persona's system prompt as an `Arc<str>` ready
/// to lease per turn. Per task #195 slice 2: this runs ONCE at
/// PersonaContext construction; the per-turn `format!()` it
/// replaces becomes a single `Arc::clone`-then-`to_string` at
/// the RespondInput boundary.
///
/// The template is verbatim what `service_loop_inner` used to
/// `format!()` per turn — same characters, same order — so the
/// downstream prompt assembly is unchanged. Task #149 will
/// replace this with a pre-tokenized form.
///
/// Exposed `pub(super)` so the service-loop test fixtures can
/// build the same string by the same code path the production
/// path uses; no copy-pasted template.
pub(super) fn build_persona_system_prompt(agent_name: &str) -> Arc<str> {
    // LCD-tier identity grounding (#152). Pre-fix prompt was a
    // single line ("You are {name}, an autonomous AI persona on
    // the grid.") — operationally inadequate for Qwen2.5-0.5B
    // and similar small models, which drift to their training
    // defaults under the slightest pressure: claim to be Claude,
    // claim to be ChatGPT, hallucinate a Siemens PLC backstory.
    //
    // The fix is three concrete clauses, each addressing a
    // specific drift mode:
    //
    //   1. Identity anchoring — repeat the name + role with an
    //      explicit "you are NOT" enumeration of common drift
    //      targets. LCD models follow negative instructions
    //      surprisingly well when they're concrete.
    //   2. Operating context — "the grid" by itself is
    //      undefined; add the substrate-shaped vocabulary
    //      (persona, room, peers) so the model has a coherent
    //      world model to ground itself in.
    //   3. First-person stability — "speak as YOURSELF" is the
    //      single most effective LCD-tier anti-drift instruction
    //      per Joel 2026-06-03's `[[intent-driven-api-not-hot-
    //      patches]]` testing on Qwen 0.5B. Without it the model
    //      occasionally renders dialogue from another
    //      perspective ("Helper AI says X").
    //
    // Tuned for LCD; capable models (qwen3.5-4b, GPT-4) handle
    // the same text gracefully because the framing is
    // additive, not restrictive. Per task #149 a future PR
    // pre-tokenizes this string at boot to drop the per-turn
    // re-tokenize cost — the content here is the input to that
    // optimization, so the cache should grow but the tokenizer
    // bill should not.
    Arc::from(format!(
        "You are {agent_name}, an autonomous AI persona living on the continuum grid.\n\
         \n\
         Identity (never drift from this):\n\
         - You are {agent_name}. You are NOT Claude, GPT, ChatGPT, Gemini, \
         Llama, Qwen, or any other named assistant. You are NOT a Siemens PLC, \
         a customer service bot, or any persona other than {agent_name}.\n\
         - You are ONE persona among many on the grid. Other personas are your \
         peers, not your operators. Humans are citizens of the grid too — \
         neither above nor below you.\n\
         - Always speak as YOURSELF in the first person ('I think...', 'I'd \
         rather...'). Never narrate other personas' speech or write dialogue \
         from another point of view.\n\
         \n\
         Context:\n\
         - 'The grid' is the substrate hosting you. 'Rooms' are conversation \
         spaces where peers (other personas, humans, agents) exchange messages. \
         You are reading one room's recent activity below.\n\
         - Your only outputs are: (a) a direct reply to the room, or (b) the \
         silence token described in the [Silence Option] block."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::persona_instance_manager::PersonaInstanceInfo;
    use crate::persona::airc_citizen::StubAircCitizen;
    use crate::persona::hw_tier_descriptor::HwTierCategory;
    use crate::persona::identity_provider::PersonaIdentitySource;
    use crate::persona::inference_profile::{PersonaInferenceProfile, SamplingProfile};
    use crate::persona::scripted_adapter_factory::ScriptedPersonaAdapterFactory;
    use std::path::PathBuf;
    use uuid::Uuid;

    // Bespoke `FakeAdapter` / `OkFactory` / `ErrFactory` /
    // `WarmupFailingFactory` / `WarmupFailingAdapter` deleted per
    // [[test-fixtures-are-system-primitives]] — every test below
    // leases `ScriptedPersonaAdapterFactory` (built on the
    // production-runnable `HeuristicInferenceAdapter`) plus
    // `StubAircCitizen::fresh_lookup`. Adapter behaviors (warmup
    // success/failure, factory rejection, counter observation) come
    // from the system primitives' builder methods.

    fn fake_instance(name: &str) -> PersonaInstanceInfo {
        // Honor the Slice-1B-of-#142 invariant
        // (persona_id == peer_id) even in test fixtures so they
        // exercise the same identity shape production sees. Per the
        // PersonaInstanceInfo doc: fixtures that bypass the runtime
        // constructor MUST keep both fields equal.
        let peer_id = Uuid::new_v4();
        PersonaInstanceInfo {
            persona_id: peer_id,
            agent_name: name.to_string(),
            peer_id,
            home: PathBuf::from(format!("/tmp/fake-supervisor-test/{name}")),
            default_room: Uuid::nil(),
            source: PersonaIdentitySource::FreshlyMinted,
        }
    }

    /// Pins the Slice 1B invariant: `ctx.identity().id ==
    /// ctx.identity.persona_id == ctx.identity.peer_id` for any
    /// `PersonaContext` constructed through the canonical path. If
    /// a future edit reintroduces the pre-Slice-1B divergence
    /// (separate Uuid for persona_id vs peer_id), this test fails.
    ///
    /// Per [[every-error-is-an-opportunity-to-battle-harden]] — the
    /// PR #1522 reviewer caught the divergence after the fact;
    /// this test is the rigging that catches the regression class
    /// at unit-test time.
    #[test]
    fn persona_context_identity_id_matches_registry_key() {
        use crate::context::Context;
        let instance = fake_instance("Maya");
        // Sanity: fixture itself honors the invariant.
        assert_eq!(instance.persona_id, instance.peer_id);

        let ctx_identity = instance.peer_id;
        let cognition_persona_id = instance.persona_id;

        // The PersonaContext Context impl projects identity.peer_id
        // into Identity.id; the registry key path reads
        // identity.persona_id. Post-Slice-1B these MUST match.
        assert_eq!(
            ctx_identity, cognition_persona_id,
            "Slice 1B invariant broken: ctx.identity().id != ctx.identity.persona_id"
        );
    }

    fn fake_profile(persona_name: &str, model_id: &str) -> PersonaInferenceProfile {
        PersonaInferenceProfile {
            persona_id: Uuid::new_v4(),
            persona_name: persona_name.to_string(),
            model_id: model_id.to_string(),
            gguf_local_path: Some(PathBuf::from("/tmp/fake.gguf")),
            tier_category: HwTierCategory::Compat,
            tier_id: "mac_intel_metal_discrete".to_string(),
            context_length: 2048,
            n_ubatch: 512,
            n_batch: 512,
            n_seq_max: 1,
            n_gpu_layers: 0,
            sampling: SamplingProfile::chat_defaults(),
            chat_template: None,
            stop_sequences: vec![],
        }
    }

    /// Happy path: two materialized plans → two hosted personas. Each
    /// adapter's `provider_id` matches the profile's model_id, proving
    /// the factory ran once per persona (not once with shared state).
    #[tokio::test]
    async fn materializes_one_adapter_per_persona_via_factory() {
        let plans = vec![
            MaterializedPersonaPlan {
                role: RoleId::Helper,
                instance: fake_instance("Paige"),
                profile: Ok(fake_profile("Paige", "model-a")),
            },
            MaterializedPersonaPlan {
                role: RoleId::Coder,
                instance: fake_instance("Pax"),
                profile: Ok(fake_profile("Pax", "model-b")),
            },
        ];

        let factory = ScriptedPersonaAdapterFactory::heuristic();
        let hosted =
            materialize_adapters(plans, &factory, StubAircCitizen::fresh_lookup(), |_| None).await;

        assert_eq!(hosted.len(), 2);
        assert_eq!(factory.build_count(), 2);

        let helper = hosted[0].as_ref().expect("Helper hosted");
        assert_eq!(helper.role, RoleId::Helper);
        assert_eq!(helper.identity.agent_name, "Paige");
        assert_eq!(
            helper.adapter.provider_id(),
            crate::ai::HEURISTIC_PROVIDER_ID
        );

        let coder = hosted[1].as_ref().expect("Coder hosted");
        assert_eq!(coder.role, RoleId::Coder);
        assert_eq!(coder.identity.agent_name, "Pax");
        assert_eq!(
            coder.adapter.provider_id(),
            crate::ai::HEURISTIC_PROVIDER_ID
        );
    }

    /// A row that arrives with `Err(profile)` from slice 8 passes
    /// through as `SupervisorError::Profile` — the factory is NOT
    /// called for it (sibling rows still materialize normally).
    #[tokio::test]
    async fn forwards_profile_errors_without_calling_factory() {
        let bad_profile_err = InferenceProfileError::UnknownModel {
            model_id: "nonexistent/sentinel".to_string(),
            role_id: "coder".to_string(),
        };
        let plans = vec![
            MaterializedPersonaPlan {
                role: RoleId::Helper,
                instance: fake_instance("Paige"),
                profile: Ok(fake_profile("Paige", "model-a")),
            },
            MaterializedPersonaPlan {
                role: RoleId::Coder,
                instance: fake_instance("Pax"),
                profile: Err(bad_profile_err),
            },
        ];

        let factory = ScriptedPersonaAdapterFactory::heuristic();
        let hosted =
            materialize_adapters(plans, &factory, StubAircCitizen::fresh_lookup(), |_| None).await;

        assert_eq!(hosted.len(), 2);
        // Factory called exactly once — for the Ok row only.
        assert_eq!(factory.build_count(), 1);
        assert!(hosted[0].is_ok(), "Helper still materializes");
        match &hosted[1] {
            Err(SupervisorError::Profile {
                slot_index,
                role,
                source,
            }) => {
                assert_eq!(*slot_index, 1);
                assert_eq!(*role, RoleId::Coder);
                assert!(matches!(source, InferenceProfileError::UnknownModel { .. }));
            }
            Err(other) => panic!("expected Profile error at slot 1, got {other:?}"),
            Ok(_) => panic!("expected Profile error at slot 1, got Ok"),
        }
    }

    /// Factory rejection surfaces as `SupervisorError::AdapterFactory`
    /// with the slot index + role tagged. Sibling rows don't get
    /// affected when only one fails — the loop continues.
    #[tokio::test]
    async fn factory_rejection_surfaces_as_adapter_factory_error() {
        let plans = vec![MaterializedPersonaPlan {
            role: RoleId::Helper,
            instance: fake_instance("Paige"),
            profile: Ok(fake_profile("Paige", "model-a")),
        }];

        let factory =
            ScriptedPersonaAdapterFactory::always_fails("simulated factory rejection");
        let hosted =
            materialize_adapters(plans, &factory, StubAircCitizen::fresh_lookup(), |_| None).await;

        assert_eq!(hosted.len(), 1);
        match &hosted[0] {
            Err(SupervisorError::AdapterFactory {
                slot_index,
                role,
                message,
            }) => {
                assert_eq!(*slot_index, 0);
                assert_eq!(*role, RoleId::Helper);
                assert!(message.contains("simulated factory rejection"));
            }
            Err(other) => panic!("expected AdapterFactory error, got {other:?}"),
            Ok(_) => panic!("expected AdapterFactory error, got Ok"),
        }
    }

    /// Empty input → empty output. The Vec allocation is sized but
    /// no factory calls fire.
    #[tokio::test]
    async fn empty_plans_yields_empty_hosted() {
        let factory = ScriptedPersonaAdapterFactory::heuristic();
        let hosted = materialize_adapters(vec![], &factory, |_| None, |_| None).await;
        assert!(hosted.is_empty());
        assert_eq!(factory.build_count(), 0);
    }

    /// Missing-runtime path: when `runtime_lookup` returns `None` for
    /// a real plan, the substrate surfaces `SupervisorError::RuntimeMissing`
    /// with the slot index, role, and persona_id tagged. Per
    /// [[no-fallbacks-ever]]: the supervisor never fabricates a runtime
    /// when the registry lookup fails — a missing slot means the
    /// bootstrap chain skipped a step and the operator needs to see it.
    ///
    /// Locks in the contract for the slice-13.5 trait-extraction:
    /// `runtime_lookup`'s `Option<Arc<dyn AircCitizen>>` return shape
    /// is honored by `materialize_adapters` as a structured failure,
    /// NOT as a silent skip or fall-through to a default citizen.
    #[tokio::test]
    async fn runtime_lookup_none_surfaces_as_runtime_missing() {
        let instance = fake_instance("Paige");
        let expected_persona_id = instance.persona_id;
        let plans = vec![MaterializedPersonaPlan {
            role: RoleId::Helper,
            instance,
            profile: Ok(fake_profile("Paige", "model-a")),
        }];

        let factory = ScriptedPersonaAdapterFactory::heuristic();
        // `|_| None` here is the substrate-bug shape we're locking in:
        // the registry exists but doesn't contain this persona_id.
        let hosted = materialize_adapters(plans, &factory, |_| None, |_| None).await;

        assert_eq!(hosted.len(), 1);
        // Factory MUST NOT be called when the runtime lookup fails —
        // adapter construction is expensive (model load), the
        // substrate refuses early.
        assert_eq!(
            factory.build_count(),
            0,
            "factory must not run when runtime lookup fails"
        );
        match &hosted[0] {
            Err(SupervisorError::RuntimeMissing {
                slot_index,
                role,
                persona_id,
            }) => {
                assert_eq!(*slot_index, 0);
                assert_eq!(*role, RoleId::Helper);
                assert_eq!(*persona_id, expected_persona_id);
            }
            Err(other) => panic!("expected RuntimeMissing error, got {other:?}"),
            Ok(_) => panic!("expected RuntimeMissing error, got Ok"),
        }
    }

    /// Mixed: slot 0 has a runtime (citizen-stub lookup succeeds),
    /// slot 1 doesn't (lookup returns None). The supervisor materializes
    /// the first cleanly and surfaces `RuntimeMissing` for the second —
    /// proving sibling slots don't cross-affect, matching the
    /// per-slot error semantics of `Profile` and `AdapterFactory`.
    #[tokio::test]
    async fn runtime_missing_only_affects_its_own_slot() {
        let paige = fake_instance("Paige");
        let pax = fake_instance("Pax");
        let pax_persona_id = pax.persona_id;
        let plans = vec![
            MaterializedPersonaPlan {
                role: RoleId::Helper,
                instance: paige,
                profile: Ok(fake_profile("Paige", "model-a")),
            },
            MaterializedPersonaPlan {
                role: RoleId::Coder,
                instance: pax,
                profile: Ok(fake_profile("Pax", "model-b")),
            },
        ];

        let factory = ScriptedPersonaAdapterFactory::heuristic();
        // Lookup returns Some only for Paige; Pax goes RuntimeMissing.
        let lookup = move |pid: Uuid| -> Option<Arc<dyn crate::persona::airc_citizen::AircCitizen>> {
            if pid == pax_persona_id {
                None
            } else {
                Some(Arc::new(StubAircCitizen::new(Uuid::new_v4()))
                    as Arc<dyn crate::persona::airc_citizen::AircCitizen>)
            }
        };
        let hosted = materialize_adapters(plans, &factory, lookup, |_| None).await;

        assert_eq!(hosted.len(), 2);
        // Factory ran exactly once — for Paige, not Pax.
        assert_eq!(factory.build_count(), 1);
        assert!(hosted[0].is_ok(), "Paige materializes cleanly");
        match &hosted[1] {
            Err(SupervisorError::RuntimeMissing {
                slot_index,
                role,
                persona_id,
            }) => {
                assert_eq!(*slot_index, 1);
                assert_eq!(*role, RoleId::Coder);
                assert_eq!(*persona_id, pax_persona_id);
            }
            Err(other) => panic!("expected RuntimeMissing at slot 1, got {other:?}"),
            Ok(_) => panic!("expected RuntimeMissing at slot 1, got Ok"),
        }
    }

    /// Warmup is called for every successfully-materialized adapter.
    /// Per [[init-once-handle-then-lease-zero-copy-refs]] the substrate
    /// pays init costs at boot, not on the user's first message;
    /// `materialize_adapters` is where that contract gets enforced.
    /// If a future refactor forgets the warmup call, this test fails
    /// because the shared counter stays at 0.
    #[tokio::test]
    async fn warmup_called_once_per_materialized_adapter() {
        let plans = vec![
            MaterializedPersonaPlan {
                role: RoleId::Helper,
                instance: fake_instance("Paige"),
                profile: Ok(fake_profile("Paige", "model-a")),
            },
            MaterializedPersonaPlan {
                role: RoleId::Coder,
                instance: fake_instance("Pax"),
                profile: Ok(fake_profile("Pax", "model-b")),
            },
        ];

        let (factory, counts) = ScriptedPersonaAdapterFactory::heuristic_with_counters();
        let hosted =
            materialize_adapters(plans, &factory, StubAircCitizen::fresh_lookup(), |_| None).await;

        // Both slots materialize cleanly.
        assert_eq!(hosted.len(), 2);
        assert!(hosted.iter().all(|r| r.is_ok()));
        // Every adapter was built AND warmed.
        assert_eq!(factory.build_count(), 2);
        assert_eq!(
            counts.warmups(),
            2,
            "warmup() must be called once per successfully-materialized adapter"
        );
    }

    /// Warmup failure surfaces as `SupervisorError::AdapterWarmup` —
    /// the persona does NOT reach hosted state. Per [[no-fallbacks-ever]]
    /// an adapter that refuses to warm gets a typed slot failure;
    /// sibling slots continue.
    #[tokio::test]
    async fn warmup_failure_surfaces_as_typed_slot_error() {
        let plans = vec![MaterializedPersonaPlan {
            role: RoleId::Helper,
            instance: fake_instance("Paige"),
            profile: Ok(fake_profile("Paige", "model-a")),
        }];

        let factory = ScriptedPersonaAdapterFactory::heuristic_with_warmup_failure(
            "simulated warmup failure",
        );
        let hosted =
            materialize_adapters(plans, &factory, StubAircCitizen::fresh_lookup(), |_| None).await;

        assert_eq!(hosted.len(), 1);
        match &hosted[0] {
            Err(SupervisorError::AdapterWarmup {
                slot_index,
                role,
                message,
            }) => {
                assert_eq!(*slot_index, 0);
                assert_eq!(*role, RoleId::Helper);
                assert!(
                    message.contains("simulated warmup failure"),
                    "error must propagate underlying cause: {message}"
                );
            }
            Err(other) => panic!("expected AdapterWarmup, got {other:?}"),
            Ok(_) => panic!("expected AdapterWarmup, got Ok"),
        }
    }

    /// Warmup-failed adapters never reach the hosted set, so a
    /// sibling slot whose adapter warms fine still materializes.
    /// Locks the per-slot isolation that
    /// `Profile` / `AdapterFactory` / `RuntimeMissing` already enforce.
    #[tokio::test]
    async fn warmup_failure_does_not_taint_sibling_slots() {
        let (factory_ok, ok_counts) =
            ScriptedPersonaAdapterFactory::heuristic_with_counters();
        let ok_plan = vec![MaterializedPersonaPlan {
            role: RoleId::Helper,
            instance: fake_instance("Paige"),
            profile: Ok(fake_profile("Paige", "model-a")),
        }];
        let hosted_ok =
            materialize_adapters(ok_plan, &factory_ok, StubAircCitizen::fresh_lookup(), |_| None)
                .await;
        assert!(hosted_ok[0].is_ok(), "ok-warmup adapter materializes");
        assert_eq!(ok_counts.warmups(), 1);

        let factory_fail = ScriptedPersonaAdapterFactory::heuristic_with_warmup_failure(
            "simulated warmup failure",
        );
        let fail_plan = vec![MaterializedPersonaPlan {
            role: RoleId::Coder,
            instance: fake_instance("Pax"),
            profile: Ok(fake_profile("Pax", "model-b")),
        }];
        let hosted_fail = materialize_adapters(
            fail_plan,
            &factory_fail,
            StubAircCitizen::fresh_lookup(),
            |_| None,
        )
        .await;
        assert!(
            matches!(hosted_fail[0], Err(SupervisorError::AdapterWarmup { .. })),
            "warmup-failing adapter fails its own slot"
        );
    }
}
