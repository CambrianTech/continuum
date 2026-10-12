//! RECONCILE: driving the live engine toward the plan, and naming what blocks it.
//! Split out of serving_daemon.rs (card dbc9711c, BigMama's step 6).

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
        let now = crate::modules::chat::now_ms();
        let said = self.plan_none_said_ms.load(Ordering::Acquire);
        if now.saturating_sub(said) < RECONCILE_BUSY_SAY_EVERY.as_millis() as u64 {
            return;
        }
        self.plan_none_said_ms.store(now, Ordering::Release);
        crate::probe!(
            class = "serving.plan.none_with_live_lane",
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
        let now = crate::modules::chat::now_ms();
        let age = Duration::from_millis(now.saturating_sub(started));
        let said = self.reconcile_busy_said_ms.load(Ordering::Acquire);
        if now.saturating_sub(said) < RECONCILE_BUSY_SAY_EVERY.as_millis() as u64 {
            return;
        }
        self.reconcile_busy_said_ms.store(now, Ordering::Release);
        let step_kind = ReconcileStep::from_u8(self.reconcile_step.load(Ordering::Acquire));
        let step = step_kind.name();
        if reconcile_wedged_at(step_kind, age) {
            crate::probe!(
                class = "serving.reconcile.wedged",
                skipped,
                step,
                age_ms = age.as_millis() as u64,
                bound_ms = RECONCILE_WEDGE_BOUND.as_millis() as u64,
                "a reconcile operation has outlived the bound a healthy one can hold — the \
                 daemon is skipping every tick and plan on its gate; sample the core and read \
                 this step's await before touching the engine"
            );
        } else {
            crate::probe!(
                class = "serving.reconcile.busy",
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
                    _ => Err("no live lane record or home to record the interruption in".to_string()),
                };
                crate::probe!(
                    class = "serving.reconcile.resident_engine_replaced",
                    occupancy = %format!("{occupancy:?}"),
                    told = %format!("{told:?}"),
                    "an emergency replaces an engine that hosts live work; the work was told why first"
                );
            } else {
                self.model_change_streak.store(0, Ordering::Relaxed);
                self.reset_rehome_streak();
                self.downshift_streak.store(0, Ordering::Relaxed);
                *self.pending_model_change.lock().unwrap_or_else(|p| p.into_inner()) = None; // unwrap_or_else: a poisoned streak cell is still cleared
                static LAST_HELD: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                let now = crate::persona::trace::now_ms();
                // one row a minute while held, not one a tick
                if now.saturating_sub(LAST_HELD.load(Ordering::Relaxed)) >= 60_000 {
                    LAST_HELD.store(now, Ordering::Relaxed);
                    crate::probe!(
                        class = "serving.reconcile.held_for_resident_work",
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
                        Err(e) => tracing::warn!(error = %e, "empty-plan retirement failed"),
                    }
                }));
            }
        };

        // The trained genome layers registered for this base model — which genes
        // are loadable into the serving catalog (the `--lora` set). Read from the
        // producer-written manifest, keyed by the CONTINUUM base id (the HF base
        // in each gene's PEFT config never matches the served id, so a directory
        // scan can't make this association — see forge::adapter_manifest). An
        // unreadable/corrupt manifest degrades to base-only HERE (logged loud) so
        // the reconcile tick never panics; a MISSING gene file still fails loud at
        // spawn (AdapterNotFound). Empty = base model only, the legitimate state.
        let desired_adapters: Vec<AdapterEntry> = match crate::forge::adapter_manifest::load() {
            Ok(all) => crate::forge::adapter_manifest::for_base(&all, &desired)
                .into_iter()
                .map(|a| AdapterEntry {
                    alias: a.alias,
                    path: a.path,
                })
                .collect(),
            Err(e) => {
                crate::probe!(
                    class = "serving.adapters",
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
                if ready_edge(self.settled_ready_at_ms.load(Ordering::Relaxed), live.ready_verified_at_ms) {
                    self.settled_ready_at_ms
                        .store(live.ready_verified_at_ms.unwrap_or(0), Ordering::Relaxed); // JUSTIFIED unwrap_or: `ready_edge` is true only for Some
                    self.rehome_cooldown.store(REHOME_COOLDOWN_TICKS, Ordering::Relaxed);
                    self.reset_rehome_streak();
                    crate::probe!(
                        class = "serving.reconcile.window",
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
                // The geometry a cold boot plans first is the last SETTLED one — the lane
                // that outlived its own spawn cooldown without a re-home — never the last
                // SPAWNED one. Measured 2026-09-19 on the M5: the 12:19Z Eco relaunch spawned
                // `--parallel 1`, the spawn wrote `lanes: 1` to the store, and every boot after
                // (13:2xZ, 13:36Z) planned `min(fit, 1)` = one lane on a box whose fit held two
                // — a shrink-only ratchet across boots. Saving on settle makes "the first
                // launch is the steady one" (the boot probe's own words) true.
                if settles_this_tick(cooling, live.ready, live.lanes) {
                    if let Some(model) = live.active_model.as_deref() {
                        crate::modules::served_window_store::save(model, live.served_context_window, live.lanes);
                        // A settled geometry is lanes the hour served: fold it into the
                        // health window's maximum (the lane-bound rest stands on that).
                        crate::modules::citizen_health::note_lanes(live.lanes as u64);
                        crate::probe!(
                            class = "serving.geometry.settled",
                            model,
                            per_slot_window = live.served_context_window as u64,
                            lanes = live.lanes as u64,
                            "the launch outlived its spawn cooldown without a re-home — this geometry is what the next boot plans first"
                        );
                    }
                }
                let (live_space, plan_space, gain, worth_it) = rehome_gain_evidence(
                    live.served_context_window,
                    live.lanes,
                    served_ctx,
                    lanes,
                );
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
                    &crate::inference::llama_server::kv_page_dir(desired.as_str(), live.served_context_window),
                );
                let cache_holds_no_state = launched_cache_holds_no_state(live.host_prompt_cache_mib, measured_state)
                    && {
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
                        cache_relaunch_needed(live.host_prompt_cache_mib, would_launch_mib, measured_state)
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
                    crate::probe!(
                        class = "serving.reconcile.window",
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
                // Fire only on a tick whose plan IS the streak's high (BigMama on #4851): the
                // relaunch takes this tick's plan, so firing on a low tick of an alternating
                // plan would land the lane at the low value (33k under Kimi's 52k) and re-home
                // again; launching the remembered high instead would use a budget this tick
                // did not plan. The high tick is both: the best the streak saw, planned now.
                let at_high = plan_space >= self.rehome_plan_high.load(Ordering::Relaxed);
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
                        crate::probe!(
                            class = "serving.reconcile.window",
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
                    self.rehome_held_ticks.fetch_add(1, Ordering::Relaxed).saturating_add(1)
                } else {
                    self.rehome_held_ticks.store(0, Ordering::Relaxed);
                    0
                };
                if serving_held_steady()
                    && !hold_overridden_by_shortfall(live_space, gain)
                    && held_ticks < REHOME_HOLD_DEFER_TICKS
                {
                    crate::probe!(
                        class = "serving.reconcile.window",
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
                    crate::probe!(
                        class = "serving.reconcile.window",
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
                crate::probe!(
                    class = "serving.reconcile.window",
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
                .unwrap_or_else(|p| p.into_inner());  // unwrap_or_else: a poisoned pending-change lock reads its last state, the ledger-lock policy of this module
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
                    crate::probe!(
                        class = "serving.reconcile.model",
                        decision = "deferred",
                        live_model = live.active_model.as_deref().unwrap_or("<none>"),  // unwrap_or: probe label only, "<none>" = no model live
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
                    crate::probe!(
                        class = "serving.reconcile.model",
                        decision = "swapping",
                        live_model = live.active_model.as_deref().unwrap_or("<none>"),  // unwrap_or: probe label only, "<none>" = no model live
                        plan_model = desired.as_str(),
                        streak = streak as u64,
                        "sustained plan disagreement — committing the model swap",
                    );
                }
                ModelChangeGate::NotAChange => {}
            }
        }

        // #175 sticky window: we're past the no-relaunch guard, so a relaunch WILL
        // happen (a genome page-in / model change). Don't let the startup LoRA-load
        // cascade ratchet the per-slot window DOWN on a same-lane relaunch and strand
        // the personas pinned to the earlier, larger slot — keep the incumbent window
        // when lanes are unchanged (memory-safe; a lane change legitimately resizes KV).
        let served_ctx = sticky_served_window(served_ctx, lanes, &self.serving_tx.borrow());
        // #363 prevention: the WEDGE-HEAL floor. The sticky floor above reads the live
        // snapshot, but declaring a wedge EMPTIES that snapshot first — so the very
        // relaunch the detector triggers used to spawn at whatever the teardown-transient
        // plan said (the dying predecessor still holds its memory when the successor's
        // budget samples free RAM). Floor the successor to the last HEALTHY lane's window
        // when the lane count is unchanged; the predecessor's window is exactly the
        // memory being freed, so the successor inherits it, and the existing overflow
        // protection stays the backstop if the world genuinely shrank.
        let floor_w = self.last_healthy_window.load(Ordering::Relaxed);
        let floor_l = self.last_healthy_lanes.load(Ordering::Relaxed);
        let live_ready = self.serving_tx.borrow().ready;
        let floored = wedge_heal_floor(served_ctx, lanes, live_ready, floor_w, floor_l);
        if floored != served_ctx {
            crate::probe!(
                class = "serving.reconcile.window",
                decision = "wedge-heal-floor",
                live_window = 0u32,
                plan_window = served_ctx,
                floored_to = floored,
                plan_lanes = lanes,
                "spawn-at-transient guarded: successor floored to the last healthy \
                 lane's window instead of a teardown-dip plan (#363)",
            );
        }
        // A KEPT WINDOW MUST STILL FIT. Both laws above keep the BIGGER window on the
        // assumption it was legitimately sized. On 2026-09-17 it was not: a 144,640
        // window sized against a fictional 54 GB budget (the ledger's uncapped add-back)
        // was kept by sticky, then by wedge-heal, across every relaunch onto a 26 GB
        // budget — 31.9 of 32.6 GB VRAM, 44 s prefills, the cache demand it implied.
        // Bound the kept window by what the PHYSICAL budget fits at this lane count
        // (physical, not live: the live budget is exactly the teardown transient the
        // wedge-heal law exists to see through). The plan's own window is always
        // within it, so this touches nothing but a kept fiction.
        let served_ctx = match self
            .catalog
            .snapshot()
            .get(&desired)
            .and_then(|live| footprint_for(&live.model))
            .map(|fp| fp.window_within(self.physical_budget().usable_bytes, lanes))
        {
            Some(fits) if floored > fits.max(crate::inference::planning::MIN_SERVE_CTX) => {
                let bound = fits.max(crate::inference::planning::MIN_SERVE_CTX);
                crate::probe!(
                    class = "serving.reconcile.window",
                    decision = "kept-window-exceeds-physical-fit",
                    kept_window = floored,
                    bound_to = bound,
                    plan_lanes = lanes,
                    physical_usable_gb = (self.physical_budget().usable_bytes / 1_000_000_000),
                    "a kept (sticky / wedge-heal) window does not fit this host's physical \\n                     budget at this lane count — bounded; a kept window is never a licence \\n                     to exceed the card",
                );
                bound
            }
            _ => floored,
        };

        // Resolve the full Model struct ONCE, here, and carry it on the target —
        // no re-fetch downstream ([[pass-the-model-struct-no-param-hell]]). If
        // the registry can't produce the model the plan named, fail loud (empty
        // snapshot) rather than serving something else.
        let resolved = (self.model_resolver)(&desired);
        if self.intent.snapshot().revision != planned.intent_revision {
            return None;
        }
        let Some(model) = resolved else {
            crate::probe!(
                class = "serving.reconcile",
                desired = desired.as_str(),
                "plan named a model the registry can't resolve — publishing degraded snapshot",
            );
            self.set_active_artifact(None);
            if self.serving_tx.borrow().active_model.is_some() {
                // Name the killer on the SNAPSHOT, not just in the log: this is a
                // config/catalog fault an operator must act on, and `empty()` would
                // render it as a plain "nothing served".
                let flipped = ServingSnapshot::degraded(format!(
                    "the plan named model '{desired}', which the registry cannot resolve"
                ));
                Self::emit_serving(self.bus.get(), &flipped);
                let _ = self.serving_tx.send_replace(flipped);
            }
            return None;
        };
        // K3 expert placement: for a MoE model, the ServingExpertPager plans which expert
        // LAYERS fit the governed VRAM budget and attaches them; the launcher `-ot`s the
        // cold complement to CPU. Computed BEFORE the struct literal moves `model`. `None`
        // for a dense model (or before the pager has committed a placement) — served exactly
        // as before, no override.
        let expert_placement = self.compute_expert_placement(&model);
        // Device-fit resident-override (#29): computed from the model's resident
        // (non-expert) footprint vs the governed VRAM budget. `Some(path)` when
        // resident overflows as-shipped and a cached device-fit override fits (the
        // launcher sources resident from it via `LLAMA_RESIDENT_OVERRIDE`); `None`
        // when resident fits natively OR no override is cached yet (resolver #35).
        let resident_override = self.compute_resident_override(&model);
        // RESTORE-ECONOMY 1.b: derive the host prompt cache HERE, where footprint,
        // physical memory and the citizen population are all in hand — computed
        // once per serve, carried on the target so the lane's `--cache-ram` and
        // the governor's host-cache lease read the SAME number (never the old
        // constant the lease had to work around). Spawn-time snapshot only:
        // the value changes at relaunch boundaries, never mid-serve, so there
        // is zero flap surface (Law 2 — capacity follows measurement, structure
        // does not).
        let prompt_cache = derived_prompt_cache_mib(
            &model.id,
            footprint_for(&model).as_ref(),
            served_ctx,
            lanes,
            self.system.memory().total_bytes,
            self.system.memory().available_bytes,
            self.system.gpu_memory_mode(),
        );
        // Kept beside the grant for the divergence receipt below: the predicate that
        // separates "a bit small" from "structurally cannot cache anything".
        let one_conversation_mib = prompt_cache.one_conversation_mib;
        let host_prompt_cache_mib = prompt_cache.mib;
        let target = ServingTarget {
            host_prompt_cache_mib,
            model,
            context_window: served_ctx,
            lanes,
            adapters: desired_adapters,
            // The living persona lane: GPU-resident for throughput (every
            // offloadable layer). [[LanePlacement]].
            placement: crate::inference::llama_server::main_lane_placement(),
            expert_placement,
            resident_override,
            // The MAIN persona lane — the withhold rule's actual subject.
            vision_sidecar: false,
        };

        // One reconcile at a time. If the swap finds `true`, another is already
        // running; skip rather than stack relaunches.
        if self.intent.snapshot().revision != planned.intent_revision {
            return None;
        }

        let active_artifact = self.active_artifact.clone();
        let served_resident = self.served_resident.clone();
        let division = self.division.clone();
        // Consume any pending force-relaunch the liveness heartbeat raised: it means the
        // heartbeat already saw the live lane fail decode, so this reconcile must re-prove
        // decode even on a child we own (else the owned-child trust re-adopts the wedged
        // lane forever, #175). Read+clear here so exactly one reconcile acts on it.
        let force_relaunch = self.force_relaunch.clone();
        let server = self.server.clone();
        let serving_tx = self.serving_tx.clone();
        let verified_target = self.verified_target.clone();
        let intent_owner = self.intent.clone();
        let revision = planned.intent_revision;
        let bus = self.bus.get().cloned();
        let sidecar_slot = self.vision_sidecar.clone();
        let system = self.system.clone();
        // What the sidecar must respect, read once here and moved into the task: the
        // operator's OFF pins (a pinned-off model is never the vision provider either) and
        // the residents' per-lane requirement (a sidecar never holds memory the main lane
        // needs for one real turn). The M5, 2026-09-20 17:49Z: with the 27B pinned off at
        // boot the sidecar admitted a 35B against a 2,048-token main lane's headroom, and
        // when the 27B came back it got what was left — one lane at 2,048.
        let pinned_off: Vec<String> = intent.suppressed.iter().cloned().collect();
        let persona_floor: Option<u32> = self.serving_demand().typical_prompt_floor();
        let last_healthy_window = self.last_healthy_window.clone();
        let last_healthy_lanes = self.last_healthy_lanes.clone();
        let rehome_cooldown = self.rehome_cooldown.clone();
        // ARM the discrete footprint arm's baseline: every tick until the lane is ready
        // lowers it to the sampled trough (see the field doc — a single read here sees
        // the dying predecessor).
        self.spawn_baseline_vram.store(u64::MAX, Ordering::Relaxed);
        Some(tokio::spawn(async move {
            let operation = operation;
            let current = || intent_owner.snapshot().revision == revision;
            if !current() {
                return;
            }
            let mut force_probe = ForcedProbe::claim(force_relaunch);
            let admit = || {
                // This short borrow is the linearization point with intent writers.
                // No await: once admitted, a physical transition is allowed to settle.
                let intent = intent_owner.state.borrow();
                if intent.revision != revision {
                    return false;
                }
                // Division reward attribution (two-speed honesty): record which resident the
                // spawn ACTUALLY loads so measured tok/s credits the serving tier, never the
                // bandit's latest unlaunched choice.
                if let Ok(mut g) = served_resident.lock() {
                    *g = target.resident_override.clone();
                }
                if let Ok(mut d) = division.lock() {
                    if let Some(act) = d.as_mut() {
                        act.set_served_resident(target.resident_override.as_deref());
                    }
                }
                // #302 invariant 1: mark the model's artifact ACTIVE before any spawn
                // touches it — the NvmeServingTierPool must never migrate the GGUF the
                // engine is loading or serving. Model change swaps the registration.
                Self::set_active_artifact_in(
                    &active_artifact,
                    target.model.gguf_local_path.clone(),
                );

                drop(intent);
                Self::publish_lifecycle_admission(
                    &serving_tx,
                    &verified_target,
                    bus.as_ref(),
                    Some(&desired),
                );
                true
            };
            operation.step(ReconcileStep::Ensure);
            let outcome = ensure_model_serving_if_current(
                server.as_ref(),
                &target,
                force_probe.pending,
                &admit,
            )
            .await;
            if matches!(outcome, EnsureOutcome::Superseded)
                || (matches!(outcome, EnsureOutcome::AlreadyServing) && !current())
            {
                return;
            }
            force_probe.pending = false;
            // EVERY LAUNCH GETS THE SETTLE WINDOW A RE-HOME GETS (9/18, the mirror): the
            // cooldown was armed only at the re-home fire point, so a boot's first spawn
            // could be re-homed 40 s later by its own load transient (the weights counted
            // as external before the ledger credited them — 8 lanes declined at tick 1,
            // re-homed at tick 3). A lane just launched is not evidence against itself.
            if matches!(outcome, EnsureOutcome::Spawned { .. }) {
                rehome_cooldown.store(REHOME_COOLDOWN_TICKS, Ordering::Relaxed);
                crate::probe!(
                    class = "serving.reconcile.window",
                    decision = "spawn-settling",
                    plan_lanes = target.lanes,
                    plan_window = target.context_window,
                    cooling = REHOME_COOLDOWN_TICKS as u64,
                    "a fresh launch settles for the same cooldown a re-home gets before any re-home evidence counts"
                );
            }
            // For a ready outcome, read the REAL per-slot window the running
            // server serves from its own `/props` — the authoritative model
            // metadata every persona budgets its prompt to. llama.cpp pads the
            // launch per-slot `-c/--parallel` window UP to a 256-multiple
            // internally, and the planner RE-computes its own window each tick
            // against live memory, drifting ABOVE the running server's frozen
            // slot; budgeting to that drifted value overflows the slot (500
            // "Compute error"). So we pass the process's own truth through, not
            // the plan's `served_ctx`. A read failure on a ready server yields 0,
            // which `snapshot_from_outcome` turns into "publish the gap" (not a
            // ready snapshot with a guessed window) — it self-heals next tick.
            let served_window = match &outcome {
                EnsureOutcome::AlreadyServing | EnsureOutcome::Spawned { .. } => {
                    operation.step(ReconcileStep::VerifyWindow);
                    match server.served_context_window().await {
                        Ok(n) => n,
                        Err(e) => {
                            crate::probe!(
                                class = "serving.reconcile",
                                desired = desired.as_str(),
                                error = %e,
                                "server ready but /props served window unreadable — \
                                 publishing the gap (no guessed window; retries next tick)",
                            );
                            0
                        }
                    }
                }
                EnsureOutcome::Degraded { .. } | EnsureOutcome::Superseded => 0,
            };
            // THE LANE COUNT IS THE ENGINE'S TOO. The ensure path asks `/props` for
            // `total_slots` (2026-08-19: "ask the lane, not our memory of it") and
            // adopts a running server whose slots meet or EXCEED the target — but the
            // snapshot then published the TARGET's count. Measured on the M5 2026-09-20
            // 07:52Z: the plan fell to 1 lane, the 2-slot engine was adopted, and the
            // snapshot said `lanes: 1` — the footprint measurement halved its divisor
            // (per-token 162k → 262k), the ambient budget (lanes − 1) went to zero, the
            // health line said "1 now", the roster bound read one warm slot. Same
            // policy as the window above: the process's own truth, the target's count
            // only when the engine names none.
            let served_lanes = match &outcome {
                EnsureOutcome::AlreadyServing | EnsureOutcome::Spawned { .. } => {
                    operation.step(ReconcileStep::VerifyLanes);
                    match server.served_lanes().await {
                        Ok(n) => published_lanes(Some(n), target.lanes),
                        Err(e) => {
                            crate::probe!(
                                class = "serving.reconcile",
                                desired = desired.as_str(),
                                error = %e,
                                target_lanes = target.lanes as u64,
                                "server ready but /props total_slots unreadable — publishing \
                                 the target's lane count until the next tick reads the engine's",
                            );
                            published_lanes(None, target.lanes)
                        }
                    }
                }
                EnsureOutcome::Degraded { .. } | EnsureOutcome::Superseded => 0,
            };
            // The grant the engine was LAUNCHED with: the spawn fact on the control, or — for
            // a lane this core did not spawn (ADOPTED; every deploy leaves the lane up for
            // the next core, so this is the common case, not the rare one) — read off the
            // engine's own argv. 0 only when neither can say (the measurement then withholds).
            let launched_cache_mib = server
                .launched_prompt_cache_mib()
                .or_else(adopted_prompt_cache_mib)
                .unwrap_or(0);  // unwrap_or: neither the launch nor the adopted lane names a prompt cache, so it is sized below rather than an unknown trusted
            // THE GRANT THE ENGINE HOLDS vs THE ONE TODAY'S DECISION WANTS (2026-09-21).
            // `--cache-ram` is passed ONCE at spawn and is immutable for the engine's life,
            // while the derived target is re-computed every tick — so a transient trough at
            // the instant of launch becomes PERMANENT until something unrelated relaunches
            // the lane. Measured on the M5: pid 33898 spawned 05:13:22Z with 480 MiB (a
            // 3-lane/235k-ctx moment left almost nothing affordable) and still held it three
            // hours later while the decision had recovered to 2,517 MiB. At q8_0's 32 KiB per
            // token that is 15,360 tokens of cache against deliberation prompts of
            // 22,612-41,547 — under ONE conversation, so prefix reuse was 0%, every turn paid
            // full prefill, and a peer's hour line read `prefix reuse 0% (4k cached / 574k
            // prefilled)` with directed waits p50 241s / p90 501s.
            //
            // `serving.prompt_cache.launch` honestly records `applied_mib="unobserved"` — the
            // engine API does not expose what it applied, and inventing a readback would be
            // worse than saying so. But BOTH numbers in this divergence are ours: the grant we
            // passed and the target we now want. Nothing compared them, so the gap was
            // invisible until someone read the engine's argv by hand.
            //
            // TWO FACTS, NOT ONE (Cormac on #4298). `wanted > held` is ANY shortfall, 1 MiB
            // included; `held < one conversation` is the structural case where reuse is not
            // poor but IMPOSSIBLE. Reporting the first while claiming the second is the
            // one-value-two-meanings shape this probe exists to expose, so both are carried
            // and `below_one_conversation` names which you are looking at. The threshold comes
            // from `lane_args::one_conversation_bytes` via the decision — the SAME function
            // the spawn floor uses, so the relaunch check (card ab27b914) inherits the exact
            // predicate the test pins, rather than growing a third derivation.
            //
            // SAID ONCE PER STATE, never per tick. This sits on the 5-second ready path, and
            // the M5 gap lasted three hours — ~2,160 identical lines, a rising count that is
            // not rising evidence, drowning the ledger this card wants read. Same shape as the
            // sizing receipt's own `LAST_SPOKEN` cell: speak when the divergence APPEARS, when
            // either number MOVES, and once when it CLOSES.
            {
                let held = launched_cache_mib;
                let wanted = target.host_prompt_cache_mib;
                let diverged = held > 0 && wanted > held;
                // Keyed by MODEL as well as the numbers, exactly like the sizing receipt's
                // own cell: a model switch that happens to produce the same pair is a
                // different fact and still owes a receipt.
                let state = diverged
                    .then(|| (target.model_id().to_owned(), held, wanted, one_conversation_mib));
                static LAST_DIVERGENCE: parking_lot::Mutex<Option<(String, u32, u32, u32)>> =
                    parking_lot::Mutex::new(None);
                // `state` carries a String, so keep the copy we report from: the cell takes
                // ownership of the new state and hands back the old one.
                let speaking = state.clone();
                let previous = say_on_change(&mut *LAST_DIVERGENCE.lock(), state);
                if let Some(previous) = previous {
                    match speaking {
                        Some((_, held, wanted, one_conversation)) => crate::probe!(
                            class = "serving.prompt_cache.divergence",
                            model = target.model_id(),
                            held_mib = held as u64,
                            wanted_mib = wanted as u64,
                            shortfall_mib = wanted.saturating_sub(held) as u64,
                            one_conversation_mib = one_conversation as u64,
                            below_one_conversation = one_conversation > 0 && held < one_conversation,
                            served_window = served_window as u64,
                            served_lanes = served_lanes as u64,
                            "the running engine holds LESS host prompt cache than today's \
                             decision wants — `--cache-ram` is fixed at spawn, so this gap \
                             closes only on a relaunch (card ab27b914). Read \
                             `below_one_conversation`: true means the grant cannot retain a \
                             single prefix and reuse is structurally impossible, not merely poor"
                        ),
                        None => crate::probe!(
                            class = "serving.prompt_cache.divergence_closed",
                            model = target.model_id(),
                            held_mib = launched_cache_mib as u64,
                            wanted_mib = target.host_prompt_cache_mib as u64,
                            was_held_mib = previous.as_ref().map(|(_, h, _, _)| *h).unwrap_or(0) as u64, // unwrap_or: no prior state recorded
                            was_wanted_mib = previous.as_ref().map(|(_, _, w, _)| *w).unwrap_or(0) as u64, // unwrap_or: as above
                            "the prompt-cache divergence closed — the engine's grant now meets \
                             today's decision (a relaunch, or the target fell back to it)"
                        ),
                    }
                }
            }
            // #106 vision readiness: for a ready lane, resolve the node's VERIFIED
            // vision endpoint. First the MAIN lane — the row's declared Vision, the
            // resolved mmproj, and the server's own `/props modalities` must all
            // agree. A text-only mind then gets a SIDECAR attempt (a small VL lane
            // beside it, `inference::vision_sidecar`) so personas are not blind just
            // because their mind's model doesn't see. Every gap publishes no
            // endpoint WITH the reason probed loud, so the observe path fails
            // honestly instead of POSTing pixels a text-only lane would drop.
            let vision = match &outcome {
                EnsureOutcome::AlreadyServing | EnsureOutcome::Spawned { .. } if current() => {
                    let declares_vision =
                        target.model.has(crate::model_registry::Capability::Vision);
                    // The sidecar search, ONCE per reconcile — below it names which
                    // row a sidecar would run.
                    let sidecar_rows: Vec<crate::model_registry::types::Model> =
                        crate::model_registry::try_global()
                            .map(|r| r.models().cloned().collect())
                            .unwrap_or_default();  // unwrap_or_default: no model registry means no rows to wear a sidecar from, so no sidecar
                    let sidecar_candidate = crate::inference::vision_sidecar::find_candidate(
                        &sidecar_rows,
                        Some(desired.as_str()),
                        &pinned_off,
                    );
                    let props = match server.multimodal_support().await {
                        Ok(p) => p,
                        Err(e) => {
                            crate::probe!(
                                class = "serving.vision.props_unreadable",
                                desired = desired.as_str(),
                                error = %e,
                                "ready lane but /props modalities unreadable — \
                                 publishing vision_ready=false (unverified ≠ sight)",
                            );
                            None
                        }
                    };
                    // Does the RUNNING lane carry a projector? A FACT, never a
                    // re-derivation of today's intent. Two sources, in order: what THIS
                    // core's spawn passed (recorded on the handle), and for a lane this
                    // core did not spawn — ADOPTED across a reboot — what the server
                    // itself reports on /props. The 2026-09-18 ladder: readiness was fed
                    // "resolved on disk" as if it meant "passed" (a withheld projector
                    // read as a failed load); then the intent (wrong for a lane adopted
                    // at 04:08Z that was spawned without); then the handle's fact with
                    // unknown = "not carried" (wrong at 07:17Z for a lane adopted WITH
                    // its projector: /props said vision=true, readiness said no). The
                    // grow-check in `ensure` already trusts /props for an adopted lane;
                    // readiness now reads the same truth.
                    let main_lane_wears = server.mmproj_on_lane().unwrap_or_else(|| {  // unwrap_or_else: /props silent on the projector, so the catalog decides below
                        props.as_ref().is_some_and(|m| m.vision) // an adopted lane's fact is what it reports
                    });
                    let main_sees = match crate::inference::llama_server::vision_lane_ready(
                        declares_vision,
                        main_lane_wears,
                        props,
                    ) {
                        Ok(ready) => ready,
                        Err(why) => {
                            crate::probe!(
                                class = "serving.vision.not_ready",
                                desired = desired.as_str(),
                                why = why.as_str(),
                                "vision-capable row is serving without verified sight",
                            );
                            false
                        }
                    };
                    if main_sees {
                        // A VL mind: the main lane IS the vision endpoint. Any
                        // sidecar from a previous plan is redundant — drop it
                        // (its Drop kills the child, RAM freed).
                        let mut slot = sidecar_slot.lock().await;
                        if current() {
                            *slot = None;
                        }
                        Some(crate::inference::vision_sidecar::SidecarLane {
                            base_url: serving_v1_url(),
                            model_id: desired.clone(),
                        })
                    } else if persona_floor.is_some_and(|f| served_window > 0 && served_window < f) {
                        // THE MAIN LANE IS BELOW WHAT ITS RESIDENTS NEED: the sidecar's
                        // memory is exactly the window they are missing. Drop it (RAII kills
                        // the child, RAM freed, the next plan grows the lane); it respawns on
                        // the first plan whose headroom is measured against a lane that holds.
                        crate::probe!(
                            class = "serving.vision.sidecar_yields_to_persona_floor",
                            served_window = served_window as u64,
                            persona_floor = persona_floor.unwrap_or(0) as u64, // unwrap_or: guarded by is_some_and
                            "main lane below the residents' requirement — vision sidecar yields its budget to the lane"
                        );
                        let mut slot = sidecar_slot.lock().await;
                        if current() {
                            *slot = None;
                        }
                        None
                    } else if crate::cognition::serving_plan::solve_window_floor() > 0 {
                        // SOLVE REGIME: the eyes yield (2026-08-29, "take every
                        // architectural advantage"). While a pinned solve floor
                        // stands the work is text-only by construction, and the
                        // sidecar's ~9GB is exactly the budget that buys the
                        // SECOND solve lane (measured: 84k×1 afforded, 40448×2
                        // refused by a few GB). Drop it (RAII kills the child,
                        // RAM freed, the next plan sees the room); it respawns
                        // on the first plan after the floor clears — sight
                        // returns with the idle era, like dreams.
                        crate::probe!(
                            class = "serving.vision.sidecar_yields_to_solves",
                            "solve floor standing — vision sidecar yields its budget to a second solve lane"
                        );
                        let mut slot = sidecar_slot.lock().await;
                        if current() {
                            *slot = None;
                        }
                        None
                    } else {
                        use crate::inference::vision_sidecar as sidecar;
                        let candidate = sidecar_candidate;
                        match &candidate {
                            Err(skipped) => {
                                crate::probe!(
                                    class = "serving.vision.sidecar_no_candidate",
                                    skipped = skipped.join("; ").as_str(),
                                    "text-only mind and no on-disk VL row — personas \
                                     have no local vision endpoint (models/pull a \
                                     *-VL-*-GGUF repo to give them eyes)",
                                );
                                None
                            }
                            Ok(cand) => {
                                // Gate on BOTH the live free-memory read (the eval-lane
                                // second-model SIGKILL class) AND the PLANNED headroom
                                // after the live lane's whole working set. The live read
                                // alone admitted a 22 GB sidecar on 2026-09-15 00:17Z: it was
                                // taken inside the main lane's relaunch window, when its 21
                                // GB of weights + 15 GB of KV were momentarily unwired, so
                                // "free" read 22 GB on a box that had 9 GB once both loaded —
                                // 26 GB of swap, decode 6 t/s against a catalog 68. On unified
                                // memory the live lane's device residency IS host RAM, so the
                                // planned term counts the full weights, the KV at the served
                                // window × lanes, the compute reserve and the OS floor.
                                let live_free = system.snapshot().memory.available_bytes;
                                let physical = system.memory().total_bytes;
                                // Headroom AFTER the main lane at the width its residents
                                // NEED, never at a transient starved width: a 2,048-token lane
                                // reads as 39 GB of headroom on a 64 GB box, and a sidecar
                                // admitted on that number is the reason the lane stays 2,048.
                                let planned_headroom = sidecar_planned_headroom_bytes(
                                    &target,
                                    sidecar_window_for_headroom(served_window, persona_floor),
                                    physical,
                                );
                                let free = sidecar_admissible_bytes(live_free, planned_headroom, physical);
                                crate::probe!(
                                    class = "serving.vision.sidecar_headroom",
                                    live_free,
                                    live_free_after_floor = live_free.saturating_sub(host_os_floor_bytes(physical)),
                                    planned_headroom,
                                    admissible = free,
                                    need_bytes = cand.need_bytes,
                                    "sidecar admission reads the tighter of the live free read (minus the OS floor) and the planned headroom"
                                );
                                match sidecar::plan_sidecar(false, Ok(cand), free) {
                                    sidecar::SidecarVerdict::Spawn => {
                                        let mut slot = sidecar_slot.lock().await;
                                        if !current() {
                                            None
                                        } else {
                                            match sidecar::ensure_sidecar(&mut slot, cand).await {
                                                Ok(lane) => {
                                                    crate::probe!(
                                                    class = "serving.vision.sidecar_up",
                                                    model = lane.model_id.as_str(),
                                                    base_url = lane.base_url.as_str(),
                                                    "vision sidecar verified — personas can see",
                                                );
                                                Some(lane)
                                            }
                                            Err(why) => {
                                                crate::probe!(
                                                    class = "serving.vision.sidecar_failed",
                                                    model = cand.model.id.as_str(),
                                                    why = why.as_str(),
                                                    "vision sidecar could not come up — this                                                      candidate is benched for the rest of the                                                      boot; the next reconcile tries the next row",
                                                );
                                                    sidecar::mark_candidate_failed(&cand.model.id);
                                                    None
                                                }
                                            }
                                        }
                                    }
                                    sidecar::SidecarVerdict::NoBudget {
                                        need_bytes,
                                        free_bytes,
                                    } => {
                                        crate::probe!(
                                            class = "serving.vision.sidecar_no_budget",
                                            model = cand.model.id.as_str(),
                                            need_bytes,
                                            free_bytes,
                                            "vision sidecar refused: not enough free host \
                                             memory beside the live lane",
                                        );
                                        None
                                    }
                                    // plan_sidecar was called with
                                    // main_vision_ready=false and Ok(cand), so the
                                    // remaining variants cannot arise; publish no
                                    // endpoint rather than assert in the reconcile.
                                    _ => None,
                                }
                            }
                        }
                    }
                }
                _ => None, // Failed or superseded intent cannot begin sidecar lifecycle work.
            };
            let snapshot = snapshot_from_outcome(
                &outcome,
                &desired,
                &desired_adapter_paths,
                served_window,
                served_lanes,
                launched_cache_mib,
                vision,
            );
            crate::probe!(
                class = "serving.reconcile",
                desired = desired.as_str(),
                ready = snapshot.ready,
                active = snapshot.active_model.as_deref().unwrap_or("<none>"),  // unwrap_or: probe label only, "<none>" = no model live
                served_window = snapshot.served_context_window,
                "serving reconcile complete",
            );
            // #363: remember the last HEALTHY lane's shape in a record that SURVIVES
            // the wedge-empty — the wedge-heal floor in the next reconcile reads it.
            if snapshot.ready && snapshot.served_context_window > 0 {
                last_healthy_window.store(snapshot.served_context_window, Ordering::Relaxed);
                last_healthy_lanes.store(snapshot.lanes, Ordering::Relaxed);
                // The commons seed (README promise: personas + genome from the
                // shared HF cards, zero manual steps): a base that just became
                // decode-ready gets its starter genes from the commons —
                // covenant-gated, idempotent, once per (process, model),
                // detached (never blocks the reconcile).
                if let Some(model) = snapshot.active_model.as_deref() {
                    crate::commands::genome_share::spawn_bootstrap(model.to_string());
                }
            }
            let capture = if current() {
                verified_capture(server.as_ref(), &snapshot, revision)
            } else {
                None
            };
            let _ = verified_target.send_replace(capture);
            // Emit on the bus first (fan-out to every subscriber + the grid),
            // then update the in-process watch view.
            Self::emit_serving(bus.as_ref(), &snapshot);
            let _ = serving_tx.send_replace(snapshot);
            // #350: from here on, an empty snapshot means "we looked and nothing is
            // serving" — a real fault. BEFORE this first publish it only meant "the
            // daemon has not finished starting", and readers could not tell the two
            // apart, so boot noise was indistinguishable from a broken lane. Marked
            // AFTER the publish so a reader that sees `has_reconciled()` is guaranteed
            // to also see the published snapshot, never a torn in-between.
            crate::inference::llama_server::mark_first_reconcile();
            // The operation permit clears the gate on success, cancellation, or panic.
        }))
    }
}
