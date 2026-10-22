//! Admission metrics: in-flight gauges, wait/hold rings and ambient-yield counters.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
// Tests name the geometry through this one upward import, so the split adds no second one.
pub(super) use crate::modules::served_window_store::ServedGeometry;

// ── Soft saturation signal (the read-only companion to the hard gate) ──────────
//
// Deliberative model calls currently outstanding against the shared serving target.
// A lock-free process-global gauge: the resource it measures — ONE shared
// llama-server with `serving_plan::MAX_LANES` decode slots serving the whole fleet
// via continuous batching — IS process-global, so the gauge granularity matches the
// resource exactly (no per-caller Arc threading buys any fidelity).
//
// This is a SOFT signal, not an allocator: it admits and denies NOTHING. The hard
// admission decision is `ResourceAdmissionGate` (above); this gauge exists because
// the live inference path is not yet lease-wired AND a background self-tick — the
// lowest-priority work in the system — needs a zero-cost read of "is every decode
// slot busy right now" to decide whether adding an idle deliberation would only
// deepen the queue that live conversation is already waiting behind. Glass-boxed
// 2026-07-15: one inbound message woke six minds, each ran a full ~54s deliberation,
// and the two lanes serialized them into a 250s tail (#139).
// [[conversational-latency-is-a-misdirection-budget]] [[idle-is-self-directed-free-time]]
pub(super) static INFLIGHT_MODEL_CALLS: Gauge = Gauge::new();

/// The in-flight model-call gauge: a saturating count of outstanding deliberative model calls.
/// Extracted as a named type (not a bare static) for one reason beyond tidiness — testability.
/// The process has exactly ONE shared instance (`INFLIGHT_MODEL_CALLS`) that production RAII
/// guards bump; a unit test constructs its OWN isolated `Gauge` and drives the identical
/// enter/read/saturate logic against it, so its absolute-count assertions are deterministic
/// under `cargo test`'s parallel execution — no cross-module guard on the shared instance can
/// perturb a gauge nothing else can reach. (This is the #1960 flaky class killed at the root:
/// the previous test read the process-global counter as an absolute value while a sibling test's
/// `InflightModelCall` guard bumped it mid-loop; drain-before-baseline couldn't catch mid-loop
/// interference. Inject the gauge instead of draining around it.)
#[derive(Debug)]
pub(super) struct Gauge {
    count: AtomicUsize,
}

impl Gauge {
    pub(super) const fn new() -> Self {
        Self {
            count: AtomicUsize::new(0),
        }
    }

    /// Enter one in-flight call; the returned guard decrements the SAME gauge on drop.
    pub(super) fn enter(&self) -> GaugeGuard<'_> {
        self.count.fetch_add(1, Ordering::AcqRel);
        GaugeGuard(&self.count)
    }

    /// Outstanding calls right now.
    pub(super) fn inflight(&self) -> usize {
        self.count.load(Ordering::Acquire)
    }

    /// True when `outstanding >= max_lanes` — one more call would queue behind the fleet.
    pub(super) fn saturated(&self, max_lanes: usize) -> bool {
        self.inflight() >= max_lanes
    }
}

/// RAII decrement for one gauge entry. Borrows the gauge it incremented so drop decrements
/// exactly that gauge — the shared instance for a production guard, a test's local instance for
/// a test's. This borrow is what makes the gauge injectable without a second counter.
#[derive(Debug)]
pub(super) struct GaugeGuard<'a>(&'a AtomicUsize);

impl Drop for GaugeGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// RAII marker: increments the shared in-flight gauge on entry and decrements on EVERY exit
/// path (Ok, Err, panic-unwind). The deliberation faculty wraps its single generate call in this
/// so the gauge reflects exactly the model-call window (lane-queue + prefill + decode) and
/// nothing downstream. Wraps a guard on the process-global `INFLIGHT_MODEL_CALLS`, so it carries
/// NO lifetime and stays storable as a plain field (e.g. `ServingLanePermit._inflight`).
#[derive(Debug)]
pub struct InflightModelCall {
    /// Held for its drop — the decrement IS the field's purpose. Named so, rather than a
    /// positional field rustc rightly reports as never read (it never is; it is released).
    _guard: GaugeGuard<'static>,
}

impl InflightModelCall {
    pub fn enter() -> Self {
        Self {
            _guard: INFLIGHT_MODEL_CALLS.enter(),
        }
    }
}

/// A gauge that also remembers its PEAK since the last read — for a reader that asks
/// "how many at once, over the interval" (the serving planner's tick) rather than "how
/// many right now" (the admission gate). Same injectable shape as [`Gauge`], for the same
/// testability reason: a test drives its own instance, never the process-global one.
#[derive(Debug)]
pub(super) struct PeakGauge {
    gauge: Gauge,
    peak: AtomicUsize,
}
impl PeakGauge {
    pub(super) const fn new() -> Self {
        Self {
            gauge: Gauge::new(),
            peak: AtomicUsize::new(0),
        }
    }
    /// Enter one call and fold the new concurrency into the peak.
    pub(super) fn enter(&self) -> GaugeGuard<'_> {
        let guard = self.gauge.enter();
        self.peak.fetch_max(self.gauge.inflight(), Ordering::AcqRel);
        guard
    }
    pub(super) fn inflight(&self) -> usize {
        self.gauge.inflight()
    }
    /// The peak concurrency since the previous take, then re-arm at the CURRENT
    /// concurrency — a call still in flight at the read is not forgotten by the reset.
    pub(super) fn take_peak(&self) -> usize {
        let now = self.gauge.inflight();
        self.peak.swap(now, Ordering::AcqRel).max(now)
    }
}

/// LEASED-IN model calls: inference THIS seat performs for minds hosted on OTHER nodes —
/// the `ai/generate` a peer sent over airc when its placement spilled a mind here.
///
/// Measured 2026-09-18 (card c84d885a): the 5090's 27B seat served the M5's coders
/// (Benchy 12, Aris 8, Demetri 5, Mara 5 answers in one afternoon) and IntelMac's eight,
/// on ONE slot, with lane waits of p50 216 s / p90 1,387 s / max 7,058 s — while its
/// serving plan counted only its own roster, because nothing counted these. A seat's
/// demand is the minds it SERVES, not only the minds it HOSTS. This gauge is the missing
/// term; the planner reads its peak per tick ([`take_leased_in_peak`]) and adds it to
/// the resident lane demand ([`crate::cognition::serving_plan::ServingDemand::leased_in`]).
/// It is fed at the seam where an inbound remote generate is executed, which today enters
/// neither this gauge nor the admission gate — the wire slice of the same card.
pub(super) static LEASED_IN_CALLS: PeakGauge = PeakGauge::new();

/// RAII marker for one leased-in call, mirror of [`InflightModelCall`]: enters on
/// construction, leaves on EVERY exit path. Hold it across the whole remote generate.
#[derive(Debug)]
pub struct LeasedInCall {
    /// Held for its drop — the decrement IS the field's purpose (see `InflightModelCall`).
    _guard: GaugeGuard<'static>,
}
impl LeasedInCall {
    pub fn enter() -> Self {
        Self {
            _guard: LEASED_IN_CALLS.enter(),
        }
    }
}
/// Demand from leased-in generates and observed service from the local engine.
/// The plan projects demand into its growth floor and model-bound service into
/// remembered geometry; a requested size must never certify successful service.
/// Without them a two-resident seat's "typical prompt" is one resident's SWE prompt, and
/// the plan grows lanes for the grid (`leased_in`) it then refuses to fit (BigMama,
/// 2026-09-18: the 5090 considered 2 × 49k every tick and kept 1 × 101k, because 49k was
/// under a floor computed from Sahar's prompts alone while twelve leased-in coders sent
/// ~30k). The existing bounded ring retains the newest [`LEASED_IN_SENT_SAMPLES`] facts.
pub(super) static LEASED_IN_SENT: std::sync::Mutex<PromptSamples> =
    std::sync::Mutex::new(PromptSamples::new());

/// Each semantic role retains its own existing history depth. Successful small
/// turns cannot age unmet demand out merely by being more frequent.
pub(super) struct PromptSamples {
    requested: std::collections::VecDeque<crate::cognition::working_set::PromptSample>,
    served: std::collections::VecDeque<crate::cognition::working_set::PromptSample>,
}

impl PromptSamples {
    pub(super) const fn new() -> Self {
        Self {
            requested: std::collections::VecDeque::new(),
            served: std::collections::VecDeque::new(),
        }
    }

    pub(super) fn record(&mut self, sample: crate::cognition::working_set::PromptSample) {
        use crate::cognition::working_set::PromptSample;
        let history = match &sample {
            PromptSample::Requested(0) | PromptSample::Served { tokens: 0, .. } => return,
            PromptSample::Requested(_) => &mut self.requested,
            PromptSample::Served { .. } => &mut self.served,
        };
        history.push_back(sample);
        while history.len() > LEASED_IN_SENT_SAMPLES {
            history.pop_front();
        }
    }

    pub(super) fn snapshot(&self) -> Vec<crate::cognition::working_set::PromptSample> {
        self.requested
            .iter()
            .chain(self.served.iter())
            .cloned()
            .collect()
    }
}
/// How many leased-in prompt sizes the floor pool remembers. Enough for a median over a
/// grid's worth of coders across several ticks; small enough that a stale burst ages out.
pub const LEASED_IN_SENT_SAMPLES: usize = 64;
/// A completed leased-in call supplies growth demand. Its caller does not own the
/// engine generation anymore, so only the adapter may certify observed service.
/// Zero is the empty-completion fault's territory, not a sample.
pub fn note_leased_in_sent(input_tokens: u32) {
    note_prompt_sample(crate::cognition::working_set::PromptSample::Requested(input_tokens));
}

/// Called while the local engine generation's admission is still owned and its
/// explicit successful terminal frame has arrived. Never sampled from a later binding.
pub(crate) fn note_served_prompt(endpoint: &str, geometry: ServedGeometry, input_tokens: u32) {
    if geometry.model_id.is_empty() || input_tokens == 0 {
        return;
    }
    note_prompt_sample(crate::cognition::working_set::PromptSample::Served {
        endpoint: endpoint.to_owned(),
        geometry,
        tokens: input_tokens,
    });
}

pub(super) fn note_prompt_sample(sample: crate::cognition::working_set::PromptSample) {
    let mut ring = LEASED_IN_SENT.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: a poisoned ring reads its last state, same policy as every ledger lock here
    ring.record(sample);
}
/// A prompt this seat REFUSED at its served window (`serving_guard`, #175) is demand the
/// seat was asked for and could not hold — it joins the typical-prompt pool exactly as a
/// served leased-in prompt does. Without this the pool saw only what fit: the 5090
/// (2026-09-20) refused 2,932 prompts at 2 × 2048 (p50 56k) while its median read 828,
/// because a refusal never reached the sample site and the window sealed itself.
pub fn note_refused_prompt(prompt_tokens: u32) {
    note_prompt_sample(crate::cognition::working_set::PromptSample::Requested(prompt_tokens));
}

/// The remembered leased-in prompt sizes, oldest first — the plan's extra median inputs.
pub fn leased_in_sent_samples() -> Vec<crate::cognition::working_set::PromptSample> {
    LEASED_IN_SENT
        .lock()
        .unwrap_or_else(|e| e.into_inner()) // unwrap_or_else: same policy — read the last state
        .snapshot()
}
/// THE SEAT'S OWN QUEUE, measured where it forms: for each leased-in generate, the wait from
/// the seat RECEIVING the request to its first progress (a prefill frame or a token) —
/// receipt-to-first-token, in ms. A citizen's `delib.gate.lane_wait` is stamped on her HOME
/// node and includes the wire; this is the number the seat can honestly advertise, and the
/// number a spiller must compare against the latency budget before choosing this seat
/// (card c84d885a, S1: a seat whose own wait is minutes is not capacity, whatever its rank).
/// Bounded ring, newest kept; the beacon carries the p50 AND the sample count, so
/// "0 ms, 0 samples" (unmeasured) is never read as "0 ms wait".
pub(super) static LEASED_IN_FIRST_PROGRESS_MS: WaitRing = WaitRing::new();
/// Record one leased-in generate's receipt-to-first-progress wait.
pub fn note_leased_in_first_progress_ms(ms: u64) {
    LEASED_IN_FIRST_PROGRESS_MS.note(ms);
}
/// The seat's leased-in wait as (p50 ms, samples). `samples == 0` means UNMEASURED — the p50
/// is then 0 and must not be read as a fast seat; the beacon carries both for that reason.
pub fn leased_in_wait_p50_ms() -> (u64, u32) {
    LEASED_IN_FIRST_PROGRESS_MS.p50()
}
/// THIS NODE'S OWN LANE QUEUE: each home turn's wait at the serving-lane admission gate
/// (`delib.gate.lane_wait` → `lane_acquired`), in ms. The comparator a spill is judged
/// against: a peer seat is capacity only when its measured wait is UNDER this node's own —
/// "offload only when the remote turn is expected faster" (card c84d885a, S1b). A median
/// under a millisecond means the lanes here are not contended, and nobody moves.
pub(super) static LOCAL_LANE_WAIT_MS: WaitRing = WaitRing::new();
/// Record one home turn's wait for a serving lane on this node.
pub fn note_local_lane_wait_ms(ms: u64) {
    LOCAL_LANE_WAIT_MS.note(ms);
}
/// This node's own lane wait as (p50 ms, samples); `samples == 0` = UNMEASURED.
pub fn local_lane_wait_p50_ms() -> (u64, u32) {
    LOCAL_LANE_WAIT_MS.p50()
}
/// What a DIRECTED call waited for its lane here — the unit the reserved lane's guarantee
/// is stated in (Cormac, 2026-09-20: "the reserve is a WAIT guarantee for a directed call,
/// not a lane count"). Fed at the faculty's lane gate for priority turns; read by the
/// hourly health line as p50 / p90 so a reader sees what a human waited, not how many
/// lanes stood idle.
pub(super) static DIRECTED_LANE_WAIT_MS: WaitRing = WaitRing::new();
pub fn note_directed_lane_wait_ms(ms: u64) {
    DIRECTED_LANE_WAIT_MS.note(ms);
}
/// (p50 ms, p90 ms, samples); `samples == 0` = no directed call waited this window.
pub fn directed_lane_wait_ms() -> (u64, u64, u32) {
    DIRECTED_LANE_WAIT_MS.p50_p90()
}
/// HOW LONG A LANE IS HELD HERE — each `ServingLanePermit`'s own lifetime (queue
/// admission → release), in ms. Already probed per release as `admission.lane.released
/// held_ms`; a probe is not a number the substrate can compute with, and this is the
/// one the lane wait's bound is derived FROM: the lanes' measured turn time.
pub(super) static LANE_HOLD_MS: WaitRing = WaitRing::new();
/// Record one serving-lane permit's held time. Called from the permit's `Drop`.
pub(super) fn note_lane_held_ms(ms: u64) {
    LANE_HOLD_MS.note(ms);
}
/// The lanes' measured turn time as (p50 ms, samples); `samples == 0` = UNMEASURED,
/// and the p50 is then 0 and must never be read as an instant lane.
pub fn lane_hold_p50_ms() -> (u64, u32) {
    LANE_HOLD_MS.p50()
}
/// Held-work callers queued for the non-directed budget RIGHT NOW — the depth of the
/// queue ahead of a mind about to park at the gate.
pub fn work_waiting_now() -> usize {
    LANES.work_waiting()
}

/// A lane wait longer than this is not a queue, it is a starve. The CEILING on the
/// derived bound below, and the bound used outright when the queue is UNMEASURED
/// (a fresh boot): busy is not dead, so it is generous — but it sits well under the
/// per-act `TICK_DEADLINE` (25 min) so a starved mind defers at HER OWN named bound
/// with her act budget intact, instead of the act's deadline expiring on her
/// (`residue_ms = 1,500,001` on the M5, 2026-09-20).
pub const LANE_WAIT_CEILING: std::time::Duration = std::time::Duration::from_secs(10 * 60);
/// The FLOOR on the derived bound: no mind is told a lane is hopeless before this,
/// however fast the lanes look. A measured p50 is a median, not a promise.
pub const LANE_WAIT_FLOOR: std::time::Duration = std::time::Duration::from_secs(60);
/// How many times the queue's own clearing estimate a mind will wait before she
/// defers. The estimate is a MEDIAN — half the turns run longer — so the bound is a
/// multiple of it, never the estimate itself.
pub const LANE_WAIT_SLACK: u32 = 2;

/// PURE: how long a mind should wait for a serving lane, derived from the QUEUE she
/// is joining rather than from a constant.
///
/// `queue_ahead` is the held-work callers already parked ([`work_waiting_now`]);
/// `lanes` the lanes serving; `hold_p50_ms` / `hold_samples` the lanes' own measured
/// turn time ([`lane_hold_p50_ms`]). Her place in line is `queue_ahead + 1`, so the
/// rounds of lane turnover ahead of her is `ceil((queue_ahead + 1) / lanes)` and the
/// honest wait is that many measured turns, times [`LANE_WAIT_SLACK`], clamped into
/// [[`LANE_WAIT_FLOOR`], [`LANE_WAIT_CEILING`]].
///
/// `hold_samples == 0` is UNMEASURED and returns `None`: the caller then uses
/// [`LANE_WAIT_CEILING`], because a node that has never timed a lane may not guess a
/// short one ([[unknown-is-not-a-quantity]]).
pub fn lane_wait_bound(
    queue_ahead: usize,
    lanes: usize,
    hold_p50_ms: u64,
    hold_samples: u32,
) -> Option<std::time::Duration> {
    if hold_samples == 0 || hold_p50_ms == 0 {
        return None;
    }
    let lanes = lanes.max(1);
    let rounds = (queue_ahead + 1).div_ceil(lanes) as u64;
    let ms = rounds
        .saturating_mul(hold_p50_ms)
        .saturating_mul(LANE_WAIT_SLACK as u64);
    Some(
        std::time::Duration::from_millis(ms)
            .clamp(LANE_WAIT_FLOOR, LANE_WAIT_CEILING),
    )
}
/// PURE: may a WORK call be lent the lane reserved for directed calls? Only while no
/// directed line is pending for any citizen here, and only when the call's expected
/// occupancy — prefill of its uncached prompt at the measured prefill rate plus its
/// decode at the measured decode rate — fits inside the directed wait budget, so the
/// most a directed arrival can wait is that bound. Unknown occupancy lends nothing.
///
/// Measured 2026-09-20 on the M5 (2 lanes): the reserve of lanes−1 left ONE non-directed
/// lane for four minds at 3–6 min a turn — 11 grants an hour with a lane idle for a
/// mention that came once all night. The reserve stays a guarantee; it is now stated in
/// the seconds a directed call may wait, not in lanes.
pub fn reserve_lendable(
    expected: Option<std::time::Duration>,
    directed_pending: bool,
    budget: std::time::Duration,
) -> bool {
    !directed_pending && expected.is_some_and(|e| e <= budget)
}
/// A bounded ring of wait samples (ms), newest kept, read as (p50, count) so an empty ring
/// is UNMEASURED — never "0 ms". One shape for every queue a node measures about itself.
pub struct WaitRing(std::sync::Mutex<std::collections::VecDeque<u64>>);
/// How many waits a ring remembers. A plan window's worth.
pub const WAIT_RING_SAMPLES: usize = 64;
impl WaitRing {
    pub const fn new() -> Self {
        Self(std::sync::Mutex::new(std::collections::VecDeque::new()))
    }
    pub fn note(&self, ms: u64) {
        let mut ring = self.0.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: a poisoned ring reads its last state, same policy as every ledger lock here
        ring.push_back(ms);
        while ring.len() > WAIT_RING_SAMPLES {
            ring.pop_front();
        }
    }
    pub(super) fn p50(&self) -> (u64, u32) {
        let ring = self.0.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: same policy — read the last state
        p50_of(ring.iter().copied())
    }
    pub fn p50_p90(&self) -> (u64, u64, u32) {
        let ring = self.0.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: same policy — read the last state
        percentiles_of(ring.iter().copied())
    }
}
/// PURE: (p50, p90, count) of a sample stream; (0, 0, 0) for none. p90 is the value at
/// the 90th-percentile rank (nearest-rank), so a single sample is its own p90.
pub fn percentiles_of(samples: impl Iterator<Item = u64>) -> (u64, u64, u32) {
    let mut v: Vec<u64> = samples.collect();
    if v.is_empty() {
        return (0, 0, 0);
    }
    v.sort_unstable();
    let n = v.len();
    let p90 = v[((n * 9).div_ceil(10)).saturating_sub(1).min(n - 1)];
    (v[n / 2], p90, n.min(u32::MAX as usize) as u32)
}
/// PURE: the median of a sample stream with its count; (0, 0) for none.
pub fn p50_of(samples: impl Iterator<Item = u64>) -> (u64, u32) {
    let mut v: Vec<u64> = samples.collect();
    if v.is_empty() {
        return (0, 0);
    }
    v.sort_unstable();
    (v[v.len() / 2], v.len().min(u32::MAX as usize) as u32)
}
/// Leased-in calls outstanding right now.
pub fn leased_in_calls() -> usize {
    LEASED_IN_CALLS.inflight()
}
/// The peak concurrent leased-in calls since the planner last asked — the seat's measured
/// demand from the rest of the grid, one number per plan tick. Re-arms at the current
/// concurrency so a still-running call carries into the next interval.
pub fn take_leased_in_peak() -> usize {
    LEASED_IN_CALLS.take_peak()
}

/// Deliberative model calls outstanding against the shared serving target right now.
/// The served lane count the admission gate is sized to (0 before the first plan).
pub fn served_lane_count() -> usize {
    LANES.lane_count()
}

pub fn inflight_model_calls() -> usize {
    INFLIGHT_MODEL_CALLS.inflight()
}

/// True when every shared decode slot is busy, so one more call would QUEUE behind
/// the fleet. Threshold is the LIVE served lane count (`LANES.lane_count()` — the real
/// `--parallel` slots serving reports, `set_served_lane_count`), falling back to the
/// `MAX_LANES` backstop only before the first plan lands. This must track the live count,
/// not the `MAX_LANES` ceiling: once the ceiling can exceed the served count (#266 raised it
/// 2 → 8), keying saturation off the ceiling would let a 2-slot host admit 8 in-flight and
/// queue 6 at the backend before a self-tick ever yields. A self-tick yields on this;
/// message/directed turns are never gated on it (a human/peer waits).
pub fn shared_model_saturated() -> bool {
    INFLIGHT_MODEL_CALLS.saturated(LANES.lane_count())
}

// ── Ambient-turn admission permit (#171) ───────────────────────────────────────
//
// The inflight GAUGE above catches STAGGERED load, but not the acute fan-out: a room
// burst wakes N peers simultaneously and they all read the gauge as 0 (none has
// generated yet) and stampede. A PERMIT fixes the timing: an ambient (non-directed)
// turn must ACQUIRE one of a small fixed number of ambient slots or it yields — held
// across the turn, so concurrency is bounded regardless of when everyone woke.
// Directed turns bypass this entirely (they were addressed; they run now). A yielded
// ambient turn defers to a later beat with free capacity — the durable transcript is
// unchanged, so nothing is lost. [[conversational-latency-is-a-misdirection-budget]]

// How many ambient turns may run at once: [`LaneAdmission::nondirected_budget`] — the
// SAME live (lanes − 1, floored at 1) budget the per-call lane reservation below uses.
// One machine, one answer to "how much non-directed concurrency is there", derived from
// live capacity instead of declared twice.
//
// This was `const AMBIENT_TURN_CONCURRENCY: usize = 1` until 2026-08-17, and the constant
// was the roster's starvation CEILING. Two things made it invisible:
//   1. `service_loop.rs`'s self-tick gate documents this permit as "sized to the LIVE
//      served lane count (LaneAdmission ← set_served_lane_count)". It never was — it was
//      a bare 1, and the comment described the design that was intended.
//   2. A hard 1 is indistinguishable from a quiet room at n≈1 citizen, which is how it
//      was reasoned about ("under light load ambient turns are naturally serial").
// Measured on this box 2026-08-17: 4 served lanes, 20+ hosted citizens, so 3 non-directed
// lanes sat permanently idle while every citizen but one yielded on a pool of 1.
//
// Why lowering the bound is still safe — the directed-turn guarantee never came from THIS
// permit. It comes from the per-call reservation (`acquire_serving_lane`), which caps
// non-directed calls at lanes−1 so a directed call always finds a lane. That is the layer
// that owns lane priority, and it is untouched. This permit's own job (#171) is anti-
// STAMPEDE: bound the fan-out when N peers wake on the same beat and all read inflight=0.
// A bound of lanes−1 does that job exactly as well as a bound of 1 — it is still fixed,
// still acquired non-blockingly, still held across the whole turn — while no longer
// throttling below the hardware. On a 1- or 2-lane box the budget floors at 1, so weak
// machines get byte-identical behaviour to before.

/// Ambient turns that YIELDED because the pool was full, since process start.
///
/// Why this exists (2026-08-17): the yield path emits no probe — deliberately, because at
/// 24 citizens on a 15s beat a per-yield row would be ~92 rows/min and would drown the
/// stream exactly as `serving.plan` did at 2.6 rows/s (#399). But the ABSENCE of a row made
/// starvation indistinguishable from health: a 10-minute window showing zero
/// `persona.selftick.*` reads identically whether one citizen is mid-turn holding the only
/// lane or nobody is ticking at all. Resolving that took a manual `/slots` curl. A counter
/// costs one relaxed increment and makes the difference reportable.
pub(super) static AMBIENT_YIELDS: AtomicUsize = AtomicUsize::new(0);
/// Epoch-ms of the last starvation report, so the emit is rate-limited rather than per-yield.
pub(super) static AMBIENT_YIELD_LAST_REPORT_MS: AtomicUsize = AtomicUsize::new(0);

/// Minimum gap between starvation reports. One row a minute is free next to a 15s beat and
/// still resolves "is the roster moving?" at the granularity anyone asks it.
pub(super) const AMBIENT_YIELD_REPORT_GAP_MS: usize = 60_000;

/// Cumulative ambient yields since process start.
pub fn ambient_yields() -> usize {
    AMBIENT_YIELDS.load(Ordering::Relaxed)
}

/// Should the caller emit a starvation report now, and if so for how many yields?
///
/// Pure over its inputs so the rate-limit rule is unit-testable without a clock: returns
/// `Some(yields_in_window)` at most once per [`AMBIENT_YIELD_REPORT_GAP_MS`]. `last_report_ms`
/// of 0 means "never reported" and always fires, so the FIRST starvation is never silent —
/// which is the one that matters, and the one a naive `now - last >= gap` would swallow on a
/// box whose clock starts near zero.
pub fn ambient_yield_report_due(
    total_yields: usize,
    yields_at_last_report: usize,
    now_ms: usize,
    last_report_ms: usize,
) -> Option<usize> {
    let fresh = total_yields.saturating_sub(yields_at_last_report);
    if fresh == 0 {
        return None;
    }
    if last_report_ms != 0 && now_ms.saturating_sub(last_report_ms) < AMBIENT_YIELD_REPORT_GAP_MS {
        return None;
    }
    Some(fresh)
}

/// Yield total as of the last report — the OTHER half of the window, and the half I
/// originally forgot.
///
/// Shipped 2026-08-17 passing a literal 0 for `yields_at_last_report`, on the reasoning that
/// "the report resets the clock, so total-at-report is implicit". It is not: resetting the
/// TIMESTAMP does not reset the COUNT, so `fresh = total - 0 = total` and every row reported
/// the cumulative figure under a field named `yields_since_last_report`. Caught within the
/// hour by reading the live rows — yields_this_window == cumulative on all six (457==457,
/// 392==392, …) when consecutive rows differed by ~65. The pure `ambient_yield_report_due`
/// was correct and its table test asserted the delta properly; the defect was entirely in
/// this impure wrapper feeding it a wrong argument, which is precisely why the pure/impure
/// split exists and precisely the gap a table test cannot close.
pub(super) static AMBIENT_YIELDS_AT_LAST_REPORT: AtomicUsize = AtomicUsize::new(0);

/// Claim the report slot if due, returning the yields IN THIS WINDOW. Swaps the timestamp
/// atomically so concurrent yielders cannot both emit for the same window, and advances the
/// count watermark in the same critical section so the next window measures from here.
pub fn take_ambient_yield_report(now_ms: u64) -> Option<usize> {
    let now = now_ms as usize;
    let last = AMBIENT_YIELD_LAST_REPORT_MS.load(Ordering::Relaxed);
    let total = AMBIENT_YIELDS.load(Ordering::Relaxed);
    let at_last = AMBIENT_YIELDS_AT_LAST_REPORT.load(Ordering::Relaxed);
    let fresh = ambient_yield_report_due(total, at_last, now, last)?;
    // Only the winner of this compare-exchange reports…
    if AMBIENT_YIELD_LAST_REPORT_MS
        .compare_exchange(last, now, Ordering::AcqRel, Ordering::Relaxed)
        .is_err()
    {
        return None;
    }
    // …and the winner alone advances the count watermark, so the loser's yields are not
    // silently dropped from the NEXT window — they are still ahead of `total` here.
    AMBIENT_YIELDS_AT_LAST_REPORT.store(total, Ordering::Relaxed);
    Some(fresh)
}
