//! SAGE's `Component`: what everything that works on its own inherits (object model 5.7). A part
//! of a mind, a stage of the Core, the node's feed, a grid service: each declares what it hears,
//! what it says, and how it handles one input, and the engine gives it everything else, once:
//!
//! - its worker: its own task, a bounded intake whose producers wait for room, in order;
//! - its subscriptions, wired by topic on its owner's [`Bus`], so a component observes another's
//!   output by naming the topic, never by holding the other component;
//! - provenance: each publication names the component's run and the revision of the input it
//!   handled;
//! - timers: [`Ctx::after`] delivers an input at a time the component chose, replacing any earlier
//!   timer under the same key, so none holds a handle to itself and none polls;
//! - lifecycle: [`Running`] stops it when dropped, and its last publications stay readable.
//!
//! Components work on their own and signal what they have when they have it; whoever cares
//! subscribes, and whoever reads a state reads the newest without waiting (CBAR's frame pipeline).
//! What a kind of owner adds (a mind: her bus, her identity, starting her parts) is a thin layer
//! above this, never a copy of it.

mod bus;
mod ctx;
mod known;
mod need;

pub use bus::{Bus, Key, Taken};
pub use ctx::Ctx;
pub use known::{Known, LastSeen, Novelty, Seen};
pub use need::{need_key, Need, Urgency, NEED};

use std::future::Future;

use crate::dataflow::{Event, InputRevision, OwnerId, RunId, Snapshot, StageId, StageIncarnation, TopicName};
use crate::{spawn_stage, Capacity, Qos, Stage, StageInput, StageWorker};

/// Something that works on its own. Implement this and nothing else: [`start`] gives it the mechanism.
pub trait Component: Send + Sized + 'static {
    /// Everything that wakes it: each subscription's events or states, its owner's acts, its timers.
    type Input: Send + 'static;
    /// Its name, the same wherever it runs (its stage id).
    const NAME: &'static str;
    /// Its class, for any capacity it submits to.
    const QOS: Qos;
    /// Declare what it hears.
    fn wire(&self, wiring: &mut Wiring<'_, Self>);
    /// Handle one input: arithmetic on its own state, publishing through what it said it says.
    fn handle(&mut self, input: Self::Input, ctx: &mut Ctx<Self>) -> impl Future<Output = ()> + Send;
    /// Rest: give back what it holds (drop or page its caches) because its owner rests or its
    /// memory is wanted elsewhere. Its next input wakes it. The default holds nothing and does
    /// nothing.
    fn rest(&mut self) -> impl Future<Output = ()> + Send {
        async {}
    }
}

/// Where a component is in its life, owned by the engine and published on its owner's bus under
/// [`PRESENCE`], keyed by the component's name: any component can observe another's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// Handling an input.
    Acting,
    /// Idle on its intake: no CPU spent, its caches warm.
    Ready,
    /// It gave back what it holds; its next input wakes it.
    Resting,
}

/// The topic every component's presence is published on.
pub const PRESENCE: TopicName = TopicName("sage/presence");

/// The key a component's presence is published under: its name.
pub fn presence_key(name: &'static str) -> Key {
    Key::of(StageId::named(name).as_uuid())
}

/// What reaches a component's worker: an input (with the frame it came from: none for an act or a
/// timer), or the engine's request that it rest.
pub struct Delivered<I> {
    what: What<I>,
    from: Option<InputRevision>,
}

enum What<I> {
    Input(I),
    Rest,
}

impl<I> Delivered<I> {
    pub(super) fn act(input: I) -> Self {
        Self { what: What::Input(input), from: None }
    }
}

/// A component's intake: its inputs with the revision each came from.
type Intake<I> = StageInput<Delivered<I>>;

/// One declared subscription, attached once the component's intake exists.
type Attach<I> = Box<dyn FnOnce(&Intake<I>) + Send>;

/// Where a component declares its subscriptions.
pub struct Wiring<'a, P: Component> {
    bus: &'a Bus,
    /// Each subscription, attached to the component's input once its worker exists.
    attach: Vec<Attach<P::Input>>,
}

impl<P: Component> Wiring<'_, P> {
    /// Its owner's bus, where it claims the topics it says.
    pub fn bus(&self) -> &Bus {
        self.bus
    }

    /// Every event on its owner's `topic` reaches this component, in order, never dropped.
    pub fn hear<T>(&mut self, topic: TopicName)
    where
        T: Send + Sync + 'static,
        P::Input: From<Event<T>>,
    {
        self.hear_on::<T>(self.bus, topic);
    }

    /// Every event on `topic` of another bus (the node's: the one feed) reaches this component.
    pub fn hear_on<T>(&mut self, bus: &Bus, topic: TopicName)
    where
        T: Send + Sync + 'static,
        P::Input: From<Event<T>>,
    {
        let subscribe = bus.subscribe::<T>(topic);
        self.attach.push(Box::new(move |input| subscribe.attach(input)));
    }

    /// This component is handed the newest value of its owner's `topic` under `key`, then each change;
    /// changes made while it is busy coalesce into the newest.
    pub fn watch<T>(&mut self, topic: TopicName, key: Key)
    where
        T: Send + Sync + 'static,
        P::Input: From<Snapshot<T>>,
    {
        let reader = self.bus.reader::<T>(topic, key);
        self.attach.push(Box::new(move |input| reader.attach(input)));
    }
}

impl<T, I: From<Event<T>>> From<Event<T>> for Delivered<I> {
    fn from(event: Event<T>) -> Self {
        let from = Some(event.revision);
        Self { what: What::Input(I::from(event)), from }
    }
}

impl<T, I: From<Snapshot<T>>> From<Snapshot<T>> for Delivered<I> {
    fn from(snapshot: Snapshot<T>) -> Self {
        let from = Some(snapshot.revision);
        Self { what: What::Input(I::from(snapshot)), from }
    }
}

struct Worker<P: Component> {
    component: P,
    ctx: Ctx<P>,
    /// Its presence on its owner's bus; `None` when another component of the same name already
    /// publishes there (a wiring defect, probed at start).
    presence: Option<crate::dataflow::Publisher<Presence>>,
    /// Its need on its owner's bus (same keying and defect rule as its presence).
    need: Option<crate::dataflow::Publisher<Need>>,
    published_need: Option<Need>,
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
                self.enter(Presence::Acting);
                self.ctx.begin(delivered.from);
                self.component.handle(input, &mut self.ctx).await;
                self.enter(Presence::Ready);
                self.publish_need();
            }
            What::Rest if self.now == Presence::Resting => {}
            What::Rest => {
                self.component.rest().await;
                self.enter(Presence::Resting);
            }
        }
    }
}

/// A running part. Its owner tells it acts; dropping it stops the part.
pub struct Running<P: Component> {
    input: StageInput<Delivered<P::Input>>,
    worker: StageWorker<Delivered<P::Input>>,
}

impl<P: Component> Running<P> {
    /// Hand the part an act; false once it has stopped.
    pub async fn tell(&self, input: P::Input) -> bool {
        self.input.send(Delivered::act(input)).await.is_ok()
    }

    /// Ask it to rest: it gives back what it holds, and its next input wakes it. False once it
    /// has stopped.
    pub async fn rest(&self) -> bool {
        self.input.send(Delivered { what: What::Rest, from: None }).await.is_ok()
    }

    /// A handle to tell it acts from elsewhere, without owning its lifetime.
    pub fn teller(&self) -> Teller<P> {
        Teller { input: self.input.clone() }
    }
}

impl<P: Component> Drop for Running<P> {
    fn drop(&mut self) {
        self.worker.stop();
    }
}

/// Tells a running part acts; holds no lifetime.
pub struct Teller<P: Component> {
    input: StageInput<Delivered<P::Input>>,
}

impl<P: Component> Clone for Teller<P> {
    fn clone(&self) -> Self {
        Self { input: self.input.clone() }
    }
}

impl<P: Component> Teller<P> {
    pub async fn tell(&self, input: P::Input) -> bool {
        self.input.send(Delivered::act(input)).await.is_ok()
    }

    /// Ask it to rest (its owner rests, or its memory is wanted elsewhere).
    pub async fn rest(&self) -> bool {
        self.input.send(Delivered { what: What::Rest, from: None }).await.is_ok()
    }
}

/// Claim a topic the engine publishes for a component; a second component of the same name on one
/// bus is a wiring defect, probed, and runs without it.
fn claim<T: Send + Sync + 'static>(bus: &Bus, topic: TopicName, name: &'static str) -> Option<crate::dataflow::Publisher<T>> {
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
    let mut wiring = Wiring { bus, attach: Vec::new() };
    component.wire(&mut wiring);
    let incarnation = StageIncarnation { stage: StageId::named(P::NAME), owner, epoch };
    // The context needs the component's own input for its timers, which exists only once the
    // worker does: the worker is spawned with a context that receives it before any input.
    let slot = ctx::InputSlot::default();
    let mut worker = Worker {
        component,
        ctx: Ctx::new(RunId { incarnation, seq: 0 }, &slot),
        presence: claim(bus, PRESENCE, P::NAME),
        need: claim(bus, NEED, P::NAME),
        published_need: None,
        now: Presence::Ready,
    };
    if let Some(publisher) = worker.presence.as_mut() {
        worker.ctx.publish(publisher, Presence::Ready);
    }
    // A fresh intake holds nothing, and it has said nothing yet: content.
    worker.say_need(Need { urgency: Urgency::Content, waiting: 0 });
    let (input, worker) = spawn_stage(worker, Capacity::granted(std::num::NonZeroUsize::MIN));
    slot.fill(&input);
    for attach in wiring.attach {
        attach(&input);
    }
    Running { input, worker }
}

#[cfg(test)]
mod tests;
