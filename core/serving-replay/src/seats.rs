//! Which mind sits in which seat, for an engine that exposes raw seats (llama-server's
//! slots). This is today's core policy (continuum-engine `inference/slots.rs`
//! `KvSlotPool`): seat a mind in a free seat, else displace the least recently acting one.
//! S3/S5 move this policy into the engine, where temperature replaces recency.

use crate::target::MindId;

/// What seating a mind requires of the engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seating {
    /// She already holds `seat`.
    Resident { seat: u32 },
    /// `seat` is free; load her page (if she has one) into it.
    Free { seat: u32 },
    /// `seat` is held by `displaced`: page that mind out, then load her into it.
    Displace { seat: u32, displaced: MindId },
}

#[derive(Debug)]
pub struct SeatTable {
    held: Vec<Option<(MindId, u64)>>,
    clock: u64,
}

impl SeatTable {
    pub fn new(seats: u32) -> Self {
        Self { held: vec![None; seats as usize], clock: 0 }
    }

    pub fn seats(&self) -> u32 {
        self.held.len() as u32
    }

    /// The seat `mind` holds, if any.
    pub fn seat_of(&self, mind: &MindId) -> Option<u32> {
        self.held
            .iter()
            .position(|h| h.as_ref().is_some_and(|(m, _)| m == mind))
            .map(|i| i as u32)
    }

    /// Decide where `mind` sits and claim the seat for her (most recent use = now).
    /// `None` only when the table has no seats at all.
    pub fn seat(&mut self, mind: &MindId) -> Option<Seating> {
        self.clock += 1;
        let now = self.clock;
        if let Some(seat) = self.seat_of(mind) {
            self.held[seat as usize] = Some((mind.clone(), now));
            return Some(Seating::Resident { seat });
        }
        if let Some(i) = self.held.iter().position(Option::is_none) {
            self.held[i] = Some((mind.clone(), now));
            return Some(Seating::Free { seat: i as u32 });
        }
        let (i, displaced) = self
            .held
            .iter()
            .enumerate()
            .filter_map(|(i, h)| h.as_ref().map(|(m, used)| (i, m.clone(), *used)))
            .min_by_key(|(_, _, used)| *used)
            .map(|(i, m, _)| (i, m))?;
        self.held[i] = Some((mind.clone(), now));
        Some(Seating::Displace { seat: i as u32, displaced })
    }

    /// Free `mind`'s seat (she was paged out). Returns the seat she held.
    pub fn release(&mut self, mind: &MindId) -> Option<u32> {
        let seat = self.seat_of(mind)?;
        self.held[seat as usize] = None;
        Some(seat)
    }

    /// Forget every seat: the engine's KV is gone (it was relaunched).
    pub fn clear(&mut self) {
        self.held.iter_mut().for_each(|h| *h = None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(s: &str) -> MindId {
        MindId(s.into())
    }

    // what this catches: the replay seating policy drifting from the core's LRU slot
    // pool — a resident mind re-seated as new, or a recently active mind displaced
    // ahead of an idle one, would make every page-in count in the report wrong.
    #[test]
    fn seats_fill_then_displace_the_least_recent_mind() {
        let mut t = SeatTable::new(2);
        assert_eq!(t.seat(&m("a")), Some(Seating::Free { seat: 0 }));
        assert_eq!(t.seat(&m("b")), Some(Seating::Free { seat: 1 }));
        assert_eq!(t.seat(&m("a")), Some(Seating::Resident { seat: 0 }));
        assert_eq!(t.seat(&m("c")), Some(Seating::Displace { seat: 1, displaced: m("b") }));
        assert_eq!(t.seat_of(&m("b")), None);
        assert_eq!(t.release(&m("a")), Some(0));
        assert_eq!(t.seat(&m("b")), Some(Seating::Free { seat: 0 }));
        t.clear();
        assert_eq!(t.seat_of(&m("c")), None);
        assert_eq!(SeatTable::new(0).seat(&m("a")), None);
    }
}
