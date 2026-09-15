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
}

static LEDGER: Ledger = Ledger {
    acts: AtomicU64::new(0),
    writes: AtomicU64::new(0),
    lanes_granted: AtomicU64::new(0),
    settles: AtomicU64::new(0),
    credits_staged: AtomicU64::new(0),
    credits_settled: AtomicU64::new(0),
};

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
/// Per-mind lane grants this hour — the placement chooser's "least served" order.
static GRANTS_BY_MIND: std::sync::LazyLock<dashmap::DashMap<uuid::Uuid, u64>> = std::sync::LazyLock::new(dashmap::DashMap::new);
pub fn note_lane_granted_to(persona: uuid::Uuid) {
    note_lane_granted();
    *GRANTS_BY_MIND.entry(persona).or_insert(0) += 1;
}
pub fn lane_grants_of(persona: uuid::Uuid) -> u64 {
    GRANTS_BY_MIND.get(&persona).map(|v| *v).unwrap_or(0) // JUSTIFIED unwrap_or: never granted this hour = 0, the truth
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Healthy,
    /// More minds than the lanes can turn in the window.
    Starved { resident: u64, lanes: u64 },
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
        return Verdict::Starved { resident: h.resident, lanes: h.lanes };
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
        "[health] last {} min: resident {} · lanes {} @ {}k · acts {} · writes {} · lane grants {} · settles {} · learning credits {} staged / {} settled — {}",
        h.window_secs / 60,
        h.resident,
        h.lanes,
        h.served_window / 1000,
        h.acts,
        h.writes,
        h.lanes_granted,
        h.settles,
        h.credits_staged,
        h.credits_settled,
        tail
    )
}

fn snapshot_and_reset() -> (u64, u64, u64, u64, u64, u64) {
    GRANTS_BY_MIND.clear();
    (
        LEDGER.acts.swap(0, Ordering::Relaxed),
        LEDGER.writes.swap(0, Ordering::Relaxed),
        LEDGER.lanes_granted.swap(0, Ordering::Relaxed),
        LEDGER.settles.swap(0, Ordering::Relaxed),
        LEDGER.credits_staged.swap(0, Ordering::Relaxed),
        LEDGER.credits_settled.swap(0, Ordering::Relaxed),
    )
}

/// Page out this hour's mindless seats: flush every resident's working memory (her
/// checkpoint), take each chosen seat off the grid (the same orderly teardown as
/// `persona/instances/despawn`), and record the rest so the reconciler does not
/// re-draw her. Returns the names paged out, for the line.
async fn page_out_mindless(h: &CitizenHealth) -> Vec<String> {
    let minds = snapshot_minds_and_reset();
    let chosen = mindless_seats(&minds, h.resident);
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
        let reason = format!(
            "{} of {} speak verdicts this hour were the gate refusing a recital or an envelope; {} acts, 0 writes",
            m.gate_refused, m.verdicts, m.acts
        );
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
        crate::probe!(
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
        );
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
        let (acts, writes, lanes_granted, settles, credits_staged, credits_settled) = snapshot_and_reset();
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
        }
    }
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
        let paged_out = page_out_mindless(&h).await;
        crate::probe!(
            class = "citizen.health.hour",
            resident = h.resident,
            lanes = h.lanes,
            served_window = h.served_window,
            acts = h.acts,
            writes = h.writes,
            lane_grants = h.lanes_granted,
            settles = h.settles,
            credits_staged = h.credits_staged,
            credits_settled = h.credits_settled,
            mindless_paged_out = paged_out.len() as u64,
            verdict = v.as_str(),
            "the hour's citizen health — the substrate's own read"
        );
        let mut said = line(&h, &v);
        if !paged_out.is_empty() {
            said.push_str(&format!(" · mindless {} paged out ({})", paged_out.len(), paged_out.join(", ")));
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
                };
                let v = verdict(&h);
                CommandResult::json(&serde_json::json!({
                    "resident": h.resident, "lanes": h.lanes, "served_window": h.served_window,
                    "acts": h.acts, "writes": h.writes, "lane_grants": h.lanes_granted, "settles": h.settles,
                    "credits_staged": h.credits_staged, "credits_settled": h.credits_settled,
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
            agent_name: name.into(), verdicts, gate_refused: refused, acts, writes,
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
        CitizenHealth { window_secs: 3600, resident, lanes, served_window: 66_000, acts, writes, lanes_granted: acts, settles: 0, credits_staged: 0, credits_settled: 0 }
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
        let (a, w, l, s, c, _) = snapshot_and_reset();
        assert!(a >= 2 && w >= 1 && l >= 1 && s >= 1 && c >= 1);
        let (a2, _, _, _, _, _) = snapshot_and_reset();
        assert_eq!(a2, 0);
    }
}
