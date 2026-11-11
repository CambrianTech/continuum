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
    /// The act left RAM for her archive and its span was compacted past her reopen horizon: its
    /// own line and output are gone, and the span's summary is what remains. The answer, typed.
    #[error("act #{act} was compacted into its span, {span}: its own line and output are no longer kept, and this summary is what remains")]
    Compacted { act: u64, span: super::archive::SpanSummary },
    /// The act's archive segment was deleted by disk-pressure retention.
    #[error("act #{act} was archived and its span has since been deleted under disk pressure: nothing of it remains to reopen")]
    Retired { act: u64 },
    /// The act's archive segment is there and cannot be read.
    #[error("act #{act} is archived but its segment could not be read: {error}")]
    Unreadable { act: u64, error: String },
}

/// Hers to read and act on: what she named reopens nothing, or names two things.
impl crate::sdk_codegen::RefusalCause for ActOutputRefusal {
    fn category(&self) -> crate::sdk_codegen::Category {
        match self {
            ActOutputRefusal::Disagree { .. } => crate::sdk_codegen::Category::Invalid,
            ActOutputRefusal::Unknown { .. }
            | ActOutputRefusal::Unsaved { .. }
            | ActOutputRefusal::NothingSaved
            | ActOutputRefusal::Compacted { .. }
            | ActOutputRefusal::Retired { .. }
            | ActOutputRefusal::Unreadable { .. } => crate::sdk_codegen::Category::NotFound,
        }
    }
}

/// Her newest act's run of identical requests with identical outcomes ([`ActionLog::repeat_run`]):
/// a fact about her hands that moves with every repeat, so her newest input never equals her last
/// (Cormac on the IntelMac loops, 2026-11-11: a grep 6 times, a missing file read 32 times, then
/// one saved output reopened 20+ times). Rendered into her newest result only, never stored, so a
/// stored outcome keeps its content identity. Never a directive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RepeatRun {
    pub times: usize,
}

impl std::fmt::Display for RepeatRun {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[the same request {} times in a row; its output byte-identical each time]", self.times)
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
    /// Where exactly this record's resident body is saved, under the body's own content address:
    /// what a shed moves it to and what her view loads back. Usually `handle` itself; for an act
    /// that reopened a saved output, `handle` names that SOURCE (raw) while the body is her rendered
    /// result, so the two never matched and every such body stayed resident as "the last copy"
    /// (the M5, 2026-11-10: 480 `wm.results.last_copy_kept` mismatches in 90 min for one resident).
    /// Set by the memory owner at shed time (`save_unsaved_bodies`), never on the act path. `None`
    /// until then, and for a record from a build before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
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
    /// The whole result's hash, taken as its body leaves RAM and kept with its handle, so an
    /// evicted observation is still the occurrence a re-read is matched against
    /// (`continuum_context::rereads`). Without it an evicted read hashed as `Absent`, no re-read
    /// ever matched, and her view measured zero re-reads while Atlas re-read an identical file
    /// 103 times in 300 acts (the M5, 2026-11-09): collapse looked free and her reads thrashed.
    /// `None` while resident (the body is hashed directly) and for records evicted before this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stored_content: Option<continuum_context::cause::ContentHash>,
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
    /// How many of her acts in its room, ending with this one, made this exact request and got a
    /// byte-identical outcome: taken as it is pushed, from the room's last act, so the run is a fact
    /// of the newest record and survives her view folding the earlier ones out of RAM (IntelMac
    /// 0051cdcf, 2026-11-11: a 3rd identical read after a turnover was told no run). `None` for an
    /// act with no identity or outcome, and for a record from a build before it was kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<u32>,
}

impl ActionRecord {
    /// What the act returned, as its hash: of the resident result, or kept from when it left RAM.
    fn outcome(&self) -> Option<continuum_context::cause::ContentHash> {
        self.result.as_deref().map(continuum_context::cause::ContentHash::of).or(self.stored_content)
    }

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

/// A shed moves its result body to its saved copy (`residency`); the line and the summary stay with
/// the record while her view shows it unfolded, then leave RAM for her archive (`archive`).
impl super::residency::Resident for ActionRecord {
    fn id(&self) -> super::residency::RecordId {
        super::residency::RecordId::Act(self.seq)
    }

    fn body(&self) -> Option<&str> {
        self.result.as_deref()
    }

    /// Its body's own saved copy once the memory owner named it, else its receipt's handle.
    fn handle(&self) -> Option<&str> {
        self.body.as_deref().or(self.handle.as_deref())
    }

    fn received_ms(&self) -> Option<u64> {
        self.received_ms
    }

    fn resident_bytes(&self) -> u64 {
        ActionRecord::resident_bytes(self)
    }

    fn shed_body(&mut self) {
        self.stored_chars = self.result.as_ref().map(|result| result.chars().count());
        self.stored_content = self.result.as_deref().map(continuum_context::cause::ContentHash::of);
        self.result = None;
    }
}

/// Her resident acts: a ring keyed by `seq`, pushed at the newest end, drained at the oldest by
/// her view's measured fold edge (`drain_through`, `WorkingMemory::release_folded`). A record her
/// view still shows unfolded stays; one it folded leaves RAM for her archive. Its bodies' RAM is
/// the one memory owner's to shed (`residency`), never a character cap.
#[derive(Debug, Default)]
pub(super) struct ActionLog {
    /// Ascending by `seq`: `seq` is minted monotonically, so a push keeps the order and a reopen
    /// finds its record by binary search.
    records: VecDeque<ActionRecord>,
    /// Each room's last act, its outcome and its run: what the next act's run is taken from. Never
    /// drained with the records (one entry per room she acts in), so a run spans a fold.
    last: std::collections::HashMap<Option<Uuid>, Last>,
}

/// A room's last act as a run's continuation reads it.
#[derive(Debug, Clone, Copy)]
struct Last {
    act: continuum_context::cause::Act,
    outcome: continuum_context::cause::ContentHash,
    run: u32,
}

impl ActionLog {
    /// A log from records as a checkpoint carries them: already ascending (they were written
    /// from this ring), so this is one O(n) check; only a checkpoint from before the ring is
    /// sorted.
    pub fn from_records(mut records: Vec<ActionRecord>) -> Self {
        if !records.windows(2).all(|pair| pair[0].seq < pair[1].seq) {
            records.sort_by_key(|record| record.seq);
            records.dedup_by_key(|record| record.seq);
        }
        let mut log = Self { records: VecDeque::with_capacity(records.len()), last: Default::default() };
        records.into_iter().for_each(|record| log.push(record));
        log
    }

    /// Every record up to and including `edge`, oldest first, out of the ring: O(drained). What her
    /// view folded at its start leaves RAM here, never by a count.
    pub fn drain_through(&mut self, edge: u64) -> Vec<ActionRecord> {
        let keep = self.records.partition_point(|record| record.seq <= edge);
        self.records.drain(..keep).collect()
    }

    /// How many of her acts are resident.
    pub fn resident(&self) -> usize {
        self.records.len()
    }

    /// Append her newest act, taking its run from the room's last act (a record restored with its
    /// run keeps it).
    pub fn push(&mut self, mut record: ActionRecord) {
        match (record.act, record.outcome()) {
            (Some(act), Some(outcome)) => {
                let run = match self.last.get(&record.room) {
                    Some(last) if last.act == act && last.outcome == outcome => last.run + 1,
                    _ => 1,
                };
                let run = *record.run.get_or_insert(run);
                self.last.insert(record.room, Last { act, outcome, run });
            }
            _ => {
                self.last.remove(&record.room);
            }
        }
        self.records.push_back(record);
    }

    pub fn get(&self, seq: u64) -> Option<&ActionRecord> {
        self.records.binary_search_by_key(&seq, |record| record.seq).ok().map(|index| &self.records[index])
    }

    pub fn get_mut(&mut self, seq: u64) -> Option<&mut ActionRecord> {
        let index = self.records.binary_search_by_key(&seq, |record| record.seq).ok()?;
        self.records.get_mut(index)
    }

    /// Her act `seq`'s run: how many of her acts in its room, ending with it, made this exact
    /// request (`cause::Act`: call and declared effect, undeclared included) and got a
    /// byte-identical outcome. `None` for a run of one.
    pub fn repeat_run(&self, seq: u64) -> Option<RepeatRun> {
        let times = self.get(seq)?.run?;
        (times > 1).then_some(RepeatRun { times: times as usize })
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

    /// Every receipt whose dependencies must stay reachable while its record is resident: each
    /// retained result's key, every handle a record names, and each body's own saved copy. Bounded
    /// by the ring, which her view's fold edge drains: a record that left for her archive is
    /// counted by the archive's references instead (`archive::Archive`), so this never keeps
    /// every handle she ever made live.
    pub fn receipt_keys(&self) -> HashSet<String> {
        let mut keys = HashSet::new();
        for record in &self.records {
            if record.result.is_some() || record.handle.is_some() {
                keys.insert(record.receipt_key());
            }
            if let Some(body) = &record.body {
                keys.insert(body.clone());
            }
        }
        keys
    }

    /// The RAM every record holds, in bytes: bodies, lines and summaries.
    pub fn resident_bytes(&self) -> u64 {
        self.records.iter().map(ActionRecord::resident_bytes).sum()
    }
}
