//! The serving budget: the governed ceiling from the memory authority's board (with
//! the substitute-value ladder when it is silent), the live and physical host budgets,
//! the pool a placement's bytes live in, the drive mode that scales the fill, the
//! unified-memory Eco band, the perf-core proxy and the OS floor.

use super::*;

/// The live host readings a serving budget is derived from. A NAMED struct, not
/// three positional args, so the two byte counts (`available_bytes` and
/// `total_vram_bytes` are both `u64`) can never be silently transposed at a call
/// site — the compiler cannot catch `host_budget_from(total, available, ..)` on
/// positionals, but `HostBudgetInputs { available_bytes, total_vram_bytes, .. }`
/// names each ([[structs-by-reference-not-massive-param-lists]]). A new input
/// (e.g. a device-tier hint) becomes ONE added field with a default, not a fourth
/// positional every caller must thread.
#[derive(Debug, Clone, Copy)]
pub struct HostBudgetInputs {
    /// Monitor's current free memory (already net of everything else running);
    /// we never plan above what's free.
    pub available_bytes: u64,
    /// Physical VRAM ceiling — we never plan above what the device has.
    pub total_vram_bytes: u64,
    /// Performance-core proxy for the lane cap (floored at 1 inside).
    pub perf_cores: u32,
    /// Fraction of the board this host treats as ours to serve from (`usable =
    /// live × this`). The operator VRAM-headroom policy: default 0.80 (leave 20%
    /// for the OS + Bevy + embeddings), a dedicated foundry sets 1.0. Live callers
    /// pass `config_env::vram_headroom()`; tests pass an explicit fraction so
    /// `host_budget_from` stays pure + environment-independent.
    pub budget_fraction: f64,
}

/// Serving budget from LIVE free memory, capped at physical VRAM, minus headroom.
/// Pure for tests — the headroom fraction is an INPUT (`budget_fraction`), never
/// read from config here, so a test can't go environment-dependent. Takes
/// [`HostBudgetInputs`] by reference so the byte-count fields are named at every
/// call site (never transposable).
pub fn host_budget_from(inputs: &HostBudgetInputs) -> HostBudget {
    let live = inputs.available_bytes.min(inputs.total_vram_bytes);
    let usable = (live as f64 * inputs.budget_fraction) as u64;
    HostBudget {
        usable_bytes: usable,
        perf_cores: inputs.perf_cores.max(1),
    }
}

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
        budget_fraction: crate::config_env::vram_headroom(),
    })
}

/// True while at least one caller demands Ludicrous (Performance) serving — [`host_budget`]
/// pins `PowerMode::Performance` regardless of the pressure read.
/// Last VRAM ceiling the governed board actually reported this process, and when.
/// The middle rung of the substitute-value ladder: when the board goes quiet, an old
/// REAL number keeps the planner honest where a fabricated zero floors it.
/// The pool the main lane's bytes live in — the ONE decision every serving site
/// keyed by a resource kind reads through: the plan's ceiling, the replace-myself
/// credit, the credited-once check and the consumer's own attribution. A CPU-placed
/// lane (`--n-gpu-layers 0`, #3729's Intel-Mac arm) puts weights AND KV in system
/// RAM, so all four belong to the `Ram` row, not the `Vram` row of a card holding
/// none of them. #4169 made the physical bound follow the placement; this makes the
/// PLAN follow it. Card 7c72c0f0 (Intel Mac, 2026-09-17): nine citizens sized against
/// a 5.7 GB Vram row on a box with 34.4 GB of RAM and 0 MB of VRAM in use — one warm
/// slot, eight exiled to a remote lane that timed out at 600 s.
pub fn serving_pool_kind() -> ResourceKind {
    pool_kind_for(crate::inference::llama_server::main_lane_placement())
}

/// Pure so the mapping is assertable without config: where the placement puts the
/// bytes is which row governs them.
pub fn pool_kind_for(placement: crate::inference::llama_server::LanePlacement) -> ResourceKind {
    use crate::inference::llama_server::LanePlacement;
    match placement {
        LanePlacement::Cpu => ResourceKind::Ram,
        LanePlacement::Gpu => ResourceKind::Vram,
    }
}

static LAST_GOOD_VRAM_CEILING: AtomicU64 = AtomicU64::new(0);
static LAST_GOOD_VRAM_CEILING_AT_MS: AtomicU64 = AtomicU64::new(0);

/// The device's PHYSICAL VRAM, seeded once at boot from the GPU monitor. A static
/// hardware fact, known long before any pressure sample — which is precisely why a cold
/// boot never has to guess zero. 0 = no GPU reported a size.
static DEVICE_VRAM_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Seed the cold-boot prior. Called once as the serving daemon starts, BEFORE the first
/// plan, so the very first tick already has a real number to stand on.
pub fn seed_device_vram_prior(total_bytes: u64) {
    DEVICE_VRAM_TOTAL.store(total_bytes, Ordering::Relaxed);
    crate::probe!(
        class = "serving.vram_ceiling_prior.seeded",
        total_bytes = total_bytes,
        gib = format!("{:.2}", total_bytes as f64 / 1024.0 / 1024.0 / 1024.0).as_str(),
        "cold-boot VRAM prior seeded from the device",
    );
}

pub fn governed_host_budget(resource_daemon: &ResourceDaemon) -> HostBudget {
    let available = governed_vram_ceiling_or_report(resource_daemon, "governed_host_budget");
    host_budget_from(&HostBudgetInputs {
        available_bytes: available,
        total_vram_bytes: available,
        perf_cores: perf_cores(),
        budget_fraction: plan_fraction(crate::config_env::vram_headroom()),
    })
}

/// The bytes the plan may fill out of the replace-myself budget `live` under a mode's
/// fraction. On UNIFIED memory the serving pool and the pressure monitor measure the
/// same bytes, so a fill to the mode's fraction can leave the box inside the Eco band:
/// measured 2026-09-19 17:56–18:20Z on the M5 (64 GB, Comfort, budget 40 GB) — 2 × 67k
/// (27 GB) became 3 × 67k (32 GB) the tick the fit allowed it, with zero slack; the
/// compressor took 6 GB of other processes' pages, "external" read 15 → 23 GB, the budget
/// fell 40 → 33, Eco entered, the plan collapsed to one lane at the floor and admission
/// followed it: five minds on one of three slots for the hour. Comfort's 20% of a 40 GB
/// budget is 8 GB — UNDER the 12 GiB Eco exit line — so a full Comfort fill is already
/// inside the band. The plan therefore never fills within [`ECO_EXIT_BYTES`] of the
/// budget on unified memory: the band the mode machinery already defines, one place,
/// where #4230's removed 15% used to sit by accident. A box small enough that Eco's own
/// fraction is the tighter bound keeps the mode alone (the band is absolute; the mode
/// scales). Discrete VRAM is a different pool from host memory: the band does not apply.
pub fn plan_fill(live: u64, fraction: f64, unified: bool) -> u64 {
    use crate::provisioning::model_catalog::{PowerMode, ECO_EXIT_BYTES};
    let by_mode = (live as f64 * fraction) as u64;
    if !unified {
        return by_mode;
    }
    let eco_floor = (live as f64 * PowerMode::Eco.serving_fraction()) as u64;
    let above_band = live.saturating_sub(ECO_EXIT_BYTES);
    if above_band > eco_floor {
        by_mode.min(above_band)
    } else {
        by_mode
    }
}

/// The fraction a production budget may hand the plan: the operator's headroom policy,
/// never above what Performance keeps. Since #4230 the plan withholds nothing itself, so
/// a foundry's `CONTINUUM_VRAM_HEADROOM=1.0` would otherwise size weights + KV + compute
/// to 100% of the governed ceiling with no room for the serving process's own fixed
/// overhead (the CUDA context, the Metal heap) — BigMama's condition on #4230: no
/// production `HostBudget` hands the plan 1.0.
pub fn plan_fraction(headroom: f64) -> f64 {
    headroom.min(crate::provisioning::model_catalog::PowerMode::Performance.serving_fraction())
}

pub(super) fn governed_vram_ceiling(resource_daemon: &ResourceDaemon) -> Option<u64> {
    governed_pool_ceiling(resource_daemon, serving_pool_kind())
}

/// [`governed_vram_ceiling`] for a named pool — the row the lane's placement puts its
/// bytes in (see [`serving_pool_kind`]). Split so the arithmetic is assertable against
/// a Ram row without a config decision in the test.
pub(super) fn governed_pool_ceiling(resource_daemon: &ResourceDaemon, kind: ResourceKind) -> Option<u64> {
    // Serving budgets from ITS OWN view of the board — global available minus
    // every OTHER consumer's unmet reservation floor (`available_for`, the same
    // math `acquire` enforces) — never the reservation-blind global number.
    // #225 (Joel 2026-08-08: "the budgeter just has all its parts figure it
    // out"): budgeting from the global figure let the serving window grow over
    // the embed lane's 1792 MiB floor, leaving 604 MiB governed-available for a
    // faculty cognition needs every turn — embedding fully dead while serving
    // sat comfortable. The board row's existence still gates None ("governor
    // hasn't reported" stays distinct from "zero bytes free").
    resource_daemon
        .board()
        .kinds
        .iter()
        .find(|k| k.kind == kind)
        // THE LAST FABRICATION POINT IN THE CHAIN (#438). A row can EXIST while its
        // capacity reads 0 — `ledger.rs`'s `capacity.get(&kind).unwrap_or(0)` invents
        // that zero for a kind whose capacity source has not reported yet, which at boot
        // is simply true for a second. The ceiling ladder then classified it `Measured(0)`
        // and passed it straight through, correctly by its own rules, because a zero that
        // arrives as a number is indistinguishable from one that was observed.
        //
        // A governed VRAM kind with ZERO CAPACITY is never a true fact about a machine
        // that reports a GPU: capacity is the device's size, not its free space, and a
        // device does not shrink to nothing. So a zero-capacity row is treated as NO
        // READING — the ladder falls to last-known, then to the device prior — while a
        // zero AVAILABLE on a real capacity stays a genuine measurement (a full GPU).
        .filter(|k| k.capacity_bytes > 0)
        // budget_for_replacing, NOT available_for. The serving planner's whole job is to
        // decide what should be resident NEXT, and the swap releases what is resident now
        // — so counting serving's own weights against serving's own plan is circular. It
        // was measured doing exactly that on 2026-08-19: 30.25 GB resident subtracted from
        // a 55.1 GB ceiling handed the planner 16.9 GB, a 27B does not fit in 16.9 GB, so
        // it abandoned the 27B it was ALREADY RUNNING for a 7B, then a 0.5B. `acquire`
        // still uses available_for — a lease must never be granted against bytes not yet
        // released; only the replace-myself DECISION gets the add-back.
        .map(|_| resource_daemon.budget_for_replacing(SERVING_CONSUMER_ID, kind))
}

/// The governed VRAM ceiling for a planner that has no way to represent "unknown",
/// with the absence RECORDED rather than swallowed.
///
/// `governed_vram_ceiling` returns `None` for exactly one reason: the board carries
/// no `Vram` row — the governor has not reported yet (boot race) or this node tracks
/// no VRAM at all. That is **not** the same fact as "zero bytes free", and the four
/// call sites that used to write `.unwrap_or(0)` could not tell them apart. A
/// silently-substituted 0 makes a 64GB machine look like it has no GPU, which floors
/// the served window and refuses lanes — indistinguishable from, and byte-identical
/// to, the #213/#214 symptom this file's own comments were written to explain.
///
/// 0 remains the right CONSERVATIVE value to plan with (never over-commit VRAM we
/// cannot confirm). What was wrong was that the substitution left no trace. Now
/// every unknown ceiling names its call site on the wire, so a machine that plans
/// like it has no GPU can be told apart from one that actually has none — in one
/// grep, instead of a night.
///
/// Joel, 2026-08-05: "this philosophy of 'errors are bad' has ruined most of my
/// projects" — a fallback value is only legitimate when the fallback is VISIBLE.
pub(super) fn governed_vram_ceiling_or_report(resource_daemon: &ResourceDaemon, site: &'static str) -> u64 {
    // THE SUBSTITUTE-VALUE LADDER (#438). This used to be `.unwrap_or(0)`, and that zero
    // — which no monitor ever measured — is what handed the planner `usable_gb = 0` at
    // boot and got a 0.5B spawned on a 64 GB machine. See
    // [`crate::resources::ceiling_prior`] for why 0 is safe for a GRANTER and
    // catastrophic for a PLANNER, and why every rung below beats it.
    // DISCRIMINATOR (#438): if the pool's row exists but carries zero capacity, say so
    // with the numbers, so the next boot PROVES the mechanism instead of leaving it
    // inferred.
    let kind = serving_pool_kind();
    if let Some(row) = resource_daemon
        .board()
        .kinds
        .iter()
        .find(|k| k.kind == kind)
    {
        if row.capacity_bytes == 0 {
            crate::probe!(
                class = "serving.vram_capacity_absent",
                site = site,
                kind = kind.label(),
                capacity_bytes = row.capacity_bytes,
                physical_used_bytes = row.physical_used_bytes,
                available_bytes = row.available_bytes,
                "a Vram row exists with ZERO CAPACITY — the capacity source has not \
                 reported. Falling to the prior ladder; this is NOT evidence of a GPU \
                 that shrank to nothing.",
            );
        }
    }

    let board = governed_vram_ceiling(resource_daemon);

    // Record every real reading so the NEXT silent tick has something honest to stand on.
    if let Some(bytes) = board {
        LAST_GOOD_VRAM_CEILING.store(bytes, Ordering::Relaxed);
        LAST_GOOD_VRAM_CEILING_AT_MS.store(EpochMs::now().millis(), Ordering::Relaxed);
    }

    let last_at = LAST_GOOD_VRAM_CEILING_AT_MS.load(Ordering::Relaxed);
    let reading = crate::resources::ceiling_prior::decide(crate::resources::CeilingEvidence {
        board_bytes: board,
        last_good: (last_at > 0).then(|| {
            (
                LAST_GOOD_VRAM_CEILING.load(Ordering::Relaxed),
                EpochMs::now().millis().saturating_sub(last_at),
            )
        }),
        // The device prior is a VRAM fact. The Ram row is never cold — its ceiling is
        // read once at construction (`HostRamCapacitySource`), so a CPU-placed lane has
        // a measured row from the first tick and no prior to fall to.
        device_total_bytes: match (kind, DEVICE_VRAM_TOTAL.load(Ordering::Relaxed)) {
            (ResourceKind::Vram, n) if n > 0 => Some(n),
            _ => None,
        },
    });

    // Only NON-measured rungs are worth a probe row: a healthy board reading every tick
    // is the steady state and would drown the stream (#399). A prior in use is exactly
    // the condition an operator needs to see, so it always speaks.
    if reading.provenance != crate::resources::Provenance::Measured {
        crate::probe!(
            class = "serving.vram_ceiling_prior",
            site = site,
            bytes = reading.bytes,
            gib = format!("{:.2}", reading.bytes as f64 / 1024.0 / 1024.0 / 1024.0).as_str(),
            provenance = format!("{:?}", reading.provenance).as_str(),
            why = reading.note,
            "planning against a SUBSTITUTE ceiling, not a measurement",
        );
    }

    // A NAMED BRANCH, NOT `unwrap_or(0)`. The value is the same; the difference is that
    // the zero is now a decision someone wrote down and a reviewer can argue with,
    // instead of a default that reads like punctuation. `unwrap_or` states no reason and
    // leaves no place to put one — which is exactly how the original zero survived
    // review at six call sites.
    match reading.usable_bytes() {
        Some(bytes) => bytes,
        // No board, no history, no device. Planning against 0 makes the planner decline,
        // and declining is the honest outcome when nothing on this machine can say how
        // much VRAM exists. Unreachable on any host with a GPU — it is the ladder's last
        // rung, not (as before) the first thing a cold boot hit.
        None => 0,
    }
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

/// The everyday drive mode this node's operator asked for, from
/// `CONTINUUM_VRAM_HEADROOM` (default 0.8 → Comfort; a dedicated node at 1.0 →
/// Performance). One reader, cached by `config_env`.
fn everyday_drive_mode() -> crate::provisioning::model_catalog::PowerMode {
    crate::provisioning::model_catalog::PowerMode::everyday_for_headroom(
        crate::config_env::vram_headroom(),
    )
}

pub(super) fn host_os_floor_bytes(physical_bytes: u64) -> u64 {
    (physical_bytes / 8).max(2 * 1024 * 1024 * 1024)
}

impl ServingDaemonModule {
    /// Honest serving budget for this host, RIGHT NOW — from the live free
    /// memory the monitor reports, capped at the device's physical VRAM. This
    /// is the organic signal: free memory drops under load, the budget shrinks,
    /// the plan picks fewer lanes / a smaller model; load clears, it flows back.
    /// Delegates to the free [`live_host_budget`] so the autonomic tick and the
    /// `serving/pin` fit-gate compute the budget from the ONE source.
    pub(super) fn host_budget(&self) -> HostBudget {
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
        let unified = matches!(
            self.system.gpu_memory_mode(),
            Some(crate::gpu::monitor::MemoryMode::Unified)
        );
        let live = governed_vram_ceiling_or_report(&self.resource_daemon, "host_budget");
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
            Some(crate::provisioning::model_catalog::PowerMode::Eco)
        } else {
            Some(crate::provisioning::model_catalog::PowerMode::Comfort)
        };
        let pressure_mode = crate::provisioning::serving_mode_for_pressure_from(available, previous);
        LAST_PRESSURE_MODE_ECO.store(
            pressure_mode == crate::provisioning::model_catalog::PowerMode::Eco,
            Ordering::Relaxed,
        );
        // Ludicrous (an exam) floors it; pressure (Eco) drops it; otherwise the
        // operator's headroom policy names the everyday mode — the SAME knob the pin
        // fit-gate budgets with, composed here as a mode rather than stacked under one.
        let mode = if serving_ludicrous_active() {
            crate::provisioning::model_catalog::PowerMode::Performance
        } else if pressure_mode == crate::provisioning::model_catalog::PowerMode::Eco {
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
                crate::probe!(
                    class = "serving.mode.transition",
                    to = ?mode,
                    from = was as u64,
                    available_gib = available / (1 << 30),
                    "serving mode moved with memory pressure — the plan's budget fraction follows it"
                );
            }
        }
        HostBudget {
            usable_bytes: plan_fill(live, mode.serving_fraction(), unified),
            perf_cores: perf_cores(),
        }
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
            crate::provisioning::model_catalog::PowerMode::Performance
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
}
