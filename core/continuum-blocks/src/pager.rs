//! The pager: the readiness part that brings each mind's blocks to the tier her consumer uses them on (UNIFIED-PAGING.md,
//! "Streamer"; THE-LANE-ON-SAGE.md, "Readiness"). Joel, 2026-11-26: "in a sage component too, so it's in its own
//! thread"; "this transfer logic needs its own object done right so you reuse and do it correctly. All cache
//! transfers done correctly."
//!
//! A consumer never moves its own bytes. It holds its blocks here ([`PagerAct::Hold`]); the lane's horizon says who
//! steps soonest; the pager, a leaf of [`Readiness`](continuum_sage::readiness::Readiness), moves each mind's block
//! through the one [`Link`] its pair of tiers has: a pointer, the OS's paging, or one DMA of one block with a fence.
//! Never a copy loop, a staging copy or an encoding: those are not links. One move at a time, the most pressed mind
//! first; a block's source stays where it is, valid, until its move lands. Her readiness, and when she will be ready
//! at the measured rate of the bytes it moves, is the parent's table.

use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::time::Instant;

use continuum_sage::dataflow::TopicName;
use continuum_sage::readiness::{Advanced, Needs, Readies};
use continuum_sage::Qos;

use crate::link::{Link, LinkRefusal, Transfer};
use crate::{Block, Placement};

/// Where the pager's readiness table is published, state keyed `Key::ONE`.
pub const PAGED: TopicName = TopicName("blocks/readiness");

/// The links a node has, one per pair of tiers. A pair with no link is one no block on this node moves across.
pub trait Links<K>: Send + 'static {
    fn between(&self, from: Placement, to: Placement) -> Option<&dyn Link<K>>;
}

/// What only the pager is handed.
pub enum PagerAct<K> {
    /// A block to hold (a mind seated: her KV block, parked or resident). One held for a key whose move is in flight
    /// waits beside it: the moving block stays leased to its transfer until its fence.
    Hold(Block<K>),
    /// A move's fence fired: its device's completion says so, never a poll on the engine thread.
    Landed(K),
}

/// Why the pager cannot bring a mind's block to its consumer's tier. The block stays where it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageRefusal {
    /// No link between these tiers on this node.
    NoLink { from: Placement, to: Placement },
    /// Device work still reads it where it is.
    Pinned,
    /// It is not backed by a file mapping, so the paging link cannot move it.
    NotMapped,
    /// The link says it is already on that tier: the block it was asked about is not the one this pager holds.
    AlreadyThere(Placement),
    /// The OS refused the move.
    Io(std::io::ErrorKind),
}

impl From<LinkRefusal> for PageRefusal {
    fn from(refusal: LinkRefusal) -> Self {
        match refusal {
            LinkRefusal::Pinned => Self::Pinned,
            LinkRefusal::NotMapped => Self::NotMapped,

            LinkRefusal::AlreadyThere(at) => Self::AlreadyThere(at),
            LinkRefusal::Io(error) => Self::Io(error.kind()),
        }
    }
}

/// One mind's move: her block's bytes, what its cost is measured by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Move {
    pub bytes: u64,
}

struct InFlight<K> {
    transfer: Transfer,
    key: K,
    block: Block<K>,
}

/// The pager's own state: its links, the tier its consumer uses blocks on, every block at rest and the one moving.
pub struct Pager<K, L> {
    /// First, so at shutdown a move in flight settles before anything else of the pager's goes.
    moving: Option<InFlight<K>>,
    links: L,
    /// Where a mind's block must be for her consumer to use it (the device's own tier on this node).
    use_at: Placement,
    /// Every block at rest, by key: the newest held for its key.
    blocks: HashMap<K, Block<K>>,
}

impl<K: Copy + Eq + Hash + Debug + Send + Sync + 'static, L: Links<K>> Pager<K, L> {
    pub fn new(links: L, use_at: Placement) -> Self {
        Self { moving: None, links, use_at, blocks: HashMap::new() }
    }

    /// Land the move in flight if its fence fired. Its block returns to the map unless a newer one was held for its
    /// key meanwhile; then the landed one is let go, now that the device is done with both its regions.
    fn land(&mut self, key: K, now: Instant) -> Option<(K, Advanced<Move, PageRefusal>)> {
        let InFlight { transfer, key: moving, mut block } = self.moving.take()?;
        if moving != key {
            self.moving = Some(InFlight { transfer, key: moving, block });
            return None;
        }
        match transfer.land(&mut block, now) {
            Ok(_) => {
                if self.blocks.contains_key(&key) {
                    continuum_base::probe!(class = "blocks.page.superseded", key = ?key, "a newer block was held for this key while its move was in flight: the landed one goes");
                } else {
                    self.blocks.insert(key, block);
                }
                Some((key, Advanced::Done)) // a newer block, still where it was held, is asked for next
            }
            Err(unlanded) => {
                self.moving = Some(InFlight { transfer: unlanded, key, block }); // told early: still in flight
                None
            }
        }
    }
}

impl<K: Copy + Eq + Hash + Debug + Send + Sync + 'static, L: Links<K>> Readies for Pager<K, L> {
    const NAME: &'static str = "blocks-pager";
    const QOS: Qos = Qos::Interactive;
    const TOPIC: TopicName = PAGED;
    type Mind = K;
    type Work = Move;
    type Refusal = PageRefusal;
    type Act = PagerAct<K>;

    /// Her block at rest and not where her consumer uses it: its bytes are the move.
    fn needs(&mut self, mind: K) -> Needs<Move> {
        match self.blocks.get(&mind) {
            Some(block) if block.placement() != self.use_at => Needs::Work(Move { bytes: block.region().len() as u64 }),
            _ => Needs::Nothing,
        }
    }

    fn left(work: &Move) -> u64 {
        work.bytes
    }

    /// Start her block's move through its pair's link. A link that moves when it starts (paging, a retag) lands at
    /// once; a DMA is in flight until its fence.
    async fn advance(&mut self, mind: K, work: Move) -> Advanced<Move, PageRefusal> {
        let use_at = self.use_at;
        let Some(block) = self.blocks.get_mut(&mind) else { return Advanced::Done }; // let go meanwhile: nothing to move
        let from = block.placement();
        let Some(link) = self.links.between(from, use_at) else { return Advanced::Refused(PageRefusal::NoLink { from, to: use_at }) };
        match link.start(block, use_at) {
            Ok(transfer) => {
                continuum_base::probe!(class = "blocks.page.started", key = ?mind, link = link.name(), bytes = transfer.bytes, "a mind's block began its move to her consumer's tier");
                let Some(block) = self.blocks.remove(&mind) else { return Advanced::More(work) }; // looked up above: it is there
                self.moving = Some(InFlight { transfer, key: mind, block });
                match self.land(mind, Instant::now()) {
                    Some((_, landed)) => landed,
                    None => Advanced::Started,
                }
            }
            Err(refusal) => Advanced::Refused(refusal.into()),
        }
    }

    fn act(&mut self, act: PagerAct<K>) -> Option<(K, Advanced<Move, PageRefusal>)> {
        match act {
            PagerAct::Hold(block) => {
                self.blocks.insert(*block.key(), block);
                None
            }
            PagerAct::Landed(key) => self.land(key, Instant::now()),
        }
    }

    /// The pager keeps custody of her block: she is only no longer put first.
    fn release(&mut self, _mind: K) {}
}

#[cfg(test)]
mod tests;
