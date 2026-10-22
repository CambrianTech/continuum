//! AIProviderModule — Adapter-based AI provider system
//!
//! Uses the adapter pattern (like ORM) for pluggable AI providers.
//! Single entry point for all text generation with tool calling support.
//!
//! Supported providers (via adapters):
//! - DeepSeek (deepseek-chat, deepseek-reasoner)
//! - Anthropic (claude-sonnet-4-5, claude-opus-4, claude-3-5-haiku)
//! - OpenAI (gpt-4, gpt-4o)
//! - Together AI (llama-3.1-70b)
//! - Groq (llama-3.1-8b-instant)
//! - Fireworks (deepseek-v3)
//! - XAI (grok-3)
//! - Google (gemini-2.0-flash)
//!
//! Commands:
//! - ai/generate: Generate text with optional tool calling
//! - ai/providers/list: List available providers
//! - ai/providers/health: Check provider health

use crate::ai::{
    adapter::{AIProviderAdapter, InferenceDevice},
    AdapterRegistry, AnthropicAdapter, OpenAICompatibleAdapter, RoutingInfo, TextGenerationRequest,
    TextGenerationResponse,
};
use crate::runtime::{
    CommandResult, LateBound, ModuleConfig, ModuleContext, ModuleLogger, ModulePriority,
    ServiceModule,
};
use continuum_base::secrets::get_secret;
use async_trait::async_trait;
use once_cell::sync::Lazy;
use serde_json::Value;
use std::any::Any;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::sync::OnceLock;
use tokio::sync::RwLock;

mod dmr_provider_id;
pub use dmr_provider_id::*;
mod block;
mod a_i_provider_module;
pub use a_i_provider_module::*;

#[cfg(test)]
mod tests;
