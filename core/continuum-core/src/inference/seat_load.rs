//! The serving seat's own load, measured where it forms (moved from
//! `cognition::resource_admission`, card c5cb7a04, crate peel step 5a): the in-flight model-call
//! gauge, the leased-in call gauge with its per-interval peak, the prompt sizes leased-in
//! calls brought, and the bounded wait ring every node-local queue measurement shares. These
//! are inference's facts about itself; cognition's admission and planning READ them, so they
//! live below cognition and inference never reaches up for them.

use std::sync::atomic::{AtomicUsize, Ordering};

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
static INFLIGHT_MODEL_CALLS: Gauge = Gauge::new();

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
struct Gauge {
    count: AtomicUsize,
}

impl Gauge {
    const fn new() -> Self {
        Self {
            count: AtomicUsize::new(0),
        }
    }

    /// Enter one in-flight call; the returned guard decrements the SAME gauge on drop.
    fn enter(&self) -> GaugeGuard<'_> {
        self.count.fetch_add(1, Ordering::AcqRel);
        GaugeGuard(&self.count)
    }

    /// Outstanding calls right now.
    fn inflight(&self) -> usize {
        self.count.load(Ordering::Acquire)
    }

    /// True when `outstanding >= max_lanes` — one more call would queue behind the fleet.
    fn saturated(&self, max_lanes: usize) -> bool {
        self.inflight() >= max_lanes
    }
}

/// RAII decrement for one gauge entry. Borrows the gauge it incremented so drop decrements
/// exactly that gauge — the shared instance for a production guard, a test's local instance for
/// a test's. This borrow is what makes the gauge injectable without a second counter.
#[derive(Debug)]
struct GaugeGuard<'a>(&'a AtomicUsize);

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
struct PeakGauge {
    gauge: Gauge,
    peak: AtomicUsize,
}
impl PeakGauge {
    const fn new() -> Self {
        Self {
            gauge: Gauge::new(),
            peak: AtomicUsize::new(0),
        }
    }
    /// Enter one call and fold the new concurrency into the peak.
    fn enter(&self) -> GaugeGuard<'_> {
        let guard = self.gauge.enter();
        self.peak.fetch_max(self.gauge.inflight(), Ordering::AcqRel);
        guard
    }
    fn inflight(&self) -> usize {
        self.gauge.inflight()
    }
    /// The peak concurrency since the previous take, then re-arm at the CURRENT
    /// concurrency — a call still in flight at the read is not forgotten by the reset.
    fn take_peak(&self) -> usize {
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
/// the resident lane demand (`ServingDemand::leased_in` in the serving plan).
/// It is fed at the seam where an inbound remote generate is executed, which today enters
/// neither this gauge nor the admission gate — the wire slice of the same card.
static LEASED_IN_CALLS: PeakGauge = PeakGauge::new();

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
/// The PROMPT SIZES of leased-in generates this seat served recently — the samples the
/// serving plan folds into its per-lane window floor beside its own residents' sent peaks.
/// Without them a two-resident seat's "typical prompt" is one resident's SWE prompt, and
/// the plan grows lanes for the grid (`leased_in`) it then refuses to fit (BigMama,
/// 2026-09-18: the 5090 considered 2 × 49k every tick and kept 1 × 101k, because 49k was
/// under a floor computed from Sahar's prompts alone while twelve leased-in coders sent
/// ~30k). Bounded ring: the newest [`LEASED_IN_SENT_SAMPLES`] measured `usage.input_tokens`.
static LEASED_IN_SENT: std::sync::Mutex<std::collections::VecDeque<u32>> =
    std::sync::Mutex::new(std::collections::VecDeque::new());
/// How many leased-in prompt sizes the floor pool remembers. Enough for a median over a
/// grid's worth of coders across several ticks; small enough that a stale burst ages out.
pub const LEASED_IN_SENT_SAMPLES: usize = 64;
/// Record one leased-in generate's measured prompt size (the server's own count, never an
/// estimate). Zero is the empty-completion fault's territory, not a sample.
pub fn note_leased_in_sent(input_tokens: u32) {
    if input_tokens == 0 {
        return;
    }
    let mut ring = LEASED_IN_SENT.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: a poisoned ring reads its last state, same policy as every ledger lock here
    ring.push_back(input_tokens);
    while ring.len() > LEASED_IN_SENT_SAMPLES {
        ring.pop_front();
    }
}
/// A prompt this seat REFUSED at its served window (`serving_guard`, #175) is demand the
/// seat was asked for and could not hold — it joins the typical-prompt pool exactly as a
/// served leased-in prompt does. Without this the pool saw only what fit: the 5090
/// (2026-09-20) refused 2,932 prompts at 2 × 2048 (p50 56k) while its median read 828,
/// because a refusal never reached the sample site and the window sealed itself.
pub fn note_refused_prompt(prompt_tokens: u32) {
    note_leased_in_sent(prompt_tokens);
}

/// The remembered leased-in prompt sizes, oldest first — the plan's extra median inputs.
pub fn leased_in_sent_samples() -> Vec<u32> {
    LEASED_IN_SENT
        .lock()
        .unwrap_or_else(|e| e.into_inner()) // unwrap_or_else: same policy — read the last state
        .iter()
        .copied()
        .collect()
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
    pub(crate) fn p50(&self) -> (u64, u32) {
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
pub fn inflight_model_calls() -> usize {
    INFLIGHT_MODEL_CALLS.inflight()
}

/// True when the outstanding model calls fill `lanes` decode slots, so one more would queue
/// behind the fleet. The lane count is the caller's (the admission gate's live served count).
pub fn model_calls_saturate(lanes: usize) -> bool {
    INFLIGHT_MODEL_CALLS.saturated(lanes)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card c84d885a): a seat that serves other nodes' minds and never
    // counts them. The planner must see "how many did I serve AT ONCE this interval", so
    // the gauge keeps a peak; and a call still in flight when the planner reads must not
    // vanish from the next interval (the re-arm-at-current rule), or a long remote turn
    // spanning two ticks would read as zero demand on the second.
    #[test]
    fn a_leased_in_gauge_remembers_its_peak_per_interval_and_never_forgets_a_running_call() {
        let g = PeakGauge::new();
        assert_eq!(g.take_peak(), 0, "nothing served yet");
        let a = g.enter();
        let b = g.enter();
        let c = g.enter();
        drop(b);
        assert_eq!(g.inflight(), 2);
        // the interval's peak was 3, not the 2 in flight at read time
        assert_eq!(g.take_peak(), 3);
        // re-armed at the CURRENT concurrency: the two still running are the floor of the
        // next interval, not forgotten
        assert_eq!(g.take_peak(), 2);
        drop(a);
        drop(c);
        assert_eq!(g.inflight(), 0);
        assert_eq!(g.take_peak(), 2, "the last interval still saw two running");
        assert_eq!(g.take_peak(), 0, "and now nothing");
    }
    // what this catches (#139 idle admission): the in-flight gauge counts each
    // model-call entry, releases on drop, and reads SATURATED exactly at the serving
    // concurrency (the served lane count) — the signal a self-tick yields on so an
    // idle deliberation never deepens the queue live conversation waits behind.
    //
    // Drives a LOCAL Gauge instance — the SAME type production's process-global
    // INFLIGHT_MODEL_CALLS is, but one nothing else in the process can reach — so the
    // absolute-count assertions are deterministic under cargo's parallel execution. The
    // prior version read the process-global counter and a sibling test's InflightModelCall
    // guard could bump it mid-loop (the #1960 flaky class, canary red 2026-07-25); scoping
    // the gauge kills that race at the root, so no TEST_SERIAL / drain-wait is needed here.
    #[test]
    fn inflight_gauge_counts_releases_and_saturates_at_serving_concurrency() {
        // The gauge's semantics at any serving concurrency; the live count is the caller's.
        let max = 4usize;
        let gauge = Gauge::new();

        assert_eq!(gauge.inflight(), 0, "a fresh gauge starts empty");
        assert!(!gauge.saturated(max), "an empty gauge is not saturated");

        let mut guards = Vec::new();
        for expected in 1..=max {
            guards.push(gauge.enter());
            assert_eq!(gauge.inflight(), expected);
        }
        // Every decode slot busy → one more call would queue behind the fleet.
        assert!(
            gauge.saturated(max),
            "MAX_LANES outstanding must read saturated"
        );

        guards.pop(); // free a slot
        assert_eq!(gauge.inflight(), max - 1);
        assert!(
            !gauge.saturated(max),
            "freeing a slot clears saturation — an idle self-tick may run again"
        );

        drop(guards);
        assert_eq!(gauge.inflight(), 0, "all guards released → baseline");
    }
    // what this catches (card c84d885a, S1b): the node's OWN lane wait is one more ring of the
    // same shape as the seat's — a median with its count, (0, 0) when nothing was measured —
    // so a chooser never reads an unmeasured home as "no queue" or a measured home as "0".
    #[test]
    fn a_nodes_own_lane_wait_is_a_median_with_a_count_of_the_same_shape_as_the_seats() {
        let ring = WaitRing::new();
        assert_eq!(ring.p50(), (0, 0), "unmeasured is (0, 0), never a fast lane");
        for ms in [40_000u64, 5, 210_000, 7, 9] {
            ring.note(ms);
        }
        assert_eq!(ring.p50(), (9, 5), "the median, not the mean — one outlier is not the queue");
        for _ in 0..(WAIT_RING_SAMPLES * 2) {
            ring.note(1);
        }
        assert_eq!(ring.p50(), (1, WAIT_RING_SAMPLES as u32), "bounded: the window forgets");
    }
    // what this catches (card c84d885a, S1): a seat's queue measured where it forms, and
    // "unmeasured" never reading as "fast". The beacon carries (p50, samples); a spiller
    // that saw p50 = 0 with 0 samples and treated it as a zero-wait seat would pile onto
    // an unmeasured one — the same absence-read-as-a-value error that cost a day.
    #[test]
    fn a_seats_measured_wait_is_a_median_with_a_count_and_unmeasured_is_not_fast() {
        assert_eq!(p50_of(std::iter::empty()), (0, 0), "no samples: unmeasured, not 0 ms");
        assert_eq!(p50_of([5_000u64].into_iter()), (5_000, 1));
        // the median, not the mean: one 7,058 s outlier does not become the seat's wait
        assert_eq!(p50_of([100u64, 200, 300, 7_058_000].into_iter()).0, 300);
        assert_eq!(p50_of([216_000u64, 216_000, 1_387_000].into_iter()), (216_000, 3));
    }
    // what this catches (BigMama's correction on #4197): leased-in LANES with a floor from
    // local residents alone — the seat grows demand it then refuses to fit. The prompt
    // sizes of the generates a seat serves for others must be remembered (bounded, newest
    // kept, zero never a sample) so the plan's median floor sees the grid's prompts.
    #[test]
    fn leased_in_prompt_sizes_are_remembered_bounded_and_zero_is_not_a_sample() {
        // the ring is process-global; assert RELATIVE to what is there (the #1960 rule)
        let before = leased_in_sent_samples().len();
        note_leased_in_sent(0);
        assert_eq!(leased_in_sent_samples().len(), before, "an empty completion is not a prompt size");
        // what this catches (the 5090, 2026-09-20): a refusal at the served window is a
        // sample of the demand the seat could not hold — it must reach the pool, or the
        // pool only ever sees what fit and the window measures itself.
        note_refused_prompt(56_057);
        assert_eq!(leased_in_sent_samples().last().copied(), Some(56_057), "a refused prompt votes at its real size");
        for i in 1..=(LEASED_IN_SENT_SAMPLES as u32 + 8) {
            note_leased_in_sent(30_000 + i);
        }
        let s = leased_in_sent_samples();
        assert_eq!(s.len(), LEASED_IN_SENT_SAMPLES, "bounded");
        assert_eq!(*s.last().expect("nonempty"), 30_000 + LEASED_IN_SENT_SAMPLES as u32 + 8, "newest kept");
        assert!(s.first().copied().expect("nonempty") > 30_000 + 8, "oldest aged out");
    }
}
