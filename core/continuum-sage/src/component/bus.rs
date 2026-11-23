//! An owner's bus (a mind's, the node's): its topics by name, key and type, created on first use by whoever names them
//! first, publisher or subscriber. A component observes another's output by naming its topic; it
//! never holds the other component. Each topic has one writer: the second claim is refused.

use std::any::{Any, TypeId};
use std::collections::HashMap;

use parking_lot::Mutex;

use crate::dataflow::{state, stream, Emitter, Publisher, Reader, Subscribe, TopicName};

/// Which instance of a topic: one per room, per card, per activity. [`Key::ONE`] for a topic with
/// a single instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Key(pub u128);

impl Key {
    pub const ONE: Key = Key(0);

    pub fn of(id: uuid::Uuid) -> Key {
        Key(id.as_u128())
    }
}

/// A topic's one writer is already claimed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Taken(pub TopicName);

struct StateTopic<T> {
    reader: Reader<T>,
    publisher: Option<Publisher<T>>,
}

struct StreamTopic<T> {
    subscribe: Subscribe<T>,
    emitter: Option<Emitter<T>>,
}

type Topics = HashMap<(TopicName, Key, TypeId), Box<dyn Any + Send>>;

/// An owner's topics (a mind's, or the node's: the one feed is a topic on the node's bus).
#[derive(Default)]
pub struct Bus {
    states: Mutex<Topics>,
    streams: Mutex<Topics>,
}

impl Bus {
    pub fn new() -> Self {
        Self::default()
    }

    /// The newest value of `topic` under `key`, read without waiting on its writer.
    pub fn reader<T: Send + Sync + 'static>(&self, topic: TopicName, key: Key) -> Reader<T> {
        self.with_state::<T, _>(topic, key, |entry| entry.reader.clone())
    }

    /// Claim `topic`'s one writer.
    pub fn publisher<T: Send + Sync + 'static>(&self, topic: TopicName, key: Key) -> Result<Publisher<T>, Taken> {
        self.with_state::<T, _>(topic, key, |entry| entry.publisher.take()).ok_or(Taken(topic))
    }

    /// Where a component subscribes to every event on `topic`.
    pub fn subscribe<T: Send + Sync + 'static>(&self, topic: TopicName) -> Subscribe<T> {
        self.with_stream::<T, _>(topic, |entry| entry.subscribe.clone())
    }

    /// Claim `topic`'s one emitter.
    pub fn emitter<T: Send + Sync + 'static>(&self, topic: TopicName) -> Result<Emitter<T>, Taken> {
        self.with_stream::<T, _>(topic, |entry| entry.emitter.take()).ok_or(Taken(topic))
    }

    fn with_state<T: Send + Sync + 'static, R>(&self, topic: TopicName, key: Key, f: impl FnOnce(&mut StateTopic<T>) -> R) -> R {
        let mut topics = self.states.lock();
        let id = (topic, key, TypeId::of::<T>());
        // Keyed by the type's id, so an entry found is that type.
        if let Some(entry) = topics.get_mut(&id).and_then(|entry| entry.downcast_mut::<StateTopic<T>>()) {
            return f(entry);
        }
        let (publisher, reader) = state::<T>(topic);
        let mut entry = StateTopic { reader, publisher: Some(publisher) };
        let out = f(&mut entry);
        topics.insert(id, Box::new(entry));
        out
    }

    fn with_stream<T: Send + Sync + 'static, R>(&self, topic: TopicName, f: impl FnOnce(&mut StreamTopic<T>) -> R) -> R {
        let mut topics = self.streams.lock();
        let id = (topic, Key::ONE, TypeId::of::<T>());
        if let Some(entry) = topics.get_mut(&id).and_then(|entry| entry.downcast_mut::<StreamTopic<T>>()) {
            return f(entry);
        }
        let (emitter, subscribe) = stream::<T>(topic);
        let mut entry = StreamTopic { subscribe, emitter: Some(emitter) };
        let out = f(&mut entry);
        topics.insert(id, Box::new(entry));
        out
    }
}

/// The node's own bus: topics every part of this process shares whatever crate starts it (the one
/// room feed, the model being served, its placement). A mind's topics live on her own bus; a node
/// fact lives here, below every crate that hears or says it, so no layer reaches up for it.
pub fn node() -> &'static Bus {
    static NODE: std::sync::OnceLock<Bus> = std::sync::OnceLock::new();
    NODE.get_or_init(Bus::new)
}
