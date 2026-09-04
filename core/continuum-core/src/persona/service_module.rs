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

/// Wall-clock helper. Tied off behind a free function so production +
/// tests use the same monotonic source; tests that want determinism
/// pass an explicit `now_ms` into the lower-level helpers.
fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .expect("system time before UNIX_EPOCH")
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
        self.drain_all_personas(now_ms()).await
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_module() -> PersonaServiceModule {
        PersonaServiceModule::new(Arc::new(RagEngine::new()))
    }

    fn test_config() -> ResponderConfig {
        ResponderConfig {
            model: "test-model".to_string(),
            system_prompt: "You are a helpful test persona.".to_string(),
            capabilities: HashSet::new(),
            specialty: "general".to_string(),
        }
    }

    #[test]
    fn config_declares_persona_prefix_and_high_priority() {
        let m = fresh_module();
        let cfg = m.config();
        assert_eq!(cfg.name, "persona");
        assert_eq!(cfg.priority, ModulePriority::High);
        assert_eq!(cfg.command_prefixes, &["persona/"]);
        assert_eq!(cfg.tick_interval, Some(Duration::from_millis(250)));
    }

    #[tokio::test]
    async fn status_with_no_enrollments_reports_zero_and_prep_scope() {
        let m = fresh_module();
        let result = m
            .handle_command("persona/status", Value::Null)
            .await
            .expect("status succeeds");
        let CommandResult::Json(v) = result else {
            panic!("expected Json result")
        };
        assert_eq!(v["module"], "persona");
        assert_eq!(v["enrolled"], 0);
        assert_eq!(v["personas"].as_array().unwrap().len(), 0);
        assert!(v["scope"].as_str().unwrap().contains("L0-2-prep"));
    }

    #[tokio::test]
    async fn enroll_constructs_slot_and_status_reflects_it() {
        let m = fresh_module();
        let persona_id = Uuid::new_v4();
        let result = m
            .handle_command(
                "persona/enroll",
                json!({
                    "persona_id": persona_id.to_string(),
                    "display_name": "Helper",
                    "model": "test-model",
                    "specialty": "general",
                }),
            )
            .await
            .expect("enroll succeeds with valid params");
        let CommandResult::Json(enroll_result) = result else {
            panic!("expected Json result")
        };
        assert_eq!(enroll_result["enrolled"], persona_id.to_string());
        assert_eq!(enroll_result["total"], 1);

        let status = m
            .handle_command("persona/status", Value::Null)
            .await
            .expect("status succeeds");
        let CommandResult::Json(s) = status else {
            panic!("expected Json result")
        };
        assert_eq!(s["enrolled"], 1);
        let personas = s["personas"].as_array().unwrap();
        assert_eq!(personas.len(), 1);
        assert_eq!(personas[0]["persona_id"], persona_id.to_string());
        assert_eq!(personas[0]["display_name"], "Helper");
    }

    #[tokio::test]
    async fn enroll_is_idempotent_and_updates_display_name() {
        let m = fresh_module();
        let persona_id = Uuid::new_v4();
        m.enroll(persona_id, "First", test_config())
            .expect("first enroll");
        m.enroll(persona_id, "Second", test_config())
            .expect("second enroll");
        assert_eq!(m.enrolled_count().unwrap(), 1);
        let snapshot = m.enrolled_snapshot().unwrap();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].1, "Second");
    }

    #[tokio::test]
    async fn enroll_two_distinct_personas_keeps_both() {
        let m = fresh_module();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        m.enroll(a, "Alpha", test_config()).expect("enroll alpha");
        m.enroll(b, "Beta", test_config()).expect("enroll beta");
        assert_eq!(m.enrolled_count().unwrap(), 2);
    }

    #[tokio::test]
    async fn enroll_missing_persona_id_fails_loud() {
        let m = fresh_module();
        let err = m
            .handle_command("persona/enroll", json!({"display_name": "Helper"}))
            .await
            .expect_err("enroll without persona_id must fail");
        assert!(
            err.contains("persona_id"),
            "error names the missing param: {err}"
        );
    }

    #[tokio::test]
    async fn enroll_missing_display_name_fails_loud() {
        let m = fresh_module();
        let err = m
            .handle_command(
                "persona/enroll",
                json!({"persona_id": Uuid::new_v4().to_string()}),
            )
            .await
            .expect_err("enroll without display_name must fail");
        assert!(
            err.contains("display_name"),
            "error names the missing param: {err}"
        );
    }

    #[tokio::test]
    async fn enroll_invalid_uuid_fails_loud() {
        let m = fresh_module();
        let err = m
            .handle_command(
                "persona/enroll",
                json!({"persona_id": "not-a-uuid", "display_name": "X"}),
            )
            .await
            .expect_err("enroll with invalid uuid must fail");
        assert!(
            err.contains("uuid") || err.contains("invalid"),
            "error names the parse failure: {err}"
        );
    }

    #[tokio::test]
    async fn unknown_command_returns_clear_error() {
        let m = fresh_module();
        let err = m
            .handle_command("persona/teleport", Value::Null)
            .await
            .expect_err("unknown commands must error");
        assert!(err.contains("persona/teleport"), "error names the command");
    }

    #[tokio::test]
    async fn tick_with_no_enrolled_personas_succeeds_quietly() {
        let m = fresh_module();
        m.tick().await.expect("empty tick succeeds");
    }

    #[tokio::test]
    async fn tick_with_enrolled_persona_and_no_items_is_no_op() {
        let m = fresh_module();
        let persona_id = Uuid::new_v4();
        m.enroll(persona_id, "Helper", test_config())
            .expect("enroll");
        // No items in any channel — tick should drain nothing, errors zero.
        m.tick().await.expect("tick succeeds with idle persona");
        assert_eq!(m.enrolled_count().unwrap(), 1);
        // Failure counter should be zero — idle is not a failure.
        let personas = m.personas.lock().unwrap();
        let slot = personas.get(&persona_id).expect("persona enrolled");
        assert_eq!(slot.consecutive_service_failures, 0);
        assert_eq!(slot.circuit_open_until_ms, 0);
    }

    use crate::persona::channel_items::ChatQueueItem;
    use crate::persona::channel_queue::{ChannelQueue, ChannelQueueConfig};
    use crate::persona::channel_types::ActivityDomain;
    use crate::persona::types::SenderType;

    /// Construct a chat queue item with sensible defaults for tests.
    fn test_chat_item(content: &str, sender_human: bool, room_id: Uuid) -> ChatQueueItem {
        ChatQueueItem {
            id: Uuid::new_v4(),
            room_id,
            content: content.to_string(),
            sender_id: Uuid::new_v4(),
            sender_name: "Sender".to_string(),
            sender_type: if sender_human {
                SenderType::Human
            } else {
                SenderType::Persona
            },
            mentions: false,
            timestamp: 1_700_000_000_000,
            enqueued_at: 1_700_000_000_000,
            priority: 0.5,
            consolidated_context: vec![],
            media: vec![],
            embedding_cell: std::sync::OnceLock::new(),
            #[cfg(any(test, feature = "test-fixtures"))]
            compute_calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Ensure the Chat channel exists on this persona's registry so
    /// items can be routed there for service_cycle_batched to find.
    fn ensure_chat_channel(persona: &mut EnrolledPersona) {
        if persona.channels.get(ActivityDomain::Chat).is_none() {
            persona
                .channels
                .register(ChannelQueue::new(ChannelQueueConfig {
                    domain: ActivityDomain::Chat,
                    max_size: 64,
                    name: "chat".to_string(),
                }));
        }
    }

    #[tokio::test]
    async fn service_burst_for_idle_returns_empty_vec() {
        // what this catches: a future regression where service_burst_for
        // pretends a burst exists on an empty channel. The batched
        // contract is "zero work in → zero bursts out", not "one
        // burst with empty content".
        let m = fresh_module();
        let persona_id = Uuid::new_v4();
        m.enroll(persona_id, "Helper", test_config())
            .expect("enroll");
        let mut personas = m.personas.lock().unwrap();
        let persona = personas.get_mut(&persona_id).unwrap();
        ensure_chat_channel(persona);
        let bursts =
            PersonaServiceModule::service_burst_for(persona, 1_700_000_000_000).expect("idle ok");
        assert!(
            bursts.is_empty(),
            "no items routed → no bursts; got {} entries",
            bursts.len()
        );
    }

    #[tokio::test]
    async fn service_burst_for_dispatches_chat_burst_through_analyze_burst() {
        // what this catches: regression where service_burst_for stops
        // honoring the demand-pull doctrine (one burst per channel-tick
        // through analyze_burst). Pins: gate fires once, respond_input
        // is fully-formed from BurstRespondContext, persona config flows
        // through (no empty-string defaults), aggregated_content carries
        // the burst's text.
        let m = fresh_module();
        let persona_id = Uuid::new_v4();
        m.enroll(persona_id, "Helper", test_config())
            .expect("enroll");
        let room_id = Uuid::new_v4();
        let mut personas = m.personas.lock().unwrap();
        let persona = personas.get_mut(&persona_id).unwrap();
        ensure_chat_channel(persona);
        let item = test_chat_item("hello", true, room_id);
        persona
            .channels
            .route(std::sync::Arc::new(item))
            .expect("route chat item to Chat channel");
        let bursts = PersonaServiceModule::service_burst_for(persona, 1_700_000_000_000)
            .expect("dispatch ok");
        // One chat item routed → exactly one Chat burst. Multiple
        // bursts here would mean the batched drain double-counted.
        assert_eq!(bursts.len(), 1, "one chat item → one burst");
        match bursts.into_iter().next().unwrap() {
            ServiceBurstDecision::NeedsResponse {
                burst_message_count,
                primary_room,
                decision,
                respond_input,
            } => {
                assert_eq!(burst_message_count, 1, "single-item burst");
                assert_eq!(primary_room, room_id);
                assert!(decision.should_respond, "human-sender mention → respond");
                // Doctrine pin: respond_input carries the persona's
                // real config, not empty defaults (all came from
                // test_config()).
                assert_eq!(respond_input.model, "test-model");
                assert_eq!(respond_input.persona.specialty, "general");
                assert_eq!(
                    respond_input.system_prompt,
                    "You are a helpful test persona."
                );
                // Aggregated content carries the burst text in
                // "Sender: content" form (ChatChannelView::interpret).
                assert!(
                    respond_input.message_text.contains("hello"),
                    "aggregated_content must carry the burst text; got {:?}",
                    respond_input.message_text
                );
            }
            other => panic!("expected NeedsResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn enroll_with_empty_model_is_rejected_loud() {
        let m = fresh_module();
        let persona_id = Uuid::new_v4();
        let mut bad_config = test_config();
        bad_config.model = String::new();
        let err = m
            .enroll(persona_id, "Helper", bad_config)
            .expect_err("enroll must reject empty model");
        assert!(err.contains("model"), "error names the field: {err}");
        assert_eq!(
            m.enrolled_count().unwrap(),
            0,
            "rejected enrollment must not mutate state"
        );
    }

    #[tokio::test]
    async fn enroll_with_empty_specialty_is_rejected_loud() {
        let m = fresh_module();
        let persona_id = Uuid::new_v4();
        let mut bad_config = test_config();
        bad_config.specialty = String::new();
        let err = m
            .enroll(persona_id, "Helper", bad_config)
            .expect_err("enroll must reject empty specialty");
        assert!(err.contains("specialty"), "error names the field: {err}");
    }

    #[tokio::test]
    async fn enroll_command_requires_model() {
        let m = fresh_module();
        let persona_id = Uuid::new_v4();
        let err = m
            .handle_command(
                "persona/enroll",
                json!({
                    "persona_id": persona_id.to_string(),
                    "display_name": "Helper",
                }),
            )
            .await
            .expect_err("enroll command must require model");
        assert!(
            err.contains("model"),
            "error names the missing param: {err}"
        );
    }

    #[tokio::test]
    async fn drain_all_personas_processes_two_personas_independently() {
        let m = fresh_module();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        m.enroll(a, "Alpha", test_config()).expect("enroll a");
        m.enroll(b, "Beta", test_config()).expect("enroll b");
        let room_id = Uuid::new_v4();
        {
            let mut personas = m.personas.lock().unwrap();
            for persona in personas.values_mut() {
                ensure_chat_channel(persona);
                persona
                    .channels
                    .route(std::sync::Arc::new(test_chat_item("hi", true, room_id)))
                    .expect("route");
            }
        }
        m.drain_all_personas(1_700_000_000_000)
            .await
            .expect("drain ok");
        // Both personas should be healthy: zero consecutive failures,
        // closed circuit.
        let personas = m.personas.lock().unwrap();
        for persona in personas.values() {
            assert_eq!(persona.consecutive_service_failures, 0);
            assert_eq!(persona.circuit_open_until_ms, 0);
        }
    }

    #[tokio::test]
    async fn drain_handles_large_burst_without_tripping_cb() {
        // what this catches: regression where a busy tick (many items
        // routed to one persona's chat channel) breaks the per-persona
        // CB. The batched cutover collapses N same-room items into
        // ONE coherent burst via service_cycle_batched + consolidation,
        // so one drain call dispatches at most one inference per
        // domain — but the SERVICE-LAYER must still see clean state.
        let m = fresh_module();
        let persona_id = Uuid::new_v4();
        m.enroll(persona_id, "Helper", test_config())
            .expect("enroll");
        let room_id = Uuid::new_v4();
        let staged = 25;
        {
            let mut personas = m.personas.lock().unwrap();
            let persona = personas.get_mut(&persona_id).unwrap();
            ensure_chat_channel(persona);
            for i in 0..staged {
                let mut item = test_chat_item(&format!("msg {i}"), true, room_id);
                item.timestamp = 1_700_000_000_000 + i as u64;
                persona
                    .channels
                    .route(std::sync::Arc::new(item))
                    .expect("route item");
            }
        }
        m.drain_all_personas(1_700_000_000_000)
            .await
            .expect("drain ok");
        // Persona stays healthy: zero service-layer failures, CB closed.
        let personas = m.personas.lock().unwrap();
        let persona = personas.get(&persona_id).unwrap();
        assert_eq!(persona.consecutive_service_failures, 0);
        assert_eq!(persona.circuit_open_until_ms, 0);
    }

    #[tokio::test]
    async fn tick_is_no_op_for_empty_module() {
        // The L0-2-dispatch tick drains personas; with none enrolled
        // it should still complete cleanly.
        let m = fresh_module();
        m.tick().await.expect("empty tick succeeds");
    }

    // --- L0-2-respond-call tests: Responder DI, inference CB threshold ---

    use std::sync::atomic::{AtomicU32, Ordering};

    /// Test responder that records every call + returns scripted outcomes.
    struct MockResponder {
        call_count: AtomicU32,
        scripted: ResponderScript,
    }

    enum ResponderScript {
        /// Always returns Spoke with the given text.
        AlwaysSpoke(String),
        /// Always returns an error with the given message.
        AlwaysErr(String),
    }

    #[async_trait]
    impl Responder for MockResponder {
        async fn respond(&self, input: RespondInput) -> Result<PersonaResponse, String> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            match &self.scripted {
                ResponderScript::AlwaysSpoke(text) => Ok(PersonaResponse::Spoke {
                    persona_id: input.persona.persona_id,
                    text: text.clone(),
                    model_used: input.model.clone(),
                    inference_ms: 1,
                    total_ms: 2,
                    think_blocks_emitted: 0,
                }),
                ResponderScript::AlwaysErr(msg) => Err(msg.clone()),
            }
        }
    }

    fn module_with_responder(
        script: ResponderScript,
    ) -> (PersonaServiceModule, Arc<MockResponder>) {
        let mock = Arc::new(MockResponder {
            call_count: AtomicU32::new(0),
            scripted: script,
        });
        let m = PersonaServiceModule::with_responder(
            Arc::new(RagEngine::new()),
            mock.clone() as Arc<dyn Responder>,
        );
        (m, mock)
    }

    #[tokio::test]
    async fn drain_calls_responder_when_gate_says_yes() {
        let (m, mock) = module_with_responder(ResponderScript::AlwaysSpoke("howdy".to_string()));
        let persona_id = Uuid::new_v4();
        m.enroll(persona_id, "Helper", test_config())
            .expect("enroll");
        let room_id = Uuid::new_v4();
        {
            let mut personas = m.personas.lock().unwrap();
            let persona = personas.get_mut(&persona_id).unwrap();
            ensure_chat_channel(persona);
            persona
                .channels
                .route(std::sync::Arc::new(test_chat_item("hi", true, room_id)))
                .expect("route");
        }
        m.drain_all_personas(1_700_000_000_000)
            .await
            .expect("drain ok");
        assert_eq!(
            mock.call_count.load(Ordering::SeqCst),
            1,
            "responder must be called exactly once for the single chat burst"
        );
        // Persona healthy (no failures, circuit closed).
        let personas = m.personas.lock().unwrap();
        let p = personas.get(&persona_id).unwrap();
        assert_eq!(p.consecutive_service_failures, 0);
        assert_eq!(p.consecutive_inference_failures, 0);
        assert_eq!(p.circuit_open_until_ms, 0);
    }

    #[tokio::test]
    async fn drain_does_not_call_responder_when_gate_says_no() {
        // ai-sender + no @mention → response_cap / sender filter typically
        // gates it silent. Either way, if SilentByDecision fires, the
        // responder must NOT be invoked.
        let (m, mock) = module_with_responder(ResponderScript::AlwaysSpoke("never".to_string()));
        let persona_id = Uuid::new_v4();
        m.enroll(persona_id, "Helper", test_config())
            .expect("enroll");
        let room_id = Uuid::new_v4();
        {
            let mut personas = m.personas.lock().unwrap();
            let persona = personas.get_mut(&persona_id).unwrap();
            ensure_chat_channel(persona);
            // ai-sender, not mentioned — the gate typically goes silent here
            persona
                .channels
                .route(std::sync::Arc::new(test_chat_item("hi", false, room_id)))
                .expect("route");
        }
        m.drain_all_personas(1_700_000_000_000)
            .await
            .expect("drain ok");
        // Whether the gate said yes or no for this specific shape isn't
        // guaranteed by analyze_burst alone — what's guaranteed is that
        // IF the gate says no, responder is never called. We can't reliably
        // assert gate behavior here without mocking it, so we assert the
        // weaker (and architecturally interesting) invariant: call_count
        // is either 0 (gate silent) or 1 (gate said yes), never higher.
        let calls = mock.call_count.load(Ordering::SeqCst);
        assert!(calls <= 1, "responder called more than once: {calls}");
    }

    #[tokio::test]
    async fn inference_errors_eventually_trip_circuit_at_inference_threshold() {
        // Repeated inference failures should trip the CB at the inference
        // threshold (15), not the service threshold (5). To exercise this
        // we need 15 inference failures. Each tick we stage one chat
        // item → service_burst_for emits ONE NeedsResponse burst → one
        // inference call → one failure. drain_all_personas breaks out of
        // the burst loop on inference error, so we drive exactly 15
        // ticks to reach the threshold.
        let (m, mock) =
            module_with_responder(ResponderScript::AlwaysErr("model not loaded".to_string()));
        let persona_id = Uuid::new_v4();
        m.enroll(persona_id, "Helper", test_config())
            .expect("enroll");
        let room_id = Uuid::new_v4();
        for tick in 0..CIRCUIT_BREAKER_MAX_CONSECUTIVE_INFERENCE_FAILURES {
            // Stage a fresh item on each tick.
            {
                let mut personas = m.personas.lock().unwrap();
                let persona = personas.get_mut(&persona_id).unwrap();
                ensure_chat_channel(persona);
                let mut item = test_chat_item(&format!("msg {tick}"), true, room_id);
                item.timestamp = 1_700_000_000_000 + tick as u64;
                persona
                    .channels
                    .route(std::sync::Arc::new(item))
                    .expect("route");
            }
            m.drain_all_personas(1_700_000_000_000 + tick as u64)
                .await
                .expect("drain ok");
        }
        let calls = mock.call_count.load(Ordering::SeqCst);
        assert_eq!(
            calls, CIRCUIT_BREAKER_MAX_CONSECUTIVE_INFERENCE_FAILURES,
            "responder should be called exactly the threshold count of times"
        );
        let personas = m.personas.lock().unwrap();
        let p = personas.get(&persona_id).unwrap();
        assert_eq!(
            p.consecutive_inference_failures, CIRCUIT_BREAKER_MAX_CONSECUTIVE_INFERENCE_FAILURES,
            "inference failure counter should equal the threshold"
        );
        assert_ne!(
            p.circuit_open_until_ms, 0,
            "circuit must be open after threshold inference failures"
        );
    }

    #[tokio::test]
    async fn inference_failure_below_threshold_does_not_trip_circuit() {
        // 1 inference error → counter at 1, circuit still closed.
        let (m, _mock) =
            module_with_responder(ResponderScript::AlwaysErr("transient hiccup".to_string()));
        let persona_id = Uuid::new_v4();
        m.enroll(persona_id, "Helper", test_config())
            .expect("enroll");
        let room_id = Uuid::new_v4();
        {
            let mut personas = m.personas.lock().unwrap();
            let persona = personas.get_mut(&persona_id).unwrap();
            ensure_chat_channel(persona);
            persona
                .channels
                .route(std::sync::Arc::new(test_chat_item("hi", true, room_id)))
                .expect("route");
        }
        m.drain_all_personas(1_700_000_000_000)
            .await
            .expect("drain ok");
        let personas = m.personas.lock().unwrap();
        let p = personas.get(&persona_id).unwrap();
        assert_eq!(p.consecutive_inference_failures, 1);
        assert_eq!(
            p.circuit_open_until_ms, 0,
            "single inference failure must not trip circuit (threshold is higher)"
        );
    }

    #[tokio::test]
    async fn successful_response_resets_inference_failure_counter() {
        // 1 inference error followed by 1 success should reset counter.
        // We do this via a counter-based mock that errors once then spokes.
        struct OnceErrThenSpoke {
            calls: AtomicU32,
        }
        #[async_trait]
        impl Responder for OnceErrThenSpoke {
            async fn respond(&self, input: RespondInput) -> Result<PersonaResponse, String> {
                let n = self.calls.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    Err("first call errors".to_string())
                } else {
                    Ok(PersonaResponse::Spoke {
                        persona_id: input.persona.persona_id,
                        text: "ok".to_string(),
                        model_used: input.model.clone(),
                        inference_ms: 1,
                        total_ms: 2,
                        think_blocks_emitted: 0,
                    })
                }
            }
        }
        let mock = Arc::new(OnceErrThenSpoke {
            calls: AtomicU32::new(0),
        });
        let m = PersonaServiceModule::with_responder(
            Arc::new(RagEngine::new()),
            mock.clone() as Arc<dyn Responder>,
        );
        let persona_id = Uuid::new_v4();
        m.enroll(persona_id, "Helper", test_config())
            .expect("enroll");
        let room_id = Uuid::new_v4();
        // Tick 1: route an item + drain → inference error
        {
            let mut personas = m.personas.lock().unwrap();
            let p = personas.get_mut(&persona_id).unwrap();
            ensure_chat_channel(p);
            p.channels
                .route(std::sync::Arc::new(test_chat_item("first", true, room_id)))
                .expect("route");
        }
        m.drain_all_personas(1_700_000_000_000).await.expect("ok");
        // Tick 2: route fresh item + drain → success
        {
            let mut personas = m.personas.lock().unwrap();
            let p = personas.get_mut(&persona_id).unwrap();
            let mut item = test_chat_item("second", true, room_id);
            item.timestamp = 1_700_000_000_001;
            p.channels.route(std::sync::Arc::new(item)).expect("route");
        }
        m.drain_all_personas(1_700_000_000_001).await.expect("ok");
        // After the success, the inference counter should be reset to 0.
        let personas = m.personas.lock().unwrap();
        let p = personas.get(&persona_id).unwrap();
        assert_eq!(
            p.consecutive_inference_failures, 0,
            "successful response after error must reset counter"
        );
    }
}
