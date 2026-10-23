//! Boot phase 13: the last module registrations (sentinel, tool parsing,
//! plasticity, avatar, dataset, vision, grid).

use super::*;

/// Returns the grid state the command executor's GridInterceptor routes on.
pub(super) fn register_closing_modules(
    runtime: &Arc<Runtime>,
    gpu_manager: &Arc<GpuMemoryManager>,
    code_state: &Arc<CodeState>,
) -> Arc<crate::modules::grid::GridState> {
    // SentinelModule: Concurrent, fault-tolerant build/task execution
    // Provides sentinel/execute, sentinel/status, sentinel/cancel, sentinel/list
    // And sentinel/logs/list, sentinel/logs/read, sentinel/logs/tail
    // Process isolation via child processes - safe for Xcode, cargo, etc.
    let sentinel_module = Arc::new(SentinelModule::new());
    runtime.register(sentinel_module.clone());
    crate::modules::sentinel::register_for_shutdown(sentinel_module);

    // PlasticityModule: Adaptive neural plasticity optimization engine
    // Provides plasticity/analyze, plasticity/compact, plasticity/topology
    // Per-head utilization-aware pruning, mixed-precision quantization, GQA-aware
    runtime.register(Arc::new(crate::modules::plasticity::PlasticityModule::new()));

    // AvatarModule: Bevy 3D avatar snapshots for profile pictures
    // Provides avatar/snapshot — allocates render slot, captures frame, saves PNG.
    // Registered in every build ("avatar" is a required MODULES row); without the
    // `avatar-3d` feature its capture refuses, naming the feature.
    runtime.register(Arc::new(AvatarModule::new()));

    // DatasetModule: Training dataset import and management
    // Provides dataset/import-csv, dataset/import-realclasseval, dataset/list, dataset/info
    runtime.register(Arc::new(DatasetModule::new()));

    // VisionModule: Content-addressed cache + event notification for vision descriptions
    // Provides vision/description-get, vision/description-put, vision/description-status,
    // vision/cache-stats, vision/cache-warm, vision/cache-evict
    runtime.register(Arc::new(VisionModule::new(code_state.clone())));

    // GridModule: inter-node transport + routing (Tailscale, Reticulum)
    let grid_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".continuum")
        .join("grid");
    let local_has_gpu = gpu_manager.total_vram_bytes() > 0;
    let local_vram_mb = gpu_manager.total_vram_bytes() / (1024 * 1024);
    // Keep a handle on the GridModule's state so we can build the
    // GridInterceptor below. The interceptor needs the same router +
    // node registry + transports the GridModule itself runs on; using
    // the public `state()` getter avoids duplicating any of that.
    let grid_module = Arc::new(GridModule::new(grid_dir, local_has_gpu, local_vram_mb));
    let grid_state = grid_module.state();
    runtime.register(grid_module);
    grid_state
}
