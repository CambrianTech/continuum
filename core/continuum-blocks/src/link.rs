//! How a block's bytes move between tiers (UNIFIED-PAGING.md, "Link"). Every move is one of the things
//! the hardware already does: a pointer, or the OS's paging of a mapped file. Never a copy loop.
//!
//! - [`Link`]: carries one block to a placement. [`Mapping`] is the link between a host tier and
//!   `Flash` for a block backed by a file mapping: demote writes its dirty pages back and lets the OS
//!   drop them; promote asks the OS to read it ahead. Its region never changes.
//! - [`Exchange`]: two blocks swap regions (the Core's switch, #6264): the stream's block and a parked
//!   mind's trade memory by pointer, and no byte moves.

use std::time::{Duration, Instant};

use crate::{Backing, Block, Placement, TierKind};

/// Why a link did not move a block. The block is left exactly where it was.
#[derive(Debug, thiserror::Error)]
pub enum LinkRefusal {
    #[error("the block is pinned: device work still reads it where it is")]
    Pinned,
    #[error("the block is not backed by a file mapping, so this link cannot page it")]
    NotMapped,
    #[error("the block is already {0:?}")]
    AlreadyThere(Placement),
    #[error("the OS refused the move: {0}")]
    Io(#[from] std::io::Error),
}

/// What a move did, for the Streamer's measured link rates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Moved {
    pub bytes: usize,
    pub took: Duration,
}

/// Carries one block to a placement.
pub trait Link<K> {
    fn name(&self) -> &'static str;
    fn carry(&self, block: &mut Block<K>, to: Placement) -> Result<Moved, LinkRefusal>;
}

/// Between a host tier (`Unified`, `Ram`) and `Flash`, for a mapped block: the OS's own paging.
pub struct Mapping;

impl<K> Link<K> for Mapping {
    fn name(&self) -> &'static str {
        "mapping"
    }

    fn carry(&self, block: &mut Block<K>, to: Placement) -> Result<Moved, LinkRefusal> {
        if block.pinned() {
            return Err(LinkRefusal::Pinned);
        }
        if block.placement() == to {
            return Err(LinkRefusal::AlreadyThere(to));
        }
        let Backing::Mapped(map) = block.region().backing() else { return Err(LinkRefusal::NotMapped) };
        let started = Instant::now();
        match to {
            Placement::Paged => map.demote()?,
            Placement::Resident(TierKind::Unified | TierKind::Ram) => map.promote()?,
            Placement::Resident(TierKind::Flash) => return Err(LinkRefusal::AlreadyThere(to)),
        }
        let bytes = map.len();
        if let Placement::Resident(tier) = to {
            block.region_mut().set_tier(tier);
        }
        block.set_placement(to);
        Ok(Moved { bytes, took: started.elapsed() })
    }
}

/// Two blocks trade regions: each now holds the other's memory, and no byte is copied.
pub struct Exchange;

impl Exchange {
    pub fn swap<K>(&self, a: &mut Block<K>, b: &mut Block<K>) -> Result<(), LinkRefusal> {
        if a.pinned() || b.pinned() {
            return Err(LinkRefusal::Pinned);
        }
        a.swap_region(b);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Region;

    fn pattern(i: usize) -> u8 {
        (i % 251) as u8
    }

    // what this catches (UNIFIED-PAGING.md, "Every Link" validator): a mapped block that comes back
    // from flash with any byte changed, a link that copies or re-maps (its address moves), and a
    // demote of a pinned block. Prints the measured rate for the Streamer's link table.
    #[test]
    fn a_mapped_block_pages_to_flash_and_back_byte_identical_at_the_same_address() {
        let dir = tempfile::tempdir().expect("test: a temp dir");
        let bytes = 64 * 1024 * 1024;
        let mut region = Region::mapped(TierKind::Unified, &dir.path().join("mind.kv"), bytes).expect("test: a mapped region");
        for (i, b) in region.as_mut_slice().iter_mut().enumerate() {
            *b = pattern(i);
        }
        let mut block = Block::new("mind a", region);
        let at = block.region().as_ptr();
        let mut host = Block::new("host", Region::host(TierKind::Unified, 8, 8).expect("test: host"));
        assert!(matches!(Mapping.carry(&mut host, Placement::Paged), Err(LinkRefusal::NotMapped)), "host memory has no file to page to");
        let down = Mapping.carry(&mut block, Placement::Paged).expect("test: demote");
        assert_eq!(block.placement(), Placement::Paged);
        let up = Mapping.carry(&mut block, Placement::Resident(TierKind::Unified)).expect("test: promote");
        assert_eq!(block.region().as_ptr(), at, "paging never moves her address");
        assert!(block.region().as_slice().iter().enumerate().all(|(i, &b)| b == pattern(i)), "byte-identical after flash");
        let gbps = |m: Moved| m.bytes as f64 / m.took.as_secs_f64().max(1e-9) / 1e9;
        eprintln!("mapping: 64 MiB demote {:.2} ms ({:.2} GB/s), promote {:.2} ms", down.took.as_secs_f64() * 1e3, gbps(down), up.took.as_secs_f64() * 1e3);
    }

    // what this catches: a pinned block moved while device work reads it.
    #[test]
    fn a_pinned_block_never_moves() {
        let dir = tempfile::tempdir().expect("test: a temp dir");
        let region = Region::mapped(TierKind::Unified, &dir.path().join("m.kv"), 4096).expect("test: a region");
        let mut block = Block::new(1u32, region);
        let pin = block.pin();
        // the pin is a shared borrow, so a mutable move cannot even be written while it lives; the
        // count covers device work that outlives a borrow (InFlight)
        drop(pin);
        block.pins_for_test(1);
        assert!(matches!(Mapping.carry(&mut block, Placement::Paged), Err(LinkRefusal::Pinned)));
        assert_eq!(block.placement(), Placement::Resident(TierKind::Unified), "left where it was");
    }

    // what this catches (the Core's switch, #6264): an exchange that copies instead of trading memory,
    // or that moves a pinned block.
    #[test]
    fn an_exchange_trades_memory_by_pointer() {
        let mut stream = Block::new("stream", Region::host(TierKind::Unified, 1 << 20, 64).expect("test: host"));
        let mut parked = Block::new("parked", Region::host(TierKind::Unified, 1 << 20, 64).expect("test: host"));
        stream.region_mut().as_mut_slice()[0] = 7;
        let (s, p) = (stream.region().as_ptr(), parked.region().as_ptr());
        Exchange.swap(&mut stream, &mut parked).expect("test: exchange");
        assert_eq!((stream.region().as_ptr(), parked.region().as_ptr()), (p, s), "each holds the other's memory");
        assert_eq!(parked.region().as_slice()[0], 7, "her bytes went with her memory, not copied");
        parked.pins_for_test(1);
        assert!(matches!(Exchange.swap(&mut stream, &mut parked), Err(LinkRefusal::Pinned)));
    }
}
