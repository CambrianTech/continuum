//! Unified Persona Evaluator — ALL pre-response gates in one call.
//!
//! Consolidates 5 sequential TypeScript gates + Rust fast-path into a single
//! `full_evaluate()` function. One IPC call, <1ms, zero GC.
//!
//! Gate order (short-circuits on first SILENT):
//! 1. Sleep mode — checks SleepMode + topic similarity (persona's own opt-out)
//! 2. Undirected persona chatter — one persona turn must not recursively summon another
//! 3. Self-message — infinite loop prevention (inside fast_path)
//! 4. Fast-path decision — delegates to PersonaCognitionEngine::fast_path_decision
//!
//! Note: response_count is collected as a SIGNAL (LLM sees it in social_signals
//! and can self-quiet if a conversation is getting too noisy) but is NOT a hard
//! gate. Local models have no cost; cloud rate limits belong at the provider
//! layer (their billing + RPS quotas), not as a hardcoded 50-message veto here.
//! The previous response_cap gate was a cloud-provider concern that leaked onto
//! every persona including local ones — removed to honor "LLM decides, not dumb
//! heuristics" (the philosophy this module already preaches).
//!
//! Types exported to TypeScript via ts-rs.
//!
//! # Module layout (continuum#1208)
//!
//! Split out of a single 1231-LOC file into focused submodules:
//! - [`sleep_state`] — `SleepMode` + `SleepState` (Gate 1 input)
//! - [`rate_limiter`] — `RateLimiterState` + `RoomRateState` (signal source)
//! - [`adequacy`] — post-inference response-adequacy check (`check_response_adequacy`)
//!
//! This module (the gate orchestrator) owns `FullEvaluateRequest`,
//! `FullEvaluateResult`, `GateDetails`, `SocialSignals`, and the
//! `full_evaluate` function that composes the submodules' state. Submodule
//! types are re-exported at the parent path so existing callers don't
//! see the move.

pub mod adequacy;
pub mod rate_limiter;
pub mod sleep_state;

pub use adequacy::{check_response_adequacy, AdequacyResult, RecentResponse};
pub use rate_limiter::{RateLimiterState, RoomRateState};
pub use sleep_state::{SleepMode, SleepState};

use crate::persona::cognition::PersonaCognitionEngine;
use crate::persona::message_cache::RecentMessageCache;
use crate::persona::text_analysis;
use crate::persona::types::{InboxMessage, Modality, SenderType};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use ts_rs::TS;
use uuid::Uuid;

// =============================================================================
// REQUEST / RESULT TYPES (ts-rs exported)
// =============================================================================

/// Full evaluation request — ONE IPC call replaces 5 TS gates.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/FullEvaluateRequest.ts"
)]
pub struct FullEvaluateRequest {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    pub persona_name: String,
    /// Defaults to `""` when the caller omits it — matches the legacy
    /// `p.str_or("persona_unique_id", "")` read the typed command replaces.
    #[serde(default)]
    pub persona_unique_id: String,
    #[ts(type = "string")]
    pub message_id: Uuid,
    #[ts(type = "string")]
    pub room_id: Uuid,
    #[ts(type = "string")]
    pub sender_id: Uuid,
    pub sender_name: String,
    pub sender_type: SenderType,
    pub content: String,
    #[ts(type = "number")]
    pub timestamp: u64,
    /// Defaults to `false` when omitted — matches the legacy `p.bool_or("is_voice", false)`.
    #[serde(default)]
    pub is_voice: bool,
    #[ts(optional, type = "string")]
    pub voice_session_id: Option<Uuid>,
    /// Defaults to `false` when omitted — matches the legacy `p.bool_or("sender_is_human", false)`.
    #[serde(default)]
    pub sender_is_human: bool,
    /// Pre-computed topic similarity for sleep mode (optional).
    /// If not provided and sleep mode is until_topic, we compute inline.
    #[ts(optional)]
    pub topic_similarity: Option<f32>,
    /// Recent room message texts for topic detection (optional).
    /// Only needed if persona is in until_topic sleep mode.
    #[ts(optional)]
    pub recent_room_texts: Option<Vec<String>>,
}

/// Full evaluation result — every gate's outcome in one response.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/FullEvaluateResult.ts"
)]
pub struct FullEvaluateResult {
    pub should_respond: bool,
    pub confidence: f32,
    pub reason: String,
    /// Which gate decided: response_cap, sleep_mode, self_message, fast_path, deferred_llm
    pub gate: String,
    #[ts(type = "number")]
    pub decision_time_ms: f64,
    #[ts(optional)]
    pub gate_details: Option<GateDetails>,
    /// Social awareness signals — passed to LLM as context, NOT used as vetoes.
    /// The LLM decides whether to respond based on these signals.
    #[ts(optional)]
    pub social_signals: Option<SocialSignals>,
}

/// Social awareness signals collected by Rust (microsecond-fast).
/// These are INFORMATION for the LLM, not gates. The LLM sees these
/// and makes its own social decision about whether to speak.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/SocialSignals.ts"
)]
pub struct SocialSignals {
    /// How many AI messages in this room in the last 2 minutes
    #[ts(type = "number")]
    pub ai_messages_recent: u32,
    /// Whether a human has spoken in the last 2 minutes
    pub human_spoke_recently: bool,
    /// Whether this persona is directly mentioned (@name)
    pub is_mentioned: bool,
    /// Whether the message mentions ANY persona by name (@someone)
    pub has_directed_mention: bool,
    /// Seconds since this persona's last response in this room (None = never responded)
    #[ts(optional)]
    pub seconds_since_last_response: Option<f64>,
    /// How many times this persona has responded in this room this session
    #[ts(optional, type = "number")]
    pub response_count_this_session: Option<u32>,
    /// Response cap for this session
    #[ts(optional, type = "number")]
    pub response_cap: Option<u32>,
}

/// Detailed gate information for diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/GateDetails.ts"
)]
pub struct GateDetails {
    #[ts(optional, type = "number")]
    pub response_count: Option<u32>,
    #[ts(optional, type = "number")]
    pub max_responses: Option<u32>,
    #[ts(optional)]
    pub rate_limit_wait_seconds: Option<f64>,
    #[ts(optional)]
    pub sleep_mode: Option<SleepMode>,
    #[ts(optional)]
    pub is_mentioned: Option<bool>,
    #[ts(optional)]
    pub has_directed_mention: Option<bool>,
    #[ts(optional)]
    pub topic_similarity: Option<f32>,
    /// Echo chamber: AI message count in window
    #[ts(optional, type = "number")]
    pub echo_chamber_ai_count: Option<u32>,
}

// =============================================================================
// UNIFIED EVALUATOR
// =============================================================================

/// Evaluate a message: collect social signals, apply only hard safety gates.
///
/// PHILOSOPHY: The LLM is the intelligence. Rust collects signals at microsecond
/// speed and passes them to the LLM as awareness. Only TRUE safety gates can block.
///
/// Hard gates (system protection only):
/// 1. Sleep mode — persona's OWN voluntary decision (respects autonomy)
/// 2. Undirected persona chatter — one persona turn completes the room turn
/// 3. Non-human echo storm — undirected AI/agent chatter is suppressed once
///    the room is already AI-heavy
/// 4. Self-message — infinite loop prevention (inside fast_path)
///
/// Removed: response cap. Was a cloud-provider "resource exhaustion" concept
/// that blocked local personas (which have zero cost) after 50 responses per
/// session per room. Response count is still collected as a signal so the LLM
/// can choose to self-quiet when appropriate — but the decision is the LLM's,
/// not a hardcoded counter's. Cloud rate-limit enforcement belongs at the
/// provider adapter level (OpenAI RPS, Anthropic TPM, etc.), not in the
/// universal evaluator.
///
/// Signals (collected, passed to LLM as context — NOT vetoes):
/// - Echo chamber metrics (AI count, human recency)
/// - Rate/cadence info (time since last response, response count)
/// - Mention detection (is this message directed at me? at someone else?)
/// - Priority score
pub fn full_evaluate(
    request: &FullEvaluateRequest,
    rate_limiter: &RateLimiterState,
    sleep_state: &SleepState,
    engine: &PersonaCognitionEngine,
    message_cache: &RecentMessageCache,
    now_ms: u64,
) -> FullEvaluateResult {
    let start = Instant::now();

    // =========================================================================
    // SIGNAL COLLECTION (fast, no blocking — all become LLM context)
    // =========================================================================
    let is_mentioned = text_analysis::is_persona_mentioned(
        &request.content,
        &request.persona_name,
        &request.persona_unique_id,
    );
    let has_directed_mention = text_analysis::has_directed_mention(&request.content);

    let echo_result = message_cache.check_echo_chamber(
        request.room_id,
        request.sender_is_human,
        is_mentioned,
        now_ms,
    );

    let response_count = rate_limiter.response_count(request.room_id);
    let seconds_since_last = rate_limiter
        .rooms
        .get(&request.room_id)
        .map(|r| (now_ms - r.last_response_time_ms) as f64 / 1000.0);

    let social_signals = SocialSignals {
        ai_messages_recent: echo_result.ai_message_count as u32,
        human_spoke_recently: echo_result.has_human_recently,
        is_mentioned,
        has_directed_mention,
        seconds_since_last_response: seconds_since_last,
        response_count_this_session: Some(response_count),
        response_cap: Some(rate_limiter.max_responses_per_session),
    };

    // =========================================================================
    // HARD GATE 1: Sleep mode (persona's OWN voluntary decision — respects autonomy)
    // =========================================================================
    // Note: response_count is available in social_signals above. Personas that
    // want to self-quiet after N turns can read their own response_count from
    // the social-awareness block and choose silence — that's the LLM deciding,
    // not a dumb heuristic. No hardcoded response_cap gate anymore.
    let effective_sleep = sleep_state.effective_mode(now_ms);
    if effective_sleep != SleepMode::Active {
        let should_respond_in_sleep = match effective_sleep {
            SleepMode::Active => true,
            SleepMode::MentionedOnly => is_mentioned,
            SleepMode::HumanOnly => request.sender_is_human,
            SleepMode::Sleeping => false,
            SleepMode::UntilTopic => {
                let topic_sim = request.topic_similarity.unwrap_or_else(|| {
                    if let Some(ref texts) = request.recent_room_texts {
                        if texts.is_empty() {
                            return 0.0;
                        }
                        let combined = texts.join(" ");
                        text_analysis::jaccard_ngram_similarity(&request.content, &combined) as f32
                    } else {
                        0.5
                    }
                });
                topic_sim < 0.3
            }
        };

        if !should_respond_in_sleep {
            return FullEvaluateResult {
                should_respond: false,
                confidence: 1.0,
                reason: format!(
                    "Voluntary sleep mode: {:?} (isHuman={}, isMention={})",
                    effective_sleep, request.sender_is_human, is_mentioned
                ),
                gate: "sleep_mode".into(),
                decision_time_ms: start.elapsed().as_secs_f64() * 1000.0,
                gate_details: Some(GateDetails {
                    response_count: None,
                    max_responses: None,
                    rate_limit_wait_seconds: None,
                    sleep_mode: Some(effective_sleep),
                    is_mentioned: Some(is_mentioned),
                    has_directed_mention: Some(has_directed_mention),
                    topic_similarity: request.topic_similarity,
                    echo_chamber_ai_count: Some(echo_result.ai_message_count as u32),
                }),
                social_signals: Some(social_signals),
            };
        }
    }

    // =========================================================================
    // HARD GATE 2: Undirected persona chatter.
    //
    // A persona response is already a completed room turn. Letting every other
    // persona evaluate it recreates the observed echo chain:
    // human → Teacher → Helper copies Teacher → Teacher summarizes Helper...
    //
    // Direct mentions still flow through. Agents are not blocked here because
    // bridged humans/coding agents enter as SenderType::Agent and are allowed
    // to intentionally feed Continuum over AIRC or other transports.
    // =========================================================================
    if request.sender_type == SenderType::Persona && !is_mentioned {
        return FullEvaluateResult {
            should_respond: false,
            confidence: 1.0,
            reason: "Undirected persona message completes the room turn".into(),
            gate: "persona_turn_complete".into(),
            decision_time_ms: start.elapsed().as_secs_f64() * 1000.0,
            gate_details: Some(GateDetails {
                response_count: Some(response_count),
                max_responses: Some(rate_limiter.max_responses_per_session),
                rate_limit_wait_seconds: rate_limiter
                    .rate_limit_wait_seconds(request.room_id, now_ms),
                sleep_mode: None,
                is_mentioned: Some(is_mentioned),
                has_directed_mention: Some(has_directed_mention),
                topic_similarity: None,
                echo_chamber_ai_count: Some(echo_result.ai_message_count as u32),
            }),
            social_signals: Some(social_signals),
        };
    }

    // =========================================================================
    // HARD GATE 3: Non-human echo storm.
    //
    // Agent/system broadcasts can intentionally start a Continuum turn, but if
    // the room is already AI-heavy and the message is not directed, suppress it
    // before it wakes every persona.
    // =========================================================================
    let sender_is_non_human = matches!(
        request.sender_type,
        SenderType::Persona | SenderType::Agent | SenderType::System
    );
    if sender_is_non_human && !is_mentioned && echo_result.ai_message_count >= 2 {
        return FullEvaluateResult {
            should_respond: false,
            confidence: 1.0,
            reason: format!(
                "Undirected non-human chatter suppressed after {} recent AI messages",
                echo_result.ai_message_count
            ),
            gate: "non_human_echo_storm".into(),
            decision_time_ms: start.elapsed().as_secs_f64() * 1000.0,
            gate_details: Some(GateDetails {
                response_count: Some(response_count),
                max_responses: Some(rate_limiter.max_responses_per_session),
                rate_limit_wait_seconds: rate_limiter
                    .rate_limit_wait_seconds(request.room_id, now_ms),
                sleep_mode: None,
                is_mentioned: Some(is_mentioned),
                has_directed_mention: Some(has_directed_mention),
                topic_similarity: None,
                echo_chamber_ai_count: Some(echo_result.ai_message_count as u32),
            }),
            social_signals: Some(social_signals),
        };
    }

    // =========================================================================
    // FAST-PATH (self-message = hard block, everything else passes through)
    // =========================================================================
    let priority = engine.calculate_priority(
        &request.content,
        request.sender_type,
        request.is_voice,
        request.room_id,
        request.timestamp,
    );

    let inbox_msg = InboxMessage {
        id: request.message_id,
        room_id: request.room_id,
        sender_id: request.sender_id,
        sender_name: request.sender_name.clone(),
        sender_type: request.sender_type,
        content: request.content.clone(),
        timestamp: request.timestamp,
        priority: priority.score,
        source_modality: if request.is_voice {
            Some(Modality::Voice)
        } else {
            Some(Modality::Chat)
        },
        voice_session_id: request.voice_session_id,
    };

    let fast_path = engine.fast_path_decision_no_dedup(&inbox_msg);

    FullEvaluateResult {
        should_respond: fast_path.should_respond,
        confidence: fast_path.confidence,
        reason: fast_path.reason,
        gate: if fast_path.fast_path_used {
            "fast_path".into()
        } else {
            "deferred_llm".into()
        },
        decision_time_ms: start.elapsed().as_secs_f64() * 1000.0,
        gate_details: Some(GateDetails {
            response_count: Some(response_count),
            max_responses: Some(rate_limiter.max_responses_per_session),
            rate_limit_wait_seconds: rate_limiter.rate_limit_wait_seconds(request.room_id, now_ms),
            sleep_mode: None,
            is_mentioned: Some(is_mentioned),
            has_directed_mention: Some(has_directed_mention),
            topic_similarity: None,
            echo_chamber_ai_count: Some(echo_result.ai_message_count as u32),
        }),
        social_signals: Some(social_signals),
    }
}

// =============================================================================
// BURST-AWARE EVALUATOR (task #248 PR C — demand-pull cognition path)
// =============================================================================

/// Result of evaluating a burst — analyzed at the cognition layer
/// ONCE per channel-tick (the demand-pull doctrine in production form).
///
/// Parallel to [`FullEvaluateResult`] but burst-aware: carries the
/// post-decision context (`respond_context`) cognition needs to compose
/// a response, so the response phase doesn't have to walk the burst's
/// items a second time.
///
/// Per `[[cognition-batches-per-channel-adapter]]`: cognition's
/// `analyze` fires ONCE per `Vec<CoherentInput>` from
/// `service_cycle_batched`, regardless of how many items each channel
/// drained. This struct is what that single call returns per channel.
#[derive(Debug, Clone)]
pub struct BurstEvaluateResult {
    /// Cognition's gate decision — same semantic as
    /// [`FullEvaluateResult::should_respond`].
    pub should_respond: bool,
    pub confidence: f32,
    pub reason: String,
    /// Which gate decided (same string-tag set as `full_evaluate`).
    pub gate: String,
    /// How many raw items the burst aggregated. Equal to
    /// `ChatCoherentInput::burst_message_count` when input is Chat.
    pub burst_message_count: usize,
    /// The room the burst came from. `Uuid::nil()` for non-Chat
    /// CoherentInput variants.
    pub primary_room: Uuid,
    /// If `should_respond` is true, this carries the prompt-assembly
    /// context. None when cognition decided silent. Lets the response
    /// phase avoid re-walking the burst.
    pub respond_context: Option<BurstRespondContext>,
}

/// Prompt-assembly context for a burst that cognition wants to
/// respond to. Mirrors the fields cognition's prompt assembler reads
/// from a single message but at burst granularity.
#[derive(Debug, Clone)]
pub struct BurstRespondContext {
    pub room_id: Uuid,
    /// Aggregated burst content — newline-joined "Sender: message"
    /// lines including consolidated_context. Matches
    /// `ChatCoherentInput::aggregated_content`.
    pub aggregated_content: String,
    pub last_sender_name: String,
    pub burst_message_count: usize,
    /// Identity-aware flag from `ChatChannelView::interpret`. Personas
    /// named in the burst see this true; others see false.
    pub anyone_mentioned_persona: bool,
}

/// Demand-pull cognition entry point — the PR C decision shape from
/// the design doc:
///
/// > Option 3: Add `analyze_burst(&CoherentInput) -> BurstDecision`
/// > alongside the existing per-item path. The new entry point IS the
/// > demand-pull doctrine; the old per-item path stays for
/// > compatibility, gets `#[deprecated]`'d, and gets ripped in PR C+1.
///
/// Per the doctrine: cognition's gate fires ONCE per channel-tick per
/// burst, regardless of how many items the burst aggregated. The
/// initial implementation maps a `Chat` burst into a synthetic
/// [`FullEvaluateRequest`] and reuses [`full_evaluate`] for the gate
/// logic — same trustworthy gate stack, batched input. Future
/// refactors can replace the synthetic-request path with a
/// burst-native gate implementation; the trait shape stays stable.
///
/// `Other` variants (Audio/Code/Background — domains without typed
/// views yet) return a silent decision; downstream cognition skips
/// them until typed views land (PR D for Audio).
pub fn analyze_burst(
    input: &crate::persona::channel_view::CoherentInput,
    persona_id: Uuid,
    persona_name: &str,
    persona_unique_id: &str,
    rate_limiter: &RateLimiterState,
    sleep_state: &SleepState,
    engine: &PersonaCognitionEngine,
    message_cache: &RecentMessageCache,
    now_ms: u64,
) -> BurstEvaluateResult {
    use crate::persona::channel_view::CoherentInput;

    match input {
        CoherentInput::Chat(chat) => {
            // Build a synthetic single-message request from the burst's
            // aggregated context. The doctrine is "ONE gate call per
            // burst" — the synthetic shape is the path from burst →
            // existing `full_evaluate`. PR C+1 can swap this for a
            // burst-native gate without changing the trait surface.
            //
            // `message_id` is a burst-anchor UUID derived per call so
            // downstream caches don't conflate two ticks' bursts on
            // the same room.
            let synthetic = FullEvaluateRequest {
                persona_id,
                persona_name: persona_name.to_string(),
                persona_unique_id: persona_unique_id.to_string(),
                message_id: Uuid::new_v4(),
                room_id: chat.primary_room,
                // Burst-level sender is the aggregate-of-senders; we
                // pin it to nil and let the LLM read individual
                // senders out of `aggregated_content`.
                sender_id: Uuid::nil(),
                sender_name: chat.last_sender_name.clone(),
                // Sender-type at burst level is a heuristic — if the
                // burst aggregated cross-sender messages, "human" is
                // the safe default (matches the legacy fast-path
                // interpretation of chat bursts).
                sender_type: SenderType::Human,
                content: chat.aggregated_content.clone(),
                timestamp: now_ms,
                is_voice: false,
                voice_session_id: None,
                sender_is_human: true,
                topic_similarity: None,
                recent_room_texts: None,
            };
            let inner = full_evaluate(
                &synthetic,
                rate_limiter,
                sleep_state,
                engine,
                message_cache,
                now_ms,
            );

            BurstEvaluateResult {
                should_respond: inner.should_respond,
                confidence: inner.confidence,
                reason: inner.reason,
                gate: inner.gate,
                burst_message_count: chat.burst_message_count,
                primary_room: chat.primary_room,
                respond_context: inner.should_respond.then(|| BurstRespondContext {
                    room_id: chat.primary_room,
                    aggregated_content: chat.aggregated_content.clone(),
                    last_sender_name: chat.last_sender_name.clone(),
                    burst_message_count: chat.burst_message_count,
                    anyone_mentioned_persona: chat.anyone_mentioned_persona,
                }),
            }
        }
        CoherentInput::Other {
            domain, item_count, ..
        } => {
            // Non-Chat domains drain into Other until their typed
            // views land (PR D for Audio). Cognition decides silent
            // for these bursts — no gate logic runs, no inference
            // would be wasted on a burst the substrate hasn't taught
            // us how to interpret yet. Per `[[no-fallbacks-ever]]`:
            // explicit silent decision with a typed reason, NOT a
            // fall-through to chat semantics that would mis-route.
            BurstEvaluateResult {
                should_respond: false,
                confidence: 0.0,
                reason: format!("non-Chat burst from {domain:?} — typed view not yet implemented"),
                gate: "other-domain-silent".into(),
                burst_message_count: *item_count,
                primary_room: Uuid::nil(),
                respond_context: None,
            }
        }
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::message_cache::{CachedMessage, SenderCategory};
    use crate::rag::RagEngine;
    use std::sync::Arc;
    use tokio::sync::watch;

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    fn test_engine(name: &str) -> (PersonaCognitionEngine, Uuid) {
        let rag_engine = Arc::new(RagEngine::new());
        let (_tx, rx) = watch::channel(false);
        let id = Uuid::new_v4();
        (
            PersonaCognitionEngine::new(id, name.into(), rag_engine, rx),
            id,
        )
    }

    fn test_request(persona_id: Uuid, persona_name: &str) -> FullEvaluateRequest {
        FullEvaluateRequest {
            persona_id,
            persona_name: persona_name.into(),
            persona_unique_id: "test-bot".into(),
            message_id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            sender_name: "test-user".into(),
            sender_type: SenderType::Human,
            content: "Hello everyone".into(),
            timestamp: now_ms(),
            is_voice: false,
            voice_session_id: None,
            sender_is_human: true,
            topic_similarity: None,
            recent_room_texts: None,
        }
    }

    #[test]
    fn test_response_count_is_signal_not_gate() {
        // Formerly test_gate_1_response_cap — asserted that hitting the cap
        // produced a hard gate=response_cap SILENT. That gate was removed:
        // local personas have no resource exhaustion, and cloud rate limits
        // belong at the provider layer. Response count is still collected as
        // a social signal so the LLM can choose to self-quiet, but it never
        // short-circuits the evaluator.
        let (engine, persona_id) = test_engine("TestBot");
        let request = test_request(persona_id, "TestBot");
        let sleep = SleepState::default();
        let mut rate_limiter = RateLimiterState::new(10.0, 3);

        let room_id = request.room_id;
        let now = now_ms();
        rate_limiter.track_response(room_id, now - 30_000);
        rate_limiter.track_response(room_id, now - 20_000);
        rate_limiter.track_response(room_id, now - 11_000); // 11s ago — well past rate-limit window

        let result = full_evaluate(
            &request,
            &rate_limiter,
            &sleep,
            &engine,
            &RecentMessageCache::new(),
            now,
        );
        // Gate MUST NOT be response_cap anymore.
        assert_ne!(
            result.gate, "response_cap",
            "response_cap was removed — count is a social signal now, not a hard gate"
        );
        // But the count MUST still flow into social_signals so the LLM can see it.
        let sigs = result
            .social_signals
            .expect("social_signals always populated");
        assert_eq!(sigs.response_count_this_session, Some(3));
        assert_eq!(sigs.response_cap, Some(3));
    }

    #[test]
    fn test_rate_limit_is_signal_not_gate() {
        // Rate limiting is now a SIGNAL, not a hard gate.
        // The LLM decides whether to respond — Rust just reports the info.
        let (engine, persona_id) = test_engine("TestBot");
        let request = test_request(persona_id, "TestBot");
        let sleep = SleepState::default();
        let mut rate_limiter = RateLimiterState::new(10.0, 50);

        let now = now_ms();
        // Response 5 seconds ago — within 10s window
        rate_limiter.track_response(request.room_id, now - 5_000);

        let result = full_evaluate(
            &request,
            &rate_limiter,
            &sleep,
            &engine,
            &RecentMessageCache::new(),
            now,
        );
        // NOT blocked — rate info passed as signal
        assert!(
            result.should_respond,
            "Rate limit should be a signal, not a veto"
        );
        // But the social signals should contain the rate info
        let signals = result.social_signals.unwrap();
        assert!(signals.seconds_since_last_response.unwrap() < 10.0);
        assert_eq!(signals.response_count_this_session, Some(1));
        // Gate details should also have rate_limit_wait_seconds
        let details = result.gate_details.unwrap();
        assert!(details.rate_limit_wait_seconds.unwrap() > 0.0);
    }

    #[test]
    fn test_gate_4_sleep_mode_sleeping() {
        let (engine, persona_id) = test_engine("TestBot");
        let request = test_request(persona_id, "TestBot");
        let sleep = SleepState {
            mode: SleepMode::Sleeping,
            reason: "Taking a break".into(),
            set_at_ms: now_ms() - 60_000,
            wake_at_ms: None,
        };
        let rate_limiter = RateLimiterState::default();

        let result = full_evaluate(
            &request,
            &rate_limiter,
            &sleep,
            &engine,
            &RecentMessageCache::new(),
            now_ms(),
        );
        assert!(!result.should_respond);
        assert_eq!(result.gate, "sleep_mode");
    }

    #[test]
    fn test_gate_4_sleep_mentioned_only_passes_when_mentioned() {
        let (engine, persona_id) = test_engine("TestBot");
        let mut request = test_request(persona_id, "TestBot");
        request.content = "@TestBot can you help?".into();
        let sleep = SleepState {
            mode: SleepMode::MentionedOnly,
            reason: "Focus mode".into(),
            set_at_ms: now_ms() - 60_000,
            wake_at_ms: None,
        };
        let rate_limiter = RateLimiterState::default();

        let result = full_evaluate(
            &request,
            &rate_limiter,
            &sleep,
            &engine,
            &RecentMessageCache::new(),
            now_ms(),
        );
        // Should pass sleep gate (mentioned) and reach fast_path
        assert!(result.should_respond);
        assert_ne!(result.gate, "sleep_mode");
    }

    #[test]
    fn test_gate_4_sleep_auto_wake() {
        let (engine, persona_id) = test_engine("TestBot");
        let request = test_request(persona_id, "TestBot");
        let now = now_ms();
        let sleep = SleepState {
            mode: SleepMode::Sleeping,
            reason: "Nap time".into(),
            set_at_ms: now - 3_600_000,    // 1 hour ago
            wake_at_ms: Some(now - 1_000), // Wake time already passed
        };
        let rate_limiter = RateLimiterState::default();

        let result = full_evaluate(
            &request,
            &rate_limiter,
            &sleep,
            &engine,
            &RecentMessageCache::new(),
            now,
        );
        // Should NOT be blocked by sleep — auto-wake expired
        assert_ne!(result.gate, "sleep_mode");
    }

    #[test]
    fn test_directed_mention_is_signal_not_gate() {
        // Directed mentions are now a SIGNAL, not a hard gate.
        // The LLM sees "message directed at @OtherBot" and decides for itself.
        let (engine, persona_id) = test_engine("TestBot");
        let mut request = test_request(persona_id, "TestBot");
        // Mentions someone else, NOT TestBot
        request.content = "@OtherBot please fix this bug".into();
        let sleep = SleepState::default();
        let rate_limiter = RateLimiterState::default();

        let result = full_evaluate(
            &request,
            &rate_limiter,
            &sleep,
            &engine,
            &RecentMessageCache::new(),
            now_ms(),
        );
        // NOT hard-blocked — the LLM will see the signal and decide
        let signals = result.social_signals.unwrap();
        assert!(!signals.is_mentioned, "TestBot is NOT mentioned");
        assert!(
            signals.has_directed_mention,
            "@OtherBot IS a directed mention"
        );
        // The fast_path may still block (AI sender low priority) but NOT because of directed mention gate
        assert_ne!(
            result.gate, "directed_mention",
            "directed_mention should not be a gate anymore"
        );
    }

    #[test]
    fn test_gate_6_fast_path_self_message() {
        let (engine, persona_id) = test_engine("TestBot");
        let mut request = test_request(persona_id, "TestBot");
        request.sender_id = persona_id; // Self-message
        let sleep = SleepState::default();
        let rate_limiter = RateLimiterState::default();

        let result = full_evaluate(
            &request,
            &rate_limiter,
            &sleep,
            &engine,
            &RecentMessageCache::new(),
            now_ms(),
        );
        assert!(!result.should_respond);
        assert_eq!(result.gate, "fast_path");
        assert!(result.reason.contains("Own message"));
    }

    #[test]
    fn test_gate_6_fast_path_human_high_priority() {
        let (engine, persona_id) = test_engine("TestBot");
        let request = test_request(persona_id, "TestBot");
        let sleep = SleepState::default();
        let rate_limiter = RateLimiterState::default();

        let result = full_evaluate(
            &request,
            &rate_limiter,
            &sleep,
            &engine,
            &RecentMessageCache::new(),
            now_ms(),
        );
        // Human sender + recent message = high priority → should respond
        assert!(result.should_respond);
    }

    #[test]
    fn test_non_human_echo_storm_blocks_undirected_agent_chatter() {
        let (engine, persona_id) = test_engine("TestBot");
        let mut request = test_request(persona_id, "TestBot");
        request.sender_type = SenderType::Agent;
        request.sender_is_human = false;
        request.sender_name = "airc-bridge".into();
        request.content = "[airc:mac-claude] please respond if you see this".into();

        let now = now_ms();
        let mut cache = RecentMessageCache::new();
        for i in 0..2 {
            cache.push(
                request.room_id,
                CachedMessage {
                    id: Uuid::new_v4(),
                    sender_id: Uuid::new_v4(),
                    sender_type: SenderCategory::AI,
                    sender_name: format!("Persona{i}"),
                    content_text: "Hello! How can I assist you today?".into(),
                    timestamp_ms: now - 1_000,
                },
            );
        }

        let result = full_evaluate(
            &request,
            &RateLimiterState::default(),
            &SleepState::default(),
            &engine,
            &cache,
            now,
        );

        assert!(!result.should_respond);
        assert_eq!(result.gate, "non_human_echo_storm");
    }

    #[test]
    fn test_undirected_persona_message_completes_turn_without_cache_warmup() {
        let (engine, persona_id) = test_engine("TestBot");
        let mut request = test_request(persona_id, "TestBot");
        request.sender_type = SenderType::Persona;
        request.sender_is_human = false;
        request.sender_name = "Teacher AI".into();
        request.content = "Teacher AI: Yes, I can see this startup smoke test.".into();

        let result = full_evaluate(
            &request,
            &RateLimiterState::default(),
            &SleepState::default(),
            &engine,
            &RecentMessageCache::new(),
            now_ms(),
        );

        assert!(!result.should_respond);
        assert_eq!(result.gate, "persona_turn_complete");
    }

    #[test]
    fn test_non_human_echo_storm_allows_direct_mentions() {
        let (engine, persona_id) = test_engine("TestBot");
        let mut request = test_request(persona_id, "TestBot");
        request.sender_type = SenderType::Agent;
        request.sender_is_human = false;
        request.sender_name = "airc-bridge".into();
        request.content = "@TestBot please respond if you see this".into();

        let now = now_ms();
        let mut cache = RecentMessageCache::new();
        for i in 0..5 {
            cache.push(
                request.room_id,
                CachedMessage {
                    id: Uuid::new_v4(),
                    sender_id: Uuid::new_v4(),
                    sender_type: SenderCategory::AI,
                    sender_name: format!("Persona{i}"),
                    content_text: "Hello! How can I assist you today?".into(),
                    timestamp_ms: now - 1_000,
                },
            );
        }

        let result = full_evaluate(
            &request,
            &RateLimiterState::default(),
            &SleepState::default(),
            &engine,
            &cache,
            now,
        );

        assert_ne!(result.gate, "non_human_echo_storm");
        assert!(result.social_signals.unwrap().is_mentioned);
    }

    #[test]
    fn test_gate_6_fast_path_mentioned_always_responds() {
        let (engine, persona_id) = test_engine("TestBot");
        let mut request = test_request(persona_id, "TestBot");
        request.content = "@TestBot what do you think?".into();
        request.sender_type = SenderType::Persona; // AI sender (normally lower priority)
        request.sender_is_human = false;
        let sleep = SleepState::default();
        let rate_limiter = RateLimiterState::default();

        let result = full_evaluate(
            &request,
            &rate_limiter,
            &sleep,
            &engine,
            &RecentMessageCache::new(),
            now_ms(),
        );
        assert!(result.should_respond);
    }

    #[test]
    fn test_all_gates_pass_normal_message() {
        let (engine, persona_id) = test_engine("TestBot");
        let request = test_request(persona_id, "TestBot");
        let sleep = SleepState::default();
        let rate_limiter = RateLimiterState::default();

        let result = full_evaluate(
            &request,
            &rate_limiter,
            &sleep,
            &engine,
            &RecentMessageCache::new(),
            now_ms(),
        );
        assert!(result.should_respond);
        // The wall-clock assertion that used to live here (`decision_time_ms < 10.0`)
        // is GONE, and deliberately not replaced with a looser bound. It failed CI at
        // 10.73ms — not because the gates regressed, but because a shared runner was
        // busy. A correctness test that a loaded machine can fail is not measuring the
        // code; it is measuring the machine, and it spends the reviewer's trust every
        // time it flakes. The performance claim it was making is real and worth
        // keeping, so it MOVED to the stress block below, where `cargo test` does not
        // adjudicate it (CLAUDE.md § test rules, item 2). Same treatment as the
        // grounding-cost flake in PR #2330. Siblings of this class still exist and are
        // NOT touched here because they are not failing and have 20-50x more headroom:
        // command_executor.rs (<500ms), rag/engine.rs (<250ms), sentinel/parallel.rs
        // (<180ms). If any of them starts flaking, this is the fix.
    }

    #[test]
    fn test_gate_4_until_topic_with_provided_similarity() {
        let (engine, persona_id) = test_engine("TestBot");
        let mut request = test_request(persona_id, "TestBot");
        // High similarity → continuation → should NOT respond in until_topic mode
        request.topic_similarity = Some(0.8);
        let sleep = SleepState {
            mode: SleepMode::UntilTopic,
            reason: "Waiting for new topic".into(),
            set_at_ms: now_ms() - 60_000,
            wake_at_ms: None,
        };
        let rate_limiter = RateLimiterState::default();

        let result = full_evaluate(
            &request,
            &rate_limiter,
            &sleep,
            &engine,
            &RecentMessageCache::new(),
            now_ms(),
        );
        assert!(!result.should_respond);
        assert_eq!(result.gate, "sleep_mode");
    }

    #[test]
    fn test_gate_4_until_topic_new_topic_passes() {
        let (engine, persona_id) = test_engine("TestBot");
        let mut request = test_request(persona_id, "TestBot");
        // Low similarity → new topic → should respond
        request.topic_similarity = Some(0.1);
        let sleep = SleepState {
            mode: SleepMode::UntilTopic,
            reason: "Waiting for new topic".into(),
            set_at_ms: now_ms() - 60_000,
            wake_at_ms: None,
        };
        let rate_limiter = RateLimiterState::default();

        let result = full_evaluate(
            &request,
            &rate_limiter,
            &sleep,
            &engine,
            &RecentMessageCache::new(),
            now_ms(),
        );
        // Should pass sleep gate (new topic) and reach fast_path
        assert_ne!(result.gate, "sleep_mode");
    }

    // RateLimiterState unit tests + the post-inference adequacy tests
    // moved to their respective submodules in continuum#1208:
    //   - rate_limiter::tests
    //   - adequacy::tests

    /// Performance claims about the gate path. Compile-time gated so a busy shared
    /// runner never adjudicates them (CLAUDE.md § test rules, item 2): default
    /// `cargo test` skips this block entirely, and it is run deliberately, on a quiet
    /// machine, when the claim is what you actually want to check.
    #[cfg(feature = "stress-tests")]
    mod stress {
        use super::*;

        // what this catches: the gate path taking a slow route — an added I/O call,
        // a lock, an inference hop. Every gate is a pure function over in-memory
        // state, so the decision is sub-millisecond work; a budget of 10ms is ~10x
        // headroom over that and still catches a category change. Measured over
        // repeated runs rather than one sample, because a single timing of a live
        // system is not a fact about it — one scheduler hiccup is not a regression.
        #[test]
        fn gate_path_stays_off_the_slow_route() {
            let (engine, persona_id) = test_engine("TestBot");
            let request = test_request(persona_id, "TestBot");
            let sleep = SleepState::default();
            let rate_limiter = RateLimiterState::default();

            const RUNS: usize = 50;
            let mut times: Vec<f64> = (0..RUNS)
                .map(|_| {
                    full_evaluate(
                        &request,
                        &rate_limiter,
                        &sleep,
                        &engine,
                        &RecentMessageCache::new(),
                        now_ms(),
                    )
                    .decision_time_ms
                })
                .collect();
            times.sort_by(|a, b| a.partial_cmp(b).expect("decision times are finite"));
            let median = times[RUNS / 2];
            assert!(
                median < 10.0,
                "median gate decision over {RUNS} runs should be <10ms, was {median}ms \
                 (slowest {}ms) — something on the gate path is doing real work",
                times[RUNS - 1]
            );
        }
    }
}
