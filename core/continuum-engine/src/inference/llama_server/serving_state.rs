//! The serving state's one writer. Every change the serving daemon makes goes through here: the
//! value changes and the same value is said on the bus under [`SERVING_TOPIC`], in one critical
//! section, so two writers can never say their changes in the reverse of the order they made them,
//! and nothing follows a channel to repeat what was said.

use std::sync::Arc;

use continuum_sage::component::{Bus, Key, Source, Taken};
use tokio::sync::watch;

use super::{ServingSnapshot, SERVING_TOPIC};

/// The serving state, held once and handed out by clone.
#[derive(Clone)]
pub struct ServingState {
    inner: Arc<Inner>,
}

struct Inner {
    /// Held across every change and its saying: the order of the bus is the order of the changes.
    said: parking_lot::Mutex<Source<ServingSnapshot>>,
    value: watch::Sender<ServingSnapshot>,
}

impl ServingState {
    /// The serving state on `bus`, starting at `initial`. `initial` is said before this returns,
    /// so a reader that arrives at any time reads a value, never an empty topic. A bus whose
    /// serving topic is already claimed is refused: one writer.
    pub fn on(bus: &Bus, initial: ServingSnapshot) -> Result<Self, Taken> {
        let mut said = Source::on(bus, SERVING_TOPIC, Key::ONE, "serving-state")?;
        let (value, _) = watch::channel(initial);
        say_now(&mut said, &value);
        Ok(Self { inner: Arc::new(Inner { said: parking_lot::Mutex::new(said), value }) })
    }

    /// Replace the state; returns the one it replaced.
    pub fn send_replace(&self, next: ServingSnapshot) -> ServingSnapshot {
        let mut said = self.inner.said.lock();
        let prior = self.inner.value.send_replace(next);
        say_now(&mut said, &self.inner.value);
        prior
    }

    /// Change the state in place.
    pub fn send_modify(&self, change: impl FnOnce(&mut ServingSnapshot)) {
        let mut said = self.inner.said.lock();
        self.inner.value.send_modify(change);
        say_now(&mut said, &self.inner.value);
    }

    /// Change the state in place when `change` says it changed it; said only then.
    pub fn send_if_modified(&self, change: impl FnOnce(&mut ServingSnapshot) -> bool) -> bool {
        let mut said = self.inner.said.lock();
        let changed = self.inner.value.send_if_modified(change);
        if changed {
            say_now(&mut said, &self.inner.value);
        }
        changed
    }

    /// The state now.
    pub fn borrow(&self) -> watch::Ref<'_, ServingSnapshot> {
        self.inner.value.borrow()
    }

    /// A receiver of every change (the readers not yet moved to the bus's serving topic).
    pub fn subscribe(&self) -> watch::Receiver<ServingSnapshot> {
        self.inner.value.subscribe()
    }
}

/// Say on the bus what `value` holds now: the one place the serving state is said (called under the
/// writer's lock, so the bus's order is the order of the changes).
fn say_now(said: &mut Source<ServingSnapshot>, value: &watch::Sender<ServingSnapshot>) {
    said.say(value.borrow().clone());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(model: &str) -> ServingSnapshot {
        ServingSnapshot { active_model: Some(model.to_owned()), ..ServingSnapshot::empty() }
    }

    // what this catches: a serving topic a late reader finds empty, a change the bus never hears
    // or hears out of order, an unchanged value said again, or a second writer on one bus.
    #[test]
    fn every_change_is_said_once_in_order_and_a_late_reader_reads_the_newest() {
        let bus = Bus::new();
        let state = ServingState::on(&bus, ServingSnapshot::empty()).expect("test: the first writer");
        assert!(ServingState::on(&bus, ServingSnapshot::empty()).is_err(), "a second writer is refused");
        let early = bus.reader::<ServingSnapshot>(SERVING_TOPIC, Key::ONE);
        assert_eq!(early.latest().map(|s| s.provenance.by.seq), Some(1), "the initial value is said before anyone reads");

        state.send_replace(named("a"));
        state.send_modify(|s| s.ready = true);
        assert!(!state.send_if_modified(|_| false), "an unchanged value is not said again");

        let late = bus.reader::<ServingSnapshot>(SERVING_TOPIC, Key::ONE);
        let newest = late.latest().expect("test: a late reader reads a value");
        assert_eq!(newest.provenance.by.seq, 3, "three sayings: initial, replace, modify");
        assert_eq!((newest.value.active_model.as_deref(), newest.value.ready), (Some("a"), true), "the newest change, as made");
        assert_eq!(*state.borrow(), *newest.value, "the bus and the state agree");
    }

    // what this catches: two writers whose changes reach the bus in the reverse of the order they
    // were made, so the bus ends on a stale value while the state holds the newest.
    #[test]
    fn concurrent_writers_leave_the_bus_on_the_newest_change() {
        let bus = Bus::new();
        let state = ServingState::on(&bus, ServingSnapshot::empty()).expect("test: the writer");
        let writers: Vec<_> = (0..8)
            .map(|w| {
                let state = state.clone();
                std::thread::spawn(move || {
                    for i in 0..200 {
                        state.send_replace(named(&format!("{w}-{i}")));
                    }
                })
            })
            .collect();
        for writer in writers {
            writer.join().expect("test: a writer finished");
        }
        let newest = bus.reader::<ServingSnapshot>(SERVING_TOPIC, Key::ONE).latest().expect("test: said");
        assert_eq!(*state.borrow(), *newest.value, "the bus ends where the state ends");
        assert_eq!(newest.provenance.by.seq, 1 + 8 * 200, "every change said exactly once");
    }
}
