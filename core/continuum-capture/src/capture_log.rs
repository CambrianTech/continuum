//! The capture's binary log (card 699578cb, slice 2): frames appended to a generation file, each found
//! through a fixed-width binary index entry. One owner's log is a sequence of generations; a writer
//! opens the next one and only appends to it, so nothing is ever rewritten in place.
//!
//! ```text
//! <owner>/<generation>.frames   frame, frame, ...                    (frame::encode)
//! <owner>/<generation>.index    [u64 offset][u32 length][u8 kind][3 pad][u64 captured_at_ms][u32 write_us] ...
//! ```
//!
//! Files are placed through the owner's disk class (`ClassWriter`), never at a path the log picks. The
//! frame is written before its index entry: a crash between them leaves an unindexed tail no reader
//! takes for history, and a frame cut off mid-write decodes as a short tail. Retention is the class's
//! pool's (oldest closed generation first), never a size this log decides.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Arc;

use continuum_kernel::system_resources::disk_reporters::ClassWriter;
use serde::Serialize;
use uuid::Uuid;

use crate::frame::{self, FrameError, FrameKind};

/// The bytes one index entry takes.
const ENTRY: usize = 8 + 4 + 1 + 3 + 8 + 4;

/// Where one frame is: its generation and its place in that generation's index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cursor {
    pub generation: u64,
    pub entry: u32,
}

/// One frame's index entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub offset: u64,
    pub length: u32,
    pub kind: u8,
    pub captured_at_ms: u64,
    /// How long writing this frame took, in microseconds: what sizes the writer's intake window.
    pub write_us: u32,
}

impl Entry {
    fn bytes(&self) -> [u8; ENTRY] {
        let mut out = [0u8; ENTRY];
        out[0..8].copy_from_slice(&self.offset.to_le_bytes());
        out[8..12].copy_from_slice(&self.length.to_le_bytes());
        out[12] = self.kind;
        out[16..24].copy_from_slice(&self.captured_at_ms.to_le_bytes());
        out[24..28].copy_from_slice(&self.write_us.to_le_bytes());
        out
    }

    fn of(raw: &[u8; ENTRY]) -> Self {
        let field = |range: std::ops::Range<usize>| -> [u8; 8] {
            let mut out = [0u8; 8];
            out[..range.len()].copy_from_slice(&raw[range]);
            out
        };
        Self {
            offset: u64::from_le_bytes(field(0..8)),
            length: u32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]),
            kind: raw[12],
            captured_at_ms: u64::from_le_bytes(field(16..24)),
            write_us: u32::from_le_bytes([raw[24], raw[25], raw[26], raw[27]]),
        }
    }
}

/// Why a log could not be written or read.
#[derive(Debug, thiserror::Error)]
pub enum LogError {
    #[error("capture log io: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Frame(#[from] FrameError),
    #[error("a frame of {0} bytes does not fit the index's 32-bit length")]
    TooLong(usize),
    #[error("this generation was damaged by an earlier failed append: its offsets no longer match its files, so it takes no more frames")]
    Damaged,
}

fn frames_of(generation: u64) -> PathBuf {
    PathBuf::from(format!("{generation:016}.frames"))
}

fn index_of(generation: u64) -> PathBuf {
    PathBuf::from(format!("{generation:016}.index"))
}

/// Every generation the owner's class holds, oldest first (a name that is not a generation is skipped).
pub fn generations(writer: &ClassWriter) -> io::Result<Vec<u64>> {
    let mut found: Vec<u64> = writer
        .files()?
        .iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "index"))
        .filter_map(|path| path.file_stem()?.to_str()?.parse().ok())
        .collect();
    found.sort_unstable();
    found.dedup();
    Ok(found)
}

/// The generations open logs are appending to, by their frames file: the pool never evicts one.
fn live() -> &'static std::sync::Mutex<std::collections::HashSet<std::sync::Arc<std::path::Path>>> {
    static LIVE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<std::sync::Arc<std::path::Path>>>> = std::sync::OnceLock::new();
    LIVE.get_or_init(Default::default)
}

/// Whether an open log is appending to the generation whose frames file is `frames`.
pub fn is_live(frames: &std::path::Path) -> bool {
    live().lock().is_ok_and(|held| held.contains(frames))
}

/// One owner's log, open on a fresh generation for appending.
pub struct CaptureLog {
    writer: ClassWriter,
    generation: u64,
    frames: File,
    index: File,
    offset: u64,
    entries: u32,
    scratch: Vec<u8>,
    /// This log's frames file, held in the live set until the log drops.
    held: std::sync::Arc<std::path::Path>,
    /// An append failed after it began writing: the files may hold bytes the offsets do not count,
    /// so every later frame would be indexed at the wrong place. A damaged log takes nothing more;
    /// its owner opens the next generation (Codex on #6236).
    damaged: bool,
}

#[cfg(test)]
thread_local! {
    /// Fail this thread's next index append, once (a full disk, injected; never another test's thread).
    pub(crate) static FAIL_NEXT_INDEX: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

impl Drop for CaptureLog {
    fn drop(&mut self) {
        if let Ok(mut held) = live().lock() {
            held.remove(&self.held);
        }
    }
}

impl CaptureLog {
    /// Open the next generation after the newest the owner's class holds.
    pub fn open(writer: ClassWriter) -> io::Result<Self> {
        let generation = match generations(&writer)?.last() {
            Some(newest) => newest + 1,
            None => 0, // an owner with no log yet: its first generation
        };
        let frames = writer.append(&frames_of(generation))?;
        let index = writer.append(&index_of(generation))?;
        let held: std::sync::Arc<std::path::Path> = writer.path(&frames_of(generation))?.into();
        live().lock().map_err(|_| io::Error::other("the live capture set was poisoned"))?.insert(std::sync::Arc::clone(&held));
        Ok(Self { writer, generation, frames, index, offset: 0, entries: 0, scratch: Vec::new(), held, damaged: false })
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Append one frame and its index entry; the frame first, so a crash leaves only an unindexed tail.
    pub fn append(&mut self, kind: FrameKind, header: &impl Serialize, runs: &[&[u8]], captured_at_ms: u64) -> Result<Cursor, LogError> {
        let header = rmp_serde::to_vec(header).map_err(FrameError::from)?;
        self.append_parts(frame::Parts { kind, header: &header, runs }, captured_at_ms)
    }

    /// [`Self::append`] for parts whose header their producer already encoded.
    pub fn append_parts(&mut self, parts: frame::Parts<'_>, captured_at_ms: u64) -> Result<Cursor, LogError> {
        let kind = parts.kind;
        self.scratch.clear();
        frame::encode_parts(&mut self.scratch, parts)?;
        let length = u32::try_from(self.scratch.len()).map_err(|_| LogError::TooLong(self.scratch.len()))?;
        if self.damaged {
            return Err(LogError::Damaged);
        }
        let started = std::time::Instant::now();
        if let Err(error) = self.frames.write_all(&self.scratch).and_then(|()| self.frames.flush()) {
            self.damaged = true;
            return Err(error.into());
        }
        let write_us = started.elapsed().as_micros().min(u128::from(u32::MAX)) as u32; // a write over ~71 minutes saturates the field
        let entry = Entry { offset: self.offset, length, kind: kind as u8, captured_at_ms, write_us };
        if let Err(error) = self.write_index(&entry) {
            self.damaged = true;
            return Err(error.into());
        }
        self.offset += u64::from(length);
        let cursor = Cursor { generation: self.generation, entry: self.entries };
        self.entries += 1;
        Ok(cursor)
    }

    /// Append `entry` to the index, the one place an injected failure stands in for a full disk in a test.
    fn write_index(&mut self, entry: &Entry) -> io::Result<()> {
        #[cfg(test)]
        if FAIL_NEXT_INDEX.with(|fail| fail.replace(false)) {
            return Err(io::Error::new(io::ErrorKind::StorageFull, "test: the index write failed as a full disk would"));
        }
        self.index.write_all(&entry.bytes())?;
        self.index.flush()
    }

    /// The class writer this log appends through (the readers read the same owner's files).
    pub fn writer(&self) -> &ClassWriter {
        &self.writer
    }
}

/// A generation's index entries, in order; a trailing partial entry (a crash mid-write) is not one.
pub fn entries(writer: &ClassWriter, generation: u64) -> io::Result<Vec<Entry>> {
    let mut raw = Vec::new();
    File::open(writer.path(&index_of(generation))?)?.read_to_end(&mut raw)?;
    let (whole, _partial) = raw.as_chunks::<ENTRY>();
    Ok(whole.iter().map(Entry::of).collect())
}

/// A generation's index entries from `from` on: what a reader that already holds the first `from`
/// reads next (an index only appends).
pub fn entries_from(writer: &ClassWriter, generation: u64, from: u32) -> io::Result<Vec<Entry>> {
    let mut file = File::open(writer.path(&index_of(generation))?)?;
    file.seek(SeekFrom::Start(u64::from(from) * ENTRY as u64))?;
    let mut raw = Vec::new();
    file.read_to_end(&mut raw)?;
    let (whole, _partial) = raw.as_chunks::<ENTRY>();
    Ok(whole.iter().map(Entry::of).collect())
}

/// The kind and header bytes of the frame `entry` locates, read without its runs.
pub fn frame_head(writer: &ClassWriter, generation: u64, entry: &Entry) -> Result<(FrameKind, Vec<u8>), LogError> {
    let mut file = File::open(writer.path(&frames_of(generation))?)?;
    file.seek(SeekFrom::Start(entry.offset))?;
    let mut fixed = [0u8; frame::HEAD_PREFIX];
    file.read_exact(&mut fixed)?;
    let (kind, header_len) = frame::head_prefix(&fixed)?;
    let mut header = vec![0u8; header_len];
    file.read_exact(&mut header)?;
    Ok((kind, header))
}

/// The bytes of the frame `entry` locates in `generation`.
pub fn frame_at(writer: &ClassWriter, generation: u64, entry: &Entry) -> io::Result<Vec<u8>> {
    let mut file = File::open(writer.path(&frames_of(generation))?)?;
    file.seek(SeekFrom::Start(entry.offset))?;
    let mut bytes = vec![0u8; entry.length as usize];
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}

/// One record to capture: its owner, its kind, its header already encoded (`frame` keeps it as given)
/// and its bulk as shared byte runs (handed over, never copied on the turn path).
#[derive(Debug, Clone)]
pub struct CaptureRecord {
    pub owner: Uuid,
    pub kind: FrameKind,
    pub header: Vec<u8>,
    pub runs: Vec<Arc<[u8]>>,
    pub captured_at_ms: u64,
}

/// Every registered open-log set, by weak handle (`OpenLogs::shared`).
fn registry() -> &'static parking_lot::Mutex<Vec<std::sync::Weak<OpenLogs>>> {
    static REGISTRY: std::sync::OnceLock<parking_lot::Mutex<Vec<std::sync::Weak<OpenLogs>>>> = std::sync::OnceLock::new();
    REGISTRY.get_or_init(Default::default)
}

/// Roll every registered open-log set (forgetting those gone): under disk pressure the generations a
/// running writer holds open become evictable, so a long-running node can reclaim its own history
/// (Codex on #6236). Returns the logs closed.
pub fn roll_open() -> usize {
    let mut registered = registry().lock();
    registered.retain(|logs| logs.strong_count() > 0);
    registered.iter().filter_map(std::sync::Weak::upgrade).map(|logs| logs.roll()).sum()
}

/// Each owner's open log, opened on its first record: the one place records are appended, shared
/// by the node's writer and the one-time migration so they never open competing generations.
#[derive(Default)]
pub struct OpenLogs(parking_lot::Mutex<std::collections::HashMap<uuid::Uuid, CaptureLog>>);

impl OpenLogs {
    /// An open-log set the captures pool can roll under disk pressure (every long-lived set is one:
    /// the node writer's, an inline delivery's). A set only a test holds may stay unregistered.
    pub fn shared() -> std::sync::Arc<Self> {
        let logs = std::sync::Arc::new(Self::default());
        registry().lock().push(std::sync::Arc::downgrade(&logs));
        logs
    }

    /// Close every open log, so each generation it held is closed and evictable; each owner's next
    /// record opens its next generation. Taken under the same lock as an append, so a roll never
    /// splits one. Returns how many logs it closed.
    pub fn roll(&self) -> usize {
        let mut logs = self.0.lock();
        let closed = logs.len();
        logs.clear();
        closed
    }

    /// Append `record` to its owner's open log in `class`, opening the owner's next generation first.
    pub fn append(&self, class: &ClassWriter, record: &CaptureRecord) -> io::Result<()> {
        let mut logs = self.0.lock();
        let log = match logs.entry(record.owner) {
            std::collections::hash_map::Entry::Occupied(open) => open.into_mut(),
            std::collections::hash_map::Entry::Vacant(slot) => slot.insert(CaptureLog::open(class.owned_by(record.owner))?),
        };
        let runs: Vec<&[u8]> = record.runs.iter().map(AsRef::as_ref).collect();
        let appended = log.append_parts(frame::Parts { kind: record.kind, header: &record.header, runs: &runs }, record.captured_at_ms);
        if appended.is_err() {
            // a failed append damages its generation: retire it, and the owner's next record opens the next
            logs.remove(&record.owner);
        }
        appended.map(|_| ()).map_err(io::Error::other)
    }
}

/// Every indexed frame of every generation, oldest first, handed to `each` with its cursor.
pub fn scan(writer: &ClassWriter, mut each: impl FnMut(Cursor, frame::FrameRef<'_>)) -> Result<(), LogError> {
    for generation in generations(writer)? {
        let mut all = Vec::new();
        File::open(writer.path(&frames_of(generation))?)?.read_to_end(&mut all)?;
        for (i, entry) in entries(writer, generation)?.iter().enumerate() {
            let start = entry.offset as usize;
            let end = start + entry.length as usize;
            let Some(bytes) = all.get(start..end) else { break }; // an indexed frame past the file's end: a torn generation stops here
            let (frame, _) = frame::decode(bytes)?;
            each(Cursor { generation, entry: i as u32 }, frame);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_kernel::system_resources::disk_reporters::TrackedDir;

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Head {
        request_id: String,
    }

    fn writer(dir: &std::path::Path) -> ClassWriter {
        TrackedDir::new("captures-test", dir.to_path_buf()).writer().owned_by(uuid::Uuid::nil())
    }

    // what this catches (Codex on #6236): a frame written whose index append then failed (a full
    // disk) leaving the log's offset behind its file, so the next frame's index entry points at the
    // orphaned bytes and reads back as the frame that failed; and an owner that cannot record again
    // once space returns.
    #[test]
    fn an_append_that_fails_midway_retires_its_generation_and_the_next_frame_reads_back_as_itself() {
        let dir = tempfile::tempdir().expect("test: dir");
        let class = TrackedDir::new("captures-test", dir.path().to_path_buf()).writer();
        let owner = uuid::Uuid::new_v4();
        let logs = OpenLogs::default();
        let record = |kind| CaptureRecord { owner, kind, header: vec![0x90], runs: vec![std::sync::Arc::from(&b"bytes"[..])], captured_at_ms: 1 };

        logs.append(&class, &record(FrameKind::Submitted)).expect("test: the first frame");
        FAIL_NEXT_INDEX.with(|fail| fail.set(true));
        assert!(logs.append(&class, &record(FrameKind::Terminal)).is_err(), "its index append failed");
        logs.append(&class, &record(FrameKind::Incomplete)).expect("test: space is back, the owner records again");

        let mut kinds = Vec::new();
        scan(&class.owned_by(owner), |_, frame| kinds.push(frame.kind)).expect("test: the owner's log reads");
        assert_eq!(kinds, vec![FrameKind::Submitted, FrameKind::Incomplete], "the failed frame is not history, and the next reads back as itself");
        assert_eq!(generations(&class.owned_by(owner)).expect("test: generations").len(), 2, "the damaged generation retired; the next began");
    }

    // what this catches: a frame read back through its index that is not the frame written (an
    // offset or length off by one, entries out of order), a reopened log that appends over an
    // earlier generation instead of starting the next, or a crash's unindexed tail or torn index
    // entry read as history.
    #[test]
    fn frames_read_back_through_their_index_across_generations_and_a_torn_tail_is_not_history() {
        let dir = tempfile::tempdir().expect("test: dir");
        let mut log = CaptureLog::open(writer(dir.path())).expect("test: open");
        let a = log.append(FrameKind::Submitted, &Head { request_id: "a".into() }, &[b"the prompt"], 10).expect("test: a");
        let b = log.append(FrameKind::Terminal, &Head { request_id: "a".into() }, &[b"the answer", b"the stream"], 11).expect("test: b");
        assert_eq!((a, b), (Cursor { generation: 0, entry: 0 }, Cursor { generation: 0, entry: 1 }));
        drop(log);
        let mut next = CaptureLog::open(writer(dir.path())).expect("test: reopen");
        assert_eq!(next.generation(), 1, "a reopened log starts the next generation, never appends over the last");
        let c = next.append(FrameKind::Submitted, &Head { request_id: "c".into() }, &[], 12).expect("test: c");

        let b_entry = entries(&writer(dir.path()), b.generation).expect("test: entries")[b.entry as usize];
        let bytes = frame_at(&writer(dir.path()), b.generation, &b_entry).expect("test: b's bytes");
        let (frame, _) = frame::decode(&bytes).expect("test: b decodes");
        assert_eq!((frame.kind, frame.runs.clone()), (FrameKind::Terminal, vec![&b"the answer"[..], &b"the stream"[..]]));

        // a crash mid-index-write: a partial entry after the last whole one is not an entry
        next.index.write_all(&[1, 2, 3]).expect("test: torn entry");
        // a crash between frame and index: frame bytes with no entry are not history
        next.frames.write_all(b"unindexed tail").expect("test: unindexed");
        let mut seen = Vec::new();
        scan(&writer(dir.path()), |cursor, frame| seen.push((cursor, frame.header::<Head>().expect("test: header").request_id))).expect("test: scan");
        assert_eq!(seen, vec![(a, "a".to_string()), (b, "a".to_string()), (c, "c".to_string())], "every indexed frame, oldest first, nothing else");
        assert_eq!(entries(&writer(dir.path()), 1).expect("test: entries").len(), 1, "the torn entry is not counted");
    }
}
