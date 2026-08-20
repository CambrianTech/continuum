//! AI Provider Module - Unified AI Integration Layer in Rust
//!
//! Provides adapter-based AI provider system similar to ORM adapter pattern.
//! Supports multiple providers with consistent interface and tool calling.
//!
//! Architecture:
//! - `adapter.rs` - The adapter trait (like StorageAdapter for ORM)
//! - `types.rs` - Shared types including tool calling
//! - `openai_adapter.rs` - OpenAI-compatible providers (DeepSeek, Together, Groq, etc.)
//! - `anthropic_adapter.rs` - Anthropic Claude models
//!
//! Usage (init-then-register pattern, task #162):
//! ```rust
//! let mut registry = AdapterRegistry::new();
//!
//! let mut deepseek = OpenAICompatibleAdapter::from_registry("deepseek");
//! deepseek.initialize().await?;
//! registry.register(Arc::new(deepseek), 0);
//!
//! let mut anthropic = AnthropicAdapter::new();
//! anthropic.initialize().await?;
//! registry.register(Arc::new(anthropic), 1);
//!
//! let (provider_id, adapter) = registry.select(None, Some("deepseek-chat"), InferenceDevice::Auto).unwrap();
//! let response = adapter.generate_text(request).await?;
//! ```

pub mod adapter;
pub mod anthropic_adapter;
// HeuristicInferenceAdapter is gated behind `cfg(any(test, feature =
// "test-fixtures"))`. Production binaries built without the feature
// do not contain it at all — the compiler enforces what the doctrine
// requires per [[no-fallbacks-ever]] and [[no-if-statements-use-llms-
// for-cognition]]. Joel (2026-06-01): "You mix this fake shit in and
// it's going live ALL THE TIME. The fake shit is a CHOSEN model
// adapter no other form. Declaration." cfg gating IS the declaration.
#[cfg(any(test, feature = "test-fixtures"))]
pub mod heuristic_adapter;
pub mod inference_error;
pub mod json_in_prompt_tools;
pub mod openai_adapter;
pub mod openai_endpoints;
pub mod registry_bridge;
pub mod types;

// Re-export commonly used types
pub use adapter::{
    AIProviderAdapter, AdapterCapabilities, AdapterConfig, AdapterRegistry, AdapterSelectionError,
    ApiStyle, LoRAAdapterInfo, LoRACapabilities,
};
pub use anthropic_adapter::AnthropicAdapter;
#[cfg(any(test, feature = "test-fixtures"))]
pub use heuristic_adapter::{
    HeuristicInferenceAdapter, HEURISTIC_DEFAULT_MODEL, HEURISTIC_PROVIDER_ID,
};
pub use openai_adapter::OpenAICompatibleAdapter;
pub use types::{
    ActiveAdapterRequest, ChatMessage, ContentPart, EmbeddingInput, EmbeddingRequest,
    EmbeddingResponse, FinishReason, HealthState, HealthStatus, MessageContent, ModelInfo,
    NativeToolSpec, RoutingInfo, TextGenerationRequest, TextGenerationResponse, ToolCall,
    ToolChoice, ToolInputSchema, ToolResult, UsageMetrics,
};
