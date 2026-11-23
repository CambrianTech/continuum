//! Where a component declares what it hears: subscriptions by topic on its owner's bus, attached
//! once its intake exists, each window registered so its context can resize it live.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use parking_lot::Mutex;

use super::delivered::Intake;
use super::{Bus, Component, Key};
use crate::dataflow::{Event, Resize, Snapshot, TopicName};

/// One declared subscription, attached once the component's intake exists.
type Attach<I> = Box<dyn FnOnce(&Intake<I>) + Send>;

/// A component's subscription windows by topic, registered as each attaches; its context resizes them.
pub(super) type Windows = Arc<Mutex<HashMap<TopicName, Arc<dyn Resize>>>>;

/// Where a component declares its subscriptions.
pub struct Wiring<'a, P: Component> {
    bus: &'a Bus,
    /// Each subscription, attached to the component's input once its worker exists.
    attach: Vec<Attach<P::Input>>,
    /// How many inputs its subscriptions declared it keeps: the sum of its windows, one per watched
    /// state. Its intake holds that many beside one act, so a delivery moves its window in one wake.
    holds: usize,
    /// Its windows, handed to its context so it can resize them live.
    windows: Windows,
}

impl<'a, P: Component> Wiring<'a, P> {
    pub(super) fn new(bus: &'a Bus) -> Self {
        Self { bus, attach: Vec::new(), holds: 0, windows: Windows::default() }
    }

    /// The windows its context resizes.
    pub(super) fn windows(&self) -> Windows {
        Arc::clone(&self.windows)
    }

    /// Its intake: one act beside everything its subscriptions declared they keep.
    pub(super) fn intake(&self) -> std::num::NonZeroUsize {
        std::num::NonZeroUsize::MIN.saturating_add(self.holds)
    }

    /// Attach every declared subscription to its intake, now that it exists.
    pub(super) fn attach(self, input: &Intake<P::Input>) {
        for attach in self.attach {
            attach(input);
        }
    }

    /// Its owner's bus, where it claims the topics it says.
    pub fn bus(&self) -> &Bus {
        self.bus
    }

    /// Every event on its owner's `topic` is offered to this component, in order, keeping at most
    /// `window` it has not taken (what it keeps of the topic itself); its producer never waits on it.
    pub fn hear<T>(&mut self, topic: TopicName, window: NonZeroUsize)
    where
        T: Send + Sync + 'static,
        P::Input: From<Event<T>>,
    {
        self.hear_on::<T>(self.bus, topic, window);
    }

    /// Every event on `topic` of another bus (the node's: the one feed), as [`Self::hear`].
    pub fn hear_on<T>(&mut self, bus: &Bus, topic: TopicName, window: NonZeroUsize)
    where
        T: Send + Sync + 'static,
        P::Input: From<Event<T>>,
    {
        self.holds = self.holds.saturating_add(window.get());
        let subscribe = bus.subscribe::<T>(topic);
        let windows = Arc::clone(&self.windows);
        self.attach.push(Box::new(move |input| {
            let resize = subscribe.attach(input, window);
            windows.lock().insert(topic, resize);
        }));
    }

    /// This component is handed the newest value of its owner's `topic` under `key`, then each change;
    /// changes made while it is busy coalesce into the newest.
    pub fn watch<T>(&mut self, topic: TopicName, key: Key)
    where
        T: Send + Sync + 'static,
        P::Input: From<Snapshot<T>>,
    {
        self.holds = self.holds.saturating_add(1); // a watched state coalesces to its newest
        let reader = self.bus.reader::<T>(topic, key);
        self.attach.push(Box::new(move |input| reader.attach(input)));
    }
}
