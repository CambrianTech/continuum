use super::*;
use crate::link::{Completion, LinkRefusal};
use crate::{Region, TierKind};
use continuum_sage::component::{start, Bus, Key, Running};
use continuum_sage::dataflow::{OwnerId, Provenance, Publisher, Reader, RunId, StageId, StageIncarnation};
use continuum_sage::decide::Measured;
use continuum_sage::readiness::{Horizon, Readiness, ReadinessIn, Ready, Table, Upcoming, HORIZON};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

type Fences = Arc<Mutex<Vec<(char, Arc<AtomicBool>)>>>;

/// A DMA-shaped link: reserves the destination, starts, and lands on its fence.
struct Dma {
    /// Each move it starts, said as it starts: what a test awaits, never polls for.
    starts: tokio::sync::mpsc::UnboundedSender<char>,
    fences: Fences,
}

impl Link<char> for Dma {
    fn name(&self) -> &'static str {
        "dma"
    }
    fn start(&self, block: &mut Block<char>, to: Placement) -> Result<Transfer, LinkRefusal> {
        let Ok(into) = Region::host(TierKind::Vram, block.region().len(), 64) else { panic!("test: the destination") };
        let fence = Arc::new(AtomicBool::new(false));
        let Ok(mut fences) = self.fences.lock() else { panic!("test: the record") };
        fences.push((*block.key(), Arc::clone(&fence)));
        assert!(self.starts.send(*block.key()).is_ok(), "the test hears every move");
        Ok(Transfer::into_region(Instant::now(), to, Box::new(Fence(fence)), into))
    }
}

struct Card(Dma);

impl Links<char> for Card {
    fn between(&self, from: Placement, to: Placement) -> Option<&dyn Link<char>> {
        (from == PINNED && to == VRAM).then_some(&self.0 as &dyn Link<char>)
    }
}

type Paged = Table<char, PageRefusal>;

/// A pager on a card with one DMA link, its table, the lane's horizon, and every fence it started.
struct Rig {
    pager: Running<Readiness<Pager<char, Card>>>,
    table: Reader<Paged>,
    lane: Publisher<Horizon<char>>,
    starts: tokio::sync::mpsc::UnboundedReceiver<char>,
    fences: Fences,
}

fn card(bus: &'static Bus) -> Rig {
    let fences: Fences = Arc::new(Mutex::new(Vec::new()));
    let (said, starts) = tokio::sync::mpsc::unbounded_channel();
    let Ok(part) = Readiness::new(Pager::new(Card(Dma { starts: said, fences: Arc::clone(&fences) }), VRAM), bus) else { panic!("test: the topic is free") };
    let pager = start(part, bus, OwnerId(uuid::Uuid::from_u128(9)), 1);
    let table = bus.reader::<Paged>(PAGED, Key::ONE);
    let Ok(lane) = bus.publisher::<Horizon<char>>(HORIZON, Key::ONE) else { panic!("test: the horizon is free") };
    Rig { pager, table, lane, starts, fences }
}

fn started(fences: &Fences) -> Vec<char> {
    let Ok(all) = fences.lock() else { panic!("test: the record") };
    all.iter().map(|(key, _)| *key).collect()
}

fn fire(fences: &Fences, i: usize) {
    let Ok(all) = fences.lock() else { panic!("test: the record") };
    all[i].1.store(true, Ordering::Release);
}

fn by() -> Provenance {
    Provenance { by: RunId { incarnation: StageIncarnation { stage: StageId::named("lane"), owner: OwnerId(uuid::Uuid::from_u128(9)), epoch: 1 }, seq: 1 }, from: Vec::new() }
}

fn horizon(minds: &[(char, u64)]) -> Horizon<char> {
    Horizon { minds: minds.iter().map(|&(mind, slack_ms)| Upcoming { mind, slack: Duration::from_millis(slack_ms) }).collect() }
}

fn parked(key: char, byte: u8) -> Block<char> {
    let Ok(mut region) = Region::host(TierKind::Pinned, 4096, 64) else { panic!("test: a parked block") };
    region.as_mut_slice().fill(byte);
    let mut block = Block::new(key, region);
    block.set_placement(PINNED);
    block
}

fn ready(table: &Paged, mind: char) -> bool {
    table.of(&mind) == Some(&Ready::Ready)
}

fn preparing(table: &Paged, mind: char) -> bool {
    matches!(table.of(&mind), Some(Ready::Preparing { .. }))
}

async fn until(table: &mut Reader<Paged>, done: impl Fn(&Paged) -> bool) -> Paged {
    loop {
        if let Some(now) = table.latest().filter(|now| done(&now.value)) {
            return (*now.value).clone();
        }
        assert!(table.changed().await.is_ok(), "the pager keeps saying its table");
    }
}

// what this catches (Joel: "All cache transfers done correctly", one object on its own worker): a consumer moving
// its own bytes, two moves on one link, a block called ready before its fence, a source let go before its copy lands,
// or a landing that does not start the next. The pager moves the most pressed mind's block first through its pair's
// link, one at a time, and once one move is measured says when the next mind will be ready.
#[tokio::test]
async fn the_pager_moves_the_most_pressed_minds_block_first_one_dma_at_a_time() {
    let bus: &'static Bus = Box::leak(Box::new(Bus::new()));
    let Rig { pager, mut table, mut lane, mut starts, fences } = card(bus);
    assert!(pager.tell(ReadinessIn::Act(PagerAct::Hold(parked('b', 0xb)))).await);
    assert!(pager.tell(ReadinessIn::Act(PagerAct::Hold(parked('c', 0xc)))).await);
    lane.publish(horizon(&[('b', 900), ('c', 100)]), by());
    until(&mut table, |t| preparing(t, 'c') && preparing(t, 'b')).await;
    assert_eq!(starts.recv().await, Some('c'), "the most pressed mind's move starts first");
    assert_eq!(started(&fences), ['c'], "one move per link, the most pressed first");
    assert!(pager.tell(ReadinessIn::Act(PagerAct::Landed('c'))).await);
    let early = until(&mut table, |t| preparing(t, 'c')).await; // told before its fence fired: still moving
    assert!(preparing(&early, 'c'));
    assert_eq!(started(&fences), ['c'], "nothing starts behind a move its device has not finished");
    fire(&fences, 0);
    assert!(pager.tell(ReadinessIn::Act(PagerAct::Landed('c'))).await);
    let after = until(&mut table, |t| ready(t, 'c') && matches!(t.of(&'b'), Some(Ready::Preparing { eta: Measured::Value(_) }))).await;
    assert!(ready(&after, 'c'));
    assert_eq!(starts.recv().await, Some('b'), "her landing starts the next mind's move");
    assert_eq!(started(&fences), ['c', 'b'], "her landing starts the next mind's move");
}

// what this catches (Codex, 2026-11-26, review of 15fcba918c): a block held again for a key whose DMA is in flight
// dropping the source the device still reads, and the old move's destination installed into the newer block when its
// fence fires. The moving block stays leased to its transfer; the newer one keeps its own bytes and gets its own move
// once the old one settles.
#[tokio::test]
async fn a_block_held_again_mid_move_never_takes_the_old_moves_landing() {
    let bus: &'static Bus = Box::leak(Box::new(Bus::new()));
    let Rig { pager, mut table, mut lane, mut starts, fences } = card(bus);
    assert!(pager.tell(ReadinessIn::Act(PagerAct::Hold(parked('c', 0xa)))).await);
    lane.publish(horizon(&[('c', 100)]), by());
    assert_eq!(starts.recv().await, Some('c'), "the most pressed mind's move starts first");
    assert!(pager.tell(ReadinessIn::Act(PagerAct::Hold(parked('c', 0xb)))).await);
    fire(&fences, 0);
    assert!(pager.tell(ReadinessIn::Act(PagerAct::Landed('c'))).await);
    assert_eq!(starts.recv().await, Some('c'), "the newer block starts its own move");
    assert_eq!(started(&fences), ['c', 'c'], "the newer block moves by its own DMA: it never took the old landing");
    fire(&fences, 1);
    assert!(pager.tell(ReadinessIn::Act(PagerAct::Landed('c'))).await);
    until(&mut table, |t| ready(t, 'c')).await;
}
