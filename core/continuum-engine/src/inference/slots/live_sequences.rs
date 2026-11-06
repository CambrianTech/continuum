//! What each slot physically holds and how new it is, against what each confirmed page captured.
//!
//! The M5 (core ed142f14c, 2026-11-06 16:13-19:17Z): Benchy's older page (22,222 slot tokens,
//! resumed at checkpoint 9,078 every time) was restored over her live, newer sequence on 7 acts,
//! 254,147 tokens and 4,420 prefill-seconds, because attribution (`holders`) was the only record
//! and a dropped admission had erased it. Attribution says whose page a slot is SAVED as; this
//! says how new the sequence IN it is, so a restore is judged against what she should resume.
//!
//! A generation is minted when a page's generation completes in a slot (its terminal receipt).
//! A slot's record changes only when the engine changes the slot: a request is POSTed there (the
//! record turns unconfirmed until that request's receipt), a generation completes there, or a
//! restore, an erase or transient traffic lands there.

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
    /// Her attribution was lost, but the slot still holds her CONFIRMED sequence, at least as new
    /// as her page: she resumes it in place. A restore here would regress her.
    LiveKept,
    /// Her page holds her newest known sequence: restore it.
    PageCurrent,
    /// Her page is OLDER than her newest completed sequence, which no slot is known to hold any
    /// more: the restore is the best left, and a regression.
    PageBehindLive,
    /// Nothing of hers to resume or restore: she prefills.
    Cold,
}

/// What a slot holds, as far as a receipt confirms it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Held {
    /// A completed generation or a verified restore left this page's sequence here, this new.
    Confirmed(PageKey, Generation),
    /// A request of this page was POSTed here and no terminal receipt has said what it left: it
    /// may have truncated or replaced the sequence (a changed prompt on the same page), so it
    /// proves no live hit until its completion confirms a new generation (Codex on #5804).
    Unconfirmed(PageKey),
}

#[derive(Debug, Default)]
pub(super) struct LiveSequences {
    minted: u64,
    /// slot -> what it holds; absent = nothing known (a restore, an erase or transient traffic).
    slots: HashMap<u32, Held>,
    /// page -> its newest generation completed in this pool.
    newest: HashMap<PageKey, u64>,
    /// page -> the generation its confirmed save captured.
    pages: HashMap<PageKey, Generation>,
    /// page -> the generation a detached slot held, for the save that follows the detach.
    detached: HashMap<PageKey, Generation>,
}

impl LiveSequences {
    /// `key`'s generation completed in `slot` (its terminal receipt): the slot holds her newest
    /// sequence.
    pub(super) fn completed(&mut self, slot: u32, key: PageKey) {
        self.minted += 1;
        self.detached.remove(&key);
        self.slots.insert(slot, Held::Confirmed(key, Some(self.minted)));
        self.newest.insert(key, self.minted);
    }

    /// A request of `key` was POSTed to `slot`: what the slot holds is unconfirmed until that
    /// request's terminal receipt ([`Self::completed`]); a failure or cancel leaves it so.
    pub(super) fn dispatched(&mut self, slot: u32, key: PageKey) {
        self.slots.insert(slot, Held::Unconfirmed(key));
    }

    /// The engine is replacing `slot`'s content (a restore, an erase): nothing is known until a
    /// receipt says.
    pub(super) fn overwritten(&mut self, slot: u32) {
        self.slots.remove(&slot);
    }

    /// Transient traffic is about to replace `slot`'s content: the slot holds nothing known, and
    /// the save of its resident that follows still captures what it confirmably held.
    pub(super) fn detached(&mut self, slot: u32) {
        if let Some(Held::Confirmed(key, generation)) = self.slots.remove(&slot) {
            self.detached.insert(key, generation);
        }
    }

    /// `key`'s page was restored into `slot`: the slot holds the sequence her page captured.
    pub(super) fn restored(&mut self, slot: u32, key: PageKey) {
        self.detached.remove(&key);
        self.slots.insert(slot, Held::Confirmed(key, self.page(&key)));
    }

    /// `key`'s page save was confirmed: it captured her newest confirmed sequence a slot (or a slot
    /// just detached for transient traffic) holds; an unconfirmed slot's capture is unknown.
    pub(super) fn saved(&mut self, key: PageKey) {
        let held = self.slots.values().filter_map(|held| match held {
            Held::Confirmed(page, generation) if *page == key => Some(*generation),
            _ => None,
        });
        let captured = held.chain(self.detached.remove(&key)).max();
        self.pages.insert(key, captured.flatten());
    }

    fn page(&self, key: &PageKey) -> Generation {
        self.pages.get(key).copied().flatten()
    }

    /// Judge `key`'s admission to `slot`, whose attribution was `prev`, with or without a
    /// confirmed page. Only a CONFIRMED sequence of hers proves a live hit; an unconfirmed or
    /// unknown slot falls back to her confirmed page, or cold.
    pub(super) fn judge(&self, slot: u32, key: PageKey, prev: Option<PageKey>, page_saved: bool) -> SequenceBasis {
        if prev == Some(key) {
            return SequenceBasis::Attributed;
        }
        if let (None, Some(Held::Confirmed(held, live))) = (prev, self.slots.get(&slot)) {
            if *held == key && (!page_saved || *live >= self.page(&key)) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn key(mind: u128) -> PageKey {
        PageKey::mind(uuid::Uuid::from_u128(mind)).expect("test: non-nil id")
    }

    // what this catches (Codex on #5804 at 541919d6f): a POST of a changed prompt on the same page
    // may truncate or replace the slot; a sent request that never confirmed must not let her old
    // completed revision claim a live hit over her confirmed page.
    #[test]
    fn a_sent_unconfirmed_request_never_proves_a_live_hit() {
        let mut sequences = LiveSequences::default();
        sequences.completed(0, key(1));
        sequences.saved(key(1));
        sequences.dispatched(0, key(1));
        assert_eq!(sequences.judge(0, key(1), None, true), SequenceBasis::PageCurrent, "her confirmed page, never LiveKept");
        assert_eq!(sequences.judge(0, key(1), None, false), SequenceBasis::Cold, "no page: cold, never LiveKept");
        sequences.completed(0, key(1));
        assert_eq!(sequences.judge(0, key(1), None, true), SequenceBasis::LiveKept, "its completion confirms the new revision");
    }
}
