//! What each slot physically holds and how new it is, against what each confirmed page captured.
//!
//! The M5 (core ed142f14c, 2026-11-06 16:13-19:17Z): Benchy's older page (22,222 slot tokens,
//! resumed at checkpoint 9,078 every time) was restored over her live, newer sequence on 7 acts,
//! 254,147 tokens and 4,420 prefill-seconds, because attribution (`holders`) was the only record
//! and a dropped admission had erased it. Attribution says whose page a slot is SAVED as; this
//! says how new the sequence IN it is, so a restore is judged against what she should resume.
//!
//! A generation is minted when a page's generation completes in a slot. A slot's record changes
//! only when the engine changes the slot: a generation completes there, another page's generation
//! is sent there, or a restore, an erase or transient traffic lands there.

use std::collections::HashMap;

use super::PageKey;

/// How new a sequence is: minted by this pool, or `None` for one from before it (a page an earlier
/// core saved). `None` orders below every minted generation.
type Generation = Option<u64>;

/// How this admission's KV relates to her live sequence and her page: the decision
/// [`super::KvSlotPool::plan_paging`] records, and the evidence a restore is judged by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceBasis {
    /// The slot is attributed to her: her KV is resident.
    Attributed,
    /// Her attribution was lost, but the slot still holds her sequence, at least as new as her
    /// page: she resumes it in place. A restore here would regress her.
    LiveKept,
    /// Her page holds her newest known sequence: restore it.
    PageCurrent,
    /// Her page is OLDER than her newest completed sequence, which no slot holds any more: the
    /// restore is the best left, and a regression.
    PageBehindLive,
    /// Nothing of hers to resume or restore: she prefills.
    Cold,
}

#[derive(Debug, Default)]
pub(super) struct LiveSequences {
    minted: u64,
    /// slot -> whose sequence it physically holds, and how new.
    slots: HashMap<u32, (PageKey, Generation)>,
    /// page -> its newest generation completed in this pool.
    newest: HashMap<PageKey, u64>,
    /// page -> the generation its confirmed save captured.
    pages: HashMap<PageKey, Generation>,
    /// page -> the generation a detached slot held, for the save that follows the detach.
    detached: HashMap<PageKey, Generation>,
}

impl LiveSequences {
    /// `key`'s generation completed in `slot`: the slot holds her newest sequence.
    pub(super) fn completed(&mut self, slot: u32, key: PageKey) {
        self.minted += 1;
        self.detached.remove(&key);
        self.slots.insert(slot, (key, Some(self.minted)));
        self.newest.insert(key, self.minted);
    }

    /// A generation of `key` was sent to `slot`: another page's sequence there is overwritten;
    /// hers only grows from what it held.
    pub(super) fn dispatched(&mut self, slot: u32, key: PageKey) {
        if self.slots.get(&slot).is_some_and(|(held, _)| *held != key) {
            self.slots.remove(&slot);
        }
    }

    /// The engine is replacing `slot`'s content (a restore, an erase, transient traffic): what it
    /// holds is unknown until a receipt says.
    pub(super) fn overwritten(&mut self, slot: u32) {
        self.slots.remove(&slot);
    }

    /// Transient traffic is about to replace `slot`'s content: the slot holds nothing known, and
    /// the save of its resident that follows still captures what it held.
    pub(super) fn detached(&mut self, slot: u32) {
        if let Some((key, generation)) = self.slots.remove(&slot) {
            self.detached.insert(key, generation);
        }
    }

    /// `key`'s page was restored into `slot`: the slot holds the sequence her page captured.
    pub(super) fn restored(&mut self, slot: u32, key: PageKey) {
        self.detached.remove(&key);
        self.slots.insert(slot, (key, self.page(&key)));
    }

    /// `key`'s page save was confirmed: it captured her newest sequence a slot (or a slot just
    /// detached for transient traffic) holds.
    pub(super) fn saved(&mut self, key: PageKey) {
        let held = self.slots.values().filter(|(held, _)| *held == key).map(|(_, generation)| *generation);
        let captured = held.chain(self.detached.remove(&key)).max();
        self.pages.insert(key, captured.flatten());
    }

    fn page(&self, key: &PageKey) -> Generation {
        self.pages.get(key).copied().flatten()
    }

    /// Judge `key`'s admission to `slot`, whose attribution was `prev`, with or without a
    /// confirmed page.
    pub(super) fn judge(&self, slot: u32, key: PageKey, prev: Option<PageKey>, page_saved: bool) -> SequenceBasis {
        if prev == Some(key) {
            return SequenceBasis::Attributed;
        }
        let held = self.slots.get(&slot).filter(|(held, _)| prev.is_none() && *held == key);
        if let Some((_, live)) = held {
            if !page_saved || *live >= self.page(&key) {
                return SequenceBasis::LiveKept;
            }
        }
        match (page_saved, self.newest.get(&key)) {
            (false, _) => SequenceBasis::Cold,
            (true, Some(newest)) if Some(*newest) > self.page(&key) => SequenceBasis::PageBehindLive,
            (true, _) => SequenceBasis::PageCurrent,
        }
    }
}
