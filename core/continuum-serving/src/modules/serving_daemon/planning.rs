//! Planning ownership for the serving daemon.

use super::*;

impl ServingDaemonModule {
    /// The daemon-owned mutation capability; raw watch writers never escape.
    pub fn intent(&self) -> ServingIntent {
        self.intent.clone()
    }

    /// Compute the current serving plan from the live host snapshot + on-disk
    /// models, WITHOUT relying on a tick having run. The boot path calls this
    /// to drive the spawner before the tick loop starts — single source of
    /// truth for "what model + how many lanes."
    pub fn compute_plan(&self) -> Option<ServingPlan> {
        let (budget, leases) = self.host_budget_snapshot();
        let mut demand = self.serving_demand();
        account_training_leases(&mut demand, &leases);
        plan_serving(&budget, &self.live_candidates(), &demand)
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
        let (budget, leases) = self.host_budget_snapshot();
        let intent = self.intent.snapshot();
        let catalog = self.catalog.snapshot();
        let candidates = servable_candidates(&catalog, &intent.suppressed, &intent.pinned);
        let on_disk = servable_candidates(&catalog, &HashSet::new(), &intent.pinned);
        self.publish_plan_with_leases(budget, &candidates, &on_disk, intent.revision, &leases);
    }

    /// Pure publish step: run the classifier on the given inputs, publish the
    /// result, log it. Split from `recompute` so it's testable without the
    /// global registry / live GPU.
    /// `candidates` is what the plan may choose from NOW (suppression and pin applied);
    /// `on_disk` is everything servable with only the pin applied — the universe the
    /// host floor is judged over. Both are inputs so the floor is never read from an
    /// ambient catalog.
    #[cfg(test)]
    pub(super) fn publish_plan(
        &self,
        budget: HostBudget,
        candidates: &[ModelFootprint],
        on_disk: &[ModelFootprint],
    ) {
        self.publish_plan_for_intent(budget, candidates, on_disk, self.intent.snapshot().revision);
    }

    pub(super) fn publish_plan_snapshot(&self, intent_revision: u64, plan: Option<ServingPlan>) {
        // Initialization can overlap the authority tick. Serialize both publications
        // under the private authoritative watch so their final views cannot diverge.
        // Public projection readers never acquire this private watch; no await here.
        self.plan_tx.send_modify(|state| {
            *state = ServingPlanSnapshot {
                intent_revision,
                plan: plan.clone(),
            };
            let _ = self.plan_view_tx.send_replace(plan);
        });
        self.plan_published.store(true, Ordering::Release);
    }

    /// A policy may keep the previous geometry. A new intent may authorize that
    /// same model, but cannot authorize a model absent from its candidate snapshot.
    pub(super) fn retain_plan_for_intent(
        &self,
        intent_revision: u64,
        candidates: &[ModelFootprint],
    ) {
        let previous = self.plan_tx.borrow().clone();
        if previous.intent_revision != intent_revision
            && previous.plan.as_ref().is_some_and(|p| {
                candidates
                    .iter()
                    .any(|c| c.model_id == p.base_model.model_id)
            })
        {
            self.publish_plan_snapshot(intent_revision, previous.plan);
        }
    }

    #[cfg(test)]
    pub(super) fn publish_plan_for_intent(
        &self,
        budget: HostBudget,
        candidates: &[ModelFootprint],
        on_disk: &[ModelFootprint],
        intent_revision: u64,
    ) {
        self.publish_plan_with_leases(budget, candidates, on_disk, intent_revision, &[]);
    }

    fn publish_plan_with_leases(
        &self, budget: HostBudget, candidates: &[ModelFootprint],
        on_disk: &[ModelFootprint], intent_revision: u64,
        leases: &[crate::resources::lease::ResourceLease],
    ) {
        // Hysteresis: pass the currently-served model as the incumbent so a
        // transient free-memory dip doesn't thrash the served model.
        //
        // Boot's first plan used to have no incumbent (plan_tx holds None) and fall through to
        // plain selection — and THAT was #438. A successor core boots while its predecessor's
        // lane is still resident, so the one moment a squeeze is GUARANTEED is the one moment
        // both the at-rest credit and the downshift debounce are disabled. Measured: the 27B's
        // successor sampled `usable_gb=6`, downshifted to a 14B at MIN_SERVE_CTX, and reclaimed
        // the 27B 1.2s later — freeing 45 GB it could no longer use. An inherited lane is not an
        // external squeeze; it is a past form of ourself, and it is the incumbent.
        let incumbent = incumbent_for_plan(
            self.plan_tx
                .borrow()
                .plan
                .as_ref()
                .map(|p| p.base_model.model_id.clone()),
            (self.inherited_lane)().as_ref(),
        );
        // Remembered geometry is a cache preference, never capacity or demand.
        // The current roster, knee and admission budget decide lanes and width.
        let remembered = crate::inference::served_window_store::load_geometry()
            .filter(|g| candidates.iter().any(|c| c.model_id == g.model_id));
        let seat_model = incumbent
            .as_deref()
            .or(remembered.as_ref().map(|g| g.model_id.as_str()));
        let mut demand = self
            .serving_demand()
            .with_sticky_window(seat_model.and_then(crate::inference::served_window_store::load_for));
        account_training_leases(&mut demand, leases);
        // THE INCUMBENT'S OWN BYTES ARE CREDITED BACK EXACTLY ONCE. `budget` is the
        // ledger's replace-myself budget: serving's own measured footprint added back,
        // capped at the device. Once serving has REPORTED that footprint (steady state,
        // and the inherited lane once its attribution lands), the budget is at rest and
        // the plan must not credit again — twice planned 2 × 70,093 on a card whose fit
        // was 2 × 47,098 and the re-home law relaunched the lane three times mid-solve
        // chasing it (5090, 2026-09-17). Before that report exists (the first ticks of a
        // boot over an inherited lane, #438) the ledger has nothing to add back, and the
        // plan's own credit is the one that keeps the successor from fleeing its
        // predecessor onto a smaller model. One credit, whichever layer holds the fact.
        let ledger_credited = self.resource_daemon.board().attributions.iter().any(|a| {
            a.consumer_id == SERVING_CONSUMER_ID && a.kind == serving_pool_kind() && a.bytes > 0
        });
        if !ledger_credited {
            // The boot planner adds incumbent bytes after this snapshot. That
            // enlarged budget no longer proves these leases were excluded.
            account_training_leases(&mut demand, &[]);
        }
        // WHICH BRANCH PLANNED THIS, AND WHAT IT PLANNED FROM (card d0ba2342, 2026-09-21).
        // These two arms differ by exactly ONE engine footprint: `at_rest` trusts that the
        // ledger has already credited serving's own bytes back, `stable` credits them here.
        // Taking the wrong one puts the budget off by a whole engine in one direction or the
        // other — and the selector is a board attribution, so which arm ran is OBSERVABLE and
        // was being inferred instead. Measured that night on the M5: the engine held a SETTLED
        // 3 lanes at 78,592 while the plan said 1 (`bound_by=host-fit`, `demand_lanes=2`) with
        // one lane of KV costing 2.4 GiB against 13.9 GiB available. Two confident explanations
        // — the plan charging f16 while the engine served q8_0, and the weights being charged
        // against a budget that already excluded the running engine — were both refuted by
        // reading the guards that prevent them (#232's `apply_kv_quantization`, and the
        // credit-back-exactly-once ledger above). Nobody could say which arm had run, because
        // nothing said so. This says it.
        let stable = if ledger_credited {
            plan_serving_at_rest(&budget, candidates, incumbent.as_deref(), &demand)
        } else {
            plan_serving_stable(&budget, candidates, incumbent.as_deref(), &demand)
        };
        {
            let credited_bytes: u64 = self
                .resource_daemon
                .board()
                .attributions
                .iter()
                .filter(|a| a.consumer_id == SERVING_CONSUMER_ID && a.kind == serving_pool_kind())
                .map(|a| a.bytes)
                .sum();
            let planned = stable.as_ref().map(|p| (p.lanes, p.served_context_window));
            // Once per CHANGE OF THE DECISION, never per 5-second tick — the same law the
            // prompt-cache divergence receipt in this file follows, and a law this probe
            // broke on its first draft (Cormac on #4298): the cell keyed on
            // `budget.usable_bytes`, which moves every tick as the governed board re-reads
            // available memory, so a "once per change" receipt fired every 5 seconds — the
            // exact shape the other half of this PR exists to fix. The KEY is the decision:
            // which arm ran, and what it produced. The budget is CONTEXT for reading that
            // decision, carried as fields, and a budget that drifts while the arm and the
            // plan hold steady is not a new fact about the planner.
            static LAST_BRANCH: parking_lot::Mutex<Option<(bool, Option<(u32, u32)>)>> =
                parking_lot::Mutex::new(None);
            let now = Some((ledger_credited, planned));
            if say_on_change(&mut *LAST_BRANCH.lock(), now).is_some() {
                continuum_base::probe!(
                    target: "continuum_core::modules::serving_daemon", class = "serving.plan.branch",
                    branch = if ledger_credited { "at_rest" } else { "stable" },
                    ledger_credited = ledger_credited,
                    credited_bytes = credited_bytes,
                    usable_bytes = budget.usable_bytes,
                    perf_cores = budget.perf_cores as u64,
                    planned_lanes = planned.map(|(l, _)| l as u64).unwrap_or(0), // unwrap_or: 0 = no plan produced, not a lane count
                    planned_window = planned.map(|(_, w)| w as u64).unwrap_or(0), // unwrap_or: as above
                    "which planning arm produced this plan, and the budget it planned from — \
                     the two arms differ by one engine footprint, so this is the first thing to \
                     read when the plan and the running engine disagree about how many lanes fit"
                );
            }
        }
        match stable {
            Some(plan) => {
                // NO PERSONA LANE BELOW WHAT THE RESIDENTS REQUIRE (2eec3977; Joel 2026-09-20:
                // "2k context is a complete waste of a lane"). A plan whose per-slot window is
                // below the residents' requirement (measured today, declared per role once the
                // allocator lands — never a constant here) does not REPLACE a published plan:
                // the previous one stands (the running lane keeps serving) and the per-token
                // footprint record that shrank the window is RETIRED so the next tick plans
                // from the estimate — a starved plan is evidence of a trapped record (the 5090
                // sat at 2 × 2,048 for an afternoon on a ~244k B/token record taken there).
                // With NO plan published (a cold boot) the best runnable plan is published and
                // SAID (Cormac's condition on #4257): on the floor tier no candidate may hold
                // the requirement at all — IntelMac's 1.5B is 32k trained against a 58k typical
                // — and a dark node is worse than a starved seat; placement routes her turns to
                // a seat that holds them. Emitted once per (model, window, lanes).
                // Typed per-mind allocation already enforces its own requirements.
                // Only legacy plans need this additional historical floor.
                let requirement = demand.publication_prompt_floor(&plan);
                if !crate::inference::planning::serving_plan::persona_lane_holds(
                    plan.served_context_window,
                    requirement,
                ) {
                    let retired =
                        crate::inference::lane_footprint::retire(&plan.base_model.model_id);
                    let previous_stands = self.plan_tx.borrow().plan.is_some();
                    static LAST_STARVED: parking_lot::Mutex<Option<(String, u32, u32)>> =
                        parking_lot::Mutex::new(None);
                    let key = (
                        plan.base_model.model_id.clone(),
                        plan.served_context_window,
                        plan.lanes as u32,
                    );
                    let mut last = LAST_STARVED.lock();
                    if last.as_ref() != Some(&key) || retired {
                        continuum_base::probe!(
                            target: "continuum_core::modules::serving_daemon", class = "serving.plan.below_persona_floor",
                            model = plan.base_model.model_id.as_str(),
                            lanes = plan.lanes as u64,
                            window = plan.served_context_window as u64,
                            requirement = requirement.unwrap_or(0) as u64, // unwrap_or: probe label; this branch only fires with a requirement present
                            usable_gb = (budget.usable_bytes / 1_000_000_000),
                            record_retired = retired,
                            published = !previous_stands,
                            "the plan's window is below what the residents require — a published plan stands \
                             instead when one exists (and the footprint record is retired); with none, the best \
                             runnable plan is published and named a starved seat (a dark node is worse)"
                        );
                        *last = Some(key);
                    }
                    if previous_stands {
                        self.retain_plan_for_intent(intent_revision, candidates);
                        return;
                    }
                }
                // THE HOST FLOOR (Joel, 2026-09-16: "if this machine ever returns less than
                // 27b we are sucking"). The debounce below separates jitter from a sustained
                // squeeze; it does not say what a host may SINK TO. Measured today: an unload
                // suppressed the 27B, so it vanished from `candidates`, the debounce saw
                // "incumbent gone from disk" and adopted the 0.5B on a 32 GB card at a
                // transient 24 GB — then relaunched it 35 times (#4145). The floor is what
                // this host's PHYSICAL capacity fits, judged by the SAME selection law over
                // everything servable on disk, ignoring suppression and pin (intent to move
                // is never licence to sink). A plan below it is refused, loudly, every time
                // it is asked for; the previous plan stands.
                let floor = host_floor_of(self.physical_budget(), on_disk, &demand);
                // The floor guards against a host SINKING below its capability — which needs an
                // incumbent to sink FROM. With no incumbent (a COLD BOOT) there is nothing to
                // preserve, and refusing serves NOTHING: the node boots dark (card 48f5438a,
                // measured on the Intel tier 2026-09-17: resident 0, active_model None,
                // degraded_reason NULL, while a servable 1.5B sat on disk). So refuse ONLY when
                // there is an incumbent worth holding.
                let verdict = floor_gate(&plan, floor.as_ref());
                // How long the plan has been below the floor with NOTHING serving. A
                // teardown transient (one low reading while a predecessor dies) clears
                // in a tick or two; a real shortfall holds for hours. The two are orders
                // of magnitude apart in duration, so the grace is not delicate.
                static BELOW_FLOOR_SINCE: parking_lot::Mutex<Option<std::time::Instant>> =
                    parking_lot::Mutex::new(None);
                let below_for = {
                    let mut since = BELOW_FLOOR_SINCE.lock();
                    if matches!(verdict, FloorVerdict::Below { .. }) && incumbent.is_none() {
                        since.get_or_insert_with(std::time::Instant::now).elapsed()
                    } else {
                        *since = None;
                        std::time::Duration::ZERO
                    }
                };
                let refuse = below_floor_refuses(&verdict, incumbent.is_some(), below_for);
                if let FloorVerdict::Below { floor, wanted } = verdict {
                    if refuse {
                        static LAST_REFUSED: parking_lot::Mutex<Option<(String, String)>> =
                            parking_lot::Mutex::new(None);
                        let key = (floor.clone(), wanted.clone());
                        if LAST_REFUSED.lock().as_ref() != Some(&key) {
                            continuum_base::probe!(
                                target: "continuum_core::modules::serving_daemon", class = "serving.plan.demotion_refused",
                                floor = floor.as_str(),
                                wanted = wanted.as_str(),
                                incumbent = incumbent.as_deref().unwrap_or("<none>"),
                                usable_gb = (budget.usable_bytes / 1_000_000_000),
                                physical_usable_gb =
                                    (self.physical_budget().usable_bytes / 1_000_000_000),
                                "plan wants a base BELOW what this host's physical capacity \
                                 serves — refused; the previous plan stands until the budget \
                                 returns (a host never sinks below its floor)"
                            );
                            *LAST_REFUSED.lock() = Some(key);
                        }
                        self.retain_plan_for_intent(intent_revision, candidates);
                        // THE MISSING ARM (card c3c50e0d; Cormac: "no decision cannot compile").
                        // "The previous plan stands" assumes a previous plan. At boot beside an
                        // INHERITED engine there is none — plan_tx holds None — so a refused
                        // demotion published NOTHING, the reconcile had nothing to do, and a
                        // healthy engine on its port was never adopted (M5 2026-09-26 18:22Z:
                        // six minutes, sixteen residents dark, cleared only by killing the
                        // engine). An inherited lane is a past form of ourself: its residency
                        // is ours to reclaim, so the plan that ADOPTS it is planned against the
                        // PHYSICAL budget at the geometry it is running. Adopt, or say why not.
                        if self.plan_tx.borrow().plan.is_none() {
                            if let Some(inc) = incumbent.as_deref() {
                                let geometry = crate::inference::served_window_store::load_geometry()
                                    .filter(|g| g.model_id == inc)
                                    .and_then(|g| g.cache_geometry());
                                match incumbent_adoption_plan(
                                    &self.physical_budget(),
                                    candidates,
                                    inc,
                                    &demand,
                                    geometry,
                                ) {
                                    Some(plan) => {
                                        continuum_base::probe!(
                                            target: "continuum_core::modules::serving_daemon", class = "serving.plan.incumbent_adopted",
                                            model = inc,
                                            lanes = plan.lanes as u64,
                                            window = plan.served_context_window as u64,
                                            usable_gb = (budget.usable_bytes / 1_000_000_000),
                                            physical_usable_gb =
                                                (self.physical_budget().usable_bytes / 1_000_000_000),
                                            "a demotion was refused with NO plan to retain: the \
                                             inherited engine is planned at its own geometry \
                                             against the physical budget — adopt, never nothing"
                                        );
                                        self.publish_plan_snapshot(intent_revision, Some(plan));
                                    }
                                    None => continuum_base::probe!(
                                        target: "continuum_core::modules::serving_daemon", class = "serving.plan.incumbent_unplannable",
                                        model = inc,
                                        physical_usable_gb =
                                            (self.physical_budget().usable_bytes / 1_000_000_000),
                                        "a demotion was refused with no plan to retain and the \
                                         inherited engine cannot be planned even against the \
                                         physical budget — refused, named; the reconcile stays idle"
                                    ),
                                }
                            }
                        }
                        return;
                    }
                    // COLD BOOT, no incumbent: a viable base the budget CAN serve beats a
                    // capability floor it cannot reach. Serve the plan instead of booting dark
                    // (Cormac's recovery 2026-09-17: pinning the 1.5B — the very plan this gate
                    // stood in front of — came up ready with all eight citizens hosted). The
                    // floor's other half — count only models with a VIABLE served window, so
                    // the 4B whose weights fit but whose window is a dead 2048 stops out-ranking
                    // a serviceable 1.5B — stays card 48f5438a and is owned with its measurements.
                    continuum_base::probe!(
                        target: "continuum_core::modules::serving_daemon", class = "serving.plan.cold_boot_below_floor_served",
                        floor = floor.as_str(),
                        wanted = wanted.as_str(),
                        waited_s = below_for.as_secs(),
                        physical_usable_gb =
                            (self.physical_budget().usable_bytes / 1_000_000_000),
                        "no incumbent to preserve and the plan stayed below the capability \
                         floor past the grace — serving it rather than staying dark (card 48f5438a)"
                    );
                }
                // DOWNSHIFT DEBOUNCE (#368): `plan_serving_stable`'s at-rest credit
                // only shields the incumbent from its OWN residency — an external
                // squeeze deep enough that even the credited budget can't hold it
                // forces `fresh` through, and `fresh` at a zeroed budget picks the
                // smallest model on disk. That is correct for a REAL eviction and
                // catastrophic for a transient (one depressed tick re-brained the
                // whole citizenry onto the 0.5B, then hysteresis defended the wrong
                // incumbent). So a downshift only takes effect after it has been
                // wanted for [`DOWNSHIFT_SUSTAINED_TICKS`] consecutive ticks; until
                // then the previous plan simply stands. Same shape as the rehome
                // streak guard above — sustained-ness separates capacity change
                // from jitter. Upshifts and same-model replans are never held.
                match downshift_gate(&plan, incumbent.as_deref(), candidates) {
                    DownshiftVerdict::NotADownshift => {
                        self.downshift_streak.store(0, Ordering::Relaxed);
                    }
                    DownshiftVerdict::Downshift => {
                        let streak = self
                            .downshift_streak
                            .fetch_add(1, Ordering::Relaxed)
                            .saturating_add(1);
                        if streak < DOWNSHIFT_SUSTAINED_TICKS {
                            continuum_base::probe!(
                                target: "continuum_core::modules::serving_daemon", class = "serving.plan",
                                decision = "downshift-held",
                                incumbent = incumbent.as_deref().unwrap_or("<none>"),
                                wanted = plan.base_model.model_id.as_str(),
                                streak,
                                needs_streak = DOWNSHIFT_SUSTAINED_TICKS,
                                usable_gb = (budget.usable_bytes / 1_000_000_000),
                                "fresh plan wants a LESS capable base — holding the \
                                 incumbent plan until the squeeze proves sustained (#368)",
                            );
                            self.retain_plan_for_intent(intent_revision, candidates);
                            return;
                        }
                        // Sustained: a real squeeze. Adopt, and re-arm the gate.
                        self.downshift_streak.store(0, Ordering::Relaxed);
                    }
                }
                // The per-prefill transient spike, derived BEFORE the probe so the
                // probe can carry it. This value decides the prefill grant's
                // deadband (#415) and prices "can local serve one more?" in
                // capacity::lease — and until 2026-08-13 it appeared in NEITHER,
                // so a flapping grant was unfalsifiable in exactly the way the
                // served_window comment below warns about. `None` here is NOT the
                // same fact as a measured zero: it means the served base model was
                // absent from its own candidate list, which is a planner
                // inconsistency, not "this model has no compute buffer".
                let spike_of_served = candidates
                    .iter()
                    .find(|c| c.model_id == plan.base_model.model_id)
                    .map(|f| f.compute_buffer_per_lane());
                // Emit-on-change gate (#399): the plan recomputes every tick but
                // the probe fires only when what it would SAY differs from the
                // last emission. Per-tick emission was 51% of the entire probe
                // stream — identical rows rotating real history away in minutes.
                //
                // The trigger is DECISIONS ONLY. Continuously-MEASURED fields
                // (usable_gb, served/demand windows) ride the row as payload but
                // never trigger it: on a live box free memory churns ±1-2 GB and
                // the derived window wobbles ~1k tokens every tick, so any
                // quantization of them still flaps at bucket boundaries — two
                // attempts measured live (1k buckets → 146 rows/3min; GB
                // granularity → 563 rows/15min). A WINDOW change only emits past
                // a relative deadband (>1/8 from the last-emitted value), the
                // same shape as the #415 prefill deadband: a real re-plan
                // (16384→26368) clears it, measurement wobble never does.
                let decisions = format!(
                    "{}|{}|{}|{}|{}|{}|{}",
                    plan.base_model.model_id,
                    plan.lanes,
                    plan.resident_models,
                    plan.fits_on_gpu,
                    candidates.len(),
                    spike_of_served.unwrap_or(0),
                    demand.lanes,
                );
                let plan_probe_changed = {
                    let mut last = self
                        .last_plan_probe
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let window_moved = |prev: u32| -> bool {
                        plan.served_context_window.abs_diff(prev) > prev / 8
                    };
                    match last.as_ref() {
                        Some((d, w)) if *d == decisions && !window_moved(*w) => false,
                        _ => {
                            *last = Some((decisions, plan.served_context_window));
                            true
                        }
                    }
                };
                if plan_probe_changed {
                    continuum_base::probe!(
                        target: "continuum_core::modules::serving_daemon", class = "serving.plan",
                        base_model = plan.base_model.model_id.as_str(),
                        lanes = plan.lanes,
                        resident = plan.resident_models,
                        fits_on_gpu = plan.fits_on_gpu,
                        usable_gb = (budget.usable_bytes / 1_000_000_000),
                        candidates = candidates.len(),
                        // #415: the deadband's deciding input. Zero DISABLES the
                        // deadband entirely (reconcile short-circuits), so a grant
                        // that flaps while this reads 0 is a different defect from one
                        // that flaps with a real spike — and they were indistinguishable.
                        spike_bytes = spike_of_served.unwrap_or(0), // JUSTIFIED unwrap_or: probe-only, and 0 is not a fabricated measurement — it is the documented DISABLED sentinel the deadband already keys on, with `spike_known` on the next line carrying the Option's provenance so a reader can still tell "no spike" from "no data"
                        spike_known = spike_of_served.is_some(),
                        // WHICH bound decided the window. Without this the plan is
                        // unfalsifiable: a window that does not grow after demand rises
                        // looks identical whether the HOST could not fit more, the
                        // measured DEMAND did not ask for more, or the reconcile held
                        // within hysteresis — three different bugs with one appearance.
                        // Read 2026-08-06 when 11 turns measured over_window ≥ 1.09 and
                        // the served window sat unmoved at 16384; nothing on hand could
                        // say why, which is a hole in the glass box
                        // ([[a-probe-that-can-only-fail-is-worse-than-no-probe]]).
                        served_window = plan.served_context_window,
                        demand_window = demand.window_tokens,
                        demand_lanes = demand.lanes,
                        leased_in = demand.leased_in,
                        leased_in_peak = LAST_LEASED_IN_PEAK.load(Ordering::Relaxed),
                        // `bootstrap` is a THIRD state, not a flavour of `demand`
                        // (2026-08-20). A cold plan reporting `demand` claims the minds
                        // asked for 16384 when none had asked for anything — and that is
                        // the one reading this field exists to rule out. Ordered so the
                        // unmeasured case wins: with no measurement there is no demand
                        // bound to be within, so calling it demand-bound is vacuous.
                        bound_by = if !demand.measured {
                            "bootstrap"
                        } else if plan.served_context_window >= demand.window_tokens {
                            "demand"
                        } else {
                            // the window came out below demand; WHICH term did it is `lanes_bound`
                            // below and the plan's rationale (this read "host-fit" until card
                            // eb8d1839, and was taken for a host-RAM finding)
                            "below-demand"
                        },
                        demand_measured = demand.measured,
                        // WHAT chose the lane count, from the planner's own decision. `bound_by`
                        // above says only whether the WINDOW came out below demand, whatever
                        // budget did it; this names the term that decided `lanes` (card eb8d1839).
                        lanes_bound = ?plan.lanes_bound,
                        "serving plan recomputed",
                    );
                    // Warm-slot oversubscription for the ADOPTED plan (#266) —
                    // probed here (not in the pure planner, which runs twice per
                    // tick on different budgets and floods) and only when the
                    // plan-fingerprint changed, so a standing condition emits at
                    // its transitions, not per tick (#399).
                    if (plan.lanes as u32) < demand.lanes {
                        continuum_base::probe!(
                            target: "continuum_core::modules::serving_daemon", class = "serving.plan",
                            decision = "warm-slot-oversubscribed",
                            resident_personas = demand.lanes,
                            warm_slots = plan.lanes,
                            without_warm_slot = demand.lanes - plan.lanes as u32,
                            per_slot_floor =
                                crate::inference::planning::serving_plan::BOOTSTRAP_WORKING_SET,
                            "adopted plan cannot warmly host all resident personas at the \
                             full-turn window floor — unslotted minds re-prefill cold every \
                             turn until tiered off or grid-placed (#266)",
                        );
                    }
                }
                // Publish the LIVE lane count to the admission gate so its directed-turn
                // reservation semaphores size by what's actually served, not the `MAX_LANES`
                // ceiling (#139 compute-buffer fit). LIVE means the engine's own `--parallel`,
                // from the serving snapshot: the plan is a proposal the reconcile may never
                // take. On the M5, 2026-10-11 01:41Z, the plan moved 2,2,1,1,1,1 lanes in 40 s
                // with no relaunch, and the gate followed it: a 2-lane engine admitted as one
                // lane, half its seats reserved for directed turns that never came.
                let admitted = {
                    let live = self.serving_tx.borrow();
                    admitted_lanes(live.ready, live.lanes, plan.lanes as u32)
                };
                crate::inference::resource_admission::set_served_lane_count(admitted as usize);
                // And the prefill throttle's demand facts (#56): the served model's per-spike
                // transient compute buffer + the lane count. Published HERE, next to the lane
                // count, so both gates read the one plan — no second path.
                // A MISSING spike is not a measured zero. Publishing 0 silently
                // disables the prefill deadband (#415) and prices lease admission
                // as if prefill were free — two safety mechanisms turned off by an
                // absent signal rather than a decision
                // ([[removing-a-hardcode-is-not-automatically-an-improvement]]:
                // a missing signal must be loud, never a default that happens to
                // parse). Still publishes 0 so behaviour is unchanged pending the
                // #415 decision on what a deadband means with no spike — but it can
                // no longer happen invisibly.
                if spike_of_served.is_none() && plan_probe_changed {
                    continuum_base::probe!(
                        target: "continuum_core::modules::serving_daemon", class = "serving.plan.spike_missing",
                        base_model = plan.base_model.model_id.as_str(),
                        candidates = candidates.len(),
                        "served base model absent from its own candidate list — per-prefill \
                         spike unknown, publishing 0, which DISABLES the prefill deadband",
                    );
                }
                let spike = spike_of_served.unwrap_or(0);
                crate::inference::prefill_spike::publish(spike);
                // send_replace keeps the latest even with no live receivers yet.
                self.publish_plan_snapshot(intent_revision, Some(plan));
            }
            None => {
                // No servable model on disk. Publish None and say so loudly —
                // the spawner gates on a model being present (no silent serve).
                // Same emit-on-change gate as the Some arm: loud ONCE at the
                // transition, silent while the condition persists (#399).
                let entered_empty = {
                    let mut last = self
                        .last_plan_probe
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    if last.as_ref().map(|(d, _)| d.as_str()) == Some("<no-servable-model>") {
                        false
                    } else {
                        *last = Some(("<no-servable-model>".to_string(), 0));
                        true
                    }
                };
                if entered_empty {
                    continuum_base::probe!(
                        target: "continuum_core::modules::serving_daemon", class = "serving.plan",
                        candidates = 0usize,
                        "no servable model on disk — serving plan empty",
                    );
                }
                self.publish_plan_snapshot(intent_revision, None);
            }
        }
    }
}

/// Match exact job identity; unrelated holders and other jobs earn no credit.
fn account_training_leases(demand: &mut ServingDemand, leases: &[crate::resources::lease::ResourceLease]) {
    for requirement in &mut demand.requirements {
        let Some(reserve) = &mut requirement.training else { continue };
        let bytes = reserve.job.map_or(0, |job| {
            let consumer = format!("genome-train:{job}");
            leases.iter().filter(|lease| lease.consumer_id == consumer)
                .fold(0u64, |sum, lease| sum.saturating_add(lease.bytes))
        });
        reserve.account_charged_bytes(bytes);
    }
}

#[cfg(test)]
mod training_lease_tests {
    use super::*;
    use crate::inference::planning::window_allocator::{allocate, LaneRequirement, TrainingGrowth, TrainingPrice, TrainingReserve};
    use crate::resources::lease::{ReclaimPolicy, ResourceLease};

    // What this catches: unrelated or different-job leases cannot cancel a
    // training reserve; two charged leases for the exact job count together.
    #[test]
    fn only_exact_job_leases_credit_training_and_credit_is_reset() {
        let job = uuid::Uuid::from_u128(7);
        let price = TrainingPrice {
            bytes: 100,
            source: crate::inference::planning::training_plan::NeedSource::LowerBound,
            model: "fixture".into(), shape: "fixture".into(),
        };
        let requirement = LaneRequirement::declared(uuid::Uuid::from_u128(1), 40_000)
            .with_training(Some(TrainingReserve::new(64_256, Some(price), Some(job))));
        let mut demand = ServingDemand::new(1, Some(40_000)).with_requirements(vec![requirement]);
        let lease = |id: &str, consumer: String, bytes| ResourceLease {
            lease_id: id.into(), consumer_id: consumer, kind: ResourceKind::Vram,
            bytes, acquired_at_ms: 0, expires_at_ms: 1000, reclaim_policy: ReclaimPolicy::Pinned,
        };
        let leases = [
            lease("a", format!("genome-train:{job}"), 30),
            lease("b", format!("genome-train:{job}"), 20),
            lease("c", format!("genome-train:{}", uuid::Uuid::from_u128(8)), 1000),
            lease("d", "embedding".into(), 1000),
        ];
        let model = ModelFootprint {
            model_id: "fixture".into(), weights_bytes: 19 * (1 << 30), kv_per_token: 33_000,
            context_window: 262_144, capability_rank: 1, fixed_per_lane_bytes: 0,
        };
        let budget = HostBudget { usable_bytes: 64 * (1 << 30), perf_cores: 1 };
        account_training_leases(&mut demand, &leases);
        let allocated = allocate(&demand.requirements, &[model.clone()], &budget, 1)
            .expect("fixture fits");
        assert_eq!(allocated.training_growth, TrainingGrowth::Reserved { bytes: 50, target: 64_256, reached: true });
        account_training_leases(&mut demand, &[]);
        let allocated = allocate(&demand.requirements, &[model], &budget, 1).expect("fixture fits");
        assert_eq!(allocated.training_growth, TrainingGrowth::Reserved { bytes: 100, target: 64_256, reached: true });
    }
}
