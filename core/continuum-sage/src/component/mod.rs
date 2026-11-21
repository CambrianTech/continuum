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

pub use bus::{Bus, Key, Taken};
pub use ctx::Ctx;

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
}

/// One input with the revision it came from (none for an act or a timer).
pub struct Delivered<I> {
    input: I,
    from: Option<InputRevision>,
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
        Self { input: I::from(event), from }
    }
}

impl<T, I: From<Snapshot<T>>> From<Snapshot<T>> for Delivered<I> {
    fn from(snapshot: Snapshot<T>) -> Self {
        let from = Some(snapshot.revision);
        Self { input: I::from(snapshot), from }
    }
}

struct Worker<P: Component> {
    component: P,
    ctx: Ctx<P>,
}

impl<P: Component> Stage for Worker<P> {
    type Input = Delivered<P::Input>;
    const QOS: Qos = P::QOS;

    async fn handle(&mut self, delivered: Delivered<P::Input>) {
        self.ctx.begin(delivered.from);
        self.component.handle(delivered.input, &mut self.ctx).await;
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
        self.input.send(Delivered { input, from: None }).await.is_ok()
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
        self.input.send(Delivered { input, from: None }).await.is_ok()
    }
}

/// Start `component` for `owner` on its owner's `bus`: its subscriptions, its worker, its context. One
/// input in hand and one waiting (the double buffer); every producer waits for room.
pub fn start<P: Component>(component: P, bus: &Bus, owner: OwnerId, epoch: u64) -> Running<P> {
    let mut wiring = Wiring { bus, attach: Vec::new() };
    component.wire(&mut wiring);
    let incarnation = StageIncarnation { stage: StageId::named(P::NAME), owner, epoch };
    let (input, worker) = spawn_stage_with(component, RunId { incarnation, seq: 0 });
    for attach in wiring.attach {
        attach(&input);
    }
    Running { input, worker }
}

fn spawn_stage_with<P: Component>(component: P, run: RunId) -> (Intake<P::Input>, StageWorker<Delivered<P::Input>>) {
    // The context needs the part's own input for its timers, which exists only once the worker
    // does: the worker is spawned with a context that receives it before any input is handled.
    let slot = ctx::InputSlot::default();
    let (input, worker) = spawn_stage(Worker { component, ctx: Ctx::new(run, &slot) }, Capacity::granted(std::num::NonZeroUsize::MIN));
    slot.fill(&input);
    (input, worker)
}

#[cfg(test)]
mod tests;
