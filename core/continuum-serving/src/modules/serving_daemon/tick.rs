//! Tick ownership for the serving daemon.

use super::*;

#[async_trait]
impl ServiceModule for ServingDaemonModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "serving-daemon",
            priority: ModulePriority::Normal,
            command_prefixes: &["serving/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: Some(TICK),
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        // Capture the bus so snapshot changes fan out to subscribers (the
        // cbar-stage shape). Set-once; ignore a re-init.
        let _ = self.bus.set(ctx.bus.clone());
        // Install our serving-state watch as the process-wide readable seam so
        // free functions + adapters read "what's live" as a pointer instead of
        // each probing /v1/models. Set-once (singleton daemon).
        let _ = crate::inference::llama_server::install_serving_state(self.subscribe_serving());
        // THE DEVICE-BUDGET AUTHORITY, wired here rather than promised in a doc comment.
        // Without it `EphemeralServingLane::spawn` admits every engine with only a
        // free-port check — which is how three sidecars once shared one GPU. Set-once
        // (singleton daemon); `the_serving_daemon_installs_the_admission_authority`
        // pins that this call exists.
        let _ =
            crate::inference::llama_server::install_lane_admission(self.lane_admission_authority());
        // NOTE: the VRAM prior is seeded and serving is registered as a ResourceConsumer at
        // WIRING time ([`declare_to_memory_authority`], called from `ipc/mod.rs` before
        // `register_planner_on_authority_tick`) — NOT here. `initialize` runs after the module
        // is handed to the runtime, and the authority's tick can fire a plan in between; a plan
        // computed while serving is not yet a registered consumer gets NO add-back for its own
        // residency, which is the #438 `usable_gb = 0` window. Both calls are idempotent, so
        // this is an ordering constraint, not a duplication hazard.
        // Reap orphaned llama-server lanes left by a crashed/SIGKILLed predecessor
        // BEFORE reconciling to the new plan — a mid-eval crash leaves ephemeral
        // lanes (their own scanned ports, no pidfile) holding ~6 GB each with zero
        // reclaim record but the registry. Sweeping first frees that VRAM so the
        // new live lane comes up without competing against dead siblings.
        for outcome in crate::inference::lane_registry::sweep_orphans() {
            continuum_base::probe!(
                target: "continuum_core::modules::serving_daemon", class = "serving.lane_registry.sweep",
                outcome = format!("{outcome:?}").as_str(),
                "boot lane-registry sweep",
            );
        }
        // Plan once at boot so the decision is published before the first tick,
        // then kick the first reconcile so the server comes up promptly rather
        // than waiting a full tick interval. The reconcile runs detached.
        self.recompute();
        self.seed_reconcile_pages().await;
        let _ = self.reconcile_to_plan();
        Ok(())
    }

    async fn tick(&self) -> Result<(), String> {
        crate::inference::llama_server::collect_retired_engines();
        self.poll_teacher_batch();
        self.note_plan_none_with_live_lane();
        // The plan is DECIDED on the memory authority's tick now (MEMORY-AUTHORITY-DAEMON:
        // `register_planner_on_authority_tick` runs `recompute()` as an `on_tick` observer,
        // publishing to `plan_tx`) — serving no longer samples memory on its own tick. This
        // tick only RECONCILES: bring the running server in line with the authority's
        // published plan. Fast-to-decide; the slow relaunch spawns off the tick.
        // BEFORE the reconcile, so a wedge reported since the last tick is already published
        // not-ready when the reconcile reads the snapshot — otherwise the escalation waits a
        // full tick behind the very guard it exists to defeat.
        self.take_reported_wedge();
        self.seed_reconcile_pages().await;
        let _ = self.reconcile_to_plan();
        // Liveness heartbeat (#175 self-heal): on a slow cadence, re-verify that the lane
        // we believe is `ready` can ACTUALLY decode — the reconcile trusts the published
        // `ready` forever and would never notice an OOM-poisoned backend otherwise. Off the
        // tick, hysteresis-gated; a sustained failure flips not-ready and the next reconcile
        // respawns the lane.
        let _ = self.spawn_health_heartbeat_if_due();
        // #287: for a ready MoE serve, re-derive the governed host-cache lease from the
        // LIVE working set (KV grows as slots fill) and publish it through the sticky
        // band to the per-port plan file — the actuator her ResidencyCache polls.
        self.publish_moe_host_cache_lease();
        self.lower_spawn_baseline_to_the_trough();
        self.sample_lane_footprint().await;
        Ok(())
    }

    async fn drain(&self) -> Result<u32, String> {
        self.interrupt_teacher_batch()
    }

    async fn shutdown(&self) -> Result<(), String> {
        self.interrupt_teacher_batch().map(|_| ())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // The full `serving/*` surface (plan · status · load · unload) is migrated to
        // the typed registry (`commands/serving/*`, wired via `commands()`). Fail loud
        // — no silent fallback.
        Err(format!(
            "serving-daemon command surface is migrated to the typed registry; \
             '{command}' has no legacy handler"
        ))
    }

    /// The typed `serving/*` family on the ONE command registry, so a persona /
    /// operator / grid peer is OFFERED serving control + inspection as real tools:
    /// the VRAM-axis deallocation pair (`serving/unload` frees a lane, `serving/load`
    /// permits it again) and the two read surfaces (`serving/plan` = intent,
    /// `serving/status` = reality). They share the daemon's suppress-set writer, its
    /// published serving snapshot, and its serving-plan receiver; the daemon's own
    /// plan/reconcile loop turns the suppress-set edits into actual (un)loads.
    fn commands(&self) -> Vec<Arc<dyn crate::sdk_codegen::DynCommand>> {
        super::commands::command_objects(
            self.intent.clone(),
            self.pin_fit_checker(),
            self.subscribe_serving(),
            self.subscribe(),
            self.catalog.clone(),
            self.pin_store.clone(),
        )
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl ServingDaemonModule {
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
            // No storage root (no home, no storage tier) holds no cached tiers.
            let tiers = match crate::model_registry::artifacts::device_fit_cache_dir(active) {
                Some(dir) => da::discover_resident_tiers(&dir, native_resident_bytes),
                None => Default::default(),
            };
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
            continuum_base::probe!(
                target: "continuum_core::modules::serving_daemon", class = "serving.division",
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
            continuum_base::probe!(
                target: "continuum_core::modules::serving_daemon", class = "serving.division_reward",
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
            continuum_base::probe!(
                target: "continuum_core::modules::serving_daemon", class = "serving.division",
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
