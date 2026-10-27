//! `ServingTarget` for an engine that exposes raw seats and saves/restores one seat at a
//! time (llama-server's slots, and the test fake). The paging flow lives here once:
//! seat by `SeatTable`, save the displaced mind's page, restore hers or start cold.

use std::collections::HashSet;

use crate::seats::{SeatTable, Seating};
use crate::target::{
    ActOutcome, ActRequest, EngineStats, MindId, PageInKind, PageInReceipt, PageOutReceipt, ServingTarget, TargetError,
};

pub struct SavedPage {
    pub tokens: u64,
    pub bytes: u64,
    pub ms: f64,
}

pub struct RestoredPage {
    pub tokens: u64,
    pub ms: f64,
}

/// What a seat-exposing engine answers.
pub trait SlotEngine {
    fn seats(&self) -> u32;
    /// Make sure the engine is up. `Ok(true)` when it had died and was replaced, which
    /// means every seat's KV is gone.
    fn revive(&mut self) -> Result<bool, TargetError>;
    fn save(&mut self, seat: u32, page: &str) -> Result<SavedPage, TargetError>;
    fn restore(&mut self, seat: u32, page: &str) -> Result<RestoredPage, TargetError>;
    fn act(&mut self, seat: u32, request: &ActRequest) -> Result<ActOutcome, TargetError>;
    /// Stats with `relaunches` filled by the engine when it can observe them.
    fn stats(&mut self) -> EngineStats;
}

pub struct SeatedTarget<E: SlotEngine> {
    engine: E,
    table: SeatTable,
    saved: HashSet<MindId>,
}

impl<E: SlotEngine> SeatedTarget<E> {
    pub fn new(engine: E) -> Self {
        let table = SeatTable::new(engine.seats());
        Self { engine, table, saved: HashSet::new() }
    }

    pub fn engine(&self) -> &E {
        &self.engine
    }

    fn revive(&mut self) -> Result<(), TargetError> {
        if self.engine.revive()? {
            self.table.clear();
        }
        Ok(())
    }

    fn save(&mut self, seat: u32, mind: &MindId) -> Result<PageOutReceipt, TargetError> {
        let page = self.engine.save(seat, &page_name(mind))?;
        self.saved.insert(mind.clone());
        Ok(PageOutReceipt { mind: mind.clone(), tokens: page.tokens, bytes: page.bytes, ms: page.ms })
    }
}

/// The page file a mind's KV is saved under. The engine validates file names, so only
/// `[A-Za-z0-9_-]` survives from the mind id.
pub fn page_name(mind: &MindId) -> String {
    let safe: String = mind.0.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    format!("page-{safe}.bin")
}

impl<E: SlotEngine> ServingTarget for SeatedTarget<E> {
    fn page_in(&mut self, mind: &MindId) -> Result<PageInReceipt, TargetError> {
        self.revive()?;
        let seating = self.table.seat(mind).ok_or_else(|| TargetError("the engine has no seats".into()))?;
        let (seat, evicted) = match seating {
            Seating::Resident { .. } => {
                return Ok(PageInReceipt { kind: PageInKind::Resident, tokens_restored: 0, ms: 0.0, evicted: None })
            }
            Seating::Free { seat } => (seat, None),
            Seating::Displace { seat, displaced } => match self.save(seat, &displaced) {
                Ok(receipt) => (seat, Some(receipt)),
                Err(e) => {
                    // The displaced mind's KV is still in the seat; the seat is no one's now.
                    self.table.release(mind);
                    return Err(e);
                }
            },
        };
        if !self.saved.contains(mind) {
            return Ok(PageInReceipt { kind: PageInKind::Cold, tokens_restored: 0, ms: 0.0, evicted });
        }
        Ok(match self.engine.restore(seat, &page_name(mind)) {
            Ok(page) => PageInReceipt { kind: PageInKind::Restored, tokens_restored: page.tokens, ms: page.ms, evicted },
            Err(_) => PageInReceipt { kind: PageInKind::RestoreRejected, tokens_restored: 0, ms: 0.0, evicted },
        })
    }

    fn act(&mut self, mind: &MindId, request: &ActRequest) -> Result<ActOutcome, TargetError> {
        let seat = self.table.seat_of(mind).ok_or_else(|| TargetError(format!("{mind} acted without a seat")))?;
        self.engine.act(seat, request)
    }

    fn page_out(&mut self, mind: &MindId) -> Result<PageOutReceipt, TargetError> {
        self.revive()?;
        let seat = self.table.seat_of(mind).ok_or_else(|| TargetError(format!("{mind} holds no seat to page out")))?;
        let receipt = self.save(seat, mind)?;
        self.table.release(mind);
        Ok(receipt)
    }

    fn stats(&mut self) -> Result<EngineStats, TargetError> {
        Ok(self.engine.stats())
    }
}
