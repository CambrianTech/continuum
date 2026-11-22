//! What a component's handler is given: provenance for what it publishes, and its timers.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use super::{Delivered, Key, Component};
use crate::dataflow::{Emitter, InputRevision, Provenance, Publisher, RunId};
use crate::StageInput;

/// The component's own input, filled once its worker exists (before any input is handled).
pub(super) struct InputSlot<I>(Arc<OnceLock<StageInput<Delivered<I>>>>);

impl<I> Default for InputSlot<I> {
    fn default() -> Self {
        Self(Arc::new(OnceLock::new()))
    }
}

impl<I> Clone for InputSlot<I> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<I> InputSlot<I> {
    pub(super) fn fill(&self, input: &StageInput<Delivered<I>>) {
        // Filled exactly once, by `start`, before the component handles anything.
        let input = input.clone(); // the context keeps its own handle
        self.0.get_or_init(move || input);
    }
}

/// A component's context: its run, the input it is handling, and its timers.
pub struct Ctx<P: Component> {
    run: RunId,
    from: Option<InputRevision>,
    myself: InputSlot<P::Input>,
    /// Its pending deadlines on the node's clock; dropping one cancels it.
    timers: HashMap<Key, crate::clock::Alarm>,
    /// How much it wants to act, in its own judgment; published by the engine after each input.
    urgency: super::Urgency,
}

impl<P: Component> Ctx<P> {
    pub(super) fn new(run: RunId, myself: &InputSlot<P::Input>) -> Self {
        Self { run, from: None, myself: myself.clone(), timers: HashMap::new(), urgency: super::Urgency::Content }
    }

    pub(super) fn begin(&mut self, from: Option<InputRevision>) {
        self.run.seq += 1;
        self.from = from;
    }

    /// Who produced what is published now, from which input.
    pub fn provenance(&self) -> Provenance {
        Provenance { by: self.run, from: self.from.into_iter().collect() }
    }

    /// Say how much it wants to act. The engine publishes it, with its intake depth beside it,
    /// once this input is handled; it stands until the component says otherwise.
    pub fn need(&mut self, urgency: super::Urgency) {
        self.urgency = urgency;
    }

    /// Its need as the engine publishes it: its urgency and the inputs waiting in its intake;
    /// `None` before its intake exists.
    pub(super) fn current_need(&self) -> Option<super::Need> {
        let waiting = self.myself.0.get()?.waiting();
        Some(super::Need { urgency: self.urgency, waiting })
    }

    /// Publish a state this component writes, stamped with its provenance.
    pub fn publish<T: Send + Sync + 'static>(&self, publisher: &mut Publisher<T>, value: T) -> InputRevision {
        publisher.publish(value, self.provenance())
    }

    /// Emit an event this component writes. Never waits: each subscriber has its own window.
    pub fn emit<T: Send + Sync + 'static>(&self, emitter: &mut Emitter<T>, value: T) -> InputRevision {
        emitter.emit(value)
    }

    /// Deliver `input` to this component `after` from now, replacing any timer under `key`: a
    /// deadline it chose, kept on the node's one clock, never a poll.
    pub fn after(&mut self, key: Key, after: Duration, input: P::Input) {
        self.cancel(key);
        let Some(myself) = self.myself.0.get().cloned() else { return };
        let alarm = crate::clock::at(tokio::time::Instant::now() + after, move || {
            tokio::spawn(async move {
                if myself.send(Delivered::act(input)).await.is_err() {
                    continuum_base::probe!(class = "sage.component.timer_after_stop", component = P::NAME, "a component stopped before its timer fell due");
                }
            });
        });
        self.timers.insert(key, alarm);
    }

    /// Drop the timer under `key`, if any.
    pub fn cancel(&mut self, key: Key) {
        self.timers.remove(&key);
    }
}
