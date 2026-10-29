//! Her actions, one record each.
//!
//! Every act she takes is ONE record under its sequence number: what it was (the ledger
//! line), where (room, workspace root), its whole result while retained, and the handle of
//! the saved whole that reopens it once collapsed. This replaces three copies that each
//! evicted on their own rule (the results ring, the steps-taken archive, the collapsed-
//! pointer index), so one act now has one key, which is what lets her prompt append an act
//! instead of re-rendering three blocks around it.

use std::collections::{HashSet, VecDeque};

use uuid::Uuid;

/// How many collapsed results the legacy results ring lists as pointer lines (and the
/// checkpoint's rollback shape carries). A rendering count only: every record that names a
/// handle keeps it live and stays in the log (`receipt_keys`, `prune`).
const POINTER_INDEX: usize = 12;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ActionRecord {
    pub seq: u64,
    /// The room the act was FOR; `None` for an unscoped or legacy receipt.
    pub room: Option<Uuid>,
    /// The workspace root she acted from; `None` when recorded before roots existed.
    pub scope: Option<String>,
    /// The typed call names in order, `unrecorded` for a legacy text receipt.
    pub operation: String,
    /// The ledger line, `[action #n] name(args)`; `None` for an act restored from a
    /// checkpoint that kept only its pointer (its line had left the ledger).
    pub head: Option<String>,
    /// The whole result while retained; `None` once collapsed to its handle.
    pub result: Option<String>,
    /// The saved whole; `None` when the save failed, and then the result never collapses
    /// (nothing else holds its bytes).
    pub handle: Option<String>,
    /// The pointer exactly as a build before the records wrote it, restored from its
    /// checkpoint: its operation and workspace live only in that text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub written_pointer: Option<String>,
    /// The result's summary level (`continuum_context::levels`): a numbered read's outline,
    /// else an output's head and tail; what her view gives it when it cannot keep it whole.
    /// Kept after the result collapses, so what she read stays addressable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// The whole result's length once its body left RAM for its handle: what her view prices the
    /// whole level at without reading it (`continuum_context::StoredWhole`). Eviction is storage,
    /// never visibility (FOCUS-CONTRACT rule 7). `None` while resident, and for records evicted
    /// before this was kept (their whole then has no price: they show at pointer level).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stored_chars: Option<usize>,
    /// When the result was received (ms since the Unix epoch): the one clock that orders every
    /// mind's resident results oldest-first when the memory owner sheds across minds. `None`
    /// for a record from a build before this was kept, which sheds first (it is older than any
    /// result received since).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub received_ms: Option<u64>,
}

impl ActionRecord {
    /// The key its per-call dependencies are filed under: its saved handle, or `seq:N` for
    /// a result whose save failed.
    pub fn receipt_key(&self) -> String {
        super::receipt_key(self.seq, self.handle.as_deref())
    }

    /// The RAM its resident body holds, in bytes: what the memory owner accounts, never a
    /// character count (that is `stored_chars`, the view's price).
    pub fn resident_bytes(&self) -> u64 {
        self.result.as_ref().map_or(0, |result| result.len() as u64)
    }
}

/// What a collapse left behind.
pub(super) struct Collapsed {
    /// Bytes still resident.
    pub total: u64,
}

#[derive(Debug, Default)]
pub(super) struct ActionLog {
    /// Ascending by `seq`.
    records: VecDeque<ActionRecord>,
}

impl ActionLog {
    /// A log from records in any order, bounded to `archive_chars` like a live one.
    pub fn from_records(mut records: Vec<ActionRecord>, archive_chars: usize) -> Self {
        records.sort_by_key(|record| record.seq);
        records.dedup_by_key(|record| record.seq);
        let mut log = Self { records: records.into() };
        log.prune(archive_chars);
        log
    }

    pub fn push(&mut self, record: ActionRecord, archive_chars: usize) {
        self.records.push_back(record);
        self.prune(archive_chars);
    }

    pub fn get(&self, seq: u64) -> Option<&ActionRecord> {
        self.records.binary_search_by_key(&seq, |record| record.seq).ok().map(|index| &self.records[index])
    }

    /// Every record, oldest first.
    pub fn records(&self) -> impl Iterator<Item = &ActionRecord> {
        self.records.iter()
    }

    /// Results retained whole, oldest first.
    pub fn retained(&self) -> impl Iterator<Item = &ActionRecord> {
        self.records.iter().filter(|record| record.result.is_some())
    }

    /// Collapsed results that still name their handle, oldest first.
    pub fn indexed_pointers(&self) -> Vec<&ActionRecord> {
        let mut pointers: Vec<&ActionRecord> = self
            .records
            .iter()
            .rev()
            .filter(|record| record.result.is_none() && record.handle.is_some())
            .take(POINTER_INDEX)
            .collect();
        pointers.reverse();
        pointers
    }

    /// The newest ledger lines that fit `archive_chars`, oldest first; at least the newest.
    pub fn archive(&self, archive_chars: usize) -> Vec<(&ActionRecord, &str)> {
        let mut used = 0usize;
        let mut kept: Vec<(&ActionRecord, &str)> = Vec::new();
        for (record, head) in self.records.iter().rev().filter_map(|r| r.head.as_deref().map(|h| (r, h))) {
            let cost = head.chars().count() + 1;
            if !kept.is_empty() && used + cost > archive_chars {
                break;
            }
            used += cost;
            kept.push((record, head));
        }
        kept.reverse();
        kept
    }

    /// Every receipt whose dependencies must stay reachable: each retained result's key and
    /// every handle a record names. A body moved to its handle exists only on disk, so its
    /// handle is live for as long as its record is, however many pointers are newer.
    pub fn receipt_keys(&self) -> HashSet<String> {
        self.records
            .iter()
            .filter(|record| record.result.is_some() || record.handle.is_some())
            .map(ActionRecord::receipt_key)
            .collect()
    }

    /// The RAM every resident body holds, in bytes.
    pub fn resident_bytes(&self) -> u64 {
        self.records.iter().map(ActionRecord::resident_bytes).sum()
    }

    /// The records a shed may move to their handles, oldest first: resident, saved, not the
    /// newest result, and vouched for by `saved(seq, handle)` (its output is there to reload).
    /// Everything else is the last copy of what she read, so it stays.
    pub fn evictable<'a>(
        &'a self,
        saved: &'a dyn Fn(u64, &str) -> bool,
    ) -> impl Iterator<Item = &'a ActionRecord> + 'a {
        let newest = self.retained().last().map(|record| record.seq);
        self.records.iter().filter(move |record| {
            record.result.is_some()
                && Some(record.seq) != newest
                && record.handle.as_deref().is_some_and(|handle| saved(record.seq, handle))
        })
    }

    /// Move the oldest evictable bodies to their handles until at most `budget` bytes stay
    /// resident. Each keeps its measured length (`stored_chars`) and its handle, so her view
    /// still prices and reloads its whole. Exactly to the budget: the memory owner decides
    /// when, so there is no hysteresis here.
    pub fn collapse_over(&mut self, budget: u64, saved: &dyn Fn(u64, &str) -> bool) -> Collapsed {
        let mut total = self.resident_bytes();
        if total <= budget {
            return Collapsed { total };
        }
        let chosen: HashSet<u64> = {
            let mut over = total - budget;
            self.evictable(saved)
                .take_while(|record| {
                    let take = over > 0;
                    over = over.saturating_sub(record.resident_bytes());
                    take
                })
                .map(|record| record.seq)
                .collect()
        };
        for record in self.records.iter_mut().filter(|record| chosen.contains(&record.seq)) {
            total -= record.resident_bytes();
            record.stored_chars = record.result.as_ref().map(|result| result.chars().count());
            record.result = None;
        }
        Collapsed { total }
    }

    /// Drop the records nothing can reach any more: past the ledger's share, no body in RAM,
    /// and no handle to reload one from. A record naming a handle stays, since its whole is
    /// reachable through her view (`StoredWhole`) for as long as the record is.
    fn prune(&mut self, archive_chars: usize) {
        let keep: HashSet<u64> = self.archive(archive_chars).into_iter().map(|(record, _)| record.seq).collect();
        self.records
            .retain(|record| record.result.is_some() || record.handle.is_some() || keep.contains(&record.seq));
    }
}
