//! The device's room for minds' KV, in simulation: the Streamer's law priced against a lane (UNIFIED-PAGING.md,
//! "Streamer"; continuum-blocks/src/streamer.rs is the live one). A copy happens only because the room cannot hold
//! every mind: while it can, a switch is a pointer on any device, discrete or unified.
//!
//! - The mind the lane runs is resident. Room for another is made by parking a resident mind not running, the one
//!   predicted to be needed latest (her pages go out by DMA overlapped with decode, so parking costs the lane
//!   nothing here; her source is counted free when she parks).
//! - A parked mind comes back through the one link, one move at a time, her destination reserved when it starts.
//! - When each parked mind's move starts is the prefetch decision ([`crate::decide::cases::prefetch`]), learned from
//!   what each move measured: the lane waiting on her (late), or her landed and unused (held early).

use std::collections::HashMap;
use std::hash::Hash;
use std::num::NonZeroU64;
use std::time::Duration;

use crate::decide::cases::prefetch::{Action, Outcome, Prefetch, Situation};
use crate::decide::{Decided, Decider, DecisionId, Measured};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Where {
    Resident { since: Duration },
    Moving { started: Duration, lands: Duration },
    Parked,
}

/// One mind's KV, as the room sees it.
struct Held {
    bytes: u64,
    at: Where,
    /// Her copy, once a move of hers measured it.
    copy: Option<Duration>,
    /// When she last left the lane, and how long she has measured away (left to needed) and how wrong that was.
    left: Option<Duration>,
    away: Option<Duration>,
    spread: Option<Duration>,
    /// Her open prefetch decision: when it starts her move, and what it decided on.
    plan: Option<(DecisionId, Duration)>,
}

/// What bringing a mind in cost the lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Brought {
    /// The lane waited this long for her move.
    pub waited: Duration,
    /// Her copy, when a move of hers landed for this switch: what the lane's slice floor learns.
    pub fetched: Option<Duration>,
}

/// The device's KV room and its one link.
pub struct Room<K> {
    capacity: NonZeroU64,
    minds: HashMap<K, Held>,
    link_free: Duration,
    bound: Duration,
}

const SMOOTHING: f64 = 0.25;

fn smooth(old: Option<Duration>, sample: Duration) -> Duration {
    old.map_or(sample, |old| old.mul_f64(1.0 - SMOOTHING) + sample.mul_f64(SMOOTHING)) // map_or: a first sample is the mean
}

impl<K: Copy + Eq + Hash + Ord> Room<K> {
    /// `capacity` bytes of KV on the device (`NonZeroU64::MAX`: room for every mind), each mind's `bytes`, and the lane's
    /// wait bound every decision reads its times against. Every mind starts resident while she fits, else parked.
    pub fn new(capacity: NonZeroU64, minds: impl IntoIterator<Item = (K, u64)>, bound: Duration) -> Self {
        let mut used = 0u64;
        let mut room = Self { capacity, minds: HashMap::new(), link_free: Duration::ZERO, bound };
        for (mind, bytes) in minds {
            let fits = used.saturating_add(bytes) <= capacity.get();
            if fits {
                used = used.saturating_add(bytes);
            }
            let at = if fits { Where::Resident { since: Duration::ZERO } } else { Where::Parked };
            room.minds.insert(mind, Held { bytes, at, copy: None, left: None, away: None, spread: None, plan: None });
        }
        room
    }

    fn used(&self) -> u64 {
        self.minds.values().filter(|h| !matches!(h.at, Where::Parked)).map(|h| h.bytes).sum()
    }

    /// When she is predicted to be needed: her leaving plus her measured time away.
    fn predicted(&self, mind: &K) -> Option<Duration> {
        let held = self.minds.get(mind)?;
        Some(held.left? + held.away?)
    }

    /// Park resident minds, or abandon moves in flight (her block stays parked, her destination freed; the link stays
    /// busy until the copy it started ends), never `keep`, until `bytes` fit: latest predicted need first, and only
    /// those needed later than `by` (a mind needed sooner than the one making room keeps her place). True if they fit.
    ///
    /// A victim's open prefetch closes here with what it wasted, so the learner hears both ways: a move abandoned in
    /// flight wasted its whole copy of link time, a prefetch landed and parked unused wasted the time it sat.
    fn make_room(&mut self, bytes: u64, keep: &[K], by: Option<Duration>, now: Duration, prefetch: &mut Decider<Prefetch>) -> bool {
        loop {
            if self.used().saturating_add(bytes) <= self.capacity.get() {
                return true;
            }
            let victim = self
                .minds
                .iter()
                .filter(|(k, h)| !keep.contains(k) && !matches!(h.at, Where::Parked))
                .map(|(k, _)| (*k, self.predicted(k)))
                .filter(|(_, need)| match (by, need) {
                    (Some(by), Some(need)) => *need > by,
                    _ => true, // an unpredicted need, on either side, is no reason to keep her
                })
                .max_by_key(|(k, need)| (need.unwrap_or(Duration::MAX), *k)); // unwrap_or: an unpredicted need is latest
            let Some((victim, _)) = victim else { return false };
            let (capacity, bound) = (self.capacity, self.bound);
            if let Some(held) = self.minds.get_mut(&victim) {
                let wasted = match held.at {
                    Where::Moving { started, lands } => lands.saturating_sub(started),
                    Where::Resident { since } => now.saturating_sub(since),
                    Where::Parked => Duration::ZERO,
                };
                if let Some((id, _)) = held.plan.take() {
                    prefetch.outcome(id, Outcome { late: Duration::ZERO, wasted, bytes: held.bytes, capacity, bound });
                }
                held.at = Where::Parked;
            }
        }
    }

    fn start(&mut self, mind: K, now: Duration, copy: Duration) -> Duration {
        let lands = now.max(self.link_free) + copy;
        self.link_free = lands;
        if let Some(held) = self.minds.get_mut(&mind) {
            held.at = Where::Moving { started: now, lands };
            held.plan = held.plan.map(|(id, _)| (id, now)); // started: its plan is spent
        }
        lands
    }

    /// The lane brings `mind` in at `now`: her move's remaining wait (none if she is resident), the decision on her
    /// move closed with what it measured, and her time away learned.
    pub fn bring_in(&mut self, mind: K, now: Duration, copy_of: impl Fn(u64) -> Duration, prefetch: &mut Decider<Prefetch>) -> Brought {
        let Some(held) = self.minds.get_mut(&mind) else { return Brought { waited: Duration::ZERO, fetched: None } };
        if let Some(left) = held.left.take() {
            let away = now.saturating_sub(left);
            if let Some(predicted) = held.away {
                let miss = if away > predicted { away - predicted } else { predicted - away };
                held.spread = Some(smooth(held.spread, miss));
            }
            held.away = Some(smooth(held.away, away));
        }
        let (bytes, at) = (held.bytes, held.at);
        let (waited, fetched) = match at {
            Where::Resident { .. } => (Duration::ZERO, None),
            Where::Moving { lands, .. } => (lands.saturating_sub(now), Some(copy_of(bytes))),
            Where::Parked => {
                self.make_room(bytes, &[mind], None, now, prefetch);
                let copy = copy_of(bytes);
                let lands = self.start(mind, now, copy);
                (lands.saturating_sub(now), Some(copy))
            }
        };
        if let Some(held) = self.minds.get_mut(&mind) {
            if let Some(copy) = fetched {
                held.copy = Some(copy);
            }
            held.at = Where::Resident { since: now + waited };
            if let Some((id, _)) = held.plan.take() {
                // she served: whatever she waited is the lead's lateness, and nothing was wasted
                prefetch.outcome(id, Outcome { late: waited, wasted: Duration::ZERO, bytes, capacity: self.capacity, bound: self.bound });
            }
        }
        Brought { waited, fetched }
    }

    /// `mind` left the lane at `now` (her slice ended, or her turn).
    pub fn left(&mut self, mind: K, now: Duration) {
        if let Some(held) = self.minds.get_mut(&mind) {
            held.left = Some(now);
        }
    }

    /// At each lane step: decide when each parked mind's move starts, and start those due, when the link and the room
    /// allow (never by parking `running`, nor a mind needed sooner).
    pub fn tick(&mut self, now: Duration, running: Option<K>, copy_of: impl Fn(u64) -> Duration, prefetch: &mut Decider<Prefetch>) {
        let mut parked: Vec<K> = self.minds.iter().filter(|(k, h)| matches!(h.at, Where::Parked) && Some(**k) != running).map(|(k, _)| *k).collect();
        parked.sort_by_key(|k| (self.predicted(k).unwrap_or(Duration::MAX), *k)); // unwrap_or: an unpredicted need comes last
        for mind in parked {
            let Some(&Held { bytes, copy: measured, spread, plan: open, .. }) = self.minds.get(&mind) else { continue };
            let plan = match open {
                Some(plan) => plan,
                None => {
                    let until = self.predicted(&mind).map(|need| need.saturating_sub(now));
                    let free = self.capacity.get().saturating_sub(self.used());
                    let situation = Situation {
                        bytes,
                        copy: measured.map_or(Measured::Unmeasured, Measured::Value), // map_or: never moved, never measured
                        until: until.map_or(Measured::Unmeasured, Measured::Value), // map_or: never away, never predicted
                        spread: spread.map_or(Measured::Unmeasured, Measured::Value), // map_or: never predicted, never wrong
                        room: free.max(bytes.min(self.capacity.get())), // the room she could have once a later mind parks
                        capacity: self.capacity,
                        bound: self.bound,
                    };
                    let Decided::Act(id, action) = prefetch.decide(&situation, &[]) else { continue };
                    let lead = situation.lead(action);
                    let start = match action {
                        Action::AtNeed => Duration::MAX, // her move starts only when the lane brings her in
                        _ => now + until.map_or(Duration::ZERO, |until| until.saturating_sub(lead)), // map_or: unpredicted, start now
                    };
                    if let Some(held) = self.minds.get_mut(&mind) {
                        held.plan = Some((id, start));
                    }
                    (id, start)
                }
            };
            let (_, start) = plan;
            if start > now || self.link_free > now {
                continue;
            }
            let need = self.predicted(&mind);
            let keep: Vec<K> = running.into_iter().chain(std::iter::once(mind)).collect();
            if self.make_room(bytes, &keep, need, now, prefetch) {
                let copy = copy_of(bytes);
                self.start(mind, now, copy);
            }
        }
        // a move that landed is resident from its landing
        for held in self.minds.values_mut() {
            if let Where::Moving { lands, .. } = held.at {
                if lands <= now {
                    held.at = Where::Resident { since: lands };
                }
            }
        }
    }
}

impl<K: Copy + Eq + Hash + Ord> Room<K> {
    /// The next moment a planned move falls due after `now`: an idle lane wakes for it, so a prefetch planned in a
    /// gap starts on time.
    pub fn next_due(&self, now: Duration) -> Option<Duration> {
        self.minds.values().filter(|h| matches!(h.at, Where::Parked)).filter_map(|h| h.plan.map(|(_, start)| start)).filter(|start| *start > now && *start != Duration::MAX).min()
    }
}
