use super::*;

/// Where a round is in its life: `Working` until every card in its set has reached a
/// terminal card state, then `Done` (and the round leaves the tracker).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundStage {
    Working,
    Done,
}

/// Terminal card states — "this card's work is finished". The vocabulary is the serde
/// snake_case form of `airc_lib::CardState` as carried by the `work.card.state_changed`
/// payload: `work/state` maps done|closed → Closed and accepts merged. ONE truth for
/// "terminal": `modules::benchmark_grade::is_terminal` delegates here.
pub fn is_terminal_card_state(state: &str) -> bool {
    matches!(
        state.to_ascii_lowercase().as_str(),
        "closed" | "done" | "merged"
    )
}

/// What one observed card-settle did to a round, read off the type by the probe
/// emission and the tests instead of re-derived from counters.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum SettleOutcome {
    /// The card is not in this round's set — not ours, ignore.
    NotOurs,
    /// The card was already settled (duplicate terminal event) — no state change.
    AlreadySettled,
    /// The card settled; `remaining` cards are still working.
    Settled { remaining: usize },
    /// This settle was the LAST one — the round just transitioned Working → Done.
    RoundDone,
}

/// One round: a card set tracked from open (Working) to all-cards-terminal (Done).
/// Identity is the room's uuid — a round is its room's activity.
pub(super) struct BenchRound {
    pub(super) round_id: Uuid,
    pub(super) benchmark: String,
    /// The room's airc NAME (joins are by name).
    pub(super) run_room_name: String,
    /// Card uuid → the terminal state it settled with (`None` = still working).
    pub(super) cards: HashMap<Uuid, Option<String>>,
    pub(super) stage: RoundStage,
    /// Card uuid → the activity its work runs in.
    pub(super) card_activities: HashMap<Uuid, CardActivity>,
    /// Card uuid → the instance it tests.
    pub(super) card_instances: HashMap<Uuid, String>,
}

/// Where one card's work LIVES: its activity room, the citizen working it, and the
/// teammates joined to that room.
#[derive(Debug, Clone)]
pub struct CardActivity {
    pub solve_room: Uuid,
    /// The room's airc NAME (joins are by name, never UUID-as-string).
    pub room_name: String,
    pub assignee: Uuid,
    /// Room-members beyond the claimer.
    pub teammates: Vec<Uuid>,
}

impl BenchRound {
    pub(super) fn new(round_id: Uuid, benchmark: &str, card_ids: &[Uuid]) -> Self {
        Self {
            round_id,
            benchmark: benchmark.to_string(),
            run_room_name: String::new(),
            cards: card_ids.iter().map(|c| (*c, None)).collect(),
            stage: RoundStage::Working,
            card_activities: HashMap::new(),
            card_instances: HashMap::new(),
        }
    }

    /// Cards not yet settled.
    pub(super) fn remaining(&self) -> usize {
        self.cards.values().filter(|s| s.is_none()).count()
    }

    /// Total cards in the round.
    pub(super) fn dispatched(&self) -> usize {
        self.cards.len()
    }

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

    /// Record that `card` reached terminal `state`. A card outside the set is `NotOurs`;
    /// a duplicate settle is `AlreadySettled` (never double-counted); the settle that
    /// empties the set transitions the round to `Done` exactly once.
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

/// The live rounds, keyed by round (= room) id. Sync `Mutex`, never held across an
/// await. A round is REMOVED the moment it completes, so the map only ever holds
/// in-flight rounds.
static ROUNDS: LazyLock<Mutex<HashMap<Uuid, BenchRound>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

pub(super) fn rounds() -> std::sync::MutexGuard<'static, HashMap<Uuid, BenchRound>> {
    ROUNDS.lock().unwrap_or_else(|p| p.into_inner()) // poisoned lock = read the last state; every touch is a short pure mutation
}

/// Open a round for `round_id`. Idempotent by round id.
pub fn open_round(round_id: Uuid, benchmark: &str) {
    rounds()
        .entry(round_id)
        .or_insert_with(|| BenchRound::new(round_id, benchmark, &[]));
        }

/// Add a card to an open round. Unknown round = no-op.
pub fn add_card(round_id: Uuid, card_id: Uuid) {
    if let Some(r) = rounds().get_mut(&round_id) {
        r.cards.entry(card_id).or_insert(None);
    }
}

/// Close the posting phase. A round with zero cards ends at once — an honest empty
/// round, never a map entry no event can ever settle.
pub fn seal_round(round_id: Uuid) {
    let mut rounds = rounds();
    let Some(round) = rounds.get(&round_id) else {
        return;
    };
    if round.dispatched() == 0 {
        let benchmark = round.benchmark.clone();
        rounds.remove(&round_id);
        continuum_base::probe!(
            class = "bench.round.done",
            round_id = %round_id,
            benchmark = %benchmark,
            dispatched = 0usize,
            settled = 0usize,
            "benchmark round END — nothing was posted"
        );
    }
}

/// The round ROOM a card belongs to. `None` for a card outside every tracked round.
pub fn room_for_card(card_id: Uuid) -> Option<Uuid> {
    rounds()
        .values()
        .find(|r| r.cards.contains_key(&card_id))
        .map(|r| r.round_id)
}

/// The run room of the round whose work runs in `room` — the tree edge the nav rail
/// renders. `None` = no tracked round works there.
pub fn run_room_for_solve(room: Uuid) -> Option<Uuid> {
    rounds()
        .values()
        .find(|r| r.card_activities.values().any(|a| a.solve_room == room))
        .map(|r| r.round_id)
}

/// The team standing in an activity room. `None` = no tracked round works there.
pub fn team_for_room(room: Uuid) -> Option<CardActivity> {
    rounds()
        .values()
        .flat_map(|r| r.card_activities.values())
        .find(|act| act.solve_room == room)
        .cloned()
    }

/// Every card, across live rounds, that carries `instance`.
pub fn cards_for_instance(instance: &str) -> Vec<Uuid> {
    rounds()
        .values()
        .flat_map(|r| {
            r.card_instances
                .iter()
                .filter(|(_, i)| i.as_str() == instance)
                .map(|(c, _)| *c)
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Record WHAT a card tests.
pub fn record_card_instance(card_id: Uuid, instance: &str) {
    if let Some(round) = rounds().values_mut().find(|r| r.cards.contains_key(&card_id)) {
        round.card_instances.insert(card_id, instance.to_string());
    }
}
