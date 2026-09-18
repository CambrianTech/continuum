//! Shared Rust resource admission.
//!
//! This is the small lease gate that every expensive subsystem can use
//! while the substrate governor becomes the process-wide allocator:
//! inference, training, rendering, audio, TTS, STT, classifiers, RAG,
//! and background work. Callers submit typed resource policy; the gate
//! admits or denies before work starts and returns an RAII guard that
//! releases the lease on every exit path.

use crate::cognition::adaptive_throughput::{ResourceClass, TargetSilicon};
use crate::cognition::throughput_lease::{
    ThroughputLease, ThroughputLeaseError, ThroughputLeaseRegistry, ThroughputLeaseRevocationPolicy,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};
use ts_rs::TS;

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
/// the resident lane demand ([`crate::cognition::serving_plan::ServingDemand::leased_in`]).
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
/// The remembered leased-in prompt sizes, oldest first — the plan's extra median inputs.
pub fn leased_in_sent_samples() -> Vec<u32> {
    LEASED_IN_SENT
        .lock()
        .unwrap_or_else(|e| e.into_inner()) // unwrap_or_else: same policy — read the last state
        .iter()
        .copied()
        .collect()
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
static AMBIENT_YIELDS: AtomicUsize = AtomicUsize::new(0);
/// Epoch-ms of the last starvation report, so the emit is rate-limited rather than per-yield.
static AMBIENT_YIELD_LAST_REPORT_MS: AtomicUsize = AtomicUsize::new(0);

/// Minimum gap between starvation reports. One row a minute is free next to a 15s beat and
/// still resolves "is the roster moving?" at the granularity anyone asks it.
const AMBIENT_YIELD_REPORT_GAP_MS: usize = 60_000;

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
static AMBIENT_YIELDS_AT_LAST_REPORT: AtomicUsize = AtomicUsize::new(0);

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

/// Try to claim an ambient-turn slot. `Some(permit)` → run the ambient turn (hold the
/// permit for the turn's lifetime; it releases on drop). `None` → all ambient slots
/// are busy; the caller yields this ambient turn. Non-blocking (never waits).
pub fn try_hold_ambient_turn() -> Option<tokio::sync::OwnedSemaphorePermit> {
    LANES.try_hold_ambient_turn()
}

/// An idle mind that has not started a turn for this long is STARVING: her next ambient
/// try may wait a bounded moment for a permit instead of losing every race. Measured
/// 2026-09-14 (16 minds, 5 lanes): three idle minds ticked every 75 s for 45 minutes and
/// never won a lane while holders-first ran the pool. Holders first is right under
/// contention; zero for an hour is not — she cannot notice, dream, or pull when a card frees.
pub const IDLE_STARVATION_MS: u64 = 15 * 60 * 1000;
/// How long a starving mind waits for an ambient permit before yielding again.
pub const IDLE_SHARE_WAIT_MS: u64 = 30_000;

static LAST_TURN_MS: std::sync::LazyLock<
    parking_lot::Mutex<std::collections::HashMap<uuid::Uuid, u64>>,
> = std::sync::LazyLock::new(|| parking_lot::Mutex::new(std::collections::HashMap::new()));

static BOOT_MS: std::sync::LazyLock<u64> = std::sync::LazyLock::new(|| {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0) // JUSTIFIED unwrap_or: a pre-epoch clock reads as boot at 0 — every mind then counts as starving at most once
});

/// Stamp that `persona` started a turn now (any kind).
pub fn note_turn_started(persona: uuid::Uuid, now_ms: u64) {
    LAST_TURN_MS.lock().insert(persona, now_ms);
}

/// Pure: is a mind whose last turn was `last_turn_ms` (None = none since boot) starving at `now_ms`?
pub fn starving_since(last_turn_ms: Option<u64>, now_ms: u64, boot_ms: u64) -> bool {
    now_ms.saturating_sub(last_turn_ms.unwrap_or(boot_ms)) >= IDLE_STARVATION_MS
    // JUSTIFIED unwrap_or: no turn yet is measured from boot — absence IS "since boot"
}

/// The ambient permit for `persona`'s musing tail: a plain try, unless she is starving —
/// then a bounded wait, so the pool is still the pool but the race is not rigged against
/// the idle forever. Probe `admission.lane.idle_share` names every granted wait.
pub async fn hold_ambient_turn_for(
    persona: uuid::Uuid,
    now_ms: u64,
) -> Option<tokio::sync::OwnedSemaphorePermit> {
    if let Some(p) = LANES.try_hold_ambient_turn() {
        return Some(p);
    }
    let last = LAST_TURN_MS.lock().get(&persona).copied();
    if !starving_since(last, now_ms, *BOOT_MS) {
        return None;
    }
    let started = std::time::Instant::now();
    match tokio::time::timeout(
        std::time::Duration::from_millis(IDLE_SHARE_WAIT_MS),
        LANES.acquire_resized(
            &LANES.ambient_semaphore(),
            &LANES.ambient_installed,
            LaneBudget::NonDirected,
        ),
    )
    .await
    {
        Ok(permit) => {
            crate::probe!(
                class = "admission.lane.idle_share",
                persona = %persona,
                waited_ms = started.elapsed().as_millis() as u64,
                idle_ms = now_ms.saturating_sub(last.unwrap_or(*BOOT_MS)), // JUSTIFIED unwrap_or: no turn yet = idle since boot
                "a starving idle mind waited for an ambient permit and got one — the minimum share"
            );
            Some(permit)
        }
        _ => None,
    }
}

// ── Serving-lane reservation for directed turns (#139) ──────────────────────────
//
// The ambient PERMIT above bounds how many ambient TURNS run at once (1). But ONE
// permitted turn is not one model call: a single `drive_to_settle` makes many calls over
// minutes (act → observe → act), so the permit-holder alone can occupy several physical
// decode lanes (`llama --parallel`), and an addressed (directed) question then queues
// INSIDE the serving process behind it. Glass-boxed 2026-07-15: a directed turn sat
// 8+ minutes behind one 197s idle self-tick + one 213s ambient turn on the two lanes;
// its latency was lane-QUEUE, not decode (a free-lane turn is ~30-60s).
//
// (That glass-box predates the permit reaching the self-tick. Both non-directed paths are
// permit-gated today — `service_loop.rs` acquires at the self-tick gate AND at the
// message-ambient gate, and they share the one ambient pool. The reservation below is
// still required, because the thing it bounds is CALLS-per-turn, which no turn-level
// permit can see. Corrected 2026-08-17: this paragraph had claimed "an idle self-tick is
// not ambient-permit-gated at all", contradicting the self-tick gate's own comment block
// in the same tree — a stale premise sitting under a live design argument.)
//
// Neither the gauge nor the turn-level permit can fix this — the reservation must live
// at the LANE the model call actually consumes. So every model call acquires a lane
// here, priced by priority:
//   - directed → acquires from the FULL pool (MAX_LANES). Waits only if MAX_LANES other
//                directed calls already hold every lane (all lanes serving live work).
//   - non-directed (ambient + idle self-tick) → additionally bounded to (MAX_LANES-1),
//                so at least ONE lane is always reserved for a directed call.
// No deadlock: directed never touches the non-directed cap, and non-directed holds at
// most MAX_LANES-1 physical lanes, so a directed acquire always finds a free lane unless
// every lane already serves directed. On a single-lane machine there is nothing to
// reserve, so the non-directed budget floors at 1 (no starvation, no false guarantee).
// [[conversational-latency-is-a-misdirection-budget]] [[never-thrash-sticky-hysteresis-on-every-lane]]

/// The LIVE `--parallel` slot count of the running serving target, published by the
/// serving daemon on every reconcile (`set_served_lane_count`). 0 until serving is up.
/// This is the ground truth for how many physical decode lanes exist RIGHT NOW —
/// distinct from `serving_plan::MAX_LANES`, which is only the safety ceiling. Before the
/// compute-buffer fit term (#139), lanes were always == MAX_LANES so the constant was
/// exact; now the plan serves DEMAND (e.g. 4 lanes on this box while MAX_LANES = 6), so
/// sizing the admission semaphores by the constant would over-admit by the difference
/// and weaken the directed-turn reservation. Sizing by THIS keeps the reservation exact.
/// Every piece of admission state that used to be six separate process-global statics
/// (`SERVED_LANE_COUNT`, `SERVING_LANES`, `NONDIRECTED_LANES`, their two installed
/// counters, and the resize lock) plus the ambient-turn permits — owned by ONE type.
///
/// Why (2026-08-06): scattered mutable statics made the tests order-dependent, so they took
/// a `TEST_SERIAL` lock whose own comment admitted it was insufficient — "TEST_SERIAL only
/// serializes THIS module, but a cross-module guard could bump it mid-loop". This file had
/// already learned the lesson once and applied it to the in-flight Gauge, stating the
/// principle outright: *the race is gone at the SOURCE, not merely serialized*. That cure
/// reached one of three cases. This applies it to the other two, and to the sibling
/// [`crate::cognition::prefill_throttle`] the same day.
///
/// Lazy semaphores capture the latest lane count on first use. If first use precedes
/// serving, a later publication collects surplus boot permits without interrupting
/// running work. Subsequent serving ticks retry any held shrink debt.
pub struct LaneAdmission {
    /// LIVE `--parallel` count; 0 until serving publishes one.
    served: AtomicUsize,
    serving: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>>,
    nondirected: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>>,
    serving_installed: AtomicUsize,
    nondirected_installed: AtomicUsize,
    /// Serializes the rare live semaphore reconcile so two concurrent re-plans can't
    /// double-count a grow.
    resize_lock: Mutex<()>,
    ambient: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>>,
    ambient_installed: AtomicUsize,
    /// Held-work callers queued for the non-directed budget right now. While it is
    /// non-zero an AMBIENT caller (a mind with no card in hand) does not enter the
    /// queue — work first (2026-09-15, [`LanePriority`]).
    work_waiting: AtomicUsize,
    /// Fires on every permit release so a parked ambient caller re-checks.
    released: tokio::sync::Notify,
}

#[derive(Clone, Copy, Debug)]
enum LaneBudget {
    Physical,
    NonDirected,
}

/// Who is asking for a lane, priced by what the grid gets back for it.
///
/// Measured 2026-09-15 04:20–05:00Z on the M5 (16 minds, 4 lanes @58k): 33 lane holds
/// at 302 s p50 / 617 s p90, 14 acts, 0 writes, and only 5 of them message turns —
/// the rest were self-cycle turns, and a mind musing with no card queued for the same
/// non-directed budget as a mind holding a card mid-edit. The health receipt read
/// READING for the hour. A lane is the scarce thing; it goes to the mind that will
/// write first.
///
/// - `Directed` — a human or a mention named her: the full pool, one lane always reserved.
/// - `Work` — she holds a card in progress (`holds_live_work`): the non-directed budget,
///   ahead of every ambient caller.
/// - `Ambient` — an undirected turn with no card in hand (musing, wandering, a dream):
///   the non-directed budget only while no `Work` caller is queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanePriority {
    Directed,
    Work,
    Ambient,
}

impl LanePriority {
    /// The prior two-class policy's flag, for the probes and the reserved-lane rule.
    pub fn is_directed(self) -> bool {
        matches!(self, Self::Directed)
    }
}

/// The process-global admission gate — ONE INSTANCE of [`LaneAdmission`], not a separate
/// implementation of it. Tests build their own with [`LaneAdmission::new`].
static LANES: LaneAdmission = LaneAdmission::new();

/// Physical decode lanes on the shared serving target — the LIVE served count when
/// serving has come up, else the planner's `MAX_LANES` ceiling as the boot-time fallback.
/// Floored at 1. Read live by the saturation gauge; captured at lazy-init by the lane
/// semaphores (serving is always up before the first persona model call, so the init
/// captures the real count, not the ceiling).
/// Publish the running serving target's live lane count (the plan's `--parallel`). The
/// serving daemon calls this on every reconcile. Sizes the admission semaphores exactly:
/// the lazy-init captures it at boot, and a LATER increase (roster grows, more personas
/// demand lanes) grows the live semaphores via `add_permits`. Decreases collect idle
/// permits immediately, as in `PrefillThrottle`; held permits become shrink debt.
/// The serving daemon retries idle debt every tick; acquisition also retires debt
/// before granting a transferred permit, so queued demand cannot defeat shrink.
/// No running permit is revoked or awaited by the publisher.
pub fn set_served_lane_count(lanes: usize) {
    LANES.set_served_lane_count(lanes);
}

/// Reconcile installed permits without waiting for running calls. The difference
/// between installed and a smaller target is debt retried on the next serving tick.
/// Caller holds the owner resize lock, matching `PrefillThrottle`'s accounting.
fn resize_semaphore_to(sem: &tokio::sync::Semaphore, installed: &AtomicUsize, target: usize) {
    let cur = installed.load(Ordering::Acquire);
    if target > cur {
        sem.add_permits(target - cur);
        installed.store(target, Ordering::Release);
    } else if target < cur {
        let forgotten = sem.forget_permits(cur - target);
        installed.store(cur - forgotten, Ordering::Release);
    }
}

/// The lane count a sibling gate should boot with before any plan publishes — the same
/// live-count-else-ceiling read the lane semaphores lazy-init from. Used by the prefill
/// throttle (#56) so both gates start from the ONE number.
pub fn boot_lane_count() -> usize {
    LANES.lane_count()
}

/// Lanes a non-directed (ambient / idle) model call may occupy: all lanes minus one
/// reserved for directed work, floored at 1 so a single-lane machine — where there is
/// nothing to reserve — still lets idle work run rather than starving it forever.
/// RAII permit for one serving-lane model call. Holds a physical-lane permit, the
/// (optional) non-directed sub-cap permit, and the in-flight gauge marker — ALL released
/// on drop. Acquire it around the single generate call so the reservation window matches
/// exactly the lane-consuming window (queue + prefill + decode) and nothing downstream.
#[derive(Debug)]
pub struct ServingLanePermit {
    _lane: tokio::sync::OwnedSemaphorePermit,
    _nondirected: Option<tokio::sync::OwnedSemaphorePermit>,
    _inflight: InflightModelCall,
    /// Ledger tag for grant/release probes (2026-08-29 leak hunt) — permits
    /// vanished with no probe naming the taker, so the permit itself speaks.
    granted_at: std::time::Instant,
    directed: bool,
    priority: LanePriority,
    /// The gate this permit came from — its release wakes parked ambient callers.
    released: &'static tokio::sync::Notify,
}

impl Drop for ServingLanePermit {
    fn drop(&mut self) {
        crate::probe!(
            class = "admission.lane.released",
            directed = self.directed,
            priority = ?self.priority,
            held_ms = self.granted_at.elapsed().as_millis() as u64,
            "serving-lane permit released"
        );
        // Permits drop AFTER this body (field order), so wake on the next poll: the
        // parked caller's try_acquire runs after the semaphore permit has returned.
        self.released.notify_waiters();
    }
}

impl LaneAdmission {
    // A released semaphore permit may transfer directly to a queued waiter,
    // leaving nothing idle for the periodic resize to collect. Retire that debt
    // before granting new work, under the same lock as target publication.
    fn permit_or_retire(
        &self,
        permit: tokio::sync::OwnedSemaphorePermit,
        installed: &AtomicUsize,
        budget: LaneBudget,
    ) -> Option<tokio::sync::OwnedSemaphorePermit> {
        let _guard = self
            .resize_lock
            .lock()
            .expect("lane-resize lock never poisoned");
        let target = match budget {
            LaneBudget::Physical => self.lane_count(),
            LaneBudget::NonDirected => self.nondirected_budget(),
        };
        let count = installed.load(Ordering::Acquire);
        if count > target {
            permit.forget();
            installed.store(count - 1, Ordering::Release);
            drop(_guard);
            crate::probe!(
                class = "admission.lane.shrink_debt",
                pool = ?budget,
                desired = target as u64,
                installed = (count - 1) as u64,
                debt = (count - 1 - target) as u64,
                "retired a transferred permit before admitting queued work"
            );
            None
        } else {
            Some(permit)
        }
    }

    fn try_acquire_resized(
        &self,
        sem: &std::sync::Arc<tokio::sync::Semaphore>,
        installed: &AtomicUsize,
        budget: LaneBudget,
    ) -> Option<tokio::sync::OwnedSemaphorePermit> {
        loop {
            let permit = sem.clone().try_acquire_owned().ok()?;
            if let Some(permit) = self.permit_or_retire(permit, installed, budget) {
                return Some(permit);
            }
        }
    }

    async fn acquire_resized(
        &self,
        sem: &std::sync::Arc<tokio::sync::Semaphore>,
        installed: &AtomicUsize,
        budget: LaneBudget,
    ) -> tokio::sync::OwnedSemaphorePermit {
        loop {
            let permit = sem
                .clone()
                .acquire_owned()
                .await
                .expect("lane semaphore never closed");
            if let Some(permit) = self.permit_or_retire(permit, installed, budget) {
                return permit;
            }
        }
    }
    /// A fresh, INDEPENDENT admission gate. `const` so the process-global can be a `static`;
    /// tests call it to get their own and therefore run order-independently and in parallel.
    pub const fn new() -> Self {
        Self {
            served: AtomicUsize::new(0),
            serving: std::sync::OnceLock::new(),
            nondirected: std::sync::OnceLock::new(),
            serving_installed: AtomicUsize::new(0),
            nondirected_installed: AtomicUsize::new(0),
            resize_lock: Mutex::new(()),
            ambient: std::sync::OnceLock::new(),
            ambient_installed: AtomicUsize::new(0),
            work_waiting: AtomicUsize::new(0),
            released: tokio::sync::Notify::const_new(),
        }
    }

    /// Physical decode lanes: the LIVE served count once serving is up, else the planner's
    /// ceiling as the boot fallback. Floored at 1.
    fn lane_count(&self) -> usize {
        match self.served.load(Ordering::Acquire) {
            0 => (crate::cognition::serving_plan::MAX_LANES as usize).max(1),
            n => n,
        }
    }

    /// Lanes a non-directed (ambient / idle) call may occupy: all but one, floored at 1 so a
    /// single-lane machine still lets idle work run rather than starving it forever.
    fn nondirected_budget(&self) -> usize {
        self.lane_count().saturating_sub(1).max(1)
    }

    /// See [`set_served_lane_count`].
    pub fn set_served_lane_count(&self, lanes: usize) {
        let lanes = lanes.max(1);
        let _guard = self
            .resize_lock
            .lock()
            .expect("lane-resize lock never poisoned");
        let before = (
            self.served.load(Ordering::Acquire),
            self.serving_installed.load(Ordering::Acquire),
            self.nondirected_installed.load(Ordering::Acquire),
            self.ambient_installed.load(Ordering::Acquire),
        );
        self.served.store(lanes, Ordering::Release);
        if let Some(sem) = self.serving.get() {
            resize_semaphore_to(sem, &self.serving_installed, lanes);
        }
        if let Some(sem) = self.nondirected.get() {
            resize_semaphore_to(
                sem,
                &self.nondirected_installed,
                lanes.saturating_sub(1).max(1),
            );
        }
        // The ambient-turn pool rides the SAME budget, so it grows on the same signal.
        // Missing this is how a pool installed at the boot floor would stay there for the
        // process's life while serving grew underneath it.
        if let Some(sem) = self.ambient.get() {
            resize_semaphore_to(sem, &self.ambient_installed, lanes.saturating_sub(1).max(1));
        }
        let after = (
            lanes,
            self.serving_installed.load(Ordering::Acquire),
            self.nondirected_installed.load(Ordering::Acquire),
            self.ambient_installed.load(Ordering::Acquire),
        );
        drop(_guard);
        if after != before {
            crate::probe!(
                class = "admission.lane.resized",
                desired = lanes as u64,
                installed = after.1 as u64,
                debt = after.1.saturating_sub(lanes) as u64,
                nondirected_installed = after.2 as u64,
                ambient_installed = after.3 as u64,
                "lane admission reconciled; installed includes held shrink debt, zero means uninitialized"
            );
        }
    }

    fn serving_lanes(&self) -> &std::sync::Arc<tokio::sync::Semaphore> {
        self.serving.get_or_init(|| {
            let n = self.lane_count();
            self.serving_installed.store(n, Ordering::Release);
            std::sync::Arc::new(tokio::sync::Semaphore::new(n))
        })
    }

    fn nondirected_lanes(&self) -> &std::sync::Arc<tokio::sync::Semaphore> {
        self.nondirected.get_or_init(|| {
            let n = self.nondirected_budget();
            self.nondirected_installed.store(n, Ordering::Release);
            std::sync::Arc::new(tokio::sync::Semaphore::new(n))
        })
    }

    /// See [`try_hold_ambient_turn`]. Lazy like its siblings, and for the same load-bearing
    /// reason: it must capture the budget at FIRST USE, once serving has published a real
    /// lane count — never at construction, when only the boot ceiling is known.
    /// The ambient pool itself, for a bounded wait (see `hold_ambient_turn_for`).
    pub fn ambient_semaphore(&self) -> std::sync::Arc<tokio::sync::Semaphore> {
        self.ambient
            .get_or_init(|| {
                let n = self.nondirected_budget();
                self.ambient_installed.store(n, Ordering::Release);
                std::sync::Arc::new(tokio::sync::Semaphore::new(n))
            })
            .clone()
    }

    pub fn try_hold_ambient_turn(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        self.try_acquire_resized(
            &self.ambient_semaphore(),
            &self.ambient_installed,
            LaneBudget::NonDirected,
        )
        .or_else(|| {
            // Count the miss. The permit is non-blocking, so a None here IS the yield.
            AMBIENT_YIELDS.fetch_add(1, Ordering::Relaxed);
            None
        })
    }

    /// See [`acquire_serving_lane`] — the reservation policy lives HERE so the global and a
    /// test instance can never drift into two different policies.
    pub async fn acquire_serving_lane(&'static self, priority: LanePriority) -> ServingLanePermit {
        let directed = priority.is_directed();
        // Non-directed reserves within the (lanes-1) budget FIRST, so the physical-lane
        // acquire below can never let non-directed work starve a directed caller.
        let nondirected = match priority {
            LanePriority::Directed => None,
            LanePriority::Work => Some(self.acquire_nondirected_as_work().await),
            LanePriority::Ambient => Some(self.acquire_nondirected_as_ambient().await),
        };
        let lane = self
            .acquire_resized(
                self.serving_lanes(),
                &self.serving_installed,
                LaneBudget::Physical,
            )
            .await;
        crate::probe!(
            class = "admission.lane.granted",
            directed,
            priority = ?priority,
            now_available = self.serving_lanes().available_permits() as u64,
            "serving-lane permit granted"
        );
        ServingLanePermit {
            _lane: lane,
            _nondirected: nondirected,
            _inflight: InflightModelCall::enter(),
            granted_at: std::time::Instant::now(),
            directed,
            priority,
            released: &self.released,
        }
    }

    /// A held-work caller queues in the non-directed semaphore in arrival order, and
    /// is COUNTED while it waits so ambient callers stand aside.
    async fn acquire_nondirected_as_work(&self) -> tokio::sync::OwnedSemaphorePermit {
        let sem = self.nondirected_lanes().clone();
        if let Some(p) =
            self.try_acquire_resized(&sem, &self.nondirected_installed, LaneBudget::NonDirected)
        {
            return p;
        }
        tracing::info!(
            probe_class = "serving.lane.nondirected_waiting",
            "held-work model call waiting — a lane is reserved for directed turns (#139)"
        );
        // COUNTED BY A GUARD, NOT BY HAND (2026-09-15, 20:35Z: sixteen minds idle for 50
        // minutes with all seven lanes free). A parked work wait is CANCELLED whenever
        // the turn's future is dropped — `delib.gate.yielded_to_directed` selects away
        // from it, the settle deadline ends the drive, a despawn aborts the loop — and a
        // by-hand `fetch_sub` after the await never ran, so `work_waiting` read 1 for
        // the rest of the process and every ambient caller parked forever on a pool
        // that was empty. Cancellation is a normal exit here; the guard makes it one.
        let _counted = WorkWaiting::enter(&self.work_waiting);
        self.acquire_resized(&sem, &self.nondirected_installed, LaneBudget::NonDirected)
            .await
    }

    /// An ambient caller never enters the semaphore's queue: it takes a permit only when
    /// one is free AND no held-work caller is waiting, otherwise it parks on the release
    /// signal (a 1 s tick guards against a missed wake). Bounded fairness comes from the
    /// work queue draining: every release re-checks, and work waiters are counted down
    /// as they are served.
    async fn acquire_nondirected_as_ambient(&self) -> tokio::sync::OwnedSemaphorePermit {
        let sem = self.nondirected_lanes().clone();
        let mut yielded = false;
        loop {
            if self.work_waiting.load(Ordering::Acquire) == 0 {
                if let Some(p) = self.try_acquire_resized(
                    &sem,
                    &self.nondirected_installed,
                    LaneBudget::NonDirected,
                ) {
                    return p;
                }
            } else if !yielded {
                yielded = true;
                crate::probe!(
                    class = "admission.lane.ambient_yielded_to_work",
                    work_waiting = self.work_waiting.load(Ordering::Acquire) as u64,
                    "an ambient turn stands aside — a held card is waiting for the lane"
                );
            }
            let _ =
                tokio::time::timeout(std::time::Duration::from_secs(1), self.released.notified())
                    .await;
        }
    }

    /// Held-work callers queued right now — a gauge for the gate probes.
    pub fn work_waiting(&self) -> usize {
        self.work_waiting.load(Ordering::Acquire)
    }
}

/// RAII count of a held-work caller waiting for the non-directed budget: incremented
/// on entry, decremented on drop — so a wait that is cancelled mid-await (the only way
/// a parked future ever leaves without a permit) is uncounted the instant it goes.
struct WorkWaiting<'a>(&'a AtomicUsize);

impl<'a> WorkWaiting<'a> {
    fn enter(counter: &'a AtomicUsize) -> Self {
        counter.fetch_add(1, Ordering::AcqRel);
        Self(counter)
    }
}

impl Drop for WorkWaiting<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

impl Default for LaneAdmission {
    fn default() -> Self {
        Self::new()
    }
}

/// Acquire a serving lane for a model call, priced by priority (#139). `directed`
/// callers take from the full lane pool; non-directed callers first claim the
/// (MAX_LANES-1) non-directed budget, guaranteeing a directed caller always finds a free
/// physical lane. Awaits only under genuine contention; the returned permit releases
/// every lane on drop.
/// How many physical serving-lane permits are free RIGHT NOW — a gauge for the
/// gate probes, so a starved waiter names the count instead of implying it.
pub fn serving_lane_permits_available() -> usize {
    LANES.serving_lanes().available_permits()
}

pub async fn acquire_serving_lane(priority: LanePriority) -> ServingLanePermit {
    LANES.acquire_serving_lane(priority).await
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ResourceAdmissionPolicy.ts"
)]
pub struct ResourceAdmissionPolicy {
    pub resource_class: ResourceClass,
    pub target_silicon: TargetSilicon,
    pub max_concurrency: usize,
    pub max_cost_units: u32,
    pub cost_units: u32,
    #[ts(type = "number")]
    pub lease_ttl_ms: u64,
    pub revocation_policy: ThroughputLeaseRevocationPolicy,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResourceAdmissionRequest {
    pub lease_id: String,
    pub artifact_key: String,
    pub holder_id: String,
    pub policy: ResourceAdmissionPolicy,
    pub now_ms: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, thiserror::Error)]
pub enum ResourceAdmissionError {
    #[error("invalid resource admission policy: {reason}")]
    InvalidPolicy { reason: String },
    #[error("resource admission denied: {reason}")]
    Denied { reason: String },
    #[error("resource lease error: {reason}")]
    Lease { reason: String },
}

#[derive(Debug, Default)]
pub struct ResourceAdmissionGate {
    registry: Mutex<ThroughputLeaseRegistry>,
}

impl ResourceAdmissionGate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn acquire(
        &'static self,
        request: ResourceAdmissionRequest,
    ) -> Result<ResourceAdmissionGuard, ResourceAdmissionError> {
        validate_policy(&request.policy)?;

        let lease = ThroughputLease {
            lease_id: request.lease_id.clone(),
            artifact_key: request.artifact_key,
            resource_class: request.policy.resource_class,
            target_silicon: request.policy.target_silicon,
            holder_id: request.holder_id,
            cost_units: request.policy.cost_units,
            acquired_at_ms: request.now_ms,
            expires_at_ms: request.now_ms.saturating_add(request.policy.lease_ttl_ms),
            revocation_policy: request.policy.revocation_policy,
        };

        let mut registry = self.lock_registry();
        registry.expire(request.now_ms);
        let snapshot = registry.snapshot(request.now_ms);
        let active_count = snapshot
            .active
            .iter()
            .filter(|lease| lease.target_silicon == request.policy.target_silicon)
            .count();
        let active_cost = snapshot
            .cost_by_target_silicon
            .get(&request.policy.target_silicon)
            .copied()
            .unwrap_or(0);

        if active_count >= request.policy.max_concurrency {
            return Err(ResourceAdmissionError::Denied {
                reason: format!(
                    "resource_class={:?} target_silicon={:?} active_count={} max_concurrency={}",
                    request.policy.resource_class,
                    request.policy.target_silicon,
                    active_count,
                    request.policy.max_concurrency
                ),
            });
        }
        if active_cost.saturating_add(request.policy.cost_units) > request.policy.max_cost_units {
            return Err(ResourceAdmissionError::Denied {
                reason: format!(
                    "resource_class={:?} target_silicon={:?} active_cost={} requested_cost={} max_cost_units={}",
                    request.policy.resource_class,
                    request.policy.target_silicon,
                    active_cost,
                    request.policy.cost_units,
                    request.policy.max_cost_units
                ),
            });
        }

        registry
            .acquire(lease, request.now_ms)
            .map_err(|err| ResourceAdmissionError::Lease {
                reason: format_lease_error(err),
            })?;

        Ok(ResourceAdmissionGuard {
            gate: self,
            lease_id: Some(request.lease_id),
        })
    }

    fn release(&self, lease_id: &str) -> Result<ThroughputLease, ThroughputLeaseError> {
        self.lock_registry().release(lease_id)
    }

    fn lock_registry(&self) -> MutexGuard<'_, ThroughputLeaseRegistry> {
        self.registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[cfg(test)]
    pub fn reset_for_test(&self) {
        *self.lock_registry() = ThroughputLeaseRegistry::new();
    }

    #[cfg(test)]
    pub fn active_count_for_test(&self, now_ms: u64) -> usize {
        self.lock_registry().snapshot(now_ms).active.len()
    }
}

#[derive(Debug)]
pub struct ResourceAdmissionGuard {
    gate: &'static ResourceAdmissionGate,
    lease_id: Option<String>,
}

impl ResourceAdmissionGuard {
    #[cfg(test)]
    pub fn release(mut self) -> Result<ThroughputLease, ThroughputLeaseError> {
        let lease_id = self
            .lease_id
            .take()
            .expect("resource admission guard must contain a lease id before release");
        self.gate.release(&lease_id)
    }
}

impl Drop for ResourceAdmissionGuard {
    fn drop(&mut self) {
        let Some(lease_id) = self.lease_id.take() else {
            return;
        };
        let _ = self.gate.release(&lease_id);
    }
}

fn validate_policy(policy: &ResourceAdmissionPolicy) -> Result<(), ResourceAdmissionError> {
    if policy.max_concurrency == 0 {
        return Err(invalid_policy("max_concurrency must be greater than zero"));
    }
    if policy.cost_units == 0 {
        return Err(invalid_policy("cost_units must be greater than zero"));
    }
    if policy.max_cost_units == 0 {
        return Err(invalid_policy("max_cost_units must be greater than zero"));
    }
    if policy.cost_units > policy.max_cost_units {
        return Err(invalid_policy(format!(
            "cost_units={} exceeds max_cost_units={}",
            policy.cost_units, policy.max_cost_units
        )));
    }
    if policy.lease_ttl_ms == 0 {
        return Err(invalid_policy("lease_ttl_ms must be greater than zero"));
    }
    Ok(())
}

fn invalid_policy(reason: impl Into<String>) -> ResourceAdmissionError {
    ResourceAdmissionError::InvalidPolicy {
        reason: reason.into(),
    }
}

fn format_lease_error(err: ThroughputLeaseError) -> String {
    match err {
        ThroughputLeaseError::DuplicateLease { lease_id } => {
            format!("duplicate lease_id={lease_id}")
        }
        ThroughputLeaseError::MissingLease { lease_id } => {
            format!("missing lease_id={lease_id}")
        }
        ThroughputLeaseError::ExpiredLease { lease_id } => {
            format!("expired lease_id={lease_id}")
        }
    }
}

#[cfg(test)]
mod tests {

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
        for i in 1..=(LEASED_IN_SENT_SAMPLES as u32 + 8) {
            note_leased_in_sent(30_000 + i);
        }
        let s = leased_in_sent_samples();
        assert_eq!(s.len(), LEASED_IN_SENT_SAMPLES, "bounded");
        assert_eq!(*s.last().expect("nonempty"), 30_000 + LEASED_IN_SENT_SAMPLES as u32 + 8, "newest kept");
        assert!(s.first().copied().expect("nonempty") > 30_000 + 8, "oldest aged out");
    }


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

    use super::*;

    // No serialization lock lives here any more, and that is the point. It used to exist
    // because the ambient-permit and serving-lane tests mutated PROCESS-GLOBAL admission
    // statics — and its own comment admitted the lock was not enough: "TEST_SERIAL only
    // serializes THIS module, but a cross-module guard could bump it mid-loop" (#1960/#191).
    // The in-flight Gauge test was rescued first by driving a LOCAL instance; these two now
    // do the same with `LaneAdmission::new()`. The race is gone at the SOURCE, not merely
    // serialized, and the tests run in parallel.

    // what this catches (#139 idle admission): the in-flight gauge counts each
    // model-call entry, releases on drop, and reads SATURATED exactly at the serving
    // concurrency (serving_plan::MAX_LANES) — the signal a self-tick yields on so an
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
        let max = crate::cognition::serving_plan::MAX_LANES as usize;
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

    // what this catches (#171 fan-out): the ambient-turn permit bounds how many
    // NON-directed turns deliberate at once, no matter how the room burst wakes them.
    // The first cut gated on the in-flight gauge and did NOT fire live: a simultaneous
    // burst wakes every peer, they all read inflight=0 (none had generated yet) and
    // stampede past the check together. A permit has no such window — the slot is
    // claimed at the decision point and held for the turn, so the (N-1) peers that woke
    // in the same instant find it taken and yield. This is the only test that touches
    // the process-global ambient semaphore, so it starts with all slots free.
    #[test]
    fn ambient_permit_bounds_concurrency_and_releases_on_drop() {
        // OUR OWN gate — no process-global, so no lock and no order dependence.
        let gate = LaneAdmission::new();
        // PREMISE CHANGE 2026-08-17: the pool used to be a bare `AMBIENT_TURN_CONCURRENCY
        // = 1`; it is now the live `nondirected_budget()` (lanes − 1, floored at 1), the
        // same budget the per-call lane reservation uses. Pin a real multi-lane machine so
        // the burst has something to bound — at the old hardcoded 1 this test could not
        // distinguish "correctly bounded" from "throttled below the hardware", which is
        // exactly how the starvation ceiling stayed invisible.
        gate.set_served_lane_count(4);
        let budget = gate.nondirected_budget();
        assert_eq!(
            budget, 3,
            "4 served lanes → 3 non-directed, 1 reserved directed"
        );
        // A simultaneous-wake burst: several ambient turns try to claim a slot at once.
        // Exactly `budget` win; the rest get None and must yield.
        let mut held: Vec<tokio::sync::OwnedSemaphorePermit> = Vec::new();
        for _ in 0..budget {
            held.push(
                gate.try_hold_ambient_turn()
                    .expect("a free slot is grantable"),
            );
        }
        // The next simultaneous ambient waker finds every slot taken → yields.
        assert!(
            gate.try_hold_ambient_turn().is_none(),
            "over-capacity ambient turn must yield (the stampede the gauge let through)"
        );

        // The addressed persona never calls this — directed work is unthrottled. Model
        // that by simply NOT touching the permit here; the held slots stay full and the
        // assertion above already proved a concurrent ambient turn can't sneak a slot.

        // A held ambient turn finishes → its permit drops → capacity frees for the next
        // beat, so a yielded room re-perceives and contributes when there's headroom.
        held.pop();
        let reclaimed = gate
            .try_hold_ambient_turn()
            .expect("dropping a finished turn frees its slot for the next");
        drop(reclaimed);
        drop(held); // release the rest (nothing else can observe this gate anyway)
    }

    // what this catches: a starvation report that is either a firehose or silent. At 24
    // citizens on a 15s beat, per-yield rows would be ~92/min and would drown the stream the
    // way serving.plan did at 2.6 rows/s (#399) — but NO row made a starved roster
    // indistinguishable from a healthy one, which cost a manual /slots curl to resolve on
    // 2026-08-17. Both failure modes are pinned here.
    #[test]
    fn the_starvation_report_is_rate_limited_but_the_first_one_is_never_silent() {
        // Never reported before (last_report_ms == 0) → fires immediately, however early.
        assert_eq!(
            ambient_yield_report_due(23, 0, 5, 0),
            Some(23),
            "the FIRST starvation must report even at t=5ms — it is the one that matters"
        );
        // Inside the gap → silent, no matter how many yields piled up.
        assert_eq!(
            ambient_yield_report_due(1_000, 23, 10_000, 9_000),
            None,
            "a second report inside the gap would be the firehose"
        );
        // Gap elapsed → reports only the DELTA, not the cumulative total.
        assert_eq!(
            ambient_yield_report_due(1_000, 23, 70_000, 9_000),
            Some(977),
            "reports yields in THIS window; a cumulative count cannot show a rate"
        );
        // No new yields → nothing to say, even after the gap. Contention ended.
        assert_eq!(
            ambient_yield_report_due(23, 23, 999_000, 9_000),
            None,
            "a quiet roster must not emit a row saying nothing happened"
        );
    }

    // what this catches: THE BUG I SHIPPED, in the impure wrapper the table test could not
    // reach. `take_ambient_yield_report` passed a literal 0 for `yields_at_last_report`, so
    // every row reported the CUMULATIVE total under a field named `yields_since_last_report`.
    // Live rows on 2026-08-17 read 457==457, 392==392 while consecutive rows differed by ~65.
    // A pure rule with a correct test is worth nothing if its caller feeds it a wrong
    // argument — this asserts the SECOND window measures from the first, not from zero.
    #[test]
    fn consecutive_reports_measure_windows_not_running_totals() {
        // Simulate two windows over the shared statics via the pure rule, which is what the
        // wrapper now actually does (the wrapper itself touches process globals, so the
        // contract that broke is pinned here at the argument boundary).
        let first = ambient_yield_report_due(135, 0, 60_000, 0).expect("first window reports");
        assert_eq!(
            first, 135,
            "first window is total-so-far — nothing preceded it"
        );

        // Second window: watermark advanced to 135, total climbed to 197.
        let second =
            ambient_yield_report_due(197, 135, 120_000, 60_000).expect("second window reports");
        assert_eq!(
            second, 62,
            "the SECOND window must be the delta (197-135), not the cumulative 197 — \
             passing 0 here is the bug that shipped"
        );
        assert_ne!(
            second, 197,
            "reporting cumulative under a per-window name is the lie"
        );
    }

    // what this catches: the WEAK-BOX floor of the same change. Deriving the ambient pool
    // from lanes−1 must not regress a 1- or 2-lane machine below the behaviour it had
    // under the old hardcoded 1 — a single-lane host has nothing to reserve, so its
    // budget floors at 1 and it stays byte-identical. This is the half of the change that
    // could silently hurt the smallest supported hardware, so it gets its own row.
    #[test]
    fn a_one_lane_box_keeps_exactly_one_ambient_slot() {
        for lanes in [1usize, 2] {
            let gate = LaneAdmission::new();
            gate.set_served_lane_count(lanes);
            assert_eq!(
                gate.nondirected_budget(),
                1,
                "{lanes}-lane box floors the non-directed budget at 1"
            );
            let held = gate
                .try_hold_ambient_turn()
                .expect("the one slot is grantable");
            assert!(
                gate.try_hold_ambient_turn().is_none(),
                "a {lanes}-lane box admits exactly ONE ambient turn — unchanged from the \
                 pre-2026-08-17 hardcoded bound"
            );
            drop(held);
        }
    }

    // what this catches: an ambient pool installed at the BOOT floor and then never grown.
    // The pool is lazy on purpose (capture the real lane count at first use), but that
    // makes "first use happened before serving published its count" a live possibility —
    // and without the grow-on-resize wiring the whole roster would stay pinned at the boot
    // budget for the life of the process while 3 lanes sat idle. Regression for the
    // starvation ceiling this change removes.
    #[test]
    fn the_ambient_pool_grows_when_serving_publishes_more_lanes() {
        let gate = LaneAdmission::new();
        gate.set_served_lane_count(1); // cold boot: one lane
        let first = gate.try_hold_ambient_turn().expect("the one slot");
        assert!(
            gate.try_hold_ambient_turn().is_none(),
            "1 lane → 1 ambient slot"
        );

        gate.set_served_lane_count(4); // serving warms up and reports its real width
        let second = gate
            .try_hold_ambient_turn()
            .expect("growing to 4 lanes must open a second ambient slot");
        let third = gate
            .try_hold_ambient_turn()
            .expect("…and a third (budget = lanes - 1)");
        assert!(
            gate.try_hold_ambient_turn().is_none(),
            "still bounded at lanes-1 — growth must not remove the directed reservation"
        );
        drop((first, second, third));
    }

    // what this catches (#139 lane starvation): a directed (addressed) turn must never
    // queue behind non-directed model calls. Non-directed callers are capped at
    // (MAX_LANES-1) lanes, so a directed caller always finds a reserved lane — this is
    // the fix for the glass-boxed 8-minute directed-turn wait behind an idle self-tick
    // + one long ambient turn on the two decode lanes. This is the only test that
    // touches the process-global serving-lane semaphores, so it starts all-free.
    #[tokio::test]
    async fn directed_turn_always_finds_a_reserved_lane() {
        let gate: &'static LaneAdmission = Box::leak(Box::new(LaneAdmission::new()));
        use std::time::Duration;
        let budget = gate.nondirected_budget();

        // Fill the ENTIRE non-directed budget (all lanes idle/ambient work may hold).
        let mut nondirected = Vec::new();
        for _ in 0..budget {
            nondirected.push(gate.acquire_serving_lane(LanePriority::Ambient).await);
        }

        // On a machine with a lane to reserve (MAX_LANES >= 2), a directed call still
        // acquires immediately — it is not blocked by the saturated non-directed budget.
        if gate.lane_count() > 1 {
            let directed = tokio::time::timeout(
                Duration::from_millis(250),
                gate.acquire_serving_lane(LanePriority::Directed),
            )
            .await;
            assert!(
                directed.is_ok(),
                "a directed turn must get a reserved lane, never queue behind non-directed work"
            );

            // And a FURTHER non-directed call must now WAIT (its budget is full) — it
            // times out rather than stealing the lane the directed turn is using.
            let extra_nondirected = tokio::time::timeout(
                Duration::from_millis(150),
                gate.acquire_serving_lane(LanePriority::Ambient),
            )
            .await;
            assert!(
                extra_nondirected.is_err(),
                "non-directed work over its (MAX_LANES-1) budget must wait, not preempt"
            );
            drop(directed);
        }

        drop(nondirected); // release (nothing else can observe this gate anyway)
    }

    // what this catches: boot-created pools must shrink when the first real plan
    // serves one lane, including the ambient and non-directed reservations.
    #[test]
    fn boot_ceiling_pools_shrink_to_the_first_served_count() {
        let gate = LaneAdmission::new();
        let serving = gate.serving_lanes();
        let nondirected = gate.nondirected_lanes();
        drop(gate.try_hold_ambient_turn().expect("boot ambient permit"));
        gate.set_served_lane_count(1);
        assert_eq!(serving.available_permits(), 1);
        assert_eq!(nondirected.available_permits(), 1);
        let ambient = gate.try_hold_ambient_turn().expect("one ambient permit");
        assert!(gate.try_hold_ambient_turn().is_none());
        drop(ambient);
    }

    // what this catches: shrink never revokes held permits; the unchanged-plan
    // publication on the existing serving tick drains debt, and a newer target
    // replaces the old debt without double-adding or losing capacity.
    #[test]
    fn held_shrink_debt_reconciles_to_the_latest_plan() {
        let gate = LaneAdmission::new();
        gate.set_served_lane_count(4);
        let serving = gate.serving_lanes();
        let nondirected = gate.nondirected_lanes();
        let held = serving.try_acquire_many(4).expect("four serving lanes");
        let nonheld = nondirected
            .try_acquire_many(3)
            .expect("three non-directed lanes");
        let ambient: Vec<_> = (0..3)
            .map(|_| gate.try_hold_ambient_turn().expect("ambient lane"))
            .collect();
        gate.set_served_lane_count(1);
        assert_eq!(gate.serving_installed.load(Ordering::Acquire), 4);
        assert_eq!(serving.available_permits(), 0);
        drop(held);
        drop(nonheld);
        drop(ambient);
        // This call is unconditional on every existing serving-daemon tick,
        // outside the plan-fingerprint/probe-change guard.
        gate.set_served_lane_count(1);
        assert_eq!(serving.available_permits(), 1);
        assert_eq!(nondirected.available_permits(), 1);
        gate.set_served_lane_count(4);
        let held = serving.try_acquire_many(4).expect("regrown lanes");
        gate.set_served_lane_count(1);
        gate.set_served_lane_count(3); // supersedes the older shrink while held
        drop(held);
        gate.set_served_lane_count(3);
        gate.set_served_lane_count(3); // repeated stable plan is idempotent
        assert_eq!(serving.available_permits(), 3);
        assert_eq!(gate.serving_installed.load(Ordering::Acquire), 3);
        assert_eq!(nondirected.available_permits(), 2);
        let ambient: Vec<_> = (0..2)
            .map(|_| gate.try_hold_ambient_turn().expect("latest ambient budget"))
            .collect();
        assert!(gate.try_hold_ambient_turn().is_none());
        drop(ambient);
    }

    // what this catches: queued semaphore waiters receive released permits
    // directly, so periodic forget_permits alone can never see idle capacity.
    // Drive the production acquisition seam without sleeps or another tick.
    #[tokio::test]
    async fn queued_acquirers_retire_shrink_debt_before_granting() {
        for pool in 0..3 {
            let gate = LaneAdmission::new();
            gate.set_served_lane_count(4);
            let (sem, installed, budget) = match pool {
                0 => (
                    gate.serving_lanes().clone(),
                    &gate.serving_installed,
                    LaneBudget::Physical,
                ),
                1 => (
                    gate.nondirected_lanes().clone(),
                    &gate.nondirected_installed,
                    LaneBudget::NonDirected,
                ),
                _ => (
                    gate.ambient_semaphore(),
                    &gate.ambient_installed,
                    LaneBudget::NonDirected,
                ),
            };
            let count = sem.available_permits();
            let mut holders: Vec<_> = (0..count)
                .map(|_| sem.clone().try_acquire_owned().unwrap())
                .collect();
            let mut waiters: Vec<_> = (0..count)
                .map(|_| Box::pin(gate.acquire_resized(&sem, installed, budget)))
                .collect();
            for waiter in &mut waiters {
                assert!(futures::poll!(waiter.as_mut()).is_pending());
            }
            gate.set_served_lane_count(1);
            for _ in 1..count {
                drop(holders.pop());
                for waiter in &mut waiters {
                    assert!(
                        futures::poll!(waiter.as_mut()).is_pending(),
                        "surplus must retire, not admit"
                    );
                }
            }
            assert_eq!(installed.load(Ordering::Acquire), 1);
            drop(holders.pop());
            let mut granted = Vec::new();
            for waiter in &mut waiters {
                if let std::task::Poll::Ready(permit) = futures::poll!(waiter.as_mut()) {
                    granted.push(permit);
                }
            }
            assert_eq!(granted.len(), 1, "only the final target capacity may run");
            drop(waiters); // cancellation must release queued reservations
            drop(granted);
            assert_eq!(sem.available_permits(), 1);
        }
    }

    // what this catches: canceling a work waiter while shrink retires permits
    // must not leave a phantom work count that permanently excludes ambient work.
    #[tokio::test]
    async fn cancelled_work_waiter_clears_its_priority_reservation() {
        let gate = LaneAdmission::new();
        gate.set_served_lane_count(2);
        let held = gate
            .nondirected_lanes()
            .clone()
            .try_acquire_owned()
            .unwrap();
        let mut waiting = Box::pin(gate.acquire_nondirected_as_work());
        assert!(futures::poll!(waiting.as_mut()).is_pending());
        assert_eq!(gate.work_waiting(), 1);
        gate.set_served_lane_count(1);
        drop(waiting);
        assert_eq!(gate.work_waiting(), 0);
        drop(held);
        let mut ambient = Box::pin(gate.acquire_nondirected_as_ambient());
        assert!(futures::poll!(ambient.as_mut()).is_ready());
    }
    // what this catches: serving_lane_count reads the LIVE published count when serving is
    // up, and only falls back to the MAX_LANES ceiling when nothing has published yet —
    // the exact fix so the admission reservation sizes by what's SERVED (e.g. 4), not the
    // ceiling (6).
    //
    // Drives its OWN gate. The prior version swapped the process-global atomic and restored
    // it at the end — brittle in two ways at once: a sibling test reading the count in the
    // window between swap and restore sees a value this test invented, and a panic anywhere
    // in between leaves the global corrupted for every test that follows. Owning the state
    // removes both, with neither a lock nor a restore.
    #[test]
    fn serving_lane_count_prefers_the_live_published_count_over_the_ceiling() {
        let gate = LaneAdmission::new();
        assert_eq!(
            gate.lane_count(),
            (crate::cognition::serving_plan::MAX_LANES as usize).max(1),
            "unset → MAX_LANES ceiling fallback"
        );
        gate.set_served_lane_count(4);
        assert_eq!(
            gate.lane_count(),
            4,
            "published live count wins over the ceiling"
        );
    }
    // what this catches (2026-09-14): an idle mind starving forever under holders-first —
    // starvation is a function of her last turn, counted from boot when she never had one.
    #[test]
    fn a_mind_with_no_turn_for_fifteen_minutes_is_starving_and_a_recent_one_is_not() {
        let boot = 1_000_000;
        assert!(
            !starving_since(Some(boot + 60_000), boot + 600_000, boot),
            "ten minutes idle: not yet"
        );
        assert!(
            starving_since(
                Some(boot + 60_000),
                boot + 60_000 + IDLE_STARVATION_MS,
                boot
            ),
            "fifteen minutes: starving"
        );
        assert!(
            starving_since(None, boot + IDLE_STARVATION_MS, boot),
            "never had a turn: counted from boot"
        );
        assert!(
            !starving_since(None, boot + 1_000, boot),
            "just booted: not starving"
        );
    }

    // what this catches (2026-09-15 20:35Z, the IDLE hour): a parked WORK wait that is
    // CANCELLED (the turn's future dropped — a yield to a directed line, the settle
    // deadline, a despawn) must leave `work_waiting` at zero, or every ambient caller
    // parks forever on a pool with free lanes. Sixteen minds sat idle for fifty
    // minutes with all seven lanes free on exactly this.
    #[tokio::test]
    async fn a_cancelled_work_wait_is_uncounted_and_ambient_turns_flow_again() {
        use std::time::Duration;
        let gate: &'static LaneAdmission = Box::leak(Box::new(LaneAdmission::new()));
        gate.set_served_lane_count(2); // non-directed budget = 1
        let held = gate.acquire_serving_lane(LanePriority::Ambient).await;
        let work = tokio::spawn(async move { gate.acquire_serving_lane(LanePriority::Work).await });
        // Deterministic (Cormac's note on #4083): wait until the task is COUNTED, under a
        // bound, instead of a fixed sleep a loaded runner can outlast.
        let counted = tokio::time::timeout(Duration::from_secs(5), async {
            while gate.work_waiting() != 1 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        assert!(counted.is_ok(), "the work caller is counted while it waits");
        work.abort(); // the cancellation every real exit takes
        let _ = work.await;
        assert_eq!(
            gate.work_waiting(),
            0,
            "a cancelled wait is uncounted by its guard"
        );
        drop(held);
        let ambient = tokio::time::timeout(
            Duration::from_millis(1500),
            gate.acquire_serving_lane(LanePriority::Ambient),
        )
        .await;
        assert!(
            ambient.is_ok(),
            "with no work waiting, the ambient turn takes the free lane"
        );
    }

    // what this catches (2026-09-15, the READING hour): a mind holding a card mid-edit
    // queued for the non-directed budget behind a mind musing with no card. With one
    // non-directed lane taken, a WORK caller and an AMBIENT caller both arrive; when the
    // lane frees, the work caller gets it and the ambient caller is still parked. And
    // once no work waits, the ambient caller takes the next free lane — parked, not
    // starved.
    #[tokio::test]
    async fn a_held_card_takes_the_lane_before_a_musing_turn() {
        use std::time::Duration;
        let gate: &'static LaneAdmission = Box::leak(Box::new(LaneAdmission::new()));
        gate.set_served_lane_count(2); // non-directed budget = 1
        let held = gate.acquire_serving_lane(LanePriority::Ambient).await;

        let work = tokio::spawn(async move { gate.acquire_serving_lane(LanePriority::Work).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            gate.work_waiting(),
            1,
            "the work caller is counted while it waits"
        );
        let ambient =
            tokio::spawn(async move { gate.acquire_serving_lane(LanePriority::Ambient).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!ambient.is_finished(), "ambient parks while work waits");

        drop(held);
        let work_permit = tokio::time::timeout(Duration::from_millis(500), work)
            .await
            .expect("work gets the freed lane")
            .expect("join");
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            !ambient.is_finished(),
            "ambient is still parked: the work caller holds the only non-directed lane"
        );
        assert_eq!(gate.work_waiting(), 0);

        drop(work_permit);
        let ambient_permit = tokio::time::timeout(Duration::from_millis(1500), ambient)
            .await
            .expect("ambient takes the lane once no work waits")
            .expect("join");
        drop(ambient_permit);
    }
}
