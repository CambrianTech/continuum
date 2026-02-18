//! Unified Persona Evaluator — ALL pre-response gates in one call.
//!
//! Consolidates 5 sequential TypeScript gates + Rust fast-path into a single
//! `full_evaluate()` function. One IPC call, <1ms, zero GC.
//!
//! Gate order (short-circuits on first SILENT):
//! 1. Response cap — response_count >= max_responses → SILENT
//! 2. Mention detection — reuses text_analysis::mention_detection
//! 3. Rate limiting — per-room time window from RateLimiterState
//! 4. Sleep mode — checks SleepMode + topic similarity
//! 5. Directed mention filter — !is_mentioned && has_directed_mention → SILENT
//! 6. Fast-path decision — delegates to PersonaCognitionEngine::fast_path_decision
//!
//! Types exported to TypeScript via ts-rs.

use crate::persona::text_analysis;
use crate::persona::cognition::PersonaCognitionEngine;
use crate::persona::types::{InboxMessage, SenderType, Modality};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;
use ts_rs::TS;
use uuid::Uuid;

// =============================================================================
// SLEEP MODE (mirrors TypeScript PersonaSleepManager)
// =============================================================================

/// Voluntary sleep modes — persona controls own attention.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../shared/generated/persona/SleepMode.ts")]
pub enum SleepMode {
    Active,
    MentionedOnly,
    HumanOnly,
    Sleeping,
    UntilTopic,
}

impl Default for SleepMode {
    fn default() -> Self {
        SleepMode::Active
    }
}

/// Per-persona sleep state with optional auto-wake.
#[derive(Debug, Clone)]
pub struct SleepState {
    pub mode: SleepMode,
    pub reason: String,
    pub set_at_ms: u64,
    pub wake_at_ms: Option<u64>,
}

impl Default for SleepState {
    fn default() -> Self {
        Self {
            mode: SleepMode::Active,
            reason: String::new(),
            set_at_ms: 0,
            wake_at_ms: None,
        }
    }
}

impl SleepState {
    /// Check if auto-wake time has passed. Returns true if should wake.
    pub fn should_auto_wake(&self, now_ms: u64) -> bool {
        if let Some(wake_at) = self.wake_at_ms {
            now_ms >= wake_at
        } else {
            false
        }
    }

    /// Get effective mode, accounting for auto-wake.
    pub fn effective_mode(&self, now_ms: u64) -> SleepMode {
        if self.should_auto_wake(now_ms) {
            SleepMode::Active
        } else {
            self.mode
        }
    }
}

// =============================================================================
// RATE LIMITER STATE (mirrors TypeScript RateLimiter)
// =============================================================================

/// Per-room rate limiting state.
#[derive(Debug, Clone)]
pub struct RoomRateState {
    pub last_response_time_ms: u64,
    pub response_count: u32,
}

/// Per-persona rate limiter with per-room tracking.
#[derive(Debug, Clone)]
pub struct RateLimiterState {
    pub rooms: HashMap<Uuid, RoomRateState>,
    pub min_seconds_between_responses: f64,
    pub max_responses_per_session: u32,
}

impl Default for RateLimiterState {
    fn default() -> Self {
        Self {
            rooms: HashMap::new(),
            min_seconds_between_responses: 10.0,
            max_responses_per_session: 50,
        }
    }
}

impl RateLimiterState {
    pub fn new(min_seconds: f64, max_responses: u32) -> Self {
        Self {
            rooms: HashMap::new(),
            min_seconds_between_responses: min_seconds,
            max_responses_per_session: max_responses,
        }
    }

    /// Check if response cap reached for a room.
    pub fn has_reached_response_cap(&self, room_id: Uuid) -> bool {
        self.rooms.get(&room_id)
            .map(|r| r.response_count >= self.max_responses_per_session)
            .unwrap_or(false)
    }

    /// Check if rate limited for a room (time-based).
    pub fn is_rate_limited(&self, room_id: Uuid, now_ms: u64) -> bool {
        self.rooms.get(&room_id)
            .map(|r| {
                let elapsed_seconds = (now_ms - r.last_response_time_ms) as f64 / 1000.0;
                elapsed_seconds < self.min_seconds_between_responses
            })
            .unwrap_or(false)
    }

    /// Get seconds until rate limit expires. None if not limited.
    pub fn rate_limit_wait_seconds(&self, room_id: Uuid, now_ms: u64) -> Option<f64> {
        self.rooms.get(&room_id).and_then(|r| {
            let elapsed = (now_ms - r.last_response_time_ms) as f64 / 1000.0;
            if elapsed < self.min_seconds_between_responses {
                Some(self.min_seconds_between_responses - elapsed)
            } else {
                None
            }
        })
    }

    /// Track a response in a room.
    pub fn track_response(&mut self, room_id: Uuid, now_ms: u64) {
        let entry = self.rooms.entry(room_id).or_insert(RoomRateState {
            last_response_time_ms: 0,
            response_count: 0,
        });
        entry.last_response_time_ms = now_ms;
        entry.response_count += 1;
    }

    /// Get response count for a room.
    pub fn response_count(&self, room_id: Uuid) -> u32 {
        self.rooms.get(&room_id).map(|r| r.response_count).unwrap_or(0)
    }
}

// =============================================================================
// REQUEST / RESULT TYPES (ts-rs exported)
// =============================================================================

/// Full evaluation request — ONE IPC call replaces 5 TS gates.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/FullEvaluateRequest.ts")]
pub struct FullEvaluateRequest {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    pub persona_name: String,
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
    pub is_voice: bool,
    #[ts(optional, type = "string")]
    pub voice_session_id: Option<Uuid>,
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
#[ts(export, export_to = "../../../shared/generated/persona/FullEvaluateResult.ts")]
pub struct FullEvaluateResult {
    pub should_respond: bool,
    pub confidence: f32,
    pub reason: String,
    /// Which gate decided: response_cap, rate_limit, sleep_mode, directed_mention, fast_path, auto_respond
    pub gate: String,
    #[ts(type = "number")]
    pub decision_time_ms: f64,
    #[ts(optional)]
    pub gate_details: Option<GateDetails>,
}

/// Detailed gate information for diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/GateDetails.ts")]
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
}

// =============================================================================
// UNIFIED EVALUATOR
// =============================================================================

/// Run all 6 gates in order, short-circuiting on first SILENT decision.
///
/// Gate order:
/// 1. Response cap
/// 2. Mention detection
/// 3. Rate limiting
/// 4. Sleep mode (with topic detection for until_topic)
/// 5. Directed mention filter
/// 6. Fast-path decision (dedup, self-check, state gating, mention/human heuristics)
pub fn full_evaluate(
    request: &FullEvaluateRequest,
    rate_limiter: &RateLimiterState,
    sleep_state: &SleepState,
    engine: &PersonaCognitionEngine,
    now_ms: u64,
) -> FullEvaluateResult {
    let start = Instant::now();

    // =========================================================================
    // GATE 1: Response cap
    // =========================================================================
    if rate_limiter.has_reached_response_cap(request.room_id) {
        let count = rate_limiter.response_count(request.room_id);
        return FullEvaluateResult {
            should_respond: false,
            confidence: 1.0,
            reason: format!(
                "Response cap reached ({}/{})",
                count, rate_limiter.max_responses_per_session
            ),
            gate: "response_cap".into(),
            decision_time_ms: start.elapsed().as_secs_f64() * 1000.0,
            gate_details: Some(GateDetails {
                response_count: Some(count),
                max_responses: Some(rate_limiter.max_responses_per_session),
                rate_limit_wait_seconds: None,
                sleep_mode: None,
                is_mentioned: None,
                has_directed_mention: None,
                topic_similarity: None,
            }),
        };
    }

    // =========================================================================
    // GATE 2: Mention detection (computed once, reused by gates 4 and 5)
    // =========================================================================
    let is_mentioned = text_analysis::is_persona_mentioned(
        &request.content,
        &request.persona_name,
        &request.persona_unique_id,
    );
    let has_directed_mention = text_analysis::has_directed_mention(&request.content);

    // =========================================================================
    // GATE 3: Rate limiting
    // =========================================================================
    if rate_limiter.is_rate_limited(request.room_id, now_ms) {
        let wait = rate_limiter
            .rate_limit_wait_seconds(request.room_id, now_ms)
            .unwrap_or(0.0);
        return FullEvaluateResult {
            should_respond: false,
            confidence: 1.0,
            reason: format!("Rate limited, wait {:.1}s more", wait),
            gate: "rate_limit".into(),
            decision_time_ms: start.elapsed().as_secs_f64() * 1000.0,
            gate_details: Some(GateDetails {
                response_count: None,
                max_responses: None,
                rate_limit_wait_seconds: Some(wait),
                sleep_mode: None,
                is_mentioned: Some(is_mentioned),
                has_directed_mention: Some(has_directed_mention),
                topic_similarity: None,
            }),
        };
    }

    // =========================================================================
    // GATE 4: Sleep mode
    // =========================================================================
    let effective_sleep = sleep_state.effective_mode(now_ms);
    if effective_sleep != SleepMode::Active {
        let should_respond_in_sleep = match effective_sleep {
            SleepMode::Active => true,
            SleepMode::MentionedOnly => is_mentioned,
            SleepMode::HumanOnly => request.sender_is_human,
            SleepMode::Sleeping => false,
            SleepMode::UntilTopic => {
                // Check topic similarity if provided, otherwise compute from recent texts
                let topic_sim = request.topic_similarity.unwrap_or_else(|| {
                    if let Some(ref texts) = request.recent_room_texts {
                        if texts.is_empty() {
                            return 0.0; // No history = new topic
                        }
                        let combined = texts.join(" ");
                        text_analysis::jaccard_ngram_similarity(&request.content, &combined) as f32
                    } else {
                        0.5 // No data provided, assume continuation
                    }
                });
                // Below 0.3 = new topic
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
                }),
            };
        }
    }

    // =========================================================================
    // GATE 5: Directed mention filter
    // =========================================================================
    if !is_mentioned && has_directed_mention {
        return FullEvaluateResult {
            should_respond: false,
            confidence: 1.0,
            reason: "Message directed at another persona via @mention".into(),
            gate: "directed_mention".into(),
            decision_time_ms: start.elapsed().as_secs_f64() * 1000.0,
            gate_details: Some(GateDetails {
                response_count: None,
                max_responses: None,
                rate_limit_wait_seconds: None,
                sleep_mode: None,
                is_mentioned: Some(false),
                has_directed_mention: Some(true),
                topic_similarity: None,
            }),
        };
    }

    // =========================================================================
    // GATE 6: Fast-path decision (dedup, self, state gating, mention/human heuristics)
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

    // Skip dedup — the service cycle already added this message to evaluated_messages.
    // full_evaluate runs all 6 gates definitively; dedup was gate 0 in the service cycle.
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
            response_count: None,
            max_responses: None,
            rate_limit_wait_seconds: None,
            sleep_mode: None,
            is_mentioned: Some(is_mentioned),
            has_directed_mention: Some(has_directed_mention),
            topic_similarity: None,
        }),
    }
}

// =============================================================================
// TESTS
// =============================================================================

// =============================================================================
// POST-INFERENCE ADEQUACY CHECK (Phase 5)
// =============================================================================

/// A recent AI response to check for adequacy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentResponse {
    pub sender_name: String,
    pub text: String,
}

/// Result of the post-inference adequacy check.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/AdequacyResult.ts")]
pub struct AdequacyResult {
    pub is_adequate: bool,
    pub confidence: f32,
    pub reason: String,
    /// Name of the AI that already answered (if adequate)
    #[ts(optional)]
    pub responder_name: Option<String>,
    /// How long the check took (microseconds)
    #[ts(type = "number")]
    pub check_time_us: u64,
}

/// Check if any existing AI responses already adequately answer the original question.
///
/// ONE Rust call replaces N individual text-similarity IPC calls.
///
/// Thresholds:
/// - Minimum response length: 100 chars
/// - Minimum similarity: 0.2 (word n-gram Jaccard)
/// - Confidence: similarity + 0.5 (capped at 1.0)
pub fn check_response_adequacy(
    original_text: &str,
    responses: &[RecentResponse],
) -> AdequacyResult {
    let start = Instant::now();

    // Pre-compute original text ngrams once — reuse across all response comparisons
    let original_ngrams = text_analysis::build_word_ngrams(original_text);

    for response in responses {
        // Skip short responses (likely not adequate)
        if response.text.len() < 100 {
            continue;
        }

        // Check if response is related to original question
        let response_ngrams = text_analysis::build_word_ngrams(&response.text);
        let similarity = text_analysis::jaccard_from_sets(&original_ngrams, &response_ngrams);

        // Substantial response (>100 chars) that's related to the question (>0.2 similarity)
        if similarity > 0.2 {
            let confidence = (similarity as f32 + 0.5).min(1.0);
            return AdequacyResult {
                is_adequate: true,
                confidence,
                reason: format!(
                    "{} already provided a substantial response ({} chars, {}% related)",
                    response.sender_name,
                    response.text.len(),
                    (similarity * 100.0) as u32
                ),
                responder_name: Some(response.sender_name.clone()),
                check_time_us: start.elapsed().as_micros() as u64,
            };
        }
    }

    AdequacyResult {
        is_adequate: false,
        confidence: 0.0,
        reason: "No adequate responses found".into(),
        responder_name: None,
        check_time_us: start.elapsed().as_micros() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        (PersonaCognitionEngine::new(id, name.into(), rag_engine, rx), id)
    }

    fn test_request(persona_id: Uuid, persona_name: &str) -> FullEvaluateRequest {
        FullEvaluateRequest {
            persona_id,
            persona_name: persona_name.into(),
            persona_unique_id: "test-bot".into(),
            message_id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            sender_name: "Joel".into(),
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
    fn test_gate_1_response_cap() {
        let (engine, persona_id) = test_engine("TestBot");
        let request = test_request(persona_id, "TestBot");
        let sleep = SleepState::default();
        let mut rate_limiter = RateLimiterState::new(10.0, 3);

        // Push count past cap
        let room_id = request.room_id;
        let now = now_ms();
        rate_limiter.track_response(room_id, now - 30_000);
        rate_limiter.track_response(room_id, now - 20_000);
        rate_limiter.track_response(room_id, now - 11_000); // 11s ago — not rate limited

        let result = full_evaluate(&request, &rate_limiter, &sleep, &engine, now);
        assert!(!result.should_respond);
        assert_eq!(result.gate, "response_cap");
        assert_eq!(result.gate_details.unwrap().response_count, Some(3));
    }

    #[test]
    fn test_gate_3_rate_limited() {
        let (engine, persona_id) = test_engine("TestBot");
        let request = test_request(persona_id, "TestBot");
        let sleep = SleepState::default();
        let mut rate_limiter = RateLimiterState::new(10.0, 50);

        let now = now_ms();
        // Response 5 seconds ago — within 10s window
        rate_limiter.track_response(request.room_id, now - 5_000);

        let result = full_evaluate(&request, &rate_limiter, &sleep, &engine, now);
        assert!(!result.should_respond);
        assert_eq!(result.gate, "rate_limit");
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

        let result = full_evaluate(&request, &rate_limiter, &sleep, &engine, now_ms());
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

        let result = full_evaluate(&request, &rate_limiter, &sleep, &engine, now_ms());
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
            set_at_ms: now - 3_600_000, // 1 hour ago
            wake_at_ms: Some(now - 1_000), // Wake time already passed
        };
        let rate_limiter = RateLimiterState::default();

        let result = full_evaluate(&request, &rate_limiter, &sleep, &engine, now);
        // Should NOT be blocked by sleep — auto-wake expired
        assert_ne!(result.gate, "sleep_mode");
    }

    #[test]
    fn test_gate_5_directed_mention_filters() {
        let (engine, persona_id) = test_engine("TestBot");
        let mut request = test_request(persona_id, "TestBot");
        // Mentions someone else, NOT TestBot
        request.content = "@OtherBot please fix this bug".into();
        let sleep = SleepState::default();
        let rate_limiter = RateLimiterState::default();

        let result = full_evaluate(&request, &rate_limiter, &sleep, &engine, now_ms());
        assert!(!result.should_respond);
        assert_eq!(result.gate, "directed_mention");
    }

    #[test]
    fn test_gate_6_fast_path_self_message() {
        let (engine, persona_id) = test_engine("TestBot");
        let mut request = test_request(persona_id, "TestBot");
        request.sender_id = persona_id; // Self-message
        let sleep = SleepState::default();
        let rate_limiter = RateLimiterState::default();

        let result = full_evaluate(&request, &rate_limiter, &sleep, &engine, now_ms());
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

        let result = full_evaluate(&request, &rate_limiter, &sleep, &engine, now_ms());
        // Human sender + recent message = high priority → should respond
        assert!(result.should_respond);
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

        let result = full_evaluate(&request, &rate_limiter, &sleep, &engine, now_ms());
        assert!(result.should_respond);
    }

    #[test]
    fn test_all_gates_pass_normal_message() {
        let (engine, persona_id) = test_engine("TestBot");
        let request = test_request(persona_id, "TestBot");
        let sleep = SleepState::default();
        let rate_limiter = RateLimiterState::default();

        let result = full_evaluate(&request, &rate_limiter, &sleep, &engine, now_ms());
        assert!(result.should_respond);
        assert!(result.decision_time_ms < 10.0, "Decision should be <10ms, was {}ms", result.decision_time_ms);
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

        let result = full_evaluate(&request, &rate_limiter, &sleep, &engine, now_ms());
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

        let result = full_evaluate(&request, &rate_limiter, &sleep, &engine, now_ms());
        // Should pass sleep gate (new topic) and reach fast_path
        assert_ne!(result.gate, "sleep_mode");
    }

    #[test]
    fn test_track_response_increments() {
        let mut rate_limiter = RateLimiterState::new(10.0, 50);
        let room_id = Uuid::new_v4();
        let now = now_ms();

        assert_eq!(rate_limiter.response_count(room_id), 0);
        assert!(!rate_limiter.has_reached_response_cap(room_id));

        rate_limiter.track_response(room_id, now);
        assert_eq!(rate_limiter.response_count(room_id), 1);

        rate_limiter.track_response(room_id, now);
        assert_eq!(rate_limiter.response_count(room_id), 2);
    }

    #[test]
    fn test_rate_limit_expired() {
        let mut rate_limiter = RateLimiterState::new(10.0, 50);
        let room_id = Uuid::new_v4();
        let now = now_ms();

        // Response 15 seconds ago — outside 10s window
        rate_limiter.track_response(room_id, now - 15_000);

        assert!(!rate_limiter.is_rate_limited(room_id, now));
    }

    // ── Adequacy Check (Phase 5) ──────────────────────────────────────

    #[test]
    fn test_adequacy_no_responses() {
        let result = check_response_adequacy("What is Rust?", &[]);
        assert!(!result.is_adequate);
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn test_adequacy_short_response_ignored() {
        let responses = vec![RecentResponse {
            sender_name: "Helper".into(),
            text: "Rust is good.".into(), // < 100 chars
        }];
        let result = check_response_adequacy("What is Rust?", &responses);
        assert!(!result.is_adequate, "Short response should be ignored");
    }

    #[test]
    fn test_adequacy_substantial_related_response() {
        // Jaccard n-gram = |intersection|/|union|. Long responses dilute the score
        // because the union grows much faster than the intersection. Use a focused
        // response that echoes question terms without excessive additional vocabulary.
        let original = "Can someone explain how PersonaGenome activateSkill works with LRU eviction and memory budget for paging adapters in and out?";
        let response_text = "PersonaGenome activateSkill works by checking LRU eviction \
                   scores against memory budget. Adapters with low LRU scores get paged \
                   out to free budget for the new skill adapter being paged in.";
        let sim = text_analysis::jaccard_ngram_similarity(original, response_text);
        let responses = vec![RecentResponse {
            sender_name: "CodeReview AI".into(),
            text: response_text.into(),
        }];
        let result = check_response_adequacy(original, &responses);
        assert!(result.is_adequate, "Substantial related response should be adequate (similarity={sim:.3})");
        assert!(result.confidence > 0.5);
        assert_eq!(result.responder_name.as_deref(), Some("CodeReview AI"));
    }

    #[test]
    fn test_adequacy_unrelated_long_response() {
        let original = "What is Rust?";
        let responses = vec![RecentResponse {
            sender_name: "Helper".into(),
            text: "The weather today is absolutely wonderful with clear skies and temperatures around \
                   seventy degrees. Perfect conditions for outdoor activities like hiking, swimming, \
                   or simply enjoying a picnic in the park with friends and family members.".into(),
        }];
        let result = check_response_adequacy(original, &responses);
        assert!(!result.is_adequate, "Unrelated response should not be adequate");
    }

    #[test]
    fn test_adequacy_first_adequate_wins() {
        // Longer question with more terms gives Jaccard more intersection surface area
        let original = "How does Rust handle memory management with ownership borrowing and lifetimes for safe concurrent access?";
        let responses = vec![
            RecentResponse {
                sender_name: "Short AI".into(),
                text: "Ownership.".into(), // Too short (<100 chars)
            },
            RecentResponse {
                sender_name: "First Good AI".into(),
                text: "Rust handle memory management with ownership and borrowing rules. \
                       Lifetimes ensure safe concurrent access. Memory management in Rust \
                       is ownership borrowing and lifetimes working together for safe access.".into(),
            },
            RecentResponse {
                sender_name: "Second Good AI".into(),
                text: "Rust handle memory management with ownership borrowing and lifetimes. \
                       Safe concurrent access is guaranteed by the borrowing rules and lifetimes \
                       for memory management in Rust.".into(),
            },
        ];
        let result = check_response_adequacy(original, &responses);
        assert!(result.is_adequate);
        assert_eq!(result.responder_name.as_deref(), Some("First Good AI"), "First adequate response should win");
    }

    #[test]
    fn test_adequacy_check_is_fast() {
        let original = "What is the meaning of life?";
        let responses: Vec<RecentResponse> = (0..10).map(|i| RecentResponse {
            sender_name: format!("AI-{i}"),
            text: format!("Response number {i} that contains enough text to exceed the minimum character \
                           threshold of one hundred characters to be considered for adequacy checking purposes. \
                           This should be sufficient length."),
        }).collect();
        let result = check_response_adequacy(original, &responses);
        assert!(result.check_time_us < 10_000, "10 responses should be checked in <10ms, took {}μs", result.check_time_us);
    }

    #[test]
    fn export_bindings_adequacyresult() {
        AdequacyResult::export_all().unwrap();
    }
}
