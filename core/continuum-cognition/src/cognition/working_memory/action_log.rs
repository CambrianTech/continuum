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

/// The act a reopen names (`tool/output`): one of her act numbers, or, for a call naming no
/// output at all, her newest act with a saved output (the one she most likely means).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActRef {
    Numbered(u64),
    NewestSaved,
}

impl std::fmt::Display for ActRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActRef::Numbered(act) => write!(f, "act #{act}"),
            ActRef::NewestSaved => f.write_str("your newest saved act"),
        }
    }
}

/// Why a reopen does not reach a saved output (`tool/output`): a typed refusal naming what she
/// named, never a fallback to some other output.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ActOutputRefusal {
    /// No record of hers carries this number. `newest_saved` is her newest act with a saved
    /// output, the one a reopen can reach (`None` when none is saved).
    #[error("act #{act} is not on your record; use a #n your view names")]
    Unknown { act: u64, newest_saved: Option<u64> },
    /// The act is on record but its save failed: what her view showed of it is all there is.
    #[error("act #{act} has no saved output: its save failed, so what your view showed of it is all there is")]
    Unsaved { act: u64 },
    /// A call naming no output, and none of her acts has a saved one yet.
    #[error("none of your acts has a saved output yet, so there is nothing to reopen")]
    NothingSaved,
    /// She named an output and an act, and they are not the same output: two readings, so the
    /// call is hers to make again with one. `act_output` is the act's own saved output, if any.
    #[error("outputId {output} and act #{act} name different outputs; name one")]
    Disagree { act: u64, output: String, act_output: Option<String> },
}

/// Hers to read and act on: what she named reopens nothing, or names two things.
impl crate::sdk_codegen::RefusalCause for ActOutputRefusal {
    fn category(&self) -> crate::sdk_codegen::Category {
        match self {
            ActOutputRefusal::Disagree { .. } => crate::sdk_codegen::Category::Invalid,
            ActOutputRefusal::Unknown { .. } | ActOutputRefusal::Unsaved { .. } | ActOutputRefusal::NothingSaved => {
                crate::sdk_codegen::Category::NotFound
            }
        }
    }
}

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
    /// What the act asked for and what its commands declare they do (MIND-CONTEXT invariant 4):
    /// the view renders an unchanged observation as a repeat from this. `None` for a record from
    /// a build before it was kept, or a legacy text receipt: such an act is never a repeat.
    /// Derived: one that no longer decodes is dropped (`derived::or_none`), so that act is never a
    /// repeat, and the checkpoint still restores.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "super::derived::or_none")]
    pub act: Option<continuum_context::cause::Act>,
}

impl ActionRecord {
    /// The key its per-call dependencies are filed under: its saved handle, or `seq:N` for
    /// a result whose save failed.
    pub fn receipt_key(&self) -> String {
        super::receipt_key(self.seq, self.handle.as_deref())
    }

    /// The RAM this record holds, in bytes: every text it owns (its ledger line, its summary and
    /// its body while resident), so a line or summary that outlives a shed stays in the memory
    /// owner's account as pinned charge (Codex on #5637). Never a character count (that is
    /// `stored_chars`, the view's price).
    pub fn resident_bytes(&self) -> u64 {
        [&self.head, &self.summary, &self.result]
            .into_iter()
            .map(|text| text.as_ref().map_or(0, |text| text.len() as u64))
            .sum()
    }

}

/// A shed moves its result body to its handle (`residency`); the line and the summary stay with
/// the record for as long as her mind lives.
impl super::residency::Resident for ActionRecord {
    fn id(&self) -> super::residency::RecordId {
        super::residency::RecordId::Act(self.seq)
    }

    fn body(&self) -> Option<&str> {
        self.result.as_deref()
    }

    fn handle(&self) -> Option<&str> {
        self.handle.as_deref()
    }

    fn received_ms(&self) -> Option<u64> {
        self.received_ms
    }

    fn resident_bytes(&self) -> u64 {
        ActionRecord::resident_bytes(self)
    }

    fn shed_body(&mut self) {
        self.stored_chars = self.result.as_ref().map(|result| result.chars().count());
        self.result = None;
    }
}

#[derive(Debug, Default)]
pub(super) struct ActionLog {
    /// Ascending by `seq`.
    records: VecDeque<ActionRecord>,
}

impl ActionLog {
    /// A log from records in any order. Nothing here bounds it: a record keeps its line and its
    /// handle for as long as her mind lives, and its body's RAM is the one memory owner's to
    /// shed (`residency`), never a character cap.
    pub fn from_records(mut records: Vec<ActionRecord>) -> Self {
        records.sort_by_key(|record| record.seq);
        records.dedup_by_key(|record| record.seq);
        Self { records: records.into() }
    }

    pub fn push(&mut self, record: ActionRecord) {
        self.records.push_back(record);
    }

    pub fn get(&self, seq: u64) -> Option<&ActionRecord> {
        self.records.binary_search_by_key(&seq, |record| record.seq).ok().map(|index| &self.records[index])
    }

    pub fn get_mut(&mut self, seq: u64) -> Option<&mut ActionRecord> {
        let index = self.records.binary_search_by_key(&seq, |record| record.seq).ok()?;
        self.records.get_mut(index)
    }

    /// Every record, oldest first.
    pub fn records(&self) -> impl DoubleEndedIterator<Item = &ActionRecord> + Clone {
        self.records.iter()
    }

    /// Her acts for `room` since `since_ms`, after her latest one there that changed the world by
    /// its commands' declared effect (`ToolEffect::Changes`: an edit, a write, a note, a claim;
    /// never inferred from a name): the write-less stretch an advisory checkpoint reads. An act from
    /// a build before receipt times were kept counts as before any hold, and one with no declared
    /// effect is an act that changed nothing it declared.
    pub fn acts_since_change(&self, room: Uuid, since_ms: u64) -> usize {
        self.records
            .iter()
            .filter(|record| record.room == Some(room) && record.received_ms.is_some_and(|at| at >= since_ms))
            .fold(0, |acts, record| {
                let changed = record.act.as_ref().is_some_and(|act| act.effect == continuum_base::tool_effect::ToolEffect::Changes);
                if changed { 0 } else { acts + 1 }
            })
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

    /// The RAM every record holds, in bytes: bodies, lines and summaries.
    pub fn resident_bytes(&self) -> u64 {
        self.records.iter().map(ActionRecord::resident_bytes).sum()
    }
}
