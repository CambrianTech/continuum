//! An owner of running components: the one place a node or a mind starts its parts, and the one thing
//! whose end stops them. It does for every owner what each used to hand-build: the bus, the parts'
//! handles, their folded need, and resting them together.
//!
//! A component's life is its owner's: started when the owner starts, stopped when the owner drops,
//! never a process-wide global started lazily by whoever calls first. Twice on 2026-11-22/23 such a
//! global (the clock, the room feed's worker) lived on the first caller's runtime and died with it,
//! silently. A node builds one `Owner` at boot on the node's bus; a mind builds one on her own bus;
//! a test builds its own.

use std::any::Any;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use super::{need_key, node, start, Bus, Component, Need, Teller, NEED};
use crate::dataflow::{OwnerId, Reader};

/// Where an owner's parts hear and say: the node's bus, or one the owner holds (a mind's own).
enum Holds {
    Node(&'static Bus),
    Own(Arc<Bus>),
}

type Resting = Pin<Box<dyn Future<Output = bool> + Send>>;

/// One running part: its handle (dropped to stop it), its published need, and how to rest it.
struct Part {
    _running: Box<dyn Any + Send + Sync>,
    need: Reader<Need>,
    rest: Box<dyn Fn() -> Resting + Send + Sync>,
}

/// The components one owner runs on its bus. Dropping it stops them all, newest first.
pub struct Owner {
    bus: Holds,
    id: OwnerId,
    epoch: u64,
    parts: Vec<Part>,
}

impl Owner {
    /// The node's owner, on the node's bus, as `id` in its `epoch` (a restart is a new epoch).
    pub fn of_node(id: OwnerId, epoch: u64) -> Self {
        Self { bus: Holds::Node(node()), id, epoch, parts: Vec::new() }
    }

    /// An owner with a bus of its own (a mind's): her parts hear and say there, and nowhere else.
    pub fn with_bus(bus: Arc<Bus>, id: OwnerId, epoch: u64) -> Self {
        Self { bus: Holds::Own(bus), id, epoch, parts: Vec::new() }
    }

    /// Start `component` as one of this owner's parts; it runs until the owner drops. Returns a
    /// teller for handing it acts, which holds no lifetime.
    pub fn start<P: Component>(&mut self, component: P) -> Teller<P> {
        let running = start(component, self.bus(), self.id, self.epoch);
        let teller = running.teller();
        let resting = teller.clone();
        self.parts.push(Part {
            _running: Box::new(running),
            need: self.bus().reader::<Need>(NEED, need_key(P::NAME)),
            rest: Box::new(move || {
                let teller = resting.clone();
                Box::pin(async move { teller.rest().await })
            }),
        });
        teller
    }

    /// The bus its parts hear and say on.
    pub fn bus(&self) -> &Bus {
        match &self.bus {
            Holds::Node(bus) => bus,
            Holds::Own(bus) => bus,
        }
    }

    /// The bus, shared (an owned bus is handed out; the node's is static).
    pub fn shared_bus(&self) -> Option<Arc<Bus>> {
        match &self.bus {
            Holds::Node(_) => None,
            Holds::Own(bus) => Some(Arc::clone(bus)),
        }
    }

    /// Her need: the highest any part published, read now without waiting on any (a part that has
    /// said nothing yet counts as content). What a dispatcher ranks owners by.
    pub fn need(&self) -> Need {
        self.parts
            .iter()
            .filter_map(|part| part.need.latest().map(|snapshot| *snapshot.value))
            .fold(Need::default(), Need::max)
    }

    /// Rest every part (the owner rests, or its memory is wanted elsewhere): each gives back what
    /// it holds and wakes on its next input. True when every part took the request.
    pub async fn rest(&self) -> bool {
        let mut all = true;
        for part in &self.parts {
            all &= (part.rest)().await;
        }
        all
    }

    /// How many parts it runs.
    pub fn parts(&self) -> usize {
        self.parts.len()
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        // Newest first: a part started later may hear what an earlier one says.
        while let Some(part) = self.parts.pop() {
            drop(part);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::{Ctx, Urgency, Wiring};
    use crate::Qos;

    /// Echoes acts; says the urgency it is told to (an act of 0 means content, 1 want, 2 urgent).
    struct Echo {
        heard: tokio::sync::mpsc::UnboundedSender<u32>,
    }

    impl Component for Echo {
        type Input = u32;
        const NAME: &'static str = "echo";
        const QOS: Qos = Qos::Work;
        fn wire(&self, _: &mut Wiring<'_, Self>) {}
        async fn handle(&mut self, input: u32, ctx: &mut Ctx<Self>) {
            ctx.need(match input {
                0 => Urgency::Content,
                1 => Urgency::Want,
                _ => Urgency::Urgent,
            });
            assert!(self.heard.send(input).is_ok(), "the test reads every value");
        }
    }

    struct Quiet;

    impl Component for Quiet {
        type Input = ();
        const NAME: &'static str = "quiet";
        const QOS: Qos = Qos::Work;
        fn wire(&self, _: &mut Wiring<'_, Self>) {}
        async fn handle(&mut self, (): (), _: &mut Ctx<Self>) {}
    }

    fn owner() -> Owner {
        Owner::with_bus(Arc::new(Bus::new()), OwnerId(uuid::Uuid::from_u128(9)), 1)
    }

    // what this catches: a component whose life is not its owner's (a lazily started global that
    // outlives or dies apart from its node or mind). Its parts run while the owner lives; when the
    // owner drops, every part stops: an act told after that is refused, not lost into a dead worker.
    #[tokio::test]
    async fn an_owners_parts_run_while_it_lives_and_stop_when_it_drops() {
        let (heard, mut values) = tokio::sync::mpsc::unbounded_channel();
        let mut owner = owner();
        let echo = owner.start(Echo { heard });
        assert_eq!(owner.parts(), 1);
        assert!(echo.tell(7).await, "a running part takes an act");
        assert_eq!(values.recv().await, Some(7));
        drop(owner);
        assert!(!echo.tell(8).await, "its owner gone, the part is stopped and says so");
        assert_eq!(values.recv().await, None, "nothing reached it after its owner dropped");
    }

    // what this catches: a mind's need assembled by hand, or read by waiting on her parts. Her need is
    // the highest any part published, read without waiting; a part that said nothing counts as content;
    // and resting her rests every part.
    #[tokio::test]
    async fn her_need_folds_her_parts_and_resting_her_rests_them() {
        let (heard, mut values) = tokio::sync::mpsc::unbounded_channel();
        let mut owner = owner();
        let echo = owner.start(Echo { heard });
        let quiet = owner.start(Quiet);
        assert!(quiet.tell(()).await);
        assert!(echo.tell(2).await);
        assert_eq!(values.recv().await, Some(2));
        // The need is published once the act is handled: wait for the frame, never for a part.
        let mut need = owner.bus().reader::<Need>(NEED, need_key("echo"));
        while need.latest().map(|n| n.value.urgency) != Some(Urgency::Urgent) {
            let _ = need.changed().await;
        }
        assert_eq!(owner.need().urgency, Urgency::Urgent, "the loudest part decides");
        assert!(owner.rest().await, "every part takes the rest");
    }
}
