//! The fetcher: bringing the next minds' KV onto the device on its own worker, so the lane never waits on a copy
//! it could have avoided (Joel, 2026-11-26: "in a sage component too, so it's in its own thread. Very good
//! parallelism pathway like if you read on the rage engine"). RAGE's renderer never loads: it draws what is
//! resident, and a streamer fed by the visibility pass's priorities pages the rest in beside it. Here the lane is
//! the renderer and this is its streamer:
//!
//! - The lane says who comes next, in the order it expects to bring them in, as state ([`NEXT`]).
//! - The fetcher watches it, starts each mind's move through its [`Mover`] (a pointer on unified memory, pinned
//!   async DMA on a discrete card), one move at a time per link, highest first.
//! - It says each mind's residency as state ([`RESIDENT`]): the lane, at a switch, reads whether she is there, and
//!   brings in one who is rather than stall.
//!
//! The replay shows why (slice/sim.rs, the 5090's day): a synchronous copy cost the lane 129 s of 900 in switches,
//! a prefetch overlapped with the slice 34 s.

use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::Arc;
use std::time::Duration;

use crate::component::{Component, Ctx, Key, Wiring};
use crate::dataflow::{Publisher, Snapshot, TopicName};
use crate::Qos;

/// Who comes next, in the order the lane expects to bring them in, with the bytes each mind's KV takes.
#[derive(Debug, Clone, PartialEq)]
pub struct NextUp<K> {
    pub minds: Vec<(K, u64)>,
}

/// Where one mind's KV is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Residency {
    /// On the device: the lane may bring her in at no copy.
    Resident,
    /// Being moved in.
    Fetching,
    /// In host memory (or flash): bringing her in now waits on her copy.
    Parked,
}

/// One mind's row: where her KV is, and her last fetch's measured time (the lane's `Fetched`: what a slice before her
/// must cover).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row<K> {
    pub mind: K,
    pub residency: Residency,
    pub fetched: Option<Duration>,
}

/// Each mind's residency, as the fetcher says it.
#[derive(Debug, Clone, PartialEq)]
pub struct Resident<K>(pub Vec<Row<K>>);

/// What the fetcher asks its mover: bring `mind`'s `bytes` onto the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveAsk<K> {
    pub mind: K,
    pub bytes: u64,
}

/// The mover's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moving {
    /// Started; its landing arrives as [`Fetch::Landed`] through the fetcher's teller.
    Started,
    /// Nothing to move (unified memory: her KV is viewed where it lies).
    AlreadyThere,
    /// It could not, and why.
    Refused { why: &'static str },
}

/// The one thing a memory does differently: start moving a mind's KV onto the device. Its completion is the
/// device's fence, told back to the fetcher; it never waits.
pub trait Mover<K>: Send + 'static {
    fn start(&mut self, ask: MoveAsk<K>) -> Moving;
}

/// What wakes the fetcher.
#[derive(Debug)]
pub enum Fetch<K> {
    /// The lane's prediction changed.
    Next(Snapshot<NextUp<K>>),
    /// A move's fence landed: `mind` is resident, in `took`.
    Landed { mind: K, took: Duration },
    /// The owner parked `mind` (her storage cached out): she must be fetched again before she is resident.
    Parked(K),
}

impl<K> From<Snapshot<NextUp<K>>> for Fetch<K> {
    fn from(next: Snapshot<NextUp<K>>) -> Self {
        Self::Next(next)
    }
}

/// The lane's prediction of who comes next, state keyed [`Key::ONE`].
pub const NEXT: TopicName = TopicName("sage/lane/next");
/// Each mind's residency, state keyed [`Key::ONE`].
pub const RESIDENT: TopicName = TopicName("sage/lane/resident");

/// The streamer beside the lane.
pub struct Fetcher<K, M> {
    mover: M,
    next: Arc<NextUp<K>>,
    residency: HashMap<K, Residency>,
    /// The mind whose move is in flight: one at a time per link, so the highest comes first.
    moving: Option<K>,
    says: Publisher<Resident<K>>,
    /// Each mind's last landing time.
    fetched: HashMap<K, Duration>,
}

impl<K: Copy + Eq + Hash + Debug + Send + Sync + 'static, M: Mover<K>> Fetcher<K, M> {
    pub fn new(mover: M, says: Publisher<Resident<K>>) -> Self {
        Self { mover, next: Arc::new(NextUp { minds: Vec::new() }), residency: HashMap::new(), moving: None, says, fetched: HashMap::new() }
    }

    /// Start the highest predicted mind that is neither resident nor moving, if the link is free.
    fn feed(&mut self) {
        if self.moving.is_some() {
            return;
        }
        for &(mind, bytes) in &self.next.minds {
            if matches!(self.residency.get(&mind), Some(Residency::Resident | Residency::Fetching)) {
                continue;
            }
            match self.mover.start(MoveAsk { mind, bytes }) {
                Moving::Started => {
                    self.residency.insert(mind, Residency::Fetching);
                    self.moving = Some(mind);
                    return;
                }
                Moving::AlreadyThere => {
                    self.residency.insert(mind, Residency::Resident);
                }
                Moving::Refused { why } => {
                    self.residency.insert(mind, Residency::Parked);
                    continuum_base::probe!(class = "lane.fetch.refused", mind = ?mind, bytes, why, "a predicted mind's move was refused: she stays parked");
                }
            }
        }
    }

    fn said(&self) -> Resident<K> {
        let mut rows: Vec<Row<K>> = self.residency.iter().map(|(k, r)| Row { mind: *k, residency: *r, fetched: self.fetched.get(k).copied() }).collect();
        // in the lane's order first, so a reader sees the next minds' state at the front
        rows.sort_by_key(|row| self.next.minds.iter().position(|(m, _)| *m == row.mind).unwrap_or(usize::MAX)); // unwrap_or: a mind no longer predicted sorts last
        Resident(rows)
    }
}

impl<K: Copy + Eq + Hash + Debug + Send + Sync + 'static, M: Mover<K>> Component for Fetcher<K, M> {
    type Input = Fetch<K>;
    const NAME: &'static str = "lane-fetcher";
    const QOS: Qos = Qos::Interactive;

    fn wire(&self, wiring: &mut Wiring<'_, Self>) {
        wiring.watch::<NextUp<K>>(NEXT, Key::ONE);
    }

    async fn handle(&mut self, input: Self::Input, ctx: &mut Ctx<Self>) {
        match input {
            Fetch::Next(next) => self.next = next.value,
            Fetch::Landed { mind, took } => {
                if self.moving == Some(mind) {
                    self.moving = None;
                }
                self.residency.insert(mind, Residency::Resident);
                self.fetched.insert(mind, took);
            }
            Fetch::Parked(mind) => {
                if self.moving == Some(mind) {
                    self.moving = None; // its move is the owner's to cancel; she is parked either way
                }
                self.residency.insert(mind, Residency::Parked);
            }
        }
        self.feed();
        let said = self.said();
        ctx.publish(&mut self.says, said);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::{start, Bus};
    use crate::dataflow::{OwnerId, Provenance, RunId, StageId, StageIncarnation};
    use std::sync::Mutex;

    const GB: u64 = 1 << 30;

    /// A discrete card's mover: records what it was asked to start; its landings are told by the test.
    struct Dma(Arc<Mutex<Vec<char>>>);

    impl Mover<char> for Dma {
        fn start(&mut self, ask: MoveAsk<char>) -> Moving {
            self.0.lock().expect("test: the record").push(ask.mind); // expect: the test's own lock
            Moving::Started
        }
    }

    /// Unified memory: a mind's KV is viewed where it lies.
    struct Pointer;

    impl Mover<char> for Pointer {
        fn start(&mut self, _: MoveAsk<char>) -> Moving {
            Moving::AlreadyThere
        }
    }

    fn by() -> Provenance {
        Provenance { by: RunId { incarnation: StageIncarnation { stage: StageId::named("lane"), owner: OwnerId(uuid::Uuid::from_u128(9)), epoch: 1 }, seq: 1 }, from: Vec::new() }
    }

    async fn until(resident: &mut crate::dataflow::Reader<Resident<char>>, want: &[(char, Residency)]) {
        loop {
            if resident.latest().is_some_and(|now| now.value.0.iter().map(|row| (row.mind, row.residency)).eq(want.iter().copied())) {
                return;
            }
            assert!(resident.changed().await.is_ok(), "the fetcher keeps saying its residency");
        }
    }

    // what this catches (Joel, 2026-11-26: "in a sage component too, so it's in its own thread", RAGE's streamer):
    // the lane waiting on a copy, two moves fighting one link, or a landing that does not start the next. On its own
    // worker the fetcher brings the lane's next minds in, highest first and one at a time, and says each one's
    // residency as state; a parked mind is fetched again.
    #[tokio::test]
    async fn the_fetcher_streams_the_lanes_next_minds_in_on_its_own_worker() {
        let bus: &'static Bus = Box::leak(Box::new(Bus::new()));
        let started = Arc::new(Mutex::new(Vec::new()));
        let says = bus.publisher(RESIDENT, Key::ONE).expect("test: the topic is free"); // expect: the test's own precondition
        let fetcher = start(Fetcher::new(Dma(Arc::clone(&started)), says), bus, OwnerId(uuid::Uuid::from_u128(9)), 1);
        let mut resident = bus.reader::<Resident<char>>(RESIDENT, Key::ONE);
        let mut next = bus.publisher::<NextUp<char>>(NEXT, Key::ONE).expect("test: the topic is free"); // expect: the test's own precondition
        next.publish(NextUp { minds: vec![('b', 8 * GB), ('c', 4 * GB)] }, by());
        until(&mut resident, &[('b', Residency::Fetching)]).await;
        assert_eq!(*started.lock().expect("test: the record"), vec!['b'], "one move per link, the highest first"); // expect: the test's own lock
        assert!(fetcher.tell(Fetch::Landed { mind: 'b', took: Duration::from_millis(340) }).await);
        until(&mut resident, &[('b', Residency::Resident), ('c', Residency::Fetching)]).await;
        assert_eq!(*started.lock().expect("test: the record"), vec!['b', 'c'], "her landing starts the next"); // expect: the test's own lock
        let row = resident.latest().and_then(|now| now.value.0.iter().find(|row| row.mind == 'b').copied());
        assert_eq!(row.and_then(|row| row.fetched), Some(Duration::from_millis(340)), "her measured fetch is said with her, for the lane's slice floor");
        assert!(fetcher.tell(Fetch::Landed { mind: 'c', took: Duration::from_millis(170) }).await);
        assert!(fetcher.tell(Fetch::Parked('b')).await);
        until(&mut resident, &[('b', Residency::Fetching), ('c', Residency::Resident)]).await;
        assert_eq!(*started.lock().expect("test: the record"), vec!['b', 'c', 'b'], "a parked mind still predicted is fetched again"); // expect: the test's own lock
    }

    // what this catches: unified memory paying for a fetch it does not need. With a pointer mover every predicted
    // mind is resident at once, nothing moved.
    #[tokio::test]
    async fn on_unified_memory_every_predicted_mind_is_resident_at_once() {
        let bus: &'static Bus = Box::leak(Box::new(Bus::new()));
        let says = bus.publisher(RESIDENT, Key::ONE).expect("test: the topic is free"); // expect: the test's own precondition
        let _fetcher = start(Fetcher::new(Pointer, says), bus, OwnerId(uuid::Uuid::from_u128(10)), 1);
        let mut resident = bus.reader::<Resident<char>>(RESIDENT, Key::ONE);
        let mut next = bus.publisher::<NextUp<char>>(NEXT, Key::ONE).expect("test: the topic is free"); // expect: the test's own precondition
        next.publish(NextUp { minds: vec![('b', 8 * GB), ('c', 4 * GB)] }, by());
        until(&mut resident, &[('b', Residency::Resident), ('c', Residency::Resident)]).await;
    }
}
