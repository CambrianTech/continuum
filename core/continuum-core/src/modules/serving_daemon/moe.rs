//! MoE serving: K3 expert-layer placement against the governed VRAM budget, the
//! device-fit resident override, the governed host-cache lease published to the
//! per-port plan file (with the pin actuator and the division actuator), and the lease
//! inputs assembled from quantities the planner already budgets.

use super::*;

/// VRAM headroom held below the governed serving budget when fitting expert LAYERS for K3
/// placement. `layer_bytes` sizes the stacked expert blob, but the `-ot`'d layer also carries
/// its router + norms and the per-layer size can drift; this margin keeps the fit honest. 512 MiB.
const EXPERT_PLACEMENT_MARGIN_BYTES: u64 = 512 * 1024 * 1024;
/// Hot-layer churn (symmetric difference vs the served set) that justifies a llama-server
/// RESPAWN. A respawn reloads every weight (seconds), so a 1–2 layer flap must not trigger one;
/// only a real residency shift (a task change moving > 2 expert layers) does.
const RELAUNCH_LAYER_CHURN_THRESHOLD: usize = 2;
/// Sticky-lease hysteresis band for the host-cache budget: 1/8 of the published
/// value (move only on >12.5% change). Same band shape as the plan hysteresis —
/// KV-sample flutter must not churn the plan file's mtime under her per-token poll.
pub(super) const HOST_CACHE_LEASE_BAND_DIVISOR: u64 = 8;
/// Recency-window tokens carried on the published plan document. Inert while
/// `pin_list` is empty (v1 publishes budget-only); matches the golden v1 wire
/// fixture so the C++ consumer's parse sees the shape it was validated against.
// context-budget-exempt: the EXPERT PAGER's recency window (thousands of tokens of routing history for the pin list), part of the published plan wire format the C++ consumer validates against — not the prompt
const MOE_PLAN_WINDOW_K: u32 = 8;

/// Windows only: the system-wide commit charge (`GetPerformanceInfo`), the number
/// the pagefile-overcommit clamp binds against — a Windows box under load can have
/// commit far above what a "free RAM" read suggests, and pagefile overcommit
/// thrashes SILENTLY (the 2026-08-01 incident: 95.9GB commit on a 63GB box, fetch
/// collapsed 2.5GB/s→205MB/s with no OOM anywhere). `None` on read failure — the
/// lease then falls back to the headroom bound alone, which is honest but looser.
#[cfg(windows)]
fn system_commit_charge_bytes() -> Option<u64> {
    use windows_sys::Win32::System::ProcessStatus::{GetPerformanceInfo, PERFORMANCE_INFORMATION};
    // SAFETY: PERFORMANCE_INFORMATION is a plain C struct; zeroed is a valid
    // initial state, and GetPerformanceInfo only writes within `cb` bytes.
    let mut info: PERFORMANCE_INFORMATION = unsafe { std::mem::zeroed() };
    info.cb = std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32;
    let ok = unsafe { GetPerformanceInfo(&mut info, info.cb) };
    (ok != 0).then(|| (info.CommitTotal as u64).saturating_mul(info.PageSize as u64))
}

/// macOS/Linux: commit is not the binding regime (no silent pagefile ballooning of
/// this shape) — the lease's headroom arithmetic is the bound. See the law's doc.
#[cfg(not(windows))]
fn system_commit_charge_bytes() -> Option<u64> {
    None
}

/// Assemble the host-cache lease inputs for a MoE serve from quantities the planner
/// ALREADY budgets — every term justified, none invented, none read from a
/// free-memory monitor (the mmap subtlety: file-backed weight pages report
/// "available" while load-bearing):
///
/// - `weights_host_bytes` = GGUF file size MINUS the expert-tensor total. Expert
///   bytes are exactly what the governed cache holds (funded BY the lease) or sit
///   hot in VRAM — they are never host working set. Without the split, a 663GB
///   streaming MoE would "cost" 663GB of host RAM and the lease would derive 0.
/// - KV and compute-buffer terms come from the planner's OWN laws (`kv_at`,
///   `prefill_compute_reserve` — one formula, one place), but computed over a
///   footprint built from the HOST-RESIDENT mass, not the full file: those
///   heuristics scale with weight bytes as a proxy for layer/graph size, and for
///   a streaming MoE the dense/attention share is the mass that actually sizes
///   the KV stack and prefill graph (663GB would yield an absurd 68GB KV
///   estimate and a 41GB buffer, deriving a permanent zero lease).
/// - `live_kv_bytes` uses the LIVE served window × LIVE lane count from the
///   snapshot (llama.cpp allocates one full window per slot) — the term whose
///   omission thrashed the 63GB box to 95.9GB commit.
///
/// `None` when the host share is zero (a degenerate all-expert read — no honest
/// footprint to derive from; publish nothing rather than a guess).
pub(super) fn moe_host_cache_lease_inputs(
    model_id: &str,
    file_weights_bytes: u64,
    expert_bytes_total: u64,
    model_context_window: u32,
    served_window: u32,
    lanes: u32,
    physical_bytes: u64,
    commit_charge_bytes: Option<u64>,
    prompt_cache_mib: u32,
) -> Option<crate::capacity::host_cache_lease::HostCacheLeaseInputs> {
    let weights_host_bytes = file_weights_bytes.saturating_sub(expert_bytes_total);
    let fp = footprint_from_parts(model_id, weights_host_bytes, model_context_window, false, None)?;
    let lanes = lanes.max(1);
    Some(crate::capacity::host_cache_lease::HostCacheLeaseInputs {
        physical_bytes,
        weights_host_bytes,
        live_kv_bytes: fp.kv_at(served_window).saturating_mul(lanes as u64),
        // The llama-server host prompt cache (`--cache-ram`, KV-ECONOMY §4) is a
        // real resident term of the serve's working set — accounted here so the
        // expert-cache lease can never be derived as if that RAM were free.
        compute_buffer_bytes: fp
            .prefill_compute_reserve(served_window, lanes)
            // The DERIVED prompt cache of the current serve — the same number the
            // lane's --cache-ram carries (restore-economy 1.b), never a constant
            // the lease has to work around.
            .saturating_add((prompt_cache_mib as u64) * 1024 * 1024),
        os_floor_bytes: host_os_floor_bytes(physical_bytes),
        commit_charge_bytes,
    })
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
            let ctx =
                crate::model_registry::artifacts::resolve_gguf_for_model(model).and_then(|gguf| {
                    crate::capacity::moe_serving::moe_serving_context(
                        &gguf,
                        &model.id,
                        EXPERT_PLACEMENT_MARGIN_BYTES,
                        RELAUNCH_LAYER_CHURN_THRESHOLD,
                    )
                });
            *guard = ctx.map(|c| (model.id.clone(), c));
        }
        let (_, ctx) = guard.as_mut()?;
        let outcome = ctx
            .pager
            .tick_layer_placement(budget, ctx.n_experts_per_layer, ctx.n_layers);
        // Glass-box the K3 residency decision (Joel: the K3 path is observability-first).
        // Every load-bearing quantity a breakpoint would want: what fit, out of how many, on
        // what budget, and whether it moves the served process this tick.
        crate::probe!(
            class = "serving.k3_placement",
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
                Some((id, ctx)) if id.as_str() == model.id.as_str() => ctx.expert_bytes_total,
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
        crate::probe!(
            class = "serving.device_fit",
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

    pub(super) fn publish_moe_host_cache_lease(&self) {
        let live = self.serving_tx.borrow().clone();
        if !live.ready || live.served_context_window == 0 {
            return;
        }
        let Some(active) = live.active_model.clone() else {
            return;
        };
        let Some(port) = crate::inference::llama_server::port_of_base_url(&live.base_url) else {
            return;
        };
        // Expert geometry for the ACTIVE model — present only for a MoE serve.
        // Read under the same never-across-await sync Mutex as the placement path.
        // top_k + experts-per-layer feed the retention verdict below: the lease
        // is only USEFUL when it exceeds one token's expert working set.
        let (expert_bytes_total, n_experts_per_layer, top_k, n_layers) = {
            let Ok(guard) = self.moe_serving.lock() else {
                return;
            };
            match guard.as_ref() {
                Some((id, ctx)) if *id == active => (
                    ctx.expert_bytes_total,
                    ctx.n_experts_per_layer,
                    ctx.top_k,
                    ctx.n_layers,
                ),
                _ => return, // dense model (or stale context) — no governed cache to lease
            }
        };
        let Some(model) = (self.model_resolver)(&active) else {
            return;
        };
        let Some(fp) = footprint_for(&model) else {
            return;
        };
        let physical = self.system.memory().total_bytes;
        if physical == 0 {
            return;
        }
        let Some(inputs) = moe_host_cache_lease_inputs(
            &active,
            fp.weights_bytes,
            expert_bytes_total,
            model.context_window,
            live.served_context_window,
            live.lanes,
            physical,
            system_commit_charge_bytes(),
            // The running engine's grant; an adopted lane (unknown, 0) accounts the
            // declared prior, as the cold-start derivation does.
            if live.host_prompt_cache_mib > 0 {
                live.host_prompt_cache_mib
            } else {
                crate::inference::lane_args::CACHE_RAM_MIB
            },
        ) else {
            return;
        };
        let derived = crate::capacity::host_cache_lease::host_cache_lease_bytes(&inputs);
        // Sticky band on the BUDGET axis only: `budget_moved` says the
        // published value changed; a held budget reuses the last published
        // value so the ACTUATOR axis (pins, below) can still trigger a
        // write. The no-churn property now lives on the write decision —
        // nothing changed on either axis ⇒ no write, no mtime churn.
        let (budget_bytes, budget_moved) = {
            let Ok(mut sticky) = self.host_cache_lease.lock() else {
                return;
            };
            match sticky.observe(derived) {
                Some(published) => (published, true),
                None => (sticky.published_bytes(), false),
            }
        };
        if budget_bytes == 0 {
            return; // nothing governed yet — never publish a zero lease
        }
        // DEVICE axis (#305, BigMama's measured ask): the copy-stream's
        // expert-slot budget = live free-VRAM-after-fit less the placement
        // margin. The board's `available(Vram)` is already net of the
        // resident serve (#79 registers it as a consumer), so this is
        // exactly "size to the free VRAM AFTER device-fit, not a fixed
        // number". Sticky like the host axis; a zero publishes as ABSENT
        // (the mechanism keeps its own sizing), never as a zero budget.
        let device_derived = governed_vram_ceiling(&self.resource_daemon)
            .unwrap_or(0)
            .saturating_sub(EXPERT_PLACEMENT_MARGIN_BYTES);
        let (device_budget_bytes, device_moved) = {
            let Ok(mut sticky) = self.device_budget_lease.lock() else {
                return;
            };
            match sticky.observe(device_derived) {
                Some(published) => (published, true),
                None => (sticky.published_bytes(), false),
            }
        };
        let Some(gb) = crate::inference::llama_server::moe_glass_box_paths(port) else {
            return;
        };
        // Pin actuator (#281): drain the fork's routed-expert trace into
        // the bandit and carry its hot list on the governed plan. The pin
        // budget is retention-derived — at most half the lease's worth of
        // experts (the recency window keeps the other half) — so a lease
        // too small to retain anything publishes budget-only (v1 shape).
        let (
            pins,
            boundary_crossed,
            tokens_observed,
            pins_moved,
            repeat_recall,
            delta_recall,
            coverage,
        ) = {
            let Ok(mut tail_guard) = self.moe_trace_tail.lock() else {
                return;
            };
            let tail = tail_guard
                .get_or_insert_with(|| crate::capacity::trace_tail::MoeTraceTail::new(n_layers));
            tail.ensure_geometry(n_layers);
            tail.drain(&gb.trace);
            let ceiling = crate::capacity::trace_tail::pin_ceiling(
                budget_bytes,
                expert_bytes_total,
                n_layers,
                n_experts_per_layer,
            );
            let pins = tail.pin_list(ceiling);
            let moved = tail.pins_changed(&pins);
            (
                pins,
                tail.boundary_crossed,
                tail.tokens_observed,
                moved,
                tail.repeat_recall_x100(),
                tail.predicted_delta_recall_x100(),
                tail.schedulable_coverage_x100(),
            )
        };
        // Division actuation (#2 of the resident/cache split, contract 2026-08-03): the
        // warm-started bandit over the discovered `--resident-only` tiers picks which
        // RESIDENT precision the next relaunch should load, and the trace-tail token
        // delta feeds back the measured decode tok/s for the tier actually serving.
        // The tier label is a fourth plan axis; the device budget above remains the
        // one budget authority (a smaller resident frees VRAM the board then sees).
        let division = self.division_tick(
            &active,
            inputs.weights_host_bytes,
            expert_bytes_total,
            n_experts_per_layer,
            top_k,
            n_layers,
            tokens_observed,
        );
        let tier_moved = division.as_ref().is_some_and(|(_, moved)| *moved);
        if !budget_moved && !pins_moved && !boundary_crossed && !device_moved && !tier_moved {
            return; // no axis changed — no write, no mtime churn
        }
        let pins_count = pins.len();
        let mut doc = crate::capacity::plan_file::PlanFileDocument::new(
            budget_bytes,
            MOE_PLAN_WINDOW_K,
            pins,
        );
        if device_budget_bytes > 0 {
            doc = doc.with_device_budget(device_budget_bytes);
        }
        if let Some((tier_label, _)) = &division {
            doc = doc.with_resident_tier(tier_label.clone());
        }
        // Retention verdict (#287): does the published lease retain even one
        // token's expert working set? Below 100 (×100 fixed-point) the cache
        // buys NOTHING — every token evicts the previous token's experts (the
        // resident=0 thrash) — and the lease is honest about it instead of
        // looking healthy while thrashing silently.
        let per_token_ws = crate::capacity::host_cache_lease::per_token_expert_working_set_bytes(
            expert_bytes_total,
            n_experts_per_layer,
            top_k,
        );
        let retention_x100 =
            crate::capacity::host_cache_lease::retention_tokens_x100(budget_bytes, per_token_ws);
        if retention_x100 < 100 {
            crate::probe!(
                class = "serving.moe_host_cache_retention",
                model = active.as_str(),
                budget_bytes,
                per_token_ws_bytes = per_token_ws,
                retention_tokens_x100 = retention_x100,
                "lease retains LESS than one token's expert working set — the \
                 cache thrashes (zero cross-token reuse); free host RAM or accept \
                 streaming-only decode",
            );
        }
        match crate::capacity::plan_file::write_plan_file(&gb.plan, &doc) {
            Ok(()) => crate::probe!(
                class = "serving.moe_host_cache_lease",
                model = active.as_str(),
                budget_bytes,
                device_budget_bytes,
                device_derived_bytes = device_derived,
                derived_bytes = derived,
                weights_host_bytes = inputs.weights_host_bytes,
                live_kv_bytes = inputs.live_kv_bytes,
                per_token_ws_bytes = per_token_ws,
                retention_tokens_x100 = retention_x100,
                pins = pins_count as u64,
                trace_tokens = tokens_observed,
                warm_start = boundary_crossed,
                repeat_recall_x100 = repeat_recall.unwrap_or(0) as u64,
                predicted_delta_recall_x100 = delta_recall.unwrap_or(0) as u64,
                schedulable_coverage_x100 = coverage.unwrap_or(0) as u64,
                plan = gb.plan.display().to_string().as_str(),
                "published governed host-cache lease: {} GiB (raw {} GiB, retains \
                 {}.{:02} tokens' expert set)",
                budget_bytes / (1024 * 1024 * 1024),
                derived / (1024 * 1024 * 1024),
                retention_x100 / 100,
                retention_x100 % 100,
            ),
            Err(e) => crate::probe!(
                class = "serving.moe_host_cache_lease",
                model = active.as_str(),
                budget_bytes,
                error = e.to_string().as_str(),
                "plan-file write FAILED — actuator frozen at last published budget",
            ),
        }
    }

    /// One division-actuation tick (#2 of the resident/cache split, contract
    /// 2026-08-03). Ensures the actuator exists for the ACTIVE model (tier discovery +
    /// warm-start, rebuilt on model change), feeds the measured decode reward from the
    /// trace-tail token watermark, and returns the chosen resident-tier label + whether
    /// it moved. `None` = no feasible division (no governed VRAM, or every tier starves
    /// the cache) — the plan carries no `resident_tier` and spawn behavior is unchanged.
    fn division_tick(
        &self,
        active: &str,
        native_resident_bytes: u64,
        expert_bytes_total: u64,
        n_experts_per_layer: u32,
        top_k: u32,
        n_layers: u32,
        tokens_observed: u64,
    ) -> Option<(String, bool)> {
        use crate::capacity::division_actuation as da;
        let shape =
            da::shape_from_geometry(expert_bytes_total, n_layers, n_experts_per_layer, top_k)?;
        let Ok(mut guard) = self.division.lock() else {
            return None;
        };
        let stale = guard
            .as_ref()
            .map(|a| a.model_id() != active)
            .unwrap_or(true);
        if stale {
            // Live-anchored budget: the board's free-VRAM-after-fit (the #305 device
            // axis — already net of the resident serve and the placement margin) IS the
            // native tier's expert-cache pool, and a precision-shrunk resident grows it
            // by exactly its shrink delta. Expressed in the policy's HardwareBudget
            // shape as vram_total = live_free + native_resident with kv/reserve
            // pre-netted to 0 — one budget derivation, no parallel arithmetic.
            let live_free = governed_vram_ceiling(&self.resource_daemon)?
                .saturating_sub(EXPERT_PLACEMENT_MARGIN_BYTES);
            if live_free == 0 {
                return None;
            }
            let tiers = da::discover_resident_tiers(
                &crate::model_registry::artifacts::device_fit_cache_dir(active),
                native_resident_bytes,
            );
            let n_tiers = tiers.len();
            let hw = expert_pager_policy::division::HardwareBudget {
                vram_total_bytes: live_free.saturating_add(native_resident_bytes),
                kv_bytes: 0,
                compute_reserve_bytes: 0,
            };
            let served = self.served_resident.lock().ok().and_then(|g| g.clone());
            *guard = da::DivisionActuator::build(
                active,
                tiers,
                &hw,
                &shape,
                // v1 curve: the measured K3 trace-replay points. Per-model measured
                // curves ride the same seam once other MoEs report theirs.
                &expert_pager_policy::division::CoverageModel::k3_measured(),
                served.as_deref(),
            );
            crate::probe!(
                class = "serving.division",
                model = active,
                tiers = n_tiers as u64,
                feasible = guard.is_some(),
                live_free_bytes = live_free,
                native_resident_bytes,
                "division actuator (re)built: {} tier(s) in catalog, feasible={}",
                n_tiers,
                guard.is_some(),
            );
        }
        let act = guard.as_mut()?;
        if let Some(tok_s) = act.observe_tick(tokens_observed, std::time::Instant::now()) {
            crate::probe!(
                class = "serving.division_reward",
                model = active,
                served_tier = act.served_tier_label(),
                tok_s_x100 = (tok_s * 100.0) as u64,
                "measured decode {:.2} tok/s credited to served tier '{}'",
                tok_s,
                act.served_tier_label(),
            );
        }
        let (label, moved) = act.choose()?;
        if moved {
            crate::probe!(
                class = "serving.division",
                model = active,
                chosen_tier = label.as_str(),
                served_tier = act.served_tier_label(),
                "division choice MOVED: next relaunch should load resident tier '{}' \
                 (two-speed — no relaunch is triggered by this publish)",
                label,
            );
        }
        Some((label, moved))
    }
}
