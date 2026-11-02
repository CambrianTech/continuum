//! Admission metrics: in-flight gauges, wait/hold rings and ambient-yield counters.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use crate::inference::served_window_store::ServedGeometry;

/// Requested capacity drives growth; only an owned, completed engine call
/// establishes observed service. A refusal cannot be interpreted as a success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptSample {
    Served { endpoint: String, tokens: u32, geometry: ServedGeometry },
    Requested(u32),
}

impl PromptSample {
    pub fn requested(&self) -> Option<u32> {
        match self {
            Self::Requested(tokens) if *tokens > 0 => Some(*tokens),
            _ => None,
        }
    }

    pub fn served(&self, endpoint: &str, model: Option<&str>) -> Option<u32> {
        match self {
            Self::Served { endpoint: sampled_endpoint, geometry, tokens }
                if *tokens > 0 && sampled_endpoint == endpoint && model == Some(geometry.model_id.as_str()) => Some(*tokens),
            _ => None,
        }
    }

    /// Exact generation cohort: a later binding never certifies an older sample.
    pub fn geometry_median(samples: &[Self], endpoint: &str, geometry: &ServedGeometry) -> Option<u32> {
        let mut observed: Vec<u32> = samples.iter().filter_map(|sample| match sample {
            Self::Served { geometry: g, .. } if g == geometry => sample.served(endpoint, Some(&geometry.model_id)),
            _ => None,
        }).collect();
        observed.sort_unstable();
        observed.get(observed.len() / 2).copied()
    }
}

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
/// the resident lane demand ([`crate::inference::planning::serving_plan::ServingDemand::leased_in`]).
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
    requested: std::collections::VecDeque<PromptSample>,
    served: std::collections::VecDeque<PromptSample>,
}

impl PromptSamples {
    pub(super) const fn new() -> Self {
        Self {
            requested: std::collections::VecDeque::new(),
            served: std::collections::VecDeque::new(),
        }
    }

    pub(super) fn record(&mut self, sample: PromptSample) {
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

    pub(super) fn snapshot(&self) -> Vec<PromptSample> {
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
    note_prompt_sample(PromptSample::Requested(input_tokens));
}

/// Called while the local engine generation's admission is still owned and its
/// explicit successful terminal frame has arrived. Never sampled from a later binding.
pub(crate) fn note_served_prompt(endpoint: &str, geometry: ServedGeometry, input_tokens: u32) {
    if geometry.model_id.is_empty() || input_tokens == 0 {
        return;
    }
    note_prompt_sample(PromptSample::Served {
        endpoint: endpoint.to_owned(),
        geometry,
        tokens: input_tokens,
    });
}

pub(super) fn note_prompt_sample(sample: PromptSample) {
    let mut ring = LEASED_IN_SENT.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: a poisoned ring reads its last state, same policy as every ledger lock here
    ring.record(sample);
}
/// A prompt this seat REFUSED at its served window (`serving_guard`, #175) is demand the
/// seat was asked for and could not hold — it joins the typical-prompt pool exactly as a
/// served leased-in prompt does. Without this the pool saw only what fit: the 5090
/// (2026-09-20) refused 2,932 prompts at 2 × 2048 (p50 56k) while its median read 828,
/// because a refusal never reached the sample site and the window sealed itself.
pub fn note_refused_prompt(prompt_tokens: u32) {
    note_prompt_sample(PromptSample::Requested(prompt_tokens));
}

/// The remembered leased-in prompt sizes, oldest first — the plan's extra median inputs.
pub fn leased_in_sent_samples() -> Vec<PromptSample> {
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
/// PURE: may a WORK call be lent the lane reserved for directed calls? Whenever no directed
/// line is pending here. A directed arrival then takes the next lane any call releases.
///
/// The old rule also required the call to fit the interactive wait budget. No resident act
/// does (median model call 71 s), so the reserve was never lent: one of four engine slots
/// sat idle for a mention while the fourth resident queued, a median 25.7 s and p90 131 s
/// per model call (M5, 176 acts, 2026-10-28, build 7ace74020). An idle slot is never the
/// price of a guarantee for an event that comes a few times a day.
pub fn reserve_lendable(directed_pending: bool) -> bool {
    !directed_pending
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
