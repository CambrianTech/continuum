//! `ServingConsumer` — serving's face to the [`ResourceGovernor`](crate::resources).
//!
//! The serving daemon holds the heaviest residency on the box: a multi-GB model
//! in VRAM/UMA for the length of a serving epoch. Under this task (#56) that
//! residency is no longer self-granted — it is a *lease* from the one
//! per-machine authority, and serving is a peer consumer alongside Bevy and
//! LiveKit. This is the half the authority calls: when another peer needs the
//! bytes (a game grabs VRAM, an avatar scene loads), the governor *asks* serving
//! to free some, and serving answers honestly.
//!
//! # The one freeing lever, used honestly
//!
//! Serving's only public way to free VRAM is the suppress set
//! ([`ServingDaemonModule::intent`](super::serving_daemon::ServingDaemonModule::intent)):
//! mark the active model id as unloaded, and the daemon's own reconcile drops it
//! on its next tick — VRAM freed live, no restart. That unload is **async and
//! multi-second** (kill the child, wait for the GPU to release), so this
//! consumer never claims bytes are free before they are:
//!
//! - First ask → suppress the active model, remember the footprint we expect to
//!   reclaim, answer [`ReclaimStatus::Deferred`] ("unload scheduled, ask again").
//! - Re-ask while the snapshot still shows that model active → the unload is in
//!   flight → `Deferred` again. The authority keeps the lease alive and re-asks.
//! - Re-ask once the snapshot no longer shows that model → the GPU released it →
//!   answer [`ReclaimStatus::Released`] with the exact footprint, and the
//!   governor shrinks the lease by that scan-confirmed delta.
//!
//! # The gentler lever: tier-down instead of going dark
//!
//! Suppress-and-unload is whole-lease-granular — serving goes dark until a
//! re-load. Under [`Pressure`] there is a gentler answer: re-home to a SMALLER
//! base ([`pin`] the target so the daemon's reconcile swaps to it, #105), keep
//! answering, and report [`ReclaimStatus::Partial`] with only the freed delta
//! (old resident − new resident). WHICH smaller model — if any — is a decision,
//! and it is not baked in here: an injected [`TierDownPolicy`] chooses, and the
//! same async handshake carries out whatever it picks (decline → full unload;
//! choose → pin + Partial). See [`super::serving_tier_down`]. `Shutdown` and
//! `Rebalance` skip the policy — both want the lease gone from this box, which a
//! smaller model here does not serve.
//!
//! [`Pressure`]: crate::resources::ReclaimReason::Pressure
//! [`pin`]: ServingConsumer::pin_model
//! [`TierDownPolicy`]: super::serving_tier_down::TierDownPolicy
//!
//! # No new task, no parallel allocator
//!
//! This is a thin adapter over handles the daemon already publishes
//! (`subscribe_serving`, `intent`) plus a footprint resolver the daemon
//! supplies from its catalog. It owns no tick, no thread, no lock across an
//! await — the governor's daemon drives it. The acquire-on-load half (serving
//! *taking* the lease, and `host_budget` becoming governed headroom) is the
//! sibling slice that converges the two allocators.

use super::serving_daemon::ServingIntent;
#[cfg(test)]
use super::serving_daemon::ServingIntentSnapshot;
use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;
use tokio::sync::watch;

use crate::inference::planning::MIN_SERVE_CTX;
use crate::inference::lane_registry::LaneRecord;
use crate::inference::llama_server::ServingSnapshot;
use crate::modules::serving_tier_down::{TierDownContext, TierDownPolicy};

/// Size a lane inherited from a previous generation of this core — **a past form of
/// ourself** — from its custody record alone, with no probe and no clock.
///
/// # Why this exists (#452, measured 2026-08-19)
///
/// A fresh core's `ServingSnapshot` is empty for a lane it did not spawn, so the
/// consumer reported ZERO bytes and the governor filed the predecessor's ~50 GB under
/// `external` — someone else's. That read as `usable_gb = 2` on a 64 GB machine, the
/// planner chose a 3B, and a healthy 27B was reaped to make room for it. Twice, in one
/// boot. The self-eviction ratchet could not save it either: the ratchet adds back our
/// OWN measured residency, and the lane was not ours yet.
///
/// # The refusal is the load-bearing half
///
/// A record written before the shape fields existed carries `0`, which means UNKNOWN,
/// not "a zero-byte lane". Returning `None` there hands the caller an absence it can
/// reason about; returning `0` would hand it a *quantity* it would happily budget
/// against — the exact defect class this file spent the day removing
/// ([[an-absence-is-an-unfinished-measurement]]).
///
/// Pure over `(record, sizer)` so both arms are testable without a filesystem, a
/// process, or a live governor.
fn inherited_footprint(
    rec: &LaneRecord,
    size: impl Fn(&str, u32, u32) -> u64,
) -> Option<(u64, String)> {
    if rec.context_window == 0 || rec.lanes == 0 {
        return None;
    }
    Some((
        size(&rec.model, rec.context_window, rec.lanes),
        format!(
            "{} inherited from a previous generation (pid {}, {} lane(s) × {} ctx) — a \
             past form of ourself, ours to count and ours to control",
            rec.model, rec.pid, rec.lanes, rec.context_window
        ),
    ))
}
use crate::resources::{
    ConsumerFootprint, ReclaimOutcome, ReclaimReason, ReclaimRequest, ReclaimStatus,
    ResourceConsumer, ResourceKind,
};

/// Resolves an active model id + its live serving shape (served per-slot window,
/// lane count) to its TOTAL resident VRAM bytes — weights PLUS the KV-cache of
/// every lane at that window (#79). The daemon supplies this from its live
/// catalog + footprint estimator (the same numbers that feed the serving plan —
/// one footprint authority, not two). The window/lanes come from the
/// [`ServingSnapshot`] (llama.cpp's own `/props` truth), never a recomputed plan
/// value. Injectable so the consumer is testable without a populated registry.
pub type FootprintFn = Arc<dyn Fn(&str, u32, u32, u32) -> u64 + Send + Sync>;

/// `model_id → physical bytes the serving process holds right now`, when measured.
pub type MeasuredFn = Arc<dyn Fn(&str) -> Option<u64> + Send + Sync>;

/// What the serving ENGINE says it holds on a discrete accelerator: its weights there plus
/// its KV and compute buffers (`/props`: `model_weight_buffers` + `memory_breakdown`).
/// The serving daemon records it on its footprint tick; this consumer credits it.
///
/// WHY (card 741f5eec, the 5090 on 2026-10-07): the physical credit read the lane
/// process's host memory (`anon_footprint_of` + `model_file_bytes_of`). On a discrete card
/// that is the wrong quantity, since the engine's real holding is VRAM and no RSS contains
/// it. On Windows both reads return `None`, so the board credited Kimi's 31 GB engine with
/// zero and filed all of it as EXTERNAL. Serving's replace-myself budget was then just free
/// VRAM, and a dead process's allocations releasing in steps swung it between two exact
/// values, flapping her window between 33k and 69k. The engine's own accounting is the
/// attribution-correct figure: a device delta would also credit a game started after spawn.
#[derive(Default)]
pub struct EngineDeviceCredit {
    state: std::sync::Mutex<DeviceCreditState>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum DeviceCreditState {
    /// No discrete-GPU lane observed: the host-process read is the right quantity.
    #[default]
    NotDiscrete,
    /// A discrete-GPU lane: `bytes` is what the engine at `pid` reported, `None` unread.
    Discrete { pid: Option<u32>, bytes: Option<u64> },
}

/// How serving's credit is to be read for the live lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeviceAnswer {
    /// Not a discrete-GPU lane: read the host process (unified memory, CPU placement).
    NotDiscrete,
    /// A discrete-GPU lane: this is the credit, `None` = nothing to credit (the gap between
    /// engines, a stale reading from a previous engine, or an unreadable `/props`). Never
    /// the host read in its place: that would credit the wrong memory.
    Discrete(Option<u64>),
}

impl EngineDeviceCredit {
    /// Record a discrete-GPU reading for the live lane `pid` (`None` = no live lane).
    /// Returns whether the reading changed, so the caller can probe on change only.
    pub fn record_discrete(&self, pid: Option<u32>, bytes: Option<u64>) -> bool {
        let next = DeviceCreditState::Discrete { pid, bytes };
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner()); // poisoned lock = a reading is still a reading; the next tick overwrites it
        let changed = *state != next;
        *state = next;
        changed
    }

    /// The lane is no longer on a discrete GPU (it moved to the CPU): the host read applies.
    pub fn record_not_discrete(&self) {
        *self.state.lock().unwrap_or_else(|p| p.into_inner()) = DeviceCreditState::NotDiscrete; // poisoned lock = a reading is still a reading; the next tick overwrites it
    }

    fn answer(&self, live_pid: Option<u32>) -> DeviceAnswer {
        device_answer(*self.state.lock().unwrap_or_else(|p| p.into_inner()), live_pid) // poisoned lock = read the last reading, the next tick overwrites it
    }
}

/// The credit decision, pure: a recorded reading counts only for the engine it was read
/// from, so a reading taken before a relaunch is never credited to the successor.
fn device_answer(state: DeviceCreditState, live_pid: Option<u32>) -> DeviceAnswer {
    match state {
        DeviceCreditState::NotDiscrete => DeviceAnswer::NotDiscrete,
        DeviceCreditState::Discrete { pid, bytes } => DeviceAnswer::Discrete(match (pid, live_pid) {
            (Some(read), Some(live)) if read == live => bytes.filter(|b| *b > 0),
            _ => None,
        }),
    }
}

/// The physical credit for serving's live lane: the engine's device holding on a discrete
/// GPU, the host process's residency otherwise, and 0 in the gap either way.
fn physical_measure(device: Arc<EngineDeviceCredit>) -> MeasuredFn {
    Arc::new(move |_model: &str| {
        let lane = crate::inference::lane_registry::live_lane();
        match device.answer(lane.as_ref().map(|l| l.pid)) {
            DeviceAnswer::Discrete(bytes) => bytes,
            DeviceAnswer::NotDiscrete => lane.and_then(|l| live_lane_resident(l.pid)),
        }
    })
}

/// The `consumer_id` serving's leases carry. Matches the id the acquire-on-load
/// half will mint leases under, so the authority's asks route back here.
pub const SERVING_CONSUMER_ID: &str = "serving";

/// A reclaim serving has scheduled and is waiting for the snapshot to confirm.
/// Keyed (in [`ServingConsumer::pending`]) by the OLD model id that must vanish
/// from the snapshot before we report freed — a full unload clears it; a
/// tier-down swaps it for the smaller model, so either way the old id leaving is
/// the confirmation that the async cleanup landed.
struct Pending {
    /// Bytes to report freed once it lands. Full unload → the whole footprint;
    /// tier-down → the delta (old resident − new resident, since serving still
    /// holds the smaller model).
    freed_on_land: u64,
    /// The terminal disposition once it lands. `Released` when serving now holds
    /// nothing (unload); `Partial` when it shrank but still holds the smaller
    /// model (tier-down).
    status_on_land: ReclaimStatus,
}

/// Serving's [`ResourceConsumer`] adapter — see the module docs.
pub struct ServingConsumer {
    /// Live serving state: which model is active + ready. Read (never blocks) to
    /// report footprint and to confirm an unload/swap actually landed.
    serving: watch::Receiver<ServingSnapshot>,
    /// The full-unload seam: insert an id → the daemon unloads it next reconcile.
    intent: ServingIntent,
    /// The re-home seam: set a smaller model id → the daemon's reconcile swaps to
    /// it (candidates intersect to the pin), freeing the delta without going dark
    /// (#105). The tier-down lever.
    /// HIGH-WATER RESIDENCY (#438, measured 2026-08-19). The bytes this consumer has
    /// held and has NOT been shown releasing. See `footprint` for why a plan-derived
    /// number alone under-reports by a whole model during every reshape.
    held_high_water: std::sync::atomic::AtomicU64,
    /// The `ready_verified_at_ms` of the last snapshot we accepted as PROOF the previous
    /// process is gone. Evidence, not a timer: the high-water decays only when readiness
    /// is re-verified at the new shape.
    decayed_at_verified_ms: std::sync::atomic::AtomicU64,
    /// active model id + live shape → resident bytes (weights + per-lane KV).
    footprint_of: FootprintFn,
    /// The LIVE serving process's physical residency (`anon_footprint_of(pid)`): what it
    /// holds right now, `None` when no live serving process exists (card 628dc958). When
    /// this consumer measures physically, the credit is exactly this figure and 0 in the
    /// gap between engines — never an estimate, never a held prior — so the board's
    /// physical `available` plus this credit does not move when the same bytes change
    /// hands. The estimate path is for backends with no process to measure.
    measured_of: MeasuredFn,
    /// Whether `measured_of` is authoritative (a process-measuring backend). False only
    /// for backends with nothing to measure, where the catalog estimate stands in.
    measures_physically: bool,
    /// The row those bytes live in — `Vram` for a GPU-placed lane, `Ram` for a
    /// CPU-placed one (`serving_daemon::serving_pool_kind`). Handed in, never resolved
    /// here: the consumer reports and yields on the SAME row the plan budgets from, or
    /// the replace-myself credit lands on a row nobody reads and the plan flees its own
    /// residency (card 7c72c0f0).
    pool_kind: ResourceKind,
    /// How to find a lane inherited from a previous generation — **injected**, for the
    /// same reason `footprint_of` is.
    ///
    /// The first cut called `lane_registry::live_lane()` directly from `footprint()`,
    /// which reaches a process-wide FILESYSTEM singleton: three unit tests immediately
    /// began reporting the operator's real live 27B and failing. A unit that reads the
    /// developer's machine is not a unit test ([[#72 embed-resolver]]), and worse, it
    /// would pass or fail depending on whether a lane happened to be up. Production
    /// wires the real lookup; tests hand it a closure.
    inherited_lane: Arc<dyn Fn() -> Option<LaneRecord> + Send + Sync>,
    /// The swappable intelligence that chooses whether/where to tier down under
    /// pressure. `DeclineTierDown` until a real selection policy is authored — the
    /// consumer's handshake is identical regardless.
    tier_down: Arc<dyn TierDownPolicy>,
    /// Reclaims we've scheduled and are waiting to confirm freed. Brief-locked; no
    /// await is ever held across this guard.
    pending: Mutex<HashMap<String, Pending>>,
}

impl ServingConsumer {
    pub fn new(
        serving: watch::Receiver<ServingSnapshot>,
        intent: ServingIntent,
        footprint_of: FootprintFn,
        pool_kind: ResourceKind,
        tier_down: Arc<dyn TierDownPolicy>,
    ) -> Self {
        Self {
            serving,
            intent,
            held_high_water: std::sync::atomic::AtomicU64::new(0),
            decayed_at_verified_ms: std::sync::atomic::AtomicU64::new(0),
            footprint_of,
            measured_of: physical_measure(Arc::new(EngineDeviceCredit::default())),
            measures_physically: true,
            pool_kind,
            inherited_lane: Arc::new(crate::inference::lane_registry::live_lane),
            tier_down,
            pending: Mutex::new(HashMap::new()),
        }
    }

    /// Share the daemon's engine-device reading (see [`EngineDeviceCredit`]): the daemon
    /// records it on its footprint tick, and this consumer's physical credit reads it.
    pub fn with_engine_device_credit(mut self, device: Arc<EngineDeviceCredit>) -> Self {
        self.measured_of = physical_measure(device);
        self
    }

    /// A process-measuring backend under test: the credit becomes exactly what `f`
    /// says the live process holds, 0 when it says there is none.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn with_measured(mut self, f: MeasuredFn) -> Self {
        self.measured_of = f;
        self.measures_physically = true;
        self
    }

    /// A backend with no process to measure (tests of the estimate path).
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn without_measurement(mut self) -> Self {
        self.measures_physically = false;
        self
    }

    /// Override how an inherited lane is found. Production keeps the default (the
    /// real lane registry); tests inject a closure so a unit never depends on whether
    /// the developer's machine happens to have a lane up.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn with_inherited_lane(
        mut self,
        f: Arc<dyn Fn() -> Option<LaneRecord> + Send + Sync>,
    ) -> Self {
        self.inherited_lane = f;
        self
    }

    /// The model the box is serving right now WITH the shape needed to size its
    /// residency: `(model_id, served_per_slot_window, lanes)`. `None` while
    /// nothing is loaded or a load is still coming ready — in both cases serving
    /// holds no reclaimable VRAM yet. The window + lane count are the process's
    /// own truth (from the snapshot's `/props` read), so the KV term is charged
    /// against what is actually resident, never a recomputed plan value.
    fn active_ready(&self) -> Option<(String, u32, u32, u32)> {
        let snap = self.serving.borrow();
        if snap.ready {
            snap.active_model
                .clone()
                .map(|id| (id, snap.served_context_window, snap.lanes, snap.host_prompt_cache_mib))
        } else {
            None
        }
    }

    /// Add `id` to the suppress set (COW insert), so the daemon's reconcile
    /// unloads it. Idempotent — re-suppressing an already-suppressed id is a
    /// no-op the daemon ignores.
    fn suppress_model(&self, id: &str) {
        self.intent.set_suppressed(id, true, false);
    }

    /// Force-pin `id` (the re-home target), so the daemon's reconcile intersects
    /// its candidates to that one model and swaps to it — the tier-down carry-out.
    /// Unlike suppress this does NOT go dark: the daemon serves the smaller model.
    fn pin_model(&self, id: &str) {
        self.intent.set_pin(Some(id.to_string()), false);
    }
}

/// What the live lane holds, both halves (card fa21f81f): its anonymous footprint (KV,
/// compute, allocator; `phys_footprint` / `RssAnon + VmSwap`, which exclude mapped files by
/// design) PLUS the model file it maps (the weights). Crediting only the anonymous half
/// left the replace-myself budget short by the engine's own weights, since those resident
/// file pages are not in the board's `available` either: a replan could not re-fit the
/// shape already running, so the plan only ever shrank (the M5 on 2026-09-26: 2 × 41k
/// incumbent, the plan 1 × 8k, lanes never grew back). Weights that cannot be read (argv
/// unreadable, file gone) leave the anonymous half alone, said once.
fn live_lane_resident(pid: u32) -> Option<u64> {
    use crate::inference::lane_footprint::{anon_footprint_of, model_file_bytes_of};
    static WEIGHTS_UNREADABLE_SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let anon = anon_footprint_of(pid)?;
    match model_file_bytes_of(pid) {
        Some(weights) => Some(anon.saturating_add(weights)),
        None => {
            if !WEIGHTS_UNREADABLE_SAID.swap(true, std::sync::atomic::Ordering::Relaxed) {
                continuum_base::probe!(
                    class = "serving.credit.weights_unreadable",
                    pid = pid as u64,
                    anon_bytes = anon,
                    "the lane's model file could not be read off its argv: the serving credit is its anonymous footprint only, short by the mapped weights"
                );
            }
            Some(anon)
        }
    }
}

#[async_trait]
impl ResourceConsumer for ServingConsumer {
    fn consumer_id(&self) -> &str {
        SERVING_CONSUMER_ID
    }

    fn footprint(&self) -> Vec<ConsumerFootprint> {
        use std::sync::atomic::Ordering;

        // WHAT THE PLAN SAYS is served right now. Zero while a lane is loading or
        // relaunching, because `active_ready` gates on `ready`.
        // THE CREDIT IS WHAT THE PROCESS HOLDS, NOT WHAT THE CATALOG PREDICTED (card
        // 628dc958, Cormac 2026-09-26: "credit only bytes still resident"). The
        // replace-myself budget is the board's physical `available` plus this figure, so
        // this figure must be physical too, or the sum is off by the estimate's error in
        // one direction and by a whole engine across a relaunch. Measured on the M5
        // 14:12-14:50Z: the catalog charged 47.7 GB for 6 × 35k (peak KV estimate plus the
        // full cache grant) while the process held ~26 GB; the plan read ~20 GB it did
        // not have, grew into swap (1.25 GB), and relaunched six times in 75 minutes.
        // The estimate stands in only before a measurement exists in this life.
        let active = self.active_ready();
        if self.measures_physically {
            // Physical, and only physical: the live process's residency, 0 in the gap.
            // No estimate (the catalog charged 47.7 GB for a process holding ~26 GB) and
            // no held prior (a prior credited while `available` has already risen by the
            // same bytes is A + 2F, Cormac on #4410). The old engine still counts for as
            // long as it is the live process; the successor counts from its first byte.
            let model = active
                .as_ref()
                .map(|(id, _, _, _)| id.clone())
                .or_else(|| self.serving.borrow().loading_model.clone())
                .unwrap_or_default();
            return match (self.measured_of)(&model) {
                Some(bytes) if bytes > 0 => vec![ConsumerFootprint {
                    kind: self.pool_kind,
                    bytes,
                    detail: format!("{model} measured resident (live serving process)"),
                }],
                _ => Vec::new(),
            };
        }
        let planned = match &active {
            Some((id, window, lanes, grant_mib)) => Some((
                (self.footprint_of)(id, *window, *lanes, *grant_mib),
                format!("{id} weights+KV estimated ({lanes} lane(s) × {window} ctx)"),
            )),
            None => None,
        };
        // A LOADING LANE STILL HOLDS BYTES. `active_ready()` is None until `/props`
        // answers, but the weights are mmap'd from the moment the process starts. Charge
        // them at a MINIMUM shape — one lane at the floor window — which under-states the
        // eventual KV but names the dominant term (weights) immediately. Conservative in
        // the safe direction: it grows to the true figure the moment readiness lands.
        let planned = planned.or_else(|| {
            let snap = self.serving.borrow();
            snap.loading_model.clone().map(|id| {
                let bytes = (self.footprint_of)(&id, MIN_SERVE_CTX, 1, 0);
                (
                    bytes,
                    format!("{id} loading — weights resident, KV not yet allocated"),
                )
            })
        });
        // A PAST FORM OF OURSELF IS STILL OURS (#452, measured 2026-08-19).
        //
        // Neither arm above can see a lane this core did not spawn. After a crash the
        // predecessor's llama-server keeps running — named in our own custody record,
        // holding tens of GB — while a fresh core's snapshot is empty. It therefore
        // reported ZERO, and the governor filed those bytes under `external`: someone
        // else's. Measured consequence: `usable_gb = 2` on a 64 GB machine, so the
        // planner chose a 3B and REAPED a healthy 27B to make room for it, twice in one
        // boot. The self-eviction ratchet could not rescue it either — the ratchet adds
        // back our OWN measured residency, and the lane wasn't ours yet.
        //
        // The record is the durable statement of what our previous generation holds, so
        // read it. Refuse to size a record that predates the shape fields: `0` there
        // means UNKNOWN, and a fabricated number is the exact class of bug this whole
        // day was spent removing.
        let planned = planned.or_else(|| {
            let rec = (self.inherited_lane)()?;
            // The custody record carries the shape, not the grant: a past form's cache
            // is under-attributed by its grant until the record learns it (named gap).
            inherited_footprint(&rec, |m, w, l| (self.footprint_of)(m, w, l, 0))
        });
        let planned_bytes = planned.as_ref().map(|(b, _)| *b).unwrap_or(0);

        // THE HIGH-WATER REQUIRES A PROOF CHANNEL. `ready_verified_at_ms` is stamped the
        // moment readiness is CONFIRMED for the lane now serving, and a verification we
        // have not yet consumed is proof the previous process is gone. If the snapshot
        // carries NO stamp at all, there is no mechanism that could ever prove release —
        // and a claim that can never be falsified would strand this machine's capacity
        // forever. So: no proof channel, no claim. Report the plan and nothing more.
        //
        // That is the same rule as everywhere else today — the failure direction must
        // move toward LESS trust. Under-reporting for a moment is recoverable; holding an
        // unfalsifiable claim on a third of the box is not.
        let Some(verified) = self.serving.borrow().ready_verified_at_ms else {
            return planned
                .map(|(bytes, detail)| {
                    vec![ConsumerFootprint {
                        kind: self.pool_kind,
                        bytes,
                        detail,
                    }]
                })
                .unwrap_or_default();
        };

        // DECAY ON EVIDENCE, NEVER ON A TIMER.
        // The high-water decays to the new engine's figure only once that engine has
        // PUBLISHED its shape: the verify stamp lands a tick or two before active_model
        // and the window do, and decaying on the stamp alone dropped the credit to the
        // loading figure (weights at the floor window) for those ticks — 47.7 → 20.2 GB at
        // an unchanged shape on the M5, 14:46:51Z.
        let consumed = self.decayed_at_verified_ms.load(Ordering::Relaxed);
        if verified > consumed && planned.is_some() && active.is_some() {
            self.decayed_at_verified_ms.store(verified, Ordering::Relaxed);
            self.held_high_water.store(planned_bytes, Ordering::Relaxed);
        } else if planned_bytes > self.held_high_water.load(Ordering::Relaxed) {
            self.held_high_water.store(planned_bytes, Ordering::Relaxed);
        }

        let held = self.held_high_water.load(Ordering::Relaxed);
        if held == 0 {
            // Never held anything and nothing is planned — a real, honest nothing.
            return Vec::new();
        }

        // REPORT WHAT WE HOLD, NOT WHAT WE INTEND. Measured live 2026-08-19: mid-reshape
        // the board read `serving 25.93 GB (1 lane)` against `phys 47.25 GB` — the old
        // 4-lane process was still resident while the snapshot had already flipped to the
        // new shape. The 21.3 GB gap became "unowned", unowned reads as immovable,
        // `available` collapsed to 8.42 GB, the 27B no longer fit, and the planner took a
        // 0.5B. Under-reporting our own residency is indistinguishable from someone else
        // holding it, and it is strictly more dangerous: it invites over-commit against
        // bytes we have not released.
        let detail = match &planned {
            Some((b, d)) if *b >= held => d.clone(),
            Some((_, d)) => format!("{d} — reporting prior residency, not yet confirmed released"),
            None => "prior residency held while no lane is ready (loading or relaunching)"
                .to_string(),
        };
        vec![ConsumerFootprint {
            kind: self.pool_kind,
            bytes: held,
            detail,
        }]
    }

    async fn reclaim(&self, request: ReclaimRequest) -> ReclaimOutcome {
        // Serving holds ONE pool — the row its lane's placement fills. An ask for any
        // other kind is honestly refused (named, never a silent freed=0) so the
        // authority does not misread it.
        if request.kind != self.pool_kind {
            return ReclaimOutcome::refused(format!(
                "serving holds no {:?}, only {:?}",
                request.kind, self.pool_kind
            ));
        }

        // First: did a reclaim we already scheduled now land? If the snapshot no
        // longer shows the OLD model (unload cleared it, or a tier-down swapped it
        // for the smaller one), the async cleanup landed — report the cached freed
        // bytes with its cached disposition and clear it. This is the
        // Deferred→(Released|Partial) transition the authority re-asks for.
        {
            let mut pending = self.pending.lock();
            if !pending.is_empty() {
                let active = self.active_ready();
                let active_id = active.as_ref().map(|(id, _, _, _)| id.as_str());
                let landed: Vec<String> = pending
                    .keys()
                    .filter(|id| active_id != Some(id.as_str()))
                    .cloned()
                    .collect();
                if !landed.is_empty() {
                    let mut freed = 0u64;
                    // Released only if EVERY landed reclaim fully released; a
                    // tier-down landing means serving still holds the smaller
                    // model, so the honest aggregate is Partial.
                    let mut status = ReclaimStatus::Released;
                    for id in &landed {
                        if let Some(p) = pending.remove(id) {
                            freed = freed.saturating_add(p.freed_on_land);
                            if p.status_on_land == ReclaimStatus::Partial {
                                status = ReclaimStatus::Partial;
                            }
                        }
                    }
                    let detail = match status {
                        ReclaimStatus::Partial => "tier-down landed — VRAM partially freed",
                        _ => "unload landed — VRAM released",
                    };
                    return ReclaimOutcome {
                        freed_bytes: freed,
                        status,
                        detail: Some(detail.into()),
                    };
                }
                // Still in flight — honest backpressure, re-ask next tick.
                return ReclaimOutcome {
                    freed_bytes: 0,
                    status: ReclaimStatus::Deferred,
                    detail: Some("cleanup in flight".into()),
                };
            }
        }

        // Nothing pending → decide the lever. We need the active model + its live
        // shape to size what we hold.
        let Some((active, window, lanes, grant_mib)) = self.active_ready() else {
            // We hold nothing reclaimable — already free, honestly zero.
            return ReclaimOutcome::released(0);
        };
        let current = (self.footprint_of)(&active, window, lanes, grant_mib);

        // Under Pressure another peer needs room NOW but serving need not go dark:
        // offer the tier-down policy the chance to re-home to a smaller base and
        // free only the delta. Shutdown wants everything gone; Rebalance is moving
        // the lease OFF this box — a smaller model here serves neither, so both
        // skip straight to the full unload.
        if request.reason == ReclaimReason::Pressure {
            let ctx = TierDownContext {
                active_model: &active,
                current_bytes: current,
                served_window: window,
                lanes,
                request: &request,
            };
            if let Some(td) = self.tier_down.choose(&ctx) {
                // Trust the policy's target but VERIFY it is a genuine shrink to a
                // real model — never carry out a phantom or a lateral/up "tier
                // down" (fail loud by declining to the honest full unload).
                if !td.target_model.is_empty() && td.resident_after < current {
                    let freed = current - td.resident_after;
                    self.pin_model(&td.target_model);
                    self.pending.lock().insert(
                        active.clone(),
                        Pending {
                            freed_on_land: freed,
                            status_on_land: ReclaimStatus::Partial,
                        },
                    );
                    return ReclaimOutcome {
                        freed_bytes: 0,
                        status: ReclaimStatus::Deferred,
                        detail: Some(format!(
                            "tier-down {active} → {} scheduled (frees {freed} bytes)",
                            td.target_model
                        )),
                    };
                }
                // Non-shrinking proposal → ignore it, fall through to full unload.
            }
        }

        // No tier-down → unload the active model to free its whole footprint.
        // Whole-lease-granular: serving's lease IS the active model.
        self.suppress_model(&active);
        self.pending.lock().insert(
            active.clone(),
            Pending {
                freed_on_land: current,
                status_on_land: ReclaimStatus::Released,
            },
        );

        // The unload is still async — Deferred is the honest first answer
        // regardless of reason; the GPU has not released yet. The authority
        // re-asks and gets Released once it lands.
        let detail = match request.reason {
            ReclaimReason::Shutdown => "shutdown unload scheduled",
            _ => "unload scheduled",
        };
        ReclaimOutcome {
            freed_bytes: 0,
            status: ReclaimStatus::Deferred,
            detail: Some(detail.into()),
        }
    }
}

#[cfg(test)]
mod tests;
