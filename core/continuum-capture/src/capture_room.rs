//! The room a call reserves in the node capture writer's window before any of its records exist
//! (card 699578cb; the 5090 and the IntelMac, 2026-11-23: on a node with no frames yet the window
//! held one record, so a call's Submitted and Terminal arriving within one append lost the older).
//!
//! A call reserves a slot for each record it will deliver before it delivers the first; the window
//! grows by those slots under one lock, synchronously, so no reserved record can be displaced. A
//! slot is spent by delivering its record and freed by the writer once that record is written; a
//! slot dropped unspent (a call that ended without that record) frees itself. The window is always
//! the writer's measured base plus every slot not yet freed, never a constant.

use std::num::NonZeroUsize;
use std::sync::{Arc, OnceLock};

use continuum_sage::dataflow::Resize;

/// The node writer's window and what is reserved in it.
struct Room {
    window: Arc<dyn Resize>,
    /// The records the writer's own measurement says one append can see arrive.
    base: NonZeroUsize,
    /// Slots reserved and not yet freed.
    reserved: usize,
}

impl Room {
    fn fit(&self) {
        self.window.resize(self.base.saturating_add(self.reserved));
    }
}

static ROOM: OnceLock<parking_lot::Mutex<Room>> = OnceLock::new();

/// Hand the room the node writer's window, sized at `base`; once, by the writer's start.
pub(crate) fn install(window: Arc<dyn Resize>, base: NonZeroUsize) -> bool {
    ROOM.set(parking_lot::Mutex::new(Room { window, base, reserved: 0 })).is_ok()
}

/// Why a call got no room.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Unreserved {
    #[error("this process runs no node capture writer: nothing would write the call's records")]
    NoWriter,
}

/// One record's place in the node writer's window.
#[must_use = "a slot is spent by delivering its record; dropped, it frees itself"]
pub struct Slot {
    held: Held,
}

/// What a slot holds now.
enum Held {
    /// A place in the node writer's window, its record not yet delivered.
    Node,
    /// Its record went to the writer, which frees the place once written; or it never held one
    /// (an inline delivery appends in its caller).
    Nothing,
}

impl Slot {
    /// A slot for a record appended inline by its caller: nothing is queued, so nothing is held.
    pub(crate) fn inline() -> Self {
        Self { held: Held::Nothing }
    }

    /// The record went to the writer, which frees this place once it has written it.
    pub(crate) fn spend(mut self) {
        self.held = Held::Nothing;
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        if let Held::Node = self.held {
            free(1);
        }
    }
}

/// Reserve a place for each of `records` records, growing the window before any arrives.
pub fn reserve<const N: usize>() -> Result<[Slot; N], Unreserved> {
    let room = ROOM.get().ok_or(Unreserved::NoWriter)?;
    let mut room = room.lock();
    room.reserved += N;
    room.fit();
    Ok(std::array::from_fn(|_| Slot { held: Held::Node }))
}

/// Free `records` places: the writer wrote them, or they were never spent.
pub(crate) fn free(records: usize) {
    let Some(room) = ROOM.get() else { return };
    let mut room = room.lock();
    room.reserved = room.reserved.saturating_sub(records);
    room.fit();
}

/// Slots reserved and not yet freed (what a test reads to see the window settle); `None` in a
/// process with no node writer.
#[cfg(test)]
pub(crate) fn reserved() -> Option<usize> {
    ROOM.get().map(|room| room.lock().reserved)
}
