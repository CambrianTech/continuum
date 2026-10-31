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
pub(super) static IN_FLIGHT: std::sync::LazyLock<parking_lot::Mutex<std::collections::HashSet<uuid::Uuid>>> =
    std::sync::LazyLock::new(|| parking_lot::Mutex::new(std::collections::HashSet::new()));

/// A turn in flight for `persona`: stamps the start (as [`note_turn_started`]) and marks
/// her IN FLIGHT until dropped — so every exit of the service-loop turn, including a
/// `continue`, ends the turn and records its shape. The placement switch moves a mind
/// only when this is not held (a move mid-turn is a torn turn).
pub struct TurnInFlight {
    persona: uuid::Uuid,
    started: std::time::Instant,
    previous_start_ms: Option<u64>,
}

pub fn begin_turn(persona: uuid::Uuid, now_ms: u64) -> TurnInFlight {
    let previous_start_ms = LAST_TURN_MS.lock().insert(persona, now_ms);
    IN_FLIGHT.lock().insert(persona);
    TurnInFlight { persona, started: std::time::Instant::now(), previous_start_ms }
}

impl Drop for TurnInFlight {
    fn drop(&mut self) {
        IN_FLIGHT.lock().remove(&self.persona);
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
    IN_FLIGHT.lock().contains(&persona)
}

/// Every mind with a turn in flight right now, and when that turn started (ms, the clock
/// [`begin_turn`] was stamped with). A resident's service window reads this so a turn that
/// never settles still counts as demand (card 1c157668). Locks taken one at a time.
pub fn in_flight_starts() -> Vec<(uuid::Uuid, u64)> {
    let personas: Vec<uuid::Uuid> = IN_FLIGHT.lock().iter().copied().collect();
    let starts = LAST_TURN_MS.lock();
    // begin_turn stamps the start before marking in flight, so a mind in flight has a start.
    personas.into_iter().filter_map(|p| starts.get(&p).map(|s| (p, *s))).collect()
}

/// Every turn in flight right now, as (ms it has run, her typical turn ms — `None` until
/// she has finished one this boot). The settle before a stop sizes its wait from this
/// ([`crate::turn_ingress::settle_bound_ms`]). `now_ms` is the clock
/// [`begin_turn`] was stamped with. Locks taken one at a time, never nested.
pub(crate) fn in_flight_turns(now_ms: u64) -> Vec<(u64, Option<u64>)> {
    let personas: Vec<uuid::Uuid> = IN_FLIGHT.lock().iter().copied().collect();
    let starts = LAST_TURN_MS.lock();
    let started: Vec<(uuid::Uuid, u64)> = personas
        .into_iter()
        .map(|p| (p, starts.get(&p).map_or(0, |s| now_ms.saturating_sub(*s)))) // map_or: begin_turn stamps the start before marking in flight, so a miss is a race at most — 0 elapsed waits LONGER, never shorter
        .collect();
    drop(starts);
    started
        .into_iter()
        .map(|(p, elapsed)| (elapsed, turn_shape_of(p).map(|s| s.turn_ms)))
        .collect()
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
