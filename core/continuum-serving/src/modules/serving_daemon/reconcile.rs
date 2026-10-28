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

    pub(super) fn reset_rehome_streak(&self) {
        self.rehome_streak.store(0, Ordering::Relaxed);

        self.rehome_plan_high.store(0, Ordering::Relaxed);
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
                self.reset_rehome_streak();
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
            // Already serving the right model AND the right genome set → no
            // relaunch. The genome comparison is what makes a freshly-trained gene
            // take effect: same model id + new gene = set change = relaunch.
            if live.ready
                && live.active_model.as_deref() == Some(desired.as_str())
                && live.adapters == desired_adapter_paths
            {
                // …UNLESS the running server's per-slot window froze at HALF or
                // less of what the current plan affords. A lane spawned under
                // transient memory pressure (a benchmark server, a build) keeps
                // its starved window forever otherwise — glass-boxed 2026-07-10:
                // 14,700 recomputes wandering 3.6k↔22k while the living lane
                // stayed frozen at 3.8k and the room degenerated into a greeting
                // loop. 2× is deliberate hysteresis (doubling rule): the plan
                // breathes with every consumer, and a relaunch kills in-flight
                // turns, so only a step-change worth of headroom justifies one.
                // One-directional by design — over-served vs a dipped plan is
                // the pressure broker's reclaim problem (#79), not a relaunch.
                // SUSTAINED-DELTA re-home (replaces the single-sample `live * 2 <=
                // plan` ratio; BigMama's call 2026-08-06, she owns the guard).
                //
                // The old ratio was the cheapest thing that could not thrash against a
                // plan derived from FREE MEMORY, which wandered 3.6k<->22k — its own
                // comment says "the plan breathes with every consumer", and that was
                // true. Demand-derived planning made the plan a peak-tracking
                // MEASUREMENT instead, and against a stable signal a 2x bar strands
                // everything in the 50-99% band forever: measured live, plan 26,323 vs
                // lane 16,384 (62%) held indefinitely while every persona's binding
                // read the stale window.
                //
                // TWO independent guards, deliberately not collapsed into one
                // predicate:
                //
                //   1. SUSTAINED DELTA — is the gain REAL? The plan must exceed the
                //      lane by [`REHOME_MIN_GAIN_PCT`] for [`REHOME_SUSTAINED_TICKS`]
                //      consecutive ticks. A dip RESETS the streak, so jitter can never
                //      accumulate into a relaunch.
                //   2. COOLDOWN — may we PAY for it yet? At most one re-home per
                //      [`REHOME_COOLDOWN_TICKS`], checked separately and answered
                //      first. A relaunch kills every in-flight turn on the lane, so
                //      the rate limit must hold even when the evidence is perfect.
                //
                // Conflating them is how a guard ends up firing on a burst of good
                // evidence. (BigMama's spec 2026-08-06 — she owns this guard; the
                // outage it came from is hers.)
                // A LAUNCH SETTLES AFTER IT IS READY, NOT AFTER IT WAS SPAWNED (2026-09-19
                // 23:13–23:15Z, the M5): the spawn arm charged the cooldown at launch, the
                // OLD lane kept serving through the 27B's ~90 s cold load, so every tick of
                // the cooldown was spent before the new lane answered — it came ready with
                // zero settle left. Its predecessor's freed pages read as a 32 GB budget for
                // one minute, the plan said 3 lanes, and a second relaunch fired 15 s after
                // the first came ready: 12 settles in the hour, acts 5. The ready stamp is
                // the edge that matters: a stamp this reconcile has not settled on re-arms
                // the full cooldown (and the streak), so the memory picture the next
                // evidence is read from is the one this lane made, not the one it inherited.
                if ready_edge(
                    self.settled_ready_at_ms.load(Ordering::Relaxed),
                    live.ready_verified_at_ms,
                ) {
                    self.settled_ready_at_ms
                        .store(live.ready_verified_at_ms.unwrap_or(0), Ordering::Relaxed); // JUSTIFIED unwrap_or: `ready_edge` is true only for Some
                    self.rehome_cooldown
                        .store(REHOME_COOLDOWN_TICKS, Ordering::Relaxed);
                    self.reset_rehome_streak();
                    continuum_base::probe!(
                        target: "continuum_core::modules::serving_daemon", class = "serving.reconcile.window",
                        decision = "ready-settling",
                        live_window = live.served_context_window,
                        live_lanes = live.lanes,
                        cooling = REHOME_COOLDOWN_TICKS as u64,
                        "the lane just came ready — it settles for the full cooldown from HERE before any re-home evidence counts"
                    );
                }
                let cooling = self.rehome_cooldown.load(Ordering::Relaxed);
                if cooling > 0 {
                    self.rehome_cooldown.store(cooling - 1, Ordering::Relaxed);
                    // Lose the streak while cooling rather than banking it: otherwise
                    // the instant the cooldown lapses we relaunch on evidence gathered
                    // a minute and a half ago. Re-proving costs 3 ticks and keeps
                    // "sustained" meaning sustained-NOW.
                    self.reset_rehome_streak();
                }
                // Settlement records an owned candidate. Only terminal usage from this
                // exact generation promotes its cache preference; neither transition
                // constrains the next plan's lanes or current workload requirement.
                let settled_now = settles_this_tick(cooling, live.ready, live.lanes);
                if let Some(model) = live.active_model.as_deref() {
                    let endpoint = crate::ai::openai_endpoints::OpenAiBase::new(&live.base_url);
                    if let Some(geometry) = crate::inference::slots::directory()
                        .endpoint(endpoint.root())
                        .served_geometry()
                        .filter(|g| {
                            g.model_id == model
                                && g.per_slot_window == live.served_context_window
                                && g.lanes == live.lanes
                        })
                    {
                        let observed = crate::inference::resource_admission::PromptSample::geometry_median(
                            &(self.leased_in_sent)(),
                            endpoint.root(),
                            &geometry,
                        );
                        crate::inference::served_window_store::record_reconciled(
                            geometry,
                            settled_now,
                            observed,
                        );
                    }
                    if settled_now {
                        // A settled geometry is lanes the hour served: fold it into the
                        // health window's maximum (the lane-bound rest stands on that).
                        crate::cognition::health_ledger::note_lanes(live.lanes as u64);
                        continuum_base::probe!(
                            target: "continuum_core::modules::serving_daemon", class = "serving.geometry.settled",
                            model,
                            per_slot_window = live.served_context_window as u64,
                            lanes = live.lanes as u64,
                            "the launch settled — same-generation successful usage may record its cache preference"
                        );
                    }
                }
                let (live_space, plan_space, gain, worth_it) =
                    rehome_gain_evidence(live.served_context_window, live.lanes, served_ctx, lanes);
                let short_of_slots = roster_short_of_slots(live.lanes, lanes);
                // THE CACHE HOLDS NO STATE (Fable and BigMama, 2026-10-11): `--cache-ram` is fixed
                // at launch, so a lane launched under a cache smaller than ONE measured state can
                // keep nothing, and every switch between minds is a full re-prefill until it
                // relaunches (the 5090: 4,288 MiB against ~5.5 GB states, 114 skips, 76% of
                // restores cold). That, and only that, is evidence for a re-home on cache size;
                // a cache that holds at least one state waits for the next relaunch. It earns the
                // same sustained streak and cooldown as the window evidence, and every hold above
                // (resident training, an eval) still defers it.
                let measured_state = crate::inference::slots::measured_state_bytes(
                    &crate::inference::llama_server::kv_page_dir(desired.as_str()),
                );
                let cache_holds_no_state =
                    crate::inference::lane_args::launched_cache_holds_no_state(
                        live.host_prompt_cache_mib,
                        measured_state,
                    ) && {
                        // what a relaunch would launch with: computed only in this rare case
                        let would_launch_mib = (self.model_resolver)(&desired)
                            .map(|m| {
                                derived_prompt_cache_mib(
                                    &desired,
                                    footprint_for(&m).as_ref(),
                                    served_ctx,
                                    lanes,
                                    self.system.memory().total_bytes,
                                    self.system.memory().available_bytes,
                                    self.system.gpu_memory_mode(),
                                )
                                .mib
                            })
                            .unwrap_or(0); // unwrap_or: an unresolvable model would launch nothing better, so no relaunch for size
                        crate::inference::lane_args::cache_relaunch_needed(
                            live.host_prompt_cache_mib,
                            would_launch_mib,
                            measured_state,
                        )
                    };
                let worth_it = worth_it || short_of_slots || cache_holds_no_state;
                // A plan may shrink while the host pages out; it never GROWS into it
                // (card 628dc958: six relaunches in 75 min at HIGH with swap climbing,
                // each grow feeding on the memory its own relaunch had just freed). The
                // gate is a measured DELTA, not a level: swap rising since the last
                // decision means the box is short of memory right now, whatever
                // `available` reads mid-relaunch. Flat swap lets the grow through — the
                // swapped pages are already subtracted from the budget it was planned in.
                let swap_now = self.system.snapshot().memory.swap_used_bytes;
                let swap_prev = self.last_swap_used.swap(swap_now, Ordering::Relaxed);
                if grow_refused_while_paging(live_space, plan_space, swap_prev, swap_now) {
                    self.reset_rehome_streak();
                    continuum_base::probe!(
                        target: "continuum_core::modules::serving_daemon", class = "serving.reconcile.window",
                        decision = "refused_while_paging",
                        swap_prev_mb = swap_prev >> 20,
                        swap_now_mb = swap_now >> 20,
                        live_window = live.served_context_window,
                        plan_window = served_ctx,
                        live_lanes = live.lanes,
                        plan_lanes = lanes,
                        shortfall = gain,
                        "the plan wants more memory than the lane holds while the host is \
                         paging out — a grow here feeds on its own relaunch; shrinks flow, \
                         and the grow fires the tick swap stops climbing",
                    );
                    self.acknowledge_verified_target(&live, planned.intent_revision);
                    return None;
                }
                // A STILL-CLIMBING plan is not settled (2026-09-02, the boot
                // staircase): personas register footprints serially at boot,
                // demand climbs in 15%+ stairs, and each stair "sustained" for
                // 3 ticks — SIXTEEN model loads in one lane's log, each paying
                // the full readiness ladder (the real ~15-minute boot, which
                // was never model load: the engine loads in 1.7–6s). Sustained
                // now ALSO requires the plan target itself to have stopped
                // moving: growth resets the streak, so a monotone climb
                // coalesces into ONE relaunch at its top. Steady state is
                // unchanged — a stable plan is flat by definition.
                // Flat = not a NEW HIGH for this streak: the first observation sets the
                // high, a climb past it resets the streak (the boot staircase), and a
                // plan moving below it (alternating, settling) keeps counting.
                let high = self.rehome_plan_high.load(Ordering::Relaxed);
                let plan_flat = high == 0 || plan_space <= high;
                if plan_space > high {
                    self.rehome_plan_high.store(plan_space, Ordering::Relaxed);
                }
                let streak = if worth_it && cooling == 0 && plan_flat {
                    self.rehome_streak
                        .fetch_add(1, Ordering::Relaxed)
                        .saturating_add(1)
                } else {
                    // Not merely "don't count" — RESET. One tick that does not want
                    // the bigger window is enough to prove the demand was not
                    // sustained, and a streak that survives dips is a streak that
                    // eventually fires on noise. A new high starts the next streak from
                    // itself; any other reason starts it from nothing.
                    self.rehome_streak.store(0, Ordering::Relaxed);
                    if plan_flat {
                        self.rehome_plan_high.store(0, Ordering::Relaxed);
                    }
                    0
                };
                // Fire only on a tick whose plan IS the high of the streak's evidence (BigMama on
                // #4851): the relaunch takes this tick's plan, so firing on a low tick of an
                // alternating plan would land the lane at the low value (33k under Kimi's 52k) and
                // re-home again; launching the remembered high instead would use a budget this
                // tick did not plan. The high is taken over the last REHOME_SUSTAINED_TICKS plans,
                // the evidence the streak itself needs, NOT over the whole streak: a high recorded
                // once, while a relaunch was still freeing memory (Kimi's node 2026-10-15,
                // 109,768, then a plan settled at 33k-87k), made this unreachable, and the lane
                // served a 6,144 window until an operator relaunched it (card 5bac4968).
                let at_high = at_recent_plan_high(&mut self.rehome_recent_plans.lock(), plan_space);
                let starved = streak >= REHOME_SUSTAINED_TICKS && cooling == 0 && at_high;
                // NOTE: the guards are re-armed at the FIRE point below, not here —
                // `starved` only means the evidence qualifies. A later suppression
                // (an eval holding the lane steady) still returns without relaunching,
                // and charging a cooldown for a relaunch that never happened would
                // rate-limit us out of the one we actually owe.
                if !starved {
                    // A lane running BELOW the plan but above the 2x bar declines to
                    // grow — silently, until this probe. That silence is the defect:
                    // measured 2026-08-06 the plan sat stable at 26,323 while the
                    // live lane served 16,384 (62% — comfortably above half), every
                    // persona's binding read the stale 16,384, and nothing anywhere
                    // said so. It looked like the demand-derived window had "grown
                    // then reverted"; it had never taken hold, and the only thing
                    // that ever applied a grown plan was a reboot.
                    //
                    // The 2x rule itself is NOT being changed here — it came from a
                    // real outage (a lane frozen at 3.8k while plans wandered
                    // 3.6k<->22k) and a relaunch kills in-flight turns
                    // ([[never-thrash-sticky-hysteresis-on-every-lane]]). But its
                    // stated premise is that "the plan breathes with every consumer",
                    // and demand-derived planning made the plan STABLE, so against it
                    // the bar now strands a third of the window indefinitely. That is
                    // a design call on a guard written from an incident, so it is
                    // surfaced for the humans who own that history rather than
                    // quietly re-tuned by me (#332/#333).
                    // CADENCE, not content. This probe exists because the SILENCE was
                    // the defect, so it must keep speaking — but at one line per 5 s
                    // tick it becomes the defect at the other end: measured on BigMama
                    // 2026-09-05, 192 consecutive identical copies, and a persona's
                    // `first_token` had to be dug out from underneath them. A chatty
                    // floor evicts the signal from every bounded view exactly when the
                    // lane is interesting — the same class that buried a wedge
                    // declaration and produced a false retraction the same night.
                    //
                    // So: every ENTRY into the declined state is logged (streak == 1),
                    // and it keeps saying so once a minute after that. Nothing is
                    // suppressed that a reader needs — `decline_ticks` carries the
                    // duration, so the 47th copy adds no information the 1st lacked.
                    // (The first version of this comment claimed `streak` carried it.
                    // It does not: `streak` is pinned at 0 throughout the declined
                    // state, which is also why that version did not throttle.)
                    // The 2x rule and this decision are untouched; only how often the
                    // same unchanged verdict repeats itself.
                    let over_knee = lane_over_its_knee(
                        live.served_context_window,
                        live.lanes,
                        served_ctx,
                        lanes,
                    );
                    let declined = live_space < plan_space || over_knee || short_of_slots;
                    let decline_ticks = if declined {
                        self.decline_log_ticks
                            .fetch_add(1, Ordering::Relaxed)
                            .saturating_add(1)
                    } else {
                        self.decline_log_ticks.store(0, Ordering::Relaxed);
                        0
                    };
                    if declined && decline_should_log(decline_ticks) {
                        continuum_base::probe!(
                            target: "continuum_core::modules::serving_daemon", class = "serving.reconcile.window",
                            decision = "declined",
                            live_window = live.served_context_window,
                            plan_window = served_ctx,
                            live_lanes = live.lanes,
                            plan_lanes = lanes,
                            shortfall = gain,
                            over_knee,
                            streak,
                            decline_ticks,
                            needs_streak = REHOME_SUSTAINED_TICKS,
                            min_gain_pct = REHOME_MIN_GAIN_PCT,
                            cooling,
                            "lane is serving BELOW plan (or OVER its decode knee) and has not yet earned a re-home: the evidence must persist, not just appear",
                        );
                    }
                    self.acknowledge_verified_target(&live, planned.intent_revision);
                    return None;
                }
                // A living-persona eval is a co-tenant decode slot on THIS lane
                // (`ShareLane`). Growing its window means a relaunch, and a relaunch
                // connection-refuses the exam's in-flight generations (hard-rs 0/8,
                // 2026-07-20). The grow-back is OPTIONAL headroom; the exam is not.
                // Hold the lane steady until the eval drops its guard — a model/genome
                // change or a pressure shrink above still runs (this only gates the
                // starved GROW re-home). [[benchmark-is-a-governor-preemption-lease]]
                let held_ticks = if serving_held_steady() {
                    self.rehome_held_ticks
                        .fetch_add(1, Ordering::Relaxed)
                        .saturating_add(1)
                } else {
                    self.rehome_held_ticks.store(0, Ordering::Relaxed);
                    0
                };
                if serving_held_steady()
                    && !hold_overridden_by_shortfall(live_space, gain)
                    && held_ticks < REHOME_HOLD_DEFER_TICKS
                {
                    continuum_base::probe!(
                        target: "continuum_core::modules::serving_daemon", class = "serving.reconcile.window",
                        decision = "declined",
                        live_window = live.served_context_window,
                        plan_window = served_ctx,
                        live_lanes = live.lanes,
                        plan_lanes = lanes,
                        shortfall = gain,
                        streak,
                        needs_streak = REHOME_SUSTAINED_TICKS,
                        min_gain_pct = REHOME_MIN_GAIN_PCT,
                        cooling,
                        "grow-back re-home suppressed: an eval holds the lane steady (co-tenant slot, no relaunch)",
                    );
                    // Streak deliberately NOT reset: the demand is genuinely sustained,
                    // the exam is simply first in line. When it drops its lease the
                    // re-home fires on the next tick instead of re-proving from zero.
                    self.acknowledge_verified_target(&live, planned.intent_revision);
                    return None;
                }
                // SYMMETRIC RECEIPT (BigMama's requirement, 2026-08-06): the defect was
                // that DECLINING was silent. A fix that makes RELAUNCHING silent has
                // moved the silence, not removed it. Same probe class, same fields, both
                // outcomes — so one query over `serving.reconcile.window` reads the
                // lane's whole decision history, not half of it.
                if serving_held_steady() {
                    self.rehome_held_ticks.store(0, Ordering::Relaxed);
                    continuum_base::probe!(
                        target: "continuum_core::modules::serving_daemon", class = "serving.reconcile.window",
                        decision = "re-homing_over_hold",
                        live_window = live.served_context_window,
                        plan_window = served_ctx,
                        live_lanes = live.lanes,
                        plan_lanes = lanes,
                        shortfall = gain,
                        held_ticks,
                        hold_override_pct = REHOME_HOLD_OVERRIDE_PCT,
                        hold_defer_ticks = REHOME_HOLD_DEFER_TICKS,
                        "a steady hold delays a re-home, never denies it — re-homing over the hold; the held work retries its flickered act",
                    );
                }
                continuum_base::probe!(
                    target: "continuum_core::modules::serving_daemon", class = "serving.reconcile.window",
                    decision = "re-homing",
                    live_window = live.served_context_window,
                    plan_window = served_ctx,
                    live_lanes = live.lanes,
                    plan_lanes = lanes,
                    shortfall = gain,
                    streak,
                    needs_streak = REHOME_SUSTAINED_TICKS,
                    min_gain_pct = REHOME_MIN_GAIN_PCT,
                    // Always 0 here (a re-home cannot fire while cooling) — carried
                    // anyway so BOTH decisions emit the IDENTICAL field set and one
                    // query over the class gets a uniform schema instead of a shape
                    // that changes with the outcome. Symmetry is the point.
                    cooling,
                    cache_holds_no_state,
                    "re-homing lane: the plan exceeded it by a real margin, or its launched cache holds no measured state, for a sustained run of ticks",
                );
                // Both guards re-armed at the moment we actually commit: the next
                // re-home must earn a fresh streak AND outlast a fresh cooldown.
                self.reset_rehome_streak();
                self.rehome_cooldown
                    .store(REHOME_COOLDOWN_TICKS, Ordering::Relaxed);
            }
        }

        // L10 (#438, live 2026-08-15 14:13): a MODEL CHANGE on a READY lane must earn
        // the same sustained streak the window re-home above already requires. The
        // asymmetry was the bug: growing a window (mild) needed REHOME_SUSTAINED_TICKS
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
                REHOME_SUSTAINED_TICKS,
            );
            match verdict {
                ModelChangeGate::Defer { streak } => {
                    continuum_base::probe!(
                        target: "continuum_core::modules::serving_daemon", class = "serving.reconcile.model",
                        decision = "deferred",
                        live_model = live.active_model.as_deref().unwrap_or("<none>"),
                        plan_model = desired.as_str(),
                        streak = streak as u64,
                        needs_streak = REHOME_SUSTAINED_TICKS as u64,
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
