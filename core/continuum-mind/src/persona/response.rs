//! Per-persona response orchestration in Rust. The Rust replacement for
//! `PersonaResponseGenerator.ts` — owns the cognitive verb of "this
//! persona, given this message in this room, produces this response."
//!
//! See docs/architecture/SHARED-COGNITION.md for the architectural
//! picture and docs/architecture/PERSONA-COGNITION-RUST-MIGRATION.md
//! for the migration discipline this module is the first rung of.
//!
//! Pipeline (per persona, per inbound message):
//!
//! 1. `cognition::analyze(...)`: shared, cached prompt-time hint map.
//!    Suggested angles per specialty are informational only, not response gates.
//! 2. `prompt_assembly::build(...)`: persona-specific prompt with voice,
//!    LoRA-rendered specialty, RAG, and multimodal attachments.
//! 3. `ai_provider::generate_text(...)`: inference. The persona's own model
//!    decides what to say; no external scorer vetoes engagement.
//! 4. `strip_thinks_emit_events(...)`: extract `<think>...</think>` blocks as
//!    `cognition:think-block` events, then return clean speech for posting.
//! 5. Return `Spoke { text, ... }` with timing and diagnostic fields. Silence
//!    is valid only as the persona's cognitive output, not a pre-inference veto.
//!
//! Why this is in Rust (not just a port):
//!   - Cognition is where the mind/machine line gets drawn — concurrency
//!     primitives matter (Joel, 2026-04-19).
//!   - SharedAnalysis cache lives here; needs lock-free DashMap for
//!     concurrent personas hitting the same message.
//!   - Per-persona renders run in parallel tokio tasks; Node's single
//!     event loop blocks every persona on every other persona's
//!     inference call.
//!   - <think> parsing is a hot path on every response; regex/str
//!     manipulation in Rust is ~100x what TS does on the same input.

use continuum_base::time::EpochMs;
use crate::cognition::tool_executor::types::MediaItemLite;
use crate::cognition::{analyze, AnalysisInput, PersonaSlot, SharedAnalysis};
use crate::persona::turn_context::TurnContext;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, LazyLock};
use ts_rs::TS;
use uuid::Uuid;

/// Input to `respond()`. Caller (chat path / PRG.ts shim) collects this
/// from the room state. Carries everything needed for ONE persona's
/// response cycle — analysis is shared via cache, so no need to pass
/// other personas in.
#[derive(Debug, Clone)]
pub struct RespondInput {
    /// THIS persona's identity + specialty for scoring.
    pub persona: PersonaSlot,
    /// Per-turn shared context (room_id + recent_history +
    /// known_specialties). All personas responding to the same
    /// message share an `Arc` to the same `TurnContext` instance —
    /// no per-persona deep clone of the same data (continuum#1206).
    pub turn_context: Arc<TurnContext>,
    pub message_id: Uuid,
    /// The new message that triggered this response cycle.
    pub message_text: String,
    /// Display names of OTHER personas in the room (excluding self).
    /// Forwarded to `prompt_assembly` so the
    /// `ProperChatMlSingleParty` strategy can drop other-AI history
    /// turns that single-party-trained models cannot coherently
    /// process. Empty when the host doesn't expose a roster or when
    /// the active model uses a strategy that doesn't need it
    /// (`NamePrefixedUserTurns` ignores).
    pub other_persona_names: Vec<String>,
    /// Persona's RAG-built identity / system prompt. Caller-supplied
    /// because the persona's identity comes from RAG (which knows the
    /// persona entity, the active adapters, the user-personalization
    /// bits). The render concatenates this with the matched angle from
    /// the shared analysis.
    pub system_prompt: String,
    /// THIS persona's model identifier. Render-time choice, NOT the
    /// analysis model. Shared-cognition architecture: 1 cheap analysis
    /// on a base model + N specialty renders each on the persona's own
    /// (potentially LoRA-adapted) model. Using analysis.model_used here
    /// would defeat the entire premise — every persona would render with
    /// the same base model.
    pub model: String,
    /// True if this is a live-voice context (changes response style
    /// instructions in the assembled prompt). False for normal chat.
    pub is_voice: bool,
    /// Media (images/audio/video) attached to the current message. When
    /// present AND `capabilities` includes the matching variant
    /// (`Vision` for images, `AudioInput` for audio), the render path
    /// constructs `MessageContent::Parts` with a real
    /// `ContentPart::Image`/`Audio` instead of `MessageContent::Text` —
    /// preserving the natively-multimodal model's ability to see / hear
    /// directly. **No text-description bridging when the model IS
    /// capable** — that's the regression Joel called out 2026-04-21.
    /// Bridge layer (VisionDescriptionService) remains for genuinely
    /// text-only models as the floor, not the default.
    /// See docs/architecture/PERSONA-CONTEXT-PAGING.md §0.5.X.
    pub message_media: Vec<MediaItemLite>,
    /// Persona's resolved model capabilities. Caller (PRG) supplies them
    /// from the persona's ModelConfig — they're a property of the
    /// caller's request, not something Rust looks up mid-flight.
    ///
    /// Why this isn't a registry lookup: `getThatThingIShouldHaveJustBeenGiven`
    /// (Joel rule). The IPC already names the model; the caller already
    /// knows what it can do; passing it across removes a global lookup
    /// that silently failed when registry keys diverged from request
    /// model strings (capabilities came back empty → image bytes
    /// demoted to text marker → vision encoder never called even though
    /// the bytes were sitting right there in `message_media`). Now the
    /// declaration travels with the request — registry-key drift can't
    /// silently disable vision.
    pub capabilities: std::collections::HashSet<crate::model_registry::Capability>,
    /// Recalled engrams (per-persona admitted memory) injected as
    /// system-prompt context (continuum#1211 PR-2). The IPC layer
    /// pulls these from `AdmissionState::recall_recent` after the
    /// inline admission gate runs, then passes them through so
    /// `prompt_assembly` can render them as a `[Recent Memory]`
    /// section. Empty when the persona has no admission state OR no
    /// admitted engrams yet — both are normal early-life states and
    /// neither blocks the response cycle.
    ///
    /// Per-persona (each persona's admission store is independent)
    /// so this lives on `RespondInput`, not the per-turn-shared
    /// `TurnContext` (#1206) — different personas in the same room
    /// recall different memory.
    ///
    /// `String` (the engram's content text) rather than `Engram`
    /// because prompt_assembly only needs the text. Keeping the full
    /// `Engram` type out of this layer means a future structural
    /// change to engrams (kind enum, embeddings, recall_keys reshape)
    /// doesn't ripple into the prompt path.
    pub recalled_engrams: Vec<String>,
    /// Roster of OTHER citizens currently present in the room — one
    /// pre-formatted line per peer (`name [runtime] — availability`),
    /// produced by `RoomRosterSource` from airc `active_agents` and
    /// projected here by the service loop. Rendered by `prompt_assembly`
    /// as a `[Present in this room]` block so the persona is grounded in
    /// who is present and who is NOT itself — the fix for the persona
    /// confabulating other citizens' turns (see
    /// docs/grid/AIRC-NATIVE-IDENTITY-ROOMS-SECURITY.md §5 slice 1).
    /// Empty when no roster source is bound or the room is otherwise
    /// quiet — backwards-compatible (no block rendered).
    pub room_roster: Vec<String>,
    /// The room's operating doctrine — the airc-published contract for
    /// what kind of activity this room is (chat vs coordination vs game
    /// vs …), produced by `RoomDoctrineSource` and projected here by the
    /// service loop. Rendered by `prompt_assembly` as a `[Room operating
    /// doctrine]` block so the persona calibrates participation to the
    /// room's nature (slice 2). `None` when the room has no published
    /// doctrine — backwards-compatible (no block rendered).
    pub room_doctrine: Option<String>,
}

/// What `respond()` returns.
///
/// `Silent` is a first-class outcome: the persona considered the message,
/// found nothing additive to add, and chose not to speak. The reason is
/// observable for trainability + the persona's own meta-cognitive trace.
/// Not the same as a failure.
///
/// `Spoke` is the response that should be posted to the room.
// NOTE on field casing: ts-rs does not propagate `rename_all = "camelCase"`
// through enum variant FIELDS (only through variant TAGS). Forcing camelCase
// on the serde side without ts-rs honoring it would silently diverge the
// wire format from the generated TS bindings (caught during initial review).
// Snake_case on both sides keeps them in lockstep. Variant tags ("silent",
// "spoke") are handled by the tag rename below.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/PersonaResponse.ts"
)]
pub enum PersonaResponse {
    /// Persona chose silence. Reason carried for observability + training.
    Silent {
        #[ts(type = "string")]
        persona_id: Uuid,
        reason: String,
        /// Relevance score that drove the decision. 0.0..1.0. Carried so
        /// downstream telemetry can analyze the silence-rate by score
        /// distribution.
        relevance_score: f32,
    },
    /// Persona produced a response. The text is the CLEANED visible
    /// speech (`<think>` blocks already stripped + emitted as events).
    Spoke {
        #[ts(type = "string")]
        persona_id: Uuid,
        /// Cleaned visible speech to post to the room. `<think>` blocks
        /// have been stripped; the visible response is what the user
        /// sees in chat.
        text: String,
        /// Model that produced the response (post-routing).
        model_used: String,
        /// Duration of the inference call itself (not including
        /// analysis or scoring — those are separate).
        #[ts(type = "number")]
        inference_ms: u64,
        /// Total duration end-to-end (analysis + scoring + inference +
        /// parsing + event emission).
        #[ts(type = "number")]
        total_ms: u64,
        /// Number of `<think>` blocks extracted (for telemetry —
        /// the actual content was emitted as events for hippocampus).
        think_blocks_emitted: u32,
    },
}

/// THE per-persona response cycle. Public entry point.
///
/// Called by the chat path (or the PRG.ts shim that the chat path
/// currently calls). Each call is for ONE persona; the shared analysis
/// is cached at the cognition layer, so M personas calling this
/// concurrently for the same message do M renders + 1 analysis (not M).
///
/// Returns `Result` because inference can genuinely fail (model down,
/// timeout, bad output we can't parse). Failure should propagate to
/// the caller for proper user-facing error reporting; we don't
/// silently fall back to "Silent" because that would hide real bugs.
pub async fn respond(input: RespondInput) -> Result<PersonaResponse, String> {
    use crate::cognition::trace::CognitionTrace;

    let total_start = EpochMs::now().millis();
    let mut trace = CognitionTrace::new();

    // Run the cognition pipeline. The inner fn carries every `?`
    // exit point so the outer fn can ALWAYS record the turn. Success
    // writes the real PersonaResponse. Failure writes a recorder-only
    // error outcome and still returns Err to the caller. The chat API
    // stays honest while replay gets evidence for failed turns.
    let result = respond_inner(&input, &mut trace, total_start).await;

    // Best-effort turn capture for observability + replay. Failures
    // log inside the recorder but never propagate — the persona's
    // response is the product, the recording is observability. Any
    // host (TS server, Unreal plugin, Swift app) gets this for free
    // because it lives Rust-side, next to `respond()`.
    match &result {
        Ok(response) => crate::persona::recorder::record_turn(&input, response, &trace),
        Err(error_msg) => crate::persona::recorder::record_failed_turn(
            &input,
            error_msg,
            EpochMs::now().millis().saturating_sub(total_start),
            &trace,
        ),
    }

    result
}

/// Internal pipeline body. All `?` exit points live here so the outer
/// `respond()` can wrap with always-record. Mutating `&mut trace` so
/// every completed seam appears in the captured fixture even when a
/// later seam fails — partial traces are the diagnostic value.
async fn respond_inner(
    input: &RespondInput,
    trace: &mut crate::cognition::trace::CognitionTrace,
    total_start: u64,
) -> Result<PersonaResponse, String> {
    use crate::cognition::trace::{SEAM_ANALYZE, SEAM_INFERENCE, SEAM_POST_PROCESS};

    // RTOS-debugger breakpoint: respond cycle entry. Captures who's
    // responding to what, plus the size of the contextual inputs the
    // brain will see. See docs/architecture/RTOS-DEBUGGER-PROBES.md
    // class taxonomy for the stable name.
    continuum_base::probe!(
        class = "persona.response.enter",
        persona = %input.persona.display_name,
        persona_id = %input.persona.persona_id,
        room_id = %input.turn_context.room_id,
        message_id = %input.message_id,
        message_text_len = input.message_text.len(),
        history_count = input.turn_context.recent_history.len(),
        known_specialties = input.turn_context.known_specialties.len(),
        media_count = input.message_media.len(),
        recalled_engrams = input.recalled_engrams.len(),
        "respond_inner entry"
    );

    // 1. Shared analysis (cached per message+room+history fingerprint).
    //    Provides matched-angle hints for the prompt — informational,
    //    NOT gating. The persona's own model is the only thing that
    //    decides what to say (or whether to stay quiet).
    //
    // analyze() returns Result<_, AnalysisError> as of #1207. We map
    // back to String here at the boundary because response.rs's own
    // public surface still uses Result<_, String>; pushing the typed
    // error up further is a follow-up (would touch persona::respond
    // signature + IPC handler + recorder traces). For now the typed
    // info is preserved in logs via Display.
    let analyze_start = EpochMs::now().millis();
    // `time_probe!` wraps the analyze future in an `info_span!` with
    // `probe_class = "timing", seam = "persona.respond.analyze"` so
    // operators tailing the JSONL can baseline analyze latency on the
    // canonical timing channel, alongside the outer `persona.respond`
    // turn timing and the inner `persona.respond.run_render` LLM call.
    // The existing `trace.record(SEAM_ANALYZE, ...)` + structured
    // `probe!(class = "persona.response.analyze.result", ...)` events
    // stay — they carry semantic data (from_cache, matched_angle,
    // intent) that the bare timing probe doesn't.
    let analysis = continuum_base::time_probe!(
        "persona.respond.analyze",
        analyze(AnalysisInput {
            message_id: input.message_id,
            room_id: input.turn_context.room_id,
            text: input.message_text.clone(),
            // These two are the only field-level clones still on the
            // analyze path. PR-2 (continuum#1206 follow-up) will rework
            // AnalysisInput to also accept &TurnContext directly so the
            // clone goes away here too — but that ripples into the
            // shared_analysis cache key, separate concern.
            recent_history: input.turn_context.recent_history.clone(),
            known_specialties: input.turn_context.known_specialties.clone(),
            // Pass the responding persona's model as the analyzer's
            // model override. Per Joel 2026-06-03 ("It's up to the
            // model"): the analyzer doesn't know what's loaded on this
            // substrate; the persona's profile does. On multi-persona
            // rooms where the canonical shared base IS loaded, the
            // first persona to analyze populates the single-flight
            // cache and the rest hit-as-cache regardless of override —
            // so this doesn't break the "ONE inference per message"
            // optimization. On single-persona substrates (like Joel's
            // LCD Intel Mac) the override becomes the path that makes
            // analysis reachable at all.
            model_override: Some(input.model.clone()),
        })
    )
    .map_err(|e| e.to_string())?;
    trace.record(
        SEAM_ANALYZE,
        analyze_start,
        EpochMs::now().millis().saturating_sub(analyze_start),
        serde_json::json!({
            "from_cache": analysis.from_cache,
            "model_used": analysis.model_used,
            "duration_ms_internal": analysis.duration_ms,
        }),
    );

    // RTOS-debugger breakpoint: what the analyze stage gave THIS
    // persona to work with. The matched-angle is the substrate's
    // signal that THIS persona's specialty is relevant — empty
    // string means "no specific perspective for you in this turn",
    // which materially shapes the render below.
    let matched_angle_for_probe = analysis
        .suggested_angles
        .get(&input.persona.specialty)
        .cloned()
        .unwrap_or_default();
    continuum_base::probe!(
        class = "persona.response.analyze.result",
        persona = %input.persona.display_name,
        specialty = %input.persona.specialty,
        from_cache = analysis.from_cache,
        model_used = %analysis.model_used,
        analyze_duration_ms = EpochMs::now().millis().saturating_sub(analyze_start),
        suggested_angles_count = analysis.suggested_angles.len(),
        matched_angle_present = !matched_angle_for_probe.is_empty(),
        matched_angle_len = matched_angle_for_probe.len(),
        intent = ?analysis.intent,
        "analyze result for persona"
    );

    // 2. Render. No external "should this persona respond" gate. Joel
    //    rule (2026-04-22): personas emulate humans — they choose
    //    themselves whether to engage. The earlier `score_persona`
    //    + suggested_angles[specialty] vetoed vision-capable personas
    //    on image-bearing messages because the analyzer's text-domain
    //    map didn't tag "general" as relevant — silenced the only
    //    persona that could SEE the image. Mechanical routing
    //    masquerading as cognition. Removed.
    //
    //    A persona may still emit Silence as its OWN cognitive
    //    output (its model returns "I'll pass on this one" or
    //    similar) — that's organic. What's gone is the external
    //    veto that decided FOR the persona.
    //
    //    `analysis.suggested_angles` remains as a prompt-time hint:
    //    if the analyzer extracted a per-specialty angle, the prompt
    //    assembler injects it; if not, the persona just sees the
    //    plain message + history + media, same as a human.
    let inference_start = EpochMs::now().millis();
    // `time_probe!` for the LLM bulk — the dominant cost of most
    // turns and the primary optimization target (see task #195 +
    // probe sprinkles #206/#207). Lands on the same JSONL channel as
    // `persona.respond` / `persona.respond.analyze` for full-stack
    // turn breakdowns.
    let raw_response =
        continuum_base::time_probe!("persona.respond.run_render", run_render(input, &analysis))?;
    let inference_ms = EpochMs::now().millis().saturating_sub(inference_start);
    trace.record(
        SEAM_INFERENCE,
        inference_start,
        inference_ms,
        serde_json::json!({
            "model_used": raw_response.model_used,
            "raw_text_chars": raw_response.text.len(),
            "media_attached": input.message_media.len(),
        }),
    );

    // RTOS-debugger breakpoint: what came OUT of the LLM. The
    // single most diagnostic snapshot in the cognition cycle —
    // every bug report about persona behavior ultimately compares
    // "what the model produced" against "what it should have
    // produced." Capture the raw text in full (truncation lives
    // in the operator's jq query, not here) so a later replay
    // can reconstruct the model's actual output verbatim. Per
    // [[jtag-probes-are-rtos-debugger]]: "what's going in and
    // out of an LLM."
    continuum_base::probe!(
        class = "persona.response.render.raw",
        persona = %input.persona.display_name,
        model_used = %raw_response.model_used,
        raw_text_len = raw_response.text.len(),
        raw_text = %raw_response.text,
        inference_ms = inference_ms,
        "LLM produced raw output"
    );

    let post_start = EpochMs::now().millis();
    let (think_stripped_text, think_count) = strip_thinks_emit_events(
        &raw_response.text,
        input.persona.persona_id,
        input.message_id,
    );
    let visible_text = strip_leaked_tool_markup(&think_stripped_text);
    trace.record(
        SEAM_POST_PROCESS,
        post_start,
        EpochMs::now().millis().saturating_sub(post_start),
        serde_json::json!({
            "think_blocks": think_count,
            "leaked_markup_chars_stripped": think_stripped_text.len().saturating_sub(visible_text.len()),
            "visible_chars": visible_text.len(),
        }),
    );

    let total_ms = EpochMs::now().millis().saturating_sub(total_start);

    // Silence-affordance recognition. `prompt_assembly::assemble`
    // appends `SILENCE_AFFORDANCE_BLOCK` to every persona's system
    // prompt, teaching the brain that replying with the single
    // word "PASS" means "I have nothing valuable to add this turn."
    // When the brain CHOOSES that signal, the substrate honors it:
    // PersonaResponse::Silent instead of Spoke. The brain decides;
    // the substrate just recognizes the documented signal.
    //
    // Doctrine `[[no-rust-gates-around-cognition]]`: this is NOT a
    // Rust-side gate. The condition is whether the brain's own
    // output matches the documented silence vocabulary — the
    // decision was made inside the LLM, not by us.
    if crate::cognition::prompt_assembly::looks_like_silence_token(&visible_text) {
        // RTOS-debugger breakpoint: the persona chose silence via
        // the PASS affordance. The `reason` is observable so a
        // training loop can analyze when personas use the signal
        // (good silences vs unnecessary ones).
        continuum_base::probe!(
            class = "persona.response.exit.silent",
            persona = %input.persona.display_name,
            persona_id = %input.persona.persona_id,
            message_id = %input.message_id,
            raw_text_len = raw_response.text.len(),
            raw_text = %raw_response.text,
            model_used = %raw_response.model_used,
            total_ms = total_ms,
            inference_ms = inference_ms,
            "persona chose silence (PASS)"
        );
        return Ok(PersonaResponse::Silent {
            persona_id: input.persona.persona_id,
            reason: "persona chose silence via PASS affordance".to_string(),
            relevance_score: 0.0,
        });
    }

    // RTOS-debugger breakpoint: the persona's final answer to
    // "what does this turn produce?" Pair with `persona.response.enter`
    // (same persona_id + message_id) for a complete turn record.
    continuum_base::probe!(
        class = "persona.response.exit.spoke",
        persona = %input.persona.display_name,
        persona_id = %input.persona.persona_id,
        message_id = %input.message_id,
        visible_text_len = visible_text.len(),
        visible_text = %visible_text,
        think_blocks = think_count,
        model_used = %raw_response.model_used,
        total_ms = total_ms,
        inference_ms = inference_ms,
        "spoke"
    );

    Ok(PersonaResponse::Spoke {
        persona_id: input.persona.persona_id,
        text: visible_text,
        model_used: raw_response.model_used,
        inference_ms,
        total_ms,
        think_blocks_emitted: think_count,
    })
}

/// What the render step returns internally (private — public type is
/// `PersonaResponse`).
struct RawRenderOutput {
    text: String,
    model_used: String,
}

/// Runs the prompt-assembly + inference for one persona's render.
///
/// 1. Pulls the matched angle for THIS persona's specialty from the
///    shared analysis (the orchestrator's "what your perspective adds
///    here" signal).
/// 2. Calls `prompt_assembly::assemble()` (memento's pure function port
///    of the TS PromptAssembler) to build the system message + chat
///    history with proper time-gap markers, social-awareness blocks,
///    and the matched-angle injection.
/// 3. Selects an inference adapter via the global registry. Routes by
///    capability — `provider="local"` + `device=Gpu` lets the registry
///    pick DMR / Vulkan / whichever GPU adapter actually supports the
///    requested model. No hardcoded provider name. Hard error if
///    nothing matches (the existing rule: never silent CPU fallback).
/// 4. Calls `adapter.generate_text(...)` and returns the raw output.
///    `<think>` parsing happens in the caller (`respond()`).
async fn run_render(
    input: &RespondInput,
    analysis: &SharedAnalysis,
) -> Result<RawRenderOutput, String> {
    use crate::ai::adapter::InferenceDevice;
    use crate::ai::types::TextGenerationRequest;
    use crate::cognition::prompt_assembly::{assemble, HistoryMessage, PromptAssemblyInput};

    // 1. The matched angle for this persona's specialty. Empty string
    //    means "no specific angle" — assemble() handles that gracefully
    //    (no angle injection in the system prompt).
    let matched_angle = analysis
        .suggested_angles
        .get(&input.persona.specialty)
        .cloned()
        .unwrap_or_default();

    // 2. Convert RecentMessage → HistoryMessage. RecentMessage is
    //    intentionally minimal (analysis-only). The render uses what
    //    we have; if the chat path later wants role/timestamp distinction,
    //    extend RecentMessage and the conversion follows.
    let history: Vec<HistoryMessage> = input
        .turn_context
        .recent_history
        .iter()
        .map(|m| HistoryMessage {
            role: "user".to_string(),
            name: Some(m.sender_name.clone()),
            content: m.text.clone(),
            timestamp_ms: None,
        })
        .collect();

    let current_message = HistoryMessage {
        role: "user".to_string(),
        name: None,
        content: input.message_text.clone(),
        timestamp_ms: None,
    };

    // Multi-party chat shape comes from the model registry — single
    // source of truth per the OOP-adapter rule. Code never branches on
    // model name. Default applies if the registry has no row (e.g. a
    // brand-new cloud model not yet declared).
    let resolved_model =
        crate::model_registry::try_global().and_then(|reg| reg.model(&input.model).cloned());
    let multi_party_strategy = resolved_model
        .as_ref()
        .map(|m| m.multi_party_strategy.clone())
        .unwrap_or_default();
    // Per-model sampling (#76): the anti-loop decode knobs live on the Model
    // row (ModelSampling default = repeat_penalty 1.1 / repeat_last_n 320 /
    // frequency_penalty 0.3, the #181 anti-loop floor). The persona request
    // used to hardcode temperature 0.7 + repeat_penalty None — so llama.cpp
    // fell back to its no-penalty default and small models DEGENERATED within a
    // turn (Atlas looped one block 4× live, 2026-07-25). Apply the model's
    // profile (or the substrate floor when the row is absent) so EVERY persona
    // turn carries the anti-loop guard, data-driven, no hardcoded magic.
    let sampling = resolved_model
        .as_ref()
        .map(|m| m.sampling)
        .unwrap_or_default();

    // Capture probe signals BEFORE moving matched_angle + history
    // into PromptAssemblyInput. Bool + usize are Copy; the probe
    // below reads from these locals after the move.
    let matched_angle_present_for_probe = !matched_angle.is_empty();
    let history_count_for_probe = history.len();

    let prompt_input = PromptAssemblyInput {
        persona_name: input.persona.display_name.clone(),
        system_prompt: input.system_prompt.clone(),
        matched_angle,
        history,
        current_message,
        is_voice: input.is_voice,
        social_signals: None,
        multi_party_strategy,
        other_persona_names: input.other_persona_names.clone(),
        // Recalled engrams populated by the IPC layer post-admission
        // (continuum#1211 PR-2). respond() is just a pass-through —
        // caller decides how many engrams to recall (sensible default
        // is 5-10, see modules/cognition.rs cognition/respond
        // handler). Empty when admission was skipped or persona has
        // no memory yet.
        recalled_engrams: input.recalled_engrams.clone(),
        // Room roster projected from RoomRosterSource by the caller.
        // Pass-through, same as engrams — respond() is the assembly
        // boundary, not the policy layer.
        room_roster: input.room_roster.clone(),
        // Room doctrine projected from RoomDoctrineSource. Pass-through.
        room_doctrine: input.room_doctrine.clone(),
    };

    let assembled = assemble(&prompt_input);

    // RTOS-debugger breakpoint: what's going INTO the LLM. Captures
    // the assembled prompt verbatim so an operator can read exactly
    // what the model was asked to do — the most informative single
    // snapshot for cognition bugs (missing instructions, drifted
    // template, wrong angle injection, social-block absence). Per
    // [[jtag-probes-are-rtos-debugger]]: "what's going in and out
    // of an LLM." Going-in lives here; going-out lives in the
    // `persona.response.render.raw` probe above.
    continuum_base::probe!(
        class = "persona.response.render.prompt",
        persona = %input.persona.display_name,
        specialty = %input.persona.specialty,
        model = %input.model,
        system_message_len = assembled.system_message.len(),
        system_message = %assembled.system_message,
        message_count = assembled.messages.len(),
        estimated_tokens = assembled.estimated_tokens,
        matched_angle_present = matched_angle_present_for_probe,
        engrams_count = input.recalled_engrams.len(),
        history_count = history_count_for_probe,
        "prompt assembled for inference"
    );

    // 3. Build the inference request from the assembled prompt.
    //
    // Native multimodal: if the caller passed media AND the persona's
    // resolved model declares the matching sensory capability
    // (Vision for image, AudioInput for audio), we attach the media
    // DIRECTLY as `ContentPart::Image` / `ContentPart::Audio` on the
    // FINAL user-role message — the one carrying the current message.
    // The model sees / hears the source bytes, no description bridge.
    //
    // When the model lacks the capability we fall through to the
    // text-only path. The sensory bridge (`VisionDescriptionService`,
    // STT) would inject a description upstream — that's the leveler
    // for genuinely text-only models, not the default route.
    //
    // See docs/architecture/PERSONA-CONTEXT-PAGING.md §0.5.X.
    //
    // Capabilities come WITH the request — no global registry lookup. The
    // prior shape (try_global → reg.model(&input.model)) silently returned
    // empty caps when the registry's lookup key didn't match `input.model`
    // verbatim; image bytes were already in `message_media` but the empty
    // caps demoted them to text markers, so the vision encoder never got
    // called even on a vision-capable persona. Caller-declared
    // capabilities removes the silent-drop seam (Joel rule:
    // "getThatThingIShouldHaveJustBeenGiven").
    let messages = build_messages_with_media(
        assembled.messages,
        &input.message_media,
        &input.capabilities,
    );

    let request = TextGenerationRequest {
        messages,
        system_prompt: Some(assembled.system_message),
        model: Some(input.model.clone()),
        provider: Some("local".to_string()),
        // Data-driven per-model sampling (#76 + #181 anti-loop): the Model row's
        // profile, NOT a scattered hardcoded 0.7. `repeat_penalty` +
        // `repeat_last_n` (windowed) and `frequency_penalty` (unwindowed) are
        // the anti-degeneration guard the persona path was missing.
        temperature: Some(sampling.temperature),
        // No cap. The adapter falls back to backend.n_ctx_train() when
        // None, giving the model its full trained context window.
        // Hardcoding 1024 here was clipping qwen3.5 mid-<think>, leaving
        // unterminated reasoning that leaked '<think>' into chat.
        max_tokens: None,
        reasoning_budget: None,
        top_p: Some(sampling.top_p),
        top_k: Some(sampling.top_k),
        repeat_penalty: Some(sampling.repeat_penalty),
        frequency_penalty: Some(sampling.frequency_penalty),
        repeat_last_n: Some(sampling.repeat_last_n),
        stop_sequences: None,
        tools: None,
        tail_tools: None,
        view_messages: None,
        tool_choice: None,
        response_format: None,
        active_adapters: None,
        request_id: None,
        user_id: None,
        room_id: Some(input.turn_context.room_id.to_string()),
        purpose: Some("persona-respond".to_string()),
        // The whole point of this request is to generate a response on
        // behalf of THIS persona — its KV bytes belong in this persona's
        // attribution bucket. Adapters that honor persona_id (LlamaCpp)
        // route the seq slot's KV into the FootprintRegistry under this
        // id; adapters that don't (DMR, cloud) ignore it.
        persona_id: Some(input.persona.persona_id.to_string()),
        turn_bound: None,
    };

    // #108 STEP 2 (cross-grid sprint, LaneDecision contract stamped 2026-07-24):
    // the lane decision now runs on EVERY persona pre-inference path — the live
    // caller the remote adapter (BigMama's step 3) lands against. THIS slice
    // decides-and-RADIATES: the decision is computed from the live grid ledger +
    // governed VRAM and probed; dispatch honors only the Local arm (a Remote
    // decision is recorded for the glass box, then served locally) until step 4
    // branches the select to provider="airc-remote". Leasability is typed at the
    // seam: media on the turn needs THIS node's artifacts → LocalOnly; pure text
    // may cross the compute-lease boundary ([[compute-lease-boundary]]).
    let leasability = if input.message_media.is_empty() {
        crate::capacity::lease::Leasability::TextOnly
    } else {
        crate::capacity::lease::Leasability::LocalOnly
    };
    let lane = crate::capacity::lease::decide_render_lane(leasability);
    continuum_base::probe!(
        class = "capacity.lane_decision",
        persona = %input.persona.persona_id,
        model = input.model.as_str(),
        decision = ?lane,
        "pre-inference lane decision (step-2 slice: Remote is recorded, dispatch stays local until step 4)",
    );

    // 4. Pick an adapter via the global registry — capability-routed,
    //    no hardcoded provider name. "local" + Auto = "best available
    //    LOCAL adapter that honestly supports the requested model,
    //    on whatever device class it declares."
    //
    //    The previous shape passed `InferenceDevice::Gpu` here, which
    //    wrongly excluded CPU-only adapters from their OWN persona's
    //    cognition cycle on Intel Mac (LlamaCppAdapter built with
    //    `mac-cpu-only` declares Cpu; this select asked for Gpu and
    //    rejected it). Per Joel 2026-06-03 the cognition layer has
    //    no opinion on device — the persona's profile + adapter
    //    declare placement. Same fix in cognition/{generate_response,
    //    should_respond, validate_response, tool_embedding}.
    let registry_arc = crate::ai::adapter::global_registry();
    let registry = registry_arc.read().await;
    let (_provider_id, adapter) = registry
        .select(Some("local"), Some(&input.model), InferenceDevice::Auto)
        .ok_or_else(|| {
            format!(
                "no GPU adapter supports model '{}' (registered: {:?}). \
                 Pull into DMR or install the right backend.",
                input.model,
                registry.available()
            )
        })?;

    let response = adapter.generate_text(request).await?;

    Ok(RawRenderOutput {
        text: response.text,
        model_used: response.model,
    })
}

/// Extract `<think>...</think>` blocks from the model's output. Emits
/// Convert assembled prompt messages into `ChatMessage`s, attaching any
/// caller-supplied `MediaItemLite`s as `ContentPart::Image`/`Audio` on
/// the FINAL user-role message — but only when the persona's resolved
/// model declares the matching capability (`Vision` for image,
/// `AudioInput` for audio). Native-multimodal models receive the source
/// bytes directly; text-only models fall back to the simple text path
/// (the sensory bridge would inject a description upstream — its job,
/// not ours).
///
/// Behavior contract:
///   - empty `media` → identical to the legacy text-only path.
///   - non-empty `media` + model has Vision/AudioInput → last user
///     message becomes `MessageContent::Parts(text + media)`.
///   - non-empty `media` + model lacks the capability → text-only
///     path; the bridge layer (VisionDescriptionService etc.) is
///     expected to have already converted media → text upstream.
///   - `media` items whose `item_type` doesn't match a capability the
///     model has are dropped (e.g. audio sent to a vision-only model).
///   - no user-role messages found → media silently dropped (rare —
///     would mean the assembler produced an unusual shape).
pub fn build_messages_with_media(
    prompt_messages: Vec<crate::cognition::prompt_assembly::PromptMessage>,
    media: &[MediaItemLite],
    model_caps: &std::collections::HashSet<crate::model_registry::Capability>,
) -> Vec<crate::ai::types::ChatMessage> {
    use crate::ai::types::{AudioInput, ChatMessage, ContentPart, ImageInput, MessageContent};
    use crate::persona::media_policy::MediaPolicy;

    // Default text-only path. Always start here; we may rewrite the
    // last user message below if the policy chose an attachable item.
    let mut messages: Vec<ChatMessage> = prompt_messages
        .into_iter()
        .map(|m| ChatMessage {
            role: m.role,
            content: MessageContent::Text(m.content),
            name: None,
        })
        .collect();

    if media.is_empty() {
        return messages;
    }

    // Apply the AT-MOST-ONE-LATEST policy. The byte-attachment slot
    // is exclusive — at most one media item ever rides as bytes per
    // inference call, and it's the LATEST item the model can natively
    // consume. Everything else (older items, items the model can't
    // natively consume) becomes a text description marker. This is
    // the architectural guard against the multi-encoder Metal brick
    // (each per-call mtmd context allocates ~2 GB; two concurrent
    // image attachments = two concurrent encoder ops = mouse-frozen
    // hard reset). See `persona/media_policy.rs` for the rule + tests.
    //
    // Joel rule (2026-04-22): "i would never let more than ONE message
    // deliver an image or tell the ais the image link". The policy
    // makes that rule a typed value, not a `for` loop.
    let plan = MediaPolicy::AtMostOneLatest.plan(media, model_caps);

    let mut emitted_parts: Vec<ContentPart> = Vec::with_capacity(plan.descriptions.len() + 1);

    // Bytes slot first (when present). Marker placement: the byte
    // attachment goes BEFORE description markers so the model
    // encounters the real sensory input before any text fallback for
    // older media. mtmd_tokenize splices the model's media marker at
    // ContentPart::Image position; description markers are inert.
    if let Some(item) = plan.attachable {
        let part = match item.item_type.as_str() {
            "image" => ContentPart::Image {
                image: ImageInput {
                    url: None,
                    base64: item.base64.clone(),
                    mime_type: item.mime_type.clone(),
                },
            },
            "audio" => ContentPart::Audio {
                audio: AudioInput {
                    url: None,
                    base64: item.base64.clone(),
                    mime_type: item.mime_type.clone(),
                },
            },
            // Policy guarantees attachable is natively-supported, so
            // any other branch is a contract violation. Falling
            // through silently would resurrect the silent-drop bug
            // we're refactoring away — make it loud instead.
            other => unreachable!(
                "MediaPolicy returned attachable item with unsupported type '{other}' — \
                 is_natively_supported is out of sync with the ContentPart variants here"
            ),
        };
        emitted_parts.push(part);
    }

    // Description markers for everything else. Pre-computed
    // `description` (from the upstream sensory bridge) gets used when
    // present; otherwise a do-not-speculate marker signals "an
    // attachment exists, you can't see it, do not invent content".
    // The marker is deliberately unhelpful — we don't want text-only
    // models inventing details from prompt context (verified
    // 2026-04-21: text-only personas hallucinated "kitten upright and
    // alert" given zero info, dropped into loop-spam patterns).
    for item in &plan.descriptions {
        let other = item.item_type.as_str();
        let text = match item.description.as_deref() {
            Some(d) if !d.trim().is_empty() => format!("[Attached {other}: {d}]"),
            _ => format!(
                "[Attached {other} — no description available; \
                 do not describe or speculate about its contents]"
            ),
        };
        emitted_parts.push(ContentPart::Text { text });
    }

    if emitted_parts.is_empty() {
        return messages;
    }

    // Find the LAST user-role message and convert it to Parts (text +
    // attached media). The current message is always the last user
    // turn after assemble().
    let last_user_idx = messages.iter().rposition(|m| m.role == "user");
    let Some(idx) = last_user_idx else {
        // No user message to attach to. Drop media silently — caller
        // shape was unusual; assembling new user messages here would
        // hide the actual bug.
        return messages;
    };

    let existing_text = match &messages[idx].content {
        MessageContent::Text(t) => t.clone(),
        // Defensive: if the assembler somehow already produced Parts,
        // we don't try to merge — leave it alone.
        MessageContent::Parts(_) => return messages,
    };

    let mut parts: Vec<ContentPart> = Vec::with_capacity(emitted_parts.len() + 1);
    if !existing_text.is_empty() {
        parts.push(ContentPart::Text {
            text: existing_text,
        });
    }
    parts.extend(emitted_parts);
    messages[idx].content = MessageContent::Parts(parts);
    messages
}

mod sanitize;
#[cfg(test)]
mod tests;

use sanitize::{strip_leaked_tool_markup, strip_thinks_emit_events};
