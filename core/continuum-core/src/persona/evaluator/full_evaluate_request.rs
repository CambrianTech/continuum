use super::*;

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
