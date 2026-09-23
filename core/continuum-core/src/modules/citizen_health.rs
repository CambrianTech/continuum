//! Citizen health is a SUBSTRATE RECEIPT, not a hand read.
//!
//! Joel, 2026-09-14: "this is up to you their health." That afternoon the health of
//! sixteen minds (3 lanes at 120k, 4 writes an hour, a fifth of every receipt "let me
//! take stock") was found by an operator querying probes by hand; every number was
//! already in the process and nobody owned the drift. This module owns it: an
//! in-process ledger fed at the three seams that already probe (an act observed, a
//! lane granted, a card settled), snapshotted once an hour beside residency and the
//! served lanes, judged by one pure rule, probed, and said in the org room in one line.
//!
//! The rule names the shape, not a score: STARVED (more minds than lanes can turn),
//! READING (acts without writes), IDLE (residents, no acts), or HEALTHY. The line is
//! what a human or a peer acts on; the probe is what a test or a dashboard reads.
//!
//! THE RECEIPT HAS AN ACTOR (card c84d885a, S3 — Joel 2026-09-18: "govern for
//! reliability"). "AT THE KNEE … owed: fewer seats" was said in the org room for hours
//! on 2026-09-18 with nobody to pay it; an operator paid it by hand (a hold + a reboot).
//! Now the tick pays it: when the roster is starved or at the knee AND the hour's card
//! pulls were mostly lane-deferrals (`bench.round.pull_deferred_wip` — the minds were
//! queued on the lanes, not idle), the least-served seats REST down to the healthy edge
//! (`lanes × MINDS_PER_LANE_STARVED_ABOVE`), the same resting-seat page-out the mindless
//! rule uses, with WHY on the record. An hour of `pull_none` pays nothing here — that is
//! a round-supply defect, not a seat one, and paging a roster for it would delete the
//! roster and report success.
//! Cadence follows the RTOS shape: the module's own `tick_interval`, no task of its own
//! ([[docs/architecture/CONCURRENCY-STYLE-GUIDE.md]]).

use crate::runtime::{CommandResult, CommandSchema, ModuleConfig, ModulePriority, ServiceModule};
use std::any::Any;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// One hour: long enough for a fair sample of a roster that gets a lane every few
/// minutes, short enough that a starved evening is caught before it is a night.
pub const HEALTH_WINDOW: Duration = Duration::from_secs(3600);

#[derive(Default)]
struct Ledger {
    acts: AtomicU64,
    writes: AtomicU64,
    lanes_granted: AtomicU64,
    settles: AtomicU64,
    credits_staged: AtomicU64,
    credits_settled: AtomicU64,
    /// Card pulls the roster attempted (the three receipted outcomes of
    /// `work_pull::try_pull_next_card`: pulled, deferred on the lanes, nothing to pull).
    pulls: AtomicU64,
    /// Of those, the pulls deferred because the roster already held a card per lane.
    pulls_deferred: AtomicU64,
    /// Turns that ended INSIDE the reasoning channel — no answer, no act (the
    /// `persona.act.think_only` seam). Two in an hour on the M5 (2026-09-20) were the
    /// whole story of #4194's allowance floor; on the hour line a regression is one number.
    think_only: AtomicU64,
    /// Turn iterations that NEVER REACHED THE MODEL — she waited at the serving gate and
    /// produced nothing (the `persona.act.lane_starved` seam, card cff534ba). The sibling
    /// of `think_only` and NOT the same failure: a think-only turn GOT a lane and ended
    /// inside the reasoning channel; a lane-starved turn never got one. On the M5
    /// 2026-09-20 ~21:55Z two of the hour's turns were `model_ms=0` waits of 6 and 25
    /// minutes at `lanes_available=0`, and the line called them acts.
    lane_starved: AtomicU64,
    /// THE HOUR'S LANES, not the tick's (Cormac's condition on #4244): the most lanes the
    /// node served at any fold point this window. The lane-bound rest is destructive and
    /// now rests the whole overage in one tick, so it must not stand on a point sample —
    /// a launch between lanes at the tick reads 1 and would rest fourteen of sixteen. Max
    /// is the right fold for a destructive act: a lane truly lost rests one hour later; a
    /// lane momentarily absent rests nobody. Folded at the geometry settle and at every
    /// lane grant, read and reset by the tick.
    lanes_max: AtomicU64,
    /// THE HOUR'S PREFIX REUSE (card c119ace7): prompt tokens the lanes served from the
    /// KV cache vs. prompt tokens they had to prefill, summed over every generation
    /// that reported timings. The ratio is the fraction of every prompt the engine did
    /// NOT re-read — on the M5 at ~90 tok/s prefill, a 30k prompt at 0% reuse is six
    /// minutes of silence per act, at 75% it is ninety seconds. Separate totals, ratio
    /// derived on read (averaging rates lies — `TurnMetrics::accumulate`'s rule).
    prompt_cached: AtomicU64,
    prompt_prefilled: AtomicU64,
    /// GENERATIONS THIS HOUR AND THE ONES THROWN AWAY (card ebce2ba0). A generation is
    /// counted once it has a terminal answer — a response OR a typed adapter refusal,
    /// both of which reached the model — and `generations_dropped` counts the ones the
    /// awaiting side abandoned WHILE THE MODEL WAS STILL PRODUCING (the
    /// `persona.generation.dropped` seam). NOT the same failure as `lane_starved`: a
    /// lane-starved turn never reached the model, a dropped one did and its output was
    /// discarded. On the M5 2026-09-20 five of an evening's generations died this way at
    /// 1,207,290–1,492,456 ms — the act deadline reaping work that was still inside its
    /// own stated bound, and the hour line called it READING (lazy personas).
    generations: AtomicU64,
    generations_dropped: AtomicU64,
    /// Placement moves this hour, by cause (card 10bba591): ON OPPORTUNITY — the grid
    /// allocation seated her on a strictly better seat and the switch took it between
    /// turns; ON FAILURE — a seat went dark, cold, queued or too narrow and she fell
    /// home, returned, or spilled off a starved node.
    moves_opportunity: AtomicU64,
    moves_failure: AtomicU64,
    /// THE HOUR'S LANE DISTRIBUTION, bucketed by lane count (card `6d444769`). Sampled at
    /// the PULL DECISION — the moment the capacity is actually spent — so the shape is the
    /// one the queueing minds met, not the one the node touched once. Read by
    /// [`lanes_sustained_of`]; `lanes_max` is unchanged and still the peak.
    lane_samples: [AtomicU64; LANE_HIST_LEN],
    /// WHEN the first and last of those samples were taken. A COUNT is not COVERAGE
    /// (Astra's review of #4356): sixteen residents pulling through one brief relaunch
    /// produce two dozen samples inside half a minute, all reading the launch's one lane.
    /// The span is what separates that burst from an hour.
    lane_first_sample_ms: AtomicU64,
    lane_last_sample_ms: AtomicU64,
}

static LEDGER: Ledger = Ledger {
    acts: AtomicU64::new(0),
    writes: AtomicU64::new(0),
    lanes_granted: AtomicU64::new(0),
    settles: AtomicU64::new(0),
    credits_staged: AtomicU64::new(0),
    credits_settled: AtomicU64::new(0),
    pulls: AtomicU64::new(0),
    pulls_deferred: AtomicU64::new(0),
    think_only: AtomicU64::new(0),
    lane_starved: AtomicU64::new(0),
    generations: AtomicU64::new(0),
    generations_dropped: AtomicU64::new(0),
    lanes_max: AtomicU64::new(0),
    prompt_cached: AtomicU64::new(0),
    prompt_prefilled: AtomicU64::new(0),
    moves_opportunity: AtomicU64::new(0),
    moves_failure: AtomicU64::new(0),
    lane_samples: [const { AtomicU64::new(0) }; LANE_HIST_LEN],
    lane_first_sample_ms: AtomicU64::new(0),
    lane_last_sample_ms: AtomicU64::new(0),
};
/// A turn ended inside the reasoning channel with no answer and no act (the
/// `persona.act.think_only` seam) — the allowance did not hold her think.
pub fn note_think_only() {
    LEDGER.think_only.fetch_add(1, Ordering::Relaxed);
}
/// A placement move landed (the `placement.move.opportunity` seam and the switch's
/// fall-home / return / spill seams). `opportunity` = a better seat, not a failed one.
pub(crate) fn note_move(opportunity: bool) {
    if opportunity {
        LEDGER.moves_opportunity.fetch_add(1, Ordering::Relaxed);
    } else {
        LEDGER.moves_failure.fetch_add(1, Ordering::Relaxed);
    }
}
fn snapshot_moves_and_reset() -> (u64, u64) {
    (LEDGER.moves_opportunity.swap(0, Ordering::Relaxed), LEDGER.moves_failure.swap(0, Ordering::Relaxed))
}
/// What the grid allocator last published about THIS hour's placement: the oldest
/// dormant mind's turn age. `None` = nothing published yet or nobody dormant.
fn oldest_dormant_turn_age_ms() -> Option<u64> {
    crate::modules::grid_allocator::current().and_then(|p| p.oldest_dormant_turn_age_ms())
}
/// The node served `lanes` lanes just now — fold into the hour's maximum (see
/// `Ledger::lanes_max`). Called at the serving daemon's geometry settle and at every
/// lane grant, so the hour's lanes are the lanes the hour actually served.
pub fn note_lanes(lanes: u64) {
    LEDGER.lanes_max.fetch_max(lanes, Ordering::Relaxed);
}

/// Lane buckets, 0..=32 lanes with everything above folded into the last. Small, fixed,
/// and lock-free — the node's lane count is a single-digit number and always has been.
const LANE_HIST_LEN: usize = 33;

/// Below this many samples the window has no SHAPE, and the peak stands. This is the
/// guard that keeps Cormac's condition on #4244 intact: a handful of pulls taken while
/// the engine relaunches is a launch window, not an hour, and must not rest the roster.
/// The hours that motivated this carried 232, 477 and 708 pulls.
pub const LANE_SUSTAINED_MIN_SAMPLES: u64 = 24;

/// AND the samples must SPAN at least this much of the window before they can actuate a
/// rest or a wake. A count floor alone does not survive Astra's review of #4356: pulls are
/// not spread through the hour, they arrive when the roster is polling, so sixteen
/// residents queueing through one 30-second relaunch clear a 24-sample floor without the
/// hour having a shape at all — and the median of that burst is the launch's single lane.
/// Ten minutes is far longer than any relaunch this substrate performs and still a sixth
/// of the window, so a genuine sustained shortage trips well inside the hour it happens.
pub const LANE_SUSTAINED_MIN_SPAN_MS: u64 = 10 * 60 * 1_000;

/// The node had `lanes` lanes at a moment work was actually asked of them — fold into the
/// hour's distribution. Called from [`note_pull`], never on its own: the whole point is
/// that the sample is taken where the capacity is spent.
fn note_lane_sample(lanes: u64) {
    let idx = (lanes as usize).min(LANE_HIST_LEN - 1);
    LEDGER.lane_samples[idx].fetch_add(1, Ordering::Relaxed);
    let now = crate::persona::recall_metadata::now_ms();
    // First stamp wins for the window; every stamp moves the last. 0 is "no sample yet",
    // which is why the first write is a compare-exchange and not a `fetch_min`.
    let _ = LEDGER
        .lane_first_sample_ms
        .compare_exchange(0, now, Ordering::Relaxed, Ordering::Relaxed);
    LEDGER.lane_last_sample_ms.fetch_max(now, Ordering::Relaxed);
}

/// How much of the window this hour's lane samples actually cover, first stamp to last.
fn lane_sample_span_ms() -> u64 {
    let first = LEDGER.lane_first_sample_ms.load(Ordering::Relaxed);
    let last = LEDGER.lane_last_sample_ms.load(Ordering::Relaxed);
    if first == 0 { 0 } else { last.saturating_sub(first) }
}

/// THE HOUR'S SUSTAINED LANES: the median of the lane counts observed at this window's
/// pull decisions, or `None` when the window is too thin to have a shape.
///
/// Pure, so the distributions are hand-computed tests. The median is "the smallest lane
/// count whose cumulative share passes half the samples" — for the 699 samples that
/// produced card `6d444769` (`1x155, 2x42, 3x344, 4x154, 5x2, 6x2`) that is **3**, while
/// `lanes_max` for the same window read **8** across a window those samples SPANNED (the
/// guard below), not a burst. The roster was 16. `16 > 3*2` is starved;
/// `16 > 8*2` is not, which is why the lane-bound rest never fired in three hours at
/// 98-100% pull deferral and `lane_bound_rested` was 0 in every one of them.
fn lanes_sustained_of(hist: &[u64; LANE_HIST_LEN], span_ms: u64) -> Option<u64> {
    let total: u64 = hist.iter().sum();
    // BOTH floors, and the span is the one that matters (Astra, #4356 review): enough
    // samples says the hour was BUSY; enough span says the hour was an HOUR. A burst
    // clears the first on its own, and a burst is exactly the transient the peak exists
    // to absorb — so below either floor there is no sustained value and the peak stands,
    // which actuates nothing.
    if total < LANE_SUSTAINED_MIN_SAMPLES || span_ms < LANE_SUSTAINED_MIN_SPAN_MS {
        return None;
    }
    let half = total / 2;
    let mut seen = 0u64;
    for (lanes, count) in hist.iter().enumerate() {
        seen = seen.saturating_add(*count);
        if seen > half {
            return Some(lanes as u64);
        }
    }
    None
}

/// The hour's lane distribution, read and reset by the tick.
fn snapshot_lane_hist_and_reset() -> ([u64; LANE_HIST_LEN], u64) {
    let mut out = [0u64; LANE_HIST_LEN];
    for (i, slot) in LEDGER.lane_samples.iter().enumerate() {
        out[i] = slot.swap(0, Ordering::Relaxed);
    }
    let span = lane_sample_span_ms();
    LEDGER.lane_first_sample_ms.store(0, Ordering::Relaxed);
    LEDGER.lane_last_sample_ms.store(0, Ordering::Relaxed);
    (out, span)
}

/// The same distribution WITHOUT reset, for the read-only `citizen/health` command.
fn read_lane_hist() -> [u64; LANE_HIST_LEN] {
    let mut out = [0u64; LANE_HIST_LEN];
    for (i, slot) in LEDGER.lane_samples.iter().enumerate() {
        out[i] = slot.load(Ordering::Relaxed);
    }
    out
}
/// A card pull was decided (the `bench.round.pull_*` seams). `deferred` = the lanes
/// were full, so she watched the board instead — the lane-bound signal.
pub fn note_pull(deferred: bool) {
    LEDGER.pulls.fetch_add(1, Ordering::Relaxed);
    // THE CAPACITY IS SAMPLED WHERE IT IS SPENT. The gate one frame above this decided
    // `deferred` against exactly this number (`work_pull`'s `served_lane_count()`), so
    // the hour's distribution is built from the values that actually turned work away.
    note_lane_sample(crate::cognition::resource_admission::served_lane_count() as u64);
    if deferred {
        LEDGER.pulls_deferred.fetch_add(1, Ordering::Relaxed);
    }
}

/// A turn iteration ended WITHOUT reaching the model (the `persona.act.lane_starved`
/// seam in the settle loop): she waited for a serving lane and never got one. Counted
/// apart from [`note_act`] — this is the hour's WAITING, and folding it into the acts is
/// what made a starved hour read as a working one.
pub fn note_lane_starved() {
    LEDGER.lane_starved.fetch_add(1, Ordering::Relaxed);
}

/// A generation reached its end, one way or the other (the
/// `crate::cognition::generation_drop::InFlight` seam). `dropped` = the awaiting side went
/// away while the model was still producing; anything else — a response, a typed adapter
/// refusal — is a terminal answer the substrate can read. Both increment the denominator,
/// so the hour can say what FRACTION of its work it threw away rather than a bare count
/// nobody can size.
pub fn note_generation_outcome(dropped: bool) {
    LEDGER.generations.fetch_add(1, Ordering::Relaxed);
    if dropped {
        LEDGER.generations_dropped.fetch_add(1, Ordering::Relaxed);
    }
}

/// A generation reported its prefill split (the `serving.kv.reuse` seam, fed by
/// `Workspace::note_generation` — the one KV writer). `cached` = prompt tokens the lane
/// served from its KV cache, `prefilled` = prompt tokens it had to re-read. Both 0 = the
/// lane reported no timings (cloud / older endpoints): an absence, never a 0% datum.
pub fn note_generation(cached: u32, prefilled: u32) {
    if cached == 0 && prefilled == 0 {
        return;
    }
    LEDGER.prompt_cached.fetch_add(u64::from(cached), Ordering::Relaxed);
    LEDGER.prompt_prefilled.fetch_add(u64::from(prefilled), Ordering::Relaxed);
}

/// An act was observed (the `persona.act.observed` seam). `wrote` = it changed a file.
pub fn note_act(wrote: bool) {
    LEDGER.acts.fetch_add(1, Ordering::Relaxed);
    if wrote {
        LEDGER.writes.fetch_add(1, Ordering::Relaxed);
    }
}
/// A lane was granted to a mind (the `delib.gate.lane_acquired` seam).
pub fn note_lane_granted() {
    LEDGER.lanes_granted.fetch_add(1, Ordering::Relaxed);
    note_lanes(crate::inference::llama_server::current_serving().lanes as u64);
}
/// Per-mind lane grants this hour — the placement chooser's and the lane-bound
/// page-out's "least served" order. One per-mind ledger ([`MindHour`]), not two.
pub fn note_lane_granted_to(persona: uuid::Uuid) {
    note_lane_granted();
    MINDS.entry(persona).or_default().lane_grants += 1;
}
pub fn lane_grants_of(persona: uuid::Uuid) -> u64 {
    MINDS.get(&persona).map(|m| m.lane_grants).unwrap_or(0) // JUSTIFIED unwrap_or: never granted this hour = 0, the truth
}
/// Per-mind speech discipline this hour — the MINDLESS receipt's inputs
/// (card aed15611; Joel: "mindless AIs should be accounted for"). Fed at the two
/// seams that already probe: the Speak verdict (`gate_refused` = a framing-echo or
/// not-speech pass — the substrate silenced her) and the act seam (`wrote`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MindHour {
    pub agent_name: String,
    pub verdicts: u64,
    pub gate_refused: u64,
    pub acts: u64,
    pub writes: u64,
    /// Serving lanes granted to her this hour — the "least served" order.
    pub lane_grants: u64,
}
static MINDS: std::sync::LazyLock<dashmap::DashMap<uuid::Uuid, MindHour>> = std::sync::LazyLock::new(dashmap::DashMap::new);
/// A Speak verdict was decided for a mind (the faculty's `verdict` seam).
pub fn note_verdict_of(persona: uuid::Uuid, agent_name: &str, gate_refused: bool) {
    let mut m = MINDS.entry(persona).or_default();
    if m.agent_name.is_empty() {
        m.agent_name = agent_name.to_string();
    }
    m.verdicts += 1;
    if gate_refused {
        m.gate_refused += 1;
    }
}
/// An act was observed for a mind (the same seam as [`note_act`], per mind).
pub fn note_act_of(persona: uuid::Uuid, wrote: bool) {
    let mut m = MINDS.entry(persona).or_default();
    m.acts += 1;
    if wrote {
        m.writes += 1;
    }
}
/// A mind is MINDLESS this hour when at least [`MINDLESS_MIN_VERDICTS`] of her Speak
/// verdicts were decided and at least [`MINDLESS_GATE_SHARE`] of them were the
/// substrate refusing what she said (a recital, an envelope), and she changed no
/// file. Pure — the rule the tick applies and a test can read.
pub const MINDLESS_MIN_VERDICTS: u64 = 6;
pub const MINDLESS_GATE_SHARE: f64 = 0.8;
pub fn is_mindless(m: &MindHour) -> bool {
    m.verdicts >= MINDLESS_MIN_VERDICTS
        && m.writes == 0
        && (m.gate_refused as f64) >= MINDLESS_GATE_SHARE * (m.verdicts as f64)
}
/// Never page the roster below this many resident minds, and never more than this
/// many seats in one tick — a page-out is a considered act, not a purge.
pub const MINDLESS_RESIDENT_FLOOR: u64 = 2;
pub const MINDLESS_MAX_PER_TICK: usize = 3;
/// The pure choice: which minds to page out this tick, worst first, bounded.
pub fn mindless_seats(minds: &[(uuid::Uuid, MindHour)], resident: u64) -> Vec<(uuid::Uuid, MindHour)> {
    let room = resident.saturating_sub(MINDLESS_RESIDENT_FLOOR) as usize;
    let mut out: Vec<(uuid::Uuid, MindHour)> = minds
        .iter()
        .filter(|(_, m)| is_mindless(m))
        .cloned()
        .collect();
    out.sort_by(|a, b| {
        let share = |m: &MindHour| m.gate_refused as f64 / m.verdicts.max(1) as f64;
        share(&b.1).partial_cmp(&share(&a.1)).unwrap_or(std::cmp::Ordering::Equal) // unwrap_or: NaN is impossible (verdicts ≥ 1); Equal keeps the order
    });
    out.truncate(room.min(MINDLESS_MAX_PER_TICK));
    out
}
/// LANE-BOUND, the rule (card c84d885a, S3): the roster is starved or at the knee, the
/// hour's pulls are a fair sample, and at least this share of them were deferred on the
/// lanes — the minds were queued, not idle, not mindless. The same four-in-five bar the
/// mindless rule uses; the same evidence floor.
pub const LANE_BOUND_DEFERRED_SHARE: f64 = MINDLESS_GATE_SHARE;
pub const LANE_BOUND_MIN_PULLS: u64 = MINDLESS_MIN_VERDICTS;
/// The reason every lane-bound rest carries — lives beside the record it marks, since
/// the record's own rule (a lane-bound rest outlives a deploy) reads it.
pub use crate::persona::resting_seat::LANE_BOUND_REASON;
pub fn is_lane_bound(h: &CitizenHealth, v: &Verdict) -> bool {
    matches!(v, Verdict::Starved { .. } | Verdict::AtKnee { .. })
        && h.pulls >= LANE_BOUND_MIN_PULLS
        && (h.pulls_deferred as f64) >= LANE_BOUND_DEFERRED_SHARE * (h.pulls as f64)
}
/// The pure choice: which seats REST this tick so the roster comes down to the healthy
/// edge (`lanes × MINDS_PER_LANE_STARVED_ABOVE` — the one number the STARVED verdict
/// already turns on). Least served first (no writes, then fewest lane grants), never
/// below the resident floor, and NOTHING on an hour whose pulls found no rounds — that
/// is a round-supply defect, not a seat one.
///
/// TO THE EDGE IN ONE TICK (Joel, 2026-09-20: the roster is BOUNDED by the warm slots,
/// not eased toward them). The per-tick cap was the mindless rule's — a page-out for
/// cause is a considered act, three at a time. A lane-bound rest is arithmetic: 16
/// minds on 3 lanes rested 3 an hour and the 22:22Z hour on the M5 still read 2,174 of
/// 2,177 pulls deferred, acts 5, writes 0. The floor still holds.
pub fn lane_bound_seats(h: &CitizenHealth, v: &Verdict, minds: &[(uuid::Uuid, MindHour)]) -> Vec<(uuid::Uuid, MindHour)> {
    if !is_lane_bound(h, v) {
        return Vec::new();
    }
    // The edge the verdict already turned on — the SUSTAINED lanes, so the remedy and the
    // rule that admits it divide by one number.
    let keep = h.lanes_sustained.saturating_mul(MINDS_PER_LANE_STARVED_ABOVE).max(MINDLESS_RESIDENT_FLOOR);
    let over = h.resident.saturating_sub(keep) as usize;
    let mut out: Vec<(uuid::Uuid, MindHour)> = minds.to_vec();
    out.sort_by_key(|(_, m)| (m.writes, m.lane_grants));
    out.truncate(over);
    out
}
/// THE MIRROR (Cormac's condition on S3): a lane-bound rest has a lane-bound WAKE. A
/// mindless seat returns on a CHANGE in her; a lane-bound seat was fine — the LANES were
/// short — so she returns when the lanes come back: while `lanes × MINDS_PER_LANE_STARVED_ABOVE`
/// has room above the residents, the most recently rested lane-bound seat (the most served
/// of those rested, since the least served rested first) wakes, as many as the room holds.
/// Without this every transient lane dip would permanently shrink the roster — the
/// one-direction shape (the peak ratchet, the claim expiry, the metronome) in a fourth
/// coat — and since a lane-bound rest now outlives a deploy, this wake and the operator's
/// word are the ONLY returns. Mindless rests are untouched. Pure.
pub const LANES_RETURNED_REASON: &str = "lanes returned";
pub fn lane_bound_wakes(h: &CitizenHealth, resting: &[crate::persona::resting_seat::RestingSeat]) -> Vec<crate::persona::resting_seat::RestingSeat> {
    // THE SAME STATISTIC AS THE REST, deliberately. Resting on the sustained value and
    // waking on the peak would rest a seat the moment the lanes sagged and wake her the
    // moment one fold point touched a high count — a seat flapping once an hour. The
    // lanes "return" when the hour HOLDS them.
    let edge = h.lanes_sustained.saturating_mul(MINDS_PER_LANE_STARVED_ABOVE);
    let room = edge.saturating_sub(h.resident) as usize;
    if room == 0 {
        return Vec::new();
    }
    let mut out: Vec<crate::persona::resting_seat::RestingSeat> = resting
        .iter()
        .filter(|s| s.reason.starts_with(LANE_BOUND_REASON))
        .cloned()
        .collect();
    out.sort_by_key(|s| std::cmp::Reverse(s.since_ms));
    out.truncate(room);
    out
}
fn snapshot_minds_and_reset() -> Vec<(uuid::Uuid, MindHour)> {
    let out: Vec<(uuid::Uuid, MindHour)> = MINDS.iter().map(|e| (*e.key(), e.value().clone())).collect();
    MINDS.clear();
    out
}

/// A round card settled (the `bench.round.card_settled` seam).
pub fn note_settle() {
    LEDGER.settles.fetch_add(1, Ordering::Relaxed);
}
/// A learning credit was staged / settled (the `training.credit.*` seams) — the
/// continual-learning half of the flywheel, in the same line as the work.
pub fn note_credit_staged() {
    LEDGER.credits_staged.fetch_add(1, Ordering::Relaxed);
}
pub fn note_credit_settled() {
    LEDGER.credits_settled.fetch_add(1, Ordering::Relaxed);
}

/// The hour's numbers, as read at the tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitizenHealth {
    pub window_secs: u64,
    pub resident: u64,
    /// The HOUR's lanes: the most the node served at any fold point this window (never
    /// less than the tick's own sample). The rest and the verdict stand on this.
    pub lanes: u64,
    /// The tick's own sample, for the line — `lanes 3 (max this hour; 1 now)` tells a
    /// reader the launch was between lanes at the tick.
    pub lanes_now: u64,
    /// THE LANES THE HOUR ACTUALLY HELD: the median of the lane counts seen at this
    /// window's pull decisions, falling back to [`CitizenHealth::lanes`] when the window
    /// is too thin to have a shape ([`LANE_SUSTAINED_MIN_SAMPLES`]).
    ///
    /// **The verdict and the rest divide by THIS, not by `lanes`** (card `6d444769`).
    /// `lanes` answers "did the node ever serve N" — a peak, and a `fetch_max` can only
    /// rise. The starvation rule asks "how many lanes did the queueing minds have", and a
    /// peak held for one fold point is not that number. Both stay in the receipt because
    /// THE GAP BETWEEN THEM IS THE DIAGNOSIS: 8 and 3 says the node touched eight lanes
    /// once and ran on three.
    pub lanes_sustained: u64,
    /// What a DIRECTED call waited for its lane this window, p50 / p90 ms, and how many
    /// waited — the reserved lane's guarantee in its own unit (Cormac, 2026-09-20). 0 waits
    /// = nothing directed arrived, which is not a zero wait.
    pub directed_wait_p50_ms: u64,
    pub directed_wait_p90_ms: u64,
    pub directed_waits: u64,
    pub served_window: u64,
    pub acts: u64,
    pub writes: u64,
    pub lanes_granted: u64,
    pub settles: u64,
    pub credits_staged: u64,
    pub credits_settled: u64,
    /// Card pulls the roster attempted this hour, and how many the full lanes deferred.
    pub pulls: u64,
    pub pulls_deferred: u64,
    /// Turns this hour that ended inside the reasoning channel — no answer, no act.
    pub think_only: u64,
    /// Turn iterations this hour that never reached the model — a wait, not an act, and
    /// not a think-only turn either. Its own term on the line (card cff534ba).
    pub lane_starved: u64,
    /// Generations this hour that reached a terminal answer, and the ones dropped in
    /// flight (card ebce2ba0). `generations` is the denominator the drop RATE is read
    /// against; 0 generations is an unmeasured hour, never a 0% loss hour.
    pub generations: u64,
    pub generations_dropped: u64,
    /// The measured decode knee for the served model (`inference::decode_knee`), when
    /// one is known: the lane count the planner will not exceed because every further
    /// stream would decode below the tax floor. Above it, lanes are not what is owed.
    pub knee: Option<u64>,
    /// Working citizen rounds on this node at the tick, and whether the standing
    /// autopilot is switched on. An IDLE roster with NO round is not a lanes story and
    /// not a seating story — there is nothing to seat into — and the line must say so.
    /// IntelMac 2026-09-19: thirty hours of "AT THE KNEE … owed: fewer seats here or
    /// more decode" over eight minds whose every pull read `no_rounds_on_node`, because
    /// the switch (`benchmark/standing`) defaults OFF and was silent about it.
    pub rounds_working: u64,
    pub standing_enabled: bool,
    /// The hour's prompt tokens served from the KV cache / prefilled (card c119ace7).
    /// Both 0 = no lane reported timings this hour, and the line says nothing rather
    /// than inventing a 0% reuse. See [`prefix_reuse_pct`].
    pub prompt_cached_tokens: u64,
    pub prompt_prefill_tokens: u64,
    /// Placement moves this hour by cause (card 10bba591) — see `Ledger`.
    pub moves_opportunity: u64,
    pub moves_failure: u64,
    /// The oldest dormant mind's last-turn age at the tick, from the grid allocator's
    /// published order; `None` = nobody dormant (or nothing published yet).
    pub oldest_dormant_turn_age_ms: Option<u64>,
}

/// The hour's PREFIX REUSE as a whole percentage — `cached / (cached + prefilled)`,
/// derived from the totals on every read, never stored and never averaged across turns
/// (averaging rates lies — `TurnMetrics::accumulate`'s rule). `None` until at least one
/// generation reported timings: an unmeasured hour is not a 0% hour.
///
/// A free function beside [`verdict`] and [`line`], not an inherent method: this module
/// reads the health struct through free functions, and giving `CitizenHealth` its first
/// `impl` block would make it read as unwired machinery to the production-reachability
/// guard (which is right — one more `impl` on a type nothing outside constructs).
pub fn prefix_reuse_pct(h: &CitizenHealth) -> Option<u64> {
    let total = h.prompt_cached_tokens.saturating_add(h.prompt_prefill_tokens);
    (total > 0).then(|| h.prompt_cached_tokens.saturating_mul(100) / total)
}

/// The hour's DROP RATE as a whole percentage — dropped / generations, derived on read
/// like [`prefix_reuse_pct`] and for the same reason. `None` until at least one
/// generation ended this hour: an hour that ran nothing did not lose 0%.
pub fn dropped_pct(h: &CitizenHealth) -> Option<u64> {
    (h.generations > 0).then(|| h.generations_dropped.saturating_mul(100) / h.generations)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Healthy,
    /// More minds than the lanes can turn in the window, and the planner has lanes to
    /// give (no knee, or lanes below it).
    Starved { resident: u64, lanes: u64 },
    /// More minds than the lanes can turn, and the lanes are AT the measured decode
    /// knee: one more lane would make every stream slower than the minds it was added
    /// for. The roster pages (the restore economy); what is owed is fewer seats on
    /// this box or more decode (a faster tier, another node) — never more lanes here.
    AtKnee { resident: u64, lanes: u64, knee: u64 },
    /// THE PIPE IS BROKEN, NOT THE MINDS (card ebce2ba0). Too large a share of this
    /// hour's generations were dropped in flight — the model was producing and the
    /// awaiting side went away. It outranks [`Verdict::Reading`] deliberately: 18 of 50
    /// generations cancelled on the M5 2026-09-20 rendered as "READING: n acts, no
    /// writes — the governor owes a delivery", which points a reader at the citizens.
    /// The citizens were writing; a bound above them was taking it back.
    Dropping { dropped: u64, generations: u64 },
    /// Acts without writes: reading and re-orienting, never delivering.
    Reading { acts: u64 },
    /// Residents, no acts at all — and WHY, as far as the node can read it: with no
    /// working round there is nothing to act on (a round is owed, not lanes); with
    /// rounds and the hour's pulls DEFERRED on the lanes the minds were seated and
    /// queued (the lanes are the block — M5 2026-09-19 13:0xZ: 4 minds, 1 lane, 0 lane
    /// grants, 398 of 398 pulls deferred, and the line said "a seating question");
    /// with rounds and pulls that found nothing, the seating is the question
    /// (`bench.round.pull_none` names it per mind).
    Idle { resident: u64, rounds_working: u64, standing_enabled: bool, lane_bound: bool },
    /// Writes happen, but too few for the roster: fewer than one write per
    /// [`RESIDENTS_PER_WRITE_HOUR`] residents in the hour.
    Slow { writes: u64, resident: u64 },
    /// No residents at the tick. Not a health verdict — a legible absence: a
    /// lane-donor node under the one-roster rule reads EMPTY with lanes offered; a
    /// node that lost its roster reads EMPTY with the grants it made before it did.
    /// It never reads "healthy" (Intel Mac 2026-09-15 10:2xZ: "resident 0 · lanes 0
    /// · lane grants 41 — healthy" after five despawns).
    Empty { lanes: u64, lanes_granted: u64 },
}

impl Verdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            Verdict::Healthy => "healthy",
            Verdict::Starved { .. } => "starved",
            Verdict::AtKnee { .. } => "at_knee",
            Verdict::Dropping { .. } => "dropping",
            Verdict::Reading { .. } => "reading",
            Verdict::Idle { .. } => "idle",
            Verdict::Slow { .. } => "slow",
            Verdict::Empty { .. } => "empty",
        }
    }
}

/// A roster is starved when it exceeds this many minds per served lane — and the ACTIVE
/// roster is bounded to it (Joel, 2026-09-20, on 23 minds over 3 warm slots: "the nursery
/// is over-saturated", the bound "is the way to go"): about two minds per slot, one
/// turning and one prefilled and waiting. At ~4-minute turns that is a lane every ~8
/// minutes each; three was a lane every ~12, and the hour it produced (16 on 3, acts 5,
/// 2,174 of 2,177 pulls deferred) was not useful. Minds past the edge rest — dormant, not
/// resident, not polling — and return when the lanes do or a card names them.
pub const MINDS_PER_LANE_STARVED_ABOVE: u64 = 2;

/// A node is DROPPING above this share of its generations thrown away in flight. One
/// dropped turn an hour is a bound trimming a genuine outlier; a tenth of them is a pipe
/// that reaps healthy work, and the loss compounds — every drop is a full prefill (five
/// to six minutes of a 27B lane on the M5) spent and discarded. The measured night this
/// was written ran at 36%.
pub const DROPPED_GENERATIONS_ABOVE_PCT: u64 = 10;

/// A roster is SLOW below one write per this many residents in an hour: 16 minds that
/// write twice in an hour (00:01Z 2026-09-15, the first receipt the core posted) are not
/// healthy, whatever the lanes say. Four residents per write-hour is a floor, not a goal.
pub const RESIDENTS_PER_WRITE_HOUR: u64 = 4;

/// The rule. Pure so the five shapes are hand-computed tests.
pub fn verdict(h: &CitizenHealth) -> Verdict {
    if h.resident == 0 {
        return Verdict::Empty { lanes: h.lanes, lanes_granted: h.lanes_granted };
    }
    if h.acts == 0 {
        return Verdict::Idle {
            resident: h.resident,
            rounds_working: h.rounds_working,
            standing_enabled: h.standing_enabled,
            // The same bar the lane-bound rest uses, so "queued on the lanes" means one
            // thing everywhere: a fair sample of pulls, four in five deferred.
            lane_bound: h.pulls >= LANE_BOUND_MIN_PULLS
                && (h.pulls_deferred as f64) >= LANE_BOUND_DEFERRED_SHARE * (h.pulls as f64),
        };
    }
    // SUSTAINED, NOT PEAK (card `6d444769`). The variants carry the number that was
    // JUDGED, so the line can never claim a starvation call was made against lanes the
    // hour did not have.
    if h.lanes_sustained > 0 && h.resident > h.lanes_sustained * MINDS_PER_LANE_STARVED_ABOVE {
        return match h.knee {
            Some(knee) if h.lanes_sustained >= knee => {
                Verdict::AtKnee { resident: h.resident, lanes: h.lanes_sustained, knee }
            }
            _ => Verdict::Starved { resident: h.resident, lanes: h.lanes_sustained },
        };
    }
    // BEFORE blaming the minds: did the substrate throw their work away? A node losing
    // this share of its generations has a bound problem, and every verdict below reads as
    // a capability verdict about the citizens.
    if dropped_pct(h).is_some_and(|pct| pct >= DROPPED_GENERATIONS_ABOVE_PCT) {
        return Verdict::Dropping { dropped: h.generations_dropped, generations: h.generations };
    }
    if h.writes == 0 {
        return Verdict::Reading { acts: h.acts };
    }
    if h.writes.saturating_mul(RESIDENTS_PER_WRITE_HOUR) < h.resident {
        return Verdict::Slow { writes: h.writes, resident: h.resident };
    }
    Verdict::Healthy
}

/// The org-room line: numbers first, the verdict last, one line.
pub fn line(h: &CitizenHealth, v: &Verdict) -> String {
    let tail = match v {
        Verdict::Healthy => "healthy".to_string(),
        Verdict::Starved { resident, lanes } => {
            format!("STARVED: {resident} minds on {lanes} lanes — the planner owes lanes")
        }
        Verdict::AtKnee { resident, lanes, knee } => format!(
            "AT THE KNEE: {resident} minds on {lanes} lanes, the measured decode knee is {knee} — the roster pages; owed: fewer seats here or more decode, never more lanes"
        ),
        Verdict::Dropping { dropped, generations } => format!(
            "DROPPING: {dropped} of {generations} generations thrown away in flight ({}%) — the model was \
             producing and the awaiting side went away; the bound that reaped them is on \
             `persona.generation.dropped`, and this is a PIPE fault, not a citizen one",
            dropped_pct(h).unwrap_or(0) // unwrap_or: unreachable — this verdict requires generations > 0
        ),
        Verdict::Reading { acts } => {
            format!("READING: {acts} acts, no writes — the progress note / governor owes a delivery")
        }
        Verdict::Idle { resident, rounds_working: 0, standing_enabled: false, .. } => format!(
            "IDLE: {resident} resident, no acts — NO ROUND on this node and the standing autopilot is OFF \
             (`benchmark/standing --enabled true`); owed: a round, not lanes"
        ),
        Verdict::Idle { resident, rounds_working: 0, standing_enabled: true, .. } => format!(
            "IDLE: {resident} resident, no acts — NO ROUND on this node with standing ON; the autopilot's \
             skip reason is on the probe stream (bench.standing.skipped)"
        ),
        Verdict::Idle { resident, rounds_working, lane_bound: true, .. } => format!(
            "IDLE: {resident} resident, no acts with {rounds_working} working round(s) — {} of {} pulls deferred on \
             {} lane(s), {} lane grants: the minds are seated and queued; the lanes are the block, not the seating",
            h.pulls_deferred, h.pulls, h.lanes, h.lanes_granted
        ),
        Verdict::Idle { resident, rounds_working, .. } => format!(
            "IDLE: {resident} resident, no acts with {rounds_working} working round(s) — a seating question \
             (bench.round.pull_none names it per mind)"
        ),
        Verdict::Empty { lanes, lanes_granted } => format!(
            "EMPTY: no residents — {lanes} lanes offered, {lanes_granted} grants this hour"
        ),
        Verdict::Slow { writes, resident } => format!(
            "SLOW: {writes} writes for {resident} residents — below one write per {RESIDENTS_PER_WRITE_HOUR} minds an hour"
        ),
    };
    // THE GAP IS THE DIAGNOSIS (card `6d444769`). The SUSTAINED count leads, because it is
    // the one the verdict and the rest divide by; the peak follows only when it disagrees.
    // A line reading `lanes 3 (peak 8 this hour; 1 now)` says, in one glance, that the node
    // touched eight lanes once and ran the hour on three — the fact that was invisible for
    // the three hours this rule was blind.
    let lanes = {
        let mut parts: Vec<String> = Vec::new();
        if h.lanes > h.lanes_sustained {
            parts.push(format!("peak {} this hour", h.lanes));
        }
        if h.lanes_now < h.lanes_sustained {
            parts.push(format!("{} now", h.lanes_now));
        }
        if parts.is_empty() {
            h.lanes_sustained.to_string()
        } else {
            format!("{} ({})", h.lanes_sustained, parts.join("; "))
        }
    };
    // The reserve's guarantee in its own unit: what a directed call waited, when any did.
    let directed = if h.directed_waits > 0 {
        format!(
            " · directed wait p50 {}s / p90 {}s ({} calls)",
            h.directed_wait_p50_ms / 1000,
            h.directed_wait_p90_ms / 1000,
            h.directed_waits
        )
    } else {
        String::new()
    };
    // The prompt cache's receipt, when any lane reported one: the fraction of every
    // prompt the engine did NOT re-read this hour (card c119ace7).
    let reuse = match prefix_reuse_pct(h) {
        Some(pct) => format!(
            " · prefix reuse {pct}% ({}k cached / {}k prefilled)",
            h.prompt_cached_tokens / 1000,
            h.prompt_prefill_tokens / 1000
        ),
        None => String::new(),
    };
    // WHAT THE SUBSTRATE THREW AWAY (card ebce2ba0): generations the awaiting side
    // abandoned while the model was still producing, with the share of the hour they
    // cost. `dropped 0` when the hour ran generations and kept them all; `dropped n/a`
    // when none ended this hour — an unmeasured hour is not a clean one.
    let dropped = match dropped_pct(h) {
        Some(pct) => format!("{} of {} ({pct}%)", h.generations_dropped, h.generations),
        None => "n/a".to_string(),
    };
    // THE GRID's half (card 10bba591): who moved and why — an opportunity move is the
    // allocation finding her a strictly better seat, a failure move is a seat that
    // stopped serving her — and how long the oldest dormant mind has waited for a clip
    // of the grid's slack ("dormant is not off"). Silent when nobody is dormant rather
    // than inventing a 0-minute wait, the same rule `reuse` and `directed` follow.
    let dormant = match h.oldest_dormant_turn_age_ms {
        Some(age) => format!(" · oldest dormant turn {} min ago", age / 60_000),
        None => String::new(),
    };
    format!(
        // THE WAYS AN HOUR CAN GO, SIDE BY SIDE AND NEVER SUMMED. `acts` is work that
        // reached the model and came back with hands; `lane-starved` never reached it at
        // all (card cff534ba — 6 and 25 minutes of waiting read as two of "8 acts" on the
        // M5, 2026-09-20); `think-only` reached it and ended inside the reasoning channel
        // (#4283); `prefix reuse` is how much of each prompt the engine did not re-read
        // (card c119ace7); `moves` is minds changing seats and `oldest dormant turn` is a
        // mind with no seat at all (card 10bba591). Six different problems, six owners.
        "[health] last {} min: resident {} · lanes {} @ {}k · acts {} · lane-starved {} · think-only {} · dropped {} · writes {} · lane grants {} · pulls {} ({} lane-deferred) · settles {} · learning credits {} staged / {} settled{}{} · moves: {} on opportunity / {} on failure{} — {}",
        h.window_secs / 60,
        h.resident,
        lanes,
        h.served_window / 1000,
        h.acts,
        h.lane_starved,
        h.think_only,
        dropped,
        h.writes,
        h.lanes_granted,
        h.pulls,
        h.pulls_deferred,
        h.settles,
        h.credits_staged,
        h.credits_settled,
        directed,
        reuse,
        h.moves_opportunity,
        h.moves_failure,
        dormant,
        tail
    )
}

/// The hour's lane-starved waits, read and reset by the tick. Its own reader for the
/// reason `snapshot_prompt_and_reset` is (card c119ace7, #4280): the main tuple is
/// already nine positional `u64`s, and on 2026-09-20 two PRs each widening it by one
/// landed a TENTH value behind a nine-wide signature — the arity was the only thing
/// that noticed. A separable counter cannot be mis-positioned by a merge.
fn snapshot_lane_starved_and_reset() -> u64 {
    LEDGER.lane_starved.swap(0, Ordering::Relaxed)
}

/// The hour's generations and the ones dropped in flight, read and reset by the tick.
/// Its own reader for the reason [`snapshot_lane_starved_and_reset`] is: the main tuple
/// is already nine positional `u64`s and a tenth cannot be mis-positioned by a merge if
/// it never joins the tuple.
fn snapshot_generations_and_reset() -> (u64, u64) {
    (
        LEDGER.generations.swap(0, Ordering::Relaxed),
        LEDGER.generations_dropped.swap(0, Ordering::Relaxed),
    )
}

/// The hour's prompt-cache totals (cached, prefilled), read and reset by the tick.
/// Its own reader, not a tenth slot on [`snapshot_and_reset`]'s tuple: the two are read
/// at the same tick but they are different ledgers, and a tuple that long stops being
/// legible at the call site.
fn snapshot_prompt_and_reset() -> (u64, u64) {
    (
        LEDGER.prompt_cached.swap(0, Ordering::Relaxed),
        LEDGER.prompt_prefilled.swap(0, Ordering::Relaxed),
    )
}

fn snapshot_and_reset() -> (u64, u64, u64, u64, u64, u64, u64, u64, u64) {
    (
        LEDGER.acts.swap(0, Ordering::Relaxed),
        LEDGER.writes.swap(0, Ordering::Relaxed),
        LEDGER.lanes_granted.swap(0, Ordering::Relaxed),
        LEDGER.settles.swap(0, Ordering::Relaxed),
        LEDGER.credits_staged.swap(0, Ordering::Relaxed),
        LEDGER.credits_settled.swap(0, Ordering::Relaxed),
        LEDGER.pulls.swap(0, Ordering::Relaxed),
        LEDGER.pulls_deferred.swap(0, Ordering::Relaxed),
        LEDGER.think_only.swap(0, Ordering::Relaxed),
    )
}

/// Page out this hour's mindless seats: flush every resident's working memory (her
/// checkpoint), take each chosen seat off the grid (the same orderly teardown as
/// `persona/instances/despawn`), and record the rest so the reconciler does not
/// re-draw her. Returns the names paged out, for the line.
/// Why a seat rests — the two rules that page out, each with its own receipt.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RestCause {
    Mindless,
    LaneBound,
}
async fn rest_seats(chosen: Vec<(uuid::Uuid, MindHour)>, cause: RestCause, h: &CitizenHealth) -> Vec<String> {
    if chosen.is_empty() {
        return Vec::new();
    }
    let Some(registry) = crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global() else {
        return Vec::new();
    };
    // Her checkpoint first — the flush is the explicit, repeatable save, on a
    // blocking worker like every other checkpoint pass (modules/cognition.rs).
    let flushed = tokio::task::spawn_blocking(|| {
        crate::cognition::persona_workspace::global().flush_volatile_all()
    })
    .await
    .unwrap_or_default(); // unwrap_or_default: a panicked flush task = no checkpoint receipts; the probe below then says checkpoint_saved=false
    let mut out = Vec::new();
    for (persona, m) in chosen {
        let saved = flushed.iter().any(|(id, r)| *id == persona && r.is_ok());
        let reason = match cause {
            RestCause::Mindless => format!(
                "{} of {} speak verdicts this hour were the gate refusing a recital or an envelope; {} acts, 0 writes",
                m.gate_refused, m.verdicts, m.acts
            ),
            RestCause::LaneBound => format!(
                "{LANE_BOUND_REASON}: {} minds on {} lanes, {} of {} pulls deferred on the lanes this hour; she had {} lane grants, {} writes — least served rests first",
                h.resident, h.lanes, h.pulls_deferred, h.pulls, m.lane_grants, m.writes
            ),
        };
        let Some(runtime) = registry.shutdown_slot(persona).await else {
            continue; // already gone this tick
        };
        let agent_name = runtime.agent_name().to_string();
        let since_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0); // unwrap_or: a clock before the epoch is not a real host; 0 reads as "unknown"
        let recorded = crate::persona::resting_seat::rest(crate::persona::resting_seat::RestingSeat {
            agent_name: agent_name.clone(),
            persona_id: persona,
            reason: reason.clone(),
            since_ms,
            build: crate::persona::resting_seat::current_build().to_string(),
        });
        match cause {
            RestCause::Mindless => crate::probe!(
                class = "persona.mindless.paged_out",
                persona = %agent_name,
                persona_id = %persona,
                verdicts = m.verdicts,
                gate_refused = m.gate_refused,
                acts = m.acts,
                checkpoint_saved = saved,
                recorded = recorded.is_ok(),
                reason = %reason,
                "a mindless seat was paged out with her checkpoint — she returns on a change, not a clock"
            ),
            RestCause::LaneBound => crate::probe!(
                class = "persona.lane_bound.rested",
                persona = %agent_name,
                persona_id = %persona,
                resident = h.resident,
                lanes = h.lanes,
                pulls = h.pulls,
                pulls_deferred = h.pulls_deferred,
                lane_grants = m.lane_grants,
                writes = m.writes,
                checkpoint_saved = saved,
                recorded = recorded.is_ok(),
                reason = %reason,
                "the receipt's actor: a lane-bound roster rests its least-served seat with her checkpoint — she returns on a change (a deploy, the operator's word), not a clock"
            ),
        }
        out.push(agent_name);
    }
    out
}

pub struct CitizenHealthModule;

impl CitizenHealthModule {
    pub fn new() -> Self {
        Self
    }
    fn read(&self) -> CitizenHealth {
        let (acts, writes, lanes_granted, settles, credits_staged, credits_settled, pulls, pulls_deferred, think_only) = snapshot_and_reset();
        let resident = crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()
            .map(|r| r.live_personas().len() as u64)
            .unwrap_or(0); // JUSTIFIED unwrap_or: no registry = no residents, and the verdict says so
        let serving = crate::inference::llama_server::current_serving();
        let lanes_now = serving.lanes as u64;
        // The hour's lanes: the window's max folded with this tick's sample, then the
        // window starts again from what is served now.
        let lanes = LEDGER.lanes_max.swap(lanes_now, Ordering::Relaxed).max(lanes_now);
        // The peak stands when the window had too few pulls to have a shape — a thin hour
        // is not evidence of a shortage.
        let (lane_hist, lane_span_ms) = snapshot_lane_hist_and_reset();
        let lanes_sustained = lanes_sustained_of(&lane_hist, lane_span_ms).unwrap_or(lanes); // JUSTIFIED unwrap_or: None means the window had no SHAPE — too few samples, or a burst too brief to be an hour — and the peak is the conservative read, which rests nobody
        let (rounds_working, standing_enabled) = round_supply();
        let (directed_wait_p50_ms, directed_wait_p90_ms, directed_waits) =
            crate::cognition::resource_admission::directed_lane_wait_ms();
        let (prompt_cached_tokens, prompt_prefill_tokens) = snapshot_prompt_and_reset();
        let lane_starved = snapshot_lane_starved_and_reset();
        let (generations, generations_dropped) = snapshot_generations_and_reset();
        let (moves_opportunity, moves_failure) = snapshot_moves_and_reset();
        CitizenHealth {
            window_secs: HEALTH_WINDOW.as_secs(),
            resident,
            lanes,
            lanes_now,
            lanes_sustained,
            directed_wait_p50_ms,
            directed_wait_p90_ms,
            directed_waits: directed_waits as u64,
            served_window: serving.served_context_window as u64,
            acts,
            writes,
            lanes_granted,
            settles,
            credits_staged,
            credits_settled,
            pulls,
            pulls_deferred,
            think_only,
            generations,
            generations_dropped,
            knee: knee_of(serving.active_model.as_deref()),
            rounds_working,
            standing_enabled,
            prompt_cached_tokens,
            prompt_prefill_tokens,
            lane_starved,
            moves_opportunity,
            moves_failure,
            oldest_dormant_turn_age_ms: oldest_dormant_turn_age_ms(),
        }
    }
}

/// The node's round supply at the tick: working citizen rounds, and whether the standing
/// autopilot is on — the two facts that turn "IDLE" into "IDLE because …". One reader
/// for both the hourly receipt and the `citizen/health` command.
fn round_supply() -> (u64, bool) {
    let rounds_working = crate::cognition::bench_round::live_rounds()
        .iter()
        .filter(|r| crate::persona::work_pull::is_working_citizen_round(r))
        .count() as u64;
    (rounds_working, crate::modules::benchmark_standing::is_enabled())
}

/// The served model's measured decode knee, if any — the fact that turns "owes lanes"
/// into "at the knee".
fn knee_of(model: Option<&str>) -> Option<u64> {
    model.and_then(crate::inference::decode_knee::knee_for).map(u64::from)
}

impl Default for CitizenHealthModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ServiceModule for CitizenHealthModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "citizen_health",
            priority: ModulePriority::Normal,
            command_prefixes: &["citizen/health"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: Some(HEALTH_WINDOW),
        }
    }
    async fn initialize(&self, _ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
        Ok(())
    }
    async fn tick(&self) -> Result<(), String> {
        let h = self.read();
        let v = verdict(&h);
        // THE OWNER ACTS (card aed15611): a mind whose hour was recitals and gate-passes
        // is paged out WITH her checkpoint — her lane goes to a working mind, and she
        // returns on a change (a deploy, a trained gene, the operator's word), never
        // on a clock. The lake and the rabbits: her working memory is flushed first.
        let minds = snapshot_minds_and_reset();
        let paged_out = rest_seats(mindless_seats(&minds, h.resident), RestCause::Mindless, &h).await;
        // THE RECEIPT'S ACTOR (card c84d885a, S3): a starved / at-the-knee roster whose
        // hour was lane-deferrals rests its least-served seats down to the healthy edge
        // — what "owed: fewer seats" was asking a human to do. Never on a pull_none hour.
        let remaining: Vec<(uuid::Uuid, MindHour)> =
            minds.iter().filter(|(_, m)| !paged_out.contains(&m.agent_name)).cloned().collect();
        let after_mindless = CitizenHealth { resident: h.resident.saturating_sub(paged_out.len() as u64), ..h.clone() };
        let rested = rest_seats(lane_bound_seats(&after_mindless, &v, &remaining), RestCause::LaneBound, &after_mindless).await;
        // THE MIRROR: the lanes came back — the lane-bound seats come back, most served
        // first, same cap. (Rest and wake cannot both fire: one needs residents above the
        // edge, the other room below it.)
        let mut woken: Vec<String> = Vec::new();
        for seat in lane_bound_wakes(&after_mindless, &crate::persona::resting_seat::resting()) {
            if crate::persona::resting_seat::wake(&seat.agent_name) {
                crate::probe!(
                    class = "persona.lane_bound.woken",
                    persona = %seat.agent_name,
                    persona_id = %seat.persona_id,
                    resident = after_mindless.resident,
                    lanes = h.lanes,
                    rested_since_ms = seat.since_ms,
                    reason = LANES_RETURNED_REASON,
                    "the lanes came back — a lane-bound seat returns at the next reconcile"
                );
                woken.push(seat.agent_name);
            }
        }
        crate::probe!(
            class = "citizen.health.hour",
            resident = h.resident,
            lanes = h.lanes,
            served_window = h.served_window,
            acts = h.acts,
            writes = h.writes,
            lane_grants = h.lanes_granted,
            pulls = h.pulls,
            pulls_deferred = h.pulls_deferred,
            think_only = h.think_only,
            lane_starved = h.lane_starved,
            generations = h.generations,
            generations_dropped = h.generations_dropped,
            dropped_pct = dropped_pct(&h).unwrap_or(0), // unwrap_or: 0 = no generation ended this hour; `generations` says so
            settles = h.settles,
            credits_staged = h.credits_staged,
            credits_settled = h.credits_settled,
            mindless_paged_out = paged_out.len() as u64,
            lane_bound_rested = rested.len() as u64,
            lane_bound_woken = woken.len() as u64,
            moves_opportunity = h.moves_opportunity,
            moves_failure = h.moves_failure,
            oldest_dormant_turn_age_ms = h.oldest_dormant_turn_age_ms.unwrap_or(0), // unwrap_or: 0 = nobody dormant
            verdict = v.as_str(),
            "the hour's citizen health — the substrate's own read"
        );
        let mut said = line(&h, &v);
        if !paged_out.is_empty() {
            said.push_str(&format!(" · mindless {} paged out ({})", paged_out.len(), paged_out.join(", ")));
        }
        if !rested.is_empty() {
            said.push_str(&format!(" · lane-bound: {} rested to the healthy edge ({})", rested.len(), rested.join(", ")));
        }
        if !woken.is_empty() {
            said.push_str(&format!(" · lanes returned: {} woken ({})", woken.len(), woken.join(", ")));
        }
        crate::modules::grid::say_in_org_room(&said).await;
        Ok(())
    }
    async fn handle_command(&self, command: &str, _params: serde_json::Value) -> Result<CommandResult, String> {
        match command {
            // A read WITHOUT reset: the hour's running counters as they stand.
            "citizen/health" => {
                let (rounds_working, standing_enabled) = round_supply();
                let h = CitizenHealth {
                    window_secs: HEALTH_WINDOW.as_secs(),
                    resident: crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()
                        .map(|r| r.live_personas().len() as u64)
                        .unwrap_or(0), // JUSTIFIED unwrap_or: no registry = no residents
                    lanes: LEDGER.lanes_max.load(Ordering::Relaxed).max(crate::inference::llama_server::current_serving().lanes as u64),
                    lanes_now: crate::inference::llama_server::current_serving().lanes as u64,
                    // A read WITHOUT reset here too — the hour's shape as it stands.
                    lanes_sustained: lanes_sustained_of(&read_lane_hist(), lane_sample_span_ms()).unwrap_or_else(|| { // JUSTIFIED unwrap_or_else: same thin-window fallback as the tick's read — the peak, which rests nobody
                        LEDGER.lanes_max.load(Ordering::Relaxed).max(crate::inference::llama_server::current_serving().lanes as u64)
                    }),
                    directed_wait_p50_ms: crate::cognition::resource_admission::directed_lane_wait_ms().0,
                    directed_wait_p90_ms: crate::cognition::resource_admission::directed_lane_wait_ms().1,
                    directed_waits: crate::cognition::resource_admission::directed_lane_wait_ms().2 as u64,
                    served_window: crate::inference::llama_server::current_serving().served_context_window as u64,
                    acts: LEDGER.acts.load(Ordering::Relaxed),
                    writes: LEDGER.writes.load(Ordering::Relaxed),
                    lanes_granted: LEDGER.lanes_granted.load(Ordering::Relaxed),
                    settles: LEDGER.settles.load(Ordering::Relaxed),
                    credits_staged: LEDGER.credits_staged.load(Ordering::Relaxed),
                    credits_settled: LEDGER.credits_settled.load(Ordering::Relaxed),
                    pulls: LEDGER.pulls.load(Ordering::Relaxed),
                    pulls_deferred: LEDGER.pulls_deferred.load(Ordering::Relaxed),
                    think_only: LEDGER.think_only.load(Ordering::Relaxed),
                    lane_starved: LEDGER.lane_starved.load(Ordering::Relaxed),
                    generations: LEDGER.generations.load(Ordering::Relaxed),
                    generations_dropped: LEDGER.generations_dropped.load(Ordering::Relaxed),
                    knee: knee_of(crate::inference::llama_server::current_serving().active_model.as_deref()),
                    rounds_working,
                    standing_enabled,
                    prompt_cached_tokens: LEDGER.prompt_cached.load(Ordering::Relaxed),
                    prompt_prefill_tokens: LEDGER.prompt_prefilled.load(Ordering::Relaxed),
                    moves_opportunity: LEDGER.moves_opportunity.load(Ordering::Relaxed),
                    moves_failure: LEDGER.moves_failure.load(Ordering::Relaxed),
                    oldest_dormant_turn_age_ms: oldest_dormant_turn_age_ms(),
                };
                let v = verdict(&h);
                CommandResult::json(&serde_json::json!({
                    "resident": h.resident, "lanes": h.lanes, "served_window": h.served_window,
                    "acts": h.acts, "writes": h.writes, "lane_grants": h.lanes_granted, "settles": h.settles,
                    "credits_staged": h.credits_staged, "credits_settled": h.credits_settled,
                    "pulls": h.pulls, "pulls_deferred": h.pulls_deferred, "think_only": h.think_only,
                    "lane_starved": h.lane_starved,
                    "generations": h.generations, "generations_dropped": h.generations_dropped,
                    "dropped_pct": dropped_pct(&h),
                    "rounds_working": h.rounds_working, "standing_enabled": h.standing_enabled,
                    "prompt_cached_tokens": h.prompt_cached_tokens, "prompt_prefill_tokens": h.prompt_prefill_tokens,
                    "prefix_reuse_pct": prefix_reuse_pct(&h),
                    "moves_opportunity": h.moves_opportunity, "moves_failure": h.moves_failure,
                    "oldest_dormant_turn_age_ms": h.oldest_dormant_turn_age_ms,
                    "verdict": v.as_str(), "line": line(&h, &v),
                    "note": "counters since the last hourly tick (not reset by this read)"
                }))
            }
            other => Err(format!("citizen_health: unknown command '{other}' — try 'citizen/health'")),
        }
    }
    fn command_schemas(&self) -> Vec<CommandSchema> {
        vec![CommandSchema {
            name: "citizen/health",
            description: "The hour's citizen health as the substrate reads it: residents, lanes, acts, lane-starved waits, think-only turns, dropped generations, writes, lane grants, settles, prefix reuse, and the verdict (healthy / starved / dropping / reading / idle)",
            params: vec![],
        }]
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card aed15611): the MINDLESS rule and the bounded choice. A
    // mind whose verdicts were ≥80% gate refusals with no write is mindless; a mind
    // that wrote once, or was refused half the time, or spoke fewer than the floor,
    // is not. The choice is worst-first, never below the resident floor, never more
    // than the per-tick cap.
    #[test]
    fn a_mind_of_recitals_is_paged_out_worst_first_and_never_below_the_floor() {
        let mind = |name: &str, verdicts, refused, acts, writes| MindHour {
            agent_name: name.into(), verdicts, gate_refused: refused, acts, writes, lane_grants: acts,
        };
        assert!(is_mindless(&mind("Sigurd", 21, 18, 3, 0)));
        assert!(!is_mindless(&mind("wrote", 21, 18, 3, 1)), "a write is a mind");
        assert!(!is_mindless(&mind("half", 10, 5, 0, 0)), "half refused is a struggling mind, not a mindless one");
        assert!(!is_mindless(&mind("quiet", 5, 5, 0, 0)), "below the verdict floor: not enough evidence");
        let minds: Vec<(uuid::Uuid, MindHour)> = [
            mind("A", 10, 8, 0, 0),
            mind("B", 20, 20, 0, 0),
            mind("C", 10, 9, 0, 0),
            mind("D", 10, 10, 0, 0),
            mind("ok", 10, 1, 4, 2),
        ]
        .into_iter()
        .map(|m| (uuid::Uuid::new_v4(), m))
        .collect();
        let chosen = mindless_seats(&minds, 16);
        let names: Vec<&str> = chosen.iter().map(|(_, m)| m.agent_name.as_str()).collect();
        assert_eq!(names, ["B", "D", "C"], "worst first, capped at {MINDLESS_MAX_PER_TICK}");
        assert_eq!(mindless_seats(&minds, 3).len(), 1, "3 resident − floor 2 = one seat may rest");
        assert!(mindless_seats(&minds, 2).is_empty(), "at the floor nobody is paged out");
    }

    fn h(resident: u64, lanes: u64, acts: u64, writes: u64) -> CitizenHealth {
        CitizenHealth { window_secs: 3600, resident, lanes, lanes_now: lanes, lanes_sustained: lanes, directed_wait_p50_ms: 0, directed_wait_p90_ms: 0, directed_waits: 0, served_window: 66_000, acts, writes, lanes_granted: acts, settles: 0, credits_staged: 0, credits_settled: 0, pulls: 0, pulls_deferred: 0, think_only: 0, lane_starved: 0, generations: 0, generations_dropped: 0, knee: None, rounds_working: 1, standing_enabled: true, prompt_cached_tokens: 0, prompt_prefill_tokens: 0, moves_opportunity: 0, moves_failure: 0, oldest_dormant_turn_age_ms: None }
    }

    // what this catches (card 10bba591): the line carries the grid's half — moves by
    // cause, and the oldest dormant turn when anyone is dormant (never a "0 min ago" for
    // nobody) — and the move ledger is a window like the rest.
    #[test]
    fn the_line_carries_the_moves_and_the_oldest_dormant_turn() {
        let x = h(4, 2, 10, 2);
        let l = line(&x, &verdict(&x));
        assert!(l.contains("moves: 0 on opportunity / 0 on failure"), "{l}");
        assert!(!l.contains("oldest dormant"), "nobody dormant: the line does not invent an age: {l}");
        let moved = CitizenHealth { moves_opportunity: 2, moves_failure: 1, oldest_dormant_turn_age_ms: Some(23 * 60_000 + 5_000), ..x };
        let l = line(&moved, &verdict(&moved));
        assert!(l.contains("moves: 2 on opportunity / 1 on failure · oldest dormant turn 23 min ago"), "{l}");
        note_move(true);
        note_move(false);
        note_move(false);
        let (o, f) = snapshot_moves_and_reset();
        assert!(o >= 1 && f >= 2);
        assert_eq!(snapshot_moves_and_reset(), (0, 0), "a window, not a lifetime");
    }

    // what this catches (2026-09-19, IntelMac): an IDLE roster says WHY. Thirty hours of
    // "AT THE KNEE … owed: fewer seats here or more decode" over eight minds whose every
    // pull read `no_rounds_on_node` — the standing switch was OFF and nothing said so.
    // No round + switch off names the switch; no round + switch on points at the
    // autopilot's own skip probe; rounds + no acts is a seating question, not a supply one.
    #[test]
    fn an_idle_roster_names_the_reason_it_is_idle() {
        let idle = |rounds_working: u64, standing_enabled: bool| CitizenHealth { rounds_working, standing_enabled, ..h(8, 2, 0, 0) };
        let off = idle(0, false);
        let v = verdict(&off);
        assert_eq!(v, Verdict::Idle { resident: 8, rounds_working: 0, standing_enabled: false, lane_bound: false });
        let said = line(&off, &v);
        assert!(said.contains("NO ROUND") && said.contains("autopilot is OFF") && said.contains("benchmark/standing"), "{said}");
        assert!(said.contains("not lanes"), "the owed thing is named: {said}");
        let on = idle(0, true);
        let said = line(&on, &verdict(&on));
        assert!(said.contains("standing ON") && said.contains("bench.standing.skipped"), "{said}");
        let seated = idle(1, true);
        let said = line(&seated, &verdict(&seated));
        assert!(said.contains("1 working round") && said.contains("seating"), "{said}");
        assert!(!said.contains("autopilot"), "with a round in flight the switch is not the story: {said}");
        // The M5's hour (2026-09-19 13:0xZ): 4 minds, 1 lane, 0 grants, 398/398 pulls
        // deferred, 7 rounds. Seated and queued — the lanes, not the seating.
        let queued = CitizenHealth { pulls: 398, pulls_deferred: 398, lanes: 1, lanes_sustained: 1, lanes_granted: 0, ..idle(7, true) };
        let v = verdict(&queued);
        assert!(matches!(v, Verdict::Idle { lane_bound: true, .. }), "{v:?}");
        let said = line(&queued, &v);
        assert!(said.contains("398 of 398 pulls deferred") && said.contains("lanes are the block"), "{said}");
        assert!(!said.contains("seating question"), "queued minds are not a seating question: {said}");
        // Below the evidence floor (a handful of pulls) it stays a seating question — one
        // deferred pull is not a lane-bound hour.
        let thin = CitizenHealth { pulls: 3, pulls_deferred: 3, lanes: 1, lanes_sustained: 1, ..idle(7, true) };
        assert!(matches!(verdict(&thin), Verdict::Idle { lane_bound: false, .. }));
    }

    // what this catches: the four shapes of 2026-09-14 named by the rule — 16 on 3 is
    // STARVED before it is anything else; acts without writes is READING; a roster
    // with no acts is IDLE; and a roster on enough lanes that writes is HEALTHY.
    #[test]
    fn the_verdict_names_the_afternoons_shapes() {
        assert_eq!(verdict(&h(16, 3, 39, 4)), Verdict::Starved { resident: 16, lanes: 3 });
        // 16 on 6 was "enough lanes" at three per lane; at two per lane it is STARVED and
        // 16 on 8 is the edge — the shapes below stand on 8 lanes.
        assert_eq!(verdict(&h(16, 6, 39, 0)), Verdict::Starved { resident: 16, lanes: 6 });
        assert_eq!(verdict(&h(16, 8, 39, 0)), Verdict::Reading { acts: 39 });
        assert_eq!(
            verdict(&h(16, 8, 0, 0)),
            Verdict::Idle { resident: 16, rounds_working: 1, standing_enabled: true, lane_bound: false }
        );
        assert_eq!(verdict(&h(16, 8, 39, 4)), Verdict::Healthy);
        // The first receipt the core ever posted (00:01Z 2026-09-15): 16 residents,
        // 6 lanes, 37 acts, 2 writes — it said "healthy". It is SLOW (on 8 lanes; on 6 it
        // is starved first).
        assert_eq!(verdict(&h(16, 8, 37, 2)), Verdict::Slow { writes: 2, resident: 16 });
        assert_eq!(verdict(&h(4, 2, 10, 1)), Verdict::Healthy, "one write per four minds is the floor, inclusive");
        // 2026-09-16 06:5xZ, the first hour under the decode knee: "STARVED: 16 minds on
        // 5 lanes — the planner owes lanes" while the planner was holding lanes AT the
        // knee on purpose. Lanes at the knee are not owed; seats or decode are.
        let at_knee = CitizenHealth { knee: Some(5), ..h(16, 5, 70, 4) };
        assert_eq!(verdict(&at_knee), Verdict::AtKnee { resident: 16, lanes: 5, knee: 5 });
        assert!(line(&at_knee, &verdict(&at_knee)).contains("never more lanes"));
        let below_knee = CitizenHealth { knee: Some(8), ..h(16, 5, 70, 4) };
        assert_eq!(verdict(&below_knee), Verdict::Starved { resident: 16, lanes: 5 }, "below the knee the planner still owes lanes");
        assert_eq!(verdict(&h(0, 0, 0, 0)), Verdict::Empty { lanes: 0, lanes_granted: 0 }, "an empty node is EMPTY, never 'healthy'");
    }

    // what this catches (card ebce2ba0): a dropped generation reaches the hour line as its
    // OWN number and its own verdict. The M5 2026-09-20 threw away 18 of 50 generations —
    // six at ~165.9 s, four at ~412.2 s, two at ~485.2 s (all of those abandoned at the
    // serving gate before dispatch) and five at 1,207–1,492 s with the model still
    // producing — and the hour line said "READING: n acts, no writes", which points a
    // reader at the citizens. A regression that folds `dropped` into any other counter, or
    // lets READING outrank it, puts the blame back on the minds.
    #[test]
    fn a_dropped_generation_is_counted_rendered_and_outranks_the_reading_verdict() {
        let _ = snapshot_generations_and_reset(); // start this assertion from a known floor
        note_generation_outcome(false);
        note_generation_outcome(true);
        note_generation_outcome(true);
        let (generations, dropped) = snapshot_generations_and_reset();
        // `>=`, not `==`: the ledger is a process global and a sibling test may be
        // generating into it. An UNDER-count is the only thing this can miss, and that is
        // the failure worth catching.
        assert!(generations >= 3 && dropped >= 2, "the ledger counts both halves: {generations}/{dropped}");

        // The rate, derived on read and never stored. An hour that ran nothing did not
        // lose 0% of it.
        let clean = CitizenHealth { generations: 50, generations_dropped: 0, ..h(4, 2, 10, 2) };
        assert_eq!(dropped_pct(&clean), Some(0));
        assert_eq!(dropped_pct(&h(4, 2, 10, 2)), None, "an unmeasured hour is not a clean one");
        assert!(line(&clean, &verdict(&clean)).contains("dropped 0 of 50 (0%)"));
        assert!(line(&h(4, 2, 10, 2), &verdict(&h(4, 2, 10, 2))).contains("dropped n/a"));

        // The measured night: 18 of 50 = 36%, well past the threshold.
        let m5 = CitizenHealth { generations: 50, generations_dropped: 18, ..h(4, 2, 13, 0) };
        assert_eq!(dropped_pct(&m5), Some(36));
        assert_eq!(
            verdict(&m5),
            Verdict::Dropping { dropped: 18, generations: 50 },
            "36% thrown away is a PIPE fault; it must not render as READING"
        );
        let l = line(&m5, &verdict(&m5));
        assert!(l.contains("dropped 18 of 50 (36%)"), "{l}");
        assert!(l.contains("DROPPING"), "{l}");
        assert!(!l.contains("READING"), "{l}");
        // Below the threshold the older verdicts still stand — this does not swallow them.
        let occasional = CitizenHealth { generations: 50, generations_dropped: 2, ..h(4, 2, 13, 0) };
        assert_eq!(verdict(&occasional), Verdict::Reading { acts: 13 }, "4% is a bound trimming an outlier");
        let healthy = CitizenHealth { generations: 50, generations_dropped: 2, ..h(4, 2, 13, 4) };
        assert_eq!(verdict(&healthy), Verdict::Healthy);
    }

    // what this catches: the line carries every number and ends with the verdict, so a
    // reader of the org room acts without a probe query.
    #[test]
    fn the_line_carries_the_numbers_and_ends_with_the_verdict() {
        let x = h(16, 3, 39, 4);
        let l = line(&x, &verdict(&x));
        for needle in ["resident 16", "lanes 3", "acts 39", "writes 4", "think-only 0", "lane-starved 0", "STARVED"] {
            assert!(l.contains(needle), "{needle} missing from {l}");
        }
        assert!(!l.contains("directed wait"), "no directed call waited: the line does not invent a zero wait");
        // The reserve's guarantee in its own unit, when a directed call did wait.
        let waited = CitizenHealth { directed_wait_p50_ms: 4_200, directed_wait_p90_ms: 61_000, directed_waits: 3, ..x.clone() };
        let l = line(&waited, &verdict(&waited));
        assert!(l.contains("directed wait p50 4s / p90 61s (3 calls)"), "{l}");
        // The prompt cache's receipt (card c119ace7): an unmeasured hour says nothing,
        // a measured one says the fraction of every prompt the lanes did not re-read.
        assert!(!l.contains("prefix reuse"), "no lane reported timings: the line does not invent a 0% reuse");
        let warm = CitizenHealth { prompt_cached_tokens: 228_000, prompt_prefill_tokens: 92_000, ..x.clone() };
        assert_eq!(prefix_reuse_pct(&warm), Some(71));
        let l = line(&warm, &verdict(&warm));
        assert!(l.contains("prefix reuse 71% (228k cached / 92k prefilled)"), "{l}");
    }

    // what this catches: the hour line folding waiting into working, and a merge
    // collapsing the ways an hour fails into one number. M5, 2026-09-20 ~21:55Z (build
    // 0047d521b, 4 residents on 2 lanes at 67,072): the line read "8 acts, 0 writes"
    // while two of those turns were `persona.act.pace` rows with `model_ms=0` and
    // `residue_ms` equal to the whole act — 365,000 and 1,500,001 — i.e. a mind who
    // waited 6 minutes and one who waited 25, both at `lanes_available=0`. A turn that
    // never got a lane (lane-starved), a turn that got one and ended inside the
    // reasoning channel (think-only, #4283), and the acts are THREE different failures
    // with three different owners. Separate terms, in order, never summed.
    #[test]
    fn the_line_says_acts_lane_starved_and_think_only_as_separate_terms() {
        let m5 = CitizenHealth { acts: 6, lane_starved: 2, think_only: 1, ..h(4, 2, 6, 0) };
        let l = line(&m5, &verdict(&m5));
        for needle in ["acts 6", "lane-starved 2", "think-only 1", "writes 0"] {
            assert!(l.contains(needle), "{needle} missing from {l}");
        }
        // Read left to right: work, then the two ways a turn produced none.
        let at = |n: &str| l.find(n).unwrap_or_else(|| panic!("{n} missing from {l}"));
        assert!(
            at("acts 6") < at("lane-starved 2") && at("lane-starved 2") < at("think-only 1"),
            "the three terms keep their order: {l}"
        );
        // Never one number standing for several.
        assert!(!l.contains("acts 8"), "the waits are not folded into the acts: {l}");
        assert!(!l.contains("acts 9"), "nor the think-only turns: {l}");
    }

    // what this catches: the ledger is a window — a tick reads AND resets, so the next
    // hour starts from zero rather than accumulating the day.
    #[test]
    fn the_tick_reads_a_window_not_a_lifetime() {
        note_act(true);
        note_act(false);
        note_lane_granted();
        note_settle();
        note_credit_staged();
        note_pull(true);
        note_think_only();
        let (a, w, l, s, c, _, p, pd, t) = snapshot_and_reset();
        assert!(a >= 2 && w >= 1 && l >= 1 && s >= 1 && c >= 1 && p >= 1 && pd >= 1);
        // what this catches (the M5, 2026-09-20): a turn that ends inside the reasoning
        // channel is counted for the hour line, and resets with the rest of the window.
        assert!(t >= 1, "a think-only turn is on the hour's ledger");
        let (a2, _, _, _, _, _, p2, _, t2) = snapshot_and_reset();
        assert_eq!((a2, p2, t2), (0, 0, 0));
        // The prompt-cache totals are a window too (card c119ace7), and an unmeasured
        // generation (no timings: 0/0) leaves them untouched.
        note_generation(0, 0);
        note_generation(22_800, 9_300);
        // The lane-starved counter is its own separable window (card cff534ba).
        note_lane_starved();
        assert!(snapshot_lane_starved_and_reset() >= 1, "a lane-starved wait is on the hour's ledger");
        assert_eq!(snapshot_lane_starved_and_reset(), 0, "the tick reset its window");
        let (cached, prefilled) = snapshot_prompt_and_reset();
        assert!(cached >= 22_800 && prefilled >= 9_300, "cached {cached} prefilled {prefilled}");
        // A window, not a lifetime: the read reset it. (`<`, not `== 0`: the ledger is a
        // process global and a parallel test may fold a generation between the two reads.)
        let (cached2, prefilled2) = snapshot_prompt_and_reset();
        assert!(
            cached2 + prefilled2 < cached + prefilled,
            "the tick must reset the window: {cached2}/{prefilled2} after {cached}/{prefilled}"
        );
    }

    // what this catches (card c84d885a, S3 — the receipt's actor): the M5's 2026-09-18
    // hour, 16 minds on 2 lanes with 424 lane-deferred pulls, rests its least-served seats
    // toward the healthy edge (2 lanes × 3) — bounded per tick, worst-served first, WHY
    // on the record; and the IntelMac's hour, 8 minds with 278 pulls that found NO
    // round, rests NOBODY — that is a round-supply defect, and paging the roster for it
    // would delete the roster and report success. The edge, the floor and the cap are the
    // rule's own constants, not a grid's numbers.
    #[test]
    fn a_lane_bound_roster_rests_its_least_served_seats_and_a_roundless_one_rests_nobody() {
        let mind = |name: &str, grants: u64, writes: u64| MindHour {
            agent_name: name.into(), verdicts: 10, gate_refused: 1, acts: grants, writes, lane_grants: grants,
        };
        let roster: Vec<(uuid::Uuid, MindHour)> = [
            mind("wrote", 30, 2), mind("busy", 40, 0), mind("least", 1, 0), mind("less", 3, 0), mind("some", 9, 0),
        ]
        .into_iter()
        .map(|m| (uuid::Uuid::new_v4(), m))
        .collect();
        // The M5 hour: 16 on 2, 424 of 430 pulls deferred, AT THE KNEE.
        let m5 = CitizenHealth { pulls: 430, pulls_deferred: 424, knee: Some(2), ..h(16, 2, 50, 3) };
        let v = verdict(&m5);
        assert_eq!(v, Verdict::AtKnee { resident: 16, lanes: 2, knee: 2 });
        assert!(is_lane_bound(&m5, &v));
        let rested = lane_bound_seats(&m5, &v, &roster);
        let names: Vec<&str> = rested.iter().map(|(_, m)| m.agent_name.as_str()).collect();
        assert_eq!(
            names,
            ["least", "less", "some", "busy", "wrote"],
            "least served first, to the edge in ONE tick (16 on 2 is 12 over the edge of 4); a mind that wrote rests last"
        );
        // Toward the edge, never past it: 7 minds on 2 lanes is three over the edge of 4.
        let seven = CitizenHealth { resident: 7, ..m5.clone() };
        assert_eq!(lane_bound_seats(&seven, &verdict(&seven), &roster).len(), 3);
        // Starved below the knee pays the same debt.
        let starved = CitizenHealth { knee: Some(8), ..m5.clone() };
        assert_eq!(verdict(&starved), Verdict::Starved { resident: 16, lanes: 2 });
        assert_eq!(lane_bound_seats(&starved, &verdict(&starved), &roster).len(), roster.len());
        // The IntelMac hour: 8 minds, 278 pulls, none deferred — NOTHING rests.
        let intel = CitizenHealth { pulls: 278, pulls_deferred: 0, ..h(8, 1, 12, 0) };
        assert!(matches!(verdict(&intel), Verdict::Starved { .. }), "starved by the numbers");
        assert!(!is_lane_bound(&intel, &verdict(&intel)), "but a roundless hour is not lane-bound");
        assert!(lane_bound_seats(&intel, &verdict(&intel), &roster).is_empty());
        // Too few pulls to judge, or a healthy verdict: nothing.
        let thin = CitizenHealth { pulls: LANE_BOUND_MIN_PULLS - 1, pulls_deferred: LANE_BOUND_MIN_PULLS - 1, ..m5.clone() };
        assert!(!is_lane_bound(&thin, &verdict(&thin)));
        let fine = CitizenHealth { pulls: 100, pulls_deferred: 100, ..h(4, 2, 20, 4) };
        assert_eq!(verdict(&fine), Verdict::Healthy);
        assert!(lane_bound_seats(&fine, &verdict(&fine), &roster).is_empty(), "a healthy roster is never paged");
        // The edge on one lane is 2 (the resident floor): 2 minds on 1 lane is AT the edge,
        // nobody rests; 3 on 1 rests one.
        let one_lane = CitizenHealth { resident: 2, lanes: 1, lanes_sustained: 1, pulls: 50, pulls_deferred: 50, ..m5.clone() };
        assert!(lane_bound_seats(&one_lane, &verdict(&one_lane), &roster).is_empty(), "2 on 1 is the edge, nobody rests");
        let three_on_one = CitizenHealth { resident: 3, ..one_lane.clone() };
        assert_eq!(lane_bound_seats(&three_on_one, &verdict(&three_on_one), &roster).len(), 1, "3 on 1 is one over");
        // THE HOUR'S LANES, NOT THE TICK'S (Cormac's condition): a tick that samples the
        // launch between lanes reads 1; the hour HELD 3. The rest stands on 3 — 8 minds
        // rest to the edge of 6 (two), not to the floor of 2 (six) — and the line says so.
        let between = CitizenHealth { resident: 8, lanes: 3, lanes_sustained: 3, lanes_now: 1, knee: Some(3), ..m5.clone() };
        assert_eq!(lane_bound_seats(&between, &verdict(&between), &roster).len(), 2, "the edge of the hour's 3 lanes, not the tick's 1");
        assert!(line(&between, &verdict(&between)).contains("lanes 3 (1 now)"));
        assert!(line(&m5, &v).contains("pulls 430 (424 lane-deferred)"), "the line carries the pulls");
    }

    // what this catches (card `6d444769`): the median of the hour's pull-time lane samples,
    // and the thin-window guard that keeps a launch from passing as an hour. The
    // distribution is the REAL one measured on the M5 on 2026-09-23 — 699 samples taken at
    // the pull gate across the three hours whose receipts all said `lanes 8`.
    #[test]
    fn the_sustained_lane_count_is_the_median_of_the_hours_pull_samples() {
        let mut hist = [0u64; LANE_HIST_LEN];
        hist[1] = 155;
        hist[2] = 42;
        hist[3] = 344;
        hist[4] = 154;
        hist[5] = 2;
        hist[6] = 2;
        assert_eq!(hist.iter().sum::<u64>(), 699);
        // NEVER 8 in 699 samples, and the receipts for those hours all read `lanes 8`.
        assert_eq!(hist[8], 0);
        let hour = LANE_SUSTAINED_MIN_SPAN_MS;
        assert_eq!(lanes_sustained_of(&hist, hour), Some(3), "the hour ran on three lanes");

        // A SINGLE BAD TICK CANNOT MOVE IT (Cormac's condition on #4244, kept): drop one
        // relaunch sample of 1 lane into an otherwise four-lane hour and the median holds.
        let mut steady = [0u64; LANE_HIST_LEN];
        steady[4] = 99;
        steady[1] = 1;
        assert_eq!(lanes_sustained_of(&steady, hour), Some(4), "one dip is not the hour");

        // A WINDOW WITH NO SHAPE IS NOT EVIDENCE. Below the sample floor there is no
        // median to speak of, and the caller falls back to the peak — which rests nobody.
        let mut thin = [0u64; LANE_HIST_LEN];
        thin[1] = LANE_SUSTAINED_MIN_SAMPLES - 1;
        assert_eq!(lanes_sustained_of(&thin, hour), None, "a launch window is not an hour");
        thin[1] += 1;
        assert_eq!(lanes_sustained_of(&thin, hour), Some(1), "at the floor it has a shape");
    }

    // what this catches (Astra's review of #4356): A COUNT IS NOT COVERAGE. Pulls are not
    // spread evenly through the hour — they arrive when the roster polls — so a dozen
    // residents queueing through ONE brief relaunch clear any sample floor while every
    // sample reads the launch's single lane. The median of that burst is 1, and acting on
    // it would rest the roster for a transient, which is precisely the hazard the peak was
    // put there to absorb (Cormac, #4244). The SPAN is what tells the two apart.
    #[test]
    fn a_burst_of_samples_inside_one_relaunch_is_not_a_sustained_shortage() {
        // Sixteen residents, each polling twice while the engine relaunches: 32 samples,
        // comfortably past the count floor, every one of them at one lane.
        let mut burst = [0u64; LANE_HIST_LEN];
        burst[1] = 32;
        assert!(burst.iter().sum::<u64>() > LANE_SUSTAINED_MIN_SAMPLES, "the count floor alone is cleared");
        let relaunch_ms = 30 * 1_000;
        assert_eq!(
            lanes_sustained_of(&burst, relaunch_ms),
            None,
            "thirty seconds of queueing is a relaunch, not an hour — the peak stands and nobody rests"
        );
        // The SAME distribution spread across a real span IS a sustained shortage, and
        // that is the whole distinction: the node genuinely held one lane for ten minutes.
        assert_eq!(
            lanes_sustained_of(&burst, LANE_SUSTAINED_MIN_SPAN_MS),
            Some(1),
            "the same samples, actually spanning the window, are the hour's shape"
        );
        // And the guard gates ACTUATION, not just the number: with no sustained value the
        // caller keeps the peak, so the rest computes against 8 lanes and pages out nobody.
        let roster: Vec<(uuid::Uuid, MindHour)> = (0..16)
            .map(|i| (uuid::Uuid::new_v4(), MindHour { lane_grants: i, ..Default::default() }))
            .collect();
        let during_relaunch = CitizenHealth {
            resident: 16,
            lanes: 8,
            lanes_sustained: 8, // what `unwrap_or(lanes)` yields when the span guard refuses
            pulls: 32,
            pulls_deferred: 32,
            ..h(16, 8, 15, 0)
        };
        assert!(
            lane_bound_seats(&during_relaunch, &verdict(&during_relaunch), &roster).is_empty(),
            "a relaunch must never rest the roster"
        );
    }

    // what this catches (card `6d444769`, the defect itself): a roster that is starved on
    // the lanes it HELD must not read healthy because the node touched a higher count once.
    // This is the exact receipt that went unremedied for three hours — resident 16, the
    // line saying `lanes 8`, 98-100% of pulls deferred, and `lane_bound_rested: 0`.
    #[test]
    fn a_peak_lane_count_can_no_longer_hide_a_starved_roster() {
        let roster: Vec<(uuid::Uuid, MindHour)> = (0..16)
            .map(|i| (uuid::Uuid::new_v4(), MindHour { lane_grants: i, ..Default::default() }))
            .collect();
        let measured = CitizenHealth {
            resident: 16,
            lanes: 8,             // the peak the receipt printed
            lanes_sustained: 3,   // the median of the 699 samples above
            lanes_now: 3,
            pulls: 708,
            pulls_deferred: 708,
            ..h(16, 8, 15, 0)
        };
        assert_eq!(
            verdict(&measured),
            Verdict::Starved { resident: 16, lanes: 3 },
            "judged on the lanes the hour held, and the verdict SAYS 3 so the line cannot claim otherwise"
        );
        assert!(is_lane_bound(&measured, &verdict(&measured)), "708 of 708 deferred is queued, not idle");
        // The edge is 3 x 2 = 6, so ten of the sixteen rest — the remedy that never fired.
        assert_eq!(lane_bound_seats(&measured, &verdict(&measured), &roster).len(), 10);
        // AND THE GAP IS IN THE LINE, because the gap is the diagnosis.
        assert!(line(&measured, &verdict(&measured)).contains("lanes 3 (peak 8 this hour)"));

        // THE OLD ARITHMETIC, for contrast: divide by the peak and 16 > 8 x 2 is false, so
        // the roster fell through to a verdict ABOUT THE CITIZENS and nothing rested.
        let by_peak = CitizenHealth { lanes_sustained: 8, ..measured.clone() };
        assert_eq!(verdict(&by_peak), Verdict::Reading { acts: 15 });
        assert!(lane_bound_seats(&by_peak, &verdict(&by_peak), &roster).is_empty());
    }

    // what this catches (Cormac's condition on S3 — the one-direction shape in a fourth
    // coat): a lane-bound rest has a lane-bound WAKE. Two seats rested at lanes = 1 stay
    // rested while lanes = 1; the tick at lanes = 2 wakes them, the most recently rested
    // (the most served) first, under the same cap; a MINDLESS rest is never woken by
    // lanes — she must have changed.
    #[test]
    fn a_lane_bound_rest_has_a_lane_bound_wake_and_a_mindless_rest_does_not() {
        use crate::persona::resting_seat::RestingSeat;
        let seat = |name: &str, reason: &str, since_ms: u64| RestingSeat {
            agent_name: name.into(), persona_id: uuid::Uuid::new_v4(), reason: reason.into(), since_ms, build: "b".into(),
        };
        let resting = vec![
            seat("least", &format!("{LANE_BOUND_REASON}: 5 minds on 1 lanes"), 100),
            seat("less", &format!("{LANE_BOUND_REASON}: 5 minds on 1 lanes"), 200),
            seat("recital", "18 of 21 speak verdicts this hour were the gate refusing a recital", 300),
        ];
        // Still 1 lane, 2 resident: the edge is 2, no room — nobody wakes.
        let one = CitizenHealth { pulls: 50, pulls_deferred: 50, ..h(2, 1, 10, 0) };
        assert!(lane_bound_wakes(&one, &resting).is_empty(), "lanes did not return");
        // 2 lanes: the edge is 4, room for 3 — both lane-bound seats wake, most served first.
        let two = CitizenHealth { ..h(1, 2, 10, 0) };
        let woken: Vec<String> = lane_bound_wakes(&two, &resting).into_iter().map(|s| s.agent_name).collect();
        assert_eq!(woken, ["less", "least"], "the last to rest is the first back; the recital stays rested");
        // Room for exactly one: only the most served returns.
        let tight = CitizenHealth { ..h(3, 2, 10, 0) };
        let woken: Vec<String> = lane_bound_wakes(&tight, &resting).into_iter().map(|s| s.agent_name).collect();
        assert_eq!(woken, ["less"]);
        // Rest and wake never both fire: over the edge there is no room; under it, nothing rests.
        let over = CitizenHealth { pulls: 50, pulls_deferred: 50, knee: Some(2), ..h(16, 2, 50, 3) };
        assert!(lane_bound_wakes(&over, &resting).is_empty());
    }
}
