//! Local Inference Module - Candle-based LLM Inference
//!
//! Provides local model loading, text generation, and LoRA support
//! using Candle ML framework. This module is absorbed from the former
//! inference-grpc worker to provide unified AI provider interface.
//!
//! Features:
//! - HuggingFace model loading (Llama architecture)
//! - Quantized model support (GGUF Q4_K_M, Q8_0)
//! - LoRA adapter loading and weight merging
//! - Multi-adapter "genome" stacking
//! - GPU acceleration (Metal/CUDA) with proper sync
//!
//! Architecture:
//! - `model.rs` - Model loading and text generation
//! - `quantized.rs` - Quantized GGUF model support
//! - `lora.rs` - LoRA weight loading and merging
//! - `candle_adapter.rs` - AIProviderAdapter implementation

pub mod lora;
pub mod model;
pub mod quantized;
pub mod candle_adapter;

// Re-export commonly used types
pub use lora::{LoRAWeights, LoadedAdapter, load_lora_adapter, merge_lora_weight};
pub use model::{ModelState, GenomeAdapter, generate_text, load_model_by_id, rebuild_with_lora_from_paths, rebuild_with_stacked_lora};
pub use quantized::{QuantizedModelState, generate_text_quantized, load_quantized_model, load_default_quantized};
pub use candle_adapter::CandleAdapter;
