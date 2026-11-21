//! A produced-once artifact: what one stage delivers and any number of others wait on.
//!
//! RULE 10 (CONTINUUM-ENGINE.md): a stage emits what it delivers; whatever needs it subscribes.
//! This is that emission, owned once here so no feature hand-wires a `oneshot`, a `Notify` or a
//! flag for it (Joel, 2026-11-14: "things that make data flow objects are more like how CBAR gives
//! its pipeline for free to consumers"). A producer holds [`Produces`]; a consumer holds a
//! [`Subscribes`] (shared, delivered by handle: `Arc`, never a copy) or the one [`Handoff`] (the
//! value itself, moved to its single consumer). Every artifact settles exactly once:
//!
//! - [`Settled::Ready`]: the producer delivered;
//! - [`Settled::Failed`]: the producer said why it could not, typed by the stage;
//! - [`Settled::Abandoned`]: the producer went away without either (its task ended, panicked or
//!   was cancelled). Its drop is the event, so a waiter can never wait on a producer that is gone.
//!
//! Timing comes free: the settle records how long the artifact was open, and a wait records how
//! long it waited and whether the artifact was already there when the consumer arrived.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::watch;

use super::topic::TopicName;

/// How an artifact settled. `P` is how the value reaches its consumer: `Arc<T>` for a shared
/// artifact, `T` itself for a handoff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Settled<P, F> {
    Ready(P),
    Failed(F),
    Abandoned,
}

impl<P, F> Settled<P, F> {
    fn outcome(&self) -> &'static str {
        match self {
            Settled::Ready(_) => "ready",
            Settled::Failed(_) => "failed",
            Settled::Abandoned => "abandoned",
        }
    }
}

/// Whether the artifact was already settled when the consumer asked for it: the consumer then
/// waited for nothing, and the overlap the producer ran in was complete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrival {
    AlreadySettled,
    WaitedFor(Duration),
}

/// A settled artifact and how its consumer met it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Awaited<P, F> {
    pub settled: Settled<P, F>,
    pub arrival: Arrival,
}

enum State<P, F> {
    Open,
    Settled(Settled<P, F>),
    /// A handoff's value was moved to its consumer.
    Taken,
}

/// The value lives under a lock held only to place or take it (never across an await), so a
/// value need only be `Send`; the settle EVENT is a separate watch every waiter subscribes to.
struct Cell<P, F> {
    topic: TopicName,
    opened: Instant,
    state: parking_lot::Mutex<State<P, F>>,
    settled: watch::Sender<bool>,
}

impl<P, F> Cell<P, F> {
    /// Settle once: a second settle (a drop after a delivery) changes nothing.
    fn settle(&self, settled: Settled<P, F>) {
        let outcome = settled.outcome();
        let placed = {
            let mut state = self.state.lock();
            let open = matches!(*state, State::Open);
            if open {
                *state = State::Settled(settled);
            }
            open
        };
        if placed {
            self.settled.send_replace(true);
            continuum_base::probe!(
                class = "stage.artifact.settled",
                topic = self.topic.0,
                outcome = outcome,
                open_ms = self.opened.elapsed().as_millis() as u64,
                "an artifact settled; its subscribers wake on this event"
            );
        }
    }

    async fn wait(&self, receiver: &mut watch::Receiver<bool>) -> Arrival {
        if *receiver.borrow_and_update() {
            return Arrival::AlreadySettled;
        }
        let asked = Instant::now();
        // The cell holds its own sender, so the channel cannot close while a waiter holds the cell:
        // the wait ends on the settle event alone. Said, never dropped, if that ever breaks.
        if let Err(closed) = receiver.wait_for(|settled| *settled).await {
            continuum_base::probe!(
                class = "stage.artifact.closed",
                topic = self.topic.0,
                error = %closed,
                "an artifact's settle channel closed under a waiter; it reads as abandoned"
            );
        }
        let waited = asked.elapsed();
        continuum_base::probe!(
            class = "stage.artifact.awaited",
            topic = self.topic.0,
            waited_ms = waited.as_millis() as u64,
            "a consumer waited on an artifact that was still open"
        );
        Arrival::WaitedFor(waited)
    }
}

fn open<P, F>(topic: TopicName) -> Arc<Cell<P, F>> {
    Arc::new(Cell {
        topic,
        opened: Instant::now(),
        state: parking_lot::Mutex::new(State::Open),
        settled: watch::channel(false).0,
    })
}

/// The producer's half. Consumed by [`Produces::produce`] or [`Produces::fail`]; dropped without
/// either, the artifact settles [`Settled::Abandoned`].
pub struct Produces<P, F> {
    cell: Arc<Cell<P, F>>,
}

impl<P, F> Produces<P, F> {
    pub fn topic(&self) -> TopicName {
        self.cell.topic
    }

    pub(super) fn settle(self, settled: Settled<P, F>) {
        self.cell.settle(settled);
    }
}

impl<T, F> Produces<Arc<T>, Arc<F>> {
    pub fn produce(self, value: T) {
        self.settle(Settled::Ready(Arc::new(value)));
    }

    pub fn fail(self, why: F) {
        self.settle(Settled::Failed(Arc::new(why)));
    }
}

impl<T, F> Produces<T, F> {
    /// Deliver a handoff's value to its one consumer.
    pub fn hand_over(self, value: T) {
        self.settle(Settled::Ready(value));
    }

    /// Say why a handoff's value will never come.
    pub fn refuse(self, why: F) {
        self.settle(Settled::Failed(why));
    }
}

impl<P, F> Drop for Produces<P, F> {
    fn drop(&mut self) {
        self.cell.settle(Settled::Abandoned);
    }
}

/// A shared artifact's consumer half: cheap to clone, every clone sees the same settle.
pub struct Subscribes<T, F> {
    cell: Arc<Cell<Arc<T>, Arc<F>>>,
    receiver: watch::Receiver<bool>,
}

impl<T, F> Clone for Subscribes<T, F> {
    fn clone(&self) -> Self {
        Self { cell: Arc::clone(&self.cell), receiver: self.cell.settled.subscribe() }
    }
}

impl<T, F> Subscribes<T, F> {
    pub fn topic(&self) -> TopicName {
        self.cell.topic
    }

    /// How it settled, if it has.
    pub fn now(&self) -> Option<Settled<Arc<T>, Arc<F>>> {
        match &*self.cell.state.lock() {
            State::Settled(settled) => Some(settled.clone()),
            State::Open | State::Taken => None,
        }
    }

    /// Wait for the settle event (never a timer), and say how the consumer met it.
    pub async fn settled(&mut self) -> Awaited<Arc<T>, Arc<F>> {
        let arrival = self.cell.wait(&mut self.receiver).await;
        let settled = match self.now() {
            Some(settled) => settled,
            // A shared artifact is never taken; the wait ended on a settle, so this is not reached.
            None => Settled::Abandoned,
        };
        Awaited { settled, arrival }
    }
}

/// A handoff's one consumer: the value itself moves to it. Dropped before the settle, the value is
/// dropped with the producer's delivery instead.
pub struct Handoff<T, F> {
    cell: Arc<Cell<T, F>>,
    receiver: watch::Receiver<bool>,
}

impl<T, F> Handoff<T, F> {
    /// Wait for the settle event and take what it settled with.
    pub async fn settled(mut self) -> Awaited<T, F> {
        let arrival = self.cell.wait(&mut self.receiver).await;
        let taken = std::mem::replace(&mut *self.cell.state.lock(), State::Taken);
        let settled = match taken {
            State::Settled(settled) => settled,
            State::Open | State::Taken => Settled::Abandoned, // unreachable: the wait ended on a settle and only this consumer takes
        };
        Awaited { settled, arrival }
    }
}

/// Open a shared artifact of `topic`.
pub fn artifact<T, F>(topic: TopicName) -> (Produces<Arc<T>, Arc<F>>, Subscribes<T, F>) {
    let cell = open(topic);
    let receiver = cell.settled.subscribe();
    (Produces { cell: Arc::clone(&cell) }, Subscribes { cell, receiver })
}

/// Open a handoff of `topic`: one producer, one consumer, the value moved.
pub fn handoff<T, F>(topic: TopicName) -> (Produces<T, F>, Handoff<T, F>) {
    let cell = open(topic);
    let receiver = cell.settled.subscribe();
    (Produces { cell: Arc::clone(&cell) }, Handoff { cell, receiver })
}
