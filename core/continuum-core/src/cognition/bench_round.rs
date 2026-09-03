//! `BenchRound` — the round LIFECYCLE spine (#371): a dispatched benchmark round is an
//! ENTITY with a stage, not a pile of cards whose fate is probe archaeology.
//!
//! Before this, `benchmark/dispatch` posted cards + kickoffs and then NOTHING owned the
//! round: no stage announcements, no END — "random and directed by agent, not an
//! ecosystem" (Joel 2026-08-16). This module is the smallest true fix: dispatch registers
//! the round (its id IS the run room's id — dispatch already mints one per run, #329a;
//! never a second id), and the EXISTING `work.card.state_changed` bus event settles cards
//! one by one until the round is Done.
//!
//! Event-driven, never polling ([[the-whole-system-is-event-based-not-polling]]): there is
//! no tick here and no new bus receiver either — the one live consumer of
//! `WORK_CARD_STATE_CHANGED` (`modules::benchmark_grade`'s receiver task, the #450
//! subscriber) forwards every payload to [`observe_card_event`]. One subscription, two
//! reactions (grade the card, advance the round).
//!
//! Probes fire at TRANSITIONS ONLY:
//! - `bench.round.staged`   — at dispatch: round id, benchmark, card count.
//! - `bench.round.card_settled` — a card in the round reached a terminal state.
//! - `bench.round.done`     — the END, exactly once: every card settled.
//!
//! ## The round also owns WHO DRIVES its work ([`WorkDriver`])
//!
//! A benchmark card can be worked two ways, and the difference decides whether the round
//! teaches anybody anything. The round is the only thing that knows which, because the
//! decision is made once at dispatch and read later at claim time — in a different verb,
//! on a different task, with nothing threaded between them.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

use serde_json::Value;
use uuid::Uuid;

/// Who actually does the work on this round's cards.
///
/// - [`DetachedSolve`](WorkDriver::DetachedSolve) — a `work/claim` (or a directed
///   dispatch) fires `agent/solve` for a FORKED copy of the citizen. Proven: it reaches
///   the repo, it grades, it produced our one SWE pass (#366). It also produces no room
///   turn, so `training_producer::produce` never runs and the round teaches nobody
///   (#456: the L2 producer is PATH-gated, and this is the path it excludes).
/// - [`Citizen`](WorkDriver::Citizen) — nothing detached fires. She claims the card in
///   her own service loop and works it on the held-work turn, which roots her hands at
///   the staged checkout and feeds the training producer. This is the path the learning
///   half of the objective requires — and the one that has never once been observed
///   end to end, because on the default path the detached solve always wins the claim.
///
/// `Citizen` is the default (flipped 2026-08-31): a round teaches by default, and an
/// operator opts INTO the detached diagnostic per round. Both are real drivers, not a
/// flag and a fallback — which is why the choice is named on the round rather than
/// hidden behind a `skip_solve` bool.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
    ts_rs::TS,
)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/WorkDriver.ts")]
pub enum WorkDriver {
    /// A detached fork works the card outside any citizen's cognition —
    /// produces no room turns, feeds no curriculum, breathes no meters
    /// (BENCHMARKS-ARE-ADAPTERS: "maximum effort, zero learning"). An
    /// explicit diagnostic mode, NEVER the default.
    DetachedSolve,
    /// The room is the runner: a citizen's own cognition works the card,
    /// acts radiate, turns feed the flywheel. THE default — benchmarks use
    /// our citizens, never disposable solvers (Joel's law; flipped
    /// 2026-08-31 after four detached rounds ran invisible all night).
    #[default]
    Citizen,
}

/// Where a round is in its life. `Working` from the moment dispatch returns (cards are
/// posted and kickoffs sent); `Done` when every card in the round's set has reached a
/// terminal card state. Two stages only — the smallest true lifecycle; claim/review
/// granularity already lives on the cards themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundStage {
    Working,
    /// Operator-held: driver edges fire nothing, boot-resume re-parks nothing,
    /// in-flight solves finish and settle normally. `benchmark/resume` lifts it.
    Paused,
    Done,
}

/// Terminal card states — "this card's work is finished". The vocabulary is the serde
/// snake_case form of `airc_lib::CardState` as carried by the `work.card.state_changed`
/// payload: `work/state` maps done|closed → Closed and accepts merged. ONE truth for
/// "terminal": `modules::benchmark_grade::is_terminal` delegates here (compression
/// principle — the grader and the round tracker must never disagree about doneness).
pub(crate) fn is_terminal_card_state(state: &str) -> bool {
    matches!(
        state.to_ascii_lowercase().as_str(),
        "closed" | "done" | "merged"
    )
}

/// What one observed card-settle did to a round. Returned by [`BenchRound::settle_card`]
/// so the probe emission (and the tests) read the transition off the type instead of
/// re-deriving it from counters.
#[derive(Debug, PartialEq, Eq)]
enum SettleOutcome {
    /// The card is not in this round's set — not ours, ignore.
    NotOurs,
    /// The card was already settled (duplicate terminal event) — no state change.
    AlreadySettled,
    /// The card settled; `remaining` cards are still working.
    Settled { remaining: usize },
    /// This settle was the LAST one — the round just transitioned Working → Done.
    /// Fires exactly once per round by construction (the transition consumes it).
    RoundDone,
}

/// One benchmark round: the card set a single `benchmark/dispatch` posted, tracked from
/// dispatch (Working) to all-cards-terminal (Done). Identity is the run ROOM's uuid —
/// the id dispatch already mints per run; a round IS its room's activity.
///
/// Serde is DURABILITY, not wire: an in-flight round persists to
/// [`rounds_state_dir`] on every mutation and reloads on the next boot. Until
/// 2026-08-21 this map was process-memory only, and the cost was measured twice in
/// one day: `benchmark/rounds` answered `rounds: []` while a staged round was live
/// (the operator re-derived the round from probe archaeology), and — worse, silent —
/// a reboot mid-round made [`driver_for_card`] fall back to `DetachedSolve` for a
/// `Citizen` round's remaining cards, quietly rebuilding the parallel runner the
/// round was configured to avoid. A round must outlive the process that opened it:
/// "kicked off by a command, owned by events" only holds if the owner survives.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct BenchRound {
    /// The RUN room's airc NAME (`round_id` IS the run room's UUID; joins are
    /// by name). `default` folds pre-field files as empty — such a round's run
    /// room stays presence-dark until a fresh dispatch names it.
    #[serde(default)]
    pub run_room_name: String,
    round_id: Uuid,
    benchmark: String,
    /// Card uuid → the terminal state it settled with (`None` = still working).
    cards: HashMap<Uuid, Option<String>>,
    stage: RoundStage,
    /// Who works this round's cards — read at CLAIM time, decided at dispatch.
    driver: WorkDriver,
    /// Card uuid → the ACTIVITY its solve runs in, recorded at first mint so a
    /// re-fire (a resume after a reboot, a retry attempt) REJOINS the same room
    /// instead of minting a stranger — the "unless rejoining" half of the law
    /// (Joel 2026-08-26: "benchmarks without new activities (unless rejoining)").
    /// `#[serde(default)]` so pre-existing round files load with no activities
    /// recorded and mint on their next dispatch.
    #[serde(default)]
    card_activities: HashMap<Uuid, CardActivity>,
    /// The ROUND's default team (resolved at dispatch): reviewers who join
    /// every card's solve room. Cards fired by a driver edge BEFORE their
    /// first dispatch have no CardActivity yet — without this, the edge
    /// dispatched them team-less and then RECORDED the empty team (observed
    /// 2026-08-30: card 62f5aee5 fired to Kira with no reviewers, silently
    /// converting a team round to solo — the review event could never fire).
    #[serde(default)]
    team: Vec<Uuid>,
    /// Card uuid → the citizen it was staged FOR, recorded at dispatch staging
    /// (before any solve fires) so the follow-on driver ([`next_unworked_after`])
    /// and the boot resume know WHO works a card that has never run.
    #[serde(default)]
    card_assignees: HashMap<Uuid, Uuid>,
    /// Card uuid → the INSTANCE it was staged for, recorded at dispatch
    /// staging. Before this the tracker only learned WHAT a card tested when
    /// its solve activity minted a room name — so an unstarted card's
    /// roll-call row was blank, and an in-flight run could not join back to
    /// its card (2026-09-01: Kira's live django-14349 run left its round
    /// pronounced `unstarted`).
    #[serde(default)]
    card_instances: HashMap<Uuid, String>,
    /// Card uuid → epoch-ms of the LATEST held-work turn a citizen drove on it. The
    /// freshness signal a CITIZEN-driven card emits, which the detached-solve run
    /// ledger never sees. DURABLE (save-on-write with the round) since 2026-09-03:
    /// it used to be a process-global map that a reboot emptied, so every citizen
    /// round read "unstarted" after a restart and the standing autopilot minted a
    /// duplicate round per tick (30 duplicate verified-mini rounds measured). Resume
    /// is read-the-saved-state, never re-derive.
    #[serde(default)]
    card_last_act_ms: HashMap<Uuid, u64>,
}

/// Where one card's solve LIVES: its per-instance activity room, the citizen
/// working it, and — for TEAM solves (#team-proof gap 1) — the teammates joined
/// to that room. Typed UUIDs, passed as a struct (Joel: "pass structs").
/// Membership is ROOM-level (teammates join and participate through normal
/// multi-room cognition); accountability stays CARD-level (one claimer, one
/// terminal transition) — no multi-claim machinery.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CardActivity {
    pub solve_room: Uuid,
    /// The solve room's airc NAME (`swe--<instance>--<card8>`). Joins are
    /// by name (never UUID-as-string — the derived-channel hazard), so any
    /// consumer that must SUBSCRIBE to activity rooms (the presence
    /// emitter, #2606) reads it from here. `default` folds old files as
    /// empty — such a room stays presence-dark until its next dispatch.
    #[serde(default)]
    pub room_name: String,
    pub assignee: Uuid,
    /// Room-members beyond the claimer. Recorded so a resume re-invites the
    /// SAME team (continuity), and so experience records can attribute team
    /// outcomes (the protocol's gap 3 reads this).
    #[serde(default)]
    pub teammates: Vec<Uuid>,
}

impl BenchRound {
    pub fn new(round_id: Uuid, benchmark: &str, card_ids: &[Uuid], driver: WorkDriver) -> Self {
        Self {
            round_id,
            run_room_name: String::new(),
            benchmark: benchmark.to_string(),
            cards: card_ids.iter().map(|c| (*c, None)).collect(),
            stage: RoundStage::Working,
            driver,
            card_activities: HashMap::new(),
            card_assignees: HashMap::new(),
            card_instances: HashMap::new(),
            card_last_act_ms: HashMap::new(),
            team: Vec::new(),
        }
    }

    pub fn stage(&self) -> RoundStage {
        self.stage
    }

    /// Cards not yet settled.
    pub fn remaining(&self) -> usize {
        self.cards.values().filter(|s| s.is_none()).count()
    }

    /// Total cards this round dispatched.
    pub fn dispatched(&self) -> usize {
        self.cards.len()
    }

    /// Record that `card` reached terminal `state`. Pure state machine — the caller
    /// ([`observe_card_event`]) turns the outcome into probes. A card outside the set is
    /// `NotOurs`; a duplicate settle is `AlreadySettled` (never double-counted); the
    /// settle that empties the set transitions the round to `Done` exactly once.
    fn settle_card(&mut self, card: Uuid, state: &str) -> SettleOutcome {
        match self.cards.get_mut(&card) {
            None => SettleOutcome::NotOurs,
            Some(Some(_)) => SettleOutcome::AlreadySettled,
            Some(slot) => {
                *slot = Some(state.to_string());
                let remaining = self.remaining();
                if remaining == 0 && self.stage == RoundStage::Working {
                    self.stage = RoundStage::Done;
                    SettleOutcome::RoundDone
                } else {
                    SettleOutcome::Settled { remaining }
                }
            }
        }
    }
}

/// The live rounds this core is tracking, keyed by round (= run room) id. Process-global
/// for the same reason `modules::work::WORK_EVENT_BUS` is: the bus and the dispatch verb
/// are process-global, and the event consumer holds no path to a threaded-through field.
/// Sync `Mutex`, never held across an await (every touch is a short pure mutation).
/// A round is REMOVED the moment it completes, so the map only ever holds in-flight
/// rounds — no unbounded growth, and "done fires once" is structural.
///
/// First touch RELOADS persisted in-flight rounds (see [`BenchRound`]'s serde note) —
/// lazy so no boot hook is needed: the first `driver_for_card` after a reboot already
/// answers from the reloaded round.
static ROUNDS: LazyLock<Mutex<HashMap<Uuid, BenchRound>>> =
    LazyLock::new(|| Mutex::new(load_rounds_in(&rounds_state_dir())));

/// Stamp that a citizen drove a held-work turn on `card_id` at `now_ms` — called
/// from the held-work turn boundary so `enrich_rounds` can see Citizen progress.
/// Persisted with the round: freshness survives the seam. A card outside any live
/// round records nothing (there is no round to read it back for).
pub fn record_card_worked(card_id: Uuid, now_ms: u64) {
    let mut rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(round) = rounds.values_mut().find(|r| r.cards.contains_key(&card_id)) {
        round.card_last_act_ms.insert(card_id, now_ms);
        persist_round_in(&rounds_state_dir(), round);
    }
}

/// The epoch-ms of the latest held-work turn on `card_id`, if any — the Citizen
/// freshness [`enrich_rounds`] merges beside the detached run ledger.
pub fn card_last_act_ms(card_id: Uuid) -> Option<u64> {
    ROUNDS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .values()
        .find_map(|r| r.card_last_act_ms.get(&card_id).copied())
}

/// Where in-flight rounds persist — one JSON file per round, removed at Done. The same
/// durable-state family as the airc attach cursor (`~/.continuum/state`): tiny, per-key,
/// self-evicting at terminal state, so the directory only ever holds in-flight rounds.
///
/// Under `cfg(test)` this is a per-process temp dir, NEVER the real state dir. The first
/// cut resolved to `$HOME` unconditionally, and the existing tests — which exercise the
/// PUBLIC `open_round`/`add_card` — persisted their fixture rounds into the operator's
/// real state: two `cargo test` runs minted four phantom rounds that the next live core
/// faithfully reloaded and reported as in-flight (measured 2026-08-21, `in_flight: 5`
/// with one real round). A durable layer's tests must be durable somewhere disposable.
fn rounds_state_dir() -> std::path::PathBuf {
    #[cfg(test)]
    {
        static TEST_DIR: LazyLock<std::path::PathBuf> = LazyLock::new(|| {
            std::env::temp_dir().join(format!("bench-rounds-test-proc-{}", std::process::id()))
        });
        TEST_DIR.clone()
    }
    #[cfg(not(test))]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        std::path::PathBuf::from(home).join(".continuum/state/bench-rounds")
    }
}

/// Persist one round. Failure degrades to the pre-2026-08-21 behaviour (the round
/// forgets on reboot) and WARNS — durability must never make a live dispatch fail.
fn persist_round_in(dir: &Path, round: &BenchRound) {
    let _ = std::fs::create_dir_all(dir);
    let path = dir.join(format!("{}.json", round.round_id));
    match serde_json::to_string(round) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                tracing::warn!(round = %round.round_id, error = %e, "bench round not persisted — it will not survive a core restart");
            }
        }
        Err(e) => tracing::warn!(round = %round.round_id, error = %e, "bench round not serializable — it will not survive a core restart"),
    }
}

/// Forget a completed round's file. Best-effort: a leftover file re-loads a Done round
/// at next boot, and [`load_rounds_in`] drops those on read.
fn remove_round_file_in(dir: &Path, round_id: Uuid) {
    let _ = std::fs::remove_file(dir.join(format!("{round_id}.json")));
}

/// Reload the in-flight rounds a previous core persisted. Unreadable or Done entries
/// are dropped (Done should have been removed at settle; tolerate the crash window).
///
/// HONEST LIMIT, by design: cards that reached a terminal state WHILE the core was
/// down settle here as still-working until the next real event touches the round. The
/// reconciler that re-derives card state from the board at boot is the follow-up —
/// this reload restores the round's EXISTENCE (driver, card set, visibility), which is
/// what a reboot was silently destroying.
fn load_rounds_in(dir: &Path) -> HashMap<Uuid, BenchRound> {
    let mut out = HashMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let Ok(round) = serde_json::from_str::<BenchRound>(&text) else {
            tracing::warn!(file = %entry.path().display(), "unreadable bench-round state file skipped (left in place for inspection)");
            continue;
        };
        if round.stage == RoundStage::Done {
            let _ = std::fs::remove_file(entry.path());
            continue;
        }
        crate::probe!(
            class = "bench.round.reloaded",
            round_id = %round.round_id,
            benchmark = %round.benchmark,
            remaining = round.remaining(),
            driver = ?round.driver,
            "in-flight benchmark round reloaded after core restart — driver and card set restored"
        );
        out.insert(round.round_id, round);
    }
    out
}

/// Boot reconciler — reap-or-ADOPT, and since plan A5 the answer is ADOPT
/// ([[boot-owns-the-process-tree-reap-or-adopt-never-fight-yourself]],
/// continuity-is-the-default). A `Working` round on disk is STILL WORKING: the
/// boot resume (`modules::benchmark_resume`) rejoins it and re-fires its next
/// unworked card through the ONE driver decision. The old behavior — deleting
/// every Working round at boot — destroyed exactly the state a resume needs
/// (room, cards, assignees, per-card activity rooms) and made "benchmarks never
/// start themselves" structural. What remains here is the eviction story this
/// state dir owes (2026-07-13 rule): a round file older than [`ROUND_TTL`] is
/// abandoned and expires. Returns the expired `(benchmark, remaining)` rows for
/// the boot probe. Still never re-dispatches from the serving daemon.
pub fn reap_orphaned_rounds() -> Vec<(String, usize)> {
    let dir = rounds_state_dir();
    // TTL expiry only (see [`reap_orphaned_rounds_in`]): a Working round at boot
    // SURVIVES — it is the durable state the boot resume (modules::benchmark_resume)
    // rejoins. Expired files are also dropped from the loaded map so live_rounds()
    // agrees this boot.
    let expired = reap_orphaned_rounds_in(&dir);
    if !expired.is_empty() {
        let dead: std::collections::HashSet<&String> =
            expired.iter().map(|(b, _)| b).collect();
        ROUNDS
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .retain(|_, r| r.stage != RoundStage::Working || !dead.contains(&r.benchmark));
    }
    expired
}

/// The disk half of [`reap_orphaned_rounds`], parameterized on the state dir so it is pure
/// and unit-testable. Removes only TTL-expired Working files; live Working rounds
/// persist across boots — continuity is the default, reset is the exception.
/// CONTINUITY IS THE DEFAULT (Joel's law; plan A5): a Working round at boot is
/// STILL WORKING — the reaper no longer deletes it (deleting destroyed exactly
/// the state a resume needs: room, cards, driver, assignees). Only a round file
/// older than [`ROUND_TTL`] is expired — the backstop against an abandoned
/// round haunting the resume forever, and the eviction story this state dir
/// owes (2026-07-13 rule).
const ROUND_TTL: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 3600);

fn reap_orphaned_rounds_in(dir: &Path) -> Vec<(String, usize)> {
    reap_with_ttl(dir, ROUND_TTL)
}

/// TTL injectable so the expiry contract is testable without clock games.
fn reap_with_ttl(dir: &Path, ttl: std::time::Duration) -> Vec<(String, usize)> {
    let mut expired = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return expired;
    };
    for entry in entries.flatten() {
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let Ok(round) = serde_json::from_str::<BenchRound>(&text) else {
            continue; // unreadable → left in place by load_rounds_in for inspection
        };
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age >= ttl);
        if round.stage == RoundStage::Working && stale {
            expired.push((round.benchmark.clone(), round.remaining()));
            let _ = std::fs::remove_file(entry.path());
        }
    }
    expired
}

/// Open a round BEFORE its first card is posted, so the driver is readable from the
/// instant a card can be claimed.
///
/// The ordering is load-bearing, not tidiness. `benchmark/dispatch` sends a kickoff
/// inside its card loop, so a citizen can claim card 1 while card 2 is still being
/// posted. If the round were only registered after the loop (as it was), that claim
/// would find no round, [`driver_for_card`] would answer with the default, and a
/// `Citizen`-driven round would silently fire the detached solver for its first card —
/// defeating exactly the thing the round was configured to do, with no error anywhere.
///
/// Idempotent by round id: re-opening a live round leaves it untouched.
pub fn open_round(round_id: Uuid, benchmark: &str, driver: WorkDriver) {
    let mut rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner());
    let round = rounds
        .entry(round_id)
        .or_insert_with(|| BenchRound::new(round_id, benchmark, &[], driver));
    persist_round_in(&rounds_state_dir(), round);
}

/// Record the round's default TEAM (idempotent; dispatch calls it right after
/// [`open_round`] with the resolved reviewer set). Driver edges fall back to
/// this when a card has no recorded activity yet — a team round stays a team
/// round on every edge.
/// Record the run room's airc name (dispatch calls it right after
/// [`open_round`]) — the presence emitter's adoption list reads it so the
/// RUN room, not just its solve children, projects presence + transcript.
pub fn set_run_room_name(round_id: Uuid, name: &str) {
    let mut rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(round) = rounds.get_mut(&round_id) {
        round.run_room_name = name.to_string();
        persist_round_in(&rounds_state_dir(), round);
    }
}

pub fn set_round_team(round_id: Uuid, team: Vec<Uuid>) {
    let mut rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(r) = rounds.get_mut(&round_id) {
        r.team = team;
        persist_round_in(&rounds_state_dir(), r);
    }
}

/// Add a freshly posted card to an open round. Unknown round = no-op (dispatch always
/// opens first; a card that arrives without one is not part of a tracked round).
pub fn add_card(round_id: Uuid, card_id: Uuid) {
    if let Some(r) = ROUNDS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get_mut(&round_id)
    {
        r.cards.entry(card_id).or_insert(None);
        persist_round_in(&rounds_state_dir(), r);
    }
}

/// Close the dispatch phase and announce the round (`bench.round.staged`). Called by
/// `benchmark/dispatch` after its card loop. A round that posted zero cards (everything
/// skipped / already on board) stages and immediately ends — an honest empty round,
/// never a map entry that no event can ever settle.
pub fn seal_round(round_id: Uuid) {
    let mut rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner());
    let Some(round) = rounds.get(&round_id) else {
        return;
    };
    let (benchmark, dispatched, driver) =
        (round.benchmark.clone(), round.dispatched(), round.driver);
    crate::probe!(
        class = "bench.round.staged",
        round_id = %round_id,
        benchmark = %benchmark,
        cards = dispatched,
        driver = ?driver,
        "benchmark round staged — cards posted, kickoffs sent, round is Working"
    );
    if dispatched == 0 {
        rounds.remove(&round_id);
        remove_round_file_in(&rounds_state_dir(), round_id);
        crate::probe!(
            class = "bench.round.done",
            round_id = %round_id,
            benchmark = %benchmark,
            dispatched = 0usize,
            settled = 0usize,
            "benchmark round END — nothing was dispatched"
        );
    }
}

/// Who drives the work for this card — the question `work/claim` asks before deciding
/// whether to fire a detached solve.
///
/// A card belonging to no live round answers [`WorkDriver::DetachedSolve`]: that covers
/// a human-claimed card, an undirected board card, and a leftover claimed after its
/// round ended. All three are the proven path, so the default is the conservative one.
/// The round ROOM a card belongs to — which IS the round id (a round is its
/// room's activity). `None` for a card outside every tracked round (human-created
/// boards, ended rounds). The FOCUS rule reads this: a citizen's self-tick binds
/// to the room of her freshest live claim, so she stops alternating rooms —
/// measured 2026-08-22: two live claims in two rooms swapped her pinned slot's
/// room-scoped context every tick, `cached: 0` by her own hand.
pub fn room_for_card(card_id: Uuid) -> Option<Uuid> {
    ROUNDS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .values()
        .find(|r| r.cards.contains_key(&card_id))
        .map(|r| r.round_id)
}

/// The recorded activity for `card_id`'s solve, if one was ever minted — the
/// REJOIN half of mint-or-rejoin. Scans in-flight rounds (same shape as
/// [`room_for_card`]).
pub fn card_activity(card_id: Uuid) -> Option<CardActivity> {
    let rounds = ROUNDS.lock().expect("bench rounds mutex");
    rounds
        .values()
        .find(|r| r.cards.contains_key(&card_id))
        .and_then(|r| r.card_activities.get(&card_id).cloned())
}

/// The team standing in a solve ROOM — the inverse of [`card_activity`], for
/// callers that hold an activity's room but not its card (the experience
/// stream's grade-time seam). `None` = no tracked round runs a solve there
/// (solo work, non-bench rooms).
/// The RUN room of the round whose solve runs in `room` — the tree edge the
/// nav rail renders (#2632): a solve activity nests under its round. `None`
/// = no tracked round solves there (top-level activity).
pub fn run_room_for_solve(room: Uuid) -> Option<Uuid> {
    let rounds = ROUNDS.lock().expect("bench rounds mutex");
    rounds
        .values()
        .find(|r| r.card_activities.values().any(|a| a.solve_room == room))
        .map(|r| r.round_id)
}

pub fn team_for_room(room: Uuid) -> Option<CardActivity> {
    let rounds = ROUNDS.lock().expect("bench rounds mutex");
    rounds
        .values()
        .flat_map(|r| r.card_activities.values())
        .find(|act| act.solve_room == room)
        .cloned()
}

/// Record (idempotently) the activity `card_id`'s solve runs in — called at the
/// MINT, persisted with the round so a post-reboot re-fire rejoins it.
pub fn record_card_activity(card_id: Uuid, activity: CardActivity) {
    let mut rounds = ROUNDS.lock().expect("bench rounds mutex");
    if let Some(round) = rounds.values_mut().find(|r| r.cards.contains_key(&card_id)) {
        round.card_activities.insert(card_id, activity);
        persist_round_in(&rounds_state_dir(), round);
    }
}

/// Record which citizen `card_id` was staged for — at DISPATCH time, so every
/// card has an owner before any solve fires.
pub fn record_card_assignee(card_id: Uuid, assignee: Uuid) {
    let mut rounds = ROUNDS.lock().expect("bench rounds mutex");
    if let Some(round) = rounds.values_mut().find(|r| r.cards.contains_key(&card_id)) {
        round.card_assignees.insert(card_id, assignee);
        persist_round_in(&rounds_state_dir(), round);
    }
}

/// WHAT one card tests, from the staged record or the activity room name —
/// so a re-say can name the instance the way the 2026-09-02 hand-written
/// operator note did (the note broke a wedged round; the substrate's own
/// kickoff must carry the same information or the hand stays in the loop).
pub fn instance_for_card(card_id: Uuid) -> Option<String> {
    let rounds = ROUNDS.lock().unwrap_or_else(|e| e.into_inner()); // safe: poisoned lock = read the last state, same policy as every ROUNDS lock here
    rounds.values().find_map(|r| {
        if !r.cards.contains_key(&card_id) {
            return None;
        }
        r.card_instances
            .get(&card_id)
            .cloned()
            .filter(|i| !i.is_empty())
            .or_else(|| {
                r.card_activities.get(&card_id).and_then(|a| {
                    match instance_of_room_name(&a.room_name) {
                        "" => None,
                        parsed => Some(parsed.to_string()),
                    }
                })
            })
    })
}

/// WHAT a card tests, recorded at dispatch staging (the dispatcher holds the
/// instance in hand there) — see the `card_instances` field for why waiting
/// until the solve activity mints is too late.
pub fn record_card_instance(card_id: Uuid, instance: &str) {
    let mut rounds = ROUNDS.lock().expect("bench rounds mutex");
    if let Some(round) = rounds.values_mut().find(|r| r.cards.contains_key(&card_id)) {
        round.card_instances.insert(card_id, instance.to_string());
        persist_round_in(&rounds_state_dir(), round);
    }
}

/// The next card the round owes a solve — passed as a struct (typed UUIDs).
#[derive(Debug, Clone)]
pub struct NextCard {
    pub card: Uuid,
    pub assignee: Uuid,
    pub run_room: Uuid,
    /// The card's recorded team (empty = solo). Carried so every driver edge
    /// re-fires the solve WITH its team — a re-dispatch that drops teammates
    /// silently converts a team round into a solo round mid-flight.
    pub teammates: Vec<Uuid>,
}

/// ONE driver decision, three edges (dispatch, card-settled, boot resume): given
/// a card of some round, the FIRST still-unsettled card with no live run and a
/// known assignee — or None when the round is fully in flight / done / not a
/// detached round (Citizen rounds drive themselves through claims). Pure over
/// the round state plus the in-flight ledger, so every edge fires the same
/// decision — never a second scheduler.
pub fn next_unworked_after(card_of_round: Uuid) -> Option<NextCard> {
    let live = live_run_ids();
    let rounds = ROUNDS.lock().expect("bench rounds mutex");
    let round = rounds
        .values()
        .find(|r| r.cards.contains_key(&card_of_round))?;
    first_unworked(round, &live)
}

/// The NON-SETTLING edge of the same driver decision — the fourth edge.
///
/// A run can finish WITHOUT its card reaching a terminal state: she produced no
/// diff, the env was absent, the cached env would not re-point. Those are honest
/// outcomes (the card stays open for a retake) but they emit no card-settled
/// event, so the settle edge never fires and the round would sit still until the
/// 5-minute becalmed watchdog noticed. Measured 2026-08-28: a non-settling run at
/// 01:58 and another at 03:30 each left the round idle behind one finished solve.
/// A round that advances only by TIMEOUT is polling wearing an actuator's coat;
/// it should move the instant the fact is known ([[the-whole-system-is-event-based-not-polling]]).
///
/// `just_finished` is EXCLUDED, and that exclusion is the loop guard: the card is
/// still unsettled and no longer live, so the unguarded decision would hand back
/// the very card that just failed and re-fire it forever. The watchdog remains the
/// backstop for everything this edge cannot see (a process that died without
/// running its close path at all).
pub fn next_unworked_excluding(just_finished: Uuid) -> Option<NextCard> {
    let live = live_run_ids();
    let rounds = ROUNDS.lock().expect("bench rounds mutex");
    let round = rounds
        .values()
        .find(|r| r.cards.contains_key(&just_finished))?;
    first_unworked_excluding(round, &live, Some(just_finished))
}

/// The boot-resume edge of the SAME decision: the first unworked card of EVERY
/// Working detached round. Called once at benchmark-module boot (after the
/// serving + residency parks); the card-settled edge then chains the rest.
pub fn next_unworked_per_round() -> Vec<NextCard> {
    let live = live_run_ids();
    let rounds = ROUNDS.lock().expect("bench rounds mutex");
    rounds
        .values()
        .filter(|r| r.stage == RoundStage::Working)
        .filter_map(|r| first_unworked(r, &live))
        .collect()
}


/// The next card a MEMBER can PULL from a shared team deck — kanban PULL, not
/// push. The first non-terminal card, with no live run, in any Working Citizen
/// round the peer belongs to (in its `team`, or already an assignee somewhere in
/// it), that is NOT already assigned to a DIFFERENT member (never poach a
/// teammate's card; Open or hers is fair game). `assignee` is the PULLER — she is
/// claiming it. Personifies a human team: when free, you take the next card off
/// the board rather than working a fixed pile pushed onto you (Joel 2026-09-02).
/// Load-balances (a fast member pulls more) and is resilient (a dropped member's
/// un-pulled cards stay in the deck for anyone).
pub fn pullable_cards(
    peer: Uuid,
    resident_rooms: &std::collections::HashSet<Uuid>,
) -> Vec<NextCard> {
    let rounds = ROUNDS.lock().expect("bench rounds mutex");
    let live = live_run_ids();
    rounds
        .values()
        .filter(|r| r.stage == RoundStage::Working && r.driver == WorkDriver::Citizen)
        // ELIGIBILITY IS RESIDENCY. She may pull from any run room she is standing in
        // — the round's `team` (really its reviewer set, empty unless `--teammates`)
        // and the dispatch-time assignee were the gate that locked 7 of 12 residents
        // out of a "shared" deck for months (2026-09-03). A card is room content; a
        // resident works it. The `run_room` IS the round id.
        .filter(|r| resident_rooms.contains(&r.round_id))
        .flat_map(|r| {
            r.cards
                .iter()
                .filter(|(_, state)| state.is_none())
                .filter(|(c, _)| !live.contains(&format!("claim-{}", c)))
                .map(|(c, _)| NextCard {
                    card: *c,
                    assignee: peer,
                    run_room: r.round_id,
                    teammates: r.team.clone(),
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

pub fn total_unworked_cards() -> usize {
    let rounds = ROUNDS.lock().expect("bench rounds mutex");
    rounds
        .values()
        .filter(|r| r.stage == RoundStage::Working)
        .map(|r| r.cards.values().filter(|s| s.is_none()).count())
        .sum()
}

/// Are any Working rounds tracked at all — the boot resume's cheap early-exit.
pub fn any_working_round() -> bool {
    ROUNDS
        .lock()
        .expect("bench rounds mutex")
        .values()
        .any(|r| r.stage == RoundStage::Working)
}

/// A round is ABANDONED-STALE when it is Working but no unsettled card has
/// produced a work artifact in [`STALE_ROUND_ABANDON_SECS`] — a wedged round
/// whose citizens died without a task boundary (measured 2026-09-02: three
/// rounds stuck ~18h, blocking the standing autopilot, un-clearable because
/// round-stop's cancellation waits for a boundary the dead citizens never
/// reach). The standing round treats these as NOT blocking, so a fresh cohort
/// dispatches over the corpse. Takes the ledger facts the board already folds.
pub const STALE_ROUND_ABANDON_SECS: u64 = 3600;

pub fn only_stale_or_no_working_rounds(runs: &[CardRunFacts], now_ms: u64) -> bool {
    let mut rounds = live_rounds();
    enrich_rounds(&mut rounds, runs, now_ms);
    no_healthy_working_round(&rounds)
}

/// How many cards across live rounds are `unstarted` — never worked, no run
/// artifact, no held-work turn. The standing autopilot's BACKLOG guard: with a
/// deep backlog, dispatching another round just deepens the pile (measured
/// 2026-09-02: the autopilot reached 30+ rounds / 111 unstarted while 4 citizens
/// worked). Complements the working-round gate — that holds while a round is
/// being worked; this holds while there is unworked work already waiting.
pub fn unworked_backlog(runs: &[CardRunFacts], now_ms: u64) -> usize {
    let mut rounds = live_rounds();
    enrich_rounds(&mut rounds, runs, now_ms);
    rounds
        .iter()
        .flat_map(|r| r.cards.iter())
        .filter(|c| c.state == "unstarted")
        .count()
}

/// PURE decision (testable without the ROUNDS global): true when no round is
/// healthily grinding — every Working round is stalled/unstarted past the
/// abandon window, or there are none. A round with a fresh act (idle under
/// the window) blocks the autopilot; a wedged one does not.
pub fn no_healthy_working_round(rounds: &[RoundSnapshot]) -> bool {
    !rounds.iter().any(|r| {
        r.stage == "working"
            && matches!(r.idle_secs, Some(idle) if idle < STALE_ROUND_ABANDON_SECS)
    })
}

fn live_run_ids() -> std::collections::HashSet<String> {
    crate::cognition::swe_bench::in_flight_solve_runs()
        .into_iter()
        .map(|(id, _)| id)
        .collect()
}

/// The one shared decision body behind both edges.
fn first_unworked(round: &BenchRound, live: &std::collections::HashSet<String>) -> Option<NextCard> {
    first_unworked_excluding(round, live, None)
}

fn first_unworked_excluding(
    round: &BenchRound,
    live: &std::collections::HashSet<String>,
    exclude: Option<Uuid>,
) -> Option<NextCard> {
    if round.driver != WorkDriver::DetachedSolve {
        return None;
    }
    // A paused round is HELD: cards stay, in-flight runs finish, and every
    // driver edge (settle-advance, non-settling advance, boot resume — all of
    // which funnel through here) hands out nothing until resume.
    if round.stage != RoundStage::Working {
        return None;
    }
    round
        .cards
        .iter()
        .filter(|(_, state)| state.is_none())
        .filter(|(c, _)| Some(**c) != exclude)
        .filter(|(c, _)| !live.contains(&format!("claim-{}", c)))
        .find_map(|(c, _)| {
            round.card_assignees.get(c).map(|a| NextCard {
                card: *c,
                assignee: *a,
                run_room: round.round_id,
                teammates: round
                    .card_activities
                    .get(c)
                    .map(|act| act.teammates.clone())
                    .unwrap_or_else(|| round.team.clone()), // no activity yet = the ROUND's team, never silently solo
            })
        })
}

/// Operator hold: flip a Working round to Paused (persisted). Returns false
/// when the round is unknown or already terminal.
pub fn pause_round(round_id: Uuid) -> bool {
    let mut rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner());
    match rounds.get_mut(&round_id) {
        Some(r) if r.stage == RoundStage::Working => {
            r.stage = RoundStage::Paused;
            persist_round_in(&rounds_state_dir(), r);
            crate::probe!(
                class = "bench.round.paused",
                round_id = %round_id,
                "round paused — in-flight solves finish, driver edges hand out nothing"
            );
            true
        }
        _ => false,
    }
}

/// Lift a pause: flip Paused → Working (persisted) and return the round's
/// first unworked card so the caller can kick the driver immediately instead
/// of waiting for the next settle edge.
pub fn resume_round(round_id: Uuid) -> Option<NextCard> {
    let live = live_run_ids();
    let mut rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner());
    let r = rounds.get_mut(&round_id)?;
    match r.stage {
        RoundStage::Paused => {
            r.stage = RoundStage::Working;
            persist_round_in(&rounds_state_dir(), r);
            crate::probe!(
                class = "bench.round.resumed_by_operator",
                round_id = %round_id,
                "round resumed from pause — driver kicked"
            );
        }
        // A WORKING round can still be IDLE — the zombie state a core reload
        // leaves behind (found live 2026-09-01: a lite round reloaded
        // 'working, 4 remaining', demand leased, and sat a full day with
        // card_activities empty, because the settle-edge driver needs a
        // settle that an idle round can never produce). The verb's own doc
        // promises 'the driver is kicked immediately' — make that true here
        // too: no stage change, just the kick. The in-flight guard
        // (`live_run_ids` via `first_unworked`) keeps this idempotent — a
        // round with a genuinely live run returns None and nothing double-
        // fires ([[launch-and-pray-is-the-defect-read-the-state-pipe-before-staging-work]]).
        RoundStage::Working => {
            crate::probe!(
                class = "bench.round.kicked_while_working",
                round_id = %round_id,
                "working round kicked by operator — firing the next unworked card if any run is not already live"
            );
        }
        _ => return None,
    }
    first_unworked(r, &live)
}

pub fn driver_for_card(card_id: Uuid) -> WorkDriver {
    ROUNDS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .values()
        .find(|r| r.cards.contains_key(&card_id))
        .map(|r| r.driver)
        .unwrap_or_default()
}

/// React to one `work.card.state_changed` payload (`{card_id, state, room_id}` — the
/// #450 bridge contract). Non-terminal transitions and cards outside every tracked round
/// are silently not-ours. Emits `bench.round.card_settled` per newly settled card and
/// `bench.round.done` exactly once when the last card lands (the round is removed on
/// that transition). Grade info is NOT in this payload — the grade runs minutes after
/// close — so the END probe reports totals, not verdicts.
/// Settle a card by DIRECT observation (not a wire event) — the reconcile path
/// for settles that happened while this core was down: a card closed during
/// downtime fired its state-change into the void, and the round would wait on
/// it forever (measured 2026-08-26: the boot resume retried a card the board no
/// longer offered, every attempt honestly aborting "not on the board"). Routes
/// through the SAME payload-shaped observer so probes and the Done transition
/// stay single-sourced.
pub fn settle_card_direct(card: Uuid, state: &str) {
    observe_card_event(&serde_json::json!({
        "card_id": card.to_string(),
        "state": state,
    }));
}

pub fn observe_card_event(payload: &Value) {
    let state = payload
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !is_terminal_card_state(state) {
        return;
    }
    let Some(card) = payload
        .get("card_id")
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
    else {
        return;
    };

    let mut rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner());
    // A card belongs to at most one round: dispatch's idempotence gate refuses a second
    // live card per task, and card uuids are minted per card. First match is THE match.
    let Some(round) = rounds.values_mut().find(|r| r.cards.contains_key(&card)) else {
        return;
    };
    let round_id = round.round_id;
    match round.settle_card(card, state) {
        SettleOutcome::NotOurs | SettleOutcome::AlreadySettled => {}
        SettleOutcome::Settled { remaining } => {
            persist_round_in(&rounds_state_dir(), round);
            crate::probe!(
                class = "bench.round.card_settled",
                round_id = %round_id,
                card_id = %card,
                state = %state,
                remaining = remaining,
                "round card settled"
            );
        }
        SettleOutcome::RoundDone => {
            let (dispatched, benchmark) = (round.dispatched(), round.benchmark.clone());
            crate::probe!(
                class = "bench.round.card_settled",
                round_id = %round_id,
                card_id = %card,
                state = %state,
                remaining = 0usize,
                "round card settled"
            );
            crate::probe!(
                class = "bench.round.done",
                round_id = %round_id,
                benchmark = %benchmark,
                dispatched = dispatched,
                settled = dispatched,
                "benchmark round END — every card reached a terminal state"
            );
            rounds.remove(&round_id);
            remove_round_file_in(&rounds_state_dir(), round_id);
        }
    }
}

/// One in-flight round, as anyone may ask about it.
///
/// # Why this type exists (2026-08-18, and it is the acceptance test)
///
/// The round entity below has tracked stage, card set, and driver since it was written —
/// and NOTHING could ask it. Transitions fired probes and the state lived in a private
/// static, so "has the round started / is it stuck / is it done" was answerable only by
/// probe archaeology. That is precisely the failure
/// [ROUND-LIFECYCLE-AS-RECIPE-OWNED-STATE-MACHINE.md](../../../../docs/architecture/ROUND-LIFECYCLE-AS-RECIPE-OWNED-STATE-MACHINE.md)
/// names in its own acceptance test:
///
/// > *Can a fresh driver — with no memory of this session — answer "is it ready, has it
/// > started, is it stuck, is it done" using only queries, with zero log reads, zero probe
/// > archaeology, and zero inference from an absence?*
///
/// A probe is a transition RECORD. It tells you what happened when someone was watching.
/// It cannot answer a question asked later, which is when every question is actually asked.
///
/// **`settled` is reported explicitly rather than left to subtraction.** `dispatched -
/// remaining` is the same number, and making the reader compute it is how an absence
/// becomes a guess ([[an-absence-is-an-unfinished-measurement]]). Law 3 of the design doc:
/// *an absence is never a state.*
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
    ts_rs::TS,
)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/RoundSnapshot.ts"
)]
pub struct RoundSnapshot {
    /// The round id — which IS its run room's id. A round is its room's activity; there is
    /// never a second identifier ([[killing-a-derived-id-needs-a-directory-at-every-scope-boundary]]).
    pub round_id: String,
    pub benchmark: String,
    /// `working` | `done`. Present here means IN FLIGHT — a completed round is removed the
    /// instant it finishes, so `done` is only ever observed in the transition probe.
    pub stage: String,
    /// Cards this round dispatched.
    pub dispatched: usize,
    /// Cards that have reached a terminal state.
    pub settled: usize,
    /// Cards still working. Zero here with the round still listed would be a defect —
    /// the settle that empties the set is what removes it.
    pub remaining: usize,
    /// Who works these cards: `citizen` (in the room, produces turns, feeds the curriculum)
    /// or `detached_solve`. Decided at dispatch, read at claim time.
    pub driver: String,
    /// Per-card status — the row the board renders under the round. `default`
    /// so pre-cards wires still deserialize. Filled from the tracker by
    /// [`live_rounds`]; run facts (acts, patch, recency) merge in via
    /// [`enrich_rounds`] wherever a run-ledger scan is in hand.
    #[serde(default)]
    pub cards: Vec<RoundCardSnapshot>,
    /// Glanceable health: `unstarted` (no card has produced a single work
    /// artifact — the shape 2026-09-01 exposed: `working 0/8` for three hours
    /// was pixel-identical to grinding) | `grinding` (an unsettled card acted
    /// within the stall window) | `stalled` (work exists but nothing acted
    /// within it) | `paused` | `done`. Never derived client-side.
    #[serde(default)]
    pub verdict: String,
    /// Seconds since the newest work artifact across this round's unsettled
    /// cards. `None` = no card has ever produced one — an absence reported
    /// as an absence, never as `0` ([[an-absence-is-an-unfinished-measurement]]).
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub idle_secs: Option<u64>,
}

/// One card of a round, as the board renders it: WHAT (instance), WHO
/// (assignee), WHERE (solve room), and how it is going (state, acts, patch,
/// recency). Tracker fields fill at [`live_rounds`]; run-ledger fields fill
/// at [`enrich_rounds`] and stay `None` until a run exists — a card with no
/// workspace renders honestly as `unstarted`, never as an empty "working".
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
    ts_rs::TS,
)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/RoundCardSnapshot.ts"
)]
pub struct RoundCardSnapshot {
    pub card_id: String,
    /// Instance under test, parsed from the solve room's name
    /// (`swe--<instance>--<card8>`). Empty until the card's activity is minted.
    pub instance: String,
    /// Solver NAME once a run ledger names one; else the staged assignee's
    /// uuid. Empty = never staged to anyone (itself a finding).
    pub assignee: String,
    /// The solve activity's airc name — the navigable door. Empty until minted.
    pub solve_room_name: String,
    /// `unstarted` (no run artifact) | the run phase (`active`, `quiet`,
    /// `ungraded`, …) while working | the terminal state once settled.
    pub state: String,
    #[ts(optional, type = "number")]
    pub acts: Option<u32>,
    #[ts(optional, type = "number")]
    pub patch_bytes: Option<u32>,
    /// Seconds since this card's newest work artifact. `None` = none ever.
    #[ts(optional, type = "number")]
    pub last_act_secs: Option<u64>,
    #[ts(optional)]
    pub resolved: Option<bool>,
    /// BOARD truth (the durable record): the card's column right now —
    /// `open|claimed|in_progress|blocked|review|merged|closed`. Empty when the
    /// board could not be read.
    #[serde(default)]
    pub board_state: String,
    /// Who holds it on the board (display name, else short id). Empty = nobody.
    #[serde(default)]
    pub owner: String,
    /// Board timestamps — the experiment axes: time-to-claim and time-to-settle
    /// read from these, never from a process clock that a reboot resets.
    #[ts(optional, type = "number")]
    #[serde(default)]
    pub created_at_ms: Option<u64>,
    #[ts(optional, type = "number")]
    #[serde(default)]
    pub updated_at_ms: Option<u64>,
    /// When the verdict file was written (`SweVerdict::graded_at_ms`).
    #[ts(optional, type = "number")]
    #[serde(default)]
    pub graded_at_ms: Option<u64>,
}

/// The run-ledger facts [`enrich_rounds`] merges into a card row — a minimal
/// projection of `commands::benchmark::BenchRunCard`, defined HERE so the
/// dependency points commands→cognition like everything else.
pub struct CardRunFacts {
    pub instance: String,
    pub solver: Option<String>,
    pub phase: String,
    pub acts: Option<u32>,
    pub patch_bytes: Option<u32>,
    pub last_activity_ms: u64,
    pub resolved: Option<bool>,
}

/// An unsettled card silent longer than this, on a round where work HAS
/// started, marks the round `stalled`. Acts land every 2–6 min when a citizen
/// is actually driving a workspace; 20 min of silence is a verdict, not noise.
const ROUND_STALL_AFTER_SECS: u64 = 1200;

/// `swe--<instance>--<card8>` → `<instance>`. Empty in → empty out.
fn instance_of_room_name(name: &str) -> &str {
    name.strip_prefix("swe--")
        .and_then(|rest| rest.rsplit_once("--").map(|(inst, _)| inst))
        .unwrap_or("")
}

/// Merge run-ledger facts into round snapshots and pronounce each round's
/// verdict. ONE fold, called by both consumers (the `benchmark/rounds`
/// command and the positron bench emitter) so a mind and a screen can never
/// disagree about whether a round is grinding.
///
/// Matching is by INSTANCE, newest run wins — two rounds sampling the same
/// instance share history, and the newest artifact is the one that answers
/// "is anyone on this now".
pub fn enrich_rounds(rounds: &mut [RoundSnapshot], runs: &[CardRunFacts], now_ms: u64) {
    for round in rounds.iter_mut() {
        let mut newest_unsettled_act: Option<u64> = None;
        let mut any_started = false;
        for card in round.cards.iter_mut() {
            let settled = !matches!(card.state.as_str(), "" | "unstarted");
            let run = runs
                .iter()
                .filter(|r| !card.instance.is_empty() && r.instance == card.instance)
                .max_by_key(|r| r.last_activity_ms);
            let Some(run) = run else {
                // No detached-solve ledger entry — but a CITIZEN-driven card
                // leaves no ledger, so fall back to the held-work freshness the
                // turn boundary stamps. A recent held-work turn makes the card
                // read "working" (not "unstarted") and feeds the round's idle
                // clock, so the standing autopilot sees the round is being worked
                // and holds instead of piling up duplicates.
                if !settled {
                    let held_act = card
                        .card_id
                        .parse::<Uuid>()
                        .ok()
                        .and_then(card_last_act_ms);
                    if let Some(act_ms) = held_act {
                        any_started = true;
                        let act_age = now_ms.saturating_sub(act_ms) / 1000;
                        card.last_act_secs = Some(act_age);
                        card.state = "working".to_string();
                        newest_unsettled_act =
                            Some(newest_unsettled_act.map_or(act_age, |a: u64| a.min(act_age)));
                    } else {
                        card.state = "unstarted".to_string();
                    }
                }
                continue;
            };
            any_started = true;
            if let Some(solver) = &run.solver {
                card.assignee = solver.clone();
            }
            card.acts = run.acts;
            card.patch_bytes = run.patch_bytes;
            card.resolved = run.resolved;
            let act_age = now_ms.saturating_sub(run.last_activity_ms) / 1000;
            card.last_act_secs = Some(act_age);
            if !settled {
                card.state = run.phase.clone();
                newest_unsettled_act =
                    Some(newest_unsettled_act.map_or(act_age, |a: u64| a.min(act_age)));
            }
        }
        round.idle_secs = newest_unsettled_act;
        round.verdict = match round.stage.as_str() {
            "paused" => "paused".to_string(),
            "done" => "done".to_string(),
            _ => match newest_unsettled_act {
                None if !any_started => "unstarted".to_string(),
                // Every card WITH a run is settled; unsettled ones never started.
                None => "unstarted".to_string(),
                Some(age) if age > ROUND_STALL_AFTER_SECS => "stalled".to_string(),
                Some(_) => "grinding".to_string(),
            },
        };
    }
}

/// Every round this core is tracking, in a stable order.
///
/// Sorted by round id so two calls a second apart cannot reorder rows under a reader —
/// a projection whose row order depends on `HashMap` iteration teaches its consumers to
/// distrust it.
/// Every live round's ACTIVITY rooms — `(solve_room, airc_name)` for each
/// card that has one, skipping rows whose name predates `room_name` (empty).
/// The presence emitter merges these with the registry rooms so per-run
/// rosters and transcripts reach the interface (#2606 / #2632: the academy's
/// children are rooms, and a room without presence renders blank).
pub fn activity_rooms() -> Vec<(Uuid, String)> {
    let rounds = ROUNDS.lock().unwrap_or_else(|e| e.into_inner());
    rounds
        .values()
        .filter(|r| r.stage != RoundStage::Done)
        .flat_map(|r| {
            let run = (!r.run_room_name.is_empty())
                .then(|| (r.round_id, r.run_room_name.clone()));
            run.into_iter().chain(
                r.card_activities
                    .values()
                    .filter(|a| !a.room_name.is_empty())
                    .map(|a| (a.solve_room, a.room_name.clone()))
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
}

pub fn live_rounds() -> Vec<RoundSnapshot> {
    let rounds = ROUNDS.lock().unwrap_or_else(|e| e.into_inner());
    let mut out: Vec<RoundSnapshot> = rounds
        .values()
        .map(|r| {
            let mut cards: Vec<RoundCardSnapshot> = r
                .cards
                .iter()
                .map(|(card_id, settled)| {
                    let activity = r.card_activities.get(card_id);
                    let assignee = activity
                        .map(|a| a.assignee)
                        .or_else(|| r.card_assignees.get(card_id).copied())
                        .map(|u| u.to_string())
                        .unwrap_or_default();
                    let room_name = activity
                        .map(|a| a.room_name.clone())
                        .unwrap_or_default();
                    // Activity room name is the authority once minted; the
                    // staging-time record covers every card before that.
                    let instance = match instance_of_room_name(&room_name) {
                        "" => r.card_instances.get(card_id).cloned().unwrap_or_default(),
                        parsed => parsed.to_string(),
                    };
                    RoundCardSnapshot {
                        card_id: card_id.to_string(),
                        instance,
                        assignee,
                        solve_room_name: room_name,
                        // Terminal state from the tracker; open cards report
                        // "unstarted" until enrich_rounds sees a run artifact.
                        state: settled.clone().unwrap_or_else(|| "unstarted".to_string()),
                        acts: None,
                        patch_bytes: None,
                        last_act_secs: None,
                        resolved: None,
                        board_state: String::new(),
                        owner: String::new(),
                        created_at_ms: None,
                        updated_at_ms: None,
                        graded_at_ms: None,
                    }
                })
                .collect();
            cards.sort_by(|a, b| a.instance.cmp(&b.instance).then(a.card_id.cmp(&b.card_id)));
            RoundSnapshot {
                round_id: r.round_id.to_string(),
                benchmark: r.benchmark.clone(),
                stage: match r.stage {
                    RoundStage::Working => "working",
                    RoundStage::Paused => "paused",
                    RoundStage::Done => "done",
                }
                .to_string(),
                dispatched: r.dispatched(),
                settled: r.dispatched().saturating_sub(r.remaining()),
                remaining: r.remaining(),
                driver: match r.driver {
                    WorkDriver::Citizen => "citizen",
                    WorkDriver::DetachedSolve => "detached_solve",
                }
                .to_string(),
                cards,
                // Honest defaults until enrich_rounds runs with ledger facts.
                verdict: String::new(),
                idle_secs: None,
            }
        })
        .collect();
    out.sort_by(|a, b| a.round_id.cmp(&b.round_id));
    out
}

#[cfg(test)]
mod tests {
    // what this catches: the sensor 2026-09-01 lacked — `working 0/8` was
    // pixel-identical for three hours of workspace-less thrash and a healthy
    // grind. The verdict must pronounce: no artifacts anywhere → `unstarted`;
    // a recent act on an unsettled card → `grinding`; artifacts gone silent
    // past the stall window → `stalled`; and an unstarted card must RENDER
    // (state "unstarted"), never vanish for lack of a run ledger.
    #[test]
    fn round_verdict_separates_thrash_from_grind() {
        use super::{enrich_rounds, CardRunFacts, RoundCardSnapshot, RoundSnapshot};
        let now_ms: u64 = 10_000_000_000;
        let card = |instance: &str| RoundCardSnapshot {
            card_id: Uuid::new_v4().to_string(),
            instance: instance.to_string(),
            assignee: String::new(),
            solve_room_name: format!("swe--{instance}--abcd1234"),
            state: "unstarted".to_string(),
            acts: None,
            patch_bytes: None,
            last_act_secs: None,
            resolved: None,
            board_state: String::new(),
            owner: String::new(),
            created_at_ms: None,
            updated_at_ms: None,
            graded_at_ms: None,
        };
        let round = |cards: Vec<RoundCardSnapshot>| RoundSnapshot {
            round_id: Uuid::new_v4().to_string(),
            benchmark: "swe-bench-verified".into(),
            stage: "working".into(),
            dispatched: cards.len(),
            settled: 0,
            remaining: cards.len(),
            driver: "citizen".into(),
            cards,
            verdict: String::new(),
            idle_secs: None,
        };
        let mut rounds = vec![
            round(vec![card("sympy__sympy-12481"), card("django__django-11211")]),
            round(vec![card("astropy__astropy-12907")]),
            round(vec![card("scikit__scikit-1")]),
        ];
        let facts = vec![
            // Fresh act 60s ago → its round grinds.
            CardRunFacts {
                instance: "astropy__astropy-12907".into(),
                solver: Some("Kira".into()),
                phase: "active".into(),
                acts: Some(7),
                patch_bytes: Some(400),
                last_activity_ms: now_ms - 60_000,
                resolved: None,
            },
            // Artifact exists but silent 2h → its round stalled.
            CardRunFacts {
                instance: "scikit__scikit-1".into(),
                solver: None,
                phase: "quiet".into(),
                acts: Some(3),
                patch_bytes: None,
                last_activity_ms: now_ms - 7_200_000,
                resolved: None,
            },
        ];
        enrich_rounds(&mut rounds, &facts, now_ms);
        assert_eq!(rounds[0].verdict, "unstarted");
        assert_eq!(rounds[0].idle_secs, None);
        // The unstarted cards RENDER — the whole point.
        assert!(rounds[0].cards.iter().all(|c| c.state == "unstarted"));
        assert_eq!(rounds[1].verdict, "grinding");
        assert_eq!(rounds[1].idle_secs, Some(60));
        assert_eq!(rounds[1].cards[0].assignee, "Kira");
        assert_eq!(rounds[1].cards[0].acts, Some(7));
        assert_eq!(rounds[2].verdict, "stalled");
        assert_eq!(rounds[2].cards[0].state, "quiet");
    }

    // what this catches: A CITIZEN-driven card leaves NO detached-solve ledger
    // entry, so before the held-work freshness signal the round read "unstarted"
    // while she was actively working it — and the standing autopilot, seeing no
    // working round, piled up duplicate rounds (measured 2026-09-02: 21 rounds,
    // 53 unstarted, 4 citizens). This pins that a held-work turn makes the round
    // read "working"/"grinding" from the ephemeral CARD_LAST_ACT signal alone
    // (empty run ledger), so the autopilot HOLDS instead of over-dispatching.
    #[test]
    fn a_citizen_held_work_turn_makes_the_round_read_working() {
        use super::{
            card_last_act_ms, enrich_rounds, no_healthy_working_round, record_card_worked,
            RoundCardSnapshot, RoundSnapshot,
        };
        let now_ms: u64 = 20_000_000_000;
        let card_uuid = Uuid::new_v4();
        // She drove a held-work turn 30s ago — no run-ledger fact exists.
        // Freshness lives ON THE ROUND (durable): a card outside any round has none,
        // a card in a live round remembers its latest held-work turn.
        let stray = Uuid::new_v4();
        record_card_worked(stray, now_ms);
        assert_eq!(card_last_act_ms(stray), None, "no round, nowhere to keep freshness");
        let round_id = Uuid::new_v4();
        open_round(round_id, "swe-bench-verified", WorkDriver::Citizen);
        add_card(round_id, card_uuid);
        record_card_worked(card_uuid, now_ms - 30_000);
        assert_eq!(card_last_act_ms(card_uuid), Some(now_ms - 30_000));

        let mut rounds = vec![RoundSnapshot {
            round_id: Uuid::new_v4().to_string(),
            benchmark: "swe-bench-verified".into(),
            stage: "working".into(),
            dispatched: 1,
            settled: 0,
            remaining: 1,
            driver: "citizen".into(),
            cards: vec![RoundCardSnapshot {
                card_id: card_uuid.to_string(),
                instance: "django__django-12273".into(),
                assignee: String::new(),
                solve_room_name: String::new(),
                state: "unstarted".into(),
                acts: None,
                patch_bytes: None,
                last_act_secs: None,
                resolved: None,
                board_state: String::new(),
                owner: String::new(),
                created_at_ms: None,
                updated_at_ms: None,
                graded_at_ms: None,
            }],
            verdict: String::new(),
            idle_secs: None,
        }];
        // EMPTY run ledger — the detached-solve facts a Citizen card never produces.
        enrich_rounds(&mut rounds, &[], now_ms);

        assert_eq!(
            rounds[0].cards[0].state, "working",
            "held-work makes the card read working, not a false 'unstarted'"
        );
        assert_eq!(
            rounds[0].idle_secs,
            Some(30),
            "the round's idle clock reflects her held-work turn"
        );
        assert_eq!(
            rounds[0].verdict, "grinding",
            "the round is visibly being worked"
        );
        assert!(
            !no_healthy_working_round(&rounds),
            "a freshly held-worked round is a healthy working round — the autopilot HOLDS"
        );
    }

    // what this catches: a wedged Working round (dead citizens, no task
    // boundary) blocking the standing autopilot FOREVER — measured 2026-09-02,
    // three rounds stuck ~18h while the claim-growth engine starved because it
    // only fired when NO round was Working. The autopilot must dispatch over a
    // round stale past the abandon window, but HOLD for a healthily grinding
    // one. Regression pin for that exact bug.
    use super::{no_healthy_working_round, RoundSnapshot, STALE_ROUND_ABANDON_SECS};
    fn snap(stage: &str, idle: Option<u64>) -> RoundSnapshot {
        RoundSnapshot {
            round_id: Uuid::new_v4().to_string(),
            benchmark: "swe-bench-verified".into(),
            stage: stage.to_string(),
            dispatched: 8,
            settled: 0,
            remaining: 8,
            driver: "citizen".into(),
            cards: Vec::new(),
            verdict: String::new(),
            idle_secs: idle,
        }
    }

    #[test]
    fn stale_working_round_does_not_block_the_autopilot_but_a_fresh_one_does() {
        // No rounds → clear to dispatch.
        assert!(no_healthy_working_round(&[]));
        // A round wedged past the abandon window → clear (the 18h-stall bug).
        assert!(no_healthy_working_round(&[snap(
            "working",
            Some(STALE_ROUND_ABANDON_SECS + 60)
        )]));
        // A round with a fresh act → BLOCKS (never dispatch over live work).
        assert!(!no_healthy_working_round(&[snap("working", Some(30))]));
        // Mixed: one fresh among stale still blocks.
        assert!(!no_healthy_working_round(&[
            snap("working", Some(STALE_ROUND_ABANDON_SECS + 60)),
            snap("working", Some(30)),
        ]));
        // Unstarted (idle None) is not "healthy grinding" → does not block.
        assert!(no_healthy_working_round(&[snap("working", None)]));
    }

    // what this catches: the round advancing only by TIMEOUT, and the loop the
    // naive fix creates. Measured 2026-08-28 — a run can finish WITHOUT settling
    // its card (no diff, env absent, cached env would not re-point). Those emit
    // no card-settled event, so the settle edge never fires and the round sat
    // idle behind one finished solve until the 5-minute becalmed watchdog
    // noticed (01:58 and 03:30 both did exactly that).
    //
    // The exclusion IS the loop guard: after a non-settling run the card is
    // still unsettled and no longer live, so the unguarded decision hands back
    // the very card that just failed — forever. Both halves are pinned here.
    #[test]
    fn the_non_settling_edge_advances_past_the_card_that_just_failed() {
        fn round_with(cards: &[(Uuid, Uuid)]) -> BenchRound {
            BenchRound {
                run_room_name: String::new(),
                round_id: Uuid::new_v4(),
                benchmark: "swe-bench-lite".into(),
                cards: cards.iter().map(|(c, _)| (*c, None)).collect(),
                stage: RoundStage::Working,
                driver: WorkDriver::DetachedSolve,
                card_activities: Default::default(),
                card_assignees: cards.iter().copied().collect(),
                card_instances: Default::default(),
                card_last_act_ms: HashMap::new(),
                team: Vec::new(),
            }
        }
        let live = std::collections::HashSet::new();
        let who = Uuid::new_v4();
        let failed = Uuid::new_v4();
        let next = Uuid::new_v4();

        // THE LOOP GUARD. One card left, and it is the one that just failed:
        // unguarded the decision hands it straight back (re-firing forever);
        // excluded, the chain honestly ends.
        let only = round_with(&[(failed, who)]);
        assert_eq!(
            first_unworked(&only, &live).map(|n| n.card),
            Some(failed),
            "sanity: unguarded, the just-failed card is handed back — the tight loop"
        );
        assert!(
            first_unworked_excluding(&only, &live, Some(failed)).is_none(),
            "the last card failing must end the chain, never re-fire itself"
        );

        // THE ADVANCE. Real work remains, so the round moves NOW rather than
        // waiting out the 5-minute becalmed watchdog.
        let two = round_with(&[(failed, who), (next, who)]);
        assert_eq!(
            first_unworked_excluding(&two, &live, Some(failed)).map(|n| n.card),
            Some(next),
            "the non-settling edge must advance PAST the card that just failed"
        );
    }


    use super::*;

    fn cards(n: usize) -> Vec<Uuid> {
        (0..n).map(|_| Uuid::new_v4()).collect()
    }

    // what this catches: the reboot amnesia measured twice on 2026-08-21 — a live
    // round answered `rounds: []` after restart, and (silent, worse) driver_for_card
    // fell back to DetachedSolve for a Citizen round's remaining cards, quietly
    // rebuilding the parallel runner the round was configured to avoid. A round must
    // round-trip its FULL deciding state: driver, card set, and which cards settled.
    #[test]
    fn a_round_survives_a_core_restart_with_driver_and_settles_intact() {
        let dir = std::env::temp_dir().join(format!("bench-rounds-test-{}", Uuid::new_v4()));
        let (id, cs) = (Uuid::new_v4(), cards(3));
        let mut round = BenchRound::new(id, "swe-bench-lite", &cs, WorkDriver::Citizen);
        assert!(matches!(round.settle_card(cs[0], "closed"), SettleOutcome::Settled { remaining: 2 }));
        persist_round_in(&dir, &round);

        // "reboot": reload from disk into a fresh map.
        let reloaded = load_rounds_in(&dir);
        let r = reloaded.get(&id).expect("in-flight round must reload");
        assert_eq!(r.driver, WorkDriver::Citizen, "driver reverting is the silent bug");
        assert_eq!(r.dispatched(), 3);
        assert_eq!(r.remaining(), 2, "the settled card must stay settled across the restart");
        std::fs::remove_dir_all(&dir).ok();
    }

    // what this catches: mint-or-rejoin losing its memory across a reboot. The
    // card's solve ACTIVITY (its per-instance room + assignee) is recorded at
    // first mint and must survive persistence — a re-fire that can't find it
    // would mint a SECOND room for the same work, stranding the first room's
    // transcript and cold-starting the KV slot the activity had warmed
    // (Joel 2026-08-26: "benchmarks without new activities (unless rejoining)").
    #[test]
    fn card_activity_survives_a_restart_for_rejoin() {
        let dir = std::env::temp_dir().join(format!("bench-rounds-test-{}", Uuid::new_v4()));
        let (id, cs) = (Uuid::new_v4(), cards(2));
        let mut round = BenchRound::new(id, "swe-bench-verified", &cs, WorkDriver::DetachedSolve);
        let act = CardActivity {
            room_name: String::new(),
            teammates: Vec::new(), // test row: solo

            solve_room: Uuid::from_u128(0xA11CE),
            assignee: Uuid::from_u128(0xBEE),
        };
        round.card_activities.insert(cs[0], act.clone()); // clone: the assert below compares against the original
        persist_round_in(&dir, &round);

        let reloaded = load_rounds_in(&dir);
        let r = reloaded.get(&id).expect("round reloads");
        let got = r.card_activities.get(&cs[0]).cloned().expect("activity survives");
        assert_eq!(got.solve_room, act.solve_room, "rejoin returns the SAME room");
        assert_eq!(got.assignee, act.assignee);
        assert!(r.card_activities.get(&cs[1]).is_none(), "unminted card has no activity yet");
        std::fs::remove_dir_all(&dir).ok();
    }

    // what this catches: the ONE driver decision. A settled card must yield the
    // round's next unworked card (with its staged assignee and the run room),
    // a fully-fired round must yield None, and a Citizen-driver round must
    // never be driven by the follow-on (citizens drive themselves via claims).
    // regression for the solve_cap starvation: dispatched − solves_fired used
    // to be PERMANENT — cards 2..N were posted, kicked off, and never worked.
    #[test]
    fn next_unworked_after_walks_the_round_to_empty() {
        let (id, cs) = (Uuid::new_v4(), cards(3));
        register_round(id, "swe-bench-verified", &cs);
        {
            let mut rounds = ROUNDS.lock().unwrap();
            let r = rounds.get_mut(&id).unwrap();
            r.driver = WorkDriver::DetachedSolve;
            for (i, c) in cs.iter().enumerate() {
                r.card_assignees.insert(*c, Uuid::from_u128(100 + i as u128));
            }
        }
        // Settle card 0 → the driver owes one of the remaining two.
        {
            let mut rounds = ROUNDS.lock().unwrap();
            rounds.get_mut(&id).unwrap().settle_card(cs[0], "closed");
        }
        let next = next_unworked_after(cs[0]).expect("two cards remain");
        assert!(cs[1..].contains(&next.card), "next is an UNSETTLED card");
        assert_eq!(next.run_room, id, "the dispatch rejoins the run room's board");
        let expected_assignee = {
            let rounds = ROUNDS.lock().unwrap();
            *rounds.get(&id).unwrap().card_assignees.get(&next.card).unwrap()
        };
        assert_eq!(next.assignee, expected_assignee, "the card's STAGED assignee carries");
        // Settle everything → the driver goes quiet.
        {
            let mut rounds = ROUNDS.lock().unwrap();
            let r = rounds.get_mut(&id).unwrap();
            r.settle_card(cs[1], "closed");
            r.settle_card(cs[2], "closed");
        }
        // The round is Done and REMOVED by observe_card_event in production; here the
        // map still holds it, which is exactly what lets us assert the quiet case.
        assert!(next_unworked_after(cs[0]).is_none(), "a finished round owes nothing");

        // A Citizen round is never follow-on driven.
        let (cid, ccs) = (Uuid::new_v4(), cards(2));
        register_round(cid, "swe-bench-verified", &ccs);
        assert!(next_unworked_after(ccs[0]).is_none(), "citizen rounds drive themselves");
    }

    // what this catches: the state dir growing without an eviction story (the
    // 2026-07-13 rule). Done is terminal — its file is removed at settle, and a
    // Done file that survives a crash window is dropped (and deleted) on reload
    // rather than resurrected as a round no event can ever settle.
    #[test]
    fn done_rounds_never_reload_and_their_files_self_evict() {
        let dir = std::env::temp_dir().join(format!("bench-rounds-test-{}", Uuid::new_v4()));
        let (id, cs) = (Uuid::new_v4(), cards(1));
        let mut round = BenchRound::new(id, "swe-bench-lite", &cs, WorkDriver::Citizen);
        persist_round_in(&dir, &round);
        assert!(matches!(round.settle_card(cs[0], "closed"), SettleOutcome::RoundDone));
        // Crash window: the round reached Done but its file removal never ran.
        persist_round_in(&dir, &round);
        let reloaded = load_rounds_in(&dir);
        assert!(reloaded.is_empty(), "a Done round must not resurrect");
        assert!(
            !dir.join(format!("{id}.json")).exists(),
            "the Done file must be evicted on the reload that drops it"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Open a round and add its cards — the dispatch sequence, for tests that only care
    /// about the finished set.
    fn register_round(round_id: Uuid, benchmark: &str, card_ids: &[Uuid]) {
        open_round(round_id, benchmark, WorkDriver::default());
        for c in card_ids {
            add_card(round_id, *c);
        }
        seal_round(round_id);
    }

    // what this catches: the window between "card posted" and "round registered". Dispatch
    // sends a kickoff inside its card loop, so a citizen can claim card 1 while card 2 is
    // still being posted — and `work/claim` asks `driver_for_card` whether to fire the
    // detached solver. If the driver were only readable after the whole loop, a
    // Citizen-driven round would fire the solver on its first card and defeat itself
    // silently. The driver must be right from the first `add_card`, before `seal_round`.
    #[test]
    fn the_driver_is_readable_the_instant_a_card_can_be_claimed() {
        let round_id = Uuid::new_v4();
        let first = Uuid::new_v4();
        open_round(round_id, "swe-bench-lite", WorkDriver::Citizen);
        add_card(round_id, first);
        // Mid-loop: the round is NOT sealed, a second card is not posted yet.
        assert_eq!(
            driver_for_card(first),
            WorkDriver::Citizen,
            "a claim landing mid-dispatch must see the round's real driver"
        );
        seal_round(round_id);
        assert_eq!(driver_for_card(first), WorkDriver::Citizen);
        ROUNDS.lock().unwrap().remove(&round_id);
    }

    // what this catches: the default biting the wrong way — INVERTED 2026-08-31
    // (PREMISE CHANGE with the WorkDriver default flip). A card belonging to no
    // live round — human-claimed, undirected, or a leftover claimed after its
    // round ended — drives CITIZEN: the claimer works it in her own service
    // loop, turns feed the flywheel. Detached is the per-round diagnostic
    // opt-in, never what an ordinary claim silently falls into (four detached
    // rounds ran invisible all night on the old default).
    #[test]
    fn a_card_in_no_round_drives_by_citizen() {
        assert_eq!(driver_for_card(Uuid::new_v4()), WorkDriver::Citizen);
    }

    // what this catches: a round that dispatched nothing must END, not sit in the map
    // forever waiting for an event that can never arrive (no cards = no card events).
    #[test]
    fn an_empty_round_seals_straight_to_done_and_leaves_no_entry() {
        let round_id = Uuid::new_v4();
        open_round(round_id, "swe-bench-lite", WorkDriver::DetachedSolve);
        seal_round(round_id);
        assert!(
            ROUNDS.lock().unwrap().get(&round_id).is_none(),
            "an empty round must not remain tracked"
        );
    }

    // what this catches: the ZOMBIE round (found live 2026-09-01) — a core
    // reload leaves a round WORKING with no live run and no settle edge ever
    // coming, and resume_round used to no-op on anything not Paused, so the
    // one verb whose doc promises "the driver is kicked immediately" left the
    // exact stalled state it exists for. A Working round with no live run must
    // hand back its first unworked card.
    #[test]
    fn resume_kicks_a_working_idle_round() {
        let ids = cards(2);
        let round_id = Uuid::new_v4();
        let mut r = BenchRound::new(round_id, "swe-bench-lite", &ids, WorkDriver::DetachedSolve);
        for c in &ids {
            r.card_assignees.insert(*c, Uuid::new_v4());
        }
        ROUNDS.lock().unwrap().insert(round_id, r);
        let next = resume_round(round_id);
        assert!(
            next.as_ref().is_some_and(|n| ids.contains(&n.card)),
            "a WORKING round with no live run must kick its first unworked card: {next:?}"
        );
        ROUNDS.lock().unwrap().remove(&round_id);
    }

    // what this catches: the END transition firing more than once. All-settled must yield
    // RoundDone exactly once — a re-delivered terminal event after Done must be a no-op,
    // or the room would get N "round over" announcements for one round.
    #[test]
    fn all_settled_transitions_to_done_exactly_once() {
        let ids = cards(3);
        let mut r = BenchRound::new(Uuid::new_v4(), "humaneval-rs", &ids, WorkDriver::default());
        assert_eq!(r.stage(), RoundStage::Working);
        assert_eq!(r.settle_card(ids[0], "closed"), SettleOutcome::Settled { remaining: 2 });
        assert_eq!(r.settle_card(ids[1], "merged"), SettleOutcome::Settled { remaining: 1 });
        assert_eq!(r.settle_card(ids[2], "closed"), SettleOutcome::RoundDone);
        assert_eq!(r.stage(), RoundStage::Done);
        // Any further event on an already-settled card is inert — never a second Done.
        assert_eq!(r.settle_card(ids[2], "closed"), SettleOutcome::AlreadySettled);
        assert_eq!(r.stage(), RoundStage::Done);
    }

    // what this catches: the projection disagreeing with the entity it projects — a
    // "settled" count that drifts from the round's own card map is worse than no query at
    // all, because a driver would BELIEVE it. Also pins that progress is legible mid-round:
    // the whole point of #371 is answering "how far along" without reading a log.
    #[test]
    fn a_round_in_flight_reports_its_own_progress_honestly() {
        let round_id = Uuid::new_v4();
        let ids = cards(3);
        open_round(round_id, "swe-bench-lite", WorkDriver::Citizen);
        for id in &ids {
            add_card(round_id, *id);
        }

        let before = live_rounds();
        let row = before
            .iter()
            .find(|r| r.round_id == round_id.to_string())
            .expect("a dispatched round must be QUERYABLE — that is the whole fix");
        assert_eq!(row.stage, "working");
        assert_eq!((row.dispatched, row.settled, row.remaining), (3, 0, 3));
        assert_eq!(
            row.driver, "citizen",
            "the driver decides whether this round teaches anybody anything — it must be readable"
        );

        // Settle one, and the projection must move WITH the entity, not lag it.
        ROUNDS
            .lock()
            .unwrap()
            .get_mut(&round_id)
            .expect("still in flight")
            .settle_card(ids[0], "closed");
        let mid = live_rounds();
        let row = mid
            .iter()
            .find(|r| r.round_id == round_id.to_string())
            .expect("two of three cards are still working, so the round is still in flight");
        assert_eq!(
            (row.dispatched, row.settled, row.remaining),
            (3, 1, 2),
            "settled is reported, never left to subtraction — an absence must not become a guess"
        );

        ROUNDS.lock().unwrap().remove(&round_id);
    }

    // what this catches: a duplicate terminal event double-counting a card. The bridge
    // dedupes by event id, but a card can legitimately transition closed→merged; the
    // second settle must not decrement `remaining` a second time (a round declared Done
    // with cards still working is a lying END).
    #[test]
    fn a_card_settling_twice_does_not_double_count() {
        let ids = cards(2);
        let mut r = BenchRound::new(Uuid::new_v4(), "humaneval-rs", &ids, WorkDriver::default());
        assert_eq!(r.settle_card(ids[0], "closed"), SettleOutcome::Settled { remaining: 1 });
        assert_eq!(r.settle_card(ids[0], "merged"), SettleOutcome::AlreadySettled);
        assert_eq!(r.remaining(), 1, "the duplicate must not consume the other card's slot");
        assert_eq!(r.stage(), RoundStage::Working);
    }

    // what this catches: a card from OUTSIDE the round's dispatched set (another round's
    // card, a hand-written work card) advancing this round. Membership is the set
    // dispatch posted, nothing else.
    #[test]
    fn a_card_outside_the_set_is_ignored() {
        let ids = cards(1);
        let mut r = BenchRound::new(Uuid::new_v4(), "humaneval-rs", &ids, WorkDriver::default());
        assert_eq!(r.settle_card(Uuid::new_v4(), "closed"), SettleOutcome::NotOurs);
        assert_eq!(r.remaining(), 1);
        assert_eq!(r.stage(), RoundStage::Working);
    }

    // what this catches: the wire seam. `observe_card_event` must parse the #450 bridge
    // payload contract ({card_id: hyphenated uuid, state: serde CardState}) into the
    // state machine — and a non-terminal transition (in_progress) must not settle. Uses
    // the REAL global registry with a unique round, so it also pins register→observe→
    // remove-on-done end to end.
    #[test]
    fn observe_parses_the_bridge_payload_and_removes_the_round_on_done() {
        let ids = cards(1);
        let round_id = Uuid::new_v4();
        register_round(round_id, "humaneval-rs", &ids);
        let in_progress = serde_json::json!({
            "card_id": ids[0].to_string(),
            "state": "in_progress",
            "room_id": round_id.to_string(),
        });
        observe_card_event(&in_progress);
        assert!(
            ROUNDS.lock().unwrap().get(&round_id).is_some_and(|r| r.remaining() == 1),
            "a non-terminal transition must not settle the card"
        );
        let closed = serde_json::json!({
            "card_id": ids[0].to_string(),
            "state": "closed",
            "room_id": round_id.to_string(),
        });
        observe_card_event(&closed);
        assert!(
            ROUNDS.lock().unwrap().get(&round_id).is_none(),
            "the round completed and must be removed — done can never fire twice"
        );
    }

    // what this catches: regression for the #371 no-END zombie — a `Working` round left on
    // disk by a dead core is reaped at boot (evicted + reported with benchmark+remaining),
    // so benchmark/rounds stops counting it in_flight forever (measured live: in_flight: 4
    // with a stopped exam lease). A `Done` file is NOT the reaper's job (load_rounds_in
    // drops those). Idempotent: a second boot reap finds nothing.
    #[test]
    fn working_rounds_survive_the_boot_reap_and_only_ttl_expiry_removes_them() {
        // what this catches: the A5 continuity contract. The old reaper DELETED
        // every Working round at boot — destroying exactly the state (room, cards,
        // assignees, activity rooms) the resume needs to rejoin. A Working round
        // must SURVIVE the reap; only a TTL-stale abandoned file is expired (the
        // state dir's eviction story, 2026-07-13 rule).
        let dir = std::env::temp_dir().join(format!("bench-reap-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let working =
            BenchRound::new(Uuid::new_v4(), "swe-bench-lite", &cards(3), WorkDriver::Citizen);
        persist_round_in(&dir, &working);

        // Default TTL: a fresh Working round is untouchable.
        assert!(
            reap_orphaned_rounds_in(&dir).is_empty(),
            "a live Working round SURVIVES the boot reap — continuity is the default"
        );
        assert_eq!(
            std::fs::read_dir(&dir).expect("readdir").flatten().count(),
            1,
            "the round file is still on disk for the resume to rejoin"
        );

        // TTL zero: the same file is expired (abandoned-round backstop).
        let expired = reap_with_ttl(&dir, std::time::Duration::ZERO);
        assert_eq!(expired, vec![("swe-bench-lite".to_string(), 3)]);
        assert_eq!(
            std::fs::read_dir(&dir).expect("readdir").flatten().count(),
            0,
            "an expired round's file is removed"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
