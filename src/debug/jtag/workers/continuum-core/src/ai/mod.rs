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
//! Usage:
//! ```rust
//! let mut registry = AdapterRegistry::new();
//! registry.register(Box::new(OpenAICompatibleAdapter::deepseek()), 0);
//! registry.register(Box::new(AnthropicAdapter::new()), 1);
//! registry.initialize_all().await?;
//!
//! let (provider_id, adapter) = registry.select(None, Some("deepseek-chat")).unwrap();
//! let response = adapter.generate_text(request).await?;
//! ```

pub mod adapter;
pub mod anthropic_adapter;
pub mod openai_adapter;
pub mod types;

// Re-export commonly used types
pub use adapter::{AdapterCapabilities, AdapterConfig, AdapterRegistry, AIProviderAdapter, ApiStyle};
pub use anthropic_adapter::AnthropicAdapter;
pub use openai_adapter::OpenAICompatibleAdapter;
pub use types::{
    ChatMessage, ContentPart, EmbeddingInput, EmbeddingRequest, EmbeddingResponse, FinishReason,
    HealthState, HealthStatus, MessageContent, ModelCapability, ModelInfo, NativeToolSpec,
    RoutingInfo, TextGenerationRequest, TextGenerationResponse, ToolCall, ToolChoice,
    ToolInputSchema, ToolResult, UsageMetrics,
};
