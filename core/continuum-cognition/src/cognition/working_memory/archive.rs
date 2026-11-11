//! Her history on disk: the acts her view folded, after they left RAM.
//!
//! Her resident action log holds only what her view still shows unfolded (`ActionLog::drain_through`
//! at the view's measured fold edge). Each act that leaves is written ONCE here: its ledger line,
//! its summary, its handles and its `#n`, never its body (the body is a content-addressed file in
//! her tool-output directory, named by `body`). Before this every resident carried her whole life
//! in RAM and in every checkpoint (the M5, 2026-11-10: 4,522-5,686 records, 12-46 MB of JSON
//! rewritten whole every ~15 s per mind, 51-161 ms each).
//!
//! It is a log-structured store, and it DRAINS:
//! - **Segments.** One immutable file per drain (`<first>-<last>.acts.jsonl`), written by the
//!   checkpoint tick off the act path: the delta, never a rewrite of what is already there.
//! - **Compaction.** A segment whose newest act is older than her measured reopen horizon (the
//!   oldest age at which she has reopened an archived act, bucketed by powers of two as
//!   `continuum_context::rereads` buckets re-requests) collapses into one [`SpanSummary`] and its
//!   lines are deleted. One segment per tick, amortized; nothing is held between steps. Before any
//!   reopen is measured the horizon is unknown, a type, never a number: nothing compacts.
//! - **Retention.** The archive lives inside the `tool-output` class (`standard_tracked_dirs`), so
//!   its owner pool (`tool-output-spill`, `evict_unless_live`) deletes its oldest segments under
//!   disk pressure, a decided eviction (`eviction_decisions::OWNED`). The archive notices a
//!   segment gone at its oldest end, one stat per tick, and forgets it.
//! - **Bodies.** A body file is referenced by a resident record or an archived line. When the
//!   last archived line naming it is compacted or retired, the file is deleted through the spill
//!   owner's guard (`evict_unless_live`), which keeps any a live mind or her checkpoint names.
//!
//! Every path here pages in what it touches and lets it go: a reopen reads one segment for that
//! read; a compaction reads one segment and writes one span; the index in RAM is one row per
//! segment plus the handles its lines name.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::io::BufRead;
use std::path::{Path, PathBuf};

use continuum_kernel::system_resources::disk_reporters::ClassWriter;
use uuid::Uuid;

use super::action_log::ActionRecord;

/// Her archive's directory inside her tool-output directory, and its index file.
const DIR: &str = "archive";
const META: &str = "archive/meta.json";

/// One act as her archive keeps it: what she did and where its output is, never the output.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ArchivedAct {
    pub seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room: Option<Uuid>,
    pub operation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// The saved output a reopen of this act names (`tool/output` by `act`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
    /// Her rendered result's own saved copy, when it differs from `handle`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// The per-call outputs its receipt pointed at (`receipt_children`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<String>,
}

impl ArchivedAct {
    /// Every saved output this act references, each once.
    fn handles(&self) -> Vec<String> {
        let mut handles: Vec<String> = self.handle.iter().chain(&self.body).chain(&self.children).cloned().collect();
        handles.sort_unstable();
        handles.dedup();
        handles
    }
}

/// A span of her acts compacted to one record: the range, how many, and what she did in it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SpanSummary {
    pub first: u64,
    pub last: u64,
    pub acts: u64,
    /// Each operation and how many of the span's acts ran it, most first.
    pub operations: Vec<(String, u64)>,
}

impl SpanSummary {
    fn of(acts: &[ArchivedAct]) -> Option<Self> {
        let first = acts.first()?.seq;
        let last = acts.last()?.seq;
        let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
        for act in acts {
            *counts.entry(act.operation.as_str()).or_default() += 1;
        }
        let mut operations: Vec<(String, u64)> = counts.into_iter().map(|(op, n)| (op.to_owned(), n)).collect();
        operations.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        Some(Self { first, last, acts: acts.len() as u64, operations })
    }
}

impl std::fmt::Display for SpanSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "acts #{}-#{}: {} acts", self.first, self.last, self.acts)?;
        let ran: Vec<String> = self.operations.iter().map(|(op, n)| format!("{op} ×{n}")).collect();
        if !ran.is_empty() {
            write!(f, " ({})", ran.join(", "))?;
        }
        Ok(())
    }
}

/// Whether a segment still holds its acts' lines or only their span's summary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum Form {
    Lines,
    Span,
}

/// One file of her archive: the acts `first..=last`, and the handles its lines reference.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct Segment {
    first: u64,
    last: u64,
    form: Form,
    /// Each distinct handle its lines name, once per line; empty once compacted (a span names none).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    handles: Vec<String>,
}

impl Segment {
    fn name(&self) -> PathBuf {
        let suffix = match self.form {
            Form::Lines => "acts.jsonl",
            Form::Span => "span.json",
        };
        PathBuf::from(format!("{DIR}/{:020}-{:020}.{suffix}", self.first, self.last))
    }

    /// A segment named by a file in her archive directory (the index rebuilt when its file is gone).
    fn from_name(name: &str) -> Option<Self> {
        let (range, form) = match name.split_once('.')? {
            (range, "acts.jsonl") => (range, Form::Lines),
            (range, "span.json") => (range, Form::Span),
            _ => return None,
        };
        let (first, last) = range.split_once('-')?;
        Some(Self { first: first.parse().ok()?, last: last.parse().ok()?, form, handles: Vec::new() })
    }
}

/// Her archive's index, kept beside it: one row per segment, and her measured reopen ages.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
struct Meta {
    segments: VecDeque<Segment>,
    /// Reopens of an archived act per age bucket (`continuum_context::rereads::bucket`, in acts
    /// since it): the measurement her compaction horizon is read from.
    #[serde(default)]
    reopens: Vec<u64>,
}

/// An act that left RAM, waiting for the checkpoint tick to write it: its line, and its body
/// when no saved copy of exactly that body is named yet (moved out of the record, never copied).
#[derive(Debug)]
pub(super) struct Pending {
    act: ArchivedAct,
    unsaved_body: Option<String>,
}

/// Where her archive lives: her own directory of the tool-output class.
#[derive(Clone)]
pub(super) struct Bound {
    persona: Uuid,
    store: ClassWriter,
}

impl std::fmt::Debug for Bound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "archive of {}", self.persona)
    }
}

/// What one reopen found in her archive.
#[derive(Debug)]
pub(super) enum Found {
    Act(ArchivedAct),
    Compacted(SpanSummary),
    /// Retention deleted the segment that held it.
    Retired,
}

/// Her archive's state in RAM: bounded by its segment count and the handles its lines name.
#[derive(Debug, Default)]
pub(super) struct Archive {
    bound: Option<Bound>,
    meta: Meta,
    /// How many archived lines name each handle (derived from `meta.segments`).
    counts: HashMap<String, u32>,
    outbox: Vec<Pending>,
    dirty: bool,
}

impl Archive {
    /// Open `persona`'s archive in `store` (her tool-output directory): its index, or, when the
    /// index is gone (retention may take it), the segments its directory still holds.
    pub fn bind(persona: Uuid, store: ClassWriter) -> std::io::Result<Self> {
        let meta = match std::fs::read(store.path(Path::new(META))?) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::listed(&store)?,
            Err(error) => return Err(error),
        };
        let mut archive = Self { bound: Some(Bound { persona, store }), meta, ..Self::default() };
        for segment in &archive.meta.segments {
            for handle in &segment.handles {
                *archive.counts.entry(handle.clone()).or_default() += 1;
            }
        }
        Ok(archive)
    }

    /// The segments her archive directory holds, oldest first, with no handle lists (an index
    /// rebuilt from names alone references no bodies: their files drain by disk pressure).
    fn listed(store: &ClassWriter) -> std::io::Result<Meta> {
        let dir = store.path(Path::new(DIR))?;
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Meta::default()),
            Err(error) => return Err(error),
        };
        let mut segments: Vec<Segment> = Vec::new();
        for entry in entries {
            if let Some(segment) = entry?.file_name().to_str().and_then(Segment::from_name) {
                segments.push(segment);
            }
        }
        segments.sort_by_key(|segment| segment.first);
        Ok(Meta { segments: segments.into(), reopens: Vec::new() })
    }

    pub fn is_bound(&self) -> bool {
        self.bound.is_some()
    }

    /// Whether any act of hers lives here rather than in RAM.
    pub fn holds_acts(&self) -> bool {
        !self.meta.segments.is_empty() || !self.outbox.is_empty()
    }

    /// Queue `record` (it left RAM) for the next tick's segment, with its receipt's per-call outputs.
    pub fn queue(&mut self, record: ActionRecord, children: Vec<String>) {
        let ActionRecord { seq, room, operation, head, summary, result, handle, body, .. } = record;
        let unsaved_body = if body.is_none() { result } else { None };
        self.outbox.push(Pending { act: ArchivedAct { seq, room, operation, head, summary, handle, body, children }, unsaved_body });
    }

    /// An act still waiting in the outbox: answered from RAM, no read.
    pub fn pending(&self, seq: u64) -> Option<ArchivedAct> {
        self.outbox.iter().find(|pending| pending.act.seq == seq).map(|pending| pending.act.clone())
    }

    /// The segment whose range holds `seq`, and where to read it: O(log segments).
    pub fn locate(&self, seq: u64) -> Option<(Bound, Segment)> {
        let bound = self.bound.clone()?;
        let at = self.meta.segments.partition_point(|segment| segment.last < seq);
        let segment = self.meta.segments.get(at).filter(|segment| segment.first <= seq)?;
        Some((bound, segment.clone()))
    }

    /// She reopened an archived act `age` acts after it: one measurement of her reopen horizon.
    pub fn note_reopen(&mut self, age: u64) {
        let bucket = continuum_context::rereads::bucket(age);
        if self.meta.reopens.len() <= bucket {
            self.meta.reopens.resize(bucket + 1, 0);
        }
        self.meta.reopens[bucket] += 1;
        self.dirty = true;
    }

    /// The oldest age (in acts) at which she has reopened an archived act: the top of the oldest
    /// bucket that saw one. `None` before any reopen: unknown, so nothing compacts.
    pub fn reopen_horizon(&self) -> Option<u64> {
        let bucket = self.meta.reopens.iter().rposition(|&count| count > 0)?;
        Some((1u64 << (bucket + 1)) - 2)
    }

    fn take_outbox(&mut self) -> Option<(Bound, Vec<Pending>)> {
        let bound = self.bound.clone()?;
        (!self.outbox.is_empty()).then(|| (bound, std::mem::take(&mut self.outbox)))
    }

    fn requeue(&mut self, mut pending: Vec<Pending>) {
        pending.append(&mut self.outbox);
        self.outbox = pending;
    }

    fn admit(&mut self, segment: Segment) {
        for handle in &segment.handles {
            *self.counts.entry(handle.clone()).or_default() += 1;
        }
        self.meta.segments.push_back(segment);
        self.dirty = true;
    }

    /// Drop each of `handles`' references; the handles nothing archived names any more.
    fn release(&mut self, handles: &[String]) -> Vec<String> {
        let mut freed = Vec::new();
        for handle in handles {
            if let Some(count) = self.counts.get_mut(handle) {
                *count -= 1;
                if *count == 0 {
                    self.counts.remove(handle);
                    freed.push(handle.clone());
                }
            }
        }
        freed
    }

    /// The oldest segment, when retention deleted its file: forget it, and the handles it freed.
    fn forget_oldest(&mut self, segment: &Segment) -> Vec<String> {
        if self.meta.segments.front() != Some(segment) {
            return Vec::new();
        }
        self.meta.segments.pop_front();
        self.dirty = true;
        self.release(&segment.handles)
    }

    /// The oldest segment still in lines whose newest act is past her reopen horizon (`newest` is
    /// her newest act): what one compaction step takes.
    fn compactable(&self, newest: u64) -> Option<(Bound, Segment)> {
        let horizon = self.reopen_horizon()?;
        let bound = self.bound.clone()?;
        let segment = self.meta.segments.iter().find(|segment| segment.form == Form::Lines)?;
        (newest.saturating_sub(segment.last) > horizon).then(|| (bound, segment.clone()))
    }

    /// `segment` was compacted: it now names no handle; the handles that freed.
    fn compacted(&mut self, segment: &Segment) -> Vec<String> {
        let Some(row) = self.meta.segments.iter_mut().find(|row| **row == *segment) else {
            return Vec::new();
        };
        row.form = Form::Span;
        row.handles = Vec::new();
        self.dirty = true;
        self.release(&segment.handles)
    }

    /// The index to write, once, when it changed since the last write.
    fn dirty_meta(&mut self) -> std::io::Result<Option<(Bound, Vec<u8>)>> {
        let Some(bound) = self.bound.clone().filter(|_| self.dirty) else {
            return Ok(None);
        };
        let bytes = serde_json::to_vec(&self.meta).map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?; // file on disk: her archive's index, read back at her next boot
        self.dirty = false;
        Ok(Some((bound, bytes)))
    }

    #[cfg(test)]
    pub fn segments(&self) -> usize {
        self.meta.segments.len()
    }
}

/// What one archive tick did: the receipt its probe carries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArchiveTick {
    /// Acts written into a new segment, and that segment's bytes.
    pub archived: usize,
    pub bytes: usize,
    pub compacted: usize,
    pub retired: usize,
    /// Body files deleted because nothing references them any more.
    pub bodies_deleted: usize,
}

/// One archive tick for the archive behind `lock`: write the outbox as one segment, notice a
/// segment retention took, compact one span, delete the bodies that freed, write the index if it
/// changed. Each step holds the lock only to read or record what it did; every file is touched
/// outside it.
pub(super) fn tick(lock: &parking_lot::Mutex<Archive>, newest: u64) -> std::io::Result<ArchiveTick> {
    let mut receipt = ArchiveTick::default();
    let taken = lock.lock().take_outbox();
    if let Some((bound, pending)) = taken {
        match write_segment(&bound, pending) {
            Ok((segment, archived, bytes)) => {
                receipt.archived = archived;
                receipt.bytes = bytes;
                lock.lock().admit(segment);
            }
            Err((error, pending)) => {
                lock.lock().requeue(pending);
                return Err(error);
            }
        }
    }
    let oldest = lock.lock().locate_oldest();
    if let Some((bound, segment)) = oldest {
        if !bound.store.path(&segment.name())?.exists() {
            let freed = lock.lock().forget_oldest(&segment);
            receipt.retired = 1;
            receipt.bodies_deleted += delete_unreferenced(&bound, &freed);
        }
    }
    let compactable = lock.lock().compactable(newest);
    if let Some((bound, segment)) = compactable {
        compact(&bound, &segment)?;
        let freed = lock.lock().compacted(&segment);
        receipt.compacted = 1;
        receipt.bodies_deleted += delete_unreferenced(&bound, &freed);
    }
    let meta = lock.lock().dirty_meta()?;
    if let Some((bound, bytes)) = meta {
        bound.store.replace(Path::new(META), &bytes)?;
    }
    Ok(receipt)
}

impl Archive {
    fn locate_oldest(&self) -> Option<(Bound, Segment)> {
        Some((self.bound.clone()?, self.meta.segments.front()?.clone()))
    }
}

/// Write `pending` as one segment: each unsaved body saved under its own content address first
/// (so the line names where it is), then every line, in one file. The delta only: no line already
/// archived is written again. On failure the acts are handed back to be queued again.
fn write_segment(bound: &Bound, pending: Vec<Pending>) -> Result<(Segment, usize, usize), (std::io::Error, Vec<Pending>)> {
    let pin = crate::cognition::tool_executor::spill::LivePin::new();
    let mut acts: Vec<ArchivedAct> = Vec::with_capacity(pending.len());
    for Pending { mut act, unsaved_body } in pending {
        if let Some(text) = unsaved_body {
            let address = continuum_context::cause::ContentHash::of(&text).address();
            act.body = if act.handle.as_deref() == Some(address.as_str()) {
                act.handle.clone()
            } else {
                pin.retain(bound.persona, &text).handle().map(str::to_owned)
            };
        }
        acts.push(act);
    }
    let mut lines = Vec::new();
    let mut handles = Vec::new();
    let mut failed = None;
    for act in &acts {
        match serde_json::to_vec(act) { // file on disk: one archived act per line of its segment
            Ok(line) => lines.extend(line),
            Err(error) => {
                failed = Some(error);
                break;
            }
        }
        lines.push(b'\n');
        handles.extend(act.handles());
    }
    if let Some(error) = failed {
        return Err((std::io::Error::new(std::io::ErrorKind::InvalidData, error), requeued(acts)));
    }
    let (Some(first), Some(last)) = (acts.first().map(|act| act.seq), acts.last().map(|act| act.seq)) else {
        return Err((std::io::Error::new(std::io::ErrorKind::InvalidInput, "an archive segment needs at least one act"), Vec::new()));
    };
    let segment = Segment { first, last, form: Form::Lines, handles };
    if let Err(error) = bound.store.replace(&segment.name(), &lines) {
        return Err((error, requeued(acts)));
    }
    drop(pin);
    Ok((segment, acts.len(), lines.len()))
}

/// Acts handed back after a failed write: their bodies are saved now, so none is pending.
fn requeued(acts: Vec<ArchivedAct>) -> Vec<Pending> {
    acts.into_iter().map(|act| Pending { act, unsaved_body: None }).collect()
}

/// Read `segment`'s acts, one line each, released as the read ends.
fn read_lines(bound: &Bound, segment: &Segment) -> std::io::Result<Vec<ArchivedAct>> {
    let file = std::fs::File::open(bound.store.path(&segment.name())?)?;
    let mut acts = Vec::new();
    for line in std::io::BufReader::new(file).lines() {
        let line = line?;
        if line.is_empty() {
            continue;
        }
        acts.push(serde_json::from_str(&line).map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?);
    }
    Ok(acts)
}

/// Collapse `segment` into its span's summary: the span file written, then the lines deleted.
fn compact(bound: &Bound, segment: &Segment) -> std::io::Result<()> {
    let acts = read_lines(bound, segment)?;
    let span = SpanSummary::of(&acts).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "an archive segment holds no act"))?;
    let bytes = serde_json::to_vec(&span).map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?; // file on disk: the span record a compacted act reopens as
    let spanned = Segment { form: Form::Span, handles: Vec::new(), ..segment.clone() };
    bound.store.replace(&spanned.name(), &bytes)?;
    match std::fs::remove_file(bound.store.path(&segment.name())?) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Delete each saved output in `handles` that no live mind and no checkpoint names (the spill
/// owner's guard decides, under its lock); how many were deleted.
fn delete_unreferenced(bound: &Bound, handles: &[String]) -> usize {
    use crate::cognition::tool_executor::spill;
    use crate::paging::filesystem_tier_pool::Removal;
    let mut deleted = 0;
    for handle in handles {
        let removal = match spill::resolve(bound.persona, handle) {
            Ok(path) if !path.exists() => continue,
            Ok(path) => spill::evict_unless_live(&path),
            Err(error) => Removal::Failed(error),
        };
        match removal {
            Removal::Removed => deleted += 1,
            Removal::Protected => {}
            Removal::Failed(error) => continuum_base::probe!(
                class = "wm.history.body_kept",
                persona_id = %bound.persona,
                handle = %handle,
                error = %error,
                "an output no archived act names could not be deleted; disk pressure retires it"
            ),
        }
    }
    deleted
}

/// Reopen act `seq` from `segment`: its line, or its span's summary once compacted, or `Retired`
/// when retention took the file. Pages in one segment for this read and lets it go.
pub(super) fn find(bound: &Bound, segment: &Segment, seq: u64) -> std::io::Result<Found> {
    let missing = |error: &std::io::Error| error.kind() == std::io::ErrorKind::NotFound;
    match segment.form {
        Form::Lines => match read_lines(bound, segment) {
            Ok(acts) => Ok(acts.into_iter().find(|act| act.seq == seq).map_or(Found::Retired, Found::Act)), // map_or: a range that lost its line is retired
            Err(error) if missing(&error) => Ok(Found::Retired),
            Err(error) => Err(error),
        },
        Form::Span => match std::fs::read(bound.store.path(&segment.name())?) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Found::Compacted)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
            Err(error) if missing(&error) => Ok(Found::Retired),
            Err(error) => Err(error),
        },
    }
}
