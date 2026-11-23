//! An owner of running components: the one place a node (or a mind) starts its parts and the one
//! thing whose end stops them.
//!
//! A component's life is its owner's: started when the owner starts, stopped when the owner drops,
//! never a process-wide global started lazily by whoever calls first. Twice on 2026-11-22/23 such a
//! global (the clock, the room feed's worker) lived on the first caller's runtime and died with it,
//! silently: later deadlines never rang, later events went nowhere. A node builds one `Owner` at boot;
//! a test builds its own on its own bus.

use std::any::Any;

use super::{start, Bus, Component, Teller};
use crate::dataflow::OwnerId;

/// The components one owner runs on its bus. Dropping it stops them all, newest first.
pub struct Owner<'b> {
    bus: &'b Bus,
    id: OwnerId,
    epoch: u64,
    /// Each part's `Running` handle, kept only so its drop stops it.
    parts: Vec<Box<dyn Any + Send>>,
}

impl<'b> Owner<'b> {
    /// An owner of parts on `bus`, as `id` in its `epoch` (its incarnation: a restart is a new epoch).
    pub fn on(bus: &'b Bus, id: OwnerId, epoch: u64) -> Self {
        Self { bus, id, epoch, parts: Vec::new() }
    }

    /// Start `component` as one of this owner's parts; it runs until the owner drops. Returns a
    /// teller for handing it acts, which holds no lifetime.
    pub fn start<P: Component>(&mut self, component: P) -> Teller<P> {
        let running = start(component, self.bus, self.id, self.epoch);
        let teller = running.teller();
        self.parts.push(Box::new(running));
        teller
    }

    /// The bus its parts hear and say on.
    pub fn bus(&self) -> &'b Bus {
        self.bus
    }

    /// How many parts it runs.
    pub fn parts(&self) -> usize {
        self.parts.len()
    }
}

impl Drop for Owner<'_> {
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
    use crate::component::{Ctx, Wiring};
    use crate::Qos;

    struct Echo {
        heard: tokio::sync::mpsc::UnboundedSender<u32>,
    }

    impl Component for Echo {
        type Input = u32;
        const NAME: &'static str = "echo";
        const QOS: Qos = Qos::Work;
        fn wire(&self, _: &mut Wiring<'_, Self>) {}
        async fn handle(&mut self, input: u32, _: &mut Ctx<Self>) {
            assert!(self.heard.send(input).is_ok(), "the test reads every value");
        }
    }

    // what this catches: a component whose life is not its owner's (a lazily started global that
    // outlives or dies apart from the node). Its parts run while the owner lives; when the owner
    // drops, every part stops: an act told after that is refused, not lost into a dead worker.
    #[tokio::test]
    async fn an_owners_parts_run_while_it_lives_and_stop_when_it_drops() {
        let bus = Bus::new();
        let (heard, mut values) = tokio::sync::mpsc::unbounded_channel();
        let mut owner = Owner::on(&bus, OwnerId(uuid::Uuid::from_u128(9)), 1);
        let echo = owner.start(Echo { heard });
        assert_eq!(owner.parts(), 1);
        assert!(echo.tell(7).await, "a running part takes an act");
        assert_eq!(values.recv().await, Some(7));
        drop(owner);
        assert!(!echo.tell(8).await, "its owner gone, the part is stopped and says so");
        assert_eq!(values.recv().await, None, "nothing reached it after its owner dropped");
    }
}
