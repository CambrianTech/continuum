//! Budget ownership for the serving daemon.

use super::*;

/// The host's PHYSICAL serving capacity — sized against wherever the KV will
/// actually live, which is a function of the PLACEMENT DECISION and never of the
/// device that happens to be installed.
///
/// A CPU-placed host (`CONTINUUM_SERVING_PLACEMENT=cpu` — the Intel-Mac arm, #3729 /
/// #129, which builds llama-server with Metal OFF) runs `--n-gpu-layers 0`: every
/// weight and the entire KV cache live in SYSTEM RAM and the card holds literally
/// zero bytes. Sizing its window against VRAM bounds the whole node by a decorative
/// GPU. Measured on the Intel tier 2026-09-17 (#4146 regression): a 3.9 GB Radeon Pro
/// 560X reporting `used_mb 0.0`, plan 33,527 ctx, the kept window bound to
/// `MIN_SERVE_CTX` = 2,048, every ~2,300-token prompt hard-refused by the engine,
/// 36/36 `settle.inference_failed` and ZERO acts across eight citizens — while the
/// same daemon had already handed that engine `--cache-ram 13452`, i.e. 13.4 GB of
/// the RAM this function refused to look at.
///
/// READ THIS WITH ITS MIRROR IMAGE. `board_authoritative_host_budget` deliberately
/// does NOT clamp to system RAM, because on UMA the raw `vm_stat` figure under-reports
/// and "would reject a model that physically fits (the same clamp that floored the
/// served window to 2048; glass-boxed 2026-07-20)". Both single-source choices produce
/// the IDENTICAL 2,048 floor from OPPOSITE directions, which is the tell: the quantity
/// was never one thing. It is VRAM when layers are offloaded and system RAM when they
/// are not, and a name that means two physical quantities depending on a decision made
/// elsewhere is the substrate defect shape
/// ([[one-value-two-meanings-is-the-substrate-defect-shape]]). Hence: branch on the
/// decision, do not pick a winner between the two sources.
///
/// Pure (placement + both capacities + fraction are INPUTS) so the arithmetic is
/// assertable with no GPU, no daemon and no host, exactly as `host_budget_from` is.
pub fn physical_usable_bytes(
    placement: crate::inference::llama_server::LanePlacement,
    total_vram_bytes: u64,
    total_ram_bytes: u64,
    fraction: f64,
) -> u64 {
    use crate::inference::llama_server::LanePlacement;
    // TOTAL, not available, on both arms — this is the "may never sink below" floor,
    // and the live figure is exactly the teardown transient the wedge-heal law exists
    // to see through (see `physical_budget`'s caller).
    let physical = match placement {
        LanePlacement::Cpu => total_ram_bytes,
        LanePlacement::Gpu => total_vram_bytes,
    };
    (physical as f64 * fraction) as u64
}

/// Pure pin fit-decision — split from the live-budget read so it is unit-testable.
/// A pin SWAPS: `serve()` kills the incumbent llama-server child, THEN launches the
/// candidate — never co-resident — so the candidate only needs to fit AFTER the
/// incumbent's VRAM is reclaimed. `base` is the live budget WITH the incumbent's
/// weights still counted as USED; crediting `incumbent`'s weights back models the
/// eviction. Without it, a swap DOWN to a model that fits alone but not alongside
/// the outgoing one is falsely denied (glass-boxed 2026-07-21: pin Devstral 14.3GB
/// refused at "budget ~12.1GB" while a 32B teacher was resident, though evicting it
/// frees ~20GB — the stronger-teacher swap-and-back the Academy needs). WEIGHTS
/// only (deterministic): the incumbent's variable KV is freed too, so this stays
/// conservative — a `fits_on_gpu` verdict here always holds in the real
/// post-eviction budget. `candidate = None` ⇒ no GGUF on disk ⇒ not servable.
/// Does another engine fit beside what is already on the device? Pure so the
/// arithmetic is assertable without a GPU, a daemon, or a live budget — the same
/// split `pin_fit_decision` makes one screen below.
///
/// CO-RESIDENCE, not a swap: an ephemeral lane joins the live lane rather than
/// replacing it, so the incumbent is SPENT, never credited back. That single sign flip
/// is the whole difference from a pin's fit check, and getting it backwards would
/// admit exactly the second engine this exists to refuse.
///
/// A refusal states every term. An operator who cannot see which number blocked them
/// deletes the check ([[stdout-is-never-a-transport]] — the fact belongs in the
/// message that carries the decision).
pub(super) fn lane_admission_decision(
    budget_bytes: u64,
    want_bytes: u64,
    live_bytes: u64,
    sibling_bytes: u64,
    sibling_count: usize,
) -> crate::inference::llama_server::LaneAdmission {
    use crate::inference::llama_server::LaneAdmission;
    let committed = live_bytes.saturating_add(sibling_bytes);
    let headroom = budget_bytes.saturating_sub(committed);
    if want_bytes <= headroom {
        return LaneAdmission::Admit;
    }
    let gb = |b: u64| b as f64 / 1e9;
    LaneAdmission::Refuse(format!(
        "another {:.1} GB engine does not fit: budget {:.1} GB, already committed {:.1} GB          (live lane {:.1} GB + {} ephemeral sibling(s) {:.1} GB), headroom {:.1} GB.          Co-residence would time-slice one device, not add capacity.",
        gb(want_bytes),
        gb(budget_bytes),
        gb(committed),
        gb(live_bytes),
        sibling_count,
        gb(sibling_bytes),
        gb(headroom),
    ))
}

pub(super) fn pin_fit_decision(
    mut base: HostBudget,
    candidate: Option<ModelFootprint>,
    incumbent: Option<&ModelFootprint>,
) -> PinFit {
    if let Some(inc) = incumbent {
        base.usable_bytes = base.usable_bytes.saturating_add(inc.weights_bytes);
    }
    let budget_bytes = base.usable_bytes;
    let weights_bytes = candidate.as_ref().map(|f| f.weights_bytes).unwrap_or(0);
    // Fit verdict at ONE lane — "can this model hold a lane at all"; the live plan
    // sizes lanes from demand separately. footprint None → not servable (plan None);
    // footprint Some but over budget → plan_serving degrades with fits_on_gpu=false,
    // which `serving/pin` reads to refuse loud.
    let plan = candidate.and_then(|f| {
        plan_serving(
            &base,
            std::slice::from_ref(&f),
            &ServingDemand::new(1, None),
        )
    });
    PinFit {
        plan,
        weights_bytes,
        budget_bytes,
    }
}

/// The live serving budget for this host RIGHT NOW: free memory (capped at
/// physical VRAM, minus headroom) against the GOVERNED VRAM ceiling. A free
/// function so BOTH the daemon's autonomic tick ([`ServingDaemonModule::host_budget`])
/// and the `serving/pin` fit-gate derive the budget from the ONE source — a pin's
/// "will it fit" can never disagree with what the next tick would actually do.
pub fn live_host_budget(
    _system: &SystemResourceMonitor,
    resource_daemon: &ResourceDaemon,
) -> HostBudget {
    // Board-authoritative, same as the autonomic tick: the governed VRAM `available`
    // is the ONE source. We deliberately do NOT min it against `system` RAM
    // available — on UMA that raw vm_stat figure under-reports and would reject a model
    // that physically fits (the same clamp that floored the served window to 2048;
    // glass-boxed 2026-07-20). `_system` stays in the signature for call-site stability.
    let vram_ceiling =
        governed_vram_ceiling_or_report(resource_daemon, "board_authoritative_host_budget");
    host_budget_from(&HostBudgetInputs {
        available_bytes: vram_ceiling,
        total_vram_bytes: vram_ceiling,
        perf_cores: perf_cores(),
        budget_fraction: continuum_base::config_env::vram_headroom(),
    })
}

/// The serving host budget from the GOVERNED board ALONE — the memory authority's
/// pre-staged snapshot (Vram `available`, netted over every measured consumer + external
/// pressure), never a raw GPU probe. This is the ONE budget an ephemeral eval/train lane
/// sizes against at spawn (MEMORY-AUTHORITY-DAEMON slice 2): reading the board is reading a
/// snapshot the authority maintains on ITS tick, not sampling memory on the spawn hot path.
pub fn governed_host_budget(resource_daemon: &ResourceDaemon) -> HostBudget {
    let available = governed_vram_ceiling_or_report(resource_daemon, "governed_host_budget");
    host_budget_from(&HostBudgetInputs {
        available_bytes: available,
        total_vram_bytes: available,
        perf_cores: perf_cores(),
        budget_fraction: plan_fraction(continuum_base::config_env::vram_headroom()),
    })
}

/// Performance-core proxy for the lane cap. `num_cpus::get_physical()` is the
/// portable floor; on Apple Silicon it over-counts (efficiency cores), but the
/// `MAX_LANES` ceiling in the classifier is the binding cap on capable boxes,
/// so this only matters on tiny machines where physical-core count is a fine
/// proxy. Refined to true P-core detection in a later slice.
///
/// `pub` so off-daemon planners that build a one-shot [`HostBudget`] (e.g. the
/// ephemeral eval lane sizing its `-c` via [`plan_serving`]) derive the lane cap
/// from the SAME proxy, never a second constant.
pub fn perf_cores() -> u32 {
    num_cpus::get_physical().max(1) as u32
}

impl ServingDaemonModule {
    /// Honest serving budget for this host, RIGHT NOW — from the live free
    /// memory the monitor reports, capped at the device's physical VRAM. This
    /// is the organic signal: free memory drops under load, the budget shrinks,
    /// the plan picks fewer lanes / a smaller model; load clears, it flows back.
    /// Delegates to the free [`live_host_budget`] so the autonomic tick and the
    /// `serving/pin` fit-gate compute the budget from the ONE source.
    pub(super) fn host_budget_snapshot(&self) -> (HostBudget, Vec<crate::resources::lease::ResourceLease>) {
        // The autonomic plan's budget = the LIVE governed available of the lane's POOL
        // (`serving_pool_kind`: the Ram row for a CPU-placed lane, the Vram row otherwise;
        // the Vram wording below predates that split and reads for the GPU arm) — net of every
        // external consumer + the 512MB driver reserve the GpuCapacitySource already holds)
        // × the pressure-adaptive drive mode. ONE reserve, the DYNAMIC one: Performance
        // (hard task + room) floors the whole GPU (1.0), Comfort is the everyday 0.80, and
        // Eco (memory starved — a game opened, a crowded call) drops to 0.55 so the base
        // claims less and the rest of the call's KV + render still fit. #56/G8: we do NOT
        // ALSO apply the operator VRAM-headroom fraction here — stacking it under the mode
        // fraction double-discounted the everyday budget to ~0.64 (24.5GB of a 38GB board)
        // and left the more-capable 32B coder + full context unused, WITHOUT buying safety
        // (the concurrent-prefill compute buffer is reserved separately, window-scaled, in
        // the serving_plan fixpoint). The pin fit-gate still applies the operator headroom
        // (`config_env::vram_headroom()`, default 0.80) via `host_budget_from` — "can this
        // model physically fit" is a different question than "how much should the shared base
        // claim now." [[verify-real-device-numbers-not-a-clamp-premise]]
        // The governed VRAM board is the ONE authority for available VRAM: its `available`
        // is free-VRAM ALREADY netted over every measured consumer + external pressure AND
        // already ≤ physical VRAM. Trust it directly — do NOT `.min()` it against the raw
        // system-RAM "available" figure. On UMA (Apple Silicon) macOS's vm_stat under-reports
        // available RAM (wired/cached/compressed counted as used) FAR below the VRAM the GPU
        // can actually use, so the old `available.min(vram_ceiling)` starved a 42.7GB governed
        // budget down to macOS's 18GB reading — LESS than Devstral's own 14GB weights — and
        // floored a 128k-capable model to MIN_SERVE_CTX (2048) with ~28GB sitting free
        // (glass-boxed 2026-07-20). The raw-probe-overriding-the-board clamp is exactly the
        // anti-pattern the memory-authority arc exists to kill. Pressure sensing stays live
        // via the drive mode below (which still reads system available for the fraction).
        let available = self.system.snapshot().memory.available_bytes;
        let (measured, leases) = self.resource_daemon
            .replacement_budget_snapshot(SERVING_CONSUMER_ID, serving_pool_kind());
        let (live, charged) = if measured > 0 {
            (measured, leases)
        } else {
            // A substitute ceiling carries no authority to credit live holders.
            (governed_vram_ceiling_or_report(&self.resource_daemon, "host_budget"), Vec::new())
        };
        // LUDICROUS override: a declared benchmark/exam intent floors the whole GPU
        // (Performance, fraction 0.96) — the biggest window the model+machine allow, past the
        // conservative pressure read (which on UMA under-reports free memory). The drive mode
        // follows the ACTIVITY, not just the pressure. Otherwise the live pressure-adaptive
        // mode (a game opening still drops us to Eco). [[serving-mode-follows-activity-ludicrous-to-dream]]
        // The previous PRESSURE mode is the hysteresis state (a ludicrous override is
        // not pressure history): Eco is left only past the exit band, so a memory read
        // wandering across the 8 GiB line once a minute no longer flips the plan.
        use std::sync::atomic::{AtomicU8, Ordering};
        static LAST_MODE: AtomicU8 = AtomicU8::new(u8::MAX);
        static LAST_PRESSURE_MODE_ECO: std::sync::atomic::AtomicBool =
            std::sync::atomic::AtomicBool::new(false);
        let previous = if LAST_PRESSURE_MODE_ECO.load(Ordering::Relaxed) {
            Some(crate::capacity::power_mode::PowerMode::Eco)
        } else {
            Some(crate::capacity::power_mode::PowerMode::Comfort)
        };
        let pressure_mode =
            crate::capacity::power_mode::serving_mode_for_pressure_from(available, previous);
        LAST_PRESSURE_MODE_ECO.store(
            pressure_mode == crate::capacity::power_mode::PowerMode::Eco,
            Ordering::Relaxed,
        );
        // Ludicrous (an exam) floors it; pressure (Eco) drops it; otherwise the
        // operator's headroom policy names the everyday mode — the SAME knob the pin
        // fit-gate budgets with, composed here as a mode rather than stacked under one.
        let mode = if serving_ludicrous_active() {
            crate::capacity::power_mode::PowerMode::Performance
        } else if pressure_mode == crate::capacity::power_mode::PowerMode::Eco {
            pressure_mode
        } else {
            everyday_drive_mode()
        };
        // Observability: emit ONLY on a mode TRANSITION so the dynamic scaling is visible
        // without spamming the hot plan tick ([[never-blind-feedback-driven-iteration]]).
        // This is the seam a learned / LLM policy will report through — watch it kick down
        // under load, and later watch a smarter policy make a better call.
        {
            let m = mode as u8;
            let was = LAST_MODE.swap(m, Ordering::Relaxed);
            if was != m {
                eprintln!(
                    "🎛 serving mode → {:?} ({} GiB free)",
                    mode,
                    available / (1 << 30)
                );
                continuum_base::probe!(
                    target: "continuum_core::modules::serving_daemon", class = "serving.mode.transition",
                    to = ?mode,
                    from = was as u64,
                    available_gib = available / (1 << 30),
                    "serving mode moved with memory pressure — the plan's budget fraction follows it"
                );
            }
        }
        let instant = plan_fill(live, mode.serving_fraction());
        // The plan budgets the sustained MINIMUM over one relaunch's duration (READY_TIMEOUT):
        // headroom that has not outlived a relaunch has not earned one; pressure is honoured
        // at once. The M5, 2026-10-18 15:27-15:28Z: the instantaneous budget read 22.9, 29.8,
        // 29.1 then 41.9 GB within 22 s (ledger flat at 41.68 GB) and the plan it fed
        // relaunched the engine at 15:06, 15:18 and 15:24, every resident cold each time.
        let usable_bytes = self
            .budget_readings
            .lock()
            .observe(continuum_base::time::EpochMs::now().millis(), instant)
            .min;
        if usable_bytes != instant {
            continuum_base::probe!(
                target: "continuum_core::modules::serving_daemon", class = "serving.budget.sustained",
                instant_bytes = instant,
                usable_bytes,
                window_ms = crate::inference::llama_server::READY_TIMEOUT.as_millis() as u64,
                "the plan budgets the sustained minimum, not this tick's reading"
            );
        }
        // The sustained minimum cannot exceed this atomic reading after mode
        // reduction. Its historical conservatism cannot spend the charged bytes.
        (HostBudget {
            usable_bytes,
            perf_cores: perf_cores(),
        }, charged)
    }

    /// The serving budget this host has when the box is OURS — physical capacity at
    /// the everyday (Comfort) fraction. Not the live governed number, which sinks
    /// under an outgoing engine, a game, or a training lease: those decide what the
    /// plan can do NOW; this decides what the host may never sink below.
    ///
    /// "Physical" is [`physical_usable_bytes`]'s job — see it for why the capacity
    /// is a function of the PLACEMENT and not of the device that happens to be
    /// present.
    pub(super) fn physical_budget(&self) -> HostBudget {
        // The SAME drive mode the live plan sizes with: a Ludicrous hold (a benchmark's
        // exam) plans at Performance — the whole GPU — and a bound fixed at Comfort
        // would then cap the kept window below what the plan legitimately sized
        // (2026-09-17: 32 GB × 0.80 = 27 GB bound vs a Performance plan). Physical
        // capacity, current mode; never the live free reading.
        let mode = if serving_ludicrous_active() {
            crate::capacity::power_mode::PowerMode::Performance
        } else {
            everyday_drive_mode()
        };
        HostBudget {
            usable_bytes: physical_usable_bytes(
                crate::inference::llama_server::main_lane_placement(),
                self.gpu.total_vram_bytes(),
                self.system.memory().total_bytes,
                mode.serving_fraction(),
            ),
            perf_cores: perf_cores(),
        }
    }

    /// The synchronous fit-gate `serving/pin` holds: given a candidate model,
    /// does it fit a serving lane on THIS host right now? Built from the same
    /// [`live_host_budget`] + [`footprint_for`] + [`plan_serving`] the autonomic
    /// tick uses, so the pin's "will it fit" verdict can never disagree with what
    /// the next reconcile would actually do. The command refuses loud BEFORE
    /// pinning when this says no — never a silent best-fit fallback.
    pub fn pin_fit_checker(&self) -> PinFitChecker {
        let system = self.system.clone();
        let resource_daemon = self.resource_daemon.clone();
        // Read the incumbent so a pin's fit-check models the SWAP it performs, not
        // co-residence (below).
        let serving_tx = self.serving_tx.clone();
        let model_resolver = self.model_resolver.clone();
        Arc::new(move |model: &Model| {
            let base = live_host_budget(&system, &resource_daemon);
            // The incumbent a pin would EVICT — its footprint credits back into the
            // budget (see [`pin_fit_decision`]). `live_host_budget` reads the live
            // system, so the eviction-crediting fit logic is split into a pure,
            // unit-testable helper below.
            let incumbent = serving_tx
                .borrow()
                .active_model
                .clone()
                .and_then(|id| (model_resolver)(&id))
                .and_then(|m| footprint_for(&m));
            pin_fit_decision(base, footprint_for(model), incumbent.as_ref())
        })
    }

    /// The device-budget authority the ephemeral spawn adapter consults before placing
    /// ANOTHER engine on this host.
    ///
    /// This is #56's `ResourceGovernor` for the one decision that was actually going
    /// unmade. That type never existed — its eight mentions in the tree are all doc
    /// comments promising a future owner — so `EphemeralServingLane::spawn` decided by
    /// `first_free_port` alone and three vision sidecars once shared one GPU at
    /// 3.1-8.6 tok/s apiece.
    ///
    /// Built over the SAME `live_host_budget` + `footprint_for` pair `pin_fit_checker`
    /// uses, so the gate and the planner cannot disagree about what fits. It models
    /// CO-RESIDENCE, not a swap: an ephemeral lane joins the live lane rather than
    /// replacing it, so the incumbent's footprint is spent, never credited back — the
    /// one real difference from a pin's arithmetic, and the reason this is a sibling
    /// function instead of a flag on that one.
    pub fn lane_admission_authority(
        &self,
    ) -> crate::inference::llama_server::LaneAdmissionAuthority {
        let system = self.system.clone();
        let resource_daemon = self.resource_daemon.clone();
        let serving_tx = self.serving_tx.clone();
        let model_resolver = self.model_resolver.clone();
        Arc::new(
            move |target: &crate::inference::llama_server::ServingTarget,
                  resident: &[crate::inference::lane_registry::LaneRecord]| {
                use crate::inference::llama_server::LaneAdmission;
                let budget = live_host_budget(&system, &resource_daemon).usable_bytes;
                let Some(model) = (model_resolver)(target.model_id()) else {
                    // We cannot price a model we cannot resolve. Refusing is the honest
                    // answer: admitting would be deciding by absence of information, which
                    // is how the port check behaved and exactly what this replaces.
                    return LaneAdmission::Refuse(format!(
                    "cannot admit an engine for '{}': the model does not resolve in the                      catalog, so its device footprint is unknown",
                    target.model_id()
                ));
                };
                // WEIGHTS as the cost, not weights+KV. The same choice `pin_fit_decision`
                // makes when it credits an incumbent back: weights dominate (measured on
                // this host, 18.97 GB of weights against a 24.63 GB budget) and they are the
                // term nothing can shrink, while KV is exactly what the planner sizes down
                // under pressure. A weights floor therefore under-states the true cost, so
                // this gate errs toward ADMITTING — it is a guard against a second ENGINE,
                // not a precise accountant.
                let Some(want) = footprint_for(&model).map(|f| f.weights_bytes) else {
                    return LaneAdmission::Refuse(format!(
                    "cannot admit an engine for '{}': no on-disk footprint, so its device                      cost is unknown",
                    target.model_id()
                ));
                };
                // Everything already on the device: the live lane it joins, plus every
                // verified-live ephemeral sibling the caller's census found.
                let live_bytes = serving_tx
                    .borrow()
                    .active_model
                    .clone()
                    .and_then(|id| (model_resolver)(&id))
                    .and_then(|m| footprint_for(&m))
                    .map(|f| f.weights_bytes)
                    .unwrap_or(0); // unwrap_or: nothing serving ⇒ the live lane costs nothing, which is the truth here
                let sibling_bytes: u64 = resident
                    .iter()
                    .filter_map(|rec| (model_resolver)(&rec.model))
                    .filter_map(|m| footprint_for(&m))
                    .map(|f| f.weights_bytes)
                    .sum();
                lane_admission_decision(budget, want, live_bytes, sibling_bytes, resident.len())
            },
        )
    }
}
