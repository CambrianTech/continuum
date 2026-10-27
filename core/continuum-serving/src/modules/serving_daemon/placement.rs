//! Placement ownership for the serving daemon.

use super::*;

/// VRAM headroom held below the governed serving budget when fitting expert LAYERS for K3
/// placement. `layer_bytes` sizes the stacked expert blob, but the `-ot`'d layer also carries
/// its router + norms and the per-layer size can drift; this margin keeps the fit honest. 512 MiB.
pub(super) const EXPERT_PLACEMENT_MARGIN_BYTES: u64 = 512 * 1024 * 1024;
/// Hot-layer churn (symmetric difference vs the served set) that justifies a llama-server
/// RESPAWN. A respawn reloads every weight (seconds), so a 1–2 layer flap must not trigger one;
/// only a real residency shift (a task change moving > 2 expert layers) does.
pub(super) const RELAUNCH_LAYER_CHURN_THRESHOLD: usize = 2;

/// What remains of physical RAM for a SIDECAR after the main lane's planned working
/// set — the same terms the host-cache lease law uses ([`moe_host_cache_lease_inputs`]),
/// with the FULL weights (on unified memory the device-resident share is host RAM):
/// `physical − weights − KV(served window) × lanes − compute reserve − OS floor`.
/// Saturating: an over-full box yields ZERO headroom, never a wrap. Pure over the
/// target and the served window, so a relaunch window cannot lie to it.
/// The main-lane width a sidecar's headroom is planned against: the served width or the
/// residents' requirement, whichever is larger. PURE.
pub fn sidecar_window_for_headroom(served_window: u32, persona_floor: Option<u32>) -> u32 {
    served_window.max(persona_floor.unwrap_or(0)) // unwrap_or: no requirement measured = the served width stands
}

pub(super) fn sidecar_planned_headroom_bytes(
    target: &ServingTarget,
    served_window: u32,
    physical_bytes: u64,
) -> u64 {
    let Some(fp) = footprint_for(&target.model) else {
        return 0; // no honest footprint = no honest headroom; the live read alone never admits
    };
    let lanes = target.lanes.max(1);
    let window = if served_window > 0 {
        served_window
    } else {
        target.context_window
    };
    let working_set = fp
        .weights_bytes
        .saturating_add(fp.kv_at(window).saturating_mul(lanes as u64))
        .saturating_add(fp.prefill_compute_reserve(window, lanes))
        .saturating_add((target.host_prompt_cache_mib as u64) * 1024 * 1024)
        .saturating_add(host_os_floor_bytes(physical_bytes));
    physical_bytes.saturating_sub(working_set)
}

/// ONE FLOOR EVERYWHERE: the bytes a sidecar may take is the tighter of the LIVE free
/// read minus the OS floor and the PLANNED headroom (which already keeps that floor).
/// The live read compared RAW admitted a 24.1 GB sidecar into 26.7 GB "free" on the M5
/// (2026-09-18 21:56:03Z, the 1800bd90b deploy): `available_bytes` counts purgeable and
/// inactive pages as free, and 2.6 GB of slack on a 64 GB box is the core's own RSS —
/// swap 6.7 GB, compressor 20 GB, the serving node paging. The planned term had kept
/// the floor (physical/8) all along; the live term did not. Pure, so the measured
/// numbers are a test.
pub(super) fn sidecar_admissible_bytes(
    live_free: u64,
    planned_headroom: u64,
    physical_bytes: u64,
) -> u64 {
    live_free
        .saturating_sub(host_os_floor_bytes(physical_bytes))
        .min(planned_headroom)
}

impl ServingDaemonModule {
    /// Bring the running `llama-server` in line with the published plan. FAST —
    /// it only decides whether a reconcile is needed and, if so, spawns a
    /// detached task to do the (possibly multi-second) relaunch + readiness
    /// wait. The tick must never block on model load, so the slow part runs off
    /// the tick. Returns the spawned `JoinHandle` (for tests to await
    /// deterministically) or `None` when no reconcile was started.
    ///
    /// K3 expert-layer placement for the model this reconcile will serve.
    ///
    /// For a MoE model it (re)builds the per-model [`MoeServingContext`](crate::capacity::moe_serving::MoeServingContext)
    /// on a model change (rare — a swap already relaunches), ticks its pager against the
    /// governed VRAM budget to decide which expert LAYERS fit, and returns the DEBOUNCED
    /// placement: `committed_placement` only changes when the hot-layer churn passes
    /// [`RELAUNCH_LAYER_CHURN_THRESHOLD`], so the serving target stays byte-stable across ticks
    /// that don't warrant a respawn (and the launcher's target-diff doesn't fire spuriously).
    ///
    /// `None` — served with no `-ot` override, exactly as before — for a dense model, an
    /// unresolved GGUF, or a zero budget. Pure of I/O except the one GGUF read on a model
    /// change; the sync `Mutex` is never held across an await (this whole method is sync).
    pub(super) fn compute_expert_placement(&self, model: &Model) -> Option<PlacementRequest> {
        // MEASUREMENT knob (off by default): a forced budget OVERRIDES the governed ceiling so
        // a fits-in-VRAM model spills expert layers to CPU, making the B-gate measurable. Every
        // placement made under it is flagged on the probe below — never mistaken for capacity.
        let forced = self.measure_force_expert_budget_bytes;
        let budget = forced.unwrap_or_else(|| {
            governed_vram_ceiling_or_report(&self.resource_daemon, "compute_expert_placement")
        });
        if budget == 0 {
            return None;
        }
        let mut guard = self.moe_serving.lock().ok()?;
        let stale = guard.as_ref().map(|(id, _)| id.as_str()) != Some(model.id.as_str());
        if stale {
            // The verdict is cached per model, dense included (it was re-read every tick); an
            // unresolved GGUF is not, so it is read once it lands (card 60f74ac5).
            *guard = crate::model_registry::artifacts::resolve_gguf_for_model(model).map(|gguf| {
                let built = crate::capacity::moe_serving::moe_serving_context(&gguf, &model.id, EXPERT_PLACEMENT_MARGIN_BYTES, RELAUNCH_LAYER_CHURN_THRESHOLD);
                (model.id.clone(), built.unwrap_or_else(|why| { continuum_base::probe!(target: "continuum_core::modules::serving_daemon", class = "serving.moe.geometry_unreadable", model = model.id.as_str(), why = %why, "MoE geometry unreadable: served dense, no expert placement"); None })) // unwrap_or_else: probed here; dense is the one placement that needs no geometry
            });
        }
        let (_, Some(ctx)) = guard.as_mut()? else { return None };
        let outcome = ctx
            .pager
            .tick_layer_placement(budget, ctx.n_experts_per_layer, ctx.n_layers);
        // Glass-box the K3 residency decision (Joel: the K3 path is observability-first).
        // Every load-bearing quantity a breakpoint would want: what fit, out of how many, on
        // what budget, and whether it moves the served process this tick.
        continuum_base::probe!(
            target: "continuum_core::modules::serving_daemon", class = "serving.k3_placement",
            model = model.id.as_str(),
            budget_bytes = budget,
            // TRUE when the budget is the measurement override, not the governed ceiling — so a
            // reader knows the hot/cold split is under an artificial constraint (the B-gate sweep).
            measurement_forced = forced.is_some(),
            n_layers = ctx.n_layers,
            n_experts_per_layer = ctx.n_experts_per_layer,
            hot_layers = outcome.request.hot_layers.len(),
            needs_relaunch = outcome.needs_relaunch,
            "K3 expert-layer placement: fit {} of {} layers hot on {} GiB budget{}",
            outcome.request.hot_layers.len(),
            ctx.n_layers,
            budget / (1024 * 1024 * 1024),
            if forced.is_some() {
                " [MEASUREMENT-FORCED]"
            } else {
                ""
            },
        );
        if outcome.needs_relaunch {
            ctx.pager.mark_layer_relaunched(&outcome.request.hot_layers);
            ctx.committed_placement = Some(outcome.request);
        }
        ctx.committed_placement.clone()
    }

    /// Device-fit resident-override (#29): does this model's RESIDENT (non-expert)
    /// tier fit the governed VRAM budget as-shipped, or must the launcher source a
    /// precision-shrunk resident from a device-fit override GGUF? The resident-FIT
    /// decision turns ONLY on `resident_bytes` vs the budget minus a fixed compute
    /// reserve — per-layer KV (M5's #2107 `ModelCapabilities`) drives the CONTEXT +
    /// expert-VRAM split, NOT this, so KV is deliberately not consulted here. `None`
    /// = resident fits as-shipped (dense / small MoE, served normally) OR no fitting
    /// override is cached yet (a >VRAM-resident MoE like K3 until the resolver #35
    /// discovers/generates one — then the launch OOMs LOUD rather than silently
    /// mis-serving, [[no-masking-fallbacks-my-style-tell]]).
    pub(super) fn compute_resident_override(&self, model: &Model) -> Option<std::path::PathBuf> {
        let budget = governed_vram_ceiling(&self.resource_daemon)?;
        if budget == 0 {
            return None;
        }
        // Expert-tensor total — present only for a MoE serve (a dense model's
        // resident always places on GPU normally, no device-fit split). Read under
        // the same never-across-await sync Mutex as the placement path.
        let expert_bytes_total = {
            let guard = self.moe_serving.lock().ok()?;
            match guard.as_ref() {
                Some((id, Some(ctx))) if id.as_str() == model.id.as_str() => ctx.expert_bytes_total,
                _ => return None,
            }
        };
        let fp = footprint_for(model)?;
        let resident_bytes = fp.weights_bytes.saturating_sub(expert_bytes_total);
        let model_id = model.id.clone();
        let inputs = crate::capacity::device_fit::DeviceFitInputs {
            resident_bytes,
            vram_budget_bytes: budget,
            kv_bytes_per_token: 0, // not consulted for the resident-fit decision — see doc
            compute_reserve_bytes: crate::capacity::device_fit::default_compute_reserve_bytes(
                budget,
            ),
            desired_context: model.context_window,
            model_max_context: model.context_window,
            lanes: 1,
        };
        let plan = crate::capacity::device_fit::plan_device_fit(&inputs, |usable| {
            crate::model_registry::artifacts::resolve_device_fit_override(&model_id, usable)
        });
        continuum_base::probe!(
            target: "continuum_core::modules::serving_daemon", class = "serving.device_fit",
            model = model.id.as_str(),
            resident_bytes,
            budget_bytes = budget,
            gpu_servable = plan.is_gpu_servable(),
            has_override = plan.resident.override_path().is_some(),
            "device-fit resident tier: {}",
            match &plan.resident {
                crate::capacity::device_fit::ResidentFit::Native =>
                    "fits as-shipped (native, all resident on GPU)".to_string(),
                crate::capacity::device_fit::ResidentFit::Override(o) => format!(
                    "device-fit override {} GiB resident from {}",
                    o.resident_bytes / (1024 * 1024 * 1024),
                    o.path.display()
                ),
                crate::capacity::device_fit::ResidentFit::Unfittable {
                    resident_bytes,
                    usable_bytes,
                } => format!(
                    "UNFITTABLE: resident {} GiB > {} GiB usable — route to grid or generate an override (#35)",
                    resident_bytes / (1024 * 1024 * 1024),
                    usable_bytes / (1024 * 1024 * 1024)
                ),
            },
        );
        plan.resident.override_path().cloned()
    }
}
