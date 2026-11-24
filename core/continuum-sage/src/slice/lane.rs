//! The lane: the one place minds share, held by one mind at a time for one short slice.
//!
//! Everything has its own cadence (Joel, 2026-11-22: "They're all their own concerns but meet on
//! their next SHORT cycle, using caches and current state ... A person doesn't hold another
//! back"). A mind's parts run on their own workers and publish her state; she never waits on the
//! lane. The lane runs on its own cycle, the decode step, and at each step boundary reads every
//! seated mind's newest state (her folded need, whether she has decoding to do) without waiting
//! on anyone, measures its own step and switch inline, and decides with its [`Slicer`]. Nothing
//! here emits an event or awaits: no mind, no lane and no observer can hold another back.

use std::hash::Hash;
use std::time::{Duration, Instant};

use crate::component::Need;
use crate::dataflow::{Provenance, Publisher, Reader, Snapshot};

use super::{Next, Slicer};

/// Whether she has decoding to do now: the state a mind publishes on her own cadence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Decoding(pub bool);

/// Who holds the lane; `None` when nobody has decoding to do. Published for whoever watches,
/// never waited on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn<K>(pub Option<K>);

struct Seat<K> {
    mind: K,
    need: Reader<Need>,
    decoding: Reader<Decoding>,
    /// The revisions last read: a state that has not moved is not re-applied.
    need_seen: Option<u64>,
    decoding_seen: Option<u64>,
}

pub struct Lane<K> {
    slicer: Slicer<K>,
    seats: Vec<Seat<K>>,
    turn: Publisher<Turn<K>>,
    said: Option<K>,
    /// The run its turns are published as.
    by: crate::dataflow::RunId,
}

/// The newest value, if it moved since `seen`.
fn moved<T: Send + Sync + 'static>(reader: &Reader<T>, seen: &mut Option<u64>) -> Option<Snapshot<T>> {
    let latest = reader.latest()?;
    if *seen == Some(latest.revision.through) {
        return None;
    }
    *seen = Some(latest.revision.through);
    Some(latest)
}

impl<K: Copy + Eq + Hash + Ord + Send + Sync + 'static> Lane<K> {
    pub fn new(slicer: Slicer<K>, turn: Publisher<Turn<K>>, by: Provenance) -> Self {
        Self { slicer, seats: Vec::new(), turn, said: None, by: by.by }
    }

    /// Seat a mind: from now on the lane reads her state at its step boundaries.
    pub fn seat(&mut self, mind: K, need: Reader<Need>, decoding: Reader<Decoding>) {
        // her new readers replace any she had; what the slicer learned of her stays
        self.seats.retain(|seat| seat.mind != mind);
        self.seats.push(Seat { mind, need, decoding, need_seen: None, decoding_seen: None });
    }

    /// She left the lane for good (a one-off whose turn ended, a mind moved to another node): her seat
    /// and everything the slicer learned of her go.
    pub fn unseat(&mut self, mind: &K) {
        self.seats.retain(|seat| &seat.mind != mind);
        self.slicer.forget(mind);
    }

    /// How many minds are seated: the ones that may step.
    pub fn seated(&self) -> usize {
        self.seats.len()
    }

    /// How many minds the slicer tracks.
    pub fn tracked(&self) -> usize {
        self.slicer.tracked()
    }

    /// The decision at this step boundary, from every seated mind's current state.
    pub fn next(&mut self, now: Instant) -> Next<K> {
        for seat in &mut self.seats {
            if let Some(need) = moved(&seat.need, &mut seat.need_seen) {
                self.slicer.need(seat.mind, *need.value);
            }
            if let Some(decoding) = moved(&seat.decoding, &mut seat.decoding_seen) {
                match *decoding.value {
                    Decoding(true) => self.slicer.ready(seat.mind),
                    Decoding(false) => self.slicer.done(&seat.mind),
                }
            }
        }
        let next = self.slicer.next(now);
        let holder = match &next {
            Next::Switch(mind) => Some(*mind),
            Next::Idle => None,
            Next::Continue => self.said,
        };
        if holder != self.said {
            self.said = holder;
            self.turn.publish(Turn(holder), Provenance { by: self.by, from: Vec::new() });
        }
        next
    }

    /// The lane stepped her once, measured inline.
    pub fn stepped(&mut self, mind: &K, took: Duration) {
        self.slicer.stepped(mind, took);
    }

    /// The lane brought her in, measured inline.
    pub fn switched(&mut self, mind: &K, took: Duration) {
        self.slicer.switched(mind, took);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::slice::tests::fixture::ENGINE;
    use crate::component::Urgency;
    use crate::dataflow::{state, OwnerId, RunId, StageId, StageIncarnation, TopicName};
    use crate::Policy;

    fn by() -> Provenance {
        let incarnation = StageIncarnation { stage: StageId::named("lane"), owner: OwnerId(uuid::Uuid::nil()), epoch: 0 };
        Provenance { by: RunId { incarnation, seq: 0 }, from: Vec::new() }
    }

    struct Mind {
        need: Publisher<Need>,
        decoding: Publisher<Decoding>,
    }

    fn mind(lane: &mut Lane<char>, name: char, urgency: Urgency) -> Mind {
        let (mut need, need_reader) = state::<Need>(TopicName("test/need"));
        let (mut decoding, decoding_reader) = state::<Decoding>(TopicName("test/decoding"));
        need.publish(Need { urgency, waiting: 0 }, by());
        decoding.publish(Decoding(true), by());
        lane.seat(name, need_reader, decoding_reader);
        Mind { need, decoding }
    }


    // what this catches: minds coupled to the lane, or the lane to them. Each mind only publishes
    // her own state on her own cadence (no await anywhere: none can be held back); the lane reads
    // it at its step boundaries. A mind whose need rises gets more of the lane at once, one whose
    // decoding ends leaves it to the others, and nobody waits past the wait target.
    #[test]
    fn minds_meet_the_lane_through_their_current_state_and_never_wait() {
        let policy = Policy { wait_bound: Duration::from_secs(1), smoothing: 0.25 };
        let (turn, turns) = state::<Turn<char>>(TopicName("test/turn"));
        let mut lane = Lane::new(Slicer::new(policy), turn, by());
        let mut minds: HashMap<char, Mind> =
            ['a', 'b', 'c'].into_iter().map(|name| (name, mind(&mut lane, name, Urgency::Want))).collect();

        let mut now = Instant::now();
        let mut used: HashMap<char, Duration> = HashMap::new();
        let mut last_step: HashMap<char, Instant> = HashMap::new();
        let mut longest_wait = Duration::ZERO;
        for step in 0..9_000 {
            // Each mind on her own cadence: b is addressed a third of the way in, c finishes her
            // decoding two thirds of the way in. Publishing returns at once.
            if step == 3_000 {
                if let Some(b) = minds.get_mut(&'b') {
                    b.need.publish(Need { urgency: Urgency::Urgent, waiting: 0 }, by());
                }
            }
            if step == 6_000 {
                if let Some(c) = minds.get_mut(&'c') {
                    c.decoding.publish(Decoding(false), by());
                }
            }
            let held = match lane.next(now) {
                Next::Idle => break,
                Next::Switch(mind) => {
                    now += ENGINE.switch;
                    lane.switched(&mind, ENGINE.switch);
                    mind
                }
                Next::Continue => match turns.latest().and_then(|turn| turn.value.0) {
                    Some(mind) => mind,
                    None => break,
                },
            };
            if let Some(prev) = last_step.get(&held) {
                let gap = now.saturating_duration_since(*prev);
                longest_wait = longest_wait.max(gap);
            }
            now += ENGINE.step;
            lane.stepped(&held, ENGINE.step);
            last_step.insert(held, now);
            let window = if step < 3_000 { 0 } else if step < 6_000 { 1 } else { 2 };
            *used.entry(held).or_default() += ENGINE.step;
            if step == 2_999 || step == 5_999 || step == 8_999 {
                let total: Duration = used.values().sum();
                // A mind that never stepped holds no share of the lane.
                let share = |m: char| match used.get(&m) {
                    Some(held) => held.as_secs_f64() / total.as_secs_f64(),
                    None => 0.0,
                };
                match window {
                    0 => assert!((share('a') - share('b')).abs() < 0.03, "equal wants share the lane"),
                    1 => assert!((share('b') / share('a') - 2.0).abs() < 0.25, "urgent b got {:.2}x of a", share('b') / share('a')),
                    _ => {
                        assert_eq!(share('c'), 0.0, "c finished: she holds nothing");
                        assert!(share('a') > 0.0, "a still runs beside urgent b");
                    }
                }
                used.clear();
            }
        }
        assert!(longest_wait <= Duration::from_secs(1) + ENGINE.step, "a mind waited {longest_wait:?} for her next step");
    }
}
