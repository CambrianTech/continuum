//! Launch ownership for the serving daemon.

use super::*;

/// Owned inputs that passed the reconcile decision guards. The operation remains held
/// through launch and is moved into the same asynchronous lifecycle task.
pub(super) struct ReconcileLaunch {
    pub(super) operation: ServingOperation,
    pub(super) planned: ServingPlanSnapshot,
    pub(super) intent: ServingIntentSnapshot,
    pub(super) desired: String,
    pub(super) served_ctx: u32,
    pub(super) lanes: u32,
    pub(super) desired_adapters: Vec<AdapterEntry>,
    pub(super) desired_adapter_paths: Vec<String>,
}

impl ServingDaemonModule {
    pub(super) fn launch_reconciled_plan(&self, launch: ReconcileLaunch) -> Option<JoinHandle<()>> {
        let ReconcileLaunch {
            operation,
            planned,
            intent,
            desired,
            served_ctx,
            lanes,
            desired_adapters,
            desired_adapter_paths,
        } = launch;
        // Past the no-relaunch guard a relaunch WILL happen: the window it takes is the plan's
        // through the sticky, wedge-heal and physical-fit laws (`launch_window`, the one owner the
        // tick's page seed reads too), each step that moved it reported here.
        let window = self.launch_window(&desired, served_ctx, lanes);
        if window.floored != window.sticky {
            continuum_base::probe!(
                target: "continuum_core::modules::serving_daemon", class = "serving.reconcile.window",
                decision = "wedge-heal-floor",
                live_window = 0u32,
                plan_window = window.sticky,
                floored_to = window.floored,
                plan_lanes = lanes,
                "spawn-at-transient guarded: successor floored to the last healthy \
                 lane's window instead of a teardown-dip plan (#363)",
            );
        }
        if window.served != window.floored {
            continuum_base::probe!(
                target: "continuum_core::modules::serving_daemon", class = "serving.reconcile.window",
                decision = "kept-window-exceeds-physical-fit",
                kept_window = window.floored,
                bound_to = window.served,
                plan_lanes = lanes,
                physical_usable_gb = (self.physical_budget().usable_bytes / 1_000_000_000),
                "a kept (sticky / wedge-heal) window does not fit this host's physical \
                 budget at this lane count — bounded; a kept window is never a licence \
                 to exceed the card",
            );
        }
        let served_ctx = window.served;

        // Resolve the full Model struct ONCE, here, and carry it on the target —
        // no re-fetch downstream ([[pass-the-model-struct-no-param-hell]]). If
        // the registry can't produce the model the plan named, fail loud (empty
        // snapshot) rather than serving something else.
        let resolved = (self.model_resolver)(&desired);
        if self.intent.snapshot().revision != planned.intent_revision {
            return None;
        }
        let Some(model) = resolved else {
            continuum_base::probe!(
                target: "continuum_core::modules::serving_daemon", class = "serving.reconcile",
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
        continuum_base::probe!(
            target: "continuum_core::modules::serving_daemon", class = "serving.launch.plan",
            model = desired.as_str(),
            intent_revision = planned.intent_revision,
            lanes_bound = ?planned.plan.as_ref().map(|p| p.lanes_bound),
            plan_lanes = lanes,
            plan_window = served_ctx,
            host_prompt_cache_mib,
            "reconcile launch requested from this plan; engine convergence may retain incumbent geometry",
        );
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
            let physical_artifact = match target.model.gguf_local_path.as_deref() {
                Some(path) => crate::system_resources::ActiveArtifactSet::resolve(path).await,
                None => None,
            };
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
                    physical_artifact.clone(),
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
                continuum_base::probe!(
                    target: "continuum_core::modules::serving_daemon", class = "serving.reconcile.window",
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
                            continuum_base::probe!(
                                target: "continuum_core::modules::serving_daemon", class = "serving.reconcile",
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
                            continuum_base::probe!(
                                target: "continuum_core::modules::serving_daemon", class = "serving.reconcile",
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
                .unwrap_or(0);
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
                let state = diverged.then(|| {
                    (
                        target.model_id().to_owned(),
                        held,
                        wanted,
                        one_conversation_mib,
                    )
                });
                static LAST_DIVERGENCE: parking_lot::Mutex<Option<(String, u32, u32, u32)>> =
                    parking_lot::Mutex::new(None);
                // `state` carries a String, so keep the copy we report from: the cell takes
                // ownership of the new state and hands back the old one.
                let speaking = state.clone();
                let previous = say_on_change(&mut *LAST_DIVERGENCE.lock(), state);
                if let Some(previous) = previous {
                    match speaking {
                        Some((_, held, wanted, one_conversation)) => continuum_base::probe!(
                            target: "continuum_core::modules::serving_daemon", class = "serving.prompt_cache.divergence",
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
                        None => continuum_base::probe!(
                            target: "continuum_core::modules::serving_daemon", class = "serving.prompt_cache.divergence_closed",
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
                            .unwrap_or_default();
                    let sidecar_candidate = crate::inference::vision_sidecar::find_candidate(
                        &sidecar_rows,
                        Some(desired.as_str()),
                        &pinned_off,
                    );
                    let props = match server.multimodal_support().await {
                        Ok(p) => p,
                        Err(e) => {
                            continuum_base::probe!(
                                target: "continuum_core::modules::serving_daemon", class = "serving.vision.props_unreadable",
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
                    let main_lane_wears = server.mmproj_on_lane().unwrap_or_else(|| {
                        props.as_ref().is_some_and(|m| m.vision) // an adopted lane's fact is what it reports
                    });
                    let main_sees = match crate::inference::llama_server::vision_lane_ready(
                        declares_vision,
                        main_lane_wears,
                        props,
                    ) {
                        Ok(ready) => ready,
                        Err(why) => {
                            continuum_base::probe!(
                                target: "continuum_core::modules::serving_daemon", class = "serving.vision.not_ready",
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
                    } else if persona_floor.is_some_and(|f| served_window > 0 && served_window < f)
                    {
                        // THE MAIN LANE IS BELOW WHAT ITS RESIDENTS NEED: the sidecar's
                        // memory is exactly the window they are missing. Drop it (RAII kills
                        // the child, RAM freed, the next plan grows the lane); it respawns on
                        // the first plan whose headroom is measured against a lane that holds.
                        continuum_base::probe!(
                            target: "continuum_core::modules::serving_daemon", class = "serving.vision.sidecar_yields_to_persona_floor",
                            served_window = served_window as u64,
                            persona_floor = persona_floor.unwrap_or(0) as u64, // unwrap_or: guarded by is_some_and
                            "main lane below the residents' requirement — vision sidecar yields its budget to the lane"
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
                                continuum_base::probe!(
                                    target: "continuum_core::modules::serving_daemon", class = "serving.vision.sidecar_no_candidate",
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
                                let free =
                                    sidecar_admissible_bytes(live_free, planned_headroom, physical);
                                continuum_base::probe!(
                                    target: "continuum_core::modules::serving_daemon", class = "serving.vision.sidecar_headroom",
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
                                                    continuum_base::probe!(
                                                        target: "continuum_core::modules::serving_daemon", class = "serving.vision.sidecar_up",
                                                        model = lane.model_id.as_str(),
                                                        base_url = lane.base_url.as_str(),
                                                        "vision sidecar verified — personas can see",
                                                    );
                                                    Some(lane)
                                                }
                                                Err(why) => {
                                                    continuum_base::probe!(
                                                        target: "continuum_core::modules::serving_daemon", class = "serving.vision.sidecar_failed",
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
                                        continuum_base::probe!(
                                            target: "continuum_core::modules::serving_daemon", class = "serving.vision.sidecar_no_budget",
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
            continuum_base::probe!(
                target: "continuum_core::modules::serving_daemon", class = "serving.reconcile",
                desired = desired.as_str(),
                ready = snapshot.ready,
                active = snapshot.active_model.as_deref().unwrap_or("<none>"),
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
                    continuum_dreaming::commands::genome_share::spawn_bootstrap(model.to_string());
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
