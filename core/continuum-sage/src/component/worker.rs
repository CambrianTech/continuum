//! The engine behind every component: its worker task, and [`start`], which wires it. The worker
//! owns the component, publishes its presence and need, and times every input through its [`Meter`].

use super::delivered::{Delivered, What};
use super::observe::Meter;
use super::wiring::Wiring;
use super::{ctx, presence_key, Bus, Component, Ctx, Need, Observe, Presence, Running, Telemetry, Urgency, NEED, OBSERVE, PRESENCE, TELEMETRY};
use crate::dataflow::{OwnerId, Publisher, Reader, RunId, StageId, StageIncarnation, TopicName};
use crate::{spawn_stage, Capacity, Qos, Stage};

struct Worker<P: Component> {
    component: P,
    ctx: Ctx<P>,
    /// Its presence on its owner's bus; `None` when another component of the same name already
    /// publishes there (a wiring defect, probed at start).
    presence: Option<Publisher<Presence>>,
    /// Its need on its owner's bus (same keying and defect rule as its presence).
    need: Option<Publisher<Need>>,
    published_need: Option<Need>,
    /// Its observation switch (a probe line per input while on).
    observe: Reader<Observe>,
    /// Its measured numbers, published as telemetry after every input.
    telemetry: Option<Publisher<Telemetry>>,
    meter: Meter,
    now: Presence,
}

impl<P: Component> Worker<P> {
    /// Publish its need when it changed: its urgency and its intake depth now.
    fn publish_need(&mut self) {
        let Some(need) = self.ctx.current_need() else { return };
        self.say_need(need);
    }

    fn say_need(&mut self, need: Need) {
        if self.published_need == Some(need) {
            return;
        }
        self.published_need = Some(need);
        if let Some(publisher) = self.need.as_mut() {
            self.ctx.publish(publisher, need);
        }
    }

    fn enter(&mut self, presence: Presence) {
        if presence == self.now {
            return;
        }
        self.now = presence;
        if let Some(publisher) = self.presence.as_mut() {
            self.ctx.publish(publisher, presence);
        }
    }
}

impl<P: Component> Stage for Worker<P> {
    type Input = Delivered<P::Input>;
    const QOS: Qos = P::QOS;

    async fn handle(&mut self, delivered: Delivered<P::Input>) {
        match delivered.what {
            What::Input(input) => {
                let watched = self.observe.latest().is_some_and(|switch| *switch.value == Observe::Timing);
                let started = std::time::Instant::now();
                let waited = started.saturating_duration_since(delivered.queued);
                self.enter(Presence::Acting);
                self.ctx.begin(delivered.from);
                self.component.handle(input, &mut self.ctx).await;
                self.enter(Presence::Ready);
                self.publish_need();
                // Before its intake exists nothing waits in it.
                let waiting = self.ctx.current_need().map_or(0, |need| need.waiting);
                let measured = self.meter.record(waited, started.elapsed(), delivered.from, waiting);
                if let Some(publisher) = self.telemetry.as_mut() {
                    self.ctx.publish(publisher, measured);
                }
                if watched {
                    continuum_base::probe!(class = "sage.component.handled", component = P::NAME, waited_us = measured.last_wait.as_micros() as u64, took_us = measured.last_handle.as_micros() as u64, waiting = waiting, frames_skipped = measured.frames_skipped, "a watched component handled an input");
                }
            }
            What::Rest if self.now == Presence::Resting => {}
            What::Rest => {
                self.component.rest().await;
                self.enter(Presence::Resting);
            }
        }
    }
}

/// Claim a topic the engine publishes for a component; a second component of the same name on one
/// bus is a wiring defect, probed, and runs without it.
fn claim<T: Send + Sync + 'static>(bus: &Bus, topic: TopicName, name: &'static str) -> Option<Publisher<T>> {
    match bus.publisher::<T>(topic, presence_key(name)) {
        Ok(publisher) => Some(publisher),
        Err(taken) => {
            continuum_base::probe!(class = "sage.component.topic_taken", component = name, topic = taken.0 .0, "another component of this name already publishes this on its bus");
            None
        }
    }
}

/// Start `component` for `owner` on its owner's `bus`: its subscriptions, its worker, its context. One
/// input in hand and one waiting (the double buffer); every producer waits for room.
pub fn start<P: Component>(component: P, bus: &Bus, owner: OwnerId, epoch: u64) -> Running<P> {
    let mut wiring = Wiring::new(bus);
    component.wire(&mut wiring);
    let incarnation = StageIncarnation { stage: StageId::named(P::NAME), owner, epoch };
    // The context needs the component's own input for its timers, which exists only once the
    // worker does: the worker is spawned with a context that receives it before any input.
    let slot = ctx::InputSlot::default();
    let mut worker = Worker {
        component,
        ctx: Ctx::new(RunId { incarnation, seq: 0 }, &slot, wiring.windows()),
        presence: claim(bus, PRESENCE, P::NAME),
        need: claim(bus, NEED, P::NAME),
        published_need: None,
        observe: bus.reader::<Observe>(OBSERVE, presence_key(P::NAME)),
        telemetry: claim(bus, TELEMETRY, P::NAME),
        meter: Meter::default(),
        now: Presence::Ready,
    };
    if let Some(publisher) = worker.presence.as_mut() {
        worker.ctx.publish(publisher, Presence::Ready);
    }
    // A fresh intake holds nothing, and it has said nothing yet: content.
    worker.say_need(Need { urgency: Urgency::Content, waiting: 0 });
    let (input, worker) = spawn_stage(worker, Capacity::granted(std::num::NonZeroUsize::MIN));
    slot.fill(&input);
    wiring.attach(&input);
    Running::new(input, worker)
}
