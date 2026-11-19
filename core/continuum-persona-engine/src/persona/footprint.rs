//! What a node holds: the sum of its children (MODULE-ARCHITECTURE 9.3-9.4; Joel, 2026-11-18: "The
//! sum of all parts. Nodes and subnodes infinitely deep and simple to understand"). A node measures
//! only its own direct state and asks each child for the rest; a parent never re-measures a child,
//! and no caller adds a mind's bytes up by hand.

use std::ops::Add;

/// Bytes held, by where they live.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Held {
    /// Host memory (RAM): token histories, recurrent checkpoints, parked blocks.
    pub host: u64,
    /// Device memory: what the context holds for her.
    pub device: u64,
}

impl Held {
    pub const NOTHING: Held = Held { host: 0, device: 0 };

    pub fn host(bytes: u64) -> Self {
        Held { host: bytes, device: 0 }
    }

    pub fn device(bytes: u64) -> Self {
        Held { host: 0, device: bytes }
    }

    /// Host and device together, for a node whose memory is one pool (unified memory).
    pub fn total(self) -> u64 {
        self.host.saturating_add(self.device)
    }
}

impl Add for Held {
    type Output = Held;
    fn add(self, other: Held) -> Held {
        Held { host: self.host.saturating_add(other.host), device: self.device.saturating_add(other.device) }
    }
}

impl std::iter::Sum for Held {
    fn sum<I: Iterator<Item = Held>>(parts: I) -> Held {
        parts.fold(Held::NOTHING, Add::add)
    }
}

/// A node's bytes: its own state plus its children's.
pub trait Footprint {
    fn held(&self) -> Held;
}

/// An absent child holds nothing.
impl<T: Footprint> Footprint for Option<T> {
    fn held(&self) -> Held {
        self.as_ref().map_or(Held::NOTHING, Footprint::held) // map_or: an absent child holds nothing
    }
}

/// A token history: its ids, in host memory.
impl Footprint for Vec<i32> {
    fn held(&self) -> Held {
        Held::host((self.capacity() * std::mem::size_of::<i32>()) as u64)
    }
}
