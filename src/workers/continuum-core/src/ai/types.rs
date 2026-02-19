//! AI Provider Types - Shared types for AI adapter system
//!
//! Single source of truth for AI types in Rust, exported to TypeScript via ts-rs.
//! Tool calling types enable PersonaUser to use native API tools.
//!
//! Generated TypeScript types are in: shared/generated/ai/

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// Chat message for text generation
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/ai/ChatMessage.ts")]
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
#[ts(export, export_to = "../../../shared/generated/ai/MessageContent.ts")]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

/// Content part for multimodal and tool protocol messages
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/ai/ContentPart.ts")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text { text: String },
    Image { image: ImageInput },
    Audio { audio: AudioInput },
    Video { video: VideoInput },
    ToolUse { id: String, name: String, #[ts(type = "Record<string, unknown>")] input: Value },
    ToolResult { tool_use_id: String, content: String, #[serde(skip_serializing_if = "Option::is_none")] is_error: Option<bool> },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/ai/ImageInput.ts")]
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
#[ts(export, export_to = "../../../shared/generated/ai/AudioInput.ts")]
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
#[ts(export, export_to = "../../../shared/generated/ai/VideoInput.ts")]
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
/// This must NOT use rename_all = "camelCase" because the wire format
/// from TypeScript AND the Anthropic API both use snake_case for this struct.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/ai/NativeToolSpec.ts")]
pub struct NativeToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: ToolInputSchema,
}

/// JSON Schema for tool input parameters.
/// Matches Anthropic API wire format (snake_case field names).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/ai/ToolInputSchema.ts")]
pub struct ToolInputSchema {
    #[serde(rename = "type")]
    pub schema_type: String, // Always "object"
    #[ts(type = "Record<string, unknown>")]
    pub properties: Value,   // JSON object describing properties
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub required: Option<Vec<String>>,
}

/// Tool call from AI response (when AI wants to use a tool)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/ai/ToolCall.ts")]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub id: String,          // Unique ID for this tool use (e.g., "toolu_01A...")
    pub name: String,        // Tool name
    #[ts(type = "Record<string, unknown>")]
    pub input: Value,        // Tool parameters as JSON
}

/// Tool result to send back to AI after execution
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/ai/ToolResult.ts")]
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
#[ts(export, export_to = "../../../shared/generated/ai/ToolChoice.ts")]
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
#[ts(export, export_to = "../../../shared/generated/ai/ActiveAdapterRequest.ts")]
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
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/ai/TextGenerationRequest.ts")]
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
    pub stop_sequences: Option<Vec<String>>,

    // Tool calling (native JSON format)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tools: Option<Vec<NativeToolSpec>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tool_choice: Option<ToolChoice>,

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
}

/// Text generation response
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/ai/TextGenerationResponse.ts")]
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

    /// Routing info for observability
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub routing: Option<RoutingInfo>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}

/// Finish reason for generation
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/ai/FinishReason.ts")]
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
#[ts(export, export_to = "../../../shared/generated/ai/UsageMetrics.ts")]
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
#[ts(export, export_to = "../../../shared/generated/ai/RoutingInfo.ts")]
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
#[ts(export, export_to = "../../../shared/generated/ai/HealthStatus.ts")]
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
#[ts(export, export_to = "../../../shared/generated/ai/HealthState.ts")]
#[serde(rename_all = "snake_case")]
pub enum HealthState {
    Healthy,
    Degraded,
    Unhealthy,
    InsufficientFunds,
    RateLimited,
}

/// Model capabilities
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/ai/ModelCapability.ts")]
#[serde(rename_all = "kebab-case")]
pub enum ModelCapability {
    TextGeneration,
    TextCompletion,
    Chat,
    AudioGeneration,
    AudioTranscription,
    ImageGeneration,
    ImageAnalysis,
    VideoGeneration,
    VideoAnalysis,
    Embeddings,
    Multimodal,
    ToolUse,
}

/// Model information
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/ai/ModelInfo.ts")]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub capabilities: Vec<ModelCapability>,
    pub context_window: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cost_per_1k_tokens: Option<CostPer1kTokens>,
    pub supports_streaming: bool,
    pub supports_tools: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/ai/CostPer1kTokens.ts")]
#[serde(rename_all = "camelCase")]
pub struct CostPer1kTokens {
    pub input: f64,
    pub output: f64,
}

/// Embedding request
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/ai/EmbeddingRequest.ts")]
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
#[ts(export, export_to = "../../../shared/generated/ai/EmbeddingInput.ts")]
#[serde(untagged)]
pub enum EmbeddingInput {
    Single(String),
    Multiple(Vec<String>),
}

/// Embedding response
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/ai/EmbeddingResponse.ts")]
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
    pub fn tool_result(tool_use_id: impl Into<String>, content: impl Into<String>, is_error: bool) -> Self {
        Self {
            role: "user".to_string(),
            content: MessageContent::Parts(vec![
                ContentPart::ToolResult {
                    tool_use_id: tool_use_id.into(),
                    content: content.into(),
                    is_error: if is_error { Some(true) } else { None },
                }
            ]),
            name: None,
        }
    }

    /// Get content as plain text (extracts from parts if needed)
    pub fn content_text(&self) -> String {
        match &self.content {
            MessageContent::Text(s) => s.clone(),
            MessageContent::Parts(parts) => {
                parts.iter()
                    .filter_map(|p| match p {
                        ContentPart::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("")
            }
        }
    }
}

impl TextGenerationResponse {
    /// Check if response has tool calls
    pub fn has_tool_calls(&self) -> bool {
        self.tool_calls.as_ref().map(|tc| !tc.is_empty()).unwrap_or(false)
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
        ChatMessage::export().expect("export ChatMessage");
        MessageContent::export().expect("export MessageContent");
        ContentPart::export().expect("export ContentPart");
        ImageInput::export().expect("export ImageInput");
        AudioInput::export().expect("export AudioInput");
        VideoInput::export().expect("export VideoInput");
        NativeToolSpec::export().expect("export NativeToolSpec");
        ToolInputSchema::export().expect("export ToolInputSchema");
        ToolCall::export().expect("export ToolCall");
        ToolResult::export().expect("export ToolResult");
        ToolChoice::export().expect("export ToolChoice");
        ActiveAdapterRequest::export().expect("export ActiveAdapterRequest");
        TextGenerationRequest::export().expect("export TextGenerationRequest");
        TextGenerationResponse::export().expect("export TextGenerationResponse");
        FinishReason::export().expect("export FinishReason");
        UsageMetrics::export().expect("export UsageMetrics");
        RoutingInfo::export().expect("export RoutingInfo");
        HealthStatus::export().expect("export HealthStatus");
        HealthState::export().expect("export HealthState");
        ModelCapability::export().expect("export ModelCapability");
        ModelInfo::export().expect("export ModelInfo");
        CostPer1kTokens::export().expect("export CostPer1kTokens");
        EmbeddingRequest::export().expect("export EmbeddingRequest");
        EmbeddingInput::export().expect("export EmbeddingInput");
        EmbeddingResponse::export().expect("export EmbeddingResponse");
    }
}
