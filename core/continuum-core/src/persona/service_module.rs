//! `PersonaServiceModule` — singleton Rust `ServiceModule` for persona
//! work.
//!
//! ## Demand-pull cognition (post-PR-C cutover)
//!
//! `drain_all_personas` calls `service_burst_for` ONCE per persona per
//! tick. That call drains every channel's coherent burst (via
//! `ChannelRegistry::service_cycle_batched`) and runs cognition's
//! `analyze_burst` per burst — ONE gate decision per channel-tick,
//! regardless of how many items each channel aggregated. Per
//! `[[cognition-batches-per-channel-adapter]]`: cognition stays dumb;
//! the channel adapter compresses N items into one coherent unit.
//!
//! Each `NeedsResponse` outcome dispatches `Responder::respond()`
//! OUTSIDE the lock. Three contracts the loop respects:
//!
//! 1. **Lock discipline.** The personas mutex is dropped before
//!    `respond().await`. Production safety: status / enroll / other
//!    personas' ticks are NOT blocked across the multi-second
//!    inference call. Pattern: collect ids briefly, then per-id: lock
//!    briefly to pop+evaluate, drop, respond, lock briefly to update
//!    circuit breaker.
//! 2. **Inference errors trip the circuit (with a higher threshold).**
//!    `consecutive_inference_failures` is a separate counter from
//!    `consecutive_service_failures`. Service-layer failures
//!    (deserialization, channel access) trip at the standard
//!    threshold (5). Inference failures trip at a higher threshold
//!    (15) — preserves "transient hiccup ≠ broken persona" while
//!    still surfacing "model never loads" as back-pressure.
//! 3. **`Responder` trait** for dependency injection. Production uses
//!    `DefaultResponder` which calls `persona::response::respond`.
//!    Tests inject a mock that captures call args + returns scripted
//!    responses (or errors) without loading a real model.
//!
//! Production safety: no production code calls `persona/enroll` yet —
//! the runtime's tick scheduler invokes `tick()` every 250ms but with
//! zero enrolled personas it's a no-op. L0-2-cutover wires the
//! production enrollment + atomically deletes
//! `PersonaAutonomousLoop.ts`.
//!
//! See [docs/grid/L0-PERSONA-COGNITION-E2E-PLAN.md] for the full
//! sequencing.

use continuum_base::time::EpochMs;
use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::cognition::response_orchestrator::PersonaSlot as ResponderPersona;
use crate::model_registry::Capability;
use crate::persona::channel_registry::{ChannelRegistry, DEFAULT_BURST_WINDOW_MS};
use crate::persona::channel_types::ActivityDomain;
use crate::persona::channel_view::CoherentInput;
use crate::persona::evaluator::{analyze_burst, BurstEvaluateResult, BurstRespondContext};
use crate::persona::persona_identity::PersonaIdentity;
use crate::persona::response::{PersonaResponse, RespondInput};
use crate::persona::turn_context::TurnContext;
use crate::persona::types::PersonaState;
use crate::persona::unified::PersonaCognition;
use std::collections::HashSet;

/// Dependency-injection point for response generation. Production binds
/// to `DefaultResponder` (which calls `persona::response::respond`).
/// Tests inject a mock that records calls and returns scripted outcomes
/// (or errors) without loading a real model.
#[async_trait]
pub trait Responder: Send + Sync {
    async fn respond(&self, input: RespondInput) -> Result<PersonaResponse, String>;
}

/// Production `Responder` — dispatches to `persona::response::respond`.
pub struct DefaultResponder;

#[async_trait]
impl Responder for DefaultResponder {
    async fn respond(&self, input: RespondInput) -> Result<PersonaResponse, String> {
        crate::persona::response::respond(input).await
    }
}

use crate::rag::RagEngine;
use crate::runtime::service_module::{CommandResult, ModuleConfig, ModulePriority, ServiceModule};
use crate::runtime::ModuleContext;

/// After this many consecutive *service-layer* failures (deserialization,
/// channel access, lock poisoning), open the per-persona circuit for
/// `CIRCUIT_BREAKER_COOLDOWN_MS`. Service-layer failures are signs of
/// real structural problems — trip fast.
const CIRCUIT_BREAKER_MAX_CONSECUTIVE_SERVICE_FAILURES: u32 = 5;
/// After this many consecutive *inference* failures from `Responder::respond`,
/// open the per-persona circuit. Higher than the service threshold —
/// inference can be transiently slow / OOMy / model-loading without
/// the persona being structurally broken. But if the model genuinely
/// never loads, eventually trip and surface back-pressure rather than
/// silently dropping every message.
const CIRCUIT_BREAKER_MAX_CONSECUTIVE_INFERENCE_FAILURES: u32 = 15;
/// Duration the per-persona circuit stays open after tripping.
const CIRCUIT_BREAKER_COOLDOWN_MS: u64 = 30_000;

/// Per-persona persistent response configuration. Required at enrollment.
/// All fields validated non-empty/non-default at enrollment time so
/// `build_respond_input_from_burst` can construct a honestly-populated `RespondInput`
/// — no empty-string fallbacks that the inference layer would have to
/// fail-loudly on. (Per Joel 2026-05-29 + the URI doctrine peer mapped:
/// empty model fails at the URI parser; same fail-loud should happen at
/// our boundary, not deeper.)
#[derive(Debug, Clone)]
pub struct ResponderConfig {
    /// Model identifier this persona renders with. Non-empty.
    pub model: String,
    /// Persona's system prompt / identity template. For now used as-is;
    /// RAG-enriched system prompt construction is upstream-context
    /// plumbing that lands when the actual `respond()` dispatch wires.
    pub system_prompt: String,
    /// Model capabilities (vision, audio input, streaming, etc.).
    /// Empty set is a VALID value (a text-only persona); but the field
    /// must be supplied explicitly, not defaulted.
    pub capabilities: HashSet<Capability>,
    /// Stable specialty identifier (e.g. "code-review", "general").
    /// Matched against `SharedAnalysis.suggested_angles` by the
    /// response orchestrator. Non-empty (use "general" for unscoped).
    pub specialty: String,
}

impl ResponderConfig {
    /// Validate required fields. Returns a clear error message naming
    /// any missing piece so misconfiguration surfaces at enrollment,
    /// not inside the inference layer.
    pub fn validate(&self) -> Result<(), String> {
        if self.model.trim().is_empty() {
            return Err(
                "ResponderConfig.model is empty (persona must declare its model)".to_string(),
            );
        }
        if self.specialty.trim().is_empty() {
            return Err(
                "ResponderConfig.specialty is empty (use 'general' if unscoped, not empty)"
                    .to_string(),
            );
        }
        // system_prompt + capabilities may legitimately be empty for
        // some personas; their emptiness is recorded but not rejected.
        Ok(())
    }
}

/// Per-persona state inside the singleton service module. One entry per
/// enrolled persona; carries the persona's cognition container, the
/// per-persona channel queues + state for the service loop, the
/// responder config supplied at enrollment, and the per-enrollment
/// circuit-breaker bookkeeping.
///
/// Named `EnrolledPersona` rather than `PersonaSlot` to avoid collision
/// with the existing `cognition::response_orchestrator::PersonaSlot`
/// DTO (which is a minimal identity+specialty handle used as input to
/// `respond()`).
pub struct EnrolledPersona {
    pub persona_id: Uuid,
    pub display_name: String,
    pub cognition: PersonaCognition,
    /// Per-persona channel queues (chat, voice, task). `service_burst_for`
    /// drains coherent bursts via `channels.service_cycle_batched(state, identity, window)`.
    pub channels: ChannelRegistry,
    /// Per-persona state (energy, mood, attention, inbox_load) consumed
    /// by `service_cycle_batched` to gate non-urgent items by `should_engage`.
    /// `service_cycle_batched` updates the inbox_load field on every call.
    pub state: PersonaState,
    /// Per-persona responder configuration. Required at enrollment;
    /// supplies `model`, `system_prompt`, `capabilities`, `specialty`
    /// for `build_respond_input_from_burst` so no field needs an empty default.
    pub responder_config: ResponderConfig,
    /// Unix-ms timestamp at which the per-persona circuit re-closes.
    /// 0 means the circuit is currently closed (healthy).
    pub circuit_open_until_ms: u64,
    /// Consecutive service-layer failures (deserialization, channel
    /// access, lock poisoning). Trips the circuit at
    /// `CIRCUIT_BREAKER_MAX_CONSECUTIVE_SERVICE_FAILURES` (5).
    pub consecutive_service_failures: u32,
    /// Consecutive inference failures from `Responder::respond`. Trips
    /// the circuit at `CIRCUIT_BREAKER_MAX_CONSECUTIVE_INFERENCE_FAILURES`
    /// (15) — higher tolerance because inference can be transiently
    /// slow/OOMy without the persona being structurally broken.
    pub consecutive_inference_failures: u32,
}

impl EnrolledPersona {
    fn new(
        persona_id: Uuid,
        display_name: String,
        cognition: PersonaCognition,
        responder_config: ResponderConfig,
    ) -> Self {
        Self {
            persona_id,
            display_name,
            cognition,
            channels: ChannelRegistry::new(),
            state: PersonaState::new(),
            responder_config,
            circuit_open_until_ms: 0,
            consecutive_service_failures: 0,
            consecutive_inference_failures: 0,
        }
    }
}

/// Output of the *synchronous* pop+decide step (`service_burst_for`)
/// inside the lock — ONE entry per coherent burst drained from a
/// persona's channels this tick. The async `Responder::respond`
/// dispatch happens OUTSIDE the lock; `drain_all_personas` iterates
/// the Vec returned by `service_burst_for` and dispatches each burst.
///
/// Per `[[cognition-batches-per-channel-adapter]]`: ONE entry per
/// channel-burst, not per item. A burst aggregating N chat messages
/// produces ONE `ServiceBurstDecision`, regardless of N.
#[derive(Debug)]
pub enum ServiceBurstDecision {
    /// `analyze_burst` decided NOT to respond.
    Silent {
        burst_message_count: usize,
        primary_room: Uuid,
        decision: BurstEvaluateResult,
    },
    /// `analyze_burst` decided to respond; `respond_input` is
    /// fully-formed from the burst's aggregated context. The caller
    /// dispatches `Responder::respond(*respond_input)` OUTSIDE the
    /// lock.
    NeedsResponse {
        burst_message_count: usize,
        primary_room: Uuid,
        decision: BurstEvaluateResult,
        respond_input: Box<RespondInput>,
    },
    /// Non-Chat burst (Audio / Code / Background) — typed view not yet
    /// implemented, so cognition has no gate semantics for it.
    /// Surfaced explicitly so callers / observers see the burst-shape
    /// rather than the substrate silently dropping it (per
    /// `[[no-fallbacks-ever]]`).
    UnsupportedDomain {
        domain: ActivityDomain,
        burst_message_count: usize,
    },
}

/// Singleton owning persona work in-process. Replaces the TS
/// `PersonaAutonomousLoop`; the deletion of `PersonaAutonomousLoop.ts`
/// lands with L0-2-cutover.
pub struct PersonaServiceModule {
    /// Per-persona state, keyed by persona_id. `std::sync::Mutex` —
    /// MUST NOT be held across `.await`. The lock discipline in
    /// `drain_all_personas` is built around that constraint: lock
    /// briefly to pop+evaluate, drop, await `Responder::respond`, lock
    /// briefly to update circuit breaker state.
    personas: Mutex<HashMap<Uuid, EnrolledPersona>>,
    /// Shared `RagEngine` used to construct each persona's cognition.
    /// Held at module level so all personas share a single retrieval
    /// substrate (corpora, indexes, caches).
    rag_engine: Arc<RagEngine>,
    /// Response dispatcher. Production injects `DefaultResponder`
    /// (calls `persona::response::respond`); tests inject a mock that
    /// returns scripted outcomes without loading a real model.
    responder: Arc<dyn Responder>,
}

impl PersonaServiceModule {
    pub fn new(rag_engine: Arc<RagEngine>) -> Self {
        Self::with_responder(rag_engine, Arc::new(DefaultResponder))
    }

    pub fn with_responder(rag_engine: Arc<RagEngine>, responder: Arc<dyn Responder>) -> Self {
        Self {
            personas: Mutex::new(HashMap::new()),
            rag_engine,
            responder,
        }
    }

    /// Enroll a persona. Constructs a `PersonaCognition` for it under the
    /// module's shared `RagEngine`, stores the slot. Idempotent: enrolling
    /// the same id with a different display name updates the name AND the
    /// responder config; the existing cognition + circuit-breaker state
    /// are preserved (silently resetting cognition would be a fallback).
    ///
    /// Validates the `ResponderConfig` before mutating any state — a
    /// rejected enrollment leaves the module untouched.
    pub fn enroll(
        &self,
        persona_id: Uuid,
        display_name: impl Into<String>,
        responder_config: ResponderConfig,
    ) -> Result<(), String> {
        responder_config.validate()?;
        let display_name = display_name.into();
        let mut personas = self
            .personas
            .lock()
            .map_err(|_| "personas lock poisoned".to_string())?;
        if let Some(slot) = personas.get_mut(&persona_id) {
            slot.display_name = display_name;
            slot.responder_config = responder_config;
            return Ok(());
        }
        let cognition = PersonaCognition::new(
            persona_id,
            display_name.clone(),
            Arc::clone(&self.rag_engine),
        );
        personas.insert(
            persona_id,
            EnrolledPersona::new(persona_id, display_name, cognition, responder_config),
        );
        Ok(())
    }

    /// Number of currently enrolled personas. Cheap; used by status.
    pub fn enrolled_count(&self) -> Result<usize, String> {
        let personas = self
            .personas
            .lock()
            .map_err(|_| "personas lock poisoned".to_string())?;
        Ok(personas.len())
    }

    /// Returns a snapshot of enrolled persona ids + display names, used
    /// by status. Allocates; for hot-path observers, iterate the map
    /// directly via your own lock.
    pub fn enrolled_snapshot(&self) -> Result<Vec<(Uuid, String)>, String> {
        let personas = self
            .personas
            .lock()
            .map_err(|_| "personas lock poisoned".to_string())?;
        Ok(personas
            .values()
            .map(|s| (s.persona_id, s.display_name.clone()))
            .collect())
    }

    /// Demand-pull service step for one enrolled persona. Drains every
    /// channel's coherent burst and runs cognition's `analyze_burst`
    /// per burst. Pure function over `&mut EnrolledPersona` so it
    /// composes inside the tick loop without re-acquiring the outer
    /// lock per call.
    ///
    /// Per `[[cognition-batches-per-channel-adapter]]`: one
    /// `BurstEvaluateResult` per channel-burst, regardless of how
    /// many items each channel aggregated. Returns one
    /// `ServiceBurstDecision` per drained burst; the Vec is empty
    /// when no channel had work this tick.
    ///
    /// Behavior:
    /// 1. `channels.service_cycle_batched(&mut state, &identity,
    ///    DEFAULT_BURST_WINDOW_MS)` drains coherent bursts from every
    ///    channel that has work (respects priority + state).
    /// 2. For each `CoherentInput::Chat`: call `analyze_burst`, build
    ///    a `RespondInput` from the burst's aggregated context when
    ///    the gate says respond, surface as `Silent` otherwise.
    /// 3. `CoherentInput::Other` (Audio / Code / Background — typed
    ///    views not yet implemented) surfaces as `UnsupportedDomain`
    ///    rather than silently dropping (per `[[no-fallbacks-ever]]`).
    pub fn service_burst_for(
        persona: &mut EnrolledPersona,
        now_ms: u64,
    ) -> Result<Vec<ServiceBurstDecision>, String> {
        let identity = PersonaIdentity::new(persona.persona_id, persona.display_name.clone());
        let inputs = persona.channels.service_cycle_batched(
            &mut persona.state,
            &identity,
            DEFAULT_BURST_WINDOW_MS,
        );
        let mut out = Vec::with_capacity(inputs.len());
        for input in inputs {
            match input {
                CoherentInput::Chat(_) => {
                    let decision = analyze_burst(
                        &input,
                        persona.persona_id,
                        &persona.display_name,
                        &persona.persona_id.to_string(),
                        &persona.cognition.rate_limiter,
                        &persona.cognition.sleep_state,
                        &persona.cognition.engine,
                        &persona.cognition.message_cache,
                        now_ms,
                    );
                    let burst_message_count = decision.burst_message_count;
                    let primary_room = decision.primary_room;
                    if decision.should_respond {
                        // `analyze_burst`'s typed contract: `should_respond`
                        // implies `respond_context` is Some. A None here
                        // would be a structural bug in the evaluator, not
                        // a runtime condition to handle gracefully.
                        let Some(ref ctx) = decision.respond_context else {
                            return Err("analyze_burst returned should_respond=true \
                                 with no respond_context — typed contract violated"
                                .to_string());
                        };
                        let respond_input = Self::build_respond_input_from_burst(persona, ctx);
                        out.push(ServiceBurstDecision::NeedsResponse {
                            burst_message_count,
                            primary_room,
                            decision,
                            respond_input: Box::new(respond_input),
                        });
                    } else {
                        out.push(ServiceBurstDecision::Silent {
                            burst_message_count,
                            primary_room,
                            decision,
                        });
                    }
                }
                CoherentInput::Other {
                    domain, item_count, ..
                } => {
                    out.push(ServiceBurstDecision::UnsupportedDomain {
                        domain,
                        burst_message_count: item_count,
                    });
                }
            }
        }
        Ok(out)
    }

    /// Construct a `RespondInput` for `persona::response::respond()`
    /// from the enrolled persona's stored config + a burst's
    /// `BurstRespondContext`. Deterministic + side-effect free; no
    /// empty-string defaults — every required field comes from
    /// `responder_config` (validated at enrollment) or from the
    /// burst's aggregated content.
    ///
    /// `message_id` is a fresh `Uuid::new_v4()`: the burst IS the
    /// unit of work, not any single item it aggregated, so we anchor
    /// at burst granularity per `[[cognition-batches-per-channel-
    /// adapter]]`. Downstream caches don't conflate two ticks'
    /// bursts on the same room because the anchor differs per call.
    ///
    /// Fields that are LEGITIMATELY empty here:
    /// - `turn_context.recent_history`: populated by L0-3/L0-4 when
    ///   the inbox-routing path plumbs prior-message context
    ///   per-turn. For now an empty Vec means "first-turn fresh
    ///   context."
    /// - `turn_context.known_specialties`: populated when the
    ///   response orchestrator has multiple-persona-in-room context.
    /// - `other_persona_names`: populated when the room roster is
    ///   plumbed.
    /// - `message_media`: populated when burst items carry media
    ///   (next slice).
    /// - `recalled_engrams`: populated when admission state recall is
    ///   wired (L0-3+).
    ///
    /// None of those are silently-substituted defaults — they're
    /// genuinely-absent context that the receiver tolerates. The
    /// fields that would be DANGEROUS to default (model,
    /// system_prompt, capabilities, specialty) come from
    /// responder_config which is validated non-empty at enrollment.
    fn build_respond_input_from_burst(
        persona: &EnrolledPersona,
        ctx: &BurstRespondContext,
    ) -> RespondInput {
        RespondInput {
            persona: ResponderPersona {
                persona_id: persona.persona_id,
                specialty: persona.responder_config.specialty.clone(),
                display_name: persona.display_name.clone(),
            },
            turn_context: TurnContext::arc(ctx.room_id, Vec::new(), Vec::new()),
            message_id: Uuid::new_v4(),
            message_text: ctx.aggregated_content.clone(),
            other_persona_names: Vec::new(),
            system_prompt: persona.responder_config.system_prompt.clone(),
            model: persona.responder_config.model.clone(),
            is_voice: false,
            message_media: Vec::new(),
            capabilities: persona.responder_config.capabilities.clone(),
            recalled_engrams: Vec::new(),
            // Roster grounding flows through the service-loop / compose
            // projection, not this burst projection — defaults empty
            // here (no [Present in this room] block), same tolerated-
            // absence shape as other_persona_names above.
            room_roster: Vec::new(),
            room_doctrine: None,
        }
    }

    /// Iterate every enrolled persona, run ONE batched pop+evaluate
    /// step (`service_burst_for`) per persona per tick, then dispatch
    /// `Responder::respond()` for every burst whose gate said respond.
    /// Per-persona circuit breaker gates failures.
    ///
    /// Per-tick item count is bounded by the channel-burst structure,
    /// not a counter: each call returns at most one burst per
    /// `ActivityDomain` (4 today). A burst aggregating N items is ONE
    /// dispatch, regardless of N — that's
    /// `[[cognition-batches-per-channel-adapter]]` in production form.
    ///
    /// Lock discipline (the load-bearing contract):
    /// 1. Brief lock at top: collect persona ids.
    /// 2. Drop lock.
    /// 3. Per persona id:
    ///    a. Brief lock: check circuit, call `service_burst_for` (sync
    ///       drain+evaluate, returns `Vec<ServiceBurstDecision>`).
    ///    b. Drop lock.
    ///    c. For each `NeedsResponse` burst: call
    ///       `responder.respond(...).await` OUTSIDE the lock —
    ///       production safety, status / enroll / other personas don't
    ///       block across multi-second inference calls.
    ///    d. Brief lock: update circuit-breaker state per respond
    ///       result.
    pub async fn drain_all_personas(&self, now_ms: u64) -> Result<(), String> {
        let persona_ids: Vec<Uuid> = {
            let personas = self
                .personas
                .lock()
                .map_err(|_| "personas lock poisoned".to_string())?;
            personas.keys().copied().collect()
        };
        for persona_id in persona_ids {
            let burst_result = {
                let mut personas = self
                    .personas
                    .lock()
                    .map_err(|_| "personas lock poisoned".to_string())?;
                let persona = match personas.get_mut(&persona_id) {
                    Some(p) => p,
                    None => continue, // unenrolled mid-tick
                };
                if persona.circuit_open_until_ms > now_ms {
                    continue;
                }
                if persona.circuit_open_until_ms != 0 {
                    persona.circuit_open_until_ms = 0;
                    persona.consecutive_service_failures = 0;
                    persona.consecutive_inference_failures = 0;
                }
                Self::service_burst_for(persona, now_ms)
            };
            let bursts = match burst_result {
                Ok(bursts) => {
                    // Service-layer success — reset the service-failure
                    // counter so an isolated past failure doesn't keep
                    // accumulating toward the CB threshold.
                    self.with_persona(persona_id, |p| {
                        p.consecutive_service_failures = 0;
                    })?;
                    bursts
                }
                Err(_) => {
                    let _ = self.with_persona(persona_id, |p| {
                        p.consecutive_service_failures += 1;
                        if p.consecutive_service_failures
                            >= CIRCUIT_BREAKER_MAX_CONSECUTIVE_SERVICE_FAILURES
                        {
                            p.circuit_open_until_ms =
                                now_ms.saturating_add(CIRCUIT_BREAKER_COOLDOWN_MS);
                        }
                    })?;
                    continue;
                }
            };
            // Iterate the tick's bursts. Lock dropped — respond() runs
            // free. Inference errors bump the inference-failure
            // counter; CB trips at the inference threshold.
            for burst in bursts {
                match burst {
                    ServiceBurstDecision::Silent { .. }
                    | ServiceBurstDecision::UnsupportedDomain { .. } => {
                        // Gate said silent (Chat) or typed view not yet
                        // wired (Other). Nothing further this burst.
                    }
                    ServiceBurstDecision::NeedsResponse { respond_input, .. } => {
                        let respond_result = self.responder.respond(*respond_input).await;
                        match respond_result {
                            Ok(_response) => {
                                self.with_persona(persona_id, |p| {
                                    p.consecutive_inference_failures = 0;
                                })?;
                            }
                            Err(_err) => {
                                let tripped = self.with_persona(persona_id, |p| {
                                    p.consecutive_inference_failures += 1;
                                    if p.consecutive_inference_failures
                                        >= CIRCUIT_BREAKER_MAX_CONSECUTIVE_INFERENCE_FAILURES
                                    {
                                        p.circuit_open_until_ms =
                                            now_ms.saturating_add(CIRCUIT_BREAKER_COOLDOWN_MS);
                                        true
                                    } else {
                                        false
                                    }
                                })?;
                                if tripped {
                                    // CB tripped — skip remaining bursts
                                    // for this persona this tick.
                                    break;
                                }
                                // Inference error, CB not tripped. Don't
                                // keep hammering the same misconfigured
                                // model with the next burst this tick;
                                // let the next tick retry.
                                break;
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Briefly lock the personas map and run `f` on the named persona
    /// if it's still enrolled. The closure runs inside the lock; do
    /// not `.await` inside.
    fn with_persona<F, R>(&self, persona_id: Uuid, f: F) -> Result<R, String>
    where
        F: FnOnce(&mut EnrolledPersona) -> R,
        R: Default,
    {
        let mut personas = self
            .personas
            .lock()
            .map_err(|_| "personas lock poisoned".to_string())?;
        Ok(match personas.get_mut(&persona_id) {
            Some(p) => f(p),
            None => R::default(),
        })
    }
}

#[async_trait]
impl ServiceModule for PersonaServiceModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "persona",
            priority: ModulePriority::High,
            command_prefixes: &["persona/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 1,
            tick_interval: Some(Duration::from_millis(250)),
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        match command {
            "persona/status" => {
                let snapshot = self.enrolled_snapshot()?;
                let entries: Vec<Value> = snapshot
                    .into_iter()
                    .map(|(id, name)| json!({"persona_id": id.to_string(), "display_name": name}))
                    .collect();
                Ok(CommandResult::Json(json!({
                    "module": "persona",
                    "enrolled": entries.len(),
                    "personas": entries,
                    "scope": "L0-2-prep: enroll opens; dispatch wiring lands in L0-2-dispatch",
                })))
            }
            "persona/enroll" => {
                let persona_id_str = params
                    .get("persona_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "persona/enroll requires persona_id (string)".to_string())?;
                let persona_id = Uuid::parse_str(persona_id_str)
                    .map_err(|e| format!("persona/enroll: invalid persona_id uuid: {e}"))?;
                let display_name = params
                    .get("display_name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "persona/enroll requires display_name (string)".to_string())?
                    .to_string();
                let model = params
                    .get("model")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "persona/enroll requires model (string)".to_string())?
                    .to_string();
                let system_prompt = params
                    .get("system_prompt")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let specialty = params
                    .get("specialty")
                    .and_then(Value::as_str)
                    .unwrap_or("general")
                    .to_string();
                // capabilities arrives as a JSON array of strings; each
                // entry is the kebab-case name of a `Capability` variant
                // (matching the serde rename in model_registry::Capability).
                let capabilities: HashSet<Capability> = params
                    .get("capabilities")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .filter_map(|s| serde_json::from_value::<Capability>(json!(s)).ok())
                            .collect()
                    })
                    .unwrap_or_default();
                let responder_config = ResponderConfig {
                    model,
                    system_prompt,
                    capabilities,
                    specialty,
                };
                self.enroll(persona_id, display_name, responder_config)?;
                Ok(CommandResult::Json(json!({
                    "enrolled": persona_id.to_string(),
                    "total": self.enrolled_count()?,
                })))
            }
            other => Err(format!("unknown persona command: {other}")),
        }
    }

    async fn tick(&self) -> Result<(), String> {
        // Tick drains every enrolled persona's channels via the
        // batched cognition path: ONE `service_burst_for` per persona
        // per tick. Production-safety: no production code calls
        // `persona/enroll` yet — until L0-2-cutover wires enrollment,
        // this tick runs over an empty map (no-op).
        self.drain_all_personas(EpochMs::now().millis()).await
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests;
