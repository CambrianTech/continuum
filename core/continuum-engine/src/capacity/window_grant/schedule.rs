//! A turn schedule played against the published grants: busy minds taking turns on what each tick
//! grants them, every turn's start admitted through [`PoolGrants::grant_for`] with her ended-turn
//! count, as a real turn's window is. The service guarantee's tests run it at the policy and at the
//! serving daemon's seam (Codex: "add both adversarial schedules to the actual policy/daemon seam
//! tests"). A fixture: production never links it.

use std::collections::BTreeMap;

use super::{PoolGrants, TurnsEnded};

/// One busy mind: she always wants a turn, and each runs `turn_ticks` grant ticks.
#[derive(Debug, Clone, Copy)]
pub struct Busy {
    pub mind: uuid::Uuid,
    pub turn_ticks: u32,
}

/// When a mind whose turn just ended starts her next one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restart {
    /// At once, between two grant ticks: every tick samples her in a turn (Codex's schedule 2).
    BetweenTicks,
    /// After the next grant tick has seen her idle.
    AfterATick,
}

/// What one mind looks like to a grant tick.
#[derive(Debug, Clone, Copy)]
pub struct Seen {
    pub mind: uuid::Uuid,
    /// In a turn: running, or started and waiting for her window.
    pub in_flight: bool,
    pub turns_ended: TurnsEnded,
}

/// What the schedule saw.
#[derive(Debug, Default)]
pub struct Served {
    /// Turns each mind ended.
    pub turns: BTreeMap<uuid::Uuid, u64>,
    /// The longest any mind wanted a turn without one, counted in turn endings of the others.
    pub longest_wait: u64,
    /// Ticks whose grants summed past the pool.
    pub over_pool: u32,
    /// Running turns whose grant shrank.
    pub shrunk: u32,
}

#[derive(Debug, Clone, Copy)]
enum State {
    Running { left: u32 },
    /// Wants a turn since the schedule had seen `since` turn endings.
    Waiting { since: u64 },
    Resting,
}

/// Play `minds` for `ticks` grant ticks; `tick` is one grant: what the minds look like in, the
/// published grants out.
pub fn play(minds: &[Busy], restart: Restart, ticks: u32, mut tick: impl FnMut(&[Seen]) -> PoolGrants) -> Served {
    let mut state: BTreeMap<uuid::Uuid, (State, u64)> = minds.iter().map(|m| (m.mind, (State::Waiting { since: 0 }, 0))).collect();
    let turn_ticks: BTreeMap<uuid::Uuid, u32> = minds.iter().map(|m| (m.mind, m.turn_ticks)).collect();
    let mut endings = 0u64;
    let mut before: BTreeMap<uuid::Uuid, u32> = BTreeMap::new();
    let mut served = Served::default();
    for _ in 0..ticks {
        let seen: Vec<Seen> = state
            .iter()
            .map(|(&mind, &(s, ended))| Seen { mind, in_flight: !matches!(s, State::Resting), turns_ended: TurnsEnded(ended) })
            .collect();
        let grants = tick(&seen);
        if grants.by_mind.values().map(|&g| u64::from(g)).sum::<u64>() > u64::from(grants.pool_tokens) {
            served.over_pool += 1;
        }
        for (mind, (s, _)) in &state {
            if matches!(s, State::Running { .. }) && grants.by_mind.get(mind) < before.get(mind) {
                served.shrunk += 1;
            }
        }
        // Turns end first, then turns start on the grants just published.
        for (mind, (s, ended)) in state.iter_mut() {
            *s = match *s {
                State::Running { left: 1 } => {
                    *ended += 1;
                    endings += 1;
                    *served.turns.entry(*mind).or_default() += 1;
                    match restart {
                        Restart::BetweenTicks => State::Waiting { since: endings },
                        Restart::AfterATick => State::Resting,
                    }
                }
                State::Running { left } => State::Running { left: left - 1 },
                State::Resting => State::Waiting { since: endings },
                waiting @ State::Waiting { .. } => waiting,
            };
        }
        for (mind, (s, ended)) in state.iter_mut() {
            if let State::Waiting { since } = *s {
                if grants.grant_for(*mind, TurnsEnded(*ended)).is_some() {
                    served.longest_wait = served.longest_wait.max(endings - since);
                    *s = State::Running { left: turn_ticks.get(mind).copied().unwrap_or(1) }; // unwrap_or: every mind played has her turn length
                }
            }
        }
        before = grants.by_mind;
    }
    for (s, _) in state.values() {
        if let State::Waiting { since } = *s {
            served.longest_wait = served.longest_wait.max(endings - since);
        }
    }
    served
}
