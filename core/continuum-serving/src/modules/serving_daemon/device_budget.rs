//! Device budget ownership for the serving daemon.

use super::*;

/// Last VRAM ceiling the governed board actually reported this process, and when.
/// The middle rung of the substitute-value ladder: when the board goes quiet, an old
/// REAL number keeps the planner honest where a fabricated zero floors it.
pub(super) static LAST_GOOD_VRAM_CEILING: AtomicU64 = AtomicU64::new(0);
pub(super) static LAST_GOOD_VRAM_CEILING_AT_MS: AtomicU64 = AtomicU64::new(0);

/// The device's PHYSICAL VRAM, seeded once at boot from the GPU monitor. A static
/// hardware fact, known long before any pressure sample — which is precisely why a cold
/// boot never has to guess zero. 0 = no GPU reported a size.
pub(super) static DEVICE_VRAM_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Seed the cold-boot prior. Called once as the serving daemon starts, BEFORE the first
/// plan, so the very first tick already has a real number to stand on.
pub fn seed_device_vram_prior(total_bytes: u64) {
    DEVICE_VRAM_TOTAL.store(total_bytes, Ordering::Relaxed);
    continuum_base::probe!(
        target: "continuum_core::modules::serving_daemon", class = "serving.vram_ceiling_prior.seeded",
        total_bytes = total_bytes,
        gib = format!("{:.2}", total_bytes as f64 / 1024.0 / 1024.0 / 1024.0).as_str(),
        "cold-boot VRAM prior seeded from the device",
    );
}

/// The bytes the plan may fill out of the replace-myself budget `live` under a mode's
/// fraction: the mode's fraction is the one reserve, and it scales with the machine. The
/// budget it is applied to is already the sustained minimum over one relaunch
/// (`budget_readings`), which is what keeps a wandering "available" from flipping the plan.
///
/// A second, flat reserve sat here: the Eco exit edge (12 GiB) subtracted from every unified
/// plan after the 2026-09-19 collapse (a fill with zero slack, the compressor's response read
/// as co-consumer growth), a month before the sustained minimum fixed that flapping at its
/// source. On a 64 GB node it withheld a fifth of the machine: on the M5, 2026-10-29, with
/// ~25 GB available it left the plan 16.5 GB, three 27,904-token lanes for four residents and
/// a median 17 s per act waiting for a slot. The Eco band stays where it belongs, deciding when
/// the MODE changes; it is not a capacity reserve.
pub fn plan_fill(live: u64, fraction: f64) -> u64 {
    (live as f64 * fraction) as u64
}

/// The fraction a production budget may hand the plan: the operator's headroom policy,
/// never above what Performance keeps. Since #4230 the plan withholds nothing itself, so
/// a foundry's `CONTINUUM_VRAM_HEADROOM=1.0` would otherwise size weights + KV + compute
/// to 100% of the governed ceiling with no room for the serving process's own fixed
/// overhead (the CUDA context, the Metal heap) — BigMama's condition on #4230: no
/// production `HostBudget` hands the plan 1.0.
pub fn plan_fraction(headroom: f64) -> f64 {
    headroom.min(crate::capacity::power_mode::PowerMode::Performance.serving_fraction())
}

/// The governed VRAM ceiling RIGHT NOW: the resource authority's `available(Vram)`
/// — capacity (free + ours − reserve, scanned live by the GpuMonitor) minus what
/// is already leased. `None` when VRAM is ungoverned (no live monitor → the kind
/// never appears on the board), which the caller treats as fail-closed (cap at 0,
/// serving refuses rather than over-committing blind). A lock-free `watch`
/// snapshot read, never the governor's accounting lock — safe on the hot tick.
/// Device VRAM physically in use right now, from the board's Vram row (the GPU
/// capacity source's sampled `physical_used`). 0 when the board has no Vram row.
pub(super) fn vram_physical_used(resource_daemon: &ResourceDaemon) -> u64 {
    resource_daemon
        .board()
        .kinds
        .iter()
        .find(|k| k.kind == ResourceKind::Vram)
        .map(|k| k.physical_used_bytes)
        .unwrap_or(0)
}

/// The serving engine's `/props`. Bounded: a localhost read of cached meta, taken only on the
/// sampler's interval; `None` when the lane does not answer in time or answers with an error.
pub(super) async fn engine_props() -> Option<serde_json::Value> {
    let url = format!("{}/props", crate::inference::llama_server::serving_root());
    reqwest::Client::new()
        .get(url)
        .timeout(std::time::Duration::from_millis(500))
        .send()
        .await
        .ok()?
        // a 503 while loading, or a 500, is an engine that did not ANSWER its props, never
        // an engine that predates a field (Codex on #4961): its error body must not be read
        .error_for_status()
        .ok()?
        .json::<serde_json::Value>()
        .await
        .ok()
}

/// What the serving engine holds on the accelerator, by its own account (`/props`): its
/// weights there plus its KV and compute buffers. `None` when the engine does not answer or
/// predates either field.
pub(super) async fn engine_device_bytes() -> Option<u64> {
    let body = engine_props().await?;
    let weights =
        crate::inference::weight_residency::WeightResidency::from_props(&body)?.accelerator_bytes();
    let buffers = crate::inference::weight_residency::EngineMemory::from_props(&body)?
        .accelerator_beyond_weights();
    Some(weights.saturating_add(buffers))
}

/// The discrete footprint arm: what the lane added to the device beyond its weights.
/// `None` without a baseline (an adopted lane: this core never saw the spawn) or a
/// reading — never a delta against zero, which would charge the desktop to the model.
///
/// Also `None` when the delta does not EXCEED the weights. A serving lane holds its KV and
/// compute on the device, so "zero beyond the weights" is not a reading: it means the
/// baseline trough already held the weights (or another engine), and the subtraction
/// saturated. On the 5090 (2026-10-11, card b5230492) that printed `anon_bytes=0
/// per_token_measured=0` every minute for a lane serving 67,584 tokens, and the planner
/// sized lanes from the estimate while the probe read like a measurement. `None` falls
/// through to the engine's own `/props` breakdown, which does not depend on a baseline.
pub(super) fn device_delta_beyond_weights(
    baseline: u64,
    now: u64,
    weights_bytes: u64,
) -> Option<u64> {
    if baseline == 0 || baseline == u64::MAX || now == 0 {
        return None;
    }
    now.checked_sub(baseline)?
        .checked_sub(weights_bytes)
        .filter(|&beyond| beyond > 0)
}

pub(super) fn governed_vram_ceiling(resource_daemon: &ResourceDaemon) -> Option<u64> {
    governed_pool_ceiling(resource_daemon, serving_pool_kind())
}

/// [`governed_vram_ceiling`] for a named pool — the row the lane's placement puts its
/// bytes in (see [`serving_pool_kind`]). Split so the arithmetic is assertable against
/// a Ram row without a config decision in the test.
pub(super) fn governed_pool_ceiling(
    resource_daemon: &ResourceDaemon,
    kind: ResourceKind,
) -> Option<u64> {
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
pub(super) fn governed_vram_ceiling_or_report(
    resource_daemon: &ResourceDaemon,
    site: &'static str,
) -> u64 {
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
            continuum_base::probe!(
                target: "continuum_core::modules::serving_daemon", class = "serving.vram_capacity_absent",
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
        continuum_base::probe!(
            target: "continuum_core::modules::serving_daemon", class = "serving.vram_ceiling_prior",
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

impl ServingDaemonModule {
    /// Once a minute, read the live lane's anonymous footprint off its process and feed
    /// the per-token cost back to the plan (`inference::lane_footprint`). One probe per
    /// sample carries predicted vs measured so the plan's arithmetic is checked against
    /// the process it launched, every minute, on every box.
    /// While no lane is ready, the device's sampled used-bytes are the trough the
    /// discrete footprint arm subtracts: take the minimum seen since the spawn armed it.
    pub(super) fn lower_spawn_baseline_to_the_trough(&self) {
        if self.serving_tx.borrow().ready {
            return;
        }
        let now = vram_physical_used(&self.resource_daemon);
        if now == 0 {
            return;
        }
        self.spawn_baseline_vram.fetch_min(now, Ordering::Relaxed);
    }

    /// THE QUESTION (SHARED-RESIDENT-LIFECYCLE.md step 1): may serving replace or measure the
    /// live engine right now? Live work bound to this very engine incarnation (a training run
    /// admitted onto it) holds it; an unreadable record holds it too, because ownership that
    /// cannot be read is not absence. No live engine is nothing to disturb.
    pub(super) fn engine_occupancy(&self) -> crate::inference::engine_residency::Occupancy {
        use crate::inference::engine_residency::{occupancy, store_path, Occupancy};
        let Some(rec) = (self.inherited_lane)() else {
            return Occupancy::Free;
        };
        match crate::paths::continuum_home() {
            Ok(home) => occupancy(&store_path(&home), rec.port),
            Err(e) => {
                Occupancy::Unknown(format!("no home to read the engine's residency from: {e}"))
            }
        }
    }

    /// Record what the serving engine holds on a discrete GPU, for the board's credit to
    /// serving (`EngineDeviceCredit`, card 741f5eec). Off a discrete GPU the host-process read
    /// is the right quantity, so the reading says so. No live lane, or an unreadable `/props`,
    /// records no credit: the gap between engines is 0, never a guess.
    pub(super) async fn refresh_engine_device_credit(&self) {
        let discrete_gpu = matches!(
            (
                crate::inference::llama_server::main_lane_placement(),
                self.system.gpu_memory_mode()
            ),
            (
                crate::inference::llama_server::LanePlacement::Gpu,
                Some(crate::gpu::monitor::MemoryMode::Discrete)
            )
        );
        if !discrete_gpu {
            self.engine_device_credit.record_not_discrete();
            return;
        }
        let pid = crate::inference::lane_pidfile::read();
        let bytes = match pid {
            Some(_) => engine_device_bytes().await,
            None => None,
        };
        if !self.engine_device_credit.record_discrete(pid, bytes) {
            return; // a steady engine: same reading, no probe (it allocates its buffers at launch)
        }
        continuum_base::probe!(
            target: "continuum_core::modules::serving_daemon", class = "serving.credit.engine_device",
            pid = pid.map_or(0, u64::from),
            device_bytes = bytes.unwrap_or(0), // unwrap_or: probe label; `read` says whether a reading exists
            read = bytes.is_some(),
            "serving's board credit on a discrete GPU changed: what its engine reports holding on the device"
        );
    }
}
