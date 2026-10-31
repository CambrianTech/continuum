//! Demand ownership for the serving daemon.

use super::*;

pub fn resident_lane_demand(boot_floor: u32, live_residents: usize, overridden: bool) -> u32 {
    if overridden {
        return boot_floor.max(1);
    }
    boot_floor.max(live_residents as u32).max(1)
}
/// The leased-in lane demand the PLANNER may act on: the MINIMUM of the last
/// [`DOWNSHIFT_SUSTAINED_TICKS`] per-tick peaks — a burst has to be a LOAD for that many
/// consecutive ticks before it moves a slot. Fewer samples than the window = 0: a fresh
/// gauge never relaunches anything.
///
/// Why (Cormac's review of #4197, 2026-09-18): `--parallel` is a LAUNCH arg, so a lane-count
/// change is a new llama-server and every in-flight generation dies with the old one. The
/// planner debounces DOWNshifts by this same constant and adopts UPshifts immediately — so a
/// raw per-tick peak of bursty grid overflow would go: three leased calls arrive → +3 lanes →
/// relaunch NOW → the relaunch kills the three that raised the count → next tick reads 0 →
/// downshift → relaunch back → next burst → again. The 718-replan flap through a new input,
/// on the one seat everyone queues on, each swing destroying the remote turns it was sized
/// for. One constant for both directions, on purpose: the guard that stops a squeeze
/// evicting the best activity is the guard that stops a burst evicting the grid's coders.
/// The leased-in demand the plan may act on: the ring's minimum, and only once the ring
/// holds a full `DOWNSHIFT_SUSTAINED_TICKS` of peaks; fewer reads as no demand, never a lane.
pub fn sustained_leased_in(samples: usize, floor: usize) -> usize {
    if samples < DOWNSHIFT_SUSTAINED_TICKS as usize {
        0
    } else {
        floor
    }
}
/// The last [`DOWNSHIFT_SUSTAINED_TICKS`] per-tick leased-in peaks, oldest first — the
/// input to [`sustained_leased_in`]. Kept beside the tick that fills it. The most recent
/// raw peak is also kept so the plan receipt can carry both numbers.
pub(super) static LEASED_IN_PEAKS: std::sync::Mutex<crate::system_resources::sustained::Sustained> =
    std::sync::Mutex::new(crate::system_resources::sustained::Sustained::bounded(
        Duration::from_secs(DOWNSHIFT_SUSTAINED_TICKS as u64 * TICK.as_secs()),
        DOWNSHIFT_SUSTAINED_TICKS as usize,
    ));
pub(super) static LAST_LEASED_IN_PEAK: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0);
/// Fold this tick's leased-in peak into the ring and return the sustained demand.
pub(super) fn note_leased_in_peak(peak: usize) -> usize {
    LAST_LEASED_IN_PEAK.store(peak.min(u32::MAX as usize) as u32, Ordering::Relaxed);
    let mut ring = LEASED_IN_PEAKS.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: a poisoned ring reads its last state, same policy as every ledger lock here
    let floor = ring
        .observe(continuum_base::time::EpochMs::now().millis(), peak as u64)
        .min;
    sustained_leased_in(ring.len(), floor as usize)
}

impl ServingDaemonModule {
    /// Set the lane DEMAND (how many minds need a concurrent lane — the
    /// persona floor). The boot wiring calls this before the first
    /// [`Self::compute_plan`]; the next tick replans if it changes.
    pub fn set_lane_demand(&self, demand: u32) {
        // Register the demand state process-globally the first time boot sets it, so a
        // measurement preemption lease (quiesce_all / lease_measured_work) can drop the
        // warm-slot demand to the ACTIVE (non-quiesced) count for its duration and
        // release on drop — without threading a ServingDaemon handle into the persona
        // registry. Idempotent: `set` after the first call is a no-op.
        crate::inference::lane_demand::register(self.lane_demand.clone(), demand);
    }

    pub(super) fn lane_demand(&self) -> u32 {
        self.lane_demand.load(Ordering::Relaxed).max(1)
    }

    /// Both axes of what this host's minds are asking for — lanes AND the window
    /// they have actually demanded. The window term is MEASURED
    /// ([`crate::cognition::working_set`]); `None` there means no turn has been
    /// assembled yet on this host, and [`ServingDemand::new`] names the cold-start
    /// decision in one place.
    pub(super) fn serving_demand(&self) -> ServingDemand {
        let live_count =
            crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()
                .map(|r| r.live_personas().len())
                .unwrap_or(0); // JUSTIFIED unwrap_or: no registry yet (boot) = no residents = the floor stands
        let residents =
            resident_lane_demand(self.lane_demand(), live_count, lane_demand_overridden());
        // THE DECODE KNEE (2026-09-16): the roster is the demand, the measured decode
        // curve is the ceiling. 16 minds on 8 lanes decoded 7 t/s per stream against a
        // catalog 68 — a ten-times tax; with KV pages restoring from disk, fewer warm
        // slots is the restore economy, not starvation. `inference::decode_knee`.
        // THE KNEE IS THE MODEL'S, NOT THE LANE'S (2026-09-19 17:20:35Z, the M5, card
        // 29e4ab34): for the thirteen seconds between boot and the adopted lane reporting
        // its model, `active_model` was None, so the knee read as unknown, the roster's 17
        // runtimes went unclamped into the demand, the plan said 8 lanes under the
        // transient budget and RELAUNCHED the warm adopted lane into 8 × 49k — then the
        // 27B's knee of 2 arrived and it relaunched again to 2 × 67k. The knee is a
        // property of the model on this box; when no lane has reported one yet, the model
        // the box served last (the remembered geometry) is the one being planned for.
        let serving = crate::inference::llama_server::current_serving();
        let active_model = serving.active_model;
        let model_for_knee = knee_model(active_model, || {
            crate::inference::served_window_store::load_geometry().map(|g| g.model_id)
        });
        // A remembered model with no curve answers with the most CONSERVATIVE knee this box
        // holds (2026-09-20: the record read "coder-14b", a harness fixture a local test run
        // had written; knee None; 8 lanes on a knee-2 box). Never the newest: a small model's
        // exam an hour ago must not unclamp the 27B (Cormac, #4266). Said once per fallback.
        let knee = match model_for_knee.as_deref().and_then(crate::inference::decode_knee::knee_for) {
            Some(k) => Some(k),
            None => crate::inference::decode_knee::conservative_knee().map(|(measured, k)| {
                static SAID: parking_lot::Mutex<Option<String>> = parking_lot::Mutex::new(None);
                let mut said = SAID.lock();
                if said.as_deref() != Some(measured.as_str()) {
                    continuum_base::probe!(
                        target: "continuum_core::modules::serving_daemon", class = "serving.decode_knee.fallback",
                        remembered = model_for_knee.as_deref().unwrap_or("<none>"), // unwrap_or: nothing remembered is a name too
                        measured = measured.as_str(),
                        knee = k as u64,
                        "no knee for the model the box remembers — the lowest knee it holds bounds the lanes until a lane reports"
                    );
                    *said = Some(measured);
                }
                k
            }),
        };
        // THE PREFILL KNEE (card d4d2ef4a): on a CPU seat the shared prefill rate, not memory
        // or decode, is the limit (the IntelMac: 6 slots at 11-51 tok/s, a 13-26k prefilled
        // median, residents "never reached the model"). The plan serves the tighter knee.
        let decode_knee = knee;
        let prefill_knee = model_for_knee
            .as_deref()
            .and_then(crate::inference::prefill_knee::knee_for);
        let knee = crate::inference::prefill_knee::tighter_knee(decode_knee, prefill_knee);
        // +1 SCRATCH LANE: the adapter's traffic-class placement
        // (`inference/slots`) reserves the HIGHEST slot for sidecar/background/
        // probe traffic whenever n_slots ≥ 3 — so a plan sized to the resident
        // count leaves residents−1 warm slots for residents minds, and one
        // citizen is ALWAYS being LRU-evicted (measured 2026-09-01: interleaved
        // hit_rate=0.0 turns between 0.55–0.77 ones — the rotation thrash).
        // Fund the scratch slot HERE, in the demand the fixpoint solves for:
        // the window shrinks slightly to pay for it (floor-guarded at one full
        // assembled turn), and every resident keeps her prefilled prefix warm.
        // residents < 2 keeps the old shape (the adapter reserves nothing
        // below n_slots 3, so asking for the extra lane would waste it).
        let lanes = if residents >= 2 {
            residents + 1
        } else {
            residents
        };
        // The knee bounds the TOTAL slot count, scratch included: the curve is measured on
        // in-flight model calls, and the scratch slot is one of them (first cut clamped the
        // resident count and the +1 put the plan right back on the collapsed side).
        let clamped = crate::inference::decode_knee::knee_lanes(lanes, knee);
        // One row when the clamp CHANGES (first live read: one row per planning tick,
        // 400 in seven minutes — the ledger-rotation class), in both directions.
        static LAST_CLAMP: std::sync::atomic::AtomicU64 =
            std::sync::atomic::AtomicU64::new(u64::MAX);
        let shape = ((lanes as u64) << 32) | clamped as u64;
        if LAST_CLAMP.swap(shape, std::sync::atomic::Ordering::Relaxed) != shape {
            continuum_base::probe!(
                target: "continuum_core::modules::serving_daemon", class = "serving.decode_knee.clamped",
                model = %model_for_knee.as_deref().unwrap_or_default(), // unwrap_or_default: no model anywhere = no knee = the roster's own count, still worth one row
                roster_lanes = lanes as u64,
                knee = clamped as u64,
                decode_knee = decode_knee.unwrap_or(0) as u64, // probe field: 0 = unmeasured
                prefill_knee = prefill_knee.unwrap_or(0) as u64, // probe field: 0 = unmeasured, or prefill not the limit
                clamped = clamped < lanes,
                "lane demand against the measured knees (the tighter of decode and prefill) — the roster pages, the streams stay fast"
            );
        }
        let lanes = clamped;
        // Both ceilings are over the RESIDENTS, never the whole persisted registry
        // (490 entries: every test fixture and departed mind); the sent ceiling is what
        // the window follows, the untrimmed one only bounds it.
        let live: Vec<uuid::Uuid> =
            crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()
                .map(|r| r.live_personas())
                .unwrap_or_default(); // JUSTIFIED unwrap_or_default: no registry yet (boot) = no residents measured = cold-start prior
                                      // The prompt sizes of the generates this seat served for OTHER nodes join the
                                      // typical-prompt pool (c84d885a): a seat that plans lanes for leased-in minds must
                                      // size them to the prompts those minds send, not to one local resident's. A seat
                                      // with no residents of its own but leased-in minds still gets a floor from them.
        let leased_samples = (self.leased_in_sent)();
        let leased_sent: Vec<u32> = leased_samples
            .iter()
            .filter_map(|s| s.requested())
            .collect();
        let (demand, sent, median) = if live.is_empty() {
            // No registry yet (boot): the residents the working set REMEMBERS (rehydrated
            // from their last boot) are the demand pool — not only the leased-in ring. A
            // node reboots with the same minds; their last prompts are the best floor it
            // has, and a harness with no registry seeds the pool the same way.
            let remembered: Vec<uuid::Uuid> = self
                .working_set
                .all()
                .into_iter()
                .map(|(id, _)| id)
                .collect();
            (
                self.working_set.ceiling(),
                None,
                self.working_set.sent_median_with(&remembered, &leased_sent),
            )
        } else {
            (
                self.working_set.ceiling_of(&live),
                self.working_set.sent_ceiling_of(&live),
                self.working_set.sent_median_with(&live, &leased_sent),
            )
        };
        // The minds this seat served for OTHER nodes since the last plan — the peak
        // concurrent leased-in generates (c84d885a), DEBOUNCED: the planner acts on the
        // minimum over the last DOWNSHIFT_SUSTAINED_TICKS peaks (`sustained_leased_in`),
        // because a lane-count change relaunches the server and a raw burst would kill
        // the very calls that raised it. Zero until the inbound-generate seam feeds the
        // gauge; never invented.
        let leased_in =
            note_leased_in_peak(crate::inference::resource_admission::take_leased_in_peak());
        // Each live mind's REQUIREMENT: the untrimmed demand her turns assemble (never
        // what a window let her send), with headroom, floored at one real turn. Built
        // once here and carried on the demand by value into the one plan — the
        // allocator's input, the governed size (card 2eec3977).
        let requirements =
            crate::cognition::working_set::requirements_for(&live, &self.working_set);
        ServingDemand::new(lanes, demand)
            .with_requirements(requirements)
            .with_sent_tokens(sent)
            .with_sent_median(median)
            .with_leased_in(leased_in.min(u32::MAX as usize) as u32)
            // The knee bounds the TOTAL the plan serves, leased-in included: the clamp
            // above covers the roster and the scratch slot; the plan re-applies it over
            // `lanes + leased_in` (2026-09-19 23:14Z: 2 + 1 = 3 lanes against a knee of 2).
            .with_knee(knee)
    }

    /// Test seam: own the heartbeat's real-decode evidence instead of inheriting whatever
    /// the process-global happens to hold. See [`DecodeAgeSource`] for why this exists.
    #[cfg(test)]
    /// Tests: the leased-in prompt samples the demand pool folds in, in place of the
    /// process-global ring.
    #[cfg(test)]
    pub(super) fn set_leased_in_sent_source(&mut self, source: LeasedInSentSource) {
        self.leased_in_sent = source;
    }
}
