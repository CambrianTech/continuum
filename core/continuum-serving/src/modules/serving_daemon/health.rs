//! Health ownership for the serving daemon.

use super::*;

/// Slow liveness HEARTBEAT cadence (#175 self-heal): reconcile ticks between decode
/// smoke-probes of the LIVE lane. The reconcile fast-path trusts the published `ready`
/// snapshot and never re-probes a child it owns ([`crate::inference::llama_server`] —
/// "trusted thereafter, no per-tick decode load"), so a Metal-GPU-OOM-POISONED backend —
/// which answers every control-plane read (`/health`, `/v1/models`, `/props`) with 200
/// while every `llama_decode` 500s (`kIOGPUCommandBufferCallbackErrorOutOfMemory`) — stays
/// `ready:true` FOREVER and the persona substrate is bricked until a human reboots. This
/// heartbeat re-verifies the actual COMPUTE path on a cadence far slower than [`TICK`], so
/// it costs one tiny generation per minute, not one per tick.
pub(super) const HEALTH_PROBE_EVERY_TICKS: u64 = 12; // 12 × 5s ≈ 60s

/// Consecutive failed heartbeats before the lane is declared WEDGED and flipped
/// not-ready (which the reconcile turns into a kill+respawn). Pure hysteresis: ONE failed
/// probe can be a merely-BUSY lane (a saturated slot times out the 10s decode probe during
/// a wake burst), and reaping a healthy-but-busy lane is the exact thrash that got the
/// prior watchdog reverted (commit 93753d812). Two consecutive failures ≈ 2 minutes of
/// sustained no-decode = genuinely poisoned, not transiently loaded.
/// How long work may stop advancing on a slot the engine still reports as processing before
/// the lane is condemned as frozen. derived-or-floor: floor — measured on the M5 2026-10-17
/// the engine logs a prefill or decode advance every ~3 s while working, and the longest
/// cold prefill seen that day (23,612 tokens at 105 t/s) still advanced continuously; a
/// frozen engine advanced nothing for 34 min. Three minutes is 60x the healthy gap.
pub(super) const FROZEN_ENGINE_MS: u64 = 180_000;

pub(super) struct FrozenSlots {
    pub(super) generation: uuid::Uuid,
    pub(super) fingerprint: u64,
    pub(super) unchanged_since: std::time::Instant,
}

impl FrozenSlots {
    pub(super) fn observe(
        last: &mut Option<Self>,
        generation: uuid::Uuid,
        slots: Option<&serde_json::Value>,
        now: std::time::Instant,
    ) -> bool {
        let fingerprint = slots.and_then(|slots| {
            let active: Vec<_> = slots
                .as_array()?
                .iter()
                .filter(|s| s.get("is_processing").and_then(|v| v.as_bool()) == Some(true))
                .collect();
            if active.is_empty()
                || active.iter().any(|s| {
                    s.get("id").and_then(|v| v.as_u64()).is_none()
                        || s.get("id_task")
                            .and_then(|v| v.as_i64())
                            .is_none_or(|id| id < 0)
                        || s.get("n_prompt_tokens_processed")
                            .and_then(|v| v.as_u64())
                            .is_none()
                        || s.get("next_token")
                            .and_then(|v| v.get("n_decoded").or_else(|| v.get(0)?.get("n_decoded")))
                            .and_then(|v| v.as_u64())
                            .is_none()
                })
            {
                return None;
            }
            crate::inference::llama_server::slots_activity_fingerprint_of(slots)
        });
        let Some(fingerprint) = fingerprint else {
            *last = None;
            return false;
        };
        match last {
            Some(previous)
                if previous.generation == generation && previous.fingerprint == fingerprint =>
            {
                now.duration_since(previous.unchanged_since).as_millis()
                    >= u128::from(FROZEN_ENGINE_MS)
            }
            _ => {
                *last = Some(Self {
                    generation,
                    fingerprint,
                    unchanged_since: now,
                });
                false
            }
        }
    }
}

pub(super) const HEALTH_FAILS_TO_RELAUNCH: u8 = 2;

impl ServingDaemonModule {
    #[cfg(test)]
    pub(super) fn set_decode_age_source(&mut self, source: DecodeAgeSource) {
        self.decode_age = source;
    }

    /// Test seam: own the heartbeat's real-turn-failure evidence (#363) instead of
    /// inheriting the process-global stamped by other tests' adapters. Same rationale
    /// as [`Self::set_decode_age_source`].
    #[cfg(test)]
    pub(super) fn set_real_fails_source(&mut self, source: RealFailsSource) {
        self.real_fails = source;
    }

    /// The liveness HEARTBEAT (#175 self-heal) — see also [`judge_smoke_miss`] (L11),
    /// which decides whether a missed probe is contention or wedge evidence.
    /// On a cadence far slower than [`TICK`],
    /// decode-probe the LIVE lane the daemon currently believes is `ready`. The reconcile
    /// fast-path trusts that published `ready` and never re-probes a child it owns, so a
    /// Metal-GPU-OOM-POISONED backend — which answers every control-plane read (`/health`,
    /// `/v1/models`, `/props`) with 200 while every `llama_decode` 500s
    /// (`kIOGPUCommandBufferCallbackErrorOutOfMemory`) — stays `ready:true` forever and the
    /// persona substrate is bricked until a human reboots. This re-verifies the actual
    /// COMPUTE path and, after [`HEALTH_FAILS_TO_RELAUNCH`] CONSECUTIVE failures (hysteresis
    /// so a merely-busy lane is never reaped), flips the published snapshot to not-ready.
    /// The very next [`Self::reconcile_to_plan`] sees `ready == false`, skips its no-op
    /// guard, and kill+respawns the lane — the ONLY recovery llama.cpp offers ("recreate
    /// the backend to recover"). Runs off the tick as its own bounded task so a slow decode
    /// never stalls the 5s tick; returns the handle so tests can await it. `None` when it
    /// isn't a heartbeat tick, nothing ready is believed live, or a reconcile/probe is
    /// already in flight (never race the reconcile's own kill/swap).
    /// The prefill knee's server-total rate, observe-only (card e370a673): one bounded
    /// control-plane read of `/slots` per health tick, on its own task. It must read a BUSY
    /// lane: the rate only counts intervals with prompt left to prefill, and a lane proven
    /// alive by real work skips the smoke probe below. Read inside that probe, it ran only
    /// on idle lanes and never published (the M5, 2026-09-27: zero `would_clamp` rows over
    /// a saturated afternoon). Not started while a reconcile is in flight, and never stacked.
    /// A reconcile that begins after the read started can replace the engine under it; the
    /// window discards that interval itself (a task counter going backwards, or a read gap
    /// past `MAX_READ_GAP_MS`), so a sample never spans an engine replacement.
    pub(super) fn spawn_prefill_knee_read(&self) -> Option<JoinHandle<()>> {
        if self.reconciling.load(Ordering::Acquire)
            || self.prefill_knee_reading.swap(true, Ordering::AcqRel)
        {
            return None;
        }
        let server = self.server.clone();
        let reading = self.prefill_knee_reading.clone();
        // the model the rates are measured FOR; with the engine's pid (read on the task), two
        // reads pair only on the same engine process
        let model = self.serving_tx.borrow().active_model.clone();
        Some(tokio::spawn(async move {
            // released however the read ends, so a failed read cannot stop the next one
            struct ReadDone(Arc<AtomicBool>);
            impl Drop for ReadDone {
                fn drop(&mut self) {
                    self.0.store(false, Ordering::Release);
                }
            }
            let _done = ReadDone(reading);
            let engine = model
                .zip(crate::inference::lane_pidfile::read())
                .map(|(model, pid)| crate::inference::prefill_knee::EngineRead { model, pid });
            if let Some(slots) = server.slots_body().await {
                crate::inference::prefill_knee::observe_slots(
                    &slots,
                    EpochMs::now().millis(),
                    crate::inference::resource_admission::served_lane_count(),
                    engine,
                );
            }
        }))
    }

    pub(super) fn spawn_health_heartbeat_if_due(&self) -> Option<JoinHandle<()>> {
        // Slow-cadence gate: only every Nth tick runs a probe.
        if self.health_ticks.fetch_add(1, Ordering::Relaxed) % HEALTH_PROBE_EVERY_TICKS != 0 {
            return None;
        }
        // Only meaningful when we BELIEVE we have a ready live lane to verify. Not-ready
        // (booting / mid-relaunch) → nothing to probe; reset the streak so a fresh lane
        // that becomes ready starts clean.
        let believe_ready = {
            let s = self.serving_tx.borrow();
            s.ready && s.active_model.is_some()
        };
        if !believe_ready {
            *self.frozen_slots.lock() = None;
            self.health_fails.store(0, Ordering::Relaxed);
            return None;
        }
        let _ = self.spawn_prefill_knee_read();
        // #363: SUSTAINED REAL-TURN FAILURE OUTRANKS EVERY TRUST PATH BELOW. The
        // 2026-08-07 blackout (25 min, every citizen turn dead, serving/status
        // ready:true throughout) was a wedge class neither trust path can see:
        // an undersized/mid-stream-dying lane REJECTS the fleet's real prompts
        // while (a) partial streams may stamp recent-decode trust and (b) a tiny
        // 1-token smoke probe still PASSES — "can generate" is not "can serve the
        // actual working set". Real failures are the honest evidence, stamped at
        // the adapter's local-lane error paths, and the same threshold as the
        // decode heartbeat keeps one hysteresis constant, not two.
        let real_fails = (self.real_fails)();
        if real_fails >= HEALTH_FAILS_TO_RELAUNCH as u64 {
            crate::inference::llama_server::reset_real_decode_failures();
            self.health_fails.store(0, Ordering::Relaxed);
            continuum_base::probe!(
                target: "continuum_core::modules::serving_daemon", class = "serving.health",
                ok = false,
                via = "real_turn_failures",
                consecutive = real_fails,
                threshold = HEALTH_FAILS_TO_RELAUNCH as u64,
                "consecutive REAL generations failed on the live lane — wedge evidence \
                 that outranks a passing smoke probe (#363 undersized/mid-stream class)",
            );
            Self::declare_lane_wedged(
                &self.force_relaunch,
                &self.serving_tx,
                self.bus.get(),
                "sustained real-turn failures (smoke probe may still pass)",
            );
            return None;
        }
        // DELIVERY BEATS PROBING. `decode_smoke_ok` is a real multi-token generation through
        // the LIVE slots, so it competes for the same slots as actual work. A lane saturated by
        // long prefills cannot hand it a slot, the miss reads as "no decode", and
        // HEALTH_FAILS_TO_RELAUNCH misses relaunch a lane that was never wedged — just busy.
        // Glass-boxed on the SWE bench (v13): health failed twice mid-run and the relaunch left
        // every downstream generate refusing with `serving: <none>`, killing the run.
        //
        // If a real generation produced tokens within this probe interval, the compute path is
        // PROVEN alive by work that already happened — a synthetic probe can add nothing and
        // can only steal a slot from the thing proving it. Probe QUIET lanes; trust busy ones.
        // The window is the probe cadence itself (no second constant to drift), and a fresh
        // boot with no observed decode yet falls through and probes as before.
        // [[a-benchmark-zero-is-a-claim-about-the-harness-until-proven-otherwise]]
        let probe_window_ms = TICK.as_millis() as u64 * HEALTH_PROBE_EVERY_TICKS;
        if let Some(since) = (self.decode_age)() {
            if since <= probe_window_ms {
                *self.frozen_slots.lock() = None;
                self.health_fails.store(0, Ordering::Relaxed);
                continuum_base::probe!(
                    target: "continuum_core::modules::serving_daemon", class = "serving.health",
                    ok = true,
                    via = "real_work",
                    ms_since_work = since,
                    "lane proven alive by real work (decode OR prefill advance — L9: a lane \
                     saturated in prefill produces no tokens but is NOT quiet) — skipped the \
                     smoke probe rather than contend for a slot with the work that proves it",
                );
                return None;
            }
        }
        // Adapter stamps miss inherited work. A processing flag is only a baseline:
        // the same engine and task progress must remain unchanged over the interval.
        // The M5's verified 34-minute freeze motivated this; its cause is unproven.
        let frozen_age = (self.decode_age)();
        if !frozen_age.is_some_and(|age| age >= FROZEN_ENGINE_MS)
            || self.reconciling.load(Ordering::Acquire)
        {
            *self.frozen_slots.lock() = None;
        }
        if let Some(since) = frozen_age {
            if since >= FROZEN_ENGINE_MS && !self.reconciling.load(Ordering::Acquire) {
                let root = self
                    .serving_tx
                    .borrow()
                    .base_url
                    .strip_suffix("/v1")
                    .map(str::to_owned);
                if let Some(root) = root {
                    let directory = crate::inference::slots::directory();
                    let endpoint = directory.endpoint(&root);
                    if let Some(generation) = directory
                        .page_discovery_generation(&root)
                        .filter(|_| endpoint.is_ready())
                    {
                        if self.health_probing.swap(true, Ordering::AcqRel) {
                            return None;
                        }
                        let server = self.server.clone();
                        let serving_tx = self.serving_tx.clone();
                        let force_relaunch = self.force_relaunch.clone();
                        let reconciling = self.reconciling.clone();
                        let health_probing = self.health_probing.clone();
                        let observation = self.frozen_slots.clone();
                        let decode_age = self.decode_age.clone();
                        let bus = self.bus.get().cloned();
                        return Some(tokio::spawn(async move {
                            let slots = server.slots_body().await;
                            let current = {
                                let state = serving_tx.borrow();
                                state.ready
                                    && state.active_model.is_some()
                                    && state.base_url.strip_suffix("/v1") == Some(root.as_str())
                                    && !reconciling.load(Ordering::Acquire)
                                    && endpoint.is_ready()
                                    && directory.page_discovery_generation(&root)
                                        == Some(generation)
                                    && decode_age().is_some_and(|age| age >= FROZEN_ENGINE_MS)
                            };
                            let frozen = if current {
                                FrozenSlots::observe(
                                    &mut observation.lock(),
                                    generation,
                                    slots.as_ref(),
                                    std::time::Instant::now(),
                                )
                            } else {
                                *observation.lock() = None;
                                false
                            };
                            if frozen {
                                continuum_base::probe!(target: "continuum_core::modules::serving_daemon", class = "serving.health", ok = false, via = "frozen_processing_slot",
                                    unchanged_ms = FROZEN_ENGINE_MS,
                                    "same engine and processing task progress remained unchanged across the observation interval");
                                Self::declare_lane_wedged(&force_relaunch, &serving_tx, bus.as_ref(), "frozen: unchanged processing task progress across the observation interval");
                            }
                            health_probing.store(false, Ordering::Release);
                        }));
                    }
                }
                *self.frozen_slots.lock() = None;
            }
        }
        // NEVER DECODE ON A RESIDENT'S ONLY SLOT (card 9e3350bd, the 5090 2026-10-11): with no
        // scratch slot the smoke probe lands on the slot a resident's live KV occupies; the
        // 72-token prompt replaced a 53,980-token conversation and the next save wrote that as
        // her page, so every restore after it re-prefilled from ~0. A quiet lane with a seated
        // resident is not probed: a wedge there surfaces on her next real turn, which the
        // real-turn-failure arm above already condemns.
        // This check is the CHEAP early-out; the authority is the slot permit taken in the spawned
        // probe below (Codex on #4960: a check here races a turn that seats a resident before the
        // probe's POST).
        let live_root = self
            .serving_tx
            .borrow()
            .base_url
            .strip_suffix("/v1")
            .map(str::to_string);
        let live_pool = live_root
            .as_deref()
            .and_then(|root| crate::inference::slots::directory().get(root))
            .flatten();
        if let Some(pool) = live_pool.as_ref() {
            if pool.smoke_would_overwrite_a_resident() {
                continuum_base::probe!(
                    target: "continuum_core::modules::serving_daemon", class = "serving.health",
                    via = "skipped_resident_slot",
                    "no scratch slot and a resident is seated: the smoke probe would overwrite her KV and \
                     her next saved page, so it is skipped; her next real turn proves the lane"
                );
                return None;
            }
        }
        // Never race the reconcile's own spawn/kill/swap, and never stack heartbeat probes.
        if self.reconciling.load(Ordering::Acquire) {
            return None;
        }
        if self.health_probing.swap(true, Ordering::AcqRel) {
            return None;
        }
        let server = self.server.clone();
        let serving_tx = self.serving_tx.clone();
        let health_fails = self.health_fails.clone();
        let health_probing = self.health_probing.clone();
        let force_relaunch = self.force_relaunch.clone();
        let last_miss_fp = self.last_miss_slots_fp.clone();
        let bus = self.bus.get().cloned();
        Some(tokio::spawn(async move {
            // A real one-token generation through the live slots — the ONLY probe that
            // distinguishes a healthy lane from an OOM-poisoned one (control-plane reads
            // stay 200 on a wedged backend). `decode_smoke_ok` is already bounded by
            // `DECODE_SMOKE_TIMEOUT`, so a wedged compute path resolves to `false` fast.
            //
            // THE SLOT PERMIT IS THE AUTHORITY (Codex on #4960). On a ONE-slot pool with no
            // scratch slot, the unpinned probe can only land on slot 0, so holding slot 0's
            // operation permit across BOTH smoke attempts (the ThinkStarved retry included)
            // excludes a turn from seating a resident mid-probe, and the resident check is
            // repeated under it. A 2-slot no-scratch pool would need the probe PINNED to the
            // reserved slot; that is card 030ffc01. The pool is read AGAIN here, not
            // taken from before the spawn: a first turn may have installed it since (Codex). An
            // engine whose pool is still undiscovered has seated no turn yet; a first turn that
            // discovers it DURING the probe is the remaining window, open on card 030ffc01 (the
            // fix is the probe going through the shared /props discovery before its POST).
            let live_pool = live_root
                .as_deref()
                .and_then(|root| crate::inference::slots::directory().get(root))
                .flatten();
            let _reserved = match live_pool
                .as_ref()
                .filter(|p| p.scratch_slot().is_none() && p.n_slots() == 1)
            {
                Some(pool) => match pool.acquire_slot(0).await {
                    Ok(permit) if !pool.smoke_would_overwrite_a_resident() => Some(permit),
                    Ok(_) => {
                        continuum_base::probe!(
                            target: "continuum_core::modules::serving_daemon", class = "serving.health",
                            via = "skipped_resident_slot",
                            under_permit = true,
                            "a resident was seated while the probe waited for the slot: skipped under the permit"
                        );
                        health_probing.store(false, Ordering::Release);
                        return;
                    }
                    Err(e) => {
                        continuum_base::probe!(target: "continuum_core::modules::serving_daemon", class = "serving.health", via = "skipped_no_permit", error = %e, "the slot permit could not be taken; no probe this tick");
                        health_probing.store(false, Ordering::Release);
                        return;
                    }
                },
                None => None,
            };
            let ok = server.decode_smoke_ok().await;
            if ok {
                health_fails.store(0, Ordering::Relaxed);
                // A success invalidates the miss-time fingerprint — a stale one must
                // never exonerate a LATER freeze.
                *last_miss_fp.lock().unwrap() = None;
                health_probing.store(false, Ordering::Release);
                return;
            }
            // L11: before counting the miss, ask the server's OWN /slots whether the
            // serve loop advanced since the LAST miss. The adapter stamps (L9) are
            // blind to work done for clients that no longer exist — measured
            // 2026-08-16 boot: an adopted lane grinding through a dead core's ghost
            // turns ate 2 smoke misses and got killed, three times, 23 minutes. A
            // changed fingerprint = queue contention (probe couldn't get a slot
            // because real work held them); frozen = wedge evidence, same threshold
            // and latency as before.
            let cur_fp = server.slots_activity_fingerprint().await;
            let prev_fp = {
                let mut guard = last_miss_fp.lock().unwrap();
                std::mem::replace(&mut *guard, cur_fp)
            };
            if matches!(
                crate::inference::llama_server::judge_smoke_miss(prev_fp, cur_fp),
                crate::inference::llama_server::SmokeMissVerdict::AliveViaSlotProgress
            ) {
                health_fails.store(0, Ordering::Relaxed);
                continuum_base::probe!(
                    target: "continuum_core::modules::serving_daemon", class = "serving.health",
                    ok = true,
                    via = "slot_progress",
                    "smoke probe missed but /slots advanced between misses — the serve \
                     loop is doing real work the adapter stamps can't see (ghost turns, \
                     other clients); contention, not a wedge (L11)",
                );
                health_probing.store(false, Ordering::Release);
                return;
            }
            let n = health_fails.fetch_add(1, Ordering::Relaxed) + 1;
            // Work-scaled patience (2026-08-23, the MirrorCode baseline's three
            // kills): before relaunching, price the kill. A lane holding a
            // window-scale in-flight prompt costs MINUTES of re-prefill to
            // destroy — and the client's retry of the same giant turn can cycle
            // into a kill loop. Large in-flight investment (≥ quarter-window,
            // relative — never an absolute constant) doubles the miss
            // threshold; a quiet lane keeps base patience and still dies fast.
            let inflight = server.slots_max_inflight_prompt_tokens().await;
            let window = serving_tx.borrow().served_context_window;
            let threshold = crate::inference::llama_server::wedge_miss_threshold(
                HEALTH_FAILS_TO_RELAUNCH,
                inflight,
                window,
            );
            continuum_base::probe!(
                target: "continuum_core::modules::serving_daemon", class = "serving.health",
                ok = false,
                consecutive = n as u64,
                threshold = threshold as u64,
                inflight_prompt_tokens = inflight.unwrap_or(0), // probe field: 0 is the no-inflight sentinel in telemetry, nothing budgets on it
                "live lane failed the decode heartbeat (control-plane may still 200 — a \
                 poisoned Metal backend); #175 self-heal",
            );
            if n >= threshold {
                // Sustained no-decode = wedged, not transiently busy. Reset the streak so we
                // don't re-trigger before the relaunch lands and republishes ready.
                health_fails.store(0, Ordering::Relaxed);
                Self::declare_lane_wedged(
                    &force_relaunch,
                    &serving_tx,
                    bus.as_ref(),
                    "sustained decode failure",
                );
            }
            health_probing.store(false, Ordering::Release);
        }))
    }

    /// Declare the live lane WEDGED: arm the force-probe and publish not-ready.
    ///
    /// BOTH steps are load-bearing and neither works alone, which is why this is one
    /// function instead of two lines copied per reporter:
    /// - `force_relaunch` makes the next reconcile re-prove decode even on a child we OWN;
    ///   without it the "trusted thereafter" short-circuit re-adopts the wedged lane forever.
    /// - the empty snapshot makes the reconcile RUN at all; its no-op guard returns early on
    ///   `ready && same model && same adapters` — *before* it ever consumes the force flag.
    ///
    /// Two independent reporters call this, because they answer different questions:
    /// - the decode heartbeat: "can this lane produce a token?" (a poisoned Metal backend)
    /// - the stderr wedge watcher: "is a slot reporting an impossible state?" (2026-08-05)
    ///
    /// The second exists because the first cannot see a single wedged SLOT: the other slots
    /// still decode, and a lane that delivered any token inside the probe window is trusted
    /// without probing at all. The 4.1-hour wedge was delivering 0.14 tok/s the whole time —
    /// alive by every liveness measure, and making no progress by the only one that mattered.
    pub(super) fn declare_lane_wedged(
        force_relaunch: &Arc<AtomicBool>,
        serving_tx: &watch::Sender<ServingSnapshot>,
        bus: Option<&Arc<MessageBus>>,
        reason: &str,
    ) {
        force_relaunch.store(true, Ordering::Release);
        // The reason is a PARAMETER here — publish it ON the snapshot, not only into
        // the log. `empty()` hardcodes `degraded_reason: None`, so for the whole
        // kill+respawn window `serving/status` read `ready:false, active_model:null,
        // lanes:0, degraded_reason:null` — absence with no stated cause, which is
        // indistinguishable from a node that has no brain at all. Measured live
        // 2026-09-05 (BigMama): a wedge-heal cycle with a healthy `/health` answering
        // on the doomed lane, recovering by itself inside 120s, three times read as a
        // possible dead-serving defect because the surface never said "relaunching".
        // The pre-flight guard already reserves a `degraded=` probe field for exactly
        // this string (added 2026-08-15: "serving/status knew the exact cause and
        // nothing surfaced it") and was being handed "" every time.
        // [[absence-rendered-as-positive-fact]] [[fallbacks-are-illegal-fail-loud]]
        let flipped = ServingSnapshot::degraded(reason.to_string());
        Self::emit_serving(bus, &flipped);
        let _ = serving_tx.send_replace(flipped);
        continuum_base::probe!(
            target: "continuum_core::modules::serving_daemon", class = "serving.health",
            action = "relaunch",
            reason = reason,
            "flipped serving snapshot not-ready — reconcile will kill+respawn the wedged \
             lane (#175 self-heal)",
        );
    }

    /// Consume any wedge the live lane's stderr watcher raised and escalate it.
    ///
    /// Polled on the daemon tick rather than pushed, because the lifecycle authority must
    /// stay HERE: the watcher runs inside the log sink, and a log sink that could reap a
    /// serving process would be a second owner of the lane's life.
    pub(super) fn take_reported_wedge(&self) {
        let Some(flag) = self.server.wedge_flag() else {
            return;
        };
        if !flag.take() {
            return;
        }
        Self::declare_lane_wedged(
            &self.force_relaunch,
            &self.serving_tx,
            self.bus.get(),
            "a slot reported impossible progress (>1.0)",
        );
    }
}
