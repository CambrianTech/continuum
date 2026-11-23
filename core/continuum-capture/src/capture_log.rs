//! The capture's binary log (card 699578cb, slice 2): frames appended to a generation file, each found
//! through a fixed-width binary index entry. One owner's log is a sequence of generations; a writer
//! opens the next one and only appends to it, so nothing is ever rewritten in place.
//!
//! ```text
//! <owner>/<generation>.frames   frame, frame, ...                    (frame::encode)
//! <owner>/<generation>.index    [u64 offset][u32 length][u8 kind][3 pad][u64 captured_at_ms] ...
//! ```
//!
//! Files are placed through the owner's disk class (`ClassWriter`), never at a path the log picks. The
//! frame is written before its index entry: a crash between them leaves an unindexed tail no reader
//! takes for history, and a frame cut off mid-write decodes as a short tail. Retention is the class's
//! pool's (oldest closed generation first), never a size this log decides.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use continuum_kernel::system_resources::disk_reporters::ClassWriter;
use serde::Serialize;

use crate::frame::{self, FrameError, FrameKind};

/// The bytes one index entry takes.
const ENTRY: usize = 8 + 4 + 1 + 3 + 8;

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
}

impl Entry {
    fn bytes(&self) -> [u8; ENTRY] {
        let mut out = [0u8; ENTRY];
        out[0..8].copy_from_slice(&self.offset.to_le_bytes());
        out[8..12].copy_from_slice(&self.length.to_le_bytes());
        out[12] = self.kind;
        out[16..24].copy_from_slice(&self.captured_at_ms.to_le_bytes());
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
    #[error("generation {generation} has no index entry {entry}")]
    NoEntry { generation: u64, entry: u32 },
    #[error("a frame of {0} bytes does not fit the index's 32-bit length")]
    TooLong(usize),
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
        Ok(Self { writer, generation, frames, index, offset: 0, entries: 0, scratch: Vec::new(), held })
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Append one frame and its index entry; the frame first, so a crash leaves only an unindexed tail.
    pub fn append(&mut self, kind: FrameKind, header: &impl Serialize, runs: &[&[u8]], captured_at_ms: u64) -> Result<Cursor, LogError> {
        self.scratch.clear();
        frame::encode(&mut self.scratch, kind, header, runs)?;
        let length = u32::try_from(self.scratch.len()).map_err(|_| LogError::TooLong(self.scratch.len()))?;
        self.frames.write_all(&self.scratch)?;
        self.frames.flush()?;
        let entry = Entry { offset: self.offset, length, kind: kind as u8, captured_at_ms };
        self.index.write_all(&entry.bytes())?;
        self.index.flush()?;
        self.offset += u64::from(length);
        let cursor = Cursor { generation: self.generation, entry: self.entries };
        self.entries += 1;
        Ok(cursor)
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

/// The frame bytes at `cursor`, read through its index entry.
pub fn frame_bytes(writer: &ClassWriter, cursor: Cursor) -> Result<Vec<u8>, LogError> {
    let entry = entries(writer, cursor.generation)?
        .get(cursor.entry as usize)
        .copied()
        .ok_or(LogError::NoEntry { generation: cursor.generation, entry: cursor.entry })?;
    let mut file = File::open(writer.path(&frames_of(cursor.generation))?)?;
    file.seek(SeekFrom::Start(entry.offset))?;
    let mut bytes = vec![0u8; entry.length as usize];
    file.read_exact(&mut bytes)?;
    Ok(bytes)
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

        let bytes = frame_bytes(&writer(dir.path()), b).expect("test: b's bytes");
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
