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
//! Architecture:
//!   backends/           — llama.cpp adapter/scheduler, `read_gguf_metadata`
//!   vendored/           — Vendored llama.cpp / metal helpers
//!   lora.rs             — LoRA weight loading and merging
//!   llamacpp_adapter.rs — Production AIProviderAdapter (in-process llama.cpp)
//!   ort_providers.rs    — ORT (ONNX Runtime) provider helpers
//!   recipe_budget.rs    — KV cache budget planning per recipe
//!   footprint_registry/ — VRAM/UMA footprint tracking
//!   kv_quant.rs         — KV cache quantization helpers

pub mod airc_remote;
pub mod backends;
pub mod batching_probe;
pub mod child_log;
pub mod engine_stall;
pub mod stderr_relay;
pub mod backend_receipt;
pub(crate) mod engine_install;
mod launch_inputs;
pub mod debug_build_watch;
pub mod coordinator;
pub mod coordinator_pool;
pub mod footprint_registry;
pub mod handle_module;
pub mod handle_store;
pub mod kv_cache_plan;
pub mod kv_quant;
pub mod lane_args;
pub mod lane_demand;
pub mod lane_health;
pub mod lane_send;
pub mod stream_liveness;
pub mod lane;
pub mod lane_pidfile;
pub mod lane_process;
pub mod decode_knee;
pub mod learn_grant;
pub mod engine_slots;
pub mod lane_footprint;
pub mod prefill_knee;
pub mod prefill_rate;
pub mod prefill_spike;
pub mod served_window_store;
pub mod serving_rates;
pub mod lane_registry;
pub mod weights_stamp;
pub mod engine_residency;
pub mod llama_server;
pub mod llamacpp_adapter;
pub mod measured_hold;
pub mod page_ref;
pub mod slots;
pub mod speed_pulse;
pub mod sse_stream;
pub mod llm_module;
pub mod llm_module_bus;
pub mod llm_module_service;
pub mod lora;
pub mod model_commands;
pub mod ort_providers;
pub mod placement_capture;
pub mod recipe_budget;
pub mod request_body;
pub mod governed_budget;
pub mod resource_admission;
pub mod serving_footprints;
pub mod serving_guard;
pub mod serving_holds;
pub mod serving_lifecycle;
pub mod single_purpose_lane;
pub mod training_admission;
pub mod throughput_expectation;
pub mod turn_admission;
pub mod turn_bound;
pub mod wire_tap;
pub mod placement_watch;
pub mod planning;
pub mod weight_residency;
pub mod vendored;
pub mod vision_sidecar;
pub mod wedge;

// Re-export commonly used types
pub use backends::read_gguf_metadata;
pub use llamacpp_adapter::{LlamaCppAdapter, LLAMACPP_PROVIDER_ID};
pub use lora::{load_lora_adapter, merge_lora_weight, LoRAWeights, LoadedAdapter};
pub use throughput_expectation::{
    baseline_for, classify_throughput, ThroughputBaseline, ThroughputVerdict, SEED_BASELINES,
};
