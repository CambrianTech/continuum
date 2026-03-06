//! GPU Memory Manager — unified VRAM coordination for all GPU consumers.
//!
//! Three subsystems share a single GPU:
//! - Rendering: Bevy render targets, avatar models (~20-70MB)
//! - Inference: Candle model weights, KV cache, LoRA adapters (2-8GB+)
//! - TTS: TTS model weights (500MB-2GB)
//!
//! GpuMemoryManager detects real VRAM at startup (Metal/CUDA), enforces
//! per-subsystem budgets, and provides an RAII allocation guard pattern.

pub mod eviction_registry;
pub mod memory_manager;
pub mod tracker;

pub use eviction_registry::{
    make_entry, EvictableEntry, EvictionRegistry, EvictionRegistrySnapshot,
};
pub use memory_manager::{
    AllocationsByPriority, GpuAllocationGuard, GpuError, GpuMemoryManager, GpuPriority, GpuStats,
    GpuSubsystem, SubsystemStats, PRESSURE_CRITICAL, PRESSURE_HIGH, PRESSURE_WARNING,
};
pub use tracker::GpuModelTracker;
