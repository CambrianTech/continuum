//! Reconcile ownership for the serving daemon.

use super::*;

impl ServingDaemonModule {
    /// No plan → publish the empty snapshot (no servable model = nothing live).
    /// Already serving the desired model & ready → no-op. A reconcile already
    /// in flight → skip (the gate). Otherwise spawn the reconcile.
    /// A tick or a planner that skips because a reconcile is in flight says so — once per
    /// half minute, with the operation's age and the await it is in — and names a WEDGE
    /// once the age passes [`RECONCILE_WEDGE_BOUND`]. Silence here is how the M5 sat dark
    /// for six minutes with a healthy engine on its port (card c3c50e0d).
    /// A LIVE lane with NO published plan for [`PLAN_NONE_LIVE_LANE_TICKS`] ticks is a
    /// named silence: `serving.plan.none_with_live_lane {ticks, lane_pid, model}`, once per
    /// half minute. The engine is on its port; nobody decided anything about it.
    pub(super) fn note_plan_none_with_live_lane(&self) {
        if self.plan_tx.borrow().plan.is_some() {
            self.plan_none_live_lane_ticks.store(0, Ordering::Release);
            return;
        }
        // The lane registry's live record covers both an inherited engine and our own
        // (the daemon writes the pidfile + record for the lane it spawns).
        let Some((lane_pid, model)) = (self.inherited_lane)().map(|r| (r.pid, r.model)) else {
            self.plan_none_live_lane_ticks.store(0, Ordering::Release);
            return;
        };
        let ticks = self
            .plan_none_live_lane_ticks
            .fetch_add(1, Ordering::AcqRel)
            .saturating_add(1);
        if ticks < PLAN_NONE_LIVE_LANE_TICKS {
            return;
        }
        let now = EpochMs::now().millis();
        let said = self.plan_none_said_ms.load(Ordering::Acquire);
        if now.saturating_sub(said) < RECONCILE_BUSY_SAY_EVERY.as_millis() as u64 {
            return;
        }
        self.plan_none_said_ms.store(now, Ordering::Release);
        continuum_base::probe!(
            target: "continuum_core::modules::serving_daemon", class = "serving.plan.none_with_live_lane",
            ticks,
            lane_pid = lane_pid as u64,
            model = model.as_str(),
            "a lane is live on its port and no plan has been published for this many ticks \
             — nothing decided anything about it; read the last serving.plan.* refusal"
        );
    }

    pub(super) fn note_reconcile_busy(&self, skipped: &'static str) {
        let started = self.reconcile_started_ms.load(Ordering::Acquire);
        if started == 0 {
            return;
        }
        let now = EpochMs::now().millis();
        let age = Duration::from_millis(now.saturating_sub(started));
        let said = self.reconcile_busy_said_ms.load(Ordering::Acquire);
        if now.saturating_sub(said) < RECONCILE_BUSY_SAY_EVERY.as_millis() as u64 {
            return;
        }
        self.reconcile_busy_said_ms.store(now, Ordering::Release);
        let step_kind = ReconcileStep::from_u8(self.reconcile_step.load(Ordering::Acquire));
        let step = step_kind.name();
        if reconcile_wedged_at(step_kind, age) {
            continuum_base::probe!(
                target: "continuum_core::modules::serving_daemon", class = "serving.reconcile.wedged",
                skipped,
                step,
                age_ms = age.as_millis() as u64,
                bound_ms = RECONCILE_WEDGE_BOUND.as_millis() as u64,
                "a reconcile operation has outlived the bound a healthy one can hold — the \
                 daemon is skipping every tick and plan on its gate; sample the core and read \
                 this step's await before touching the engine"
            );
        } else {
            continuum_base::probe!(
                target: "continuum_core::modules::serving_daemon", class = "serving.reconcile.busy",
                skipped,
                step,
                age_ms = age.as_millis() as u64,
                "a reconcile is in flight; this pass skipped on the gate"
            );
        }
    }

    /// A re-home streak starts over: the count and the plan high it was measured

    /// against together, so a stale high can never make a later climb read as flat.

    /// An ADOPTED engine on fixed slots moves to the shared pool once its pool can be
    /// checkpointed: a core that adopts the engine its predecessor left learns the slot pool
    /// only after serving it, so the move asked for at boot is refused and handed back ("adopted
    /// checkpoint requires a known pool", the M5 2026-11-03 04:22Z), and a ready lane never
    /// re-enters the launch path on its own. One try per engine process; the check is one lock
    /// per tick until the pool is known, and the argv is read once.
    fn shared_pool_move_due(&self, base_url: &str) -> bool {
        let endpoint = crate::ai::openai_endpoints::OpenAiBase::new(base_url);
        if !crate::inference::slots::directory().adopted_replacement_ready(endpoint.root()) {
            return false;
        }
        let Some(engine) = self.server.served_incarnation() else {
            return false;
        };
        let pid = u64::from(engine.pid);
        if self.shared_pool_move_tried.swap(pid, Ordering::Relaxed) == pid {
            return false;
        }
        let due = self.server.launched_on_shared_pool() == Some(false);
        if due {
            continuum_base::probe!(
                target: "continuum_core::modules::serving_daemon", class = "serving.pool.move",
                pid,
                "an adopted engine on fixed slots can now be checkpointed: moving it onto the shared pool, once"
            );
        }
        due
    }

    pub(super) fn reconcile_to_plan(&self) -> Option<JoinHandle<()>> {
        let operation = match ServingOperation::acquire(
            self.reconciling.clone(),
            self.reconcile_started_ms.clone(),
            self.reconcile_step.clone(),
        ) {
            Some(operation) => operation,
            None => {
                self.note_reconcile_busy("reconcile");
                return None;
            }
        };
        let planned = self.plan_tx.borrow().clone();
        let intent = self.intent.snapshot();
        if planned.intent_revision != intent.revision {
            return None;
        }

        // External serving pin (misfit / grid design): when the operator pinned an
        // EXTERNAL OpenAI-compatible endpoint via `LLAMA_SERVER_BASE_URL`, this node
        // does NOT own a local GPU serving lane — it ADOPTS the pinned endpoint. We
        // spawn / reclaim NOTHING (that would fight a co-located engine, e.g. a K3
        // llama-server, for its port), but we MUST publish the endpoint's ready
        // ServingSnapshot to `SERVING_STATE` so every consumer sees a ready lane:
        //   - `await_ready_serving` (persona-host gate + adapter factory),
        //   - the adapter's pre-generate model-guard via `current_serving()` — which
        //     refuses to generate unless the request's model == the resident model,
        //     read straight from the published snapshot (an empty snapshot → every
        //     turn "model is not the active served model", the bug this fixes).
        // Trust once-ready (a genuine wedge surfaces LOUD on a real turn); re-probe
        // only while not-yet-ready. The reachability probe is control-plane-fast, so
        // this publishes within a tick and never overlaps.
        if crate::inference::llama_server::external_serving_pin().is_some() {
            if self.serving_tx.borrow().ready {
                return None;
            }
            let serving_tx = self.serving_tx.clone();
            let bus = self.bus.get().cloned();
            return Some(tokio::spawn(async move {
                let operation = operation;
                operation.step(ReconcileStep::External);
                if let Some(snap) = crate::inference::llama_server::probe_external_serving(
                    crate::inference::llama_server::DEFAULT_SERVING_WAIT,
                )
                .await
                {
                    Self::emit_serving(bus.as_ref(), &snap);
                    let _ = serving_tx.send_replace(snap);
                }
            }));
        }
        // A healthy HTTP control plane cannot acknowledge a cancelled paging
        // operation. Surface the existing endpoint owner's quarantine before the
        // unchanged-plan shortcut, then use the normal drain/retire/serve path.
        if self.server.paging_recovery_required() {
            let mut live = self.serving_tx.borrow().clone();
            if live.ready {
                live.ready = false;
                live.vision_ready = false;
                live.vision_base_url = None;
                live.vision_model = None;
                live.degraded_reason =
                    Some("KV paging completion unverified; replacing owned engine".into());
                Self::emit_serving(self.bus.get(), &live);
                let _ = self.serving_tx.send_replace(live);
            }
        }
        // AN ENGINE THAT HOSTS LIVE WORK IS NOT RECONCILED (SHARED-RESIDENT-LIFECYCLE.md step 1;
        // Kimi's attempt 2, 5090, 2026-09-28 12:29Z: the plan downshifted the 27B under a live
        // /train, the swap killed the engine, and the run with it). Every path below can end
        // the engine: a model swap, a window re-home, a genome page-in, an empty-plan
        // retirement (idle_if_current). None runs while work is bound to it, and the streaks
        // those paths earn restart from zero, so a plan formed under the work's memory cannot
        // commit the instant it releases. The one exception is the emergency this owner may not
        // wait out, an engine whose KV paging is unverified: that replacement proceeds, and the
        // work is TOLD why before it commits.
        let occupancy = self.engine_occupancy();
        // Published either way (card a18452d6): a held engine says what holds it; a free one, or
        // one the emergency below replaces anyway, is no longer held.
        let held = occupancy.holds() && !self.server.paging_recovery_required();
        self.publish_hold(if held { &occupancy } else { &crate::inference::engine_residency::Occupancy::Free });
        if occupancy.holds() {
            let port = (self.inherited_lane)().map(|r| r.port);
            if self.server.paging_recovery_required() {
                let told = match (port, crate::paths::continuum_home()) {
                    (Some(port), Ok(home)) => crate::inference::engine_residency::interrupt(
                        &crate::inference::engine_residency::store_path(&home),
                        port,
                        "an emergency replacement: the engine's KV paging completion is unverified",
                    )
                    .map(|jobs| jobs.len())
                    .map_err(|e| e.to_string()),
                    _ => {
                        Err("no live lane record or home to record the interruption in".to_string())
                    }
                };
                continuum_base::probe!(
                    target: "continuum_core::modules::serving_daemon", class = "serving.reconcile.resident_engine_replaced",
                    occupancy = %format!("{occupancy:?}"),
                    told = %format!("{told:?}"),
                    "an emergency replaces an engine that hosts live work; the work was told why first"
                );
            } else {
                self.model_change_streak.store(0, Ordering::Relaxed);
                self.downshift_streak.store(0, Ordering::Relaxed);
                *self
                    .pending_model_change
                    .lock()
                    .unwrap_or_else(|p| p.into_inner()) = None; // unwrap_or_else: a poisoned streak cell is still cleared
                static LAST_HELD: std::sync::atomic::AtomicU64 =
                    std::sync::atomic::AtomicU64::new(0);
                let now = EpochMs::now().millis();
                // one row a minute while held, not one a tick
                if now.saturating_sub(LAST_HELD.load(Ordering::Relaxed)) >= 60_000 {
                    LAST_HELD.store(now, Ordering::Relaxed);
                    continuum_base::probe!(
                        target: "continuum_core::modules::serving_daemon", class = "serving.reconcile.held_for_resident_work",
                        occupancy = %format!("{occupancy:?}"),
                        plan_model = planned.plan.as_ref().map(|p| p.base_model.model_id.as_str()).unwrap_or("<none>"), // unwrap_or: probe label for an empty plan
                        "the engine hosts live work: no swap, re-home, page-in or retirement until it is released"
                    );
                }
                return None;
            }
        }
        // Pull the desired model id, the host-fit PER-LANE served window, AND
        // the lane count out of the plan in one borrow — both are the planner's
        // single source of truth (task #50). We carry them on the ServingTarget
        // so llama-server's `-c` (= window × lanes) and `--parallel` (= lanes)
        // match exactly what was planned: each slot gets one full served window.
        let (desired, served_ctx, lanes) = match planned.plan.as_ref() {
            Some(plan) => (
                plan.base_model.model_id.clone(),
                plan.served_context_window,
                plan.lanes,
            ),
            None => {
                let live = self.serving_tx.borrow();
                if live.active_model.is_none()
                    && live.loading_model.is_none()
                    && self.server.owned_engine().is_none()
                {
                    return None;
                }
                drop(live);
                let server = self.server.clone();
                let serving_tx = self.serving_tx.clone();
                let verified_target = self.verified_target.clone();
                let intent_owner = self.intent.clone();
                let revision = planned.intent_revision;
                let reason = self.no_candidate_reason();
                let bus = self.bus.get().cloned();
                return Some(tokio::spawn(async move {
                    let operation = operation;
                    let current = || intent_owner.snapshot().revision == revision;
                    if !current() {
                        return;
                    }
                    let admit = || {
                        let intent = intent_owner.state.borrow();
                        if intent.revision != revision {
                            return false;
                        }
                        drop(intent);
                        Self::publish_lifecycle_admission(
                            &serving_tx,
                            &verified_target,
                            bus.as_ref(),
                            None,
                        );
                        true
                    };
                    operation.step(ReconcileStep::Retire);
                    match server.idle_if_current(&admit).await {
                        Ok(()) => {
                            let _ = verified_target.send_replace(None);
                            // An admitted retirement request is not a capacity receipt.
                            // Publish physical unavailability even if newer intent arrived;
                            // the next normal owner pass reconciles that newer intent.
                            let snapshot = ServingSnapshot::degraded(reason);
                            Self::emit_serving(bus.as_ref(), &snapshot);
                            let _ = serving_tx.send_replace(snapshot);
                        }
                        Err(crate::inference::llama_server::LlamaServerError::Superseded) => {}
                        Err(e) => {
                            tracing::warn!(target: "continuum_core::modules::serving_daemon", error = %e, "empty-plan retirement failed")
                        }
                    }
                }));
            }
        };

        // The trained genome layers registered for this base model — which genes
        // are loadable into the serving catalog (the `--lora` set). Read from the
        // producer-written manifest, keyed by the CONTINUUM base id (the HF base
        // in each gene's PEFT config never matches the served id, so a directory
        // scan can't make this association — see genome::adapter_manifest). An
        // unreadable/corrupt manifest degrades to base-only HERE (logged loud) so
        // the reconcile tick never panics; a MISSING gene file still fails loud at
        // spawn (AdapterNotFound). Empty = base model only, the legitimate state.
        let desired_adapters: Vec<AdapterEntry> = match crate::genome::adapter_manifest::load() {
            Ok(all) => crate::genome::adapter_manifest::for_base(&all, &desired)
                .into_iter()
                .map(|a| AdapterEntry {
                    alias: a.alias,
                    path: a.path,
                })
                .collect(),
            Err(e) => {
                continuum_base::probe!(
                    target: "continuum_core::modules::serving_daemon", class = "serving.adapters",
                    desired = desired.as_str(),
                    error = e.as_str(),
                    "adapter manifest unreadable — serving base model only this tick",
                );
                Vec::new()
            }
        };
        let mut desired_adapter_paths: Vec<String> = desired_adapters
            .iter()
            .map(|a| a.path.to_string_lossy().into_owned())
            .collect();
        desired_adapter_paths.sort();

        {
            let live = self.serving_tx.borrow();
            // Already serving the right model AND the right genome set: nothing relaunches.
            // ONE LAUNCH (2026-11-03, Joel: "Slot counts should never require shutdown"): the
            // engine holds one shared pool sized once, so a plan that wants a wider window or
            // more lanes is not a relaunch. The re-home ladder that lived here (sustained
            // streaks, cooldowns, plan highs, holds, the cache-size re-home) relaunched the
            // M5's engine 40 times. Who holds how much of the pool is decided at runtime.
            if live.ready
                && live.active_model.as_deref() == Some(desired.as_str())
                && live.adapters == desired_adapter_paths
                && !self.shared_pool_move_due(&live.base_url)
            {
                // The lanes this hour served: the health window's maximum (the lane-bound rest
                // stands on it).
                crate::cognition::health_ledger::note_lanes(live.lanes as u64);
                // Remember the shape each mind is served for the next boot's plan: the engine's
                // own generation and sequences, at the share published from its pool (its
                // `/props` window is the whole pool on the shared engine). Settlement is
                // idempotent per generation, so a ready tick may record it.
                if let Some(model) = live.active_model.as_deref() {
                    let endpoint = crate::ai::openai_endpoints::OpenAiBase::new(&live.base_url);
                    if let Some(geometry) = crate::inference::slots::directory()
                        .endpoint(endpoint.root())
                        .served_geometry()
                        .filter(|g| g.model_id == model && g.lanes == live.lanes)
                    {
                        // Samples carry the engine's own shape: match them on it, record the share.
                        let observed = crate::inference::resource_admission::PromptSample::geometry_median(
                            &(self.leased_in_sent)(),
                            endpoint.root(),
                            &geometry,
                        );
                        let served = crate::inference::served_window_store::ServedGeometry {
                            per_slot_window: live.served_context_window,
                            ..geometry
                        };
                        crate::inference::served_window_store::record_reconciled(served, true, observed);
                    }
                }
                self.acknowledge_verified_target(&live, planned.intent_revision);
                return None;
            }
        }

        // L10 (#438, live 2026-08-15 14:13): a MODEL CHANGE on a READY lane must earn
        // the same sustained streak the window re-home above already requires. The
        // asymmetry was the bug: growing a window (mild) needed MODEL_CHANGE_SUSTAINED_TICKS
        // of consistent evidence, while swapping the base model (a full teardown that
        // also re-homes every persona to different WEIGHTS) actuated on a single plan
        // tick. One bogus usable_bytes sample during the incumbent's own load made the
        // planner's nothing-fits arm name the SMALLEST candidate, the swap fired
        // immediately, and citizens spoke qwen2.5-0.5B template salad into the round's
        // room — durable garbage every healthy peer then perceived as conversation.
        // With this gate the plan must want the DIFFERENT model for the same sustained
        // run of ticks; a one-tick transient can never re-home minds. Deliberately
        // scoped: only when the lane is READY and serving something else — boot,
        // wedge-recovery, and genome page-ins (same model, adapter change) stay
        // immediate, and a change to a DIFFERENT different model resets the streak.
        {
            let live = self.serving_tx.borrow();
            let mut pending = self
                .pending_model_change
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            let verdict = model_change_gate(
                live.ready,
                live.active_model.as_deref(),
                desired.as_str(),
                &mut pending,
                &self.model_change_streak,
                MODEL_CHANGE_SUSTAINED_TICKS,
            );
            match verdict {
                ModelChangeGate::Defer { streak } => {
                    continuum_base::probe!(
                        target: "continuum_core::modules::serving_daemon", class = "serving.reconcile.model",
                        decision = "deferred",
                        live_model = live.active_model.as_deref().unwrap_or("<none>"),
                        plan_model = desired.as_str(),
                        streak = streak as u64,
                        needs_streak = MODEL_CHANGE_SUSTAINED_TICKS as u64,
                        "plan wants a DIFFERENT model on a ready lane — the swap must \
                         persist across ticks before it re-homes minds (#438: a one-tick \
                         bogus-sample fallback once served citizens a 0.5B)",
                    );
                    return None;
                }
                ModelChangeGate::Commit { streak } => {
                    continuum_base::probe!(
                        target: "continuum_core::modules::serving_daemon", class = "serving.reconcile.model",
                        decision = "swapping",
                        live_model = live.active_model.as_deref().unwrap_or("<none>"),
                        plan_model = desired.as_str(),
                        streak = streak as u64,
                        "sustained plan disagreement — committing the model swap",
                    );
                }
                ModelChangeGate::NotAChange => {}
            }
        }

        self.launch_reconciled_plan(launch::ReconcileLaunch {
            operation,
            planned,
            intent,
            desired,
            served_ctx,
            lanes,
            desired_adapters,
            desired_adapter_paths,
        })
    }
}
