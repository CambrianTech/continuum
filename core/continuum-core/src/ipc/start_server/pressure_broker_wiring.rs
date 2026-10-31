//! Boot phase 5: the cross-pool PressureBroker, the inference lane
//! coordinator, and every ResourcePool attached to the broker.

use super::*;

pub(super) fn register_pressure_broker(
    runtime: &Arc<Runtime>,
    resource_daemon: &Arc<crate::resources::ResourceDaemon>,
    disk_pressure_monitor: &Arc<crate::system_resources::DiskPressureMonitor>,
    pressure_monitor: &Arc<crate::system_resources::MemoryPressureMonitor>,
) {
    // Phase 2 of #1239 (continuum#1299 PR-1): PressureBrokerModule.
    // Brings the cross-pool PressureBroker online — instantiates the
    // singleton, pre-registers DockerTierPool as a ResourcePool, and
    // hands the broker's `relieve()` tick to the runtime's standard
    // start_tick_loops() machinery (cadence = BrokerConfig.tick_interval,
    // default 5s, matching DMR_TICK_INTERVAL). Other pools (VRAM, KV
    // cache) attach via `module.broker().register(...)` from their own
    // construction sites. Observer-only in PR-1: no commands routed
    // here yet. PR-2 of #1299 adds `system/pressure-broker-state` IPC;
    // PR-3 wires the chat-substrate alert sink.
    let pressure_broker =
        Arc::new(crate::modules::pressure_broker_module::PressureBrokerModule::new());
    // The descriptor table has an owner: the `process-fds` tier restarts the daemon
    // this core spawned when the broker turns to it (2026-09-12: the gauge fired,
    // nothing followed, a human ran lsof).
    pressure_broker.broker().register(Arc::new(
        crate::system_resources::fd_pressure::FdPressurePool::with_relief(Box::new(
            crate::airc::fd_relief::DaemonRestartRelief,
        )),
    ));
    // The memory monitor says an unkillable anomaly in the org room; the room is the grid's,
    // so the grid hands its voice to the kernel here.
    crate::system_resources::memory_pressure::register_org_room_voice(|line| {
        Box::pin(async move { crate::modules::grid::say_in_org_room(&line).await })
    });
    runtime.register(pressure_broker);
    // InferenceCoordinatorModule — stands up the multi-persona-one-model
    // lane coordinator. Registered before the broker block below so its
    // CoordinatorResourcePool can be attached to the broker in the same
    // pass, closing the realistic-lane pressure→eviction loop in
    // production. HARDWARE-DETECTED silicon (probes the machine: Gpu on a
    // discrete GPU, UnifiedMemory on Apple Silicon, Cpu on a GPU-less host)
    // — no hardcoded CPU/UMA floor. Per-tier budgets governor-informed in a
    // later slice. Opens route through the same coordinator Arc via the
    // handle module.
    runtime.register(Arc::new(
        crate::modules::inference_coordinator_module::InferenceCoordinatorModule::with_detected_hardware(),
    ));
    // Register DiskPressureMonitor with the broker as a signal-only
    // ResourcePool — `evict_at_least` returns 0 (the monitor doesn't
    // own files), but the broker emits typed PressureAlert events on
    // tier transitions, surfacing disk pressure on the same wire +
    // dashboard as memory + Docker + LoRA + KV pools. Concrete disk
    // pools (genome cache, probe JSONL rotation, model registry) will
    // register their own ResourcePool impls in follow-up slices and
    // the broker will drive eviction against THOSE. Per task #88
    // "broker pool" half.
    let broker_arc = runtime
        .registry()
        .module_of_type::<crate::modules::pressure_broker_module::PressureBrokerModule>()
        .and_then(|m| {
            m.as_any()
                .downcast_ref::<crate::modules::pressure_broker_module::PressureBrokerModule>()
                .map(|bm| bm.broker())
        });
    if let Some(broker) = broker_arc {
        broker
            .register(disk_pressure_monitor.clone() as Arc<dyn crate::paging::pool::ResourcePool>);
        broker.register(pressure_monitor.clone() as Arc<dyn crate::paging::pool::ResourcePool>);

        // Cargo-target eviction owner (task #155 wire 2). The 2026-07-13
        // incident: the broker spent days emitting the designed zero-byte
        // "disk hot AND nobody owns the eviction" alerts while the unswept
        // cargo-target cache reached 363 GB. This pool owns that class:
        // budget-capped, flock-guarded against live builds, derived
        // artifacts only (safe on any user's machine — the next build
        // recreates everything it deletes). Shares its TrackedDir with the
        // disk reporter — one measurement per class.
        // Bind existing and later writer-selected roots through one registry;
        // each pool reads the same monitor snapshot for its actual owning volume.
        // Legacy roots remain owned after configuration changes (d86e79e8).
        if let Err(error) = crate::system_resources::disk_reporters::bind_cargo_owners(
            &disk_pressure_monitor,
            &broker,
        ) {
            crate::clog_warn!("Cargo disk ownership binding refused: {error}");
        }

        // Rotation-generation eviction owners (2026-08-06): the
        // substrate's own `logs` + `probes` dirs. These were the two
        // directories continuum writes to most continuously and the
        // only ones with no governed owner at all — `capped_appender`
        // bounded itself with a private constant, which is a bound but
        // not an authority: the broker could not reclaim a byte of it
        // under real disk pressure, and the probe sink on the actual
        // boot path wasn't even rotating. Eviction is safe by
        // construction here (rotated `.N` generations only, never the
        // live file), so this class is OWNED rather than deferred.
        // Citizen workspace eviction remains unregistered until preservation can
        // restore local commits and refs as well as dirty files. A clean checkout
        // can contain the only copy of a citizen's autosaved work (#3920).

        for class in ["logs", "probes"] {
            // The pool governs ONE writer's ledger (live file + its `.N` generations)
            // and nothing else in the directory — a stranger file must never make it
            // read over budget (card 4b496146: a 134 MB legacy capture did, and the
            // broker evicted every probe generation the moment rotation made it).
            let live_file = if class == "logs" {
                crate::routing::tracing_init::CORE_LOG_FILE
            } else {
                crate::routing::probe_file_sink::PROBE_LEDGER_FILE
            };
            if let Some(dir) = crate::system_resources::tracked_dir(class) {
                broker.register(Arc::new(crate::system_resources::RotationLogPool::new(
                    dir,
                    crate::routing::capped_appender::rotation_budget_bytes(),
                    live_file,
                ))
                    as Arc<dyn crate::paging::pool::ResourcePool>);
                log_info!(
                    "ipc",
                    "server",
                    "RotationLogPool registered with PressureBroker (class={class}, \
                     oldest-generation-first, live file never evicted)"
                );
            }
        }

        // NVMe serving-tier eviction owner (#302): the genome-models class
        // holds the HOT per-token-paged serving set (served GGUFs, expert
        // containers, device-fit overrides). Capacity derives from the
        // volume (total − governed reserve, #287-style); relief MIGRATES
        // frozen artifacts to the detected COLD drive (verified copy
        // before delete, never the actively-paged artifact, never a blind
        // delete). No cold drive ⇒ the pool refuses loudly — models are
        // re-fetch-hours, not derived artifacts.
        if let Some(models_dir) = crate::system_resources::tracked_dir("genome-models") {
            use crate::system_resources::drives::{detect_drives, drive_holding, DriveRole};
            let drives = detect_drives();
            // The volume holding the hot tier: longest mount-point prefix.
            let volume_total = drive_holding(&drives, models_dir.path()).map(|d| d.total_bytes);
            let cold_root = drives
                .iter()
                .find(|d| d.role == DriveRole::Cold)
                .map(|d| d.mount.join("continuum-cold").join("models"));
            if let Some(volume_total) = volume_total {
                broker.register(Arc::new(crate::system_resources::NvmeServingTierPool::new(
                    models_dir,
                    volume_total,
                    cold_root.clone(),
                    crate::system_resources::serving_active_artifacts(),
                ))
                    as Arc<dyn crate::paging::pool::ResourcePool>);
                log_info!(
                    "ipc",
                    "server",
                    "NvmeServingTierPool registered with PressureBroker (derived capacity, cold_root={:?})",
                    cold_root
                );
            } else {
                log_info!(
                    "ipc",
                    "server",
                    "NvmeServingTierPool NOT registered — no volume matched the models dir (class stays report-only)"
                );
            }
        }

        // Wire the resource authority's per-kind lease pools onto the broker so
        // cross-resource pressure relief reaches VRAM/RAM/disk leases: when a
        // kind goes over its scanned ceiling (a game grabs VRAM), the broker's
        // tick asks the lease pool to free bytes and the daemon's reconcile
        // claws them back from the lowest-priority leaseholder. One orchestrator
        // (the broker), not a parallel one. (#56)
        resource_daemon.register_with_broker(&broker);
        log_info!(
            "ipc",
            "server",
            "ResourceDaemon lease pools registered with PressureBroker (VRAM/RAM/disk leases)"
        );

        // Register a `FilesystemTierPool` for the probe JSONL
        // rotation dir — closes the broker → pool → real eviction
        // loop end-to-end. When disk pressure crosses the broker's
        // act_above threshold, the broker walks the registered pools
        // and asks each to free bytes; this pool actually does it
        // (deletes oldest probe files until want_bytes are freed).
        //
        // The probe sink's own `tracing_appender::rolling` handles
        // the time axis (daily rotation, 7-day retention); this pool
        // handles the space-pressure axis (broker-driven eviction
        // when disk is hot). Both bounds active simultaneously.
        //
        // Soft cap = 500 MB. Picked to be larger than a typical
        // 7-day probe window (probably < 100 MB even under heavy
        // capture) but bounded so a runaway sprinkle / leak gets
        // visible to the broker before disk fills.
        const PROBE_POOL_SOFT_CAP_BYTES: u64 = 500 * 1024 * 1024;
        if let Ok(home) = crate::paths::continuum_home() {
            let probe_dir = home.join("jtag/logs/probes");
            broker.register(Arc::new(crate::paging::FilesystemTierPool::new(
                "probe-jsonl",
                probe_dir,
                PROBE_POOL_SOFT_CAP_BYTES,
            )) as Arc<dyn crate::paging::pool::ResourcePool>);
            log_info!(
                "ipc",
                "server",
                "FilesystemTierPool 'probe-jsonl' registered with PressureBroker (soft cap 500 MB)"
            );
        } else {
            log_error!(
                "ipc",
                "server",
                "no HOME — probe-jsonl FilesystemTierPool not registered"
            );
        }

        // Register a `FilesystemTierPool` for the tool-output spill dir — the
        // space-pressure axis of tier-2 flood protection. When a build/test/read
        // tool overflows the context budget, the executor spills the WHOLE result
        // to `~/.continuum/tool-output/<persona_id>/<handle>.log` so the persona
        // can grep it back with `tool/output`. Those artifacts accrete with no
        // intrinsic bound; this pool lets the broker delete the oldest spills
        // (recursively across persona dirs) when disk goes hot — exactly the
        // probe-jsonl pattern above, against the spill root.
        //
        // Soft cap = 1 GB. Larger than the probe pool because a single Xcode /
        // cargo build log can be tens of MB and many personas spill in parallel,
        // but bounded so a runaway tool can't fill the disk before the broker acts.
        // Path comes from `spill::spill_root()` — single source, no re-typed string.
        const SPILL_POOL_SOFT_CAP_BYTES: u64 = 1024 * 1024 * 1024;
        match crate::cognition::tool_executor::spill::spill_root() {
            Ok(spill_dir) => {
                broker.register(Arc::new(
                    crate::paging::FilesystemTierPool::new(
                        "tool-output-spill",
                        spill_dir,
                        SPILL_POOL_SOFT_CAP_BYTES,
                    )
                    // A handle a live mind still points at is never evicted (card 9197cd01).
                    .removing_with(Arc::new(
                        crate::cognition::tool_executor::spill::evict_unless_live,
                    )),
                )
                    as Arc<dyn crate::paging::pool::ResourcePool>);
                log_info!(
                    "ipc",
                    "server",
                    "FilesystemTierPool 'tool-output-spill' registered with PressureBroker (soft cap 1 GB)"
                );
            }
            Err(e) => {
                log_error!(
                    "ipc",
                    "server",
                    "no HOME — tool-output-spill FilesystemTierPool not registered: {e}"
                );
            }
        }

        log_info!(
            "ipc",
            "server",
            "Disk + Memory pressure monitors registered as ResourcePools ('disk-root', 'sys-memory') with PressureBroker"
        );

        // Register the inference lane coordinator's pool so the broker's
        // tick drives lane eviction (expired → Hard → Graceful, never an
        // active Pinned lane) the same way it drives disk/Docker eviction.
        // Until opens route through the coordinator (follow-up slice) the
        // pool is armed but idle (usage 0 → Normal tier → broker no-ops),
        // which is the correct inert state for the not-yet-trafficked path.
        let coordinator_pool_registered = runtime
            .registry()
            .module_of_type::<crate::modules::inference_coordinator_module::InferenceCoordinatorModule>()
            .and_then(|m| {
                m.as_any()
                    .downcast_ref::<crate::modules::inference_coordinator_module::InferenceCoordinatorModule>()
                    .map(|cm| cm.register_with_broker(&broker))
            })
            .is_some();
        if coordinator_pool_registered {
            log_info!(
                "ipc",
                "server",
                "InferenceCoordinator lanes registered as ResourcePool ('inference-lanes') with PressureBroker"
            );
        } else {
            // Mirrors the parent broker-fetch failure log: a load-bearing
            // registration that silently no-ops would hide the lane pool
            // from pressure relief. Unreachable today (the module is
            // registered unconditionally above) but logged for symmetry.
            log_error!(
                "ipc",
                "server",
                "InferenceCoordinatorModule not retrievable after registration — lane pool won't appear on the broker"
            );
        }
    } else {
        log_error!(
            "ipc",
            "server",
            "PressureBrokerModule not retrievable after registration — pressure monitors won't appear on the broker"
        );
    }
}
