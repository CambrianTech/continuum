//! PLAN: the host budget, live candidates, admission authority, the serving plan, and expert placement.
//! Split out of serving_daemon.rs (card dbc9711c, BigMama's step 6).

use super::*;

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

    /// The servable candidates RIGHT NOW: the on-disk Ready models, MINUS any an
    /// operator has explicitly unloaded (`serving/unload`), and — when a force-pin
    /// is set (`serving/pin`) — INTERSECTED to just the pinned model. The single
    /// chokepoint every plan flows through, so a suppressed model can never be
    /// planned and a pin can never be escaped: pin set → the planner has exactly
    /// one candidate, so the reconcile serves that model or (if it has dropped off
    /// disk) nothing. Suppress subtracts; pin intersects; the planner still owns
    /// the choice among whatever remains.
    /// What to tell a human when nothing is serving. "No local weights" is a LIE when the
    /// weights are there and every candidate was refused for a nameable reason.
    pub(super) fn no_candidate_reason(&self) -> String {
        let intent = self.intent.snapshot();
        let (kept, refused) = servable_candidates_with_refusals(
            &self.catalog.snapshot(),
            &intent.suppressed,
            &intent.pinned,
        );
        if !kept.is_empty() {
            return "a candidate exists but no plan was produced — read serving.plan".to_string();
        }
        if refused.is_empty() {
            return "no servable model on disk — the planner found no local weights to \
                    bring up (pull one with `continuum models/pull`)"
                .to_string();
        }
        let named = refused
            .iter()
            .map(|r| format!("{} ({})", r.model_id, r.reason))
            .collect::<Vec<_>>()
            .join("; ");
        format!(
            "no candidate to host citizens — {} model(s) on disk were refused: {named}",
            refused.len()
        )
    }

    pub(super) fn live_candidates(&self) -> Vec<ModelFootprint> {
        let intent = self.intent.snapshot();
        servable_candidates(&self.catalog.snapshot(), &intent.suppressed, &intent.pinned)
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
    pub fn lane_admission_authority(&self) -> crate::inference::llama_server::LaneAdmissionAuthority {
        let system = self.system.clone();
        let resource_daemon = self.resource_daemon.clone();
        let serving_tx = self.serving_tx.clone();
        let model_resolver = self.model_resolver.clone();
        Arc::new(move |target: &crate::inference::llama_server::ServingTarget,
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
        })
    }

    /// Compute the current serving plan from the live host snapshot + on-disk
    /// models, WITHOUT relying on a tick having run. The boot path calls this
    /// to drive the spawner before the tick loop starts — single source of
    /// truth for "what model + how many lanes."
    pub fn compute_plan(&self) -> Option<ServingPlan> {
        plan_serving(
            &self.host_budget(),
            &self.live_candidates(),
            &self.serving_demand())
    }

    /// The detected hardware tier for this host, for the persona spawner's
    /// `n_gpu_layers` + roster shape. Same GPU detection that feeds the serving
    /// plan — one hardware authority, not two.
    pub fn detected_tier(&self) -> (HwCapabilityTier, HwTierCategory, &'static str) {
        detect_tier(self.gpu.gpu_name())
    }

    /// Recompute the plan from the live host snapshot + on-disk models, publish
    /// it, and log the decision. Idempotent — safe to call on init and tick.
    pub(super) fn recompute(&self) {
        // Don't re-plan while a relaunch of the serving lane is IN FLIGHT (#216). During the
        // kill→respawn, the GPU scan (`physical_used`) transiently drops as the old lane exits
        // and rises as the new one loads, so `host_budget()` — which reads the board's
        // `available = capacity − max(granted, physical_used)` — sees a PHANTOM free-VRAM
        // spike/dip. A plan recomputed off that transient flaps the window and triggers ANOTHER
        // relaunch: the thrash loop (glass-boxed 2026-07-20 — board available swung 12.9↔41GB,
        // window 44800↔7680, back-to-back relaunches). Serving's own in-flight churn must not
        // feed back into its own plan. Hold the last plan until the reconcile settles; the gate
        // clears via RAII (#214) the instant the relaunch finishes OR fails, so re-planning
        // resumes promptly against the STABLE post-relaunch budget — no thrash, no stall. (The
        // deeper fix — serving holding an explicit board lease so `granted` pins its residency
        // and the scan transient never reaches `available` at all — is the #56 consumers-LEASE
        // residual; this breaks the feedback loop cleanly in the meantime.)
        if self.reconciling.load(Ordering::Acquire) {
            self.note_reconcile_busy("recompute");
            return;
        }
        let budget = self.host_budget();
        let intent = self.intent.snapshot();
        let catalog = self.catalog.snapshot();
        let candidates = servable_candidates(&catalog, &intent.suppressed, &intent.pinned);
        let on_disk = servable_candidates(&catalog, &HashSet::new(), &intent.pinned);
        self.publish_plan_for_intent(budget, &candidates, &on_disk, intent.revision);
    }

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
        let budget = forced.unwrap_or_else(|| {  // unwrap_or_else: the forced expert budget when a measurement set one, else the measured budget below
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

    /// One division-actuation tick (#2 of the resident/cache split, contract
    /// 2026-08-03). Ensures the actuator exists for the ACTIVE model (tier discovery +
    /// warm-start, rebuilt on model change), feeds the measured decode reward from the
    /// trace-tail token watermark, and returns the chosen resident-tier label + whether
    /// it moved. `None` = no feasible division (no governed VRAM, or every tier starves
    /// the cache) — the plan carries no `resident_tier` and spawn behavior is unchanged.
    pub(super) fn division_tick(
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
            .unwrap_or(true);  // unwrap_or: no artifact recorded reads as stale, so the footprint is re-measured, never trusted
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
