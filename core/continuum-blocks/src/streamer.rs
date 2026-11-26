//! The Streamer: the one SAGE component that places blocks (UNIFIED-PAGING.md, "Streamer"; RAGE's streaming system).
//! Joel, 2026-11-26: "in a sage component too, so it's in its own thread. Very good parallelism pathway like if you
//! read on the rage engine"; "this transfer logic needs its own object done right so you reuse and do it correctly.
//! All cache transfers done correctly."
//!
//! A consumer never moves its own bytes. It says what it will need soonest ([`Demand`], the lane's forecast of who
//! steps next), and reads where its blocks are ([`Placements`]). The Streamer, on its own worker, moves each block
//! through the one [`Link`] its pair of tiers has: a pointer, the OS's paging, or one DMA of one block with a fence.
//! Never a copy loop, a staging copy or an encoding: those are not links. One move per link at a time, soonest first;
//! a block's source stays where it is, charged and valid, until its move lands.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::Arc;
use std::time::{Duration, Instant};

use continuum_sage::component::{Component, Ctx, Key, Wiring};
use continuum_sage::dataflow::{Publisher, Snapshot, TopicName};
use continuum_sage::Qos;

use crate::link::{Link, Transfer};
use crate::{Block, Placement};

/// What a consumer will need, soonest first (the lane: who steps next).
#[derive(Debug, Clone, PartialEq)]
pub struct Demand<K> {
    pub soonest: Vec<K>,
}

/// One block's row: where it is, whether a move is in flight, and its last move's measured time (what a lane slice
/// before it must cover).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row<K> {
    pub key: K,
    pub placement: Placement,
    pub moving: bool,
    pub last: Option<Duration>,
}

/// Every block's row, soonest-demanded first.
#[derive(Debug, Clone, PartialEq)]
pub struct Placements<K>(pub Vec<Row<K>>);

/// The links a node has, one per pair of tiers. A pair with no link is one no block on this node moves across.
pub trait Links<K>: Send + 'static {
    fn between(&self, from: Placement, to: Placement) -> Option<&dyn Link<K>>;
}

/// What wakes the Streamer.
pub enum Stream<K> {
    /// The consumer's forecast changed.
    Demand(Snapshot<Demand<K>>),
    /// A block it is to place (a mind seated: her KV block, parked or resident).
    Hold(Block<K>),
    /// A move's fence fired: its device's completion tells this, never a poll on the engine thread.
    Landed(K),
}

impl<K> From<Snapshot<Demand<K>>> for Stream<K> {
    fn from(demand: Snapshot<Demand<K>>) -> Self {
        Self::Demand(demand)
    }
}

/// A consumer's forecast, state keyed [`Key::ONE`].
pub const DEMAND: TopicName = TopicName("blocks/demand");
/// Every block's placement, state keyed [`Key::ONE`].
pub const PLACEMENTS: TopicName = TopicName("blocks/placements");

/// The one move in flight: its block travels with it, out of the Streamer's map, so a newer block held for the same
/// key while it moves can never drop the source a device still reads (Codex, 2026-11-26). Fields drop in order: the
/// transfer first (an unlanded one waits for its device), then the block's source.
struct InFlight<K> {
    transfer: Transfer,
    key: K,
    block: Block<K>,
}

/// The streaming system beside its consumer.
pub struct Streamer<K, L> {
    /// First, so at shutdown a move in flight settles before anything else of the Streamer's goes.
    moving: Option<InFlight<K>>,
    links: L,
    /// Where a demanded block must be for its consumer to use it (the device's own tier on this node).
    use_at: Placement,
    demand: Arc<Demand<K>>,
    /// Every block at rest, by key: the newest held for its key.
    blocks: HashMap<K, Block<K>>,
    last: HashMap<K, Duration>,
    says: Publisher<Placements<K>>,
}

impl<K: Copy + Eq + Hash + Debug + Send + Sync + 'static, L: Links<K>> Streamer<K, L> {
    pub fn new(links: L, use_at: Placement, says: Publisher<Placements<K>>) -> Self {
        Self { moving: None, links, use_at, demand: Arc::new(Demand { soonest: Vec::new() }), blocks: HashMap::new(), last: HashMap::new(), says }
    }

    /// Hold `block`. One held for a key whose move is in flight waits beside it: the moving block stays leased to its
    /// transfer until its fence, and is superseded only then.
    fn hold(&mut self, block: Block<K>) {
        self.blocks.insert(*block.key(), block);
    }

    /// Land the move in flight if its fence fired. Its block returns to the map unless a newer one was held for its
    /// key meanwhile; then the landed one is let go, now that the device is done with both its regions.
    fn land(&mut self, key: K, now: Instant) {
        let Some(InFlight { transfer, key: moving, mut block }) = self.moving.take() else { return };
        if moving != key {
            self.moving = Some(InFlight { transfer, key: moving, block });
            return;
        }
        match transfer.land(&mut block, now) {
            Ok(moved) => {
                self.last.insert(key, moved.took);
                match self.blocks.entry(key) {
                    Entry::Occupied(_) => {
                        continuum_base::probe!(class = "blocks.stream.superseded", key = ?key, "a newer block was held for this key while its move was in flight: the landed one goes");
                    }
                    Entry::Vacant(rest) => {
                        rest.insert(block);
                    }
                }
            }
            Err(unlanded) => self.moving = Some(InFlight { transfer: unlanded, key, block }), // told early: still in flight
        }
    }

    /// Start the soonest demanded block that is not where its consumer uses it, if no move is in flight.
    fn feed(&mut self, now: Instant) {
        if self.moving.is_some() {
            return;
        }
        let use_at = self.use_at;
        let demand = Arc::clone(&self.demand);
        for &key in &demand.soonest {
            let Some(block) = self.blocks.get_mut(&key) else { continue };
            let from = block.placement();
            if from == use_at {
                continue;
            }
            let Some(link) = self.links.between(from, use_at) else {
                continuum_base::probe!(class = "blocks.stream.no_link", key = ?key, from = ?from, to = ?use_at, "no link between these tiers on this node: the block stays");
                continue;
            };
            match link.start(block, use_at) {
                Ok(transfer) => {
                    let name = link.name();
                    continuum_base::probe!(class = "blocks.stream.started", key = ?key, link = name, bytes = transfer.bytes, "a demanded block's move started");
                    let Some(block) = self.blocks.remove(&key) else { continue }; // looked up above: it is there
                    self.moving = Some(InFlight { transfer, key, block });
                    self.land(key, now); // a link that moved when it started (paging, a retag) lands at once
                    if self.moving.is_some() {
                        return;
                    }
                }
                Err(refusal) => {
                    continuum_base::probe!(class = "blocks.stream.refused", key = ?key, error = %refusal, "a demanded block's move was refused: it stays where it was");
                }
            }
        }
    }

    /// Every key's row: its newest block, moving if that block is the one in flight.
    fn placements(&self) -> Placements<K> {
        // an undemanded block (no rank) sorts after every demanded one
        let rank = |key: &K| self.demand.soonest.iter().position(|k| k == key);
        let last = |key: &K| self.last.get(key).copied();
        let mut rows: Vec<Row<K>> = self.blocks.iter().map(|(key, block)| Row { key: *key, placement: block.placement(), moving: false, last: last(key) }).collect();
        if let Some(InFlight { key, block, .. }) = &self.moving {
            if !self.blocks.contains_key(key) {
                rows.push(Row { key: *key, placement: block.placement(), moving: true, last: last(key) });
            }
        }
        rows.sort_by_key(|row| { let at = rank(&row.key); (at.is_none(), at) });
        Placements(rows)
    }
}

impl<K: Copy + Eq + Hash + Debug + Send + Sync + 'static, L: Links<K>> Component for Streamer<K, L> {
    type Input = Stream<K>;
    const NAME: &'static str = "blocks-streamer";
    const QOS: Qos = Qos::Interactive;

    fn wire(&self, wiring: &mut Wiring<'_, Self>) {
        wiring.watch::<Demand<K>>(DEMAND, Key::ONE);
    }

    async fn handle(&mut self, input: Self::Input, ctx: &mut Ctx<Self>) {
        let now = Instant::now();
        match input {
            Stream::Demand(demand) => self.demand = demand.value,
            Stream::Hold(block) => self.hold(block),
            Stream::Landed(key) => self.land(key, now),
        }
        self.feed(now);
        let placements = self.placements();
        ctx.publish(&mut self.says, placements);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::{Completion, LinkRefusal};
    use crate::{Region, TierKind};
    use continuum_sage::component::{start, Bus, Running};
    use continuum_sage::dataflow::{OwnerId, Provenance, Publisher, Reader, RunId, StageId, StageIncarnation};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    const VRAM: Placement = Placement::Resident(TierKind::Vram);
    const PINNED: Placement = Placement::Resident(TierKind::Pinned);

    /// The fence a test fires by hand, as a device's completion would.
    struct Fence(Arc<AtomicBool>);

    impl Completion for Fence {
        fn landed(&self) -> bool {
            self.0.load(Ordering::Acquire)
        }
        fn settle(&self) {
            self.0.store(true, Ordering::Release); // the test is the device: asked to finish, it does
        }
    }

    /// A DMA-shaped link: reserves the destination, starts, and lands on its fence.
    struct Dma {
        fences: Fences,
    }

    impl Link<char> for Dma {
        fn name(&self) -> &'static str {
            "dma"
        }
        fn start(&self, block: &mut Block<char>, to: Placement) -> Result<Transfer, LinkRefusal> {
            let into = Region::host(TierKind::Vram, block.region().len(), 64).expect("test: the destination"); // expect: the test's own allocation
            let fence = Arc::new(AtomicBool::new(false));
            self.fences.lock().expect("test: the record").push((*block.key(), Arc::clone(&fence))); // expect: the test's own lock
            Ok(Transfer::into_region(Instant::now(), to, Box::new(Fence(fence)), into))
        }
    }

    struct Card(Dma);

    impl Links<char> for Card {
        fn between(&self, from: Placement, to: Placement) -> Option<&dyn Link<char>> {
            (from == PINNED && to == VRAM).then_some(&self.0 as &dyn Link<char>)
        }
    }

    type Fences = Arc<Mutex<Vec<(char, Arc<AtomicBool>)>>>;

    /// A Streamer on a card with one DMA link, its placements, the consumer's demand, and every fence it started.
    struct Rig {
        streamer: Running<Streamer<char, Card>>,
        placements: Reader<Placements<char>>,
        demand: Publisher<Demand<char>>,
        fences: Fences,
    }

    fn card(bus: &'static Bus) -> Rig {
        let fences: Fences = Arc::new(Mutex::new(Vec::new()));
        let says = bus.publisher(PLACEMENTS, Key::ONE).expect("test: the topic is free"); // expect: the test's own precondition
        let streamer = start(Streamer::new(Card(Dma { fences: Arc::clone(&fences) }), VRAM, says), bus, OwnerId(uuid::Uuid::from_u128(9)), 1);
        let placements = bus.reader::<Placements<char>>(PLACEMENTS, Key::ONE);
        let demand = bus.publisher::<Demand<char>>(DEMAND, Key::ONE).expect("test: the topic is free"); // expect: the test's own precondition
        Rig { streamer, placements, demand, fences }
    }

    fn fence(fences: &Fences, i: usize) -> (char, Arc<AtomicBool>) {
        let all = fences.lock().expect("test: the record"); // expect: the test's own lock
        (all[i].0, Arc::clone(&all[i].1))
    }

    fn by() -> Provenance {
        Provenance { by: RunId { incarnation: StageIncarnation { stage: StageId::named("lane"), owner: OwnerId(uuid::Uuid::from_u128(9)), epoch: 1 }, seq: 1 }, from: Vec::new() }
    }

    fn parked(key: char, byte: u8) -> Block<char> {
        let mut region = Region::host(TierKind::Pinned, 4096, 64).expect("test: a parked block"); // expect: the test's own allocation
        region.as_mut_slice().fill(byte);
        let mut block = Block::new(key, region);
        block.set_placement(PINNED);
        block
    }

    async fn until(placements: &mut Reader<Placements<char>>, want: &[(char, Placement, bool)]) {
        loop {
            if placements.latest().is_some_and(|now| now.value.0.iter().map(|r| (r.key, r.placement, r.moving)).eq(want.iter().copied())) {
                return;
            }
            assert!(placements.changed().await.is_ok(), "the Streamer keeps saying placements");
        }
    }

    // what this catches (Joel: "All cache transfers done correctly", one object on its own worker): a consumer
    // moving its own bytes, two moves on one link, a block marked resident before its fence, a source let go before
    // its copy lands, or a landing that does not start the next. The Streamer moves the soonest demanded block first
    // through its pair's link, one at a time, and publishes each block's placement and measured move.
    #[tokio::test]
    async fn the_streamer_moves_demanded_blocks_soonest_first_one_dma_at_a_time() {
        let bus: &'static Bus = Box::leak(Box::new(Bus::new()));
        let Rig { streamer, mut placements, mut demand, fences } = card(bus);
        assert!(streamer.tell(Stream::Hold(parked('b', 0xb))).await);
        assert!(streamer.tell(Stream::Hold(parked('c', 0xc))).await);
        demand.publish(Demand { soonest: vec!['c', 'b'] }, by());
        until(&mut placements, &[('c', PINNED, true), ('b', PINNED, false)]).await;
        assert_eq!(fences.lock().expect("test: the record").len(), 1, "one move per link, the soonest first"); // expect: the test's own lock
        assert!(streamer.tell(Stream::Landed('c')).await);
        until(&mut placements, &[('c', PINNED, true), ('b', PINNED, false)]).await; // told before its fence fired: still moving, still at its source
        let (first, landed) = fence(&fences, 0);
        assert_eq!(first, 'c');
        landed.store(true, Ordering::Release);
        assert!(streamer.tell(Stream::Landed('c')).await);
        until(&mut placements, &[('c', VRAM, false), ('b', PINNED, true)]).await;
        let row = placements.latest().and_then(|now| now.value.0.first().copied());
        assert!(row.and_then(|r| r.last).is_some(), "her measured move is said with her, for the lane's slice floor");
    }

    // what this catches (Codex, 2026-11-26, review of 15fcba918c): a block held again for a key whose DMA is in
    // flight dropping the source the device still reads, and the old move's destination installed into the newer
    // block when its fence fires. The moving block stays leased to its transfer; the newer one keeps its own bytes
    // and gets its own move once the old one settles.
    #[tokio::test]
    async fn a_block_held_again_mid_move_never_takes_the_old_moves_landing() {
        let bus: &'static Bus = Box::leak(Box::new(Bus::new()));
        let Rig { streamer, mut placements, mut demand, fences } = card(bus);
        assert!(streamer.tell(Stream::Hold(parked('c', 0xa))).await);
        demand.publish(Demand { soonest: vec!['c'] }, by());
        until(&mut placements, &[('c', PINNED, true)]).await;
        assert!(streamer.tell(Stream::Hold(parked('c', 0xb))).await);
        until(&mut placements, &[('c', PINNED, false)]).await; // her row is the newest block, at rest where it was held
        let (_, old) = fence(&fences, 0);
        old.store(true, Ordering::Release);
        assert!(streamer.tell(Stream::Landed('c')).await);
        until(&mut placements, &[('c', PINNED, true)]).await; // the newer block starts its own move: it never took the old landing
        assert_eq!(fences.lock().expect("test: the record").len(), 2, "the newer block moves by its own DMA"); // expect: the test's own lock
        fence(&fences, 1).1.store(true, Ordering::Release);
        assert!(streamer.tell(Stream::Landed('c')).await);
        until(&mut placements, &[('c', VRAM, false)]).await;
    }
}
