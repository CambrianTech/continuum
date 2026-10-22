use super::*;

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
pub(super) enum SettleOutcome {
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
    /// When the round was opened (wall ms). Enrichment matches run facts by
    /// instance, so without this floor a fresh card inherited an older round's
    /// run on the same instance and read `resolved` before anyone touched it
    /// (2026-09-05). `#[serde(default)]`: an old round file reads 0 = no floor.
    #[serde(default)]
    pub opened_at_ms: u64,
    pub(super) round_id: Uuid,
    pub(super) benchmark: String,
    /// Card uuid → the terminal state it settled with (`None` = still working).
    pub(super) cards: HashMap<Uuid, Option<String>>,
    pub(super) stage: RoundStage,
    /// Who works this round's cards — read at CLAIM time, decided at dispatch.
    pub(super) driver: WorkDriver,
    /// Card uuid → the ACTIVITY its solve runs in, recorded at first mint so a
    /// re-fire (a resume after a reboot, a retry attempt) REJOINS the same room
    /// instead of minting a stranger — the "unless rejoining" half of the law
    /// (Joel 2026-08-26: "benchmarks without new activities (unless rejoining)").
    /// `#[serde(default)]` so pre-existing round files load with no activities
    /// recorded and mint on their next dispatch.
    #[serde(default)]
    pub(super) card_activities: HashMap<Uuid, CardActivity>,
    /// The ROUND's default team (resolved at dispatch): reviewers who join
    /// every card's solve room. Cards fired by a driver edge BEFORE their
    /// first dispatch have no CardActivity yet — without this, the edge
    /// dispatched them team-less and then RECORDED the empty team (observed
    /// 2026-08-30: card 62f5aee5 fired to Kira with no reviewers, silently
    /// converting a team round to solo — the review event could never fire).
    #[serde(default)]
    pub(super) team: Vec<Uuid>,
    /// The team BY NAME — what a recipe or a dispatch named. The hosting reconciler seats
    /// the union of every WORKING citizen-driven round's team when no operator hold stands
    /// (`roster_hold::active`), so a round staffs itself; an old round file reads empty.
    #[serde(default)]
    pub(super) team_names: Vec<String>,
    /// The coursework lesson set this round teaches, parsed ONCE when the round opens; `None` is
    /// real work. Every reader (the pull order, `coursework_set_of`, so the staged row and the
    /// settle decision) reads this typed value. `default`: an older round file is real work.
    #[serde(default)]
    pub(super) coursework: Option<crate::cognition::coursework::CourseworkSet>,
    /// Card uuid → the citizen it was staged FOR, recorded at dispatch staging
    /// (before any solve fires) so the follow-on driver ([`next_unworked_after`])
    /// and the boot resume know WHO works a card that has never run.
    #[serde(default)]
    pub(super) card_assignees: HashMap<Uuid, Uuid>,
    /// Card uuid → the INSTANCE it was staged for, recorded at dispatch
    /// staging. Before this the tracker only learned WHAT a card tested when
    /// its solve activity minted a room name — so an unstarted card's
    /// roll-call row was blank, and an in-flight run could not join back to
    /// its card (2026-09-01: Kira's live django-14349 run left its round
    /// pronounced `unstarted`).
    #[serde(default)]
    pub(super) card_instances: HashMap<Uuid, String>,
    /// Card uuid → epoch-ms of the LATEST held-work turn a citizen drove on it. The
    /// freshness signal a CITIZEN-driven card emits, which the detached-solve run
    /// ledger never sees. DURABLE (save-on-write with the round) since 2026-09-03:
    /// it used to be a process-global map that a reboot emptied, so every citizen
    /// round read "unstarted" after a restart and the standing autopilot minted a
    /// duplicate round per tick (30 duplicate verified-mini rounds measured). Resume
    /// is read-the-saved-state, never re-derive.
    #[serde(default)]
    pub(super) card_last_act_ms: HashMap<Uuid, u64>,
    /// THE IDLE CLOCK (card deb26770's kin; Joel 2026-09-14: "waiting three hours for
    /// something to fix itself is insane"): when a claimed card was first seen with
    /// no act, by which holder. Persisted with the round (file + room bundle) so a
    /// reboot never restarts it; an act clears it; a new holder restarts it.
    #[serde(default)]
    pub(super) card_idle_since: HashMap<Uuid, IdleMark>,
    /// THE REVIEW GATE (team-recipe Arm 2, 2026-09-03): when set, an owner's `done`
    /// on a card of this round becomes `review` + a sibling review card that a
    /// NON-owner pulls; only the reviewer's `done` closes the parent (and fires its
    /// grade). The one structural intervention the literature found moved SWE
    /// scores — a fresh-context, execution-grounded verdict — as recipe DATA.
    #[serde(default)]
    pub(super) review_gate: bool,
    /// Review card → the parent card it reviews. A review card is round work (it is
    /// pullable) but not a round CARD (it never counts toward the round's remaining).
    #[serde(default)]
    pub(super) review_cards: HashMap<Uuid, Uuid>,
    /// Parents whose review passed — their next `done` goes through.
    #[serde(default)]
    pub(super) reviews_passed: std::collections::HashSet<Uuid>,
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
            opened_at_ms: EpochMs::now().millis(),
            benchmark: benchmark.to_string(),
            cards: card_ids.iter().map(|c| (*c, None)).collect(),
            stage: RoundStage::Working,
            driver,
            card_activities: HashMap::new(),
            card_assignees: HashMap::new(),
            card_instances: HashMap::new(),
            card_last_act_ms: HashMap::new(),
            card_idle_since: HashMap::new(),
            review_gate: false,
            review_cards: HashMap::new(),
            reviews_passed: Default::default(),
            team: Vec::new(),
            team_names: Vec::new(),
            coursework: crate::cognition::coursework::CourseworkSet::parse(benchmark),
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
    /// Undo a settle: the card's slot returns to unsettled and a round the settle
    /// finished returns to Working. True when the tracker held it settled.
    pub(super) fn reopen_card(&mut self, card: Uuid) -> bool {
        match self.cards.get_mut(&card) {
            Some(slot @ Some(_)) => {
                *slot = None;
                if self.stage == RoundStage::Done {
                    self.stage = RoundStage::Working;
                }
                true
            }
            _ => false,
        }
    }

    pub(super) fn settle_card(&mut self, card: Uuid, state: &str) -> SettleOutcome {
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
pub(super) static ROUNDS: LazyLock<Mutex<HashMap<Uuid, BenchRound>>> =
    LazyLock::new(|| Mutex::new(load_rounds_in(&rounds_state_dir())));

/// Stamp that a citizen drove a held-work turn on `card_id` at `now_ms` — called
/// from the held-work turn boundary so `enrich_rounds` can see Citizen progress.
/// Persisted with the round: freshness survives the seam. A card outside any live
/// round records nothing (there is no round to read it back for).
/// When a holder's actless claim was first seen (persisted on the round).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IdleMark {
    pub holder: Uuid,
    pub since_ms: u64,
}

/// The instant an idle claim's clock runs from: the holder's last act on the card
/// when there is one; else the first pass that saw THIS holder's claim actless —
/// recorded on the round and persisted, so a reboot does not restart it and a new
/// holder gets a fresh clock. Never the board row's `updated_at_ms`, which lease
/// heartbeats refresh.
pub fn idle_clock_for(card_id: Uuid, holder: Uuid, now_ms: u64) -> u64 {
    let mut rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner()); // JUSTIFIED unwrap_or_else: poisoned lock = read the last state, same policy as every ROUNDS lock
    let Some(round) = rounds.values_mut().find(|r| r.cards.contains_key(&card_id)) else {
        return now_ms;
    };
    if let Some(act) = round.card_last_act_ms.get(&card_id).copied() {
        if round.card_idle_since.remove(&card_id).is_some() {
            persist_round_in(&rounds_state_dir(), round);
        }
        return act;
    }
    match round.card_idle_since.get(&card_id).copied() {
        Some(mark) if mark.holder == holder => mark.since_ms,
        _ => {
            round.card_idle_since.insert(card_id, IdleMark { holder, since_ms: now_ms });
            persist_round_in(&rounds_state_dir(), round);
            now_ms
        }
    }
}

pub fn record_card_worked(card_id: Uuid, now_ms: u64) {
    let mut rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner());  // poisoned lock = read the last state, same policy as every ROUNDS lock
    if let Some(round) = rounds.values_mut().find(|r| r.cards.contains_key(&card_id)) {
        round.card_last_act_ms.insert(card_id, now_ms);
        round.card_idle_since.remove(&card_id);
        persist_round_in(&rounds_state_dir(), round);
    }
}

/// The epoch-ms of the latest held-work turn on `card_id`, if any — the Citizen
/// freshness [`enrich_rounds`] merges beside the detached run ledger.
/// Every card of a WORKING round the tracker still counts unsettled: (round id —
/// which IS the run room id — card, instance). The reconciler reads the board for
/// each and settles what the board already settled (a close the tracker missed
/// across a seam) or returns idle claims to the deck.
pub fn unsettled_cards() -> Vec<(Uuid, Uuid, String)> {
    let rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner()); // JUSTIFIED unwrap_or_else: poisoned lock = read the last state, same policy as every ROUNDS lock
    let mut out = Vec::new();
    for r in rounds.values() {
        if r.stage != RoundStage::Working {
            continue;
        }
        for (card, settled) in &r.cards {
            if settled.is_none() {
                let instance = r.card_instances.get(card).cloned().unwrap_or_default(); // JUSTIFIED unwrap_or_default: a card dispatched before instances were recorded has no name, only an id
                out.push((r.round_id, *card, instance));
            }
        }
    }
    out
}

pub fn card_last_act_ms(card_id: Uuid) -> Option<u64> {
    ROUNDS
        .lock()
        .unwrap_or_else(|p| p.into_inner())  // poisoned lock = read the last state, same policy as every ROUNDS lock
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
pub(super) fn rounds_state_dir() -> std::path::PathBuf {
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
/// THE BUNDLE (card c9ddb911, slice 2): every persisted change to a round also marks
/// it dirty for the ACTIVITY BUNDLE — the round's saved instance state on its own
/// room (`experience::activity_state`, kind `benchmark/round`, key
/// [`ROUND_BUNDLE_KEY`]). The reconciler drains the set each pass and publishes;
/// a node that never held the round restores it from the room alone
/// ([`restore_round_from_bundle`]). The file under `state/bench-rounds/` stays as
/// this process's fast path; the room is the truth that crosses a seam.
pub(super) static BUNDLE_DIRTY: LazyLock<Mutex<std::collections::HashSet<Uuid>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashSet::new()));

/// The bundle key the round's serialized state lives under.
pub const ROUND_BUNDLE_KEY: &str = "round";

/// A round whose bundle is behind its state — what the reconciler publishes.
pub struct DirtyRound {
    pub round_id: Uuid,
    pub run_room_name: String,
    pub json: Value,
}

pub(super) fn mark_bundle_dirty(round_id: Uuid) {
    BUNDLE_DIRTY
        .lock()
        .unwrap_or_else(|p| p.into_inner()) // JUSTIFIED unwrap_or_else: a poisoned set still marks — same policy as every ROUNDS lock
        .insert(round_id);
}

/// Put a round back on the dirty set (its publish failed; the next pass retries).
pub fn requeue_bundle_dirty(round_id: Uuid) {
    mark_bundle_dirty(round_id);
}

/// Drain the dirty set and serialize each round under the tracker lock. Lock order
/// is DIRTY (dropped) then ROUNDS — the marker holds ROUNDS while taking DIRTY, so
/// never the reverse here. A round with no run room yet stays dirty (nowhere to
/// publish); a round the tracker no longer knows is dropped.
pub fn take_dirty_bundles() -> Vec<DirtyRound> {
    let ids: Vec<Uuid> = {
        let mut set = BUNDLE_DIRTY.lock().unwrap_or_else(|p| p.into_inner()); // JUSTIFIED unwrap_or_else: a poisoned set drains what it has
        set.drain().collect()
    };
    if ids.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut homeless = Vec::new();
    {
        let rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner()); // JUSTIFIED unwrap_or_else: poisoned lock = read the last state, same policy as every ROUNDS lock
        for id in ids {
            let Some(round) = rounds.get(&id) else { continue };
            if round.run_room_name.is_empty() {
                homeless.push(id);
                continue;
            }
            match serde_json::to_value(round) {
                Ok(json) => out.push(DirtyRound { round_id: id, run_room_name: round.run_room_name.clone(), json }),
                Err(e) => tracing::warn!(round = %id, error = %e, "bench round not serializable — its bundle stays behind"),
            }
        }
    }
    for id in homeless {
        mark_bundle_dirty(id);
    }
    out
}

/// Restore a round from its bundle (the value under [`ROUND_BUNDLE_KEY`]). Returns
/// `Ok(Some(id))` when the tracker learned a round it did not know, `Ok(None)` when
/// it already knew it (this process's own state wins — it is newer or equal), `Err`
/// for a bundle that is not a round.
pub fn restore_round_from_bundle(bundle: &Value) -> Result<Option<Uuid>, String> {
    let round: BenchRound = serde_json::from_value(bundle.clone()).map_err(|e| format!("bundle is not a round: {e}"))?; // ORM boundary: the bundle is the round's own serialization, read back across a seam
    let mut rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner()); // JUSTIFIED unwrap_or_else: poisoned lock = read the last state, same policy as every ROUNDS lock
    if rounds.contains_key(&round.round_id) {
        return Ok(None);
    }
    let id = round.round_id;
    continuum_base::probe!(
        class = "activity.state.restored",
        kind = "benchmark/round",
        round_id = %id,
        room = %round.run_room_name,
        stage = ?round.stage,
        remaining = round.remaining(),
        "round restored from its room's bundle — this node never held it"
    );
    persist_file_only(&rounds_state_dir(), &round);
    rounds.insert(id, round);
    Ok(Some(id))
}

pub(super) fn persist_round_in(dir: &Path, round: &BenchRound) {
    mark_bundle_dirty(round.round_id);
    persist_file_only(dir, round);
}

/// The file half alone — what a restore writes (a restored bundle is already
/// published; marking it dirty would republish it unchanged).
pub(super) fn persist_file_only(dir: &Path, round: &BenchRound) {
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
pub(super) fn remove_round_file_in(dir: &Path, round_id: Uuid) {
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
pub(super) fn load_rounds_in(dir: &Path) -> HashMap<Uuid, BenchRound> {
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
        continuum_base::probe!(
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
/// continuity-is-the-default). A `Working` round on disk is STILL WORKING: its
/// room and cards are the durable state, and residents pull from them as they choose
/// (no runner re-fires anything at boot). The old behavior — deleting
/// every Working round at boot — destroyed exactly the state a resume needs
/// (room, cards, assignees, per-card activity rooms) and made "benchmarks never
/// start themselves" structural. What remains here is the eviction story this
/// state dir owes (2026-07-13 rule): a round file older than [`ROUND_TTL`] is
/// abandoned and expires. Returns the expired `(benchmark, remaining)` rows for
/// the boot probe. Still never re-dispatches from the serving daemon.
pub fn reap_orphaned_rounds() -> Vec<(String, usize)> {
    let dir = rounds_state_dir();
    // TTL expiry only (see [`reap_orphaned_rounds_in`]): a Working round at boot
    // SURVIVES — it is the durable state residents pull from. Expired files are also dropped from the loaded map so live_rounds()
    // agrees this boot.
    let expired = reap_orphaned_rounds_in(&dir);
    if !expired.is_empty() {
        let dead: std::collections::HashSet<&String> =
            expired.iter().map(|(b, _)| b).collect();
        ROUNDS
            .lock()
            .unwrap_or_else(|p| p.into_inner())  // poisoned lock = read the last state, same policy as every ROUNDS lock
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
pub(super) const ROUND_TTL: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 3600);

pub(super) fn reap_orphaned_rounds_in(dir: &Path) -> Vec<(String, usize)> {
    reap_with_ttl(dir, ROUND_TTL)
}

/// TTL injectable so the expiry contract is testable without clock games.
pub(super) fn reap_with_ttl(dir: &Path, ttl: std::time::Duration) -> Vec<(String, usize)> {
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
    let mut rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner());  // poisoned lock = read the last state, same policy as every ROUNDS lock
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
    let mut rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner());  // poisoned lock = read the last state, same policy as every ROUNDS lock
    if let Some(round) = rounds.get_mut(&round_id) {
        round.run_room_name = name.to_string();
        persist_round_in(&rounds_state_dir(), round);
    }
}

/// The instances with an OPEN card (not yet settled in the tracker) in any WORKING round
/// on this node. A new round must not draw them again: staging reuses the checkout by
/// instance, so two rounds would work one tree and grade one verdict (2026-09-13:
/// pytest-7236 and astropy-13453 sat in seed-4 and in a paused duplicate round at once).
pub fn instances_open_in_working_rounds() -> std::collections::HashSet<String> {
    let rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner()); // poisoned lock = read the last state, same policy as every ROUNDS lock
    let mut out = std::collections::HashSet::new();
    for r in rounds.values() {
        if r.stage != RoundStage::Working {
            continue;
        }
        for (card, state) in &r.cards {
            if state.is_none() {
                if let Some(inst) = r.card_instances.get(card) {
                    out.insert(inst.clone());
                }
            }
        }
    }
    out
}

/// Record the team a round was born with, by name (see `team_names`).
pub fn set_round_team_names(round_id: Uuid, names: Vec<String>) {
    let mut rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner()); // poisoned lock = read the last state, same policy as every ROUNDS lock
    if let Some(r) = rounds.get_mut(&round_id) {
        r.team_names = names;
        persist_round_in(&rounds_state_dir(), r);
    }
}

pub fn set_round_team(round_id: Uuid, team: Vec<Uuid>) {
    let mut rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner());  // poisoned lock = read the last state, same policy as every ROUNDS lock
    if let Some(r) = rounds.get_mut(&round_id) {
        r.team = team;
        persist_round_in(&rounds_state_dir(), r);
    }
}

/// Turn the round's REVIEW GATE on (dispatch calls it right after [`open_round`]).
pub fn set_review_gate(round_id: Uuid, on: bool) {
    let mut rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner());  // poisoned lock = read the last state, same policy as every ROUNDS lock
    if let Some(r) = rounds.get_mut(&round_id) {
        r.review_gate = on;
        persist_round_in(&rounds_state_dir(), r);
    }
}

/// Must an owner's `done` on `card` go through review first? True iff the card's
/// round is gated and no review has passed for it yet.
pub fn review_required(card: Uuid) -> bool {
    ROUNDS
        .lock()
        .unwrap_or_else(|p| p.into_inner())  // poisoned lock = read the last state, same policy as every ROUNDS lock
        .values()
        .find(|r| r.cards.contains_key(&card))
        .is_some_and(|r| r.review_gate && !r.reviews_passed.contains(&card))
}

/// Record the sibling review card of `parent` (posted by the gate). Returns the
/// round id it joined, or `None` when the parent is in no live round.
pub fn register_review_card(parent: Uuid, review: Uuid) -> Option<Uuid> {
    let mut rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner());  // poisoned lock = read the last state, same policy as every ROUNDS lock
    let r = rounds.values_mut().find(|r| r.cards.contains_key(&parent))?;
    r.review_cards.insert(review, parent);
    persist_round_in(&rounds_state_dir(), r);
    Some(r.round_id)
}
