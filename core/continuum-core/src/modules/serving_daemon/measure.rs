//! MEASURE: the active artifact, engine occupancy and device credit, lane footprint samples, the MoE host-cache lease.
//! Split out of serving_daemon.rs (card dbc9711c, BigMama's step 6).

use super::*;

impl ServingDaemonModule {
    /// #287 slice 2 — the governed host-cache lease, derived LIVE on the tick and
    /// published to the per-port plan file her `ResidencyCache` mtime-polls. This is
    /// the loop-closer that retires the `GGML_MOE_HOST_CACHE_GB=40` scratchpad
    /// constant: the budget is arithmetic over the serve's REAL working set
    /// ([`host_cache_lease_bytes`](crate::capacity::host_cache_lease::host_cache_lease_bytes)),
    /// re-derived every tick because KV grows as slots fill, and published through
    /// the sticky band so sub-band flutter never churns the file. Publishes only for
    /// a READY MoE serve (the moe context exists for the active model) — a dense
    /// model or a warming lane writes nothing.
    /// Track the resident model's on-disk artifact in the process-wide
    /// active set (#302 invariant 1). Registered BEFORE spawn (the pool
    /// must not migrate a GGUF mid-load), swapped on model change (old
    /// registration released exactly once), cleared when nothing serves.
    /// Prefix containment in [`ActiveArtifactSet::protects`] means the
    /// file path protects its per-model dir and vice versa, whichever
    /// layout the artifact uses.
    pub(super) fn set_active_artifact(&self, path: Option<std::path::PathBuf>) {
        Self::set_active_artifact_in(&self.active_artifact, path);
    }

    pub(super) fn set_active_artifact_in(
        active: &std::sync::Mutex<Option<std::path::PathBuf>>,
        path: Option<std::path::PathBuf>,
    ) {
        let Ok(mut current) = active.lock() else {
            return; // poisoned: the set fails SAFE (protects everything)
        };
        if *current == path {
            return;
        }
        let set = crate::system_resources::serving_active_artifacts();
        if let Some(old) = current.take() {
            set.release(&old);
        }
        if let Some(new) = path {
            set.register(new.clone());
            *current = Some(new);
        }
    }

    /// Once a minute, read the live lane's anonymous footprint off its process and feed
    /// the per-token cost back to the plan (`inference::lane_footprint`). One probe per
    /// sample carries predicted vs measured so the plan's arithmetic is checked against
    /// the process it launched, every minute, on every box.
    /// While no lane is ready, the device's sampled used-bytes are the trough the
    /// discrete footprint arm subtracts: take the minimum seen since the spawn armed it.
    pub(super) fn lower_spawn_baseline_to_the_trough(&self) {
        if self.serving_tx.borrow().ready {
            return;
        }
        let now = vram_physical_used(&self.resource_daemon);
        if now == 0 {
            return;
        }
        self.spawn_baseline_vram.fetch_min(now, Ordering::Relaxed);
    }

    /// THE QUESTION (SHARED-RESIDENT-LIFECYCLE.md step 1): may serving replace or measure the
    /// live engine right now? Live work bound to this very engine incarnation (a training run
    /// admitted onto it) holds it; an unreadable record holds it too, because ownership that
    /// cannot be read is not absence. No live engine is nothing to disturb.
    pub(super) fn engine_occupancy(&self) -> crate::inference::engine_residency::Occupancy {
        use crate::inference::engine_residency::{occupancy, store_path, Occupancy};
        let Some(rec) = (self.inherited_lane)() else {
            return Occupancy::Free;
        };
        match crate::paths::continuum_home() {
            Ok(home) => occupancy(&store_path(&home), rec.port),
            Err(e) => Occupancy::Unknown(format!("no home to read the engine's residency from: {e}")),
        }
    }

    /// Record what the serving engine holds on a discrete GPU, for the board's credit to
    /// serving (`EngineDeviceCredit`, card 741f5eec). Off a discrete GPU the host-process read
    /// is the right quantity, so the reading says so. No live lane, or an unreadable `/props`,
    /// records no credit: the gap between engines is 0, never a guess.
    pub(super) async fn refresh_engine_device_credit(&self) {
        let discrete_gpu = matches!(
            (crate::inference::llama_server::main_lane_placement(), self.system.gpu_memory_mode()),
            (
                crate::inference::llama_server::LanePlacement::Gpu,
                Some(crate::gpu::monitor::MemoryMode::Discrete)
            )
        );
        if !discrete_gpu {
            self.engine_device_credit.record_not_discrete();
            return;
        }
        let pid = crate::inference::lane_pidfile::read();
        let bytes = match pid {
            Some(_) => engine_device_bytes().await,
            None => None,
        };
        if !self.engine_device_credit.record_discrete(pid, bytes) {
            return; // a steady engine: same reading, no probe (it allocates its buffers at launch)
        }
        crate::probe!(
            class = "serving.credit.engine_device",
            pid = pid.map_or(0, u64::from),
            device_bytes = bytes.unwrap_or(0), // unwrap_or: probe label; `read` says whether a reading exists
            read = bytes.is_some(),
            "serving's board credit on a discrete GPU changed: what its engine reports holding on the device"
        );
    }

    pub(super) async fn sample_lane_footprint(&self) {
        // The board's credit first, every tick and unconditionally. The per-token reading
        // below is taken once a minute and withheld while other work lives in the engine,
        // but the engine's device holding is serving's own the whole time. A once-a-minute
        // credit would read a freshly relaunched engine as nobody's for up to a minute, which
        // is exactly when a relaunch is sized. `/props` is one local call per tick.
        self.refresh_engine_device_credit().await;
        let now = EpochMs::now().millis();
        if !crate::inference::lane_footprint::sample_due(now) {
            return;
        }
        // THE FOOTPRINT IS SERVING'S ONLY WHEN NOTHING ELSE LIVES IN THE ENGINE (SHARED-RESIDENT-
        // LIFECYCLE.md step 2). On the 5090 (2026-09-28 12:29:07) the lane's reading carried a live
        // /train run's graph, optimizer and adapter, ~7.4 GB that the record then treated as fixed
        // per-lane serving residency; the 27B stopped fitting and the plan replaced the engine
        // the run was in. So while live work is bound to this engine, or its binding cannot be
        // read, the reading is WITHHELD; and for one sample interval after work leaves (the
        // engine frees asynchronously, the device reports late) it is withheld too, as it is for
        // the first interval of a process (a predecessor's release is unknown to it). The release
        // retired a record sampled during the work; a reading in flight across a bind or release
        // is refused when it publishes.
        // The token is taken BEFORE occupancy is read: work that binds or leaves after this
        // point refuses the reading at publication (Codex on #4536), however long it awaits.
        let token = crate::inference::lane_footprint::begin_sample();
        let occupancy = self.engine_occupancy();
        let settling = crate::inference::engine_residency::released_within(now, crate::inference::lane_footprint::SAMPLE_EVERY_MS);
        if occupancy.holds() || settling {
            crate::probe!(
                class = "serving.footprint.unmeasured",
                leg = if occupancy.holds() { "resident_work" } else { "resident_work_settling" },
                occupancy = %format!("{occupancy:?}"),
                "the engine hosts (or just released) work that is not serving: its footprint is not \
                 serving cost, so the per-token reading is WITHHELD"
            );
            return;
        }
        let live = self.serving_tx.borrow().clone();
        if !live.ready || live.lanes == 0 || live.served_context_window == 0 {
            return;
        }
        let Some(active) = live.active_model.clone() else {
            return;
        };
        let Some(pid) = crate::inference::lane_pidfile::read() else {
            return;
        };
        let Some(model) = (self.model_resolver)(&active) else {
            return;
        };
        let Some(fp) = footprint_for(&model) else {
            return;
        };
        // THE FOOTPRINT SOURCE IS PER HOST CLASS (Fable, 2026-09-17, converging #4152 and
        // #4164): on unified memory the lane's KV lives in system RAM, so its anonymous
        // process footprint IS the bytes beyond the weights. On a discrete GPU the KV
        // lives in VRAM, a process read misses it entirely, and WDDM attributes VRAM to
        // no process at all (`nvidia-smi --query-compute-apps` prints [N/A] for every
        // pid on the 5090) — so the discrete arm is the DEVICE delta across the launch:
        // used-now minus used-before-spawn, minus the weights (in VRAM on this class).
        // Both arms feed the ONE consumer below; the probe names which one spoke. A
        // CPU-placed lane (#3729) holds nothing on the device whatever the host class,
        // so the device delta would read zero for a lane whose KV fills system RAM —
        // the process read is the one that sees it.
        //
        // ON A DISCRETE GPU THE ENGINE'S OWN KV SPEAKS FIRST (card 03cdeee3; BigMama's 5090
        // receipt and Codex's design, 2026-10-11). The device delta holds every tenant of the
        // GPU (the embedding lane's 1.75 GiB lease) and the engine's unreported overhead; divided
        // by tokens it read 85.8 KB/token where this engine's own context bytes read 46.4 KB, and
        // the plan carried the excess as fixed bytes PER LANE, so every extra lane paid for the
        // other tenants again. The reading is now the engine's KV over the tokens it allocated;
        // what the delta holds beyond everything the engine claims is probed as UNATTRIBUTED and
        // never enters this model's record. The delta still speaks for an engine that predates
        // `memory_breakdown`.
        // On a HYBRID model the context buffer also holds each slot's fixed recurrent state, so the
        // rate is an UPPER bound by that state spread over the slot's tokens, and a plan for more,
        // shorter slots is short one recurrent state per extra slot (BigMama on #4961). Splitting
        // it out needs the engine to report it per sequence: card c8b2e1df.
        let placement = crate::inference::llama_server::main_lane_placement();
        let (source, beyond_weights, allocated) = match (placement, self.system.gpu_memory_mode()) {
            (
                crate::inference::llama_server::LanePlacement::Gpu,
                Some(crate::gpu::monitor::MemoryMode::Discrete),
            ) => {
                let baseline = self.spawn_baseline_vram.load(Ordering::Relaxed);
                let now = vram_physical_used(&self.resource_daemon);
                let delta = device_delta_beyond_weights(baseline, now, fp.weights_bytes);
                // An engine that did not ANSWER is an unavailable observation, not an engine
                // without the field: falling to the delta there would write the tenant-charged
                // rate back under the same record on one 500 ms miss (Codex on #4961). Only an
                // engine that answered WITHOUT `memory_breakdown` (one predating it) is measured
                // by the delta; a breakdown or geometry that is present but unusable is no reading.
                let Some(props) = engine_props().await else {
                    crate::probe!(
                        class = "serving.footprint.unmeasured",
                        leg = "engine_unanswered",
                        model = %active,
                        pid = pid as u64,
                        "the engine's /props did not answer in time: no reading this sample, and the device delta does not stand in for it"
                    );
                    return;
                };
                use crate::inference::weight_residency::EngineContextReading;
                match (EngineContextReading::of(&props), delta) {
                    (EngineContextReading::Invalid, _) => {
                        crate::probe!(
                            class = "serving.footprint.unmeasured",
                            leg = "engine_breakdown_invalid",
                            model = %active,
                            pid = pid as u64,
                            "the engine reports a memory breakdown, but it or its slot geometry is unusable: no reading this sample, and the device delta does not stand in for it"
                        );
                        return;
                    }
                    (EngineContextReading::Read(ctx), _) => {
                        crate::probe!(
                            class = "serving.footprint.device_residual",
                            model = %active,
                            engine_context_bytes = ctx.context_bytes,
                            engine_beyond_weights = ctx.beyond_weights,
                            device_delta_read = delta.is_some(),
                            device_delta_beyond_weights = delta.unwrap_or(0), // unwrap_or: probe label; `device_delta_read` says whether it exists
                            // absent, never 0: an unread delta is not "nothing unattributed" (BigMama on #4961)
                            unattributed_bytes = %delta.map_or_else(|| "none".to_string(), |d| d.saturating_sub(ctx.beyond_weights).to_string()),
                            "the device's growth beyond the weights against what the engine says it holds: the excess is other tenants or engine overhead, never this model's per-token cost"
                        );
                        ("engine_context", Some(ctx.context_bytes), Some(ctx))
                    }
                    (EngineContextReading::Absent, Some(delta)) => ("device_delta", Some(delta), None),
                    (EngineContextReading::Absent, None) => {
                        crate::probe!(
                            class = "serving.footprint.device_delta_unread",
                            baseline_bytes = baseline,
                            now_bytes = now,
                            weights_bytes = fp.weights_bytes,
                            "neither the engine's breakdown nor a device delta beyond the weights (no baseline, no reading, or a baseline that already held the weights)"
                        );
                        ("device_delta", None, None)
                    }
                }
            }
            _ => (
                "process_anon",
                crate::inference::lane_footprint::anon_footprint_of(pid),
                None,
            ),
        };
        let outcome = match beyond_weights {
            // An UNKNOWN grant withholds the reading (Cormac's condition, 2026-09-20): with 0
            // subtracted the whole prompt cache would read as per-token bytes and be saved
            // as the record — the corrupt 262k reading this measurement produced on the
            // re-home path, reproduced on the deploy path. An absence stays an absence:
            // no `observe`, no record written, the previous record stands.
            Some(_) if live.host_prompt_cache_mib == 0 => {
                crate::probe!(
                    class = "serving.footprint.unmeasured",
                    leg = "unknown_grant",
                    model = %active,
                    pid = pid as u64,
                    source,
                    lanes = live.lanes as u64,
                    window = live.served_context_window as u64,
                    "the engine's cache grant is unknown (an adopted lane whose argv names no \
                     --cache-ram, and no spawn fact) — the per-token reading is WITHHELD rather \
                     than taken with the whole prompt cache attributed as per-token bytes"
                );
                "unmeasured"
            }
            // A STARVED window withholds the reading too: at 2,048 tokens the bytes beyond
            // weights are the engine's fixed buffers, and dividing them by 2,048 reads as
            // hundreds of KB per token — saved, that record fits no window at all and pins
            // the next plan at the floor it fell to (the trap a collapsed node cannot climb
            // out of). Below one full turn's window the measurement is not a measurement.
            // …and RETIRES the model's record (Cormac's condition on #4255): a node already
            // in the trap holds a ~244k B/token record that corrects the plan UP to the
            // floor; left standing behind a withheld reading it rules forever. Retired, the
            // plan falls back to the estimate, plans a real window, and the next honest
            // reading replaces the estimate — the node climbs out without a hand on a file.
            Some(_) if !crate::inference::lane_footprint::window_measurable(live.served_context_window) => {
                let retired = crate::inference::lane_footprint::retire(&active);
                crate::probe!(
                    class = "serving.footprint.unmeasured",
                    leg = "starved_window",
                    model = %active,
                    pid = pid as u64,
                    source,
                    lanes = live.lanes as u64,
                    window = live.served_context_window as u64,
                    floor = crate::inference::lane_footprint::MEASURABLE_WINDOW_MIN as u64,
                    record_retired = retired,
                    "the served window is below one full turn — the per-token reading is WITHHELD \
                     (the excess over weights is fixed buffers, not tokens) and the model's record \
                     is RETIRED so a trap record cannot keep the plan at the floor"
                );
                "unmeasured"
            }
            Some(anon) => {
                // The engine's KV is decomposed over the geometry the ENGINE holds (its slots, a
                // scratch slot included, at its per-slot context) with no compute floor: the
                // reading holds no compute. Every other source is the plan's own decomposition.
                let (lanes, window, compute_floor) = match allocated {
                    Some(ctx) => (ctx.slots, ctx.n_ctx_per_slot, 0),
                    // the BASE buffer: the fixed term is derived from this reading
                    // (`base_compute_buffer_per_lane`, the M5's two-tick flap)
                    None => (live.lanes, live.served_context_window, fp.base_compute_buffer_per_lane()),
                };
                let predicted = match allocated {
                    Some(_) => fp.kv_at(window).saturating_mul(lanes as u64),
                    None => fp
                        .kv_at(window)
                        .saturating_mul(lanes as u64)
                        .saturating_add(fp.prefill_compute_reserve(window, lanes)),
                };
                // The host prompt cache the ENGINE was launched with (`--cache-ram`, on
                // the snapshot as a fact about the process): it fills the same anonymous
                // footprint over the serve's life and is not per-token cost — subtracted
                // before anything is attributed. It used to be read from a cell the
                // target derivation rewrote every tick the plan changed; on 2026-09-20
                // 07:52Z that cell went 14,396 → 256 MiB under a running 2-lane engine
                // and the reading here went 33k → 262k B/token, saved to disk.
                // `--cache-ram` lives in HOST RAM: only a process reading contains it. The device
                // readings (device_delta, engine_context) never saw it, so subtracting it there
                // would undercount the per-token cost or retire the record (Cormac on #4459).
                let host_cache_bytes = if source == "process_anon" {
                    (live.host_prompt_cache_mib as u64) * 1024 * 1024
                } else {
                    0
                };
                let measured = crate::inference::lane_footprint::observe(
                    token,
                    &active,
                    lanes,
                    window,
                    anon,
                    compute_floor,
                    host_cache_bytes,
                );
                let Ok(measured) = measured else {
                    crate::probe!(
                        class = "serving.footprint.unmeasured",
                        leg = "residency_changed",
                        model = %active,
                        pid = pid as u64,
                        source,
                        "work bound to or left the engine while this reading was taken: it may \
                         straddle that work, so it is DROPPED and the record stands"
                    );
                    return;
                };
                crate::probe!(
                    class = "serving.footprint.measured",
                    model = %active,
                    pid = pid as u64,
                    source,
                    lanes = lanes as u64,
                    window = window as u64,
                    anon_bytes = anon,
                    host_cache_bytes,
                    predicted_beyond_weights = predicted,
                    ratio = anon as f64 / predicted.max(1) as f64,
                    per_token_estimate = fp.kv_per_token,
                    // absent, never 0: a reading that yields no per-token rate is not a rate of
                    // zero (card b5230492: 0 printed every minute read like a measurement)
                    per_token_measured = %measured.map_or_else(|| "none".to_string(), |rate| rate.to_string()),
                    "the lane's footprint (its process, or on a discrete GPU the engine's own KV) against the plan's arithmetic"
                );
                "read"
            }
            None => "could_not_look",
        };
        if outcome == "could_not_look" {
            crate::probe!(
                class = "serving.footprint.measured",
                model = %active,
                pid = pid as u64,
                source,
                outcome,
                "the lane's footprint could not be read: no process reading on this host class, \
                 or no spawn baseline (an adopted lane) for the device delta"
            );
        }
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
            .unwrap_or(0)  // unwrap_or: no governed ceiling publishes as ABSENT (see above), never as a zero budget
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
                repeat_recall_x100 = repeat_recall.unwrap_or(0) as u64,  // unwrap_or: probe display only, 0 = not measured this tick; no decision reads it
                predicted_delta_recall_x100 = delta_recall.unwrap_or(0) as u64,  // unwrap_or: probe display only, 0 = not measured this tick; no decision reads it
                schedulable_coverage_x100 = coverage.unwrap_or(0) as u64,  // unwrap_or: probe display only, 0 = not measured this tick; no decision reads it
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
}
