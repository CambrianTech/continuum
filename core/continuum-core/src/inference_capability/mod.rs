//! Inference capability surface — local-side only (PR-1 of GRID-INFERENCE-ROUTING).
//!
//! This module ships the **data + pure derivation** layer the supervisor
//! needs to describe what inference work this node can take. No grid
//! wiring, no broadcast, no async — just:
//!
//! - [`types`] — wire-shape (ts-rs camelCase): `InferenceKind`,
//!   `LatencyClass`, `HardwareProfile`, `InferenceCapability`,
//!   `NodeCapability`. Carried by PR-2 (`GridCapabilityAnnouncer`)
//!   across the mesh; consumed by PR-3 (`GridInferenceRouter`) when
//!   scoring placement.
//!
//! - [`probe`] — pure function `probe_inference_capabilities(hw)` that
//!   maps a hardware profile to its capability list. No IO, no globals
//!   — synthetic profiles for the four hardware tiers vhsm-d1f4 named
//!   (MacBook Air, M5 Pro, Blackwell, generic Dell) are testable
//!   directly.
//!
//! - [`registry`] — `NodeCapabilityRegistry` in-memory map of
//!   `node_id -> NodeCapability` with insert/remove/list/find_capable.
//!   PR-2 owns the announcer + locking; this layer is sync, single-threaded.
//!
//! ## Why pure-functions slice first
//!
//! Per the rate_proposals / generate_recipe PR-1 cadence: data + pure
//! derivation lands independently mergeable, with full test coverage,
//! before any IPC / async wiring. PR-2 stacks the announcer on this
//! surface; PR-3 stacks the router on PR-2.
//!
//! ## Failure-mode discipline (vhsm-d1f4 audit pass 1)
//!
//! - **No CPU fallback**: `probe_inference_capabilities` returns ZERO
//!   capabilities for a CPU-only node. The grid router seeing "0
//!   capabilities" + the supervisor admission gate failing > "GPU
//!   advertised, then mid-inference CPU degrade".
//! - **No hardcoded enums**: `InferenceKind(String)` newtype, not a
//!   const enum. New backends plug in without a schema change.
//! - **No `unwrap_or` / silent defaults**: every field carries explicit
//!   data; no "default to zero VRAM and pretend it works."

pub mod enforcement;
pub mod gguf_loader;
pub mod hw_probe;
pub mod probe;
pub mod registry;
pub mod residency;
pub mod types;

pub use enforcement::{enforce_residency, enforce_residency_with, ResidencyBlock};
pub use gguf_loader::read_qwen_model_metadata;
pub use hw_probe::{build_hardware_profile, probe_hardware_profile};
pub use probe::probe_inference_capabilities;
pub use registry::NodeCapabilityRegistry;
pub use residency::{
    check_residency_gate, select_backend, BackendChoice, BlockReason, QwenModelMetadata,
    ResidencyEvidence, ResidencyGateResult,
};
pub use types::{
    kinds, HardwareProfile, InferenceCapability, InferenceKind, LatencyClass, NodeCapability,
};
