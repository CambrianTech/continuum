//! Local Inference Module — llama.cpp-backed LLM Inference
//!
//! Production inference path is `LlamaCppAdapter` wrapping the bundled
//! `llama` crate (statically linked llama.cpp). The Candle-based path
//! (`CandleAdapter`, `ContinuumModel`, `quantized.rs`, the vendored
//! qwen3.5/qwen2/llama backends, the dispatch-policy `compute_router`,
//! the stub `metal_deltanet`) was deleted across #1262/#1273/#1274/
//! #1280 — it had been vestigial since the llama.cpp migration; only
//! `LlamaCppAdapter` was registered by `AIProviderModule::register_adapters`.
//!
//! What survives in `model.rs`: `rebuild_with_stacked_lora`, the in-memory
//! LoRA-merge helper used by `backends/llama_safetensors.rs`
//! (`CompactLlamaSafetensorsBackend` — itself test-only, exercised by
//! plasticity validation tests). Phase 2 of #1280 will delete the
//! safetensors backends + `rebuild_with_stacked_lora` together once
//! plasticity's LoRA training infrastructure is migrated or retired.
//!
//! Architecture:
//!   backends/           — `read_gguf_metadata` + `ModelBackend`/`ModelFormat`
//!                          types (still used by llamacpp_adapter for header
//!                          inspection; also hosts test-only safetensors
//!                          backends pending Phase 2 deletion)
//!   vendored/           — Vendored llama.cpp / metal helpers
//!   lora.rs             — LoRA weight loading and merging
//!   llamacpp_adapter.rs — Production AIProviderAdapter (in-process llama.cpp)
//!   ort_providers.rs    — ORT (ONNX Runtime) provider helpers
//!   recipe_budget.rs    — KV cache budget planning per recipe
//!   footprint_registry/ — VRAM/UMA footprint tracking
//!   kv_quant.rs         — KV cache quantization helpers
//!   model.rs            — Minimal: just `rebuild_with_stacked_lora`

pub mod airc_remote;
pub mod backends;
pub mod batching_probe;
pub mod child_log;
pub mod coordinator;
pub mod coordinator_pool;
pub mod footprint_registry;
pub mod handle_module;
pub mod handle_store;
pub mod kv_quant;
pub mod lane_args;
pub mod lane_health;
pub mod stream_liveness;
pub mod lane;
pub mod lane_pidfile;
pub mod lane_process;
pub mod lane_registry;
pub mod llama_server;
pub mod llamacpp_adapter;
pub mod slots;
pub mod llm_module;
pub mod llm_module_bus;
pub mod llm_module_service;
pub mod lora;
pub mod model;
pub mod model_commands;
pub mod ort_providers;
pub mod placement_capture;
pub mod recipe_budget;
pub mod throughput_expectation;
pub mod placement_watch;
pub mod vendored;
pub mod vision_sidecar;
pub mod wedge;

// Re-export commonly used types
pub use backends::{read_gguf_metadata, GenomeAdapter, ModelBackend, ModelFormat};
pub use llamacpp_adapter::{LlamaCppAdapter, LLAMACPP_PROVIDER_ID};
pub use lora::{load_lora_adapter, merge_lora_weight, LoRAWeights, LoadedAdapter};
pub use model::rebuild_with_stacked_lora;
pub use throughput_expectation::{
    baseline_for, classify_throughput, ThroughputBaseline, ThroughputVerdict, SEED_BASELINES,
};
