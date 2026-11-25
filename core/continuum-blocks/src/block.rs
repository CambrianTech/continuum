//! [`Block`]: what a consumer holds. Its key is the consumer's own (a mind, a model shard, an expert),
//! so this crate depends on no consumer. A block knows its region and where its bytes are now, and a
//! [`Pin`] keeps it where it is while device work reads it.

use std::sync::atomic::{AtomicU32, Ordering};

use crate::{Region, TierKind};

/// Where a block's bytes are right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// In memory on this tier: a device may read them where they lie.
    Resident(TierKind),
    /// Written back to the drive; their next touch reads them in (a mapped block demoted to `Flash`).
    Paged,
}

/// A consumer's block: its key, its region, and how many pins hold it in place.
pub struct Block<K> {
    key: K,
    region: Region,
    placement: Placement,
    pins: AtomicU32,
}

impl<K> Block<K> {
    /// A block for `key` in `region`, resident on the region's tier.
    pub fn new(key: K, region: Region) -> Self {
        let placement = Placement::Resident(region.tier());
        Self { key, region, placement, pins: AtomicU32::new(0) }
    }

    pub fn key(&self) -> &K {
        &self.key
    }

    pub fn region(&self) -> &Region {
        &self.region
    }

    pub fn region_mut(&mut self) -> &mut Region {
        &mut self.region
    }

    pub fn placement(&self) -> Placement {
        self.placement
    }

    pub(crate) fn set_placement(&mut self, placement: Placement) {
        self.placement = placement;
    }

    /// Trade regions (and so placements) with `other`: each holds the other's memory.
    pub(crate) fn swap_region(&mut self, other: &mut Self) {
        std::mem::swap(&mut self.region, &mut other.region);
        std::mem::swap(&mut self.placement, &mut other.placement);
    }

    /// Device work that outlives a borrow (an `InFlight` read) holds the count directly.
    #[cfg(test)]
    pub(crate) fn pins_for_test(&self, n: u32) {
        self.pins.fetch_add(n, Ordering::AcqRel);
    }

    /// Hold the block in place: while the pin lives, no link moves it.
    pub fn pin(&self) -> Pin<'_, K> {
        self.pins.fetch_add(1, Ordering::AcqRel);
        Pin { block: self }
    }

    pub fn pinned(&self) -> bool {
        self.pins.load(Ordering::Acquire) > 0
    }
}

/// While held, its block does not move.
pub struct Pin<'b, K> {
    block: &'b Block<K>,
}

impl<K> Drop for Pin<'_, K> {
    fn drop(&mut self) {
        self.block.pins.fetch_sub(1, Ordering::AcqRel);
    }
}
