//! AI Provider Adapter Trait - The AI abstraction interface
//!
//! All AI providers implement this trait. The AIProviderModule works with
//! this trait, never with concrete implementations directly.
//!
//! Supported backends:
//! - OpenAI (GPT models)
//! - Anthropic (Claude models)
//! - DeepSeek
//! - Together AI
//! - Groq
//! - Fireworks
//! - XAI (Grok)
//! - Google (Gemini)
//! - Local (Candle, llama.cpp)

use crate::clog_warn;
use crate::model_registry::{Capability, ProviderKind, ToolProtocol};
use async_trait::async_trait;
use std::collections::BTreeSet;
use std::sync::Arc;

use super::types::{
    EmbeddingRequest, EmbeddingResponse, HealthStatus, ModelInfo, TextGenerationRequest,
    TextGenerationResponse,
};

mod inference_device;
pub use inference_device::*;
mod adapter_selection_error;
pub use adapter_selection_error::*;
mod global_registry;
pub use global_registry::global_registry;

// ArcAdapterShim + register_arc were deleted in task #162's Box→Arc
// migration. The registry stores Arc natively; callers pass
// `Arc::new(adapter)` directly to `register`. The shim was a
// transitional wrapper; the elegant intentional architecture is the
// Arc-native registry above.
#[cfg(test)]
mod tests;
