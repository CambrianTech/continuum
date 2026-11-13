//! Launch ownership for the serving daemon.

use super::*;
use crate::inference::planning::window_allocator::Measure;

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
    /// The engine's one KV pool, sized ONCE when the engine starts and never by a relaunch, as
    /// `(per-sequence tokens, sequences)` so `-c` is the pool and `--parallel` the ceiling.
    ///
    /// The launch decides only the pool's size; no mind's window comes from it. Each mind's
    /// window is her grant from her measured requirement (`grants.rs`), and under `--kv-unified`
    /// any one sequence may grow into the whole pool. The sequences are every seated mind, the
    /// minds leased in at the last plan, and the scratch traffic, so seating never relaunches.
    /// The size is [`launch_pool`] at the published plan's LIVE budget: what memory holds now, beside
    /// everything else on the node. Never the host's physical total (590df4bc3 sized the M5 at
    /// 5 x 262,144 from it: memory pressure 91%, 5.6 GB available, no resident hosted; rolled back)
    /// and never a cold boot's first-tick fallback plan, which planning no longer publishes.
    pub(super) fn engine_pool(&self, model: &crate::model_registry::types::Model, served_ctx: u32, lanes: u32) -> (u32, u32) {
        let leased_in = LAST_LEASED_IN_PEAK.load(Ordering::Relaxed);
        let sequences = self.seated().saturating_add(1).saturating_add(leased_in).max(lanes).max(1);
        // Each lock is taken and released in its own statement: a guard made inside a call's
        // argument list lives to the end of the statement, so reading the budget there while
        // `seated_for` locks it again deadlocked the reconcile and wedged the core's boot (the M5,
        // 2026-11-07 19:11Z: no module past grid-capacity initialized, the IPC socket never bound).
        let usable = self.planned_budget.lock().map_or(0, |b| b.usable_bytes); // map_or: no plan computed yet is no sampled budget, which launch_pool reads as 0 exactly as before
        let seated = self.seated_for(model);
        let pool = launch_pool(footprint_for(model).as_ref(), usable, served_ctx, lanes, sequences, model.context_window, seated);
        // `-c` is the CLI's `slots x per-slot`: the pool carried in that shape, not anyone's window.
        let per_slot = u32::try_from(pool / u64::from(sequences)).unwrap_or(u32::MAX); // unwrap_or: a pool over u32 x sequences is no real engine; the CLI caps it
        (per_slot, sequences)
    }

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
        // Past the no-relaunch guard the ensure runs: it adopts a serving engine, or launches one
        // whose window starts from the plan's through the sticky, wedge-heal and physical-fit laws
        // (`launch_window`, the one owner the tick's page seed reads too), each step reported here.
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
        let (per_sequence, sequences) = self.engine_pool(&model, served_ctx, lanes);
        // A launch with no engine on its port, below what the seated minds need, waits while the
        // host is still returning memory: the one launch keeps its pool for life, and a pool sized
        // from that reading left the M5 dark at 2,048 tokens (2026-11-13 19:36Z, `settle`).
        if (self.inherited_lane)().is_none() {
            let pool = u64::from(per_sequence) * u64::from(sequences);
            let need: u64 = self.seated_needs(&model).iter().sum();
            let host = *self.host_settle.lock(); // its own statement: the guard is released before the gate and its probe
            if let settle::LaunchGate::AwaitSettle { pool, need, settle: host } = settle::launch_gate(host, pool, need) {
                continuum_base::probe!(
                    target: "continuum_core::modules::serving_daemon", class = "serving.launch.await_settle",
                    model = desired.as_str(),
                    pool,
                    need,
                    settle = ?host,
                    "the launch pool is below the seated minds' needs while the host is still returning memory: the next reconcile sizes it from the settled reading"
                );
                return None;
            }
        }
        let target = ServingTarget {
            host_prompt_cache_mib,
            model,
            context_window: per_sequence,
            lanes: sequences,
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
            "reconcile launch requested from this plan: the engine relaunches only for a different model, genome, sight, KV decision, binary or a cache not yet on the shared pool",
        );
        let served_resident = self.served_resident.clone();
        let division = self.division.clone();
        // Consume any pending force-relaunch the liveness heartbeat raised: it means the
        // heartbeat already saw the live lane fail decode, so this reconcile must re-prove
        // decode even on a child we own (else the owned-child trust re-adopts the wedged
        // lane forever, #175). Read+clear here so exactly one reconcile acts on it.
        let force_relaunch = self.force_relaunch.clone();
        let server = self.server.clone();
        let served_pool = Arc::clone(&self.served_pool);
        let served_sequence = Arc::clone(&self.served_sequence);
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
        let persona_floor: Option<u32> = crate::capacity::window_grant::largest_target(&self.serving_demand().requirements, target.model.context_window);
        let last_healthy_window = self.last_healthy_window.clone();
        let last_healthy_lanes = self.last_healthy_lanes.clone();
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
            // The engine's own geometry, read the same way under either launch (CONTINUUM-ENGINE.md,
            // serving engine: admission (4.8)): the pool and the most one sequence may hold. The grants read both from
            // here, never from argv. What sequences are PUBLISHED as their share still differs by
            // launch: on the shared pool each is a share of the pool the engine reports (never the
            // slot's `n_ctx`, which the fork caps at the trained window, halving the IntelMac's
            // share, 2026-11-03); on fixed slots, its slot. An unreadable geometry publishes the gap
            // and retries next tick.
            let geometry = match served_window > 0 {
                false => Err("no served window yet".to_string()),
                true => server
                    .served_geometry()
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|g| g.pool_tokens().zip(g.sequence_tokens()).ok_or_else(|| "geometry past the grants' token width".to_string())),
            };
            let (served_window, served_lanes) = match geometry {
                Ok((pool, sequence)) => {
                    served_pool.store(pool, Ordering::Relaxed);
                    served_sequence.store(sequence, Ordering::Relaxed);
                    crate::inference::proven_pool::observe(
                        &target.model.id,
                        u64::from(pool),
                        crate::inference::proven_pool::PoolContract::running(),
                    );
                    match server.pool_launch().map(|launch| launch.kv_shared()) {
                        Some(true) => sequence_share(pool, served_lanes, target.model.context_window),
                        _ => (served_window, served_lanes),
                    }
                }
                Err(e) => {
                    continuum_base::probe!(
                        target: "continuum_core::modules::serving_daemon", class = "serving.reconcile",
                        desired = desired.as_str(),
                        error = %e,
                        "server ready but its geometry is unreadable: publishing the gap (no guessed \
                         pool or limit; retries next tick)",
                    );
                    served_pool.store(0, Ordering::Relaxed);
                    served_sequence.store(0, Ordering::Relaxed);
                    (0, served_lanes)
                }
            };
            // The grant the engine was LAUNCHED with: the spawn fact on the control, or — for
            // a lane this core did not spawn (ADOPTED; every deploy leaves the lane up for
            // the next core, so this is the common case, not the rare one) — read off the
            // engine's own argv. 0 only when neither can say (the measurement then withholds).
            let launched_cache_mib = server
                .launched_prompt_cache_mib()
                .or_else(adopted_prompt_cache_mib)
                .unwrap_or(0); // unwrap_or: neither the launch nor the adopted argv names it; the measurement withholds
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
                             decision wants — `--cache-ram` is fixed at spawn and the engine is \
                             never relaunched for a size (one launch, 2026-11-03), so the gap \
                             stands until its next launch; the cache becomes a runtime grant \
                             with the seat API. Read `below_one_conversation`: true means the \
                             grant cannot retain a single prefix and reuse is structurally \
                             impossible, not merely poor"
                        ),
                        None => continuum_base::probe!(
                            target: "continuum_core::modules::serving_daemon", class = "serving.prompt_cache.divergence_closed",
                            model = target.model_id(),
                            held_mib = launched_cache_mib as u64,
                            wanted_mib = target.host_prompt_cache_mib as u64,
                            was_held_mib = previous.as_ref().map(|(_, h, _, _)| *h).unwrap_or(0) as u64, // unwrap_or: no prior state recorded
                            was_wanted_mib = previous.as_ref().map(|(_, _, w, _)| *w).unwrap_or(0) as u64, // unwrap_or: as above
                            "the prompt-cache divergence closed — the engine's grant now meets \
                             today's decision (a new launch, or the target fell back to it)"
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
                            .unwrap_or_default(); // unwrap_or_default: no registry yet holds no sidecar rows
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
                    let main_lane_wears = server.mmproj_on_lane().unwrap_or_else(|| { // unwrap_or_else: unknown reads the readiness truth below
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
                active = snapshot.active_model.as_deref().unwrap_or("<none>"), // probe field: no active model
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

/// The pool an engine launches with: the most memory the host's measured capacity proves fits.
/// That is the fit at `sequences` (a window within `budget_bytes` for each, capped at the trained
/// window) or the plan's own window x its lanes, whichever is larger, and never less than what the
/// seated minds need while this node has proven it can hold that ([`Seated`]).
///
/// The floor is the relaunch fix (the M5, 2026-11-07 16:14Z, card 9ffcb1b5): the engine aborted, its
/// own bytes read 0, so the plan's budget was a gap reading (usable 0) and the pool fell to the plan's
/// window x 2 lanes, 176,640 split over 5 sequences = 35,328 each, while the five minds needed 44-70k.
/// Every mind was refused for 45 minutes. Their needs had survived the abort (the working set); only
/// the launch ignored them. Capped by the larger of the live fit and the pool this model last ran
/// healthy at, so the floor never asks for more than this node has already held.
///
/// `budget_bytes` is the published plan's live budget. `--kv-unified` allocates the whole pool up
/// front and one launch keeps it, so the budget must be what memory holds now: the host's physical
/// total ignores everything else resident (590df4bc3: 5 x 262,144 on the M5, memory pressure 91%,
/// rolled back), and a cold boot's first-tick plan is a transient (b4be7bc59: 5 x 2,048, rolled
/// back), which planning no longer publishes. On the M5 (plan 2 x 128,768) the result is 257,536 for
/// 5; on the IntelMac (plan 2 x 32,768, fit 9 x 32,768) 294,912 for 9, its healthy geometry.
pub(super) fn launch_pool(
    footprint: Option<&crate::inference::planning::serving_plan::ModelFootprint>,
    budget_bytes: u64,
    plan_window: u32,
    lanes: u32,
    sequences: u32,
    trained_window: u32,
    seated: Seated,
) -> u64 {
    let plan_pool = u64::from(plan_window) * u64::from(lanes);
    let fit_pool = footprint.map_or(0, |fp| {
        u64::from(fp.window_within(budget_bytes, sequences).min(trained_window)) * u64::from(sequences)
    }); // map_or: no footprint proves no fit beyond the plan's own pool
    let held = seated.need.min(seated.proven.map_or(fit_pool, |proven| fit_pool.max(proven))); // map_or: nothing proven leaves the budget's fit alone
    plan_pool.max(fit_pool).max(held)
}

/// What the seated minds need of the pool, and what this node has proven it can hold for the model:
/// the launch's floor ([`launch_pool`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Seated {
    /// The sum of the seated minds' HELD needs (what a held turn measured), each capped at the model's
    /// trained window (a need above it is a measurement defect, card bf3b6d68, never a size). A mind
    /// with no held turn adds nothing: her wish is a target with headroom, not a need.
    pub(super) need: u64,
    /// The largest pool this model ran at under the engine and KV plan the next launch serves on
    /// (`proven_pool`, kept across core restarts), counted only while the live budget is a gap
    /// reading (no engine credited, e.g. after an exit): a steady budget's fit is the live truth, and
    /// a pool proven earlier never overrides it. `None` when steady or nothing is proven.
    pub(super) proven: Option<u64>,
}

impl ServingDaemonModule {
    /// The launch floor for `model`: the seated minds' needs (the working set keeps them across an
    /// engine exit), and while the budget is a gap reading, the largest pool this model ran at.
    pub(super) fn seated_for(&self, model: &crate::model_registry::types::Model) -> Seated {
        let need = self.seated_needs(model).iter().sum();
        let steady = matches!(*self.planned_budget.lock(), Some(PlannedBudget { credit: OwnCredit::Included { .. }, .. }));
        let proven = match steady {
            true => None,
            false => crate::inference::proven_pool::proven(&model.id),
        };
        Seated { need, proven }
    }

    /// Each seated mind's HELD need on `model`, capped at its trained window: the residents a boot
    /// remembers before its registry exists, else the live roster (`seated_requirements`).
    pub(super) fn seated_needs(&self, model: &crate::model_registry::types::Model) -> Vec<u64> {
        let trained = u64::from(model.context_window);
        self.seated_requirements()
            .iter()
            .filter_map(|requirement| match requirement.measure {
                // Only a need a held turn measured: a wish's target carries headroom and untrimmed
                // overflow (Fable, 2026-11-07: Aiko's 902k on a 262k model), never a size to launch.
                Measure::Held { need, .. } => Some(u64::from(need.value().get()).min(trained)),
                Measure::Unmeasured | Measure::Wished { .. } | Measure::Declared { .. } => None,
            })
            .collect()
    }
}
