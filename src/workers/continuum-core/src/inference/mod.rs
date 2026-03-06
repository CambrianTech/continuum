//! Local Inference Module - Candle-based LLM Inference
//!
//! Provides local model loading, text generation, and LoRA support
//! using Candle ML framework.
//!
//! Architecture:
//!   backends/           — ModelBackend trait + implementations (one per arch/format)
//!     mod.rs            — ModelBackend trait, unified generate(), factory functions
//!     llama_gguf.rs     — GGUF quantized Llama backend
//!     llama_safetensors.rs — BF16/FP32 safetensors Llama backend
//!   vendored/           — Vendored candle-transformers code with bug fixes
//!   model.rs            — Model loading utilities, LoRA merge, device selection
//!   quantized.rs        — GGUF model download and loading
//!   lora.rs             — LoRA weight loading and merging
//!   candle_adapter.rs   — AIProviderAdapter implementation (uses ModelBackend)

pub mod backends;
pub mod candle_adapter;
pub mod lora;
pub mod model;
pub mod quantized;
pub mod vendored;

// Re-export commonly used types
pub use backends::{
    generate, load_gguf_backend, read_gguf_metadata, GenomeAdapter, ModelBackend, ModelFormat,
};
pub use candle_adapter::CandleAdapter;
pub use lora::{load_lora_adapter, merge_lora_weight, LoRAWeights, LoadedAdapter};
pub use model::{load_model_by_id, rebuild_with_stacked_lora};
pub use quantized::{load_default_quantized, load_quantized_model};
