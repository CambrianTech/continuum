//! Pending perception: the room lines a mind has durably accepted from the store but not yet been
//! shown, held in her working-memory snapshot beside task results, through the one owner checkpoint
//! (PERCEPTION-FROM-THE-STORE.md, P2; Codex #5691 773d34a4). The source cursor, the gap receipts and
//! the wake obligation commit in the same snapshot as the references, so a crash can replay an
//! acceptance (idempotent) but never skip or repeat a perceived line.
//!
//! The references are the typed routing header only (sequence, event, room, author); the event body
//! stays in the store, opened by the consumer that renders it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::WorkingMemory;

/// One accepted room line, by its typed header.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerceptionRef {
    pub arrival_seq: u64,
    pub event: Uuid,
    pub room: Uuid,
    pub author: Uuid,
}

/// Why history cannot be perceived; receipted once in the snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerceptionGap {
    Retention { from: u64, through: u64 },
    UnvouchedImport,
}

/// One page the drain produced, to accept atomically.
#[derive(Clone, Debug, PartialEq)]
pub enum AcceptedPage {
    /// Rows to perceive and the sequence the page scanned through (her own rows included).
    Rows {
        rows: Vec<PerceptionRef>,
        scanned_through: u64,
    },
    /// A gap she is told about; accepting it moves the cursor to its end.
    Gap(PerceptionGap),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct PendingPerception {
    /// Everything through this sequence is accepted (or receipted as a gap).
    through: u64,
    unvouched_receipted: bool,
    gaps: Vec<PerceptionGap>,
    entries: BTreeMap<u64, PendingLine>,
    // Runtime publication fence: a wake is visible only after the snapshot holding it is saved.
    #[serde(skip)]
    published: BTreeMap<u64, bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PendingLine {
    line: PerceptionRef,
    wake_pending: bool,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum PerceptionAcceptError {
    #[error(
        "page scans through {scanned} but the cursor is already through {through}: not contiguous"
    )]
    Behind { scanned: u64, through: u64 },
    #[error("row {row} is outside the page (cursor {through}, scanned through {scanned})")]
    OutsidePage {
        row: u64,
        through: u64,
        scanned: u64,
    },
    #[error("arrival {0} has no published acceptance")]
    Unpublished(u64),
}

impl WorkingMemory {
    /// The sequence her accepted prefix reaches and whether the unvouched gap was receipted: the
    /// drain's cursor.
    pub fn perception_cursor(&self) -> (u64, bool) {
        let pending = self.pending_perception.lock();
        (pending.through, pending.unvouched_receipted)
    }

    /// Stage one drained page: validate the whole page, then advance the cursor and add its rows
    /// with their wake obligations. A replay of an already-accepted page is inert (returns false).
    /// The registry must checkpoint this exact state before any wake is published.
    pub fn stage_perception_page(&self, page: AcceptedPage) -> Result<bool, PerceptionAcceptError> {
        let mut pending = self.pending_perception.lock();
        match page {
            AcceptedPage::Gap(PerceptionGap::UnvouchedImport) => {
                let changed = !pending.unvouched_receipted;
                if changed {
                    pending.unvouched_receipted = true;
                    pending.gaps.push(PerceptionGap::UnvouchedImport);
                }
                Ok(changed)
            }
            AcceptedPage::Gap(gap @ PerceptionGap::Retention { through, .. }) => {
                if through <= pending.through {
                    return Ok(false); // replay of a receipted gap
                }
                pending.through = through;
                pending.gaps.push(gap);
                Ok(true)
            }
            AcceptedPage::Rows {
                rows,
                scanned_through,
            } => {
                if scanned_through <= pending.through {
                    return Ok(false); // replay of an accepted page
                }
                let through = pending.through;
                if let Some(row) = rows
                    .iter()
                    .find(|r| r.arrival_seq <= through || r.arrival_seq > scanned_through)
                {
                    return Err(PerceptionAcceptError::OutsidePage {
                        row: row.arrival_seq,
                        through,
                        scanned: scanned_through,
                    });
                }
                for line in rows {
                    pending
                        .entries
                        .entry(line.arrival_seq)
                        .or_insert(PendingLine {
                            line,
                            wake_pending: true,
                        });
                }
                pending.through = scanned_through;
                Ok(true)
            }
        }
    }

    /// Lines accepted, published by a saved snapshot, and not yet shown: her pending perception.
    pub fn pending_perception_lines(&self) -> Vec<PerceptionRef> {
        let pending = self.pending_perception.lock();
        pending
            .entries
            .iter()
            .filter(|(seq, line)| {
                line.wake_pending && pending.published.get(seq) == Some(&true)
            })
            .map(|(_, line)| line.line)
            .collect()
    }

    /// Stage retirement of lines a rendered-context receipt proves she was shown. Unknown or
    /// unpublished lines cannot be retired.
    pub fn stage_perception_reads(&self, seqs: &[u64]) -> Result<bool, PerceptionAcceptError> {
        let mut pending = self.pending_perception.lock();
        if let Some(seq) = seqs
            .iter()
            .find(|s| !pending.published.contains_key(s) || !pending.entries.contains_key(s))
        {
            return Err(PerceptionAcceptError::Unpublished(*seq));
        }
        let mut changed = false;
        for seq in seqs {
            if let Some(line) = pending.entries.get_mut(seq) {
                changed |= line.wake_pending;
                line.wake_pending = false;
            }
        }
        Ok(changed)
    }

    /// The shared checkpoint writer calls this only after syncing its exact snapshot.
    pub(crate) fn publish_perception_checkpoint(&self, saved: PerceptionPublication) {
        let mut pending = self.pending_perception.lock();
        for (seq, wake) in saved.0 {
            if pending.entries.contains_key(&seq) {
                pending.published.insert(seq, wake);
            }
        }
    }
}

/// Exact wake projection of one serialized snapshot. Constructed only by its owner.
pub(crate) struct PerceptionPublication(BTreeMap<u64, bool>);

impl PendingPerception {
    /// A restored snapshot was saved, so every entry it holds is published as saved.
    pub(crate) fn publish_restored(&mut self) {
        self.published = self
            .entries
            .iter()
            .map(|(seq, line)| (*seq, line.wake_pending))
            .collect();
    }

    pub(crate) fn publication(&self) -> PerceptionPublication {
        PerceptionPublication(
            self.entries
                .iter()
                .map(|(seq, line)| (*seq, line.wake_pending))
                .collect(),
        )
    }
}
