use super::*;

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

/// Production factory: resolves every profile to its launch and hands it to
/// `LlamaCppAdapter::for_launch`. Stateless — safe to share via
/// `Arc` across the supervisor's persona materialization loop.
pub struct LlamaCppPersonaAdapterFactory;

#[async_trait]
impl PersonaAdapterFactory for LlamaCppPersonaAdapterFactory {
    async fn build_adapter(
        &self,
        profile: &PersonaInferenceProfile,
    ) -> Result<Arc<dyn AIProviderAdapter>, String> {
        let launch = profile
            .llamacpp_launch()
            .map_err(|e| format!("LlamaCppAdapter launch failed: {e}"))?;
        Ok(Arc::new(crate::inference::llamacpp_adapter::LlamaCppAdapter::for_launch(&launch)))
    }
}

/// Routes a persona's reasoning through whatever model the **serving daemon**
/// has live, read off its published [`ServingSnapshot`] (the `watch` seam in
/// [`crate::inference::llama_server`]). The transport underneath is still the
/// OpenAI-compatible `/v1` adapter — chosen 2026-06-21 (Joel) because it does
/// **native function calling for free** (sends `tools`, parses `tool_calls` →
/// `FinishReason::ToolUse`), so the persona's HANDS actually fire instead of the
/// model narrating fake tool use.
///
/// The factory does NOT probe `/v1/models` itself: it reads the daemon's snapshot
/// (one source of truth for "what is served"), so every upstart binds to the same
/// model the serving plan reconciled — no drift between what this picks and what
/// the daemon actually loaded. All personas share the one served model
/// ([[seamless-persona-failover-model-and-genome]]: the lease contracts on
/// (base_model, genome), re-homable across grid nodes via this same snapshot).
///
/// Fail LOUD if the daemon brought up nothing: a persona cannot upstart without a
/// model, and we never fall back to a stand-in ([[fallbacks-are-illegal-fail-loud]]).
pub struct ServedModelPersonaAdapterFactory;

#[async_trait]
impl PersonaAdapterFactory for ServedModelPersonaAdapterFactory {
    async fn build_adapter(
        &self,
        profile: &PersonaInferenceProfile,
    ) -> Result<Arc<dyn AIProviderAdapter>, String> {
        // Read the serving daemon's own readiness signal — the same `watch` it
        // publishes — and wait (bounded) for the first reconcile so an upstart
        // that races boot still binds correctly. No HTTP probe of our own; the
        // daemon is the single source of truth for "what is served".
        let snap = crate::inference::llama_server::await_ready_serving(
            crate::inference::llama_server::DEFAULT_SERVING_WAIT,
        )
        .await
        .ok_or_else(|| {
            "serving daemon brought up NO ready model — a persona cannot upstart without one \
             (no servable GGUF on disk, or it failed to become ready; no local fallback)"
                .to_string()
        })?;
        let model = snap.active_model.clone().ok_or_else(|| {
            "serving daemon reports ready but no active model (daemon invariant violated)"
                .to_string()
        })?;
        continuum_base::probe!(
            class = "persona.upstart.bind",
            persona = %profile.persona_name,
            persona_id = %profile.persona_id,
            model = %model,
            base = %snap.base_url,
            "persona inference bound to the live served model"
        );
        build_served_adapter(&snap).await
    }
}

/// Build the shared served-model adapter from a serving snapshot — the ONE site
/// that turns "what the daemon serves" into an OpenAI-compatible adapter pointed
/// at the live server. Used by the persona upstart factory ABOVE (first bind) AND
/// by the served-model re-home reconciler (`ipc/mod.rs`, live rebind), so an
/// upstart and a grid-driven re-home construct the adapter through IDENTICAL code
/// — no drift between the boot binding and the swap.
///
/// The `llama-server` catalog key names the OpenAI-compatible provider entry
/// (header shape + capabilities); the runtime `base_url` + `active_model` come from
/// the snapshot, overriding the catalog default (which can drift from what's
/// loaded). One adapter is shared by every persona on this node
/// (INFERENCE-LANES-REALISTIC: one base model, N persona lanes), so this HTTP init
/// runs once per served-model edge, not per persona.
///
/// Fails LOUD if the snapshot carries no active model — the caller must pass a
/// `ready` snapshot ([[fallbacks-are-illegal-fail-loud]]).
pub async fn build_served_adapter(
    snap: &crate::inference::llama_server::ServingSnapshot,
) -> Result<Arc<dyn AIProviderAdapter>, String> {
    let model = snap.active_model.clone().ok_or_else(|| {
        "serving snapshot has no active model — build_served_adapter needs a ready snapshot \
         (no local fallback)"
            .to_string()
    })?;
    let mut adapter = crate::ai::openai_adapter::OpenAICompatibleAdapter::from_registry(
        crate::inference::llama_server::PROVIDER_ID,
    )
    .with_runtime_base_url(snap.base_url.clone())
    .with_default_model(model);
    adapter
        .initialize()
        .await
        .map_err(|e| format!("persona adapter initialize failed: {e}"))?;
    Ok(Arc::new(adapter))
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
    pub identity: crate::persona::instance_manager::PersonaInstanceInfo,
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
    pub cognition: Arc<crate::persona::unified::PersonaCognition>,
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
    /// `tracing::warn!(persona_id = %ctx.identity.peer_id.as_uuid(), ...)`
    /// at every call site.
    ///
    /// Per the `&ctx` doctrine: the span derives from the context,
    /// the loop scopes the span, the substrate's observability stays
    /// honest about who did what without manual field threading.
    pub fn span(&self) -> tracing::Span {
        tracing::info_span!(
            "persona",
            persona_id = %self.identity.peer_id.as_uuid(),
            agent_name = %self.identity.agent_name,
            peer_id = %self.identity.peer_id,
            role = ?self.role,
            tier = %self.profile.tier_id,
            ctx_len = self.profile.context_length,
            model = %self.profile.model_id,
        )
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
    /// A resident whose checkpoint cannot be loaded must not be hosted with
    /// an empty memory. The existing resident, if any, remains registered.
    #[error("slot {slot_index} (role {role:?}): workspace registration for {persona_id} failed: {source}")]
    WorkspaceRegistration {
        slot_index: usize,
        role: RoleId,
        persona_id: uuid::Uuid,
        #[source]
        source: std::io::Error,
    },
    /// Persistent admission must be loaded before the resident's recall and
    /// future admissions can bind to it. A failed load is not an empty store.
    #[error(
        "slot {slot_index} (role {role:?}): admission restore for {persona_id} failed: {source}"
    )]
    AdmissionRestore {
        slot_index: usize,
        role: RoleId,
        persona_id: uuid::Uuid,
        #[source]
        source: crate::orm::OrmStoreError,
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
/// The persona's model override, read from the home the factory reads it from
/// (`citizens/personas/<Name>` under the continuum root), falling back to the
/// identity's own home root. `None` when neither holds a readable record.
pub(super) fn remote_override_for(
    persona_name: &str,
    identity_home: &std::path::Path,
) -> Option<crate::persona::model_override::PersonaModelOverride> {
    let by_name = crate::identity::citizen_path::citizen_home_path(
        &crate::paths::continuum_root(),
        crate::identity::IdentityKind::Persona,
        None,
        persona_name,
    )
    .parent()
    .map(|root| crate::cognition::home::PersonaHome::from_root(root.to_path_buf()));
    let by_identity = crate::cognition::home::PersonaHome::from_root(identity_home.to_path_buf());
    by_name
        .into_iter()
        .chain(std::iter::once(by_identity))
        .find_map(|home| {
            crate::persona::model_override::PersonaModelOverride::load(&home)
                .ok()
                .flatten()
        })
}
