//! Her work in hand: what her view keeps whole through a turnover while she works on it, as data
//! her sources mark (`continuum_context::source::Item::in_hand`), never a separate allowance. A
//! start charges it to its source's grant like any item and collapses everything else oldest
//! first (`continuum_context::view`).
//!
//! Two things, both read off state her memory already keeps: the read of each file she is editing
//! from where her hands stand (her open views: an edit of a file she has open, `OpenView::edited_seq`),
//! and her own notes on what those edits did (the first thought she wrote after each edit's result,
//! since the read she keeps). On the M5, 2026-11-14 00:24-00:28Z, two overflow turnovers collapsed
//! the 849-line file Aris was editing to its outline and her "item 4 is done" note to its head and
//! tail; she read the file whole again after each and went back to item 4.

use super::*;
use crate::cognition::act_observe::ToolVerb;

/// What her work in hand holds open, by the keys her sources give it.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct InHand {
    /// The acts (`#n`) whose reads of the files she is editing her view keeps whole.
    pub reads: std::collections::HashSet<u64>,
    /// Her notes on what her edits did: the first thought (by occurrence) she wrote after each
    /// edit's result since the oldest read she keeps.
    pub notes: std::collections::HashSet<u64>,
}

impl WorkingMemory {
    /// Her work in hand now: empty with no edit open where her hands stand.
    pub(crate) fn in_hand(&self) -> InHand {
        let scope = self.scope.lock().clone();
        let reads = self.open_views.lock().editing_reads(scope.as_deref());
        let Some(since) = reads.iter().min().copied() else {
            return InHand::default();
        };
        // When each edit since that read returned, from where her hands stand.
        let edited: Vec<u64> = self
            .actions
            .lock()
            .records()
            .filter(|record| record.seq > since && record.scope == scope)
            .filter(|record| matches!(ToolVerb::classify(&record.operation), ToolVerb::Edit | ToolVerb::Write))
            .filter_map(|record| record.received_ms)
            .collect();
        // Her note on an edit: the first thought she had after its result returned (strictly after,
        // so the thought that decided the edit, recorded before it ran, is never its note).
        let thoughts = self.thoughts.lock();
        let notes = edited
            .iter()
            .filter_map(|&returned| thoughts.records().find(|thought| thought.at_ms > returned).map(|thought| thought.occurrence))
            .collect();
        InHand { reads: reads.into_iter().collect(), notes }
    }
}
