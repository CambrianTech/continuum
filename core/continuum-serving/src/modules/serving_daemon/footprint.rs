//! Sampling the live serving engine's physical footprint.

use super::*;

impl ServingDaemonModule {
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
            continuum_base::probe!(
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
            (crate::inference::llama_server::LanePlacement::Gpu, Some(crate::gpu::monitor::MemoryMode::Discrete)) => {
                let baseline = self.spawn_baseline_vram.load(Ordering::Relaxed);
                let now = vram_physical_used(&self.resource_daemon);
                let delta = device_delta_beyond_weights(baseline, now, fp.weights_bytes);
                // An engine that did not ANSWER is an unavailable observation, not an engine
                // without the field: falling to the delta there would write the tenant-charged
                // rate back under the same record on one 500 ms miss (Codex on #4961). Only an
                // engine that answered WITHOUT `memory_breakdown` (one predating it) is measured
                // by the delta; a breakdown or geometry that is present but unusable is no reading.
                let Some(props) = engine_props().await else {
                    continuum_base::probe!(
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
                        continuum_base::probe!(
                            class = "serving.footprint.unmeasured",
                            leg = "engine_breakdown_invalid",
                            model = %active,
                            pid = pid as u64,
                            "the engine reports a memory breakdown, but it or its slot geometry is unusable: no reading this sample, and the device delta does not stand in for it"
                        );
                        return;
                    }
                    (EngineContextReading::Read(ctx), _) => {
                        continuum_base::probe!(
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
                        continuum_base::probe!(
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
            _ => ("process_anon", crate::inference::lane_footprint::anon_footprint_of(pid), None),
        };
        let outcome = match beyond_weights {
            // An UNKNOWN grant withholds the reading (Cormac's condition, 2026-09-20): with 0
            // subtracted the whole prompt cache would read as per-token bytes and be saved
            // as the record — the corrupt 262k reading this measurement produced on the
            // re-home path, reproduced on the deploy path. An absence stays an absence:
            // no `observe`, no record written, the previous record stands.
            Some(_) if live.host_prompt_cache_mib == 0 => {
                continuum_base::probe!(
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
                continuum_base::probe!(
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
                // The engine's KV is decomposed over its physical token capacity,
                // independently of its sequence limits, with no compute floor: the
                // reading holds no compute. Every other source is the plan's own decomposition.
                let (lanes, window, allocated_tokens, compute_floor) = match allocated {
                    Some(ctx) => (ctx.slots, ctx.n_ctx_per_slot, ctx.allocated_tokens, 0),
                    // the BASE buffer: the fixed term is derived from this reading
                    // (`base_compute_buffer_per_lane`, the M5's two-tick flap)
                    None => (
                        live.lanes,
                        live.served_context_window,
                        u64::from(live.lanes) * u64::from(live.served_context_window),
                        fp.base_compute_buffer_per_lane(),
                    ),
                };
                let predicted = match allocated {
                    Some(_) => fp.kv_per_token.saturating_mul(allocated_tokens),
                    None => fp.kv_at(window).saturating_mul(lanes as u64).saturating_add(fp.prefill_compute_reserve(window, lanes)),
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
                let host_cache_bytes = if source == "process_anon" { (live.host_prompt_cache_mib as u64) * 1024 * 1024 } else { 0 };
                let measured =
                    crate::inference::lane_footprint::observe(token, &active, lanes, window, allocated_tokens, anon, compute_floor, host_cache_bytes);
                let Ok(measured) = measured else {
                    continuum_base::probe!(
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
                continuum_base::probe!(
                    class = "serving.footprint.measured",
                    model = %active,
                    pid = pid as u64,
                    source,
                    lanes = lanes as u64,
                    window = window as u64,
                    allocated_tokens,
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
            continuum_base::probe!(
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
}
