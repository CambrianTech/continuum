//! An event topic: every event reaches every subscriber, in order, never dropped.
//!
//! CBAR's reliable lane (object model 5.7, "built as CBAR"): for what must each be seen (a percept, a
//! receipt, a surprise, a request to deliberate). One [`Emitter`] owns the topic; a worker subscribes
//! by [`attach`](Subscribe::attach)ing its intake, and from then on is handed every event by handle
//! (`Arc`). An emit waits until every subscriber has room (backpressure), so a slow subscriber slows
//! its producer instead of losing events. A subscriber whose worker has stopped is let go; the others
//! keep receiving.

use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;

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

#[async_trait]
trait Subscriber<T>: Send + Sync {
    /// False when the subscriber's worker has stopped.
    async fn deliver(&self, event: &Event<T>) -> bool;
}

struct Intake<I>(StageInput<I>);

#[async_trait]
impl<T, I> Subscriber<T> for Intake<I>
where
    T: Send + Sync + 'static,
    I: From<Event<T>> + Send + 'static,
{
    async fn deliver(&self, event: &Event<T>) -> bool {
        let event = event.clone(); // each subscriber is handed its own handle (an Arc)
        self.0.send(I::from(event)).await.is_ok()
    }
}

type Subscribers<T> = Arc<Mutex<Vec<Arc<dyn Subscriber<T>>>>>;

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
    /// Hand every event emitted from now on to the worker behind `input`.
    pub fn attach<I>(&self, input: &StageInput<I>)
    where
        I: From<Event<T>> + Send + 'static,
    {
        self.subscribers.lock().push(Arc::new(Intake(input.clone())));
    }
}

impl<T: Send + Sync + 'static> Emitter<T> {
    /// Deliver `value` to every subscriber, waiting for each one's room. Returns its revision.
    pub async fn emit(&mut self, value: T) -> InputRevision {
        self.emitted += 1;
        let event = Event { value: Arc::new(value), revision: InputRevision { topic: self.topic, through: self.emitted } };
        // The list is copied under the lock and delivered to outside it: no lock across an await.
        let subscribers = self.subscribers.lock().clone();
        let mut stopped: Vec<Arc<dyn Subscriber<T>>> = Vec::new();
        for subscriber in subscribers {
            if !subscriber.deliver(&event).await {
                stopped.push(subscriber);
            }
        }
        if !stopped.is_empty() {
            self.subscribers.lock().retain(|subscriber| !stopped.iter().any(|gone| Arc::ptr_eq(gone, subscriber)));
        }
        event.revision
    }
}
