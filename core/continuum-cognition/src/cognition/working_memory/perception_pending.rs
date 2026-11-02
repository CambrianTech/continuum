//! Pending perception: the room lines a mind has durably accepted from the store but not yet been
//! shown, held in her working-memory snapshot beside task results, through the one owner checkpoint
//! (PERCEPTION-FROM-THE-STORE.md, P2; Codex #5691 773d34a4). The source cursor, the gap receipts and
//! the wake obligation commit in the same snapshot as the references, so a crash can replay an
//! acceptance (idempotent) but never skip or repeat a perceived line.
//!
//! The references are the typed routing header only (sequence, event, room, author); the event body
//! stays in the store, opened by the consumer that renders it.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::WorkingMemory;

/// Which sequence a cursor walks. Sequences of different domains never alias: a page from one domain
/// cannot advance another's cursor (Codex on P2). Arrival sequences are global within
/// `chat_arrivals`, so one cursor covers all her rooms in that domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SequenceDomain {
    ChatArrivals,
}

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
        domain: SequenceDomain,
        rows: Vec<PerceptionRef>,
        scanned_through: u64,
    },
    /// A gap she is told about; accepting a retention gap moves that domain's cursor to its end.
    Gap {
        domain: SequenceDomain,
        gap: PerceptionGap,
    },
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct PendingPerception {
    /// Per domain: everything through this sequence is accepted (or receipted as a gap).
    through: BTreeMap<SequenceDomain, u64>,
    unvouched_receipted: BTreeSet<SequenceDomain>,
    gaps: Vec<(SequenceDomain, PerceptionGap)>,
    #[serde(with = "entry_list")]
    entries: BTreeMap<(SequenceDomain, u64), PendingLine>,
    // Runtime publication fence: a wake is visible only after the snapshot holding it is saved.
    #[serde(skip)]
    published: BTreeMap<(SequenceDomain, u64), bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PendingLine {
    line: PerceptionRef,
    wake_pending: bool,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum PerceptionAcceptError {
    #[error("row {row} is outside the page (cursor {through}, scanned through {scanned})")]
    OutsidePage {
        row: u64,
        through: u64,
        scanned: u64,
    },
    #[error("arrival {0} has no published acceptance")]
    Unpublished(u64),
}

impl PendingPerception {
    fn cursor(&self, domain: SequenceDomain) -> u64 {
        self.through.get(&domain).copied().unwrap_or_default() // unwrap_or_default: a domain never drained starts before its first sequence
    }
}

impl WorkingMemory {
    /// The sequence her accepted prefix reaches in `domain`, and whether its unvouched gap was
    /// receipted: the drain's cursor.
    pub fn perception_cursor(&self, domain: SequenceDomain) -> (u64, bool) {
        let pending = self.pending_perception.lock();
        (
            pending.cursor(domain),
            pending.unvouched_receipted.contains(&domain),
        )
    }

    /// Stage one drained page: validate the whole page, then advance its domain's cursor and add its
    /// rows with their wake obligations. A replay of an accepted page is inert (returns false). The
    /// registry must checkpoint this exact state before any wake is published.
    pub fn stage_perception_page(&self, page: AcceptedPage) -> Result<bool, PerceptionAcceptError> {
        let mut pending = self.pending_perception.lock();
        match page {
            AcceptedPage::Gap {
                domain,
                gap: PerceptionGap::UnvouchedImport,
            } => {
                let changed = pending.unvouched_receipted.insert(domain);
                if changed {
                    pending.gaps.push((domain, PerceptionGap::UnvouchedImport));
                }
                Ok(changed)
            }
            AcceptedPage::Gap {
                domain,
                gap: gap @ PerceptionGap::Retention { through, .. },
            } => {
                if through <= pending.cursor(domain) {
                    return Ok(false); // replay of a receipted gap
                }
                pending.through.insert(domain, through);
                pending.gaps.push((domain, gap));
                Ok(true)
            }
            AcceptedPage::Rows {
                domain,
                rows,
                scanned_through,
            } => {
                let through = pending.cursor(domain);
                if scanned_through <= through {
                    return Ok(false); // replay of an accepted page
                }
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
                        .entry((domain, line.arrival_seq))
                        .or_insert(PendingLine {
                            line,
                            wake_pending: true,
                        });
                }
                pending.through.insert(domain, scanned_through);
                Ok(true)
            }
        }
    }

    /// Lines accepted in `domain`, published by a saved snapshot, and not yet shown.
    pub fn pending_perception_lines(&self, domain: SequenceDomain) -> Vec<PerceptionRef> {
        let pending = self.pending_perception.lock();
        pending
            .entries
            .iter()
            .filter(|((d, _), line)| *d == domain && line.wake_pending)
            .filter(|(key, _)| pending.published.get(key) == Some(&true))
            .map(|(_, line)| line.line)
            .collect()
    }

    /// Stage retirement of lines a rendered-context receipt proves she was shown. Unknown or
    /// unpublished lines cannot be retired.
    pub fn stage_perception_reads(
        &self,
        domain: SequenceDomain,
        seqs: &[u64],
    ) -> Result<bool, PerceptionAcceptError> {
        let mut pending = self.pending_perception.lock();
        if let Some(seq) = seqs.iter().find(|s| {
            let key = (domain, **s);
            !pending.published.contains_key(&key) || !pending.entries.contains_key(&key)
        }) {
            return Err(PerceptionAcceptError::Unpublished(*seq));
        }
        let mut changed = false;
        for seq in seqs {
            if let Some(line) = pending.entries.get_mut(&(domain, *seq)) {
                changed |= line.wake_pending;
                line.wake_pending = false;
            }
        }
        Ok(changed)
    }

    /// The shared checkpoint writer calls this only after syncing its exact snapshot.
    pub(crate) fn publish_perception_checkpoint(&self, saved: PerceptionPublication) {
        let mut pending = self.pending_perception.lock();
        for (key, wake) in saved.0 {
            if pending.entries.contains_key(&key) {
                pending.published.insert(key, wake);
            }
        }
    }
}

/// Exact wake projection of one serialized snapshot. Constructed only by its owner.
pub(crate) struct PerceptionPublication(BTreeMap<(SequenceDomain, u64), bool>);

impl PendingPerception {
    /// A restored snapshot was saved, so every entry it holds is published as saved.
    pub(crate) fn publish_restored(&mut self) {
        self.published = self
            .entries
            .iter()
            .map(|(key, line)| (*key, line.wake_pending))
            .collect();
    }

    pub(crate) fn publication(&self) -> PerceptionPublication {
        PerceptionPublication(
            self.entries
                .iter()
                .map(|(key, line)| (*key, line.wake_pending))
                .collect(),
        )
    }
}

/// The snapshot is JSON, whose map keys must be strings: the `(domain, sequence)`-keyed entries are
/// written as a list of `(domain, sequence, line)` and read back into the map.
mod entry_list {
    use super::{PendingLine, SequenceDomain};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::BTreeMap;

    type Entries = BTreeMap<(SequenceDomain, u64), PendingLine>;

    pub(super) fn serialize<S: Serializer>(
        entries: &Entries,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let list: Vec<(SequenceDomain, u64, &PendingLine)> = entries
            .iter()
            .map(|((d, seq), line)| (*d, *seq, line))
            .collect();
        list.serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Entries, D::Error> {
        let list: Vec<(SequenceDomain, u64, PendingLine)> = Vec::deserialize(deserializer)?;
        Ok(list
            .into_iter()
            .map(|(d, seq, line)| ((d, seq), line))
            .collect())
    }
}
