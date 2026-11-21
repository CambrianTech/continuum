//! The mechanism every component inherits, proven on components that never hold each other.

use std::time::Duration;

use super::*;
use crate::dataflow::{Event, Publisher, Snapshot, TopicName};
use uuid::Uuid;

const NUMBERS: TopicName = TopicName("test/numbers");
const TOTAL: TopicName = TopicName("test/total");

/// Hears every number on its owner's bus, says the running total.
struct Adder {
    total: u64,
    says: Publisher<u64>,
}

struct Number(u64);

impl From<Event<u64>> for Number {
    fn from(event: Event<u64>) -> Self {
        Number(*event.value)
    }
}

impl Component for Adder {
    type Input = Number;
    const NAME: &'static str = "adder";
    const QOS: Qos = Qos::Work;
    fn wire(&self, wiring: &mut Wiring<'_, Self>) {
        wiring.hear::<u64>(NUMBERS);
    }
    async fn handle(&mut self, Number(n): Number, ctx: &mut Ctx<Self>) {
        self.total += n;
        ctx.publish(&mut self.says, self.total);
    }
}

// what this catches: a component wired by hand, or holding another to observe it. The adder names
// only the topic it hears and claims the topic it says; whoever reads the total names that topic.
// Its publication names the event it came from: provenance stamped by the engine, not the component.
#[tokio::test]
async fn a_part_hears_and_says_by_topic_with_provenance_from_the_engine() {
    let bus = Bus::new();
    let claimed = bus.publisher::<u64>(TOTAL, Key::ONE);
    assert_eq!(bus.publisher::<u64>(TOTAL, Key::ONE).err(), Some(Taken(TOTAL)), "a topic has one writer");
    let says = match claimed {
        Ok(says) => says,
        Err(taken) => return assert_eq!(Some(taken), None, "the total is unclaimed"),
    };
    let _adder = start(Adder { total: 0, says }, &bus, OwnerId(Uuid::from_u128(1)), 1);
    let mut total = bus.reader::<u64>(TOTAL, Key::ONE);
    let mut numbers = match bus.emitter::<u64>(NUMBERS) {
        Ok(numbers) => numbers,
        Err(taken) => return assert_eq!(Some(taken), None, "the numbers topic is unclaimed"),
    };
    numbers.emit(2).await;
    let last_sent = numbers.emit(3).await;
    let mut last: Option<Snapshot<u64>> = None;
    while last.as_ref().map(|s| *s.value) != Some(5) {
        last = total.changed().await.ok();
    }
    let provenance = last.map(|s| s.provenance);
    assert_eq!(provenance.as_ref().map(|p| p.by.incarnation.stage), Some(StageId::named("adder")));
    assert_eq!(provenance.map(|p| p.from), Some(vec![last_sent]));
}

/// Arms a timer when told, and reports what fell due.
struct Alarm {
    rang: tokio::sync::mpsc::UnboundedSender<&'static str>,
}

enum AlarmIn {
    Set { label: &'static str, after: Duration },
    Ring(&'static str),
}

impl Component for Alarm {
    type Input = AlarmIn;
    const NAME: &'static str = "alarm";
    const QOS: Qos = Qos::Work;
    fn wire(&self, _: &mut Wiring<'_, Self>) {}
    async fn handle(&mut self, input: AlarmIn, ctx: &mut Ctx<Self>) {
        match input {
            AlarmIn::Set { label, after } => ctx.after(Key::ONE, after, AlarmIn::Ring(label)),
            AlarmIn::Ring(label) => assert!(self.rang.send(label).is_ok(), "the test reads every ring"),
        }
    }
}

// what this catches: a component keeping a handle to itself for its deadline, or two deadlines both
// firing. A timer under the same key replaces the earlier one; only the newest rings, and nothing
// rings after the component stops.
#[tokio::test]
async fn a_newer_timer_under_the_same_key_replaces_the_older() {
    let (rang, mut rings) = tokio::sync::mpsc::unbounded_channel();
    let alarm = start(Alarm { rang }, &Bus::new(), OwnerId(Uuid::from_u128(2)), 1);
    assert!(alarm.tell(AlarmIn::Set { label: "first", after: Duration::from_millis(5) }).await);
    assert!(alarm.tell(AlarmIn::Set { label: "second", after: Duration::from_millis(10) }).await);
    assert_eq!(rings.recv().await, Some("second"));
    drop(alarm);
    assert_eq!(rings.recv().await, None, "the first never rang");
}
