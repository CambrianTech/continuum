//! AI Provider Types - Shared types for AI adapter system
//!
//! Single source of truth for AI types in Rust, exported to TypeScript via ts-rs.
//! Tool calling types enable PersonaUser to use native API tools.
//!
//! Generated TypeScript types are in: protocol/typescript/ai/

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// Chat message for text generation
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/ai/ChatMessage.ts")]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: String,
    pub content: MessageContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
}

/// Message content - either plain text or multimodal content blocks
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai/MessageContent.ts"
)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

/// Content part for multimodal and tool protocol messages
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/ai/ContentPart.ts")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text {
        text: String,
    },
    Image {
        image: ImageInput,
    },
    Audio {
        audio: AudioInput,
    },
    Video {
        video: VideoInput,
    },
    ToolUse {
        id: String,
        name: String,
        #[ts(type = "Record<string, unknown>")]
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/ai/ImageInput.ts")]
#[serde(rename_all = "camelCase")]
pub struct ImageInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base64: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/ai/AudioInput.ts")]
#[serde(rename_all = "camelCase")]
pub struct AudioInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base64: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/ai/VideoInput.ts")]
#[serde(rename_all = "camelCase")]
pub struct VideoInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base64: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mime_type: Option<String>,
}

// ============================================================================
// TOOL CALLING TYPES
// ============================================================================

/// Native tool specification for providers with JSON tool support
/// (Anthropic, OpenAI, DeepSeek, etc.)
///
/// Field names match the Anthropic API wire format (snake_case):
/// - `input_schema` NOT `inputSchema`
///   This must NOT use rename_all = "camelCase" because the wire format
///   from TypeScript AND the Anthropic API both use snake_case for this struct.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai/NativeToolSpec.ts"
)]
pub struct NativeToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: ToolInputSchema,
}

/// JSON Schema for tool input parameters.
/// Matches Anthropic API wire format (snake_case field names).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai/ToolInputSchema.ts"
)]
pub struct ToolInputSchema {
    #[serde(rename = "type")]
    pub schema_type: String, // Always "object"
    #[ts(type = "Record<string, unknown>")]
    pub properties: Value, // JSON object describing properties
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub required: Option<Vec<String>>,
    /// Nested-type definitions — the `#/definitions/<Name>` targets schemars
    /// emits for any param with a nested struct/enum (`EditMode`, `OrderByClause`,
    /// the self-referential `RagSourceRequest`, …). They MUST travel with the
    /// schema: a backend's grammar/parser resolves each `$ref` against this
    /// sibling, and without it llama.cpp rejects the whole turn with a 400
    /// ("definitions not in {…}"). Carried verbatim under `definitions` (the key
    /// the refs name); harmless standard JSON Schema for OpenAI/Anthropic too.
    /// Inlining is NOT an option — recursive params (`sources: Vec<Self>`) express
    /// recursion AS a `$ref`, so the ref must resolve, not expand.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(type = "Record<string, unknown>")]
    pub definitions: Option<Value>,
}

/// Tool call from AI response (when AI wants to use a tool)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/ai/ToolCall.ts")]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub id: String,   // Unique ID for this tool use (e.g., "toolu_01A...")
    pub name: String, // Tool name
    #[ts(type = "Record<string, unknown>")]
    pub input: Value, // Tool parameters as JSON
}

impl ToolCall {
    /// Stable identity of THIS call for loop / repeat detection: `name|json(input)`.
    ///
    /// The random per-call `id` is deliberately excluded — two calls with the same name
    /// and arguments ARE the same action regardless of their generated ids. This is the
    /// SINGLE source of the fingerprint that both the settle loop's stuck-batch signature
    /// (`act_observe::settle::drive_to_settle`) and `apply_act`'s repeat guard key on;
    /// two hand-inlined copies of this format drifting apart would silently break loop
    /// detection, so they share this one method.
    pub fn loop_fingerprint(&self) -> String {
        format!(
            "{}|{}",
            self.name,
            serde_json::to_string(&self.input).unwrap_or_default()
        )
    }
}

/// Tool result to send back to AI after execution
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/ai/ToolResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub tool_use_id: String, // Matches ToolCall.id
    pub content: String,     // Tool execution result (or error message)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub is_error: Option<bool>, // True if tool execution failed
}

/// Tool choice specification
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/ai/ToolChoice.ts")]
#[serde(untagged)]
pub enum ToolChoice {
    Mode(String), // "auto", "any", "none"
    Specific { name: String },
}

// ============================================================================
// REQUEST/RESPONSE TYPES
// ============================================================================

/// Active LoRA adapter to apply during generation
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai/ActiveAdapterRequest.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct ActiveAdapterRequest {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub domain: String,
    #[serde(default = "default_adapter_scale")]
    pub scale: f64,
}

fn default_adapter_scale() -> f64 {
    1.0
}

/// Text generation request
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai/TextGenerationRequest.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct TextGenerationRequest {
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub system_prompt: Option<String>,

    // Model config
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub repeat_penalty: Option<f32>,
    /// llama.cpp-native, UNWINDOWED repetition guard: scales each token's penalty by how
    /// often it has appeared across the ENTIRE generation (unlike `repeat_penalty`, which
    /// only scans the last `repeat_last_n` tokens). Catches gap-separated loops — a code
    /// block re-emitted many times regardless of the text between (#181). `None` → the
    /// adapter's llama.cpp default (0.3). Joins the Model row with the other sampling
    /// knobs under #76. Ignored by cloud OpenAI-compat providers.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub frequency_penalty: Option<f32>,
    /// Window (trailing tokens) that `repeat_penalty` scans on llama.cpp-
    /// family gateways. `None` → the gateway's own default (64). Widened
    /// by the substrate sampling defaults to catch loops whose span
    /// exceeds 64 tokens (#181). Ignored by cloud OpenAI-compat providers.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub repeat_last_n: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub stop_sequences: Option<Vec<String>>,

    // Tool calling (native JSON format)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tools: Option<Vec<NativeToolSpec>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tool_choice: Option<ToolChoice>,

    /// Force the model to output a specific format (e.g. JSON object).
    /// OpenAI-compatible: serializes as `{"type": "json_object"}` etc. The
    /// underlying llama.cpp / DMR pathway respects this and constrains the
    /// sampler so the model can ONLY emit valid JSON. Removes the
    /// "qwen3.5 emits 'Thinking Process:' prose instead of JSON" failure
    /// mode at the source instead of papering over it with a parser
    /// fallback (banned by the 'no fallbacks' directive).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub response_format: Option<ResponseFormat>,

    // LoRA adapters
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub active_adapters: Option<Vec<ActiveAdapterRequest>>,

    // Request metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub room_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub purpose: Option<String>,
    /// Persona generating this request — the inference's "owner" for
    /// per-persona resource attribution (KV cache bytes, GPU pressure,
    /// recipe budgets). Wire format is a stringified UUID; the local
    /// adapter parses to `uuid::Uuid` at the Rust boundary. None = the
    /// inference is not attributable to a persona (test rigs, ad-hoc
    /// system probes, benchmarks). Production paths through
    /// PersonaResponseGenerator MUST set this — without it the registry
    /// can't tell whose conversation owns this seq's KV slot, and the
    /// pressure policy can't make per-persona eviction decisions.
    /// See docs/architecture/PERSONA-CONTEXT-PAGING.md §13.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub persona_id: Option<String>,
}

/// Constrains the model's output format. OpenAI-compatible serialization:
/// `{"type": "json_object"}` for `JsonObject`, `{"type": "text"}` for `Text`.
/// llama.cpp / DMR honors this by constraining the sampler so the model
/// can only emit valid JSON (when JsonObject) — no thinking prose, no
/// commentary, no leading/trailing text. The right way to enforce structured
/// output: at the model level, not via a downstream parser fallback.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai/ResponseFormat.ts"
)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseFormat {
    /// Model output is constrained to a single valid JSON object.
    JsonObject,
    /// Plain text output (default; equivalent to omitting response_format).
    Text,
}

/// Text generation response
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai/TextGenerationResponse.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct TextGenerationResponse {
    pub text: String,
    pub finish_reason: FinishReason,
    pub model: String,
    pub provider: String,
    pub usage: UsageMetrics,
    #[ts(type = "number")]
    pub response_time_ms: u64,
    pub request_id: String,

    /// Full content blocks (text + tool_use blocks)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub content: Option<Vec<ContentPart>>,

    /// Tool calls extracted from response (when finish_reason is ToolUse)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tool_calls: Option<Vec<ToolCall>>,

    /// The model's separated chain-of-thought / "thinking", when it is a reasoning
    /// model. SEPARATED FROM `text` at the adapter boundary so reasoning NEVER
    /// reaches the user/room — `text` is the clean answer; this is captured for the
    /// glass-box harness + memory consolidation. Sources: a server `reasoning_content`
    /// field, or inline `<think>…</think>` the adapter strips out. `None` for
    /// non-reasoning models or turns with no thinking.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reasoning: Option<String>,

    /// Routing info for observability
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub routing: Option<RoutingInfo>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,

    /// Per-call inference timing breakdown from the lane (llama-server `timings`):
    /// cached-prefix size, NEW prefill tokens + ms, decode tokens + ms, and the
    /// lane's own prefill/decode tok-s. Lets the harness separate PREFILL cost
    /// (re-encoding the prompt on a KV-cache miss) from DECODE cost (token
    /// generation) instead of one conflated wall-clock tok-s. `None` when the
    /// provider doesn't report timings.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub timing: Option<GenerationTiming>,
}

/// Per-call inference timing breakdown, sourced from llama-server's `timings`
/// object on the final stream frame. The split that matters: on Apple-Silicon
/// Metal prefill is only ~3-5× decode (vs CUDA's 20-50×), so re-prefilling a
/// ~2000-token prompt every settle-loop turn DOMINATES wall-clock — measured 77%
/// of eval time was prefill, not generation. `cached_tokens` vs `prefill_tokens`
/// is the KV-cache hit/miss that governs that cost; a high cached fraction means
/// the static identity+catalog prefix stayed resident across turns.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai/GenerationTiming.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct GenerationTiming {
    /// KV-prefix tokens reused from cache this call (llama `cache_n`).
    pub cached_tokens: u32,
    /// NEW prompt tokens that had to be prefilled this call (llama `prompt_n`).
    /// This is the re-rasterization tax the KV cache exists to avoid.
    pub prefill_tokens: u32,
    /// Wall-ms spent prefilling the new tokens (llama `prompt_ms`).
    pub prefill_ms: f64,
    /// Lane prefill throughput, tok/s (llama `prompt_per_second`).
    pub prefill_tokens_per_second: f64,
    /// Tokens generated this call (llama `predicted_n`).
    pub decode_tokens: u32,
    /// Wall-ms spent decoding (llama `predicted_ms`).
    pub decode_ms: f64,
    /// Lane decode throughput — the REAL tok-s, undiluted by prefill
    /// (llama `predicted_per_second`).
    pub decode_tokens_per_second: f64,
}

/// Finish reason for generation
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/ai/FinishReason.ts")]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ToolUse,
    Error,
}

impl std::fmt::Display for FinishReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FinishReason::Stop => write!(f, "stop"),
            FinishReason::Length => write!(f, "length"),
            FinishReason::ToolUse => write!(f, "tool_use"),
            FinishReason::Error => write!(f, "error"),
        }
    }
}

/// Token usage metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/ai/UsageMetrics.ts")]
#[serde(rename_all = "camelCase")]
pub struct UsageMetrics {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub estimated_cost: Option<f64>,
}

/// Routing observability info
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/ai/RoutingInfo.ts")]
#[serde(rename_all = "camelCase")]
pub struct RoutingInfo {
    pub provider: String,
    pub is_local: bool,
    pub routing_reason: String,
    #[serde(default)]
    pub adapters_applied: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model_mapped: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model_requested: Option<String>,
}

/// Provider health status
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/ai/HealthStatus.ts")]
#[serde(rename_all = "camelCase")]
pub struct HealthStatus {
    pub status: HealthState,
    pub api_available: bool,
    #[ts(type = "number")]
    pub response_time_ms: u64,
    pub error_rate: f32,
    #[ts(type = "number")]
    pub last_checked: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/ai/HealthState.ts")]
#[serde(rename_all = "snake_case")]
pub enum HealthState {
    Healthy,
    Degraded,
    Unhealthy,
    InsufficientFunds,
    RateLimited,
}

/// Model information — ALL fields REQUIRED.
/// The adapter knows its model. No optionals, no defaults, no guessing.
/// If an adapter can't provide a field, it's not ready to register.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/ai/ModelInfo.ts")]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    /// What this model can do. ONE capability vocabulary across the whole
    /// substrate: [`crate::model_registry::Capability`]. There is no second
    /// enum and no bool mirror — modality routing (vision/audio bridge),
    /// tool-use gating, streaming, and embedding/image-gen support all read
    /// this set via [`ModelInfo::has`]. (#55 / #65 capability collapse —
    /// ModelCapability + ModalitySet deleted; #66 — supports_* bools deleted.)
    /// `Vec` (not the internal `BTreeSet`) is the idiomatic JSON-array shape
    /// for this wire DTO; the registry hands it an already-deduped set.
    pub capabilities: Vec<crate::model_registry::Capability>,
    pub context_window: u32,
    pub max_output_tokens: u32,
    pub cost_per_1k_tokens: CostPer1kTokens,
    /// Measured or estimated inference speed on current hardware.
    /// Used by RAG budget and slot coordination.
    #[ts(type = "number")]
    pub tokens_per_second: f32,
}

impl ModelInfo {
    /// Does this model declare `cap`? The ONE accessor — streaming, tool-use,
    /// vision, embedding all resolve here, never a bool mirror that can drift
    /// from `capabilities`. Mirrors [`crate::ai::adapter::AdapterCapabilities::has`].
    pub fn has(&self, cap: crate::model_registry::Capability) -> bool {
        self.capabilities.contains(&cap)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai/CostPer1kTokens.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct CostPer1kTokens {
    pub input: f64,
    pub output: f64,
}

/// Embedding request
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai/EmbeddingRequest.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingRequest {
    pub input: EmbeddingInput,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub provider: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai/EmbeddingInput.ts"
)]
#[serde(untagged)]
pub enum EmbeddingInput {
    Single(String),
    Multiple(Vec<String>),
}

/// Embedding response
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai/EmbeddingResponse.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingResponse {
    pub embeddings: Vec<Vec<f32>>,
    pub model: String,
    pub provider: String,
    pub usage: UsageMetrics,
    #[ts(type = "number")]
    pub response_time_ms: u64,
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

impl ChatMessage {
    /// Create a simple text message
    pub fn text(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: MessageContent::Text(content.into()),
            name: None,
        }
    }

    /// Create a message with tool result
    pub fn tool_result(
        tool_use_id: impl Into<String>,
        content: impl Into<String>,
        is_error: bool,
    ) -> Self {
        Self {
            role: "user".to_string(),
            content: MessageContent::Parts(vec![ContentPart::ToolResult {
                tool_use_id: tool_use_id.into(),
                content: content.into(),
                is_error: if is_error { Some(true) } else { None },
            }]),
            name: None,
        }
    }

    /// An assistant turn that REQUESTED a batch of tool calls — the
    /// agent-transcript shape a provider expects echoed back before the
    /// matching results, so its next generation sees what it asked for.
    pub fn assistant_tool_use(calls: &[ToolCall]) -> Self {
        Self {
            role: "assistant".to_string(),
            content: MessageContent::Parts(
                calls
                    .iter()
                    .map(|c| ContentPart::ToolUse {
                        id: c.id.clone(),
                        name: c.name.clone(),
                        input: c.input.clone(),
                    })
                    .collect(),
            ),
            name: None,
        }
    }

    /// A user turn carrying the RESULTS of a batch of tool calls, each paired
    /// by `tool_use_id` to the assistant turn that requested it. The companion
    /// to [`assistant_tool_use`](Self::assistant_tool_use) — together they form
    /// one agent round in the message thread.
    pub fn tool_results(results: &[ToolResult]) -> Self {
        Self {
            role: "user".to_string(),
            content: MessageContent::Parts(
                results
                    .iter()
                    .map(|r| ContentPart::ToolResult {
                        tool_use_id: r.tool_use_id.clone(),
                        content: r.content.clone(),
                        is_error: r.is_error,
                    })
                    .collect(),
            ),
            name: None,
        }
    }

    /// Get content as plain text (extracts from parts if needed)
    pub fn content_text(&self) -> String {
        match &self.content {
            MessageContent::Text(s) => s.clone(),
            MessageContent::Parts(parts) => parts
                .iter()
                .filter_map(|p| match p {
                    ContentPart::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(""),
        }
    }
}

impl TextGenerationResponse {
    /// Check if response has tool calls
    pub fn has_tool_calls(&self) -> bool {
        self.tool_calls
            .as_ref()
            .map(|tc| !tc.is_empty())
            .unwrap_or(false)
    }
}

impl HealthStatus {
    /// A nominal-healthy status — the sensible default for adapters that
    /// don't run a real probe (in-process local adapters, test fixtures,
    /// heuristic adapters). Cloud adapters that DO probe their endpoint
    /// override `health_check` and build a status from the live result.
    pub fn healthy() -> Self {
        Self {
            status: HealthState::Healthy,
            api_available: true,
            response_time_ms: 0,
            error_rate: 0.0,
            last_checked: 0,
            message: None,
        }
    }
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self {
            status: HealthState::Unhealthy,
            api_available: false,
            response_time_ms: 0,
            error_rate: 1.0,
            last_checked: 0,
            message: Some("Not checked".to_string()),
        }
    }
}

// ============================================================================
// TESTS TO GENERATE TS TYPES
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_ai_types() {
        // These tests trigger ts-rs to generate TypeScript types
        // Run: cargo test --package continuum-core -- --test-threads=1
        let cfg = ts_rs::Config::default();
        ChatMessage::export(&cfg).expect("export ChatMessage");
        MessageContent::export(&cfg).expect("export MessageContent");
        ContentPart::export(&cfg).expect("export ContentPart");
        ImageInput::export(&cfg).expect("export ImageInput");
        AudioInput::export(&cfg).expect("export AudioInput");
        VideoInput::export(&cfg).expect("export VideoInput");
        NativeToolSpec::export(&cfg).expect("export NativeToolSpec");
        ToolInputSchema::export(&cfg).expect("export ToolInputSchema");
        ToolCall::export(&cfg).expect("export ToolCall");
        ToolResult::export(&cfg).expect("export ToolResult");
        ToolChoice::export(&cfg).expect("export ToolChoice");
        ActiveAdapterRequest::export(&cfg).expect("export ActiveAdapterRequest");
        TextGenerationRequest::export(&cfg).expect("export TextGenerationRequest");
        TextGenerationResponse::export(&cfg).expect("export TextGenerationResponse");
        FinishReason::export(&cfg).expect("export FinishReason");
        UsageMetrics::export(&cfg).expect("export UsageMetrics");
        RoutingInfo::export(&cfg).expect("export RoutingInfo");
        HealthStatus::export(&cfg).expect("export HealthStatus");
        HealthState::export(&cfg).expect("export HealthState");
        ModelInfo::export(&cfg).expect("export ModelInfo");
        CostPer1kTokens::export(&cfg).expect("export CostPer1kTokens");
        EmbeddingRequest::export(&cfg).expect("export EmbeddingRequest");
        EmbeddingInput::export(&cfg).expect("export EmbeddingInput");
        EmbeddingResponse::export(&cfg).expect("export EmbeddingResponse");
    }
}
