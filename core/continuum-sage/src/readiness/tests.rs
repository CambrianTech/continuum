use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use uuid::Uuid;

use super::{Advanced, Horizon, Needs, Readies, Readiness, Ready, Table, Upcoming, HORIZON};
use crate::component::{start, Bus, Key};
use crate::dataflow::{OwnerId, Provenance, RunId, StageId, StageIncarnation, TopicName};
use crate::Qos;

const STAGED: TopicName = TopicName("test/staged");

/// What the test leaf did, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Did {
    Advanced(u8),
    Released(u8),
}

/// A leaf whose minds each need `owed[mind]` units, one advanced at a time.
struct Stager {
    owed: Arc<Mutex<[u64; 4]>>,
    did: tokio::sync::mpsc::UnboundedSender<Did>,
}

impl Readies for Stager {
    const NAME: &'static str = "test-stager";
    const QOS: Qos = Qos::Utility;
    const TOPIC: TopicName = STAGED;
    type Mind = u8;
    type Work = u64;
    type Refusal = &'static str;

    fn needs(&mut self, mind: u8) -> Needs<u64> {
        match self.owed.lock()[mind as usize] {
            0 => Needs::Nothing,
            units => Needs::Work(units),
        }
    }

    fn left(work: &u64) -> u64 {
        *work
    }

    async fn advance(&mut self, mind: u8, work: u64) -> Advanced<u64, &'static str> {

        if self.did.send(Did::Advanced(mind)).is_err() {
            panic!("test: the test holds what the leaf did");
        }
        let left = work - 1;
        self.owed.lock()[mind as usize] = left;
        if left == 0 { Advanced::Done } else { Advanced::More(left) }
    }

    fn release(&mut self, mind: u8) {
        if self.did.send(Did::Released(mind)).is_err() {
            panic!("test: the test holds what the leaf did");
        }
    }
}

fn by() -> Provenance {
    Provenance { by: RunId { incarnation: StageIncarnation { stage: StageId::named("lane"), owner: OwnerId(Uuid::from_u128(9)), epoch: 1 }, seq: 1 }, from: Vec::new() }
}

fn horizon(minds: &[(u8, u64)]) -> Horizon<u8> {
    Horizon { minds: minds.iter().map(|&(mind, slack_ms)| Upcoming { mind, slack: Duration::from_millis(slack_ms) }).collect() }
}

async fn until(table: &mut crate::dataflow::Reader<Table<u8, &'static str>>, done: impl Fn(&Table<u8, &'static str>) -> bool) -> Table<u8, &'static str> {
    loop {
        if let Some(latest) = table.latest().filter(|latest| done(&latest.value)) {
            return (*latest.value).clone();
        }
        if table.changed().await.is_err() {
            panic!("test: the part stopped publishing");
        }
    }
}

// what this catches: a preparation that serves minds in arrival order, so the mind about to step waits behind one
// who has time to spare (the lane would switch to her unready, or wait). Two minds each owe three units; the one
// with less slack is made ready first, every unit of hers before any of the other's.
#[tokio::test]
async fn the_most_pressed_mind_is_made_ready_first() {
    let bus = Bus::new();
    let (did, mut done) = tokio::sync::mpsc::unbounded_channel();
    let owed = Arc::new(Mutex::new([3, 3, 0, 0]));
    let Ok(part) = Readiness::new(Stager { owed: Arc::clone(&owed), did }, &bus) else { panic!("test: the topic is unclaimed") };
    let mut table = bus.reader::<Table<u8, &'static str>>(STAGED, Key::ONE);
    let Ok(mut lane) = bus.publisher::<Horizon<u8>>(HORIZON, Key::ONE) else { panic!("test: the horizon is unclaimed") };
    let _running = start(part, &bus, OwnerId(Uuid::from_u128(1)), 1);
    lane.publish(horizon(&[(0, 900), (1, 100)]), by());
    let ready = until(&mut table, |t| t.minds.iter().all(|(_, ready)| *ready == Ready::Ready)).await;
    assert_eq!(ready.minds.len(), 2);
    let mut order = Vec::new();
    while let Ok(event) = done.try_recv() {
        order.push(event);
    }
    assert_eq!(order, [Did::Advanced(1), Did::Advanced(1), Did::Advanced(1), Did::Advanced(0), Did::Advanced(0), Did::Advanced(0)]);
}

// what this catches: a mind the lane no longer means to step kept prepared (her KV held hot, her genes attached),
// holding memory another mind needs. She is let go when she leaves the horizon, and the table no longer names her.
#[tokio::test]
async fn a_mind_who_leaves_the_horizon_is_let_go() {
    let bus = Bus::new();
    let (did, mut done) = tokio::sync::mpsc::unbounded_channel();
    let owed = Arc::new(Mutex::new([0, 0, 0, 0]));
    let Ok(part) = Readiness::new(Stager { owed, did }, &bus) else { panic!("test: the topic is unclaimed") };
    let mut table = bus.reader::<Table<u8, &'static str>>(STAGED, Key::ONE);
    let Ok(mut lane) = bus.publisher::<Horizon<u8>>(HORIZON, Key::ONE) else { panic!("test: the horizon is unclaimed") };
    let _running = start(part, &bus, OwnerId(Uuid::from_u128(2)), 1);
    lane.publish(horizon(&[(0, 500), (2, 500)]), by());
    until(&mut table, |t| t.minds.len() == 2).await;
    lane.publish(horizon(&[(0, 500)]), by());
    let after = until(&mut table, |t| t.minds.len() == 1).await;
    assert_eq!(after.of(&0), Some(&Ready::Ready));
    assert_eq!(after.of(&2), None);
    assert_eq!(done.recv().await, Some(Did::Released(2)));
}

// what this catches: a mind marked ready once and never asked again, so after her genes or her KV moved the lane
// switches to her unready. A ready mind whose leaf now owes work is prepared again at the next horizon, and the
// table says so with an estimate from the units it measured.
#[tokio::test]
async fn a_ready_mind_is_prepared_again_once_what_it_prepared_changed() {
    let bus = Bus::new();
    let (did, _done) = tokio::sync::mpsc::unbounded_channel();
    let owed = Arc::new(Mutex::new([2, 0, 0, 0]));
    let Ok(part) = Readiness::new(Stager { owed: Arc::clone(&owed), did }, &bus) else { panic!("test: the topic is unclaimed") };
    let mut table = bus.reader::<Table<u8, &'static str>>(STAGED, Key::ONE);
    let Ok(mut lane) = bus.publisher::<Horizon<u8>>(HORIZON, Key::ONE) else { panic!("test: the horizon is unclaimed") };
    let _running = start(part, &bus, OwnerId(Uuid::from_u128(3)), 1);
    lane.publish(horizon(&[(0, 500)]), by());
    until(&mut table, |t| t.of(&0) == Some(&Ready::Ready)).await;
    owed.lock()[0] = 50; // what it prepared changed: her work again, long enough to be seen preparing
    lane.publish(horizon(&[(0, 400)]), by());
    let preparing = until(&mut table, |t| matches!(t.of(&0), Some(Ready::Preparing { eta: crate::decide::Measured::Value(_) }))).await;
    let Some(Ready::Preparing { eta: crate::decide::Measured::Value(eta) }) = preparing.of(&0) else { panic!("test: matched above") };
    assert!(*eta < Duration::from_secs(2), "ready in about her units at the measured cost: {eta:?}");
    until(&mut table, |t| t.of(&0) == Some(&Ready::Ready)).await;
}
