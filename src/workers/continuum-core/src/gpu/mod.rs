//! GPU Memory Manager — unified VRAM coordination for all GPU consumers.
//!
//! Three subsystems share a single GPU:
//! - Rendering: Bevy render targets, avatar models (~20-70MB)
//! - Inference: Candle model weights, KV cache, LoRA adapters (2-8GB+)
//! - TTS: TTS model weights (500MB-2GB)
//!
//! GpuMemoryManager detects real VRAM at startup (Metal/CUDA), enforces
//! per-subsystem budgets, and provides an RAII allocation guard pattern.

pub mod memory_manager;
pub mod tracker;

pub use memory_manager::{
    GpuMemoryManager, GpuSubsystem, GpuAllocationGuard, GpuStats, SubsystemStats,
    GpuPriority, GpuError, AllocationsByPriority,
    PRESSURE_WARNING, PRESSURE_HIGH, PRESSURE_CRITICAL,
};
pub use tracker::GpuModelTracker;
