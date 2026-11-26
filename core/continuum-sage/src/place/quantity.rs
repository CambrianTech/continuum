//! What a place holds, typed: bytes on a tier, slots of something kept (minds awake), tokens of a
//! window. Each is its own type, so one is never spent as another (UNIFIED-PAGING.md, slice 3a). A
//! quantity is whole units, so a claim's remainder can be consumed from any thread by one atomic.

use std::fmt::Debug;

/// A countable amount a place holds and a claim reserves.
pub trait Quantity: Copy + Ord + Debug + Send + Sync + 'static {
    /// Its count of whole units.
    fn units(self) -> u64;
    /// The amount of `units` whole units.
    fn of(units: u64) -> Self;
}

/// Bytes, on a tier of memory or a volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Bytes(pub u64);

/// Slots of something held whole: a mind awake, a model hot, a participant rendered full.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Slots(pub u64);

impl Quantity for Bytes {
    fn units(self) -> u64 {
        self.0
    }
    fn of(units: u64) -> Self {
        Self(units)
    }
}

impl Quantity for Slots {
    fn units(self) -> u64 {
        self.0
    }
    fn of(units: u64) -> Self {
        Self(units)
    }
}
