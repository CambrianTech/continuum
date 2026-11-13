//! Turn bookkeeping: last-turn stamps, in-flight turns and turn shapes.


pub(super) static LAST_TURN_MS: std::sync::LazyLock<
    parking_lot::Mutex<std::collections::HashMap<uuid::Uuid, u64>>,
> = std::sync::LazyLock::new(|| parking_lot::Mutex::new(std::collections::HashMap::new()));

pub(super) static BOOT_MS: std::sync::LazyLock<u64> = std::sync::LazyLock::new(|| {
    continuum_base::time::EpochMs::now().millis() // pre-1970 reads 0: a pre-epoch clock reads as boot at 0 — every mind then counts as starving at most once
});

/// When `persona` last started a turn this boot; `None` = no turn since boot. The
/// dormant order (card 10bba591) reads this: the mind whose last turn is oldest is the
/// first the grid's slack owes a clip.
pub fn last_turn_ms(persona: uuid::Uuid) -> Option<u64> {
    LAST_TURN_MS.lock().get(&persona).copied()
}

/// The instant this core booted, ms — the age a mind with no turn yet is measured from.
pub fn boot_ms() -> u64 {
    *BOOT_MS
}

/// One mind's measured turn shape this boot — fed by [`begin_turn`]'s guard at every
/// turn's end (any exit: spoke, silent, error, a `continue`), read by the placement
/// switch (a move between turns only; a cooldown in her own units) and the dormant
/// clip (the population's typical turn). PURE arithmetic in [`TurnShape::observe`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TurnShape {
    /// EMA of her turn duration, ms (start → end of the service-loop turn).
    pub turn_ms: u64,
    /// EMA of the gap between two of her turn starts, ms — her cadence.
    pub cadence_ms: u64,
    pub turns: u64,
}

impl TurnShape {
    /// Fold one finished turn in: its duration, and the gap since the previous start
    /// (`None` for her first). An EMA with a 1/4 step — a few turns to settle, one
    /// slow turn never the whole story; the first sample of either is taken whole.
    pub fn observe(self, turn_ms: u64, gap_ms: Option<u64>) -> Self {
        let ema = |old: u64, new: u64| if old == 0 { new } else { old - old / 4 + new / 4 };
        Self {
            turn_ms: if self.turns == 0 { turn_ms } else { ema(self.turn_ms, turn_ms) },
            cadence_ms: match gap_ms {
                Some(g) => ema(self.cadence_ms, g),
                None => self.cadence_ms,
            },
            turns: self.turns + 1,
        }
    }
}

pub(super) static TURN_SHAPES: std::sync::LazyLock<
    parking_lot::Mutex<std::collections::HashMap<uuid::Uuid, TurnShape>>,
> = std::sync::LazyLock::new(|| parking_lot::Mutex::new(std::collections::HashMap::new()));
/// One admitted turn's identity, issued by [`begin_turn`]. A mind can hold two turns at once (a
/// fork beside her act), so a turn is keyed by its own id and her persona is its attribute:
/// keyed by persona, two turns counted once and the first to end erased the other's mark (Fable
/// on #5736, card 7471b449).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TurnId(u64);

static NEXT_TURN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A turn in flight as the ledger holds it.
#[derive(Debug, Clone, Copy)]
pub(super) struct Turn {
    pub(super) persona: uuid::Uuid,
    /// The clock [`begin_turn`] was stamped with, ms.
    pub(super) started_ms: u64,
    /// Its obligation is confirmed durable (stop-drain S2): a stop that cuts it leaves a durable
    /// row the next start hands back (S3), so the stop need not wait for it (S4).
    pub(super) confirmed: bool,
}

/// Every turn in flight, by its own id.
pub(super) static TURNS: std::sync::LazyLock<parking_lot::Mutex<std::collections::HashMap<TurnId, Turn>>> =
    std::sync::LazyLock::new(|| parking_lot::Mutex::new(std::collections::HashMap::new()));

/// A turn in flight for `persona`: stamps the start (as [`note_turn_started`]) and marks
/// it IN FLIGHT until dropped — so every exit of the service-loop turn, including a
/// `continue`, ends the turn and records its shape. The placement switch moves a mind
/// only when she holds none (a move mid-turn is a torn turn).
pub struct TurnInFlight {
    id: TurnId,
    persona: uuid::Uuid,
    started: std::time::Instant,
    previous_start_ms: Option<u64>,
}

pub fn begin_turn(persona: uuid::Uuid, now_ms: u64) -> TurnInFlight {
    let previous_start_ms = LAST_TURN_MS.lock().insert(persona, now_ms);
    let id = TurnId(NEXT_TURN.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    TURNS.lock().insert(id, Turn { persona, started_ms: now_ms, confirmed: false });
    TurnInFlight { id, persona, started: std::time::Instant::now(), previous_start_ms }
}

impl TurnInFlight {
    /// This turn's obligation is confirmed durable: from here a stop may cut it, because the
    /// next start hands it back (stop-drain S3), so the stop no longer waits for it (S4).
    pub fn confirm(&self) {
        if let Some(turn) = TURNS.lock().get_mut(&self.id) {
            turn.confirmed = true;
        }
    }
}

impl Drop for TurnInFlight {
    fn drop(&mut self) {
        TURNS.lock().remove(&self.id);
        let turn_ms = self.started.elapsed().as_millis() as u64;
        let gap_ms = self
            .previous_start_ms
            .and_then(|prev| LAST_TURN_MS.lock().get(&self.persona).map(|start| start.saturating_sub(prev)));
        let mut shapes = TURN_SHAPES.lock();
        let shape = shapes.get(&self.persona).copied().unwrap_or_default().observe(turn_ms, gap_ms); // unwrap_or_default: her first turn folds into an empty shape
        shapes.insert(self.persona, shape);
    }
}

/// Is a turn of `persona`'s in flight right now?
pub fn turn_in_flight(persona: uuid::Uuid) -> bool {
    TURNS.lock().values().any(|t| t.persona == persona)
}

/// Every mind with a turn in flight right now, and when her longest-running one started (ms, the
/// clock [`begin_turn`] was stamped with). A resident's service window reads this so a turn that
/// never settles still counts as demand (card 1c157668).
pub fn in_flight_starts() -> Vec<(uuid::Uuid, u64)> {
    let mut earliest: std::collections::HashMap<uuid::Uuid, u64> = std::collections::HashMap::new();
    for turn in TURNS.lock().values() {
        let start = earliest.entry(turn.persona).or_insert(turn.started_ms);
        *start = (*start).min(turn.started_ms);
    }
    earliest.into_iter().collect()
}

/// One turn in flight, as the settle before a stop reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InFlightTurn {
    /// How long it has run, ms.
    pub(crate) elapsed_ms: u64,
    /// Her typical turn, ms; `None` until she has finished one this boot.
    pub(crate) typical_ms: Option<u64>,
    /// Its obligation is confirmed durable (S2), so a stop that cuts it hands it back (S3).
    pub(crate) confirmed: bool,
}

/// Every turn in flight right now, one per turn (a mind with two counts twice). The settle before
/// a stop sizes its wait from the ones not yet confirmed ([`crate::turn_ingress::settle_bound_ms`]).
/// `now_ms` is the clock [`begin_turn`] was stamped with.
pub(crate) fn in_flight_turns(now_ms: u64) -> Vec<InFlightTurn> {
    let turns: Vec<Turn> = TURNS.lock().values().copied().collect();
    turns
        .into_iter()
        .map(|t| InFlightTurn {
            elapsed_ms: now_ms.saturating_sub(t.started_ms),
            typical_ms: turn_shape_of(t.persona).map(|s| s.turn_ms),
            confirmed: t.confirmed,
        })
        .collect()
}

/// How many turns in flight hold a confirmed obligation right now.
pub(crate) fn confirmed_in_flight() -> u64 {
    TURNS.lock().values().filter(|t| t.confirmed).count() as u64
}

/// How many of `persona`'s turns in flight are confirmed. Tests count their own mind's turns, since
/// the global count moves under every other test thread (Fable on #5737).
#[cfg(test)]
pub(crate) fn confirmed_turns_of(persona: uuid::Uuid) -> usize {
    TURNS.lock().values().filter(|t| t.persona == persona && t.confirmed).count()
}

/// Self-ticks in flight: a mind's own initiative (`Wake::Tick` / `Wake::Mind`), which consumes
/// no external input. What it writes is in-memory and recomputed from the world each beat, and
/// its notices are marked read only after its generation, so a stop that cuts one loses nothing
/// a restart does not rebuild (Fable on card 161d2590). It holds the turn gate but no turn mark
/// and no obligation, so the settle counts it here and never waits for it (stop-drain S4b).
static SELF_TICKS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A self-tick in flight until dropped.
#[must_use = "the guard IS the self-tick's mark; dropping it ends the mark"]
pub struct SelfTickInFlight(());

pub fn begin_self_tick() -> SelfTickInFlight {
    SELF_TICKS.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    SelfTickInFlight(())
}

impl Drop for SelfTickInFlight {
    fn drop(&mut self) {
        SELF_TICKS.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

/// How many self-ticks are in flight right now.
pub(crate) fn self_ticks_in_flight() -> u64 {
    SELF_TICKS.load(std::sync::atomic::Ordering::Acquire)
}

/// Her measured turn shape; `None` until a turn has finished this boot.
pub fn turn_shape_of(persona: uuid::Uuid) -> Option<TurnShape> {
    TURN_SHAPES.lock().get(&persona).copied().filter(|s| s.turns > 0)
}

/// The TYPICAL turn among `personas`: the median of their measured turn durations.
/// `None` when nobody has finished a turn — an absence, never a number.
pub fn typical_turn_ms(personas: &[uuid::Uuid]) -> Option<u64> {
    let shapes = TURN_SHAPES.lock();
    let mut turns: Vec<u64> = personas
        .iter()
        .filter_map(|p| shapes.get(p).filter(|s| s.turns > 0).map(|s| s.turn_ms))
        .collect();
    if turns.is_empty() {
        return None;
    }
    turns.sort_unstable();
    Some(turns[turns.len() / 2])
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (Fable on #5736, card 7471b449): a ledger keyed by persona. Two turns of
    // one mind (a fork beside her act) counted once, and the first to end erased the other's
    // mark, so the settle and `awaited()` under-counted while both ran. Each turn is its own
    // entry: ending one leaves the other in flight, confirmed or not, and her start is the
    // longest-running turn's.
    #[test]
    fn two_turns_of_one_mind_are_two_turns() {
        let persona = uuid::Uuid::new_v4();
        let act = begin_turn(persona, 1_000);
        let fork = begin_turn(persona, 2_000);
        act.confirm();
        assert_eq!(confirmed_turns_of(persona), 1, "only the confirmed one counts as confirmed");
        let hers = |turns: &[(uuid::Uuid, u64)]| turns.iter().find(|(p, _)| *p == persona).map(|(_, s)| *s);
        assert_eq!(hers(&in_flight_starts()), Some(1_000), "her start is the longest-running turn's");

        drop(act);
        assert!(turn_in_flight(persona), "ending the confirmed turn leaves the fork in flight");
        assert_eq!(confirmed_turns_of(persona), 0, "and its confirmation ends with it, not the fork's state");
        assert_eq!(hers(&in_flight_starts()), Some(2_000));

        drop(fork);
        assert!(!turn_in_flight(persona), "both ended: she has none");
    }
}
