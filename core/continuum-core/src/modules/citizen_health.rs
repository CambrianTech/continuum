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
};
/// A card pull was decided (the `bench.round.pull_*` seams). `deferred` = the lanes
/// were full, so she watched the board instead — the lane-bound signal.
pub fn note_pull(deferred: bool) {
    LEDGER.pulls.fetch_add(1, Ordering::Relaxed);
    if deferred {
        LEDGER.pulls_deferred.fetch_add(1, Ordering::Relaxed);
    }
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
/// The reason every lane-bound rest carries — a routing defect that pages a roster must
/// say THIS, never "mindless", so the record explains the empty seats.
pub const LANE_BOUND_REASON: &str = "lane_bound";
pub fn is_lane_bound(h: &CitizenHealth, v: &Verdict) -> bool {
    matches!(v, Verdict::Starved { .. } | Verdict::AtKnee { .. })
        && h.pulls >= LANE_BOUND_MIN_PULLS
        && (h.pulls_deferred as f64) >= LANE_BOUND_DEFERRED_SHARE * (h.pulls as f64)
}
/// The pure choice: which seats REST this tick so the roster comes down to the healthy
/// edge (`lanes × MINDS_PER_LANE_STARVED_ABOVE` — the one number the STARVED verdict
/// already turns on). Least served first (no writes, then fewest lane grants), never
/// below the resident floor, never more than the per-tick cap, and NOTHING on an hour
/// whose pulls found no rounds — that is a round-supply defect, not a seat one.
pub fn lane_bound_seats(h: &CitizenHealth, v: &Verdict, minds: &[(uuid::Uuid, MindHour)]) -> Vec<(uuid::Uuid, MindHour)> {
    if !is_lane_bound(h, v) {
        return Vec::new();
    }
    let keep = h.lanes.saturating_mul(MINDS_PER_LANE_STARVED_ABOVE).max(MINDLESS_RESIDENT_FLOOR);
    let over = h.resident.saturating_sub(keep) as usize;
    let mut out: Vec<(uuid::Uuid, MindHour)> = minds.to_vec();
    out.sort_by_key(|(_, m)| (m.writes, m.lane_grants));
    out.truncate(over.min(MINDLESS_MAX_PER_TICK));
    out
}
/// THE MIRROR (Cormac's condition on S3): a lane-bound rest has a lane-bound WAKE. A
/// mindless seat returns on a CHANGE in her; a lane-bound seat was fine — the LANES were
/// short — so she returns when the lanes come back: while `lanes × MINDS_PER_LANE_STARVED_ABOVE`
/// has room above the residents, the most recently rested lane-bound seat (the most served
/// of those rested, since the least served rested first) wakes, same cap per tick. Without
/// this every transient lane dip would permanently shrink the roster — the one-direction
/// shape (the peak ratchet, the claim expiry, the metronome) in a fourth coat. Mindless
/// rests are untouched. Pure.
pub const LANES_RETURNED_REASON: &str = "lanes returned";
pub fn lane_bound_wakes(h: &CitizenHealth, resting: &[crate::persona::resting_seat::RestingSeat]) -> Vec<crate::persona::resting_seat::RestingSeat> {
    let edge = h.lanes.saturating_mul(MINDS_PER_LANE_STARVED_ABOVE);
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
    out.truncate(room.min(MINDLESS_MAX_PER_TICK));
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
    pub lanes: u64,
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
    /// The measured decode knee for the served model (`inference::decode_knee`), when
    /// one is known: the lane count the planner will not exceed because every further
    /// stream would decode below the tax floor. Above it, lanes are not what is owed.
    pub knee: Option<u64>,
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
    /// Acts without writes: reading and re-orienting, never delivering.
    Reading { acts: u64 },
    /// Residents, no acts at all.
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
}

impl Verdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            Verdict::Healthy => "healthy",
            Verdict::Starved { .. } => "starved",
            Verdict::AtKnee { .. } => "at_knee",
            Verdict::Reading { .. } => "reading",
            Verdict::Idle { .. } => "idle",
            Verdict::Slow { .. } => "slow",
            Verdict::Empty { .. } => "empty",
        }
    }
}

/// A roster is starved when it exceeds this many minds per served lane: at ~4-minute
/// turns, three minds per lane is a lane every ~12 minutes each — the edge of useful.
pub const MINDS_PER_LANE_STARVED_ABOVE: u64 = 3;

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
        return Verdict::Idle { resident: h.resident };
    }
    if h.lanes > 0 && h.resident > h.lanes * MINDS_PER_LANE_STARVED_ABOVE {
        return match h.knee {
            Some(knee) if h.lanes >= knee => Verdict::AtKnee { resident: h.resident, lanes: h.lanes, knee },
            _ => Verdict::Starved { resident: h.resident, lanes: h.lanes },
        };
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
        Verdict::Reading { acts } => {
            format!("READING: {acts} acts, no writes — the progress note / governor owes a delivery")
        }
        Verdict::Idle { resident } => format!("IDLE: {resident} resident, no acts"),
        Verdict::Empty { lanes, lanes_granted } => format!(
            "EMPTY: no residents — {lanes} lanes offered, {lanes_granted} grants this hour"
        ),
        Verdict::Slow { writes, resident } => format!(
            "SLOW: {writes} writes for {resident} residents — below one write per {RESIDENTS_PER_WRITE_HOUR} minds an hour"
        ),
    };
    format!(
        "[health] last {} min: resident {} · lanes {} @ {}k · acts {} · writes {} · lane grants {} · pulls {} ({} lane-deferred) · settles {} · learning credits {} staged / {} settled — {}",
        h.window_secs / 60,
        h.resident,
        h.lanes,
        h.served_window / 1000,
        h.acts,
        h.writes,
        h.lanes_granted,
        h.pulls,
        h.pulls_deferred,
        h.settles,
        h.credits_staged,
        h.credits_settled,
        tail
    )
}

fn snapshot_and_reset() -> (u64, u64, u64, u64, u64, u64, u64, u64) {
    (
        LEDGER.acts.swap(0, Ordering::Relaxed),
        LEDGER.writes.swap(0, Ordering::Relaxed),
        LEDGER.lanes_granted.swap(0, Ordering::Relaxed),
        LEDGER.settles.swap(0, Ordering::Relaxed),
        LEDGER.credits_staged.swap(0, Ordering::Relaxed),
        LEDGER.credits_settled.swap(0, Ordering::Relaxed),
        LEDGER.pulls.swap(0, Ordering::Relaxed),
        LEDGER.pulls_deferred.swap(0, Ordering::Relaxed),
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
        let (acts, writes, lanes_granted, settles, credits_staged, credits_settled, pulls, pulls_deferred) = snapshot_and_reset();
        let resident = crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()
            .map(|r| r.live_personas().len() as u64)
            .unwrap_or(0); // JUSTIFIED unwrap_or: no registry = no residents, and the verdict says so
        let serving = crate::inference::llama_server::current_serving();
        CitizenHealth {
            window_secs: HEALTH_WINDOW.as_secs(),
            resident,
            lanes: serving.lanes as u64,
            served_window: serving.served_context_window as u64,
            acts,
            writes,
            lanes_granted,
            settles,
            credits_staged,
            credits_settled,
            pulls,
            pulls_deferred,
            knee: knee_of(serving.active_model.as_deref()),
        }
    }
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
            settles = h.settles,
            credits_staged = h.credits_staged,
            credits_settled = h.credits_settled,
            mindless_paged_out = paged_out.len() as u64,
            lane_bound_rested = rested.len() as u64,
            lane_bound_woken = woken.len() as u64,
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
                let h = CitizenHealth {
                    window_secs: HEALTH_WINDOW.as_secs(),
                    resident: crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()
                        .map(|r| r.live_personas().len() as u64)
                        .unwrap_or(0), // JUSTIFIED unwrap_or: no registry = no residents
                    lanes: crate::inference::llama_server::current_serving().lanes as u64,
                    served_window: crate::inference::llama_server::current_serving().served_context_window as u64,
                    acts: LEDGER.acts.load(Ordering::Relaxed),
                    writes: LEDGER.writes.load(Ordering::Relaxed),
                    lanes_granted: LEDGER.lanes_granted.load(Ordering::Relaxed),
                    settles: LEDGER.settles.load(Ordering::Relaxed),
                    credits_staged: LEDGER.credits_staged.load(Ordering::Relaxed),
                    credits_settled: LEDGER.credits_settled.load(Ordering::Relaxed),
                    pulls: LEDGER.pulls.load(Ordering::Relaxed),
                    pulls_deferred: LEDGER.pulls_deferred.load(Ordering::Relaxed),
                    knee: knee_of(crate::inference::llama_server::current_serving().active_model.as_deref()),
                };
                let v = verdict(&h);
                CommandResult::json(&serde_json::json!({
                    "resident": h.resident, "lanes": h.lanes, "served_window": h.served_window,
                    "acts": h.acts, "writes": h.writes, "lane_grants": h.lanes_granted, "settles": h.settles,
                    "credits_staged": h.credits_staged, "credits_settled": h.credits_settled,
                    "pulls": h.pulls, "pulls_deferred": h.pulls_deferred,
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
            description: "The hour's citizen health as the substrate reads it: residents, lanes, acts, writes, lane grants, settles, and the verdict (healthy / starved / reading / idle)",
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
        CitizenHealth { window_secs: 3600, resident, lanes, served_window: 66_000, acts, writes, lanes_granted: acts, settles: 0, credits_staged: 0, credits_settled: 0, pulls: 0, pulls_deferred: 0, knee: None }
    }

    // what this catches: the four shapes of 2026-09-14 named by the rule — 16 on 3 is
    // STARVED before it is anything else; acts without writes is READING; a roster
    // with no acts is IDLE; and a roster on enough lanes that writes is HEALTHY.
    #[test]
    fn the_verdict_names_the_afternoons_shapes() {
        assert_eq!(verdict(&h(16, 3, 39, 4)), Verdict::Starved { resident: 16, lanes: 3 });
        assert_eq!(verdict(&h(16, 6, 39, 0)), Verdict::Reading { acts: 39 });
        assert_eq!(verdict(&h(16, 6, 0, 0)), Verdict::Idle { resident: 16 });
        assert_eq!(verdict(&h(16, 6, 39, 4)), Verdict::Healthy);
        // The first receipt the core ever posted (00:01Z 2026-09-15): 16 residents,
        // 6 lanes, 37 acts, 2 writes — it said "healthy". It is SLOW.
        assert_eq!(verdict(&h(16, 6, 37, 2)), Verdict::Slow { writes: 2, resident: 16 });
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

    // what this catches: the line carries every number and ends with the verdict, so a
    // reader of the org room acts without a probe query.
    #[test]
    fn the_line_carries_the_numbers_and_ends_with_the_verdict() {
        let x = h(16, 3, 39, 4);
        let l = line(&x, &verdict(&x));
        for needle in ["resident 16", "lanes 3", "acts 39", "writes 4", "STARVED"] {
            assert!(l.contains(needle), "{needle} missing from {l}");
        }
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
        let (a, w, l, s, c, _, p, pd) = snapshot_and_reset();
        assert!(a >= 2 && w >= 1 && l >= 1 && s >= 1 && c >= 1 && p >= 1 && pd >= 1);
        let (a2, _, _, _, _, _, p2, _) = snapshot_and_reset();
        assert_eq!((a2, p2), (0, 0));
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
        assert_eq!(names, ["least", "less", "some"], "least served first, capped at {MINDLESS_MAX_PER_TICK}; a mind that wrote rests last");
        // Toward the edge, never past it: 7 minds on 2 lanes is one over the edge of 6.
        let seven = CitizenHealth { resident: 7, ..m5.clone() };
        assert_eq!(lane_bound_seats(&seven, &verdict(&seven), &roster).len(), 1);
        // Starved below the knee pays the same debt.
        let starved = CitizenHealth { knee: Some(8), ..m5.clone() };
        assert_eq!(verdict(&starved), Verdict::Starved { resident: 16, lanes: 2 });
        assert_eq!(lane_bound_seats(&starved, &verdict(&starved), &roster).len(), MINDLESS_MAX_PER_TICK);
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
        // The edge on one lane is 3: 3 minds on 1 lane is AT the edge, nobody rests.
        let one_lane = CitizenHealth { resident: 3, lanes: 1, pulls: 50, pulls_deferred: 50, ..m5.clone() };
        assert!(lane_bound_seats(&one_lane, &verdict(&one_lane), &roster).is_empty(), "3 on 1 is the edge, nobody rests");
        assert!(line(&m5, &v).contains("pulls 430 (424 lane-deferred)"), "the line carries the pulls");
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
        // Still 1 lane, 3 resident: the edge is 3, no room — nobody wakes.
        let one = CitizenHealth { pulls: 50, pulls_deferred: 50, ..h(3, 1, 10, 0) };
        assert!(lane_bound_wakes(&one, &resting).is_empty(), "lanes did not return");
        // 2 lanes: the edge is 6, room for 3 — both lane-bound seats wake, most served first.
        let two = CitizenHealth { ..h(3, 2, 10, 0) };
        let woken: Vec<String> = lane_bound_wakes(&two, &resting).into_iter().map(|s| s.agent_name).collect();
        assert_eq!(woken, ["less", "least"], "the last to rest is the first back; the recital stays rested");
        // Room for exactly one: only the most served returns.
        let tight = CitizenHealth { ..h(5, 2, 10, 0) };
        let woken: Vec<String> = lane_bound_wakes(&tight, &resting).into_iter().map(|s| s.agent_name).collect();
        assert_eq!(woken, ["less"]);
        // Rest and wake never both fire: over the edge there is no room; under it, nothing rests.
        let over = CitizenHealth { pulls: 50, pulls_deferred: 50, knee: Some(2), ..h(16, 2, 50, 3) };
        assert!(lane_bound_wakes(&over, &resting).is_empty());
    }
}
