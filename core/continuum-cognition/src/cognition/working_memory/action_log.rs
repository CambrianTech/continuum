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

/// How many collapsed results keep their pointer (and their saved whole) reachable. The
/// bound the collapsed-pointer index already had; the ledger line outlives it.
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
    /// What each of its calls did to a file, from the typed acts at the receipt: what lets a
    /// later read or edit of the same file supersede this result ([`ActionLog::superseded`]).
    /// Empty for a record restored from a checkpoint written before it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub touches: Vec<Touch>,
}

/// What one call of an act did to a file.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Touch {
    /// Read it; `lines` is the 1-based range shown, when its producer said.
    Read { path: String, lines: Option<(u32, u32)> },
    /// Wrote or edited it.
    Changed { path: String },
    /// Anything that is not a file read or change (a search, a shell, a board call).
    Other,
}

impl ActionRecord {
    /// The key its per-call dependencies are filed under: its saved handle, or `seq:N` for
    /// a result whose save failed.
    pub fn receipt_key(&self) -> String {
        super::receipt_key(self.seq, self.handle.as_deref())
    }

    fn result_chars(&self) -> usize {
        self.result.as_ref().map_or(0, |result| result.chars().count() + 1)
    }
}

impl Touch {
    /// What each typed act of a receipt did to a file: a read by the lines it showed
    /// (`open_views::file_read`, the one reading of a read's result), a write or edit by the
    /// path its call named, anything else `Other`. An apply names no single file, so it is
    /// `Other`: it never supersedes a read, the conservative side.
    pub fn of(acts: &[crate::cognition::act_observe::Observation]) -> Vec<Touch> {
        use crate::cognition::act_observe::ToolVerb;
        acts.iter()
            .map(|act| {
                if let Some(read) = crate::cognition::open_views::file_read(act) {
                    return Touch::Read { path: read.path, lines: read.shown };
                }
                let named = act.call.input.get("file_path").or_else(|| act.call.input.get("path")).and_then(|path| path.as_str());
                match (&act.output.verb, named) {
                    (ToolVerb::Write | ToolVerb::Edit, Some(path)) if act.completed_successfully() => {
                        Touch::Changed { path: path.to_owned() }
                    }
                    _ => Touch::Other,
                }
            })
            .collect()
    }
}

/// What a collapse left behind.
pub(super) struct Collapsed {
    /// Characters still retained in full.
    pub total: usize,
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
    /// each indexed pointer's handle.
    pub fn receipt_keys(&self) -> HashSet<String> {
        self.retained().chain(self.indexed_pointers()).map(ActionRecord::receipt_key).collect()
    }

    /// Results a newer act made redundant: every call of it read a file (in its workspace)
    /// that a later act changed, or read again showing at least the same lines. Their whole
    /// result is still one expand away by their handle; a start gives them their summary
    /// (`Source::superseded`). A record with any call that is not a file read never is: its
    /// other output is not repeated anywhere.
    pub fn superseded(&self) -> Vec<u64> {
        let records: Vec<&ActionRecord> = self.records.iter().collect();
        let covered = |earlier: &ActionRecord, path: &str, lines: Option<(u32, u32)>| {
            records.iter().filter(|later| later.seq > earlier.seq && later.scope == earlier.scope).any(|later| {
                later.touches.iter().any(|touch| match touch {
                    Touch::Changed { path: changed } => changed == path,
                    Touch::Read { path: read, lines: shown } => {
                        read == path
                            && match (lines, shown) {
                                (Some((a, b)), Some((c, d))) => *c <= a && b <= *d,
                                // A range unknown on either side proves no coverage.
                                _ => false,
                            }
                    }
                    Touch::Other => false,
                })
            })
        };
        records
            .iter()
            .filter(|record| record.result.is_some() && !record.touches.is_empty())
            .filter(|record| {
                record.touches.iter().all(|touch| match touch {
                    Touch::Read { path, lines } => covered(**record, path.as_str(), *lines),
                    Touch::Changed { .. } | Touch::Other => false,
                })
            })
            .map(|record| record.seq)
            .collect()
    }

    /// Collapse the oldest saved results to their handles once the retained results exceed
    /// `cap`, down to half of it, so the head of what she reads stays byte-stable between two
    /// bursts. The newest result always stays, and an unsaved one stays whole.
    pub fn collapse_over(&mut self, cap: usize) -> Collapsed {
        let mut total: usize = self.records.iter().map(ActionRecord::result_chars).sum();
        if total <= cap {
            return Collapsed { total };
        }
        let target = cap / 2;
        let newest = self.retained().last().map(|record| record.seq);
        for record in self.records.iter_mut() {
            if total <= target {
                break;
            }
            if record.result.is_none() || record.handle.is_none() || Some(record.seq) == newest {
                continue;
            }
            total -= record.result_chars();
            record.result = None;
        }
        Collapsed { total }
    }

    /// Drop the records nothing shows any more: past the ledger's share, collapsed, and
    /// no longer indexed as a pointer.
    fn prune(&mut self, archive_chars: usize) {
        let keep: HashSet<u64> = self
            .archive(archive_chars)
            .into_iter()
            .map(|(record, _)| record)
            .chain(self.indexed_pointers())
            .map(|record| record.seq)
            .collect();
        self.records.retain(|record| record.result.is_some() || keep.contains(&record.seq));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(seq: u64, scope: &str, touches: Vec<Touch>) -> ActionRecord {
        ActionRecord {
            seq,
            room: None,
            scope: Some(scope.to_owned()),
            operation: "code/read".into(),
            head: None,
            result: Some(format!("result {seq}")),
            handle: Some(format!("h{seq}")),
            written_pointer: None,
            summary: None,
            touches,
        }
    }

    fn read(path: &str, lines: Option<(u32, u32)>) -> Touch {
        Touch::Read { path: path.into(), lines }
    }

    // what this catches: a whole read kept whole after she read the same lines again or edited
    // the file (Aiko, the M5, 2026-10-26: command_inbound_pump.rs, 33k chars, still whole 2.5
    // hours later), and the opposite error, a result summarized when nothing newer repeats it:
    // a partial re-read, an unknown range, another checkout's file, or a batch with other output.
    #[test]
    fn a_read_is_superseded_only_by_a_newer_read_of_the_same_lines_or_a_change() {
        let changed = |path: &str| Touch::Changed { path: path.into() };
        let records = vec![
            record(1, "w", vec![read("a.rs", Some((1, 100)))]),
            record(2, "w", vec![read("a.rs", Some((1, 200)))]),
            record(3, "w", vec![read("b.rs", Some((1, 50)))]),
            record(4, "w", vec![changed("b.rs")]),
            record(5, "w", vec![read("c.rs", Some((1, 50))), Touch::Other]),
            record(6, "w", vec![changed("c.rs")]),
            record(7, "w", vec![read("d.rs", None)]),
            record(8, "w", vec![read("d.rs", Some((1, 10)))]),
            record(9, "w", vec![read("e.rs", Some((1, 300)))]),
            record(10, "w", vec![read("e.rs", Some((100, 200)))]),
            record(11, "other", vec![read("f.rs", Some((1, 9)))]),
            record(12, "w", vec![read("f.rs", Some((1, 9)))]),
        ];
        let log = ActionLog::from_records(records, usize::MAX);
        assert_eq!(log.superseded(), [1, 3]);
    }
}
