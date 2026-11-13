//! Boot phase 3: the live model catalog and the ONE per-machine resource
//! authority (`ResourceDaemon`), built from the detected GPU and host memory.

use super::*;

pub(super) fn build_model_catalog_and_resource_daemon(
    rt_handle: &tokio::runtime::Handle,
    system_monitor: &Arc<SystemResourceMonitor>,
) -> (
    Arc<crate::model_registry::live::ModelCatalog>,
    Arc<crate::resources::ResourceDaemon>,
) {
    // The live model universe — the runtime-mutable watch-snapshot layer SEEDED
    // from the immutable registry global (initialized above at `init_global`).
    // Constructed HERE so the SAME `Arc<ModelCatalog>` is shared by the serving
    // daemon (which plans off its snapshot) and the `models/*` command surface
    // (which mutates it). One owner, one live universe: a `models/pull` that
    // flips a model Ready is seen by the very next serving tick — no reboot.
    // Timed: this sits inside the 164-line stretch between the `system` and `serving`
    // registrations, which `boot.construct` bills entirely to `serving` because
    // attribution happens on the NEXT registration. On BigMama that stretch is
    // 209,170 ms of a 232,429 ms construct_modules phase (M5: 14,270 ms total), and it
    // emits nothing — the row is an upper bound on a SPAN, not a constructor cost.
    // These three `time_sync!` seams exist to give the span real owners.
    // `probe!` (an EVENT), not `time_sync!` (a SPAN): span durations need a
    // span-aware subscriber, and nothing records them pre-bind — a first cut of this
    // used time_sync! and produced ZERO rows on a live boot while every
    // `boot.construct` event landed. Same class/field shape as #3742's marks so one
    // query covers both stretches.
    let t_catalog = std::time::Instant::now();
    let model_catalog = Arc::new(crate::model_registry::live::ModelCatalog::from_registry(
        crate::model_registry::global(),
    ));
    continuum_base::probe!(
        class = "boot.stretch",
        stretch = "serving_span",
        step = "model_catalog",
        ms = t_catalog.elapsed().as_millis() as u64,
        "pre-bind stretch step"
    );

    // The ONE per-machine resource authority (#56). Its VRAM ceiling comes from
    // the LIVE GpuMonitor (`gpu::monitor::detect()` — Metal + every NVIDIA host),
    // so serving leases against capacity net of Bevy/LiveKit + outstanding leases
    // rather than a fraction of *total* VRAM (the host_budget() OOM bug). Bytes
    // held off the top so we never lease the last sliver the driver/compositor
    // needs; the ceiling already nets out external *resident* usage, so this is a
    // small safety reserve, not a budget for the compositor.
    const GPU_SAFETY_RESERVE_BYTES: u64 = 512 * 1024 * 1024;
    // `gpu::monitor::detect()` (→ `MetalMonitor::new`/`NvidiaMonitor::new`) and
    // `ResourceDaemon::start()` each adopt the canonical Daemon base, which calls
    // `tokio::spawn` from inside the constructor to own its interval task. But
    // `start_server` runs on a plain `std::thread` (main spawns it OFF the runtime
    // so its blocking accept-loop never steals a tokio worker — see main.rs), so
    // there is no ambient reactor and those `tokio::spawn`s panic "no reactor
    // running". Enter the runtime context for exactly this daemon-construction
    // region. The guard is scoped so it drops here, well before the `block_on`
    // regions further down — we never hold a runtime-context guard across a
    // `block_on`.
    let resource_daemon = {
        let _rt_guard = rt_handle.enter();
        let mut capacity_sources: Vec<Arc<dyn crate::resources::CapacitySource>> = Vec::new();
        // Some(gpu working-set hint) when the detected GPU shares the host's memory — the
        // signal that memory must be governed as ONE pool rather than two ledgers.
        let mut unified_gpu_hint: Option<u64> = None;
        // Timed: a live device scan on the boot path (see the span note above).
        // `#3733` bounded `GpuMemoryManager::detect()`; this is a DIFFERENT probe
        // (MetalMonitor/NvidiaMonitor construction) and nothing has ever measured it.
        // The load-bearing seam. #3733 bounded nvidia-smi in
        // gpu/memory_manager.rs::detect_cuda only; NvidiaMonitor::new ->
        // NvidiaProbe::detect() reaches four more unbounded `.output()` calls in
        // gpu/backends/nvidia.rs (:215 :228 :243 :257). Boot cost on this host swings
        // 18 s to >300 s between runs, which is what an unbounded device probe under
        // contention looks like.
        let t_gpu = std::time::Instant::now();
        let detected_monitor = crate::gpu::monitor::detect();
        continuum_base::probe!(
            class = "boot.stretch",
            stretch = "serving_span",
            step = "gpu_monitor_detect",
            ms = t_gpu.elapsed().as_millis() as u64,
            "pre-bind stretch step"
        );
        match detected_monitor {
            Some(monitor) => {
                log_info!(
                    "ipc",
                    "server",
                    "ResourceGovernor: live VRAM scan via {} ({}) — VRAM is governed",
                    monitor.platform(),
                    monitor.device_name()
                );
                // Feed the SAME detected monitor to the system-resource snapshot
                // so the holistic SYS view (and the Positron SYS gauge) carries
                // live GPU stats next to cpu+mem — one probe, reused, never a
                // second `gpu::monitor::detect()`.
                system_monitor.attach_gpu_monitor(monitor.clone());
                // The SAME one probe also fixes the host's serving backend for the KV
                // cache decision (inference/kv_cache_plan.rs). Recorded here because
                // this is the only place the substrate asks the device what it is —
                // and because both consumers of that decision (the launcher's
                // `--cache-type-k/v` flags and the serving plan's resident-KV divisor)
                // must read ONE answer, not re-derive it independently.
                crate::inference::kv_cache_plan::record_host_backend(monitor.platform());
                unified_gpu_hint = match monitor.memory_mode() {
                    // UMA (Apple Silicon): VRAM and RAM are ONE physical pool. Registering
                    // a GPU source here would create the second independent ledger that
                    // `ResourceKind::Vram`'s doc forbids ("the authority's scan layer is
                    // responsible for not double-counting") — measured live as VRAM
                    // advertising 16.9 GB while RAM correctly reported 0. Both axes come
                    // from UnifiedMemoryPool below instead.
                    crate::gpu::monitor::MemoryMode::Unified => Some(monitor.total_bytes()),
                    // Discrete: the axes really are separate pools. Unchanged.
                    crate::gpu::monitor::MemoryMode::Discrete => {
                        capacity_sources.push(Arc::new(crate::resources::GpuCapacitySource::new(
                            monitor,
                            GPU_SAFETY_RESERVE_BYTES,
                        )));
                        None
                    }
                };
            }
            None => {
                // GpuMemoryManager::detect() already panics on a truly GPU-less host,
                // so reaching here means a GPU exists but has no live GpuMonitor
                // adapter yet — the non-NVIDIA Vulkan (VK_EXT_memory_budget/ash) gap.
                // Name it loudly: VRAM stays UNGOVERNED and serving fails closed
                // (host_budget caps at 0) rather than over-committing against a
                // fabricated number. Build the Vulkan adapter or run on Metal/NVIDIA.
                log_error!(
                    "ipc",
                    "server",
                    "ResourceGovernor: NO live GpuMonitor (nvidia-smi absent and the \
                     non-NVIDIA Vulkan VK_EXT_memory_budget adapter is not built) — \
                     VRAM is UNGOVERNED; serving will refuse until a live monitor exists"
                );
            }
        }
        // HOST MEMORY (#56). Until 2026-08-19 this vec held ONLY the GPU source, so
        // `capacity(Ram)` was 0 and therefore `available_for(_, Ram)` returned 0 for every
        // consumer, permanently — the whole RAM half of the governor was structurally
        // unreachable while looking finished. Nothing noticed because serving/Bevy/voice
        // lease VRAM; the first consumer to plan against RAM (benchmark staging) was refused
        // on a box with tens of gigabytes free. Unlike the GPU there is no detect() that can
        // fail: every host has RAM, so memory is governed unconditionally.
        //
        // WHICH shape depends on the hardware, which is why the GPU arm above reports its
        // memory_mode rather than the boot path guessing from target_os.
        match unified_gpu_hint {
            Some(gpu_hint) => {
                // UMA: ONE pool, two views. Honors the contract in `ResourceKind::Vram`'s
                // own doc — "the authority's scan layer is responsible for not
                // double-counting" — which was written, assigned here, and never built.
                let pool = Arc::new(crate::resources::UnifiedMemoryPool::with_default_reserve(
                    crate::resources::LiveHostMemory::new(),
                    gpu_hint,
                ));
                let views = pool.views();
                log_info!(
                    "ipc",
                    "server",
                    "ResourceGovernor: UNIFIED memory — VRAM and RAM are ONE pool, \
                     ceiling {} MB (GPU working-set hint {} MB); both axes report one \
                     measured usage so neither can grant into the other's bytes",
                    views.iter().map(|v| v.ceiling_bytes()).max().unwrap_or(0) / (1024 * 1024),
                    gpu_hint / (1024 * 1024)
                );
                capacity_sources.extend(views);
            }
            None => {
                // Discrete (or no GPU): RAM is its own pool, VRAM registered above if present.
                let host_ram = crate::resources::HostRamCapacitySource::with_default_reserve(
                    crate::resources::LiveHostMemory::new(),
                );
                log_info!(
                    "ipc",
                    "server",
                    "ResourceGovernor: host RAM governed — ceiling {} MB \
                     (reserve held back for the OS); VRAM is a separate pool",
                    crate::resources::CapacitySource::ceiling_bytes(&host_ram) / (1024 * 1024)
                );
                capacity_sources.push(Arc::new(host_ram));
            }
        }

        crate::resources::ResourceDaemon::start(
            capacity_sources,
            Vec::new(),
            crate::resources::DaemonConfig::default(),
        )
    };
    (model_catalog, resource_daemon)
}
