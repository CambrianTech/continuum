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
pub mod eviction_registry;
pub mod memory_manager;
#[cfg(target_os = "macos")]
pub mod metal_monitor;
pub mod monitor;
// NvidiaMonitor is pure subprocess (`nvidia-smi`) + parsing — no NVIDIA
// FFI — so it compiles on every platform (its parser tests run on the Mac
// dev box). `new()` returns None where `nvidia-smi` is absent, so building
// it everywhere costs nothing on non-NVIDIA hosts.
pub mod nvidia_monitor;
pub mod tracker;

pub use eviction_registry::{
    make_entry, EvictableEntry, EvictionRegistry, EvictionRegistrySnapshot,
};
pub use memory_manager::{
    AllocationsByPriority, GpuAllocationGuard, GpuError, GpuMemoryManager, GpuPriority, GpuStats,
    GpuSubsystem, SubsystemStats, PRESSURE_CRITICAL, PRESSURE_HIGH, PRESSURE_WARNING,
};
// `device_probe` is the PLATFORM-NEUTRAL base every backend implements — it must be
// exported everywhere. `metal_monitor` is the macOS adapter and carries the gate that
// its `mod` declaration already has. Getting these two the wrong way round is exactly
// what shipped: the cfg attribute binds to the NEXT item only, so inserting the
// device_probe line above metal_monitor silently moved the gate onto the base and left
// the macOS-only adapter exported on every target. Compiled clean on this Mac, E0432
// on every other host.
pub use device_probe::{GpuDeviceProbe, GpuSample, MonitoredGpu};
#[cfg(target_os = "macos")]
pub use metal_monitor::{MetalMonitor, MetalProbe};
pub use monitor::{GpuMonitor, GpuSnapshot, MockMonitor};
pub use nvidia_monitor::{NvidiaMonitor, NvidiaProbe};
pub use tracker::GpuModelTracker;
