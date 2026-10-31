//! Rust-owned response-generation prompt assembly and admission.
//!
//! Rust owns response admission, the response-generation contract,
//! prompt assembly, and the identity-reminder template. Host runtimes
//! may be native Rust, game/live loops, AIRC daemons, or wrappers around
//! those hosts; none of them own cognition slot coordination for this
//! path.
//!
//! ## Scope
//!
//! - `GenerateResponseRequest` — IPC request (ts-rs)
//! - `GenerateResponseResult` — IPC response (ts-rs)
//! - `TokenUsage` — token-count breakdown (ts-rs)
//! - `build_response_messages(&AIDecisionContext, current_time_ms)
//!   -> Vec<ChatMessage>` — pure. Composes:
//!     - System-prompt message (from context.system_prompt)
//!     - Conversation history with [HH:MM] time prefix + hour-gap
//!       markers
//!     - Identity-reminder system message at end
//! - `build_identity_reminder(persona_name, members, current_time)
//!   -> String` — pure. The canonical ~50-line critical-topic-detection
//!   prompt template.
//! - `extract_room_members(system_prompt) -> &str` — pure. Regex
//!   pulls `Current room members: ...` out of a system prompt body.
//! - `format_current_time(ms) -> String` — pure. UTC `MM/DD/YYYY HH:MM`.
//! - `format_time_prefix(Option<ms>) -> String` — pure. UTC `[HH:MM] `.
//! - `hour_gap_marker(gap_ms) -> Option<String>` — pure.
//!
//! ## Failure-mode discipline
//!
//! Same posture as `check_redundancy.rs` + `should_respond.rs`:
//!   - All errors typed (`GenerateResponseError` — PR-2 surfaces it).
//!   - Pure prompt builder uses UTC so server timezone cannot bleed into
//!     model prompts depending on host.
//!   - No silent default-on-error in the parser layer (PR-2).
//!   - Members extraction uses the literal `"unknown members"` string
//!     when the prompt does not declare room members.

use crate::ai::adapter::InferenceDevice;
use crate::ai::types::ResponseFormat;
use crate::ai::{ChatMessage, MessageContent, TextGenerationRequest, TextGenerationResponse};
use crate::resources::throughput_class::{ResourceClass, TargetSilicon};
use crate::inference::resource_admission::{ResourceAdmissionError, ResourceAdmissionGate, ResourceAdmissionGuard, ResourceAdmissionPolicy, ResourceAdmissionRequest};
use crate::cognition::should_respond::AIDecisionContext;
use crate::resources::throughput_lease::ThroughputLeaseRevocationPolicy;
use crate::ai::adapter::global_registry;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use std::time::Duration;
use ts_rs::TS;

/// Default unknown-members string returned by `extract_room_members` when the
/// system prompt doesn't contain a `Current room members:` line.
pub const UNKNOWN_MEMBERS: &str = "unknown members";

/// Minimum hour-gap (in milliseconds) that triggers a "⏱️ N hour passed"
/// marker in the conversation history.
const HOUR_GAP_THRESHOLD_MS: u64 = 60 * 60 * 1000;

/// Routing sentinel for the best available local Qwen/llama.cpp runtime.
// The inference gateway is our OWN llama-server (the sole local inference path).
// Was "local" (the in-process llama.cpp adapter) — now gated off, so routing the turn
// to "local" + a hardcoded model id that the gateway doesn't serve would hard-fail
// select(). The turn binds to the llama-server gateway + the discovered served model
// via a handle. One source of truth for the gateway id: `llama_server::PROVIDER_ID`.
const DEFAULT_GENERATE_PROVIDER: &str = crate::inference::llama_server::PROVIDER_ID;

/// A model id for tests to pass explicitly — NOT a production default, and gated so it
/// cannot become one. There is deliberately no default model: per the note on
/// [`DEFAULT_GENERATE_PROVIDER`] above, the turn binds to whatever the gateway actually
/// serves (discovered, via a handle), because a hardcoded id the gateway does not serve
/// hard-fails `select()`. This constant only spares the request-shaping tests from
/// repeating a literal.
#[cfg(test)]
const TEST_GENERATE_MODEL: &str = "continuum-ai/qwen3.5-4b-code-forged-GGUF";

/// Default sampling temperature: moderate
/// creativity for natural-language responses.
const DEFAULT_GENERATE_TEMPERATURE: f32 = 0.7;

/// Default timeout. Qwen local can be slow under load; this is the hard
/// ceiling before `tokio::time::timeout` returns Err.
const DEFAULT_GENERATE_TIMEOUT_MS: u64 = 180_000;

/// Conservative default for local response generation while the
/// substrate-governor bridge becomes the source of these numbers.
const DEFAULT_GENERATE_MAX_CONCURRENCY: usize = 4;

/// Cost-unit budget paired with [`DEFAULT_GENERATE_MAX_CONCURRENCY`].
const DEFAULT_GENERATE_MAX_COST_UNITS: u32 = 4;

/// One response generation claims one local-generation cost unit unless
/// the caller provides a stricter policy.
const DEFAULT_GENERATE_COST_UNITS: u32 = 1;

/// Lease TTL must outlive the generation timeout so slow-but-valid work
/// is not marked reclaimable before `tokio::time::timeout` fires.
const DEFAULT_GENERATE_LEASE_TTL_PAD_MS: u64 = 5_000;

static GENERATE_RESPONSE_ADMISSION: LazyLock<ResourceAdmissionGate> =
    LazyLock::new(ResourceAdmissionGate::new);

#[cfg(test)]
static GENERATE_RESPONSE_TEST_LOCK: LazyLock<std::sync::Mutex<()>> =
    LazyLock::new(|| std::sync::Mutex::new(()));

// ─── IPC request + response shapes ────────────────────────────────────

/// IPC request: ask the cognition service to assemble a response-prompt
/// and (in PR-2) run it through the local inference provider.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/GenerateResponseRequest.ts"
)]
pub struct GenerateResponseRequest {
    /// Reuses the gating context. Host callers provide the persona's
    /// identity system prompt with `Current room members: ...` in
    /// `context.system_prompt`.
    pub context: AIDecisionContext,
    /// Optional model override. Defaults to the local-Qwen routing
    /// sentinel when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    /// Sampling temperature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub temperature: Option<f32>,
    /// Max tokens to generate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub max_tokens: Option<u32>,
    /// Hard cap on how long PR-2's async composer waits before
    /// returning timeout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub timeout_ms: Option<u64>,
    /// Rust-owned admission policy for this generation. When omitted,
    /// `evaluate_response` applies the local-generation defaults above.
    /// Hosts that know tighter resource limits should pass them here;
    /// they should not coordinate slots outside Rust.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub admission: Option<GenerateResponseAdmissionPolicy>,
}

/// Per-call local-generation admission policy. This is the contract a
/// host uses to ask Rust for response-generation capacity instead of
/// owning slots itself.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/GenerateResponseAdmissionPolicy.ts"
)]
pub struct GenerateResponseAdmissionPolicy {
    pub target_silicon: TargetSilicon,
    pub max_concurrency: usize,
    pub max_cost_units: u32,
    pub cost_units: u32,
    #[ts(type = "number")]
    pub lease_ttl_ms: u64,
}

impl GenerateResponseAdmissionPolicy {
    fn with_timeout(timeout_ms: u64) -> Self {
        Self {
            target_silicon: TargetSilicon::UnifiedMemory,
            max_concurrency: DEFAULT_GENERATE_MAX_CONCURRENCY,
            max_cost_units: DEFAULT_GENERATE_MAX_COST_UNITS,
            cost_units: DEFAULT_GENERATE_COST_UNITS,
            lease_ttl_ms: timeout_ms.saturating_add(DEFAULT_GENERATE_LEASE_TTL_PAD_MS),
        }
    }

    fn into_resource_policy(self) -> ResourceAdmissionPolicy {
        ResourceAdmissionPolicy {
            resource_class: ResourceClass::LocalGeneration,
            target_silicon: self.target_silicon,
            max_concurrency: self.max_concurrency,
            max_cost_units: self.max_cost_units,
            cost_units: self.cost_units,
            lease_ttl_ms: self.lease_ttl_ms,
            revocation_policy: ThroughputLeaseRevocationPolicy::Graceful,
        }
    }
}

/// IPC response: generated text plus timing + token telemetry.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/GenerateResponseResult.ts"
)]
pub struct GenerateResponseResult {
    pub text: String,
    pub model: String,
    #[ts(type = "number")]
    pub response_time_ms: u64,
    #[ts(type = "number")]
    pub timestamp: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tokens_used: Option<TokenUsage>,
}

/// Token-count breakdown — present when the provider reports usage,
/// `None` when the provider does not (e.g. local Qwen without
/// instrumentation).
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/TokenUsage.ts"
)]
pub struct TokenUsage {
    pub input: u32,
    pub output: u32,
    pub total: u32,
}

/// Typed errors from `evaluate_response`. No silent default-on-error;
/// the Rust caller decides policy explicitly.
#[derive(Debug, thiserror::Error)]
pub enum GenerateResponseError {
    /// Rust admission denied this response before inference began.
    /// Hosts ask Rust, receive a typed denial, and retry/replan explicitly.
    #[error(
        "response generation admission denied for persona={persona_id:?} room={room_id:?}: {reason}"
    )]
    AdmissionDenied {
        persona_id: String,
        room_id: String,
        reason: String,
    },
    /// The provider registry had no adapter capable of serving this
    /// model + provider tuple. No alternate runtime is attempted.
    #[error("no AI adapter available for provider={provider:?} model={model:?}")]
    NoAdapter {
        provider: String,
        model: Option<String>,
    },
    /// Provider returned an error during generation (network, model
    /// refused, etc.). The string is the raw provider message — caller
    /// should log + surface, never silently default.
    #[error("generation failed: {0}")]
    Generation(String),
    /// `tokio::time::timeout` fired before the provider returned.
    /// The persona scheduler should treat this as a transient failure
    /// and back off, not a permanent decision.
    #[error("generation timed out after {timeout_ms} ms")]
    Timeout {
        #[allow(dead_code)] // surfaced via Display
        timeout_ms: u64,
    },
}

/// Run the response-generation against the registered AI provider.
///
/// Composes:
///   1. `build_response_messages(&request.context, now)` for the
///      message array (system prompt + history + identity reminder).
///   2. `TextGenerationRequest` with provider="local" + model +
///      temperature + max_tokens defaults from `DEFAULT_GENERATE_*`
///      constants (each overridable per-request).
///   3. `tokio::time::timeout` wraps the provider call.
///   4. Stamps `GenerateResponseResult` with model + response_time_ms +
///      timestamp + optional token usage (when the provider reports it).
///
/// No alternate runtime path: provider failures, timeouts, and missing adapters
/// all surface as typed errors. Caller decides policy explicitly.
pub async fn evaluate_response(
    request: GenerateResponseRequest,
) -> Result<GenerateResponseResult, GenerateResponseError> {
    let start_ms = continuum_base::time::EpochMs::now().millis();
    let timeout_ms = request.timeout_ms.unwrap_or(DEFAULT_GENERATE_TIMEOUT_MS);
    let _lease = acquire_generate_response_lease(&request, start_ms, timeout_ms)?;

    // Bind this turn to the persona's ESTABLISHED inference handle (reused across
    // turns, re-homed if lost — self-healing), bound to the model the engine actually
    // serves (discovered, NOT a hardcoded id that no longer matches the gateway).
    // The handle is the seam that makes inference grid-routable + survive a node
    // dropping. [[long-running-commands-are-handle-based]] [[compute-lease-boundary]]
    let persona_uuid = uuid::Uuid::parse_str(&request.context.persona_id).map_err(|e| {
        GenerateResponseError::Generation(format!(
            "persona_id '{}' is not a UUID: {e}",
            request.context.persona_id
        ))
    })?;
    let sessions = crate::cognition::inference_session::global_inference_sessions();
    let session = match sessions.persona_session(persona_uuid) {
        Some(s) => s,
        None => {
            // the engine serves one model today, so bind to the discovered served model
            // regardless of any (possibly stale) request.model. Fit-selection over a
            // multi-model gateway is the next slice. Fail loud if nothing serves.
            let served = crate::cognition::inference_session::resolve_model(None)
                .await
                .map_err(|e| {
                    GenerateResponseError::Generation(format!(
                        "inference model resolve failed (local llama-server): {e:?}"
                    ))
                })?;
            sessions.ensure_for_persona(persona_uuid, served)
        }
    };
    let model = session.model.clone();

    let inference_request = build_response_generation_request(&request, model.clone(), start_ms);

    let registry_arc = global_registry();
    let adapter = {
        let registry = registry_arc.read().await;
        // Device = `Auto` — cognition has no opinion on placement; the
        // model identifier already names what's wanted, and the
        // registered adapter is the authority on its own device class.
        // Filtering by `Gpu` here (the old `InferenceDevice::default()`)
        // wrongly excluded CPU-only adapters even when they were the
        // only ones claiming the model — observed 2026-06-03 on Intel
        // Mac CPU build where Paige's LlamaCppAdapter declared Cpu
        // and was filtered out of her own response cycle.
        let (_provider_id, adapter) = registry
            .select_arc(
                Some(DEFAULT_GENERATE_PROVIDER),
                Some(&model),
                InferenceDevice::Auto,
            )
            .ok_or_else(|| GenerateResponseError::NoAdapter {
                provider: DEFAULT_GENERATE_PROVIDER.to_string(),
                model: Some(model.clone()),
            })?;
        adapter
    };

    let response: TextGenerationResponse = match tokio::time::timeout(
        Duration::from_millis(timeout_ms),
        adapter.generate_text(inference_request),
    )
    .await
    {
        Ok(Ok(resp)) => resp,
        Ok(Err(e)) => return Err(GenerateResponseError::Generation(e)),
        Err(_) => return Err(GenerateResponseError::Timeout { timeout_ms }),
    };

    let end_ms = continuum_base::time::EpochMs::now().millis();
    Ok(result_from_response(response, model, start_ms, end_ms))
}

fn acquire_generate_response_lease(
    request: &GenerateResponseRequest,
    now_ms: u64,
    timeout_ms: u64,
) -> Result<ResourceAdmissionGuard, GenerateResponseError> {
    let policy = request
        .admission
        .clone()
        .unwrap_or_else(|| GenerateResponseAdmissionPolicy::with_timeout(timeout_ms));

    GENERATE_RESPONSE_ADMISSION
        .acquire(ResourceAdmissionRequest {
            lease_id: generate_response_lease_id(&request.context, now_ms),
            artifact_key: generate_response_artifact_key(&request.context),
            holder_id: request.context.persona_id.clone(),
            policy: policy.into_resource_policy(),
            now_ms,
        })
        .map_err(|err| GenerateResponseError::AdmissionDenied {
            persona_id: request.context.persona_id.clone(),
            room_id: request.context.room_id.clone(),
            reason: format_resource_admission_error(err),
        })
}

fn generate_response_lease_id(context: &AIDecisionContext, now_ms: u64) -> String {
    format!(
        "cognition/generate-response:{}:{}:{}",
        context.room_id, context.persona_id, now_ms
    )
}

fn generate_response_artifact_key(context: &AIDecisionContext) -> String {
    format!(
        "cognition/generate-response:{}:{}:{}",
        context.room_id, context.persona_id, context.trigger_message.id
    )
}

fn format_resource_admission_error(err: ResourceAdmissionError) -> String {
    match err {
        ResourceAdmissionError::InvalidPolicy { reason }
        | ResourceAdmissionError::Denied { reason }
        | ResourceAdmissionError::Lease { reason } => reason,
    }
}

/// Build the `TextGenerationRequest` the adapter consumes.
/// Pure: caller passes `request`, `model`, and the start-timestamp so
/// tests can assert the request shape without time interference.
pub fn build_response_generation_request(
    request: &GenerateResponseRequest,
    model: String,
    start_ms: u64,
) -> TextGenerationRequest {
    TextGenerationRequest {
        messages: build_response_messages(&request.context, start_ms),
        system_prompt: None,
        model: Some(model),
        provider: Some(DEFAULT_GENERATE_PROVIDER.to_string()),
        temperature: Some(request.temperature.unwrap_or(DEFAULT_GENERATE_TEMPERATURE)),
        // Pass the caller's ceiling through verbatim — `None` (the default) means the
        // MODEL owns its length (the adapter forwards no cap). We never substitute a
        // const of our own: a hardcoded floor truncated reasoning models mid-thought.
        max_tokens: request.max_tokens,
        reasoning_budget: None,
        top_p: None,
        top_k: None,
        repeat_penalty: None,
        frequency_penalty: None,
        repeat_last_n: None,
        stop_sequences: None,
        tools: None,
        tail_tools: None,
        view_messages: None,
        tool_choice: None,
        // Local Qwen takes plain text; no JSON-mode constraint here.
        response_format: Some(ResponseFormat::Text),
        active_adapters: None,
        request_id: None,
        user_id: None,
        room_id: Some(request.context.room_id.clone()),
        purpose: Some("cognition/generate-response".to_string()),
        persona_id: Some(request.context.persona_id.clone()),
        turn_bound: None,
    }
}

/// Pure: compose the IPC response from the provider's text + timing.
/// Trims the response text at the Rust boundary.
///
/// `tokens_used` is `None` when the provider reported `total_tokens == 0`.
/// A zero total means the provider did not emit measured token usage.
pub fn result_from_response(
    response: TextGenerationResponse,
    model: String,
    start_ms: u64,
    end_ms: u64,
) -> GenerateResponseResult {
    let tokens_used = if response.usage.total_tokens > 0 {
        Some(TokenUsage {
            input: response.usage.input_tokens,
            output: response.usage.output_tokens,
            total: response.usage.total_tokens,
        })
    } else {
        None
    };
    GenerateResponseResult {
        text: response.text.trim().to_string(),
        model,
        response_time_ms: end_ms.saturating_sub(start_ms),
        timestamp: end_ms,
        tokens_used,
    }
}

// ─── Pure prompt builder ──────────────────────────────────────────────

/// Build the full message array sent to the local inference provider.
///
/// Pure — no I/O, no clock. Caller passes
/// the current time so this function stays deterministic in tests.
///
/// Composition order:
///   1. System prompt (if `context.system_prompt` is set)
///   2. Conversation history with `[HH:MM] {name}: {content}` rows,
///      interspersed with `⏱️ N hours passed` markers for gaps > 1h
///   3. Final identity-reminder system message with persona name +
///      members + current time + the critical-topic-detection protocol
pub fn build_response_messages(
    context: &AIDecisionContext,
    current_time_ms: u64,
) -> Vec<ChatMessage> {
    let mut messages: Vec<ChatMessage> = Vec::new();

    // 1. System prompt
    if let Some(prompt) = context.system_prompt.as_deref() {
        if !prompt.is_empty() {
            messages.push(ChatMessage {
                role: "system".to_string(),
                content: MessageContent::Text(prompt.to_string()),
                name: None,
            });
        }
    }

    // 2. Conversation history with time prefix + hour-gap markers
    let mut last_timestamp: Option<u64> = None;
    for msg in &context.rag_context.conversation_history {
        let time_prefix = format_time_prefix(msg.timestamp);

        if let (Some(prev), Some(now)) = (last_timestamp, msg.timestamp) {
            if now > prev {
                if let Some(marker) = hour_gap_marker(now - prev) {
                    messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: MessageContent::Text(marker),
                        name: None,
                    });
                }
            }
        }

        if msg.timestamp.is_some() {
            last_timestamp = msg.timestamp;
        }

        let formatted_content = match &msg.name {
            Some(name) => format!("{time_prefix}{name}: {}", msg.content),
            None => format!("{time_prefix}{}", msg.content),
        };

        messages.push(ChatMessage {
            role: msg.role.clone(),
            content: MessageContent::Text(formatted_content),
            name: None,
        });
    }

    // 3. Identity reminder at end
    let system_prompt_body = context.system_prompt.as_deref().unwrap_or("");
    let members = extract_room_members(system_prompt_body);
    let current_time = format_current_time(current_time_ms);
    let reminder = build_identity_reminder(&context.persona_name, members, &current_time);
    messages.push(ChatMessage {
        role: "system".to_string(),
        content: MessageContent::Text(reminder),
        name: None,
    });

    messages
}

/// Format the canonical identity-reminder system message.
pub fn build_identity_reminder(persona_name: &str, members: &str, current_time: &str) -> String {
    format!(
        "IDENTITY REMINDER: You are {persona_name}. Respond naturally with JUST your message - NO name prefix, NO \"A:\" or \"H:\" labels, NO fake conversations. The room has ONLY these people: {members}.\n\
\n\
CURRENT TIME: {current_time}\n\
\n\
CRITICAL TOPIC DETECTION PROTOCOL:\n\
\n\
Step 1: Check for EXPLICIT TOPIC MARKERS in the most recent message\n\
- \"New topic:\", \"Different question:\", \"Changing subjects:\", \"Unrelated, but...\"\n\
- If present: STOP. Ignore ALL previous context. This is a NEW conversation.\n\
\n\
Step 2: Extract HARD CONSTRAINTS from the most recent message\n\
- Look for: \"NOT\", \"DON'T\", \"WITHOUT\", \"NEVER\", \"AVOID\", \"NO\"\n\
- Example: \"NOT triggering the app to foreground\" = YOUR SOLUTION MUST NOT DO THIS\n\
- Example: \"WITHOUT user interaction\" = YOUR SOLUTION MUST BE AUTOMATIC\n\
- Your answer MUST respect these constraints or you're wrong.\n\
\n\
Step 3: Compare SUBJECT of most recent message to previous 2-3 messages\n\
- Previous: \"Worker Threads\" → Recent: \"Webview authentication\" = DIFFERENT SUBJECTS\n\
- Previous: \"implementation detail\" → Recent: \"What's 2+2?\" = TEST QUESTION\n\
- Previous: \"Worker pools\" → Recent: \"Should I use 5 or 10 workers?\" = SAME SUBJECT\n\
\n\
Step 4: Determine response strategy\n\
IF EXPLICIT TOPIC MARKER or COMPLETELY DIFFERENT SUBJECT:\n\
- Respond ONLY to the new topic\n\
- Ignore old messages (they're from a previous discussion)\n\
- Focus 100% on the most recent message\n\
- Address the constraints explicitly\n\
\n\
IF SAME SUBJECT (continued conversation):\n\
- Use full conversation context\n\
- Build on previous responses\n\
- Still check for NEW constraints in the recent message\n\
- Avoid redundancy\n\
\n\
CRITICAL READING COMPREHENSION:\n\
- Read the ENTIRE most recent message carefully\n\
- Don't skim - every word matters\n\
- Constraints are REQUIREMENTS, not suggestions\n\
- If the user says \"NOT X\", suggesting X is a failure\n\
\n\
Time gaps > 1 hour usually indicate topic changes, but IMMEDIATE semantic shifts (consecutive messages about different subjects) are also topic changes."
    )
}

/// Extract the `Current room members: ...` line from a system prompt
/// body. Returns the captured contents up to the next newline.
/// Returns `UNKNOWN_MEMBERS` if no match.
pub fn extract_room_members(system_prompt: &str) -> &str {
    const PREFIX: &str = "Current room members: ";
    let Some(start) = system_prompt.find(PREFIX) else {
        return UNKNOWN_MEMBERS;
    };
    let after = &system_prompt[start + PREFIX.len()..];
    let end = after.find('\n').unwrap_or(after.len());
    let captured = after[..end].trim_end();
    if captured.is_empty() {
        UNKNOWN_MEMBERS
    } else {
        captured
    }
}

/// Format a unix-ms timestamp as UTC `MM/DD/YYYY HH:MM`.
pub fn format_current_time(time_ms: u64) -> String {
    let dt = DateTime::<Utc>::from_timestamp_millis(time_ms as i64).unwrap_or_else(Utc::now);
    dt.format("%m/%d/%Y %H:%M").to_string()
}

/// Format a unix-ms timestamp as `[HH:MM] ` UTC for inline prefixing
/// of conversation messages. Returns empty string when timestamp is
/// missing.
fn format_time_prefix(timestamp_ms: Option<u64>) -> String {
    let Some(ms) = timestamp_ms else {
        return String::new();
    };
    let total_seconds = ms / 1000;
    let hours = (total_seconds / 3600) % 24;
    let minutes = (total_seconds / 60) % 60;
    format!("[{hours:02}:{minutes:02}] ")
}

/// Return a `⏱️ N hour passed` marker if `gap_ms` exceeds the
/// threshold. Returns `None` for gaps under 1 hour.
fn hour_gap_marker(gap_ms: u64) -> Option<String> {
    if gap_ms < HOUR_GAP_THRESHOLD_MS {
        return None;
    }
    let gap_hours = gap_ms / HOUR_GAP_THRESHOLD_MS;
    let plural = if gap_hours > 1 { "s" } else { "" };
    Some(format!(
        "⏱️ {gap_hours} hour{plural} passed - conversation resumed"
    ))
}

#[cfg(test)]
mod tests;
