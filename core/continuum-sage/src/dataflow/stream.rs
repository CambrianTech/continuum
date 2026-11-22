//! An event topic: every event is offered to every subscriber, in order, and nobody waits on anybody.
//!
//! CBAR's event lane (object model 5.7, "built as CBAR"): a percept, a receipt, a surprise. One
//! [`Emitter`] owns the topic; a worker subscribes by [`attach`](Subscribe::attach)ing its intake with
//! the window it can hold, and from then on is handed each event by handle (`Arc`).
//!
//! An emit never waits (Joel, 2026-11-22: one part must never hold another back). It appends the event
//! to each subscriber's own window and returns. Each subscription has its own delivery task that hands
//! its window's events to its worker in order: only that task ever waits for its worker's room, the way
//! a [`super::Reader`]'s delivery does. A subscriber that falls a whole window behind loses its oldest
//! events, and the gap shows in the revisions it is handed (frames behind, never silence); a consumer
//! that needs every event reads the topic's durable truth by cursor from the revision it last saw.
//!
//! This replaced a stream whose emit waited on each subscriber in turn, so the room feed stalled for
//! every resident behind the slowest one's senses (docs/planning/LIVE-PATH-SERIAL-WAITS.md, #7).

use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::Notify;

use super::topic::{InputRevision, TopicName};
use crate::StageInput;

/// One event, shared by handle, with the cursor a consumer cites in its own provenance.
#[derive(Debug)]
pub struct Event<T> {
    pub value: Arc<T>,
    pub revision: InputRevision,
}

impl<T> Clone for Event<T> {
    fn clone(&self) -> Self {
        Self { value: Arc::clone(&self.value), revision: self.revision }
    }
}

/// One subscriber's window: the events it has not been handed yet, newest last.
struct Window<T> {
    events: Mutex<VecDeque<Event<T>>>,
    holds: NonZeroUsize,
    /// Raised on every append; a permit is stored when the delivery is busy, so no wake is lost.
    appended: Notify,
    /// Its worker stopped: the emitter lets it go.
    gone: AtomicBool,
    /// The emitter is gone: the delivery hands what is left and ends, letting its worker go.
    ended: AtomicBool,
}

impl<T> Window<T> {
    /// Append, displacing the oldest when full. Never waits.
    fn append(&self, event: Event<T>) {
        let mut events = self.events.lock();
        if events.len() == self.holds.get() {
            events.pop_front();
        }
        events.push_back(event);
        drop(events);
        self.appended.notify_one();
    }
}

type Subscribers<T> = Arc<Mutex<Vec<Arc<Window<T>>>>>;

/// The topic's one writer. Not `Clone`: an event stream has one owner.
pub struct Emitter<T> {
    topic: TopicName,
    emitted: u64,
    subscribers: Subscribers<T>,
}

/// Where workers subscribe. Cheap to clone; held by whoever wires the graph.
pub struct Subscribe<T> {
    subscribers: Subscribers<T>,
}

impl<T> Clone for Subscribe<T> {
    fn clone(&self) -> Self {
        Self { subscribers: Arc::clone(&self.subscribers) }
    }
}

/// An event topic with no subscribers yet.
pub fn stream<T: Send + Sync + 'static>(topic: TopicName) -> (Emitter<T>, Subscribe<T>) {
    let subscribers: Subscribers<T> = Arc::default();
    (Emitter { topic, emitted: 0, subscribers: Arc::clone(&subscribers) }, Subscribe { subscribers })
}

impl<T: Send + Sync + 'static> Subscribe<T> {
    /// Hand every event emitted from now on to the worker behind `input`, holding at most `window`
    /// events it has not taken yet (what the consumer itself keeps of this topic).
    pub fn attach<I>(&self, input: &StageInput<I>, window: NonZeroUsize)
    where
        I: From<Event<T>> + Send + 'static,
    {
        let window = Arc::new(Window {
            events: Mutex::new(VecDeque::new()),
            holds: window,
            appended: Notify::new(),
            gone: AtomicBool::new(false),
            ended: AtomicBool::new(false),
        });
        self.subscribers.lock().push(Arc::clone(&window));
        let input = input.clone();
        // Its own delivery: the only place this subscription ever waits, for its own worker's room.
        tokio::spawn(async move {
            loop {
                let next = window.events.lock().pop_front();
                match next {
                    Some(event) => {
                        if input.send(I::from(event)).await.is_err() {
                            window.gone.store(true, Ordering::Release);
                            return;
                        }
                    }
                    None if window.ended.load(Ordering::Acquire) => return,
                    None => window.appended.notified().await,
                }
            }
        });
    }
}

impl<T: Send + Sync + 'static> Emitter<T> {
    /// Offer `value` to every subscriber and return its revision. Never waits.
    pub fn emit(&mut self, value: T) -> InputRevision {
        self.emitted += 1;
        let event = Event { value: Arc::new(value), revision: InputRevision { topic: self.topic, through: self.emitted } };
        let mut subscribers = self.subscribers.lock();
        subscribers.retain(|window| !window.gone.load(Ordering::Acquire));
        for window in subscribers.iter() {
            window.append(event.clone());
        }
        event.revision
    }
}

impl<T> Drop for Emitter<T> {
    fn drop(&mut self) {
        for window in self.subscribers.lock().iter() {
            window.ended.store(true, Ordering::Release);
            window.appended.notify_one();
        }
    }
}
