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
use crate::cognition::adaptive_throughput::{ResourceClass, TargetSilicon};
use crate::cognition::resource_admission::{
    ResourceAdmissionError, ResourceAdmissionGate, ResourceAdmissionGuard, ResourceAdmissionPolicy,
    ResourceAdmissionRequest,
};
use crate::cognition::should_respond::AIDecisionContext;
use crate::cognition::throughput_lease::ThroughputLeaseRevocationPolicy;
use crate::modules::ai_provider::global_registry;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use ts_rs::TS;

/// Default unknown-members string returned by `extract_room_members` when the
/// system prompt doesn't contain a `Current room members:` line.
pub const UNKNOWN_MEMBERS: &str = "unknown members";

/// Minimum hour-gap (in milliseconds) that triggers a "⏱️ N hour passed"
/// marker in the conversation history.
const HOUR_GAP_THRESHOLD_MS: u64 = 60 * 60 * 1000;

/// Routing sentinel for the best available local Qwen/llama.cpp runtime.
// The inference gateway is our OWN llama-server (the sole local inference path; Unsloth excised).
// Was "local" (the in-process llama.cpp adapter) — now gated off, so routing the turn
// to "local" + a hardcoded model id that the gateway doesn't serve would hard-fail
// select(). The turn binds to the llama-server gateway + the discovered served model
// via a handle. One source of truth for the gateway id: `llama_server::PROVIDER_ID`.
const DEFAULT_GENERATE_PROVIDER: &str = crate::inference::llama_server::PROVIDER_ID;

/// Default model when caller doesn't override.
const DEFAULT_GENERATE_MODEL: &str = "continuum-ai/qwen3.5-4b-code-forged-GGUF";

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
    let start_ms = now_ms();
    let timeout_ms = request.timeout_ms.unwrap_or(DEFAULT_GENERATE_TIMEOUT_MS);
    let _lease = acquire_generate_response_lease(&request, start_ms, timeout_ms)?;

    // Bind this turn to the persona's ESTABLISHED inference handle (reused across
    // turns, re-homed if lost — self-healing), bound to the model unsloth actually
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
            // unsloth serves one model today, so bind to the discovered served model
            // regardless of any (possibly stale) request.model. Fit-selection over a
            // multi-model gateway is the next slice. Fail loud if nothing serves.
            let served = crate::cognition::inference_session::resolve_model(None)
                .await
                .map_err(|e| GenerateResponseError::Generation(format!(
                    "inference model resolve failed (unsloth gateway): {e:?}"
                )))?;
            sessions.ensure_for_persona(persona_uuid, served)
        }
    };
    let model = session.model.clone();

    let inference_request = build_response_generation_request(&request, model.clone(), start_ms);

    let registry_arc = global_registry();
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
        .select(
            Some(DEFAULT_GENERATE_PROVIDER),
            Some(&model),
            InferenceDevice::Auto,
        )
        .ok_or_else(|| GenerateResponseError::NoAdapter {
            provider: DEFAULT_GENERATE_PROVIDER.to_string(),
            model: Some(model.clone()),
        })?;

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

    let end_ms = now_ms();
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
        top_p: None,
        top_k: None,
        repeat_penalty: None,
        frequency_penalty: None,
        repeat_last_n: None,
        stop_sequences: None,
        tools: None,
        tool_choice: None,
        // Local Qwen takes plain text; no JSON-mode constraint here.
        response_format: Some(ResponseFormat::Text),
        active_adapters: None,
        request_id: None,
        user_id: None,
        room_id: Some(request.context.room_id.clone()),
        purpose: Some("cognition/generate-response".to_string()),
        persona_id: Some(request.context.persona_id.clone()),
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

/// Current unix-ms timestamp. Private helper — internal use only.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
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
mod tests {
    use super::*;
    use crate::cognition::should_respond::{
        AIDecisionContext, GatingConversationMessage, GatingMessageContent, GatingRagContext,
        GatingRagMetadata, GatingTriggerMessage,
    };

    // ─── Fixtures ─────────────────────────────────────────────────────

    fn msg(
        role: &str,
        name: Option<&str>,
        content: &str,
        ts: Option<u64>,
    ) -> GatingConversationMessage {
        GatingConversationMessage {
            role: role.to_string(),
            content: content.to_string(),
            name: name.map(str::to_string),
            timestamp: ts,
        }
    }

    fn ctx(
        system_prompt: Option<&str>,
        history: Vec<GatingConversationMessage>,
    ) -> AIDecisionContext {
        AIDecisionContext {
            persona_id: "p-001".to_string(),
            persona_name: "Alice".to_string(),
            room_id: "r-001".to_string(),
            trigger_message: GatingTriggerMessage {
                id: "m-trigger".to_string(),
                sender_name: "human".to_string(),
                content: GatingMessageContent {
                    text: "any".to_string(),
                },
            },
            rag_context: GatingRagContext {
                conversation_history: history,
                recipe_strategy: None,
                metadata: GatingRagMetadata { recipe_name: None },
            },
            system_prompt: system_prompt.map(str::to_string),
        }
    }

    fn text_of(msg: &ChatMessage) -> &str {
        match &msg.content {
            MessageContent::Text(s) => s.as_str(),
            _ => panic!("expected text content; ChatMessage carried a non-text variant"),
        }
    }

    // ─── format_current_time ──────────────────────────────────────────

    /// What this catches: timestamp 1_700_000_000_000ms renders as
    /// `11/14/2023 22:13` UTC. If the format string drifts (e.g. to
    /// ISO 8601), the model sees a different prompt body and the
    /// identity-reminder layer regresses silently.
    #[test]
    fn format_current_time_matches_mm_dd_yyyy_hh_mm() {
        // 1_700_000_000_000 ms = 2023-11-14 22:13:20 UTC
        assert_eq!(format_current_time(1_700_000_000_000), "11/14/2023 22:13");
    }

    /// What this catches: epoch 0 renders as `01/01/1970 00:00`.
    /// Boundary check — verifies UTC + no off-by-one in the date
    /// formatter.
    #[test]
    fn format_current_time_handles_epoch_zero() {
        assert_eq!(format_current_time(0), "01/01/1970 00:00");
    }

    // ─── extract_room_members ─────────────────────────────────────────

    /// What this catches: well-formed system prompt with members line
    /// — pulls out exactly the comma-separated list, trimmed.
    #[test]
    fn extract_members_pulls_line_after_prefix() {
        let prompt =
            "You are a helpful AI.\nCurrent room members: alice, bob, carol\nMore text below.";
        assert_eq!(extract_room_members(prompt), "alice, bob, carol");
    }

    /// What this catches: members line at end-of-string without
    /// trailing newline — still extracts.
    #[test]
    fn extract_members_handles_no_trailing_newline() {
        let prompt = "Header line.\nCurrent room members: alice, bob";
        assert_eq!(extract_room_members(prompt), "alice, bob");
    }

    /// What this catches: missing prefix returns the canonical
    /// `UNKNOWN_MEMBERS` string. Downstream prompt machinery may depend
    /// on the literal value.
    #[test]
    fn extract_members_missing_returns_unknown() {
        let prompt = "Generic system prompt with no members line.";
        assert_eq!(extract_room_members(prompt), UNKNOWN_MEMBERS);
        assert_eq!(extract_room_members(""), UNKNOWN_MEMBERS);
    }

    /// What this catches: empty members list (just whitespace after the
    /// prefix) falls back to `UNKNOWN_MEMBERS` — avoids emitting a
    /// prompt that says "the room has ONLY these people: ." which is
    /// worse than the explicit unknown-members value.
    #[test]
    fn extract_members_empty_after_prefix_returns_unknown() {
        let prompt = "Current room members: \nSomething else.";
        assert_eq!(extract_room_members(prompt), UNKNOWN_MEMBERS);
    }

    // ─── format_time_prefix ───────────────────────────────────────────

    /// What this catches: present timestamp renders as `[HH:MM] ` UTC.
    /// Same shape as `check_redundancy.rs` for consistency.
    #[test]
    fn format_time_prefix_renders_hh_mm_utc() {
        assert_eq!(format_time_prefix(Some(1_700_000_000_000)), "[22:13] ");
    }

    /// What this catches: missing timestamp returns empty string —
    /// guard against `[00:00] ` for clockless messages (would mislead
    /// the model).
    #[test]
    fn format_time_prefix_missing_returns_empty() {
        assert_eq!(format_time_prefix(None), "");
    }

    // ─── hour_gap_marker ──────────────────────────────────────────────

    /// What this catches: gap < 1h returns None — no marker injected
    /// for normal back-and-forth.
    #[test]
    fn hour_gap_marker_under_threshold_returns_none() {
        assert_eq!(hour_gap_marker(0), None);
        assert_eq!(hour_gap_marker(59 * 60 * 1000), None);
        assert_eq!(hour_gap_marker(HOUR_GAP_THRESHOLD_MS - 1), None);
    }

    /// What this catches: gap >= 1h returns the singular "1 hour"
    /// marker. Plural/singular toggle catches a regression where the
    /// `s` suffix bleeds into the 1-hour case.
    #[test]
    fn hour_gap_marker_one_hour_singular() {
        assert_eq!(
            hour_gap_marker(HOUR_GAP_THRESHOLD_MS).as_deref(),
            Some("⏱️ 1 hour passed - conversation resumed")
        );
    }

    /// What this catches: gap >= 2h renders plural "hours".
    #[test]
    fn hour_gap_marker_two_hours_plural() {
        assert_eq!(
            hour_gap_marker(3 * HOUR_GAP_THRESHOLD_MS).as_deref(),
            Some("⏱️ 3 hours passed - conversation resumed")
        );
    }

    // ─── build_identity_reminder ──────────────────────────────────────

    /// What this catches: the reminder embeds persona name, members
    /// list, and current time at the expected anchors. If any anchor
    /// regresses (e.g. `format!` arg order), the prompt loses its
    /// identity-establishing line and the model role-confuses.
    #[test]
    fn identity_reminder_embeds_persona_members_and_time() {
        let body = build_identity_reminder("Alice", "alice, bob, carol", "11/14/2023 22:13");
        assert!(body.starts_with("IDENTITY REMINDER: You are Alice."));
        assert!(body.contains("ONLY these people: alice, bob, carol."));
        assert!(body.contains("CURRENT TIME: 11/14/2023 22:13"));
        assert!(body.contains("CRITICAL TOPIC DETECTION PROTOCOL"));
    }

    /// What this catches: the four-step topic-detection rubric is
    /// preserved end-to-end. If steps get dropped, the model loses the
    /// constraint-extraction guidance.
    #[test]
    fn identity_reminder_preserves_four_step_protocol() {
        let body = build_identity_reminder("X", "y", "z");
        assert!(body.contains("Step 1: Check for EXPLICIT TOPIC MARKERS"));
        assert!(body.contains("Step 2: Extract HARD CONSTRAINTS"));
        assert!(body.contains("Step 3: Compare SUBJECT"));
        assert!(body.contains("Step 4: Determine response strategy"));
    }

    /// What this catches: the closing line about time-gap inference is
    /// preserved. Removing it would break the model's "topic shift on
    /// hour gap" heuristic which the runtime relies on.
    #[test]
    fn identity_reminder_preserves_time_gap_heuristic_line() {
        let body = build_identity_reminder("X", "y", "z");
        assert!(body.contains("Time gaps > 1 hour usually indicate topic changes"));
    }

    // ─── build_response_messages ──────────────────────────────────────

    /// What this catches: smoke test — system prompt + history +
    /// identity reminder all present in correct order. The "skeleton"
    /// shape any future refactor must preserve.
    #[test]
    fn build_response_messages_emits_system_history_identity_in_order() {
        let context = ctx(
            Some("You are Alice in a chat."),
            vec![
                msg("user", Some("human"), "Hello?", Some(1_700_000_000_000)),
                msg("assistant", Some("Alice"), "Hi!", Some(1_700_000_060_000)),
            ],
        );
        let messages = build_response_messages(&context, 1_700_000_120_000);
        assert_eq!(messages.len(), 4, "1 system + 2 history + 1 identity");
        assert_eq!(messages[0].role, "system");
        assert_eq!(text_of(&messages[0]), "You are Alice in a chat.");
        assert_eq!(messages[1].role, "user");
        assert!(text_of(&messages[1]).contains("human: Hello?"));
        assert_eq!(messages[2].role, "assistant");
        assert!(text_of(&messages[2]).contains("Alice: Hi!"));
        assert_eq!(messages[3].role, "system");
        assert!(text_of(&messages[3]).starts_with("IDENTITY REMINDER: You are Alice."));
    }

    /// What this catches: missing system prompt skips the first message
    /// but still emits the identity reminder.
    #[test]
    fn build_response_messages_omits_system_when_missing() {
        let context = ctx(None, vec![]);
        let messages = build_response_messages(&context, 0);
        assert_eq!(messages.len(), 1, "only identity reminder");
        assert!(text_of(&messages[0]).starts_with("IDENTITY REMINDER:"));
    }

    /// What this catches: empty-string system prompt is treated as
    /// missing — avoids emitting a `{ role: "system", content: "" }`
    /// row that some providers reject.
    #[test]
    fn build_response_messages_omits_system_when_empty_string() {
        let context = ctx(Some(""), vec![]);
        let messages = build_response_messages(&context, 0);
        assert_eq!(
            messages.len(),
            1,
            "only identity reminder; no empty system row"
        );
        assert!(text_of(&messages[0]).starts_with("IDENTITY REMINDER:"));
    }

    /// What this catches: hour-gap marker fires for a > 1h gap between
    /// consecutive messages. The marker injects as its own system
    /// message AFTER the older history line and BEFORE the newer one.
    #[test]
    fn build_response_messages_injects_hour_gap_marker() {
        let context = ctx(
            None,
            vec![
                msg("user", Some("human"), "Earlier?", Some(1_700_000_000_000)),
                // 2 hours later
                msg("user", Some("human"), "Later!", Some(1_700_007_200_000)),
            ],
        );
        let messages = build_response_messages(&context, 0);
        // Expected: [history-1, gap-marker, history-2, identity]
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].role, "user");
        assert!(text_of(&messages[0]).contains("human: Earlier?"));
        assert_eq!(messages[1].role, "system");
        assert_eq!(
            text_of(&messages[1]),
            "⏱️ 2 hours passed - conversation resumed"
        );
        assert_eq!(messages[2].role, "user");
        assert!(text_of(&messages[2]).contains("human: Later!"));
        assert_eq!(messages[3].role, "system");
        assert!(text_of(&messages[3]).starts_with("IDENTITY REMINDER:"));
    }

    /// What this catches: gap markers DO NOT fire between messages
    /// with sub-hour gaps — guards against an off-by-one where a
    /// 59-minute gap accidentally triggers.
    #[test]
    fn build_response_messages_no_marker_under_one_hour() {
        let context = ctx(
            None,
            vec![
                msg("user", Some("h"), "A", Some(1_700_000_000_000)),
                // 30 minutes later
                msg("user", Some("h"), "B", Some(1_700_001_800_000)),
            ],
        );
        let messages = build_response_messages(&context, 0);
        // 2 history + 1 identity, no gap marker
        assert_eq!(messages.len(), 3);
        assert!(text_of(&messages[0]).contains("A"));
        assert!(text_of(&messages[1]).contains("B"));
    }

    /// What this catches: gap tracking only updates when a timestamp
    /// is present — a clockless message in the middle doesn't reset
    /// the gap-from-previous-timestamped-message counter incorrectly.
    #[test]
    fn build_response_messages_gap_tracking_ignores_clockless_messages() {
        let context = ctx(
            None,
            vec![
                msg("user", Some("h"), "A", Some(1_700_000_000_000)),
                msg("user", Some("h"), "B-clockless", None),
                // 3 hours after A
                msg("user", Some("h"), "C", Some(1_700_010_800_000)),
            ],
        );
        let messages = build_response_messages(&context, 0);
        // Expected: history-A, history-B-clockless, gap-marker (A→C 3h), history-C, identity
        assert_eq!(messages.len(), 5);
        assert!(text_of(&messages[0]).contains("[22:13] h: A"));
        assert_eq!(messages[1].role, "user");
        assert_eq!(text_of(&messages[1]), "h: B-clockless"); // no time prefix
        assert_eq!(messages[2].role, "system");
        assert!(text_of(&messages[2]).contains("3 hours passed"));
        assert!(text_of(&messages[3]).contains("h: C"));
    }

    /// What this catches: messages without a name use the bare time
    /// prefix + content (no `name: ` chunk).
    #[test]
    fn build_response_messages_falls_back_when_name_missing() {
        let context = ctx(
            None,
            vec![msg("user", None, "bare content", Some(1_700_000_000_000))],
        );
        let messages = build_response_messages(&context, 0);
        // 1 history + 1 identity
        assert_eq!(messages.len(), 2);
        assert_eq!(text_of(&messages[0]), "[22:13] bare content");
    }

    /// What this catches: members extraction reads from the system
    /// prompt body — the identity reminder gets the right list. Pins
    /// the end-to-end path from system_prompt → extract_room_members
    /// → build_identity_reminder.
    #[test]
    fn build_response_messages_extracts_members_for_identity_reminder() {
        let prompt = "You are Alice.\nCurrent room members: alice, bob, carol\nBe helpful.";
        let context = ctx(Some(prompt), vec![]);
        let messages = build_response_messages(&context, 1_700_000_000_000);
        let reminder = text_of(messages.last().expect("identity reminder present"));
        assert!(
            reminder.contains("ONLY these people: alice, bob, carol."),
            "identity reminder should embed members extracted from system prompt; got: {reminder}"
        );
        assert!(reminder.contains("CURRENT TIME: 11/14/2023 22:13"));
    }

    /// What this catches: missing members in the system prompt still
    /// renders the identity reminder with the `UNKNOWN_MEMBERS`
    /// unknown-members string. No panic on a recipe-less room.
    #[test]
    fn build_response_messages_unknown_members_when_prompt_missing_line() {
        let context = ctx(Some("Generic system prompt."), vec![]);
        let messages = build_response_messages(&context, 0);
        let reminder = text_of(messages.last().expect("identity reminder present"));
        assert!(
            reminder.contains(&format!("ONLY these people: {UNKNOWN_MEMBERS}.")),
            "missing members line must render unknown-members value; got: {reminder}"
        );
    }

    /// What this catches: when system_prompt is None entirely, the
    /// identity reminder still composes with `UNKNOWN_MEMBERS` (no
    /// panic from `unwrap_or("")` path).
    #[test]
    fn build_response_messages_no_system_prompt_falls_back_to_unknown_members() {
        let context = ctx(None, vec![]);
        let messages = build_response_messages(&context, 0);
        let reminder = text_of(messages.last().expect("identity reminder present"));
        assert!(reminder.contains(&format!("ONLY these people: {UNKNOWN_MEMBERS}.")));
    }

    /// What this catches: assistant + user roles round-trip in their
    /// original case + spelling. Rust preserves whatever string the
    /// message carried, which is the correct conservative choice
    /// because provider routing depends on these exact strings.
    #[test]
    fn build_response_messages_preserves_role_strings() {
        let context = ctx(
            None,
            vec![
                msg("user", Some("h"), "U", None),
                msg("assistant", Some("a"), "A", None),
            ],
        );
        let messages = build_response_messages(&context, 0);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[1].role, "assistant");
    }

    /// What this catches: empty conversation history still produces a
    /// well-formed message list (system prompt if any + identity
    /// reminder). Important for first-turn responses.
    #[test]
    fn build_response_messages_handles_empty_history() {
        let context = ctx(Some("sys"), vec![]);
        let messages = build_response_messages(&context, 0);
        assert_eq!(messages.len(), 2, "system + identity");
        assert_eq!(messages[0].role, "system");
        assert_eq!(text_of(&messages[0]), "sys");
        assert!(text_of(&messages[1]).starts_with("IDENTITY REMINDER:"));
    }

    // ─── build_response_generation_request ────────────────────────────

    fn request_with_overrides(
        model: Option<&str>,
        temp: Option<f32>,
        max: Option<u32>,
        timeout: Option<u64>,
    ) -> GenerateResponseRequest {
        GenerateResponseRequest {
            context: ctx(Some("You are Alice."), vec![]),
            model: model.map(str::to_string),
            temperature: temp,
            max_tokens: max,
            timeout_ms: timeout,
            admission: None,
        }
    }

    fn request_with_admission(
        context: AIDecisionContext,
        admission: GenerateResponseAdmissionPolicy,
    ) -> GenerateResponseRequest {
        GenerateResponseRequest {
            context,
            model: None,
            temperature: None,
            max_tokens: None,
            timeout_ms: Some(100),
            admission: Some(admission),
        }
    }

    fn admission(
        max_concurrency: usize,
        max_cost_units: u32,
        cost_units: u32,
    ) -> GenerateResponseAdmissionPolicy {
        GenerateResponseAdmissionPolicy {
            target_silicon: TargetSilicon::UnifiedMemory,
            max_concurrency,
            max_cost_units,
            cost_units,
            lease_ttl_ms: 1_000,
        }
    }

    fn reset_generate_response_leases_for_test() {
        GENERATE_RESPONSE_ADMISSION.reset_for_test();
    }

    fn lock_generate_response_tests() -> std::sync::MutexGuard<'static, ()> {
        GENERATE_RESPONSE_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn active_generate_response_leases_for_test(now_ms: u64) -> usize {
        GENERATE_RESPONSE_ADMISSION.active_count_for_test(now_ms)
    }

    /// What this catches: response admission is Rust-owned. A successful
    /// acquire claims a local-generation lease, and dropping the RAII
    /// guard releases it. The same drop path is what runs when
    /// `evaluate_response` exits via success, provider error, missing
    /// adapter, or timeout.
    #[test]
    fn rust_admission_guard_releases_local_generation_lease_on_exit() {
        let _test_lock = lock_generate_response_tests();
        reset_generate_response_leases_for_test();
        let request =
            request_with_admission(ctx(Some("You are Alice."), vec![]), admission(4, 4, 1));

        {
            let _guard = acquire_generate_response_lease(&request, 1_000, 100)
                .expect("valid request should acquire a Rust lease");
            assert_eq!(active_generate_response_leases_for_test(1_001), 1);
        }

        assert_eq!(
            active_generate_response_leases_for_test(1_002),
            0,
            "dropping the guard must release the local-generation lease"
        );
    }

    /// What this catches: Rust denies over-capacity response generation
    /// before any provider call. This is the hard boundary that keeps
    /// host wrappers from owning cognition slots.
    #[test]
    fn rust_admission_denies_concurrency_and_cost_pressure() {
        let _test_lock = lock_generate_response_tests();
        reset_generate_response_leases_for_test();
        let first = request_with_admission(ctx(Some("You are Alice."), vec![]), admission(1, 4, 1));
        let second =
            request_with_admission(ctx(Some("You are Alice."), vec![]), admission(1, 4, 1));
        let _held = acquire_generate_response_lease(&first, 2_000, 100)
            .expect("first request should fit the policy");

        let err = acquire_generate_response_lease(&second, 2_001, 100)
            .expect_err("second request must be denied by Rust concurrency policy");
        assert!(matches!(
            err,
            GenerateResponseError::AdmissionDenied { reason, .. }
                if reason.contains("max_concurrency=1")
        ));

        reset_generate_response_leases_for_test();
        let expensive =
            request_with_admission(ctx(Some("You are Alice."), vec![]), admission(4, 2, 3));
        let err = acquire_generate_response_lease(&expensive, 3_000, 100)
            .expect_err("request whose cost exceeds policy must be denied");
        assert!(matches!(
            err,
            GenerateResponseError::AdmissionDenied { reason, .. }
                if reason.contains("cost_units=3 exceeds max_cost_units=2")
        ));
    }

    /// What this catches: expired leases are reaped during Rust
    /// admission, so a dead holder does not permanently block the
    /// local-generation lane.
    #[test]
    fn rust_admission_reaps_expired_generation_leases() {
        let _test_lock = lock_generate_response_tests();
        reset_generate_response_leases_for_test();
        let request =
            request_with_admission(ctx(Some("You are Alice."), vec![]), admission(1, 1, 1));
        let guard = acquire_generate_response_lease(&request, 4_000, 100)
            .expect("first request should fit the policy");
        std::mem::forget(guard);

        assert_eq!(active_generate_response_leases_for_test(4_001), 1);
        let replacement = acquire_generate_response_lease(&request, 5_001, 100)
            .expect("expired forgotten lease should be reaped before admission");
        replacement
            .release()
            .expect("explicit release should return the replacement lease");
        assert_eq!(active_generate_response_leases_for_test(5_002), 0);
    }

    /// What this catches: defaults — no overrides — produces a
    /// TextGenerationRequest with provider="local", model=Qwen-default,
    /// temperature=0.7, max_tokens=None (the model owns its length),
    /// response_format=Text, purpose="cognition/generate-response", and
    /// persona/room attribution carried from the context. Pins the wire
    /// shape so downstream provider routing doesn't drift silently.
    #[test]
    fn generation_request_uses_documented_defaults() {
        let request = request_with_overrides(None, None, None, None);
        let inference =
            build_response_generation_request(&request, DEFAULT_GENERATE_MODEL.to_string(), 0);
        assert_eq!(
            inference.provider.as_deref(),
            Some(DEFAULT_GENERATE_PROVIDER)
        );
        assert_eq!(inference.model.as_deref(), Some(DEFAULT_GENERATE_MODEL));
        assert_eq!(inference.temperature, Some(DEFAULT_GENERATE_TEMPERATURE));
        // No override + no client default = the model owns its length.
        assert_eq!(inference.max_tokens, None);
        assert_eq!(
            inference.purpose.as_deref(),
            Some("cognition/generate-response")
        );
        assert_eq!(inference.persona_id.as_deref(), Some("p-001"));
        assert_eq!(inference.room_id.as_deref(), Some("r-001"));
        assert!(matches!(
            inference.response_format,
            Some(ResponseFormat::Text)
        ));
        // messages list = system prompt + identity reminder for an empty history
        assert_eq!(inference.messages.len(), 2);
    }

    /// What this catches: per-request overrides actually override
    /// (temperature, max_tokens, model). Without this, a caller passing
    /// `temperature=0.1` would silently get the default 0.7.
    #[test]
    fn generation_request_honors_overrides() {
        let request = request_with_overrides(Some("custom-model"), Some(0.1), Some(500), None);
        let inference = build_response_generation_request(&request, "custom-model".to_string(), 0);
        assert_eq!(inference.model.as_deref(), Some("custom-model"));
        assert_eq!(inference.temperature, Some(0.1));
        assert_eq!(inference.max_tokens, Some(500));
    }

    /// What this catches: build_response_generation_request embeds the
    /// timestamp it's given into the identity reminder via
    /// build_response_messages. Pins the time-flow through the layers.
    #[test]
    fn generation_request_embeds_caller_timestamp() {
        let request = request_with_overrides(None, None, None, None);
        let inference = build_response_generation_request(
            &request,
            DEFAULT_GENERATE_MODEL.to_string(),
            1_700_000_000_000,
        );
        let identity = match &inference.messages.last().expect("identity present").content {
            MessageContent::Text(s) => s.clone(),
            _ => panic!("non-text identity"),
        };
        assert!(identity.contains("CURRENT TIME: 11/14/2023 22:13"));
    }

    // ─── result_from_response ─────────────────────────────────────────

    fn fake_response(
        text: &str,
        total_tokens: u32,
        input: u32,
        output: u32,
    ) -> TextGenerationResponse {
        TextGenerationResponse {
            text: text.to_string(),
            finish_reason: crate::ai::types::FinishReason::Stop,
            model: "ignored".to_string(),
            provider: "local".to_string(),
            usage: crate::ai::types::UsageMetrics {
                input_tokens: input,
                output_tokens: output,
                total_tokens,
                estimated_cost: None,
            },
            response_time_ms: 0,
            request_id: "test".to_string(),
            content: None,
            tool_calls: None,
            reasoning: None,
            routing: None,
            error: None,
            timing: None,
        }
    }

    /// What this catches: result trims surrounding whitespace from the
    /// provider's text. Models often emit leading/trailing newlines;
    /// without trim the chat surface gets extra blank lines.
    #[test]
    fn result_trims_response_text() {
        let r = fake_response("  hello world\n\n", 0, 0, 0);
        let result = result_from_response(r, "m".to_string(), 0, 1000);
        assert_eq!(result.text, "hello world");
    }

    /// What this catches: model + timestamps stamped correctly on the
    /// returned struct. response_time_ms = end - start, timestamp = end.
    #[test]
    fn result_stamps_model_and_timing() {
        let r = fake_response("body", 0, 0, 0);
        let result = result_from_response(r, "qwen3.5".to_string(), 1_000, 1_250);
        assert_eq!(result.model, "qwen3.5");
        assert_eq!(result.response_time_ms, 250);
        assert_eq!(result.timestamp, 1_250);
    }

    /// What this catches: total_tokens > 0 -> Some(TokenUsage) with all
    /// three counts. The provider-reported case.
    #[test]
    fn result_populates_tokens_when_provider_reports() {
        let r = fake_response("body", 100, 40, 60);
        let result = result_from_response(r, "m".to_string(), 0, 0);
        assert_eq!(
            result.tokens_used,
            Some(TokenUsage {
                input: 40,
                output: 60,
                total: 100,
            })
        );
    }

    /// What this catches: total_tokens == 0 -> None. Avoids emitting
    /// `{input:0, output:0, total:0}` as if the provider had measured
    /// usage.
    #[test]
    fn result_tokens_none_when_provider_reports_zero() {
        let r = fake_response("body", 0, 0, 0);
        let result = result_from_response(r, "m".to_string(), 0, 0);
        assert_eq!(result.tokens_used, None);
    }

    /// What this catches: response_time_ms uses saturating subtraction
    /// — if end_ms < start_ms (clock-backwards artifact, e.g. NTP
    /// adjustment mid-call), result_time is 0, not a wrapped huge u64.
    #[test]
    fn result_response_time_saturates_when_clock_goes_backward() {
        let r = fake_response("body", 0, 0, 0);
        let result = result_from_response(r, "m".to_string(), 2_000, 1_000);
        assert_eq!(result.response_time_ms, 0);
    }

    // ─── GenerateResponseError ────────────────────────────────────────

    /// What this catches: Display impl carries the provider + model
    /// values in NoAdapter so debug logs surface what went unrouted.
    #[test]
    fn error_no_adapter_displays_provider_and_model() {
        let err = GenerateResponseError::NoAdapter {
            provider: "local".to_string(),
            model: Some("qwen3.5".to_string()),
        };
        let s = format!("{err}");
        assert!(s.contains("local"));
        assert!(s.contains("qwen3.5"));
    }

    /// What this catches: Display impl for Timeout includes the
    /// configured timeout — diagnostic value for operators tuning
    /// the value.
    #[test]
    fn error_timeout_displays_duration() {
        let err = GenerateResponseError::Timeout {
            timeout_ms: 180_000,
        };
        let s = format!("{err}");
        assert!(s.contains("180000"));
    }
}
