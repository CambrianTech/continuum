//! GPU Memory Manager — unified VRAM coordination for all GPU consumers.
//!
//! Three subsystems share a single GPU:
//! - Rendering: Bevy render targets, avatar models (~20-70MB)
//! - Inference: Candle model weights, KV cache, LoRA adapters (2-8GB+)
//! - TTS: TTS model weights (500MB-2GB)
//!
//! GpuMemoryManager detects real VRAM at startup (Metal/CUDA), enforces
//! per-subsystem budgets, and provides an RAII allocation guard pattern.

/// The GPU adapter contract: backends supply a sample, the base owns the rest.
/// Read this before adding a Vulkan/MLX/ROCm backend — implement `GpuDeviceProbe`,
/// never `GpuMonitor` directly.
pub mod device_probe;
/// Per-platform adapters. ALL `#[cfg(target_os = ...)]` for GPU backends lives in
/// `backends/mod.rs` and nowhere else — see that file for why.
pub mod backends;
pub mod eviction_registry;
pub mod memory_manager;
pub mod monitor;
pub mod tracker;

pub use eviction_registry::{
    make_entry, EvictableEntry, EvictionRegistry, EvictionRegistrySnapshot,
};
pub use memory_manager::{
    AllocationsByPriority, GpuAllocationGuard, GpuError, GpuMemoryManager, GpuPriority, GpuStats,
    GpuSubsystem, SubsystemStats, PRESSURE_CRITICAL, PRESSURE_HIGH, PRESSURE_WARNING,
};
// No `cfg` below this line, deliberately. `backends` has already answered the
// which-platform question, so there is no second gate here to drift onto the wrong
// item — the failure that took out both CI jobs on 2026-08-20.
#[cfg(target_os = "macos")]
pub use backends::{MetalMonitor, MetalProbe};
pub use backends::{NvidiaMonitor, NvidiaProbe};
pub use device_probe::{GpuDeviceProbe, GpuSample, MonitoredGpu};
pub use monitor::{GpuMonitor, GpuSnapshot, MockMonitor};
pub use tracker::GpuModelTracker;
