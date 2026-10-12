//! HOST BUDGET: what this host can give serving, the steady/ludicrous holds over it, and which resource pool serving draws.
//! Split out of serving_daemon.rs (card dbc9711c, BigMama's step 6).

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
    let weights_bytes = candidate.as_ref().map(|f| f.weights_bytes).unwrap_or(0); // unwrap_or: no footprint means the plan below is None (not servable), so this 0 never reaches a fit verdict
    // Fit verdict at ONE lane — "can this model hold a lane at all"; the live plan
    // sizes lanes from demand separately. footprint None → not servable (plan None);
    // footprint Some but over budget → plan_serving degrades with fits_on_gpu=false,
    // which `serving/pin` reads to refuse loud.
    let plan = candidate
        .and_then(|f| plan_serving(&base, std::slice::from_ref(&f), &ServingDemand::new(1, None)));
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
        budget_fraction: crate::config_env::vram_headroom(),
    })
}

/// The serving host budget from the GOVERNED board ALONE — the memory authority's
/// pre-staged snapshot (Vram `available`, netted over every measured consumer + external
/// pressure), never a raw GPU probe. This is the ONE budget an ephemeral eval/train lane
/// sizes against at spawn (MEMORY-AUTHORITY-DAEMON slice 2): reading the board is reading a
/// snapshot the authority maintains on ITS tick, not sampling memory on the spawn hot path.
/// Board-only (no `SystemResourceMonitor` needed), so any consumer holding the
/// `Arc<ResourceDaemon>` sizes a lane through the SAME `plan_serving` the autonomic serving
/// plan uses — one budget, one authority, no duplicate calc.
/// Process-global count of active "hold the live serving lane STEADY" requests. A
/// living-persona benchmark/eval runs ON the shared lane as a co-tenant decode slot —
/// `resources::placement::Placement::ShareLane` (same base already resident ⇒ no second
/// weight copy). The one thing that breaks it is a grow-back RE-HOME relaunch mid-exam:
/// it connection-refuses every in-flight generation (glass-boxed 2026-07-20: hard-rs 0/8,
/// zero output tokens, the lane bounced under the exam). While any hold is active the
/// daemon SKIPS the OPTIONAL grow-back re-home for an already-correctly-served lane. It
/// does NOT suppress a model/genome CHANGE or a pressure-driven shrink — a real resource
/// emergency still preempts (rare, correct). Zero-cost when no eval runs.
pub(super) static SERVING_STEADY_HOLDS: AtomicUsize = AtomicUsize::new(0);

/// RAII: hold the live serving lane steady (suppress the grow-back re-home) until dropped.
/// A living-persona eval binds one for its whole run — the concrete form of `ShareLane`'s
/// "pin the lane for the demand's duration" clause ([[benchmark-is-a-governor-preemption-lease]],
/// the co-tenant/steady case: no second weight copy, just don't bounce the lane).
#[must_use = "the hold releases the instant this guard drops — bind it for the eval's lifetime"]
pub struct ServingSteadyHold {
    /// WHO pinned the lane — a run id, "eval", etc. Carried so the acquire and
    /// release EVENTS name the holder (Joel 2026-08-08: "emit events for
    /// everything — need to know"): a suppressed relaunch with no event is
    /// indistinguishable from a planner that never wanted one, and a leaked
    /// hold with no holder name is unattributable.
    pub(super) holder: String,
}

impl ServingSteadyHold {
    pub fn acquire(holder: impl Into<String>) -> Self {
        let holder = holder.into();
        let holds = SERVING_STEADY_HOLDS.fetch_add(1, Ordering::AcqRel) + 1;
        crate::probe!(
            class = "serving.lane.hold",
            action = "acquired",
            holder = %holder,
            holds,
            "serving lane pinned STEADY (optional grow-back re-home suppressed while held)"
        );
        Self { holder }
    }
}

impl Drop for ServingSteadyHold {
    fn drop(&mut self) {
        let holds = SERVING_STEADY_HOLDS.fetch_sub(1, Ordering::AcqRel) - 1;
        crate::probe!(
            class = "serving.lane.hold",
            action = "released",
            holder = %self.holder,
            holds,
            "serving lane steady-hold released"
        );
    }
}

/// True while at least one caller holds the live lane steady — the reconcile's grow-back
/// re-home is suppressed for an already-correctly-served lane.
pub fn serving_held_steady() -> bool {
    SERVING_STEADY_HOLDS.load(Ordering::Acquire) > 0
}

/// LUDICROUS mode (Joel 2026-07-21: "extreme mode for benchmarks or ludicrous lol"). A
/// benchmark / project / "the fight" wants the biggest window the model+machine can give —
/// not the timid pressure-derived fraction. While any caller holds this, [`host_budget`]
/// forces `PowerMode::Performance` (fraction 0.96 — nearly the whole GPU, less the process's own overhead), OVERRIDING
/// `serving_mode_for_pressure`'s conservative read (which on UMA under-reports free memory
/// and floored a 47872-capable model to 2048 with ~28GB idle). This is the drive mode
/// following the ACTIVITY, not just the pressure: a declared Ludicrous intent → serve
/// maximal. Non-thrashing: one grow-relaunch when the hold is taken, one shrink when it
/// drops — bookends around the exam, not a flap. Zero-cost when nothing holds it.
/// [[serving-mode-follows-activity-ludicrous-to-dream]] [[benchmark-window-must-be-big-not-a-clamped-prompt]]
pub(super) static SERVING_LUDICROUS_HOLDS: AtomicUsize = AtomicUsize::new(0);

/// RAII: while held, serving plans at `PowerMode::Performance` (the whole GPU, biggest
/// window). A benchmark binds one for its whole run so the exam is measured on the largest
/// window the model+machine allow — never a starved boot-window. Drop reverts to the live
/// pressure-adaptive mode.
#[must_use = "the Ludicrous hold releases the instant this guard drops — bind it for the benchmark's lifetime"]
pub struct ServingLudicrousHold(());

impl ServingLudicrousHold {
    pub fn acquire() -> Self {
        SERVING_LUDICROUS_HOLDS.fetch_add(1, Ordering::AcqRel);
        Self(())
    }
}

impl Drop for ServingLudicrousHold {
    fn drop(&mut self) {
        SERVING_LUDICROUS_HOLDS.fetch_sub(1, Ordering::AcqRel);
    }
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
