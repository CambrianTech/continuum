//! The slice governor: the SAGE component that owns one engine's [`Slicer`] and says, as a state,
//! which mind holds the engine now.
//!
//! It wakes only on events: a mind's need changed, a turn arrived or ended, the engine measured a
//! step or a switch (all on [`SLICE`], in order), or the deadline of the running slice it set on the
//! node's clock. It never polls. Each time, it decides and publishes [`Turn`] on [`TURN`] when the
//! holder changes; the engine reads the newest turn at its step boundary, a state read with no wait.

use std::hash::Hash;
use std::sync::Arc;
use std::time::Duration;

use crate::component::{Component, Ctx, Key, Need, Wiring};
use crate::dataflow::{Event, Publisher, TopicName};
use crate::Qos;

use super::{Next, Slicer};

/// What the governor hears from the minds and the engine.
pub const SLICE: TopicName = TopicName("engine/slice");
/// Who holds the engine: the governor's one output.
pub const TURN: TopicName = TopicName("engine/turn");

/// One fact about the minds sharing the engine, in the order it happened.
#[derive(Debug, Clone)]
pub enum SliceEvent<K> {
    /// Her folded need changed: her share follows.
    Need(K, Need),
    /// She has a turn to run.
    Ready(K),
    /// Her turn ended.
    Done(K),
    /// The engine stepped her once, measured.
    Stepped(K, Duration),
    /// The engine brought her in, measured.
    Switched(K, Duration),
}

/// Who holds the engine; `None` when nobody is ready.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn<K>(pub Option<K>);

pub enum GovernorIn<K> {
    Heard(Arc<SliceEvent<K>>),
    /// The running slice's deadline.
    Due,
}

impl<K> From<Event<SliceEvent<K>>> for GovernorIn<K> {
    fn from(event: Event<SliceEvent<K>>) -> Self {
        GovernorIn::Heard(event.value)
    }
}

pub struct Governor<K> {
    slicer: Slicer<K>,
    says: Publisher<Turn<K>>,
    said: Option<K>,
}

impl<K> Governor<K> {
    pub fn new(slicer: Slicer<K>, says: Publisher<Turn<K>>) -> Self {
        Self { slicer, says, said: None }
    }
}

impl<K: Clone + Eq + Hash + Ord + Send + Sync + 'static> Governor<K> {
    fn apply(&mut self, event: &SliceEvent<K>) {
        match event {
            SliceEvent::Need(mind, need) => self.slicer.need(mind.clone(), *need),
            SliceEvent::Ready(mind) => self.slicer.ready(mind.clone()),
            SliceEvent::Done(mind) => self.slicer.done(mind),
            SliceEvent::Stepped(mind, took) => self.slicer.stepped(mind, *took),
            SliceEvent::Switched(mind, took) => self.slicer.switched(mind, *took),
        }
    }
}

impl<K: Clone + Eq + Hash + Ord + Send + Sync + 'static> Component for Governor<K> {
    type Input = GovernorIn<K>;
    const NAME: &'static str = "slice-governor";
    const QOS: Qos = Qos::Interactive;

    fn wire(&self, wiring: &mut Wiring<'_, Self>) {
        wiring.hear::<SliceEvent<K>>(SLICE);
    }

    async fn handle(&mut self, input: GovernorIn<K>, ctx: &mut Ctx<Self>) {
        if let GovernorIn::Heard(event) = &input {
            self.apply(event);
        }
        let now = tokio::time::Instant::now().into_std();
        let holder = match self.slicer.next(now) {
            Next::Switch(mind) => Some(mind),
            Next::Idle => None,
            Next::Continue => self.said.clone(),
        };
        if holder != self.said {
            self.said = holder.clone();
            ctx.publish(&mut self.says, Turn(holder));
        }
        match self.slicer.deadline() {
            Some(at) => ctx.after(Key::ONE, at.saturating_duration_since(now), GovernorIn::Due),
            None => ctx.cancel(Key::ONE),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::{start, Bus, Urgency};
    use crate::dataflow::OwnerId;
    use crate::Policy;

    // what this catches: a governor that needs a step event, or a poll, to hand the engine over. Two
    // minds ready: the first gets the turn, and the second gets it when the measured slice is spent,
    // on the governor's own deadline with no further event.
    #[tokio::test]
    async fn the_governor_hands_over_on_its_own_deadline() {
        let bus = Bus::new();
        let says = match bus.publisher::<Turn<char>>(TURN, Key::ONE) {
            Ok(says) => says,
            Err(taken) => return assert_eq!(Some(taken), None, "the turn is unclaimed"),
        };
        let policy = Policy { switch_share: 0.05, wait_target: Duration::from_millis(40), smoothing: 0.25 };
        let _governor = start(Governor::new(Slicer::new(policy), says), &bus, OwnerId(uuid::Uuid::from_u128(7)), 1);
        let mut turns = bus.reader::<Turn<char>>(TURN, Key::ONE);
        let mut slice = match bus.emitter::<SliceEvent<char>>(SLICE) {
            Ok(slice) => slice,
            Err(taken) => return assert_eq!(Some(taken), None, "the slice topic is unclaimed"),
        };
        let want = Need { urgency: Urgency::Want, waiting: 0 };
        for mind in ['a', 'b'] {
            slice.emit(SliceEvent::Need(mind, want)).await;
            slice.emit(SliceEvent::Ready(mind)).await;
        }
        let mut seen = Vec::new();
        while seen.last() != Some(&Some('b')) {
            let Ok(turn) = tokio::time::timeout(Duration::from_secs(2), turns.changed()).await else {
                return assert_eq!(seen, vec![Some('a'), Some('b')], "the governor handed over within its slice");
            };
            let Ok(turn) = turn else { return assert_eq!(seen, vec![Some('a'), Some('b')], "the governor runs") };
            seen.push(turn.value.0);
            if seen == vec![Some('a')] {
                // The engine brings her in and reports what it cost: the slice is sized from it.
                slice.emit(SliceEvent::Switched('a', Duration::from_millis(5))).await;
            }
        }
        assert_eq!(seen, vec![Some('a'), Some('b')]);
    }
}
