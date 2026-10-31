//! The citizens' health LEDGER: what each mind did this hour (acts, writes, verdicts, lane
//! grants, settles, moves), fed at the mind's own seams and read by the hourly health tick
//! (`modules::citizen_health`). The facts are the mind's, so they live in the mind ring; the
//! service module that snapshots, judges and speaks them stays in the surface.

use continuum_base::time::EpochMs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// One hour: long enough for a fair sample of a roster that gets a lane every few
/// minutes, short enough that a starved evening is caught before it is a night.
pub const HEALTH_WINDOW: Duration = Duration::from_secs(3600);

#[derive(Default)]
pub struct Ledger {
    pub acts: AtomicU64,
    pub writes: AtomicU64,
    pub lanes_granted: AtomicU64,
    pub settles: AtomicU64,
    pub credits_staged: AtomicU64,
    pub credits_settled: AtomicU64,
    /// How every completed generation stopped, counted once at the one place it is
    /// classified (see [`Stop`]). Counting at a branch instead missed every cut: Aris's
    /// three 963-token cuts inside thinking (the M5, 2026-10-30) read "think-only 0".
    pub(crate) stops: StopCounters,
    /// Turn iterations that NEVER REACHED THE MODEL — she waited at the serving gate and
    /// produced nothing (the `persona.act.lane_starved` seam, card cff534ba). The sibling
    /// of the stops and NOT the same failure: a generation that stopped (landed, cut, or
    /// ended in its reasoning) GOT a lane; a lane-starved turn never got one. On the M5
    /// 2026-09-20 ~21:55Z two of the hour's turns were `model_ms=0` waits of 6 and 25
    /// minutes at `lanes_available=0`, and the line called them acts.
    pub lane_starved: AtomicU64,
    /// THE HOUR'S LANES, not the tick's (Cormac's condition on #4244): the most lanes the
    /// node served at any fold point this window. The lane-bound rest is destructive and
    /// now rests the whole overage in one tick, so it must not stand on a point sample —
    /// a launch between lanes at the tick reads 1 and would rest fourteen of sixteen. Max
    /// is the right fold for a destructive act: a lane truly lost rests one hour later; a
    /// lane momentarily absent rests nobody. Folded at the geometry settle and at every
    /// lane grant, read and reset by the tick.
    pub lanes_max: AtomicU64,
    /// THE HOUR'S PREFIX REUSE (card c119ace7): prompt tokens the lanes served from the
    /// KV cache vs. prompt tokens they had to prefill, summed over every generation
    /// that reported timings. The ratio is the fraction of every prompt the engine did
    /// NOT re-read — on the M5 at ~90 tok/s prefill, a 30k prompt at 0% reuse is six
    /// minutes of silence per act, at 75% it is ninety seconds. Separate totals, ratio
    /// derived on read (averaging rates lies — `TurnMetrics::accumulate`'s rule).
    pub prompt_cached: AtomicU64,
    pub prompt_prefilled: AtomicU64,
    /// GENERATIONS THIS HOUR AND THE ONES THROWN AWAY (card ebce2ba0). A generation is
    /// counted once it has a terminal answer — a response OR a typed adapter refusal,
    /// both of which reached the model — and `generations_dropped` counts the ones the
    /// awaiting side abandoned WHILE THE MODEL WAS STILL PRODUCING (the
    /// `persona.generation.dropped` seam). NOT the same failure as `lane_starved`: a
    /// lane-starved turn never reached the model, a dropped one did and its output was
    /// discarded. On the M5 2026-09-20 five of an evening's generations died this way at
    /// 1,207,290–1,492,456 ms — the act deadline reaping work that was still inside its
    /// own stated bound, and the hour line called it READING (lazy personas).
    pub generations: AtomicU64,
    pub generations_dropped: AtomicU64,
    /// Placement moves this hour, by cause (card 10bba591): ON OPPORTUNITY — the grid
    /// allocation seated her on a strictly better seat and the switch took it between
    /// turns; ON FAILURE — a seat went dark, cold, queued or too narrow and she fell
    /// home, returned, or spilled off a starved node.
    pub moves_opportunity: AtomicU64,
    pub moves_failure: AtomicU64,
    /// THE HOUR'S LANE DISTRIBUTION, bucketed by lane count (card `6d444769`). Sampled at
    /// the LANE ASK — the moment the capacity is actually spent — so the shape is the
    /// one the queueing minds met, not the one the node touched once. Read by
    /// [`lanes_sustained_of`]; `lanes_max` is unchanged and still the peak.
    /// THE HOUR'S LANES, ONE VALUE PER TIME BUCKET. Not a histogram of samples: a
    /// histogram counts ASKS, and asks arrive in bursts, so whoever polls most often
    /// decides the median (Astra's second review of #4356 — a count floor AND a
    /// first-to-last span floor are both cleared by one stray early sample plus a burst).
    /// Each bucket contributes exactly once, so an hour is judged by its MINUTES.
    pub(crate) lane_bucket_max: [AtomicU64; LANE_BUCKETS],
    /// When this window's bucketing began. 0 = no sample yet this window.
    pub(crate) lane_window_start_ms: AtomicU64,
}

pub static LEDGER: Ledger = Ledger {
    acts: AtomicU64::new(0),
    writes: AtomicU64::new(0),
    lanes_granted: AtomicU64::new(0),
    settles: AtomicU64::new(0),
    credits_staged: AtomicU64::new(0),
    credits_settled: AtomicU64::new(0),
    stops: StopCounters::new(),
    lane_starved: AtomicU64::new(0),
    generations: AtomicU64::new(0),
    generations_dropped: AtomicU64::new(0),
    lanes_max: AtomicU64::new(0),
    prompt_cached: AtomicU64::new(0),
    prompt_prefilled: AtomicU64::new(0),
    moves_opportunity: AtomicU64::new(0),
    moves_failure: AtomicU64::new(0),
    lane_bucket_max: [const { AtomicU64::new(0) }; LANE_BUCKETS],
    lane_window_start_ms: AtomicU64::new(0),
};

/// How one completed generation stopped: the outcome the hour line counts. Mapped once
/// from the emission classifier's verdict where it is made, so every generation lands in
/// exactly one class whatever branch handles it afterwards. A generation that ERRORED never
/// completed and is not a stop: it is counted in `generations_dropped`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// The model stopped on its own with an answer or a call.
    Landed,
    /// The model stopped on its own having produced nothing: no answer, no call, no reasoning.
    /// It said and did nothing, so it is never read as work.
    Empty,
    /// The model stopped on its own inside its reasoning: no answer, no call.
    ThinkEnded,
    /// Cut at the output limit inside the answer.
    CutAnswer,
    /// Cut at the output limit inside the reasoning, nothing said.
    CutThink,
    /// Landed, with the think stopped at the reasoning budget the request set.
    ThinkBudget,
}

/// One counter per [`Stop`], fed only by [`note_stop`].
#[derive(Default)]
pub(crate) struct StopCounters {
    landed: AtomicU64,
    empty: AtomicU64,
    think_ended: AtomicU64,
    cut_answer: AtomicU64,
    cut_think: AtomicU64,
    think_budget: AtomicU64,
}

impl StopCounters {
    pub(crate) const fn new() -> Self {
        Self {
            landed: AtomicU64::new(0),
            empty: AtomicU64::new(0),
            think_ended: AtomicU64::new(0),
            cut_answer: AtomicU64::new(0),
            cut_think: AtomicU64::new(0),
            think_budget: AtomicU64::new(0),
        }
    }

    /// Count one generation that stopped as `stop`.
    pub(crate) fn note(&self, stop: Stop) {
        self.counter(stop).fetch_add(1, Ordering::Relaxed);
    }

    fn counter(&self, stop: Stop) -> &AtomicU64 {
        match stop {
            Stop::Landed => &self.landed,
            Stop::Empty => &self.empty,
            Stop::ThinkEnded => &self.think_ended,
            Stop::CutAnswer => &self.cut_answer,
            Stop::CutThink => &self.cut_think,
            Stop::ThinkBudget => &self.think_budget,
        }
    }

    fn read(&self, take: impl Fn(&AtomicU64) -> u64) -> StopCounts {
        StopCounts {
            landed: take(&self.landed),
            empty: take(&self.empty),
            think_ended: take(&self.think_ended),
            cut_answer: take(&self.cut_answer),
            cut_think: take(&self.cut_think),
            think_budget: take(&self.think_budget),
        }
    }
}

/// The hour's stops, as read: every class side by side, never summed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StopCounts {
    pub landed: u64,
    pub empty: u64,
    pub think_ended: u64,
    pub cut_answer: u64,
    pub cut_think: u64,
    pub think_budget: u64,
}

/// One completed generation stopped as `stop`. Called once per generation, at the
/// emission classifier.
pub fn note_stop(stop: Stop) {
    LEDGER.stops.note(stop);
}

/// The hour's stops, read and reset by the tick. Its own reader, for the reason
/// [`snapshot_lane_starved_and_reset`] gives.
pub fn snapshot_stops_and_reset() -> StopCounts {
    LEDGER.stops.read(|c| c.swap(0, Ordering::Relaxed))
}

/// The hour's stops so far, without a reset (the on-demand health read).
pub fn read_stops() -> StopCounts {
    LEDGER.stops.read(|c| c.load(Ordering::Relaxed))
}

/// A placement move landed (the `placement.move.opportunity` seam and the switch's
/// fall-home / return / spill seams). `opportunity` = a better seat, not a failed one.
pub fn note_move(opportunity: bool) {
    if opportunity {
        LEDGER.moves_opportunity.fetch_add(1, Ordering::Relaxed);
    } else {
        LEDGER.moves_failure.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn snapshot_moves_and_reset() -> (u64, u64) {
    (LEDGER.moves_opportunity.swap(0, Ordering::Relaxed), LEDGER.moves_failure.swap(0, Ordering::Relaxed))
}

/// The node served `lanes` lanes just now — fold into the hour's maximum (see
/// `Ledger::lanes_max`). Called at the serving daemon's geometry settle and at every
/// lane grant, so the hour's lanes are the lanes the hour actually served.
pub fn note_lanes(lanes: u64) {
    LEDGER.lanes_max.fetch_max(lanes, Ordering::Relaxed);
}

/// THIRTY buckets of two minutes each, covering the hour's window.
///
/// Thirty and not sixty for the reason #4356 already learned once: `std` implements
/// `Default` for `[T; N]` only up to N = 32, and `Ledger` derives it. Two minutes is
/// also comfortably longer than any relaunch this substrate performs, which is what
/// makes the per-bucket MAXIMUM absorb one — a bucket that contained a relaunch and
/// also served normally reports the normal value.
pub const LANE_BUCKETS: usize = 30;

pub(crate) const LANE_BUCKET_MS: u64 = 2 * 60 * 1_000;

/// How many DISTINCT buckets must carry a sample before the hour has a shape. Ten
/// buckets is twenty minutes of separate two-minute periods — not twenty minutes of
/// elapsed time between two samples, which is the guard Astra broke: one stray early
/// sample plus a burst cleared both a count floor and a first-to-last span.
// derived-or-floor: a sample-sufficiency floor over the hour's own 30 buckets (a third of them),
// not a stand-in for any rate or duration the substrate measures.
pub const LANE_MIN_BUCKETS: usize = 10;

/// The node had `lanes` lanes at a moment work was actually asked of them — fold into
/// THIS bucket's maximum. Called from [`note_lane_granted`] and [`note_lane_starved`],
/// never on its own: the sample is taken where the capacity is spent (until 2026-10-04 at
/// the card pull, which is gone; a lane asked for is the same moment).
///
/// **Maximum within the bucket, deliberately.** A burst of lane asks during a relaunch
/// all read the launch's single lane; if any ask in the same two minutes saw the real
/// lane count, that is what the bucket reports. The bias is AGAINST resting the roster, which
/// is the direction an error should take (Cormac's condition on #4244).
pub(crate) fn note_lane_sample(lanes: u64) {
    let now = EpochMs::now().millis();
    // First sample of the window opens the bucketing clock.
    let _ = LEDGER
        .lane_window_start_ms
        .compare_exchange(0, now, Ordering::Relaxed, Ordering::Relaxed);
    let start = LEDGER.lane_window_start_ms.load(Ordering::Relaxed);
    let idx = now
        .saturating_sub(start)
        .checked_div(LANE_BUCKET_MS)
        .unwrap_or(0) as usize; // JUSTIFIED unwrap_or: LANE_BUCKET_MS is a non-zero const, so the division cannot fail
    // Past the window's end, keep folding into the last bucket rather than wrapping
    // onto bucket 0 and contaminating the start of the hour.
    LEDGER.lane_bucket_max[idx.min(LANE_BUCKETS - 1)].fetch_max(lanes, Ordering::Relaxed);
}

/// THE HOUR'S SUSTAINED LANES: the median over the buckets that carried a sample, or
/// `None` when too few distinct buckets did.
///
/// Pure, so the shapes are hand-computed tests. This is the statistic the verdict
/// divides by. For the window that produced card `6d444769` — 699
/// lane-time samples reading `1x155, 2x42, 3x344, 4x154, 5x2, 6x2`, spread across the
/// hour — the per-minute picture is a node that held about three lanes, and `lanes_max`
/// for the same window read **8**. The roster was 16. `16 > 3*2` is starved;
/// `16 > 8*2` is not, which is why a starved roster read as a working one for three hours.
///
/// **A burst cannot win here.** Astra's counterexample — one 8-lane sample at t0 and
/// thirty-two 1-lane samples during a relaunch ten minutes later — touches exactly TWO
/// buckets, so it is below [`LANE_MIN_BUCKETS`] and yields `None`. Volume buys nothing;
/// only MINUTES do.
pub fn lanes_sustained_of(buckets: &[u64; LANE_BUCKETS]) -> Option<u64> {
    let mut seen: Vec<u64> = buckets.iter().copied().filter(|&v| v > 0).collect();
    if seen.len() < LANE_MIN_BUCKETS {
        return None;
    }
    seen.sort_unstable();
    Some(seen[seen.len() / 2])
}

/// The hour's buckets, read and reset by the tick.
pub fn snapshot_lane_buckets_and_reset() -> [u64; LANE_BUCKETS] {
    let mut out = [0u64; LANE_BUCKETS];
    for (i, slot) in LEDGER.lane_bucket_max.iter().enumerate() {
        out[i] = slot.swap(0, Ordering::Relaxed);
    }
    LEDGER.lane_window_start_ms.store(0, Ordering::Relaxed);
    out
}

/// The same buckets WITHOUT reset, for the read-only `citizen/health` command.
pub fn read_lane_buckets() -> [u64; LANE_BUCKETS] {
    let mut out = [0u64; LANE_BUCKETS];
    for (i, slot) in LEDGER.lane_bucket_max.iter().enumerate() {
        out[i] = slot.load(Ordering::Relaxed);
    }
    out
}

/// A turn iteration ended WITHOUT reaching the model (the `persona.act.lane_starved`
/// seam in the settle loop): she waited for a serving lane and never got one. Counted
/// apart from [`note_act`] — this is the hour's WAITING, and folding it into the acts is
/// what made a starved hour read as a working one.
pub fn note_lane_starved() {
    LEDGER.lane_starved.fetch_add(1, Ordering::Relaxed);
    note_lane_sample(continuum_engine::inference::resource_admission::served_lane_count() as u64);
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
    // The same split, per turn, for the prefill knee's median (card e370a673).
    continuum_engine::inference::prefill_knee::note_turn(cached, prefilled);
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
    note_lanes(continuum_engine::inference::llama_server::current_serving().lanes as u64);
    note_lane_sample(continuum_engine::inference::resource_admission::served_lane_count() as u64);
}

/// Per-mind lane grants this hour — the placement chooser's "least served" order. One per-mind ledger ([`MindHour`]), not two.
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

pub(crate) static MINDS: std::sync::LazyLock<dashmap::DashMap<uuid::Uuid, MindHour>> = std::sync::LazyLock::new(dashmap::DashMap::new);

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
// derived-or-floor: an evidence floor (how many decided verdicts before a pattern is judged), a
// count of her own acts, not a measured quantity.
pub const MINDLESS_MIN_VERDICTS: u64 = 6;

// derived-or-floor: a policy share of her own verdicts (four in five refused), a ratio of counts
// the ledger keeps, not a rate, a size or a time.
pub const MINDLESS_GATE_SHARE: f64 = 0.8;

pub fn is_mindless(m: &MindHour) -> bool {
    m.verdicts >= MINDLESS_MIN_VERDICTS
        && m.writes == 0
        && (m.gate_refused as f64) >= MINDLESS_GATE_SHARE * (m.verdicts as f64)
}

/// Never page the roster below this many resident minds, and never more than this
/// many seats in one tick — a page-out is a considered act, not a purge.
// derived-or-floor: a floor that only ever keeps minds resident (never pages below two), a
// roster policy, not a measurement.
pub const MINDLESS_RESIDENT_FLOOR: u64 = 2;

// derived-or-floor: a pacing cap on a considered act (at most three page-outs a tick), so a
// rest is never a purge; nothing measured stands behind it.
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

/// THE SLOW CLIP (card ef25bf6c; Joel: dormant is not off, every mind gets a slow clip at
/// any grid size). With every seat on the grid taken, a mind outside the draw waited
/// forever: 8 of 18 on the IntelMac, the oldest 533 minutes, with no resting record for
/// any wake to reach. Each hourly pass, while a dormant mind waits and no seat was already
/// freed this pass, ONE resident yields: the least served of those whose hour held no act
/// and no write, and who has held her seat at least a full window. Ordered least lane
/// grants first. Pure; the caller also skips anyone holding a live claim.
pub fn rotation_candidates(
    dormant_waiting: bool,
    freed_this_pass: usize,
    residents: &[(uuid::Uuid, MindHour)],
    seated_since: impl Fn(&str) -> Option<u64>,
    now_ms: u64,
) -> Vec<(uuid::Uuid, MindHour)> {
    if !dormant_waiting || freed_this_pass > 0 {
        return Vec::new();
    }
    let window = HEALTH_WINDOW.as_millis() as u64;
    let mut out: Vec<(uuid::Uuid, MindHour)> = residents
        .iter()
        .filter(|(_, m)| m.acts == 0 && m.writes == 0)
        .filter(|(_, m)| seated_since(&m.agent_name).is_some_and(|at| now_ms.saturating_sub(at) >= window))
        .cloned()
        .collect();
    out.sort_by_key(|(_, m)| (m.lane_grants, m.verdicts));
    out
}

pub fn snapshot_minds_and_reset() -> Vec<(uuid::Uuid, MindHour)> {
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
    /// carried a sample in fewer than [`LANE_MIN_BUCKETS`] distinct two-minute buckets.
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
    /// How this hour's generations stopped, every class side by side.
    pub stops: StopCounts,
    /// Turn iterations this hour that never reached the model — a wait, not an act, and
    /// not a stop either. Its own term on the line (card cff534ba).
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
    /// The hold live work had on the engine at the tick, as serving published it (card
    /// a18452d6): a node held for a mind's training reads HELD with the work, not EMPTY.
    pub held_for: Option<continuum_engine::inference::llama_server::serving_hold::ServingHold>,
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
    /// Residents, no acts observed in the window. Observational only: zero acts names no
    /// cause (she may hold a card while inference, tools or context stall), and no
    /// benchmark round is "owed" an idle roster (no harness, Joel 2026-10-19).
    Idle { resident: u64 },
    /// Writes happen, but too few for the roster: fewer than one write per
    /// [`RESIDENTS_PER_WRITE_HOUR`] residents in the hour.
    Slow { writes: u64, resident: u64 },
    /// No residents at the tick. Not a health verdict — a legible absence: a
    /// lane-donor node under the one-roster rule reads EMPTY with lanes offered; a
    /// node that lost its roster reads EMPTY with the grants it made before it did.
    /// It never reads "healthy" (Intel Mac 2026-09-15 10:2xZ: "resident 0 · lanes 0
    /// · lane grants 41 — healthy" after five despawns).
    Empty { lanes: u64, lanes_granted: u64 },
    /// The engine was held for live work (a mind's training, say) and nothing acted: a
    /// deliberate hold, not a dead or empty node (card a18452d6; enrico 2026-10-27 read EMPTY
    /// twice while Delia's gene trained on its only engine).
    Held { resident: u64, hold: continuum_engine::inference::llama_server::serving_hold::ServingHold },
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
            Verdict::Held { .. } => "held",
        }
    }
}

/// A roster is starved when it exceeds this many minds per served lane — and the ACTIVE
/// roster is bounded to it (Joel, 2026-09-20, on 23 minds over 3 warm slots: "the nursery
/// is over-saturated", the bound "is the way to go"): about two minds per slot, one
/// turning and one prefilled and waiting. At ~4-minute turns that is a lane every ~8
/// minutes each; three was a lane every ~12, and the hour it produced (16 on 3, acts 5,
/// every lane ask deferred) was not useful. Minds past the edge rest — dormant, not
/// resident, not polling — and return on a change.
// derived-or-floor: the roster shape Joel set (one mind turning, one prefilled per slot, above),
// a policy on the count of minds per lane, not a stand-in for a measured rate.
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
    // A held engine explains an hour in which nothing acted; it outranks EMPTY and IDLE.
    if let Some(hold) = h.held_for.as_ref().filter(|_| h.resident == 0 || h.acts == 0) {
        return Verdict::Held { resident: h.resident, hold: hold.clone() };
    }
    if h.resident == 0 {
        return Verdict::Empty { lanes: h.lanes, lanes_granted: h.lanes_granted };
    }
    if h.acts == 0 {
        return Verdict::Idle { resident: h.resident };
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

/// What holds the engine, in the line's words: each job with its consumer and engine, or why
/// ownership is unknown.
fn held_by(by: &continuum_engine::inference::llama_server::serving_hold::HoldCause) -> String {
    use continuum_engine::inference::llama_server::serving_hold::HoldCause;
    match by {
        HoldCause::Resident { work } => format!(
            "the engine hosts live work: {}",
            work.iter()
                .map(|w| format!("job {} ({}) on engine pid {}", w.job, w.consumer, w.engine_pid))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        // No work is confirmed here: the residency record could not be read, so serving holds
        // rather than risk replacing an engine that may host some (Codex on #5543).
        HoldCause::Unknown { reason } => format!("the engine's ownership is unknown, so serving holds it ({reason})"),
    }
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
        Verdict::Idle { resident } => format!("IDLE: {resident} resident, no acts observed in this window"),
        Verdict::Empty { lanes, lanes_granted } => format!(
            "EMPTY: no residents — {lanes} lanes offered, {lanes_granted} grants this hour"
        ),
        Verdict::Held { resident, hold } => format!(
            "HELD since {}: {}; {resident} resident, no lanes until it is released",
            continuum_base::time::EpochMs::from_millis(hold.since_ms).rfc3339(),
            held_by(&hold.by)
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
        // M5, 2026-09-20); `stops` is how each generation that reached it ended, every
        // class apart (a cut inside thinking is not a landed turn); `prefix reuse` is how much of each
        // prompt the engine did not re-read (card c119ace7); `moves` is minds changing
        // seats and `oldest dormant turn` is a mind with no seat at all (card 10bba591).
        "[health] last {} min: resident {} · lanes {} @ {}k · acts {} · lane-starved {} · stops: landed {} / empty {} / think-ended {} / cut-answer {} / cut-think {} / think-budget {} · dropped {} · writes {} · lane grants {} · settles {} · learning credits {} staged / {} settled{}{} · moves: {} on opportunity / {} on failure{} — {}",
        h.window_secs / 60,
        h.resident,
        lanes,
        h.served_window / 1000,
        h.acts,
        h.lane_starved,
        h.stops.landed,
        h.stops.empty,
        h.stops.think_ended,
        h.stops.cut_answer,
        h.stops.cut_think,
        h.stops.think_budget,
        dropped,
        h.writes,
        h.lanes_granted,
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
pub fn snapshot_lane_starved_and_reset() -> u64 {
    LEDGER.lane_starved.swap(0, Ordering::Relaxed)
}

/// The hour's generations and the ones dropped in flight, read and reset by the tick.
/// Its own reader for the reason [`snapshot_lane_starved_and_reset`] is: the main tuple
/// is already nine positional `u64`s and a tenth cannot be mis-positioned by a merge if
/// it never joins the tuple.
pub fn snapshot_generations_and_reset() -> (u64, u64) {
    (
        LEDGER.generations.swap(0, Ordering::Relaxed),
        LEDGER.generations_dropped.swap(0, Ordering::Relaxed),
    )
}

/// The hour's prompt-cache totals (cached, prefilled), read and reset by the tick.
/// Its own reader, not a tenth slot on [`snapshot_and_reset`]'s tuple: the two are read
/// at the same tick but they are different ledgers, and a tuple that long stops being
/// legible at the call site.
pub fn snapshot_prompt_and_reset() -> (u64, u64) {
    (
        LEDGER.prompt_cached.swap(0, Ordering::Relaxed),
        LEDGER.prompt_prefilled.swap(0, Ordering::Relaxed),
    )
}

pub fn snapshot_and_reset() -> (u64, u64, u64, u64, u64, u64) {
    (
        LEDGER.acts.swap(0, Ordering::Relaxed),
        LEDGER.writes.swap(0, Ordering::Relaxed),
        LEDGER.lanes_granted.swap(0, Ordering::Relaxed),
        LEDGER.settles.swap(0, Ordering::Relaxed),
        LEDGER.credits_staged.swap(0, Ordering::Relaxed),
        LEDGER.credits_settled.swap(0, Ordering::Relaxed),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a stop class counted under another, or not at all. Each class
    // lands in its own counter exactly once per note; the regression is Aris's three
    // cuts inside thinking (the M5, 2026-10-30), which the old branch counter never saw.
    #[test]
    fn every_stop_class_is_counted_once_in_its_own_counter() {
        let stops = StopCounters::new();
        for stop in [Stop::CutThink, Stop::CutThink, Stop::CutThink, Stop::Landed, Stop::Empty, Stop::ThinkEnded, Stop::CutAnswer, Stop::ThinkBudget] {
            stops.note(stop);
        }
        assert_eq!(
            stops.read(|c| c.load(Ordering::Relaxed)),
            StopCounts { landed: 1, empty: 1, think_ended: 1, cut_answer: 1, cut_think: 3, think_budget: 1 }
        );
    }

    // what this catches: the tick's read-and-reset losing or keeping counts. A read
    // without reset leaves the hour intact; the tick's swap returns the hour and starts
    // the next one at zero.
    #[test]
    fn a_snapshot_without_reset_keeps_the_hour_and_the_tick_starts_the_next_at_zero() {
        let stops = StopCounters::new();
        stops.note(Stop::CutThink);
        let peek = stops.read(|c| c.load(Ordering::Relaxed));
        assert_eq!(peek.cut_think, 1);
        assert_eq!(stops.read(|c| c.swap(0, Ordering::Relaxed)).cut_think, 1);
        assert_eq!(stops.read(|c| c.load(Ordering::Relaxed)), StopCounts::default());
    }
}
