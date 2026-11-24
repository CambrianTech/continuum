//! Where each captured call is in its owner's log (card 699578cb, slice 4): its submission, its end
//! and any receipt of an unwritable record, by the call's key. Built from the frames' headers only
//! (never their runs), and extended from each generation's index tail: a refresh reads what was
//! appended since the last one, never the whole log again. A generation retention removed takes its
//! places with it.

use std::collections::{BTreeMap, HashMap};

use continuum_kernel::system_resources::disk_reporters::ClassWriter;

use crate::call_frames::{CallKey, IncompleteHead, SubmittedHead, TerminalHead};
use crate::capture_log::{self, Cursor, Entry, LogError};
use crate::frame::{FrameError, FrameKind};
use crate::call_frames::CallStatus;

/// One frame's place: its cursor and its index entry (offset, length, when it was captured).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    pub cursor: Cursor,
    pub entry: Entry,
}

/// Where one call's frames are.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CallPlaces {
    pub submitted: Option<Placed>,
    pub terminal: Option<Placed>,
    pub incomplete: Option<Placed>,
    /// How the call ended, from its terminal frame's header.
    pub ended: Option<CallStatus>,
}

impl CallPlaces {
    /// When and where the call was first captured: what orders calls oldest to newest. Two calls
    /// captured in one millisecond keep the order they were written in (their cursors), never an
    /// order of their keys (a session id is random: Linux CI listed two such calls swapped, 2026-11-23).
    pub fn first(&self) -> Option<(u64, Cursor)> {
        [self.submitted, self.terminal, self.incomplete].iter().flatten().map(|p| (p.entry.captured_at_ms, p.cursor)).min()
    }

    /// How the call stands: its end's status, else a receipt that it could not be recorded, else
    /// submitted and not yet ended.
    pub fn status(&self) -> CallStatus {
        if let Some(ended) = self.ended {
            return ended;
        }
        if self.incomplete.is_some() {
            return CallStatus::Incomplete;
        }
        CallStatus::Submitted
    }

    fn without(&mut self, generation: u64) {
        for place in [&mut self.submitted, &mut self.terminal, &mut self.incomplete] {
            if place.is_some_and(|p| p.cursor.generation == generation) {
                *place = None;
            }
        }
        if self.terminal.is_none() {
            self.ended = None;
        }
    }

    fn is_empty(&self) -> bool {
        self.submitted.is_none() && self.terminal.is_none() && self.incomplete.is_none()
    }
}

/// One owner's calls by key.
pub struct CallIndex {
    writer: ClassWriter,
    /// The index entries already read, per generation.
    read: HashMap<u64, u32>,
    calls: BTreeMap<CallKey, CallPlaces>,
}

impl CallIndex {
    pub fn new(writer: ClassWriter) -> Self {
        Self { writer, read: HashMap::new(), calls: BTreeMap::new() }
    }

    /// Read what the owner's log appended since the last refresh, and forget removed generations.
    pub fn refresh(&mut self) -> Result<(), LogError> {
        let generations = capture_log::generations(&self.writer)?;
        let gone: Vec<u64> = self.read.keys().copied().filter(|g| !generations.contains(g)).collect();
        for generation in gone {
            self.read.remove(&generation);
            for places in self.calls.values_mut() {
                places.without(generation);
            }
        }
        self.calls.retain(|_, places| !places.is_empty());
        for generation in generations {
            let mut from = self.read.get(&generation).copied().unwrap_or(0); // unwrap_or: a generation not read yet is read from its first entry
            if capture_log::entry_count(&self.writer, generation)? < from {
                // emptied in place under pressure since the last refresh: what was read of it is gone
                // (Codex on #6236), so its places go and it is read again from its first entry
                for places in self.calls.values_mut() {
                    places.without(generation);
                }
                self.calls.retain(|_, places| !places.is_empty());
                from = 0;
            }
            let entries = capture_log::entries_from(&self.writer, generation, from)?;
            for (i, entry) in entries.iter().enumerate() {
                let cursor = Cursor { generation, entry: from + i as u32 };
                let placed = Placed { cursor, entry: *entry };
                let (kind, header) = capture_log::frame_head(&self.writer, generation, entry)?;
                let decode = |e: rmp_serde::decode::Error| LogError::Frame(FrameError::Decode(e));
                match kind {
                    FrameKind::Submitted => {
                        let key = rmp_serde::from_slice::<SubmittedHead>(&header).map_err(decode)?.key;
                        self.calls.entry(key).or_default().submitted = Some(placed);
                    }
                    FrameKind::Terminal => {
                        let head = rmp_serde::from_slice::<TerminalHead>(&header).map_err(decode)?;
                        let places = self.calls.entry(head.key).or_default();
                        places.terminal = Some(placed);
                        places.ended = Some(head.status);
                    }
                    FrameKind::Incomplete => {
                        let key = rmp_serde::from_slice::<IncompleteHead>(&header).map_err(decode)?.key;
                        self.calls.entry(key).or_default().incomplete = Some(placed);
                    }
                }
            }
            self.read.insert(generation, from + entries.len() as u32);
        }
        Ok(())
    }

    pub fn get(&self, key: &CallKey) -> Option<&CallPlaces> {
        self.calls.get(key)
    }

    /// Every call, oldest first captured first.
    pub fn oldest_first(&self) -> Vec<(&CallKey, &CallPlaces)> {
        let mut calls: Vec<_> = self.calls.iter().collect();
        calls.sort_by_key(|(key, places)| (places.first(), *key));
        calls
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::call_frames::{Ending, Submission};
    use crate::capture_log::CaptureLog;
    use continuum_engine::ai::types::{ChatMessage, TextGenerationRequest};
    use continuum_kernel::system_resources::disk_reporters::TrackedDir;
    use uuid::Uuid;

    fn append(log: &mut CaptureLog, record: crate::capture_log::CaptureRecord) {
        let runs: Vec<&[u8]> = record.runs.iter().map(AsRef::as_ref).collect();
        log.append_parts(crate::frame::Parts { kind: record.kind, header: &record.header, runs: &runs }, record.captured_at_ms).expect("test: append");
    }

    fn submitted(key: &CallKey, request: &TextGenerationRequest, at: u64) -> crate::capture_log::CaptureRecord {
        let head = SubmittedHead {
            key: key.clone(),
            build_sha: "abc".into(),
            room_id: Uuid::nil(),
            cycle_id: None,
            context_window: None,
            cause: "chat".into(),
            cause_root: None,
            model: None,
        };
        Submission { head, request, replay: None }.record(Uuid::nil(), at).expect("test: record")
    }

    // what this catches (Linux CI, 2026-11-23): two calls captured in one millisecond listed in the
    // order of their keys (a random session id) instead of the order they were written in.
    #[test]
    fn calls_captured_in_one_millisecond_keep_their_write_order() {
        let dir = tempfile::tempdir().expect("test: dir");
        let writer = || TrackedDir::new("call-index-tie-test", dir.path().to_path_buf()).writer().owned_by(Uuid::nil());
        let request = TextGenerationRequest { messages: vec![ChatMessage::text("user", "tie")], ..Default::default() };
        // keyed so the keys sort opposite to the write order
        let (written_first, written_second) = (CallKey { session_id: Uuid::max(), request_id: "a".into() }, CallKey { session_id: Uuid::nil(), request_id: "a".into() });
        let mut log = CaptureLog::open(writer()).expect("test: open");
        append(&mut log, submitted(&written_first, &request, 10));
        append(&mut log, submitted(&written_second, &request, 10));
        let mut index = CallIndex::new(writer());
        index.refresh().expect("test: refresh");
        let order: Vec<Uuid> = index.oldest_first().iter().map(|(k, _)| k.session_id).collect();
        assert_eq!(order, [Uuid::max(), Uuid::nil()], "the call written first is listed first");
    }

    // what this catches: a call whose end is filed under another call, a refresh that re-reads or
    // misses what was appended since the last one (its places double or vanish), an end that lands
    // in a later generation than its submission and is not joined to it, or places that outlive the
    // generation retention removed.
    #[test]
    fn a_call_is_found_by_its_key_across_generations_and_refreshes() {
        let dir = tempfile::tempdir().expect("test: dir");
        let writer = || TrackedDir::new("call-index-test", dir.path().to_path_buf()).writer().owned_by(Uuid::nil());
        let request = TextGenerationRequest { messages: vec![ChatMessage::text("user", "close the stray cards")], ..Default::default() };
        let (a, b) = (CallKey { session_id: Uuid::nil(), request_id: "z".into() }, CallKey { session_id: Uuid::nil(), request_id: "b".into() });

        let mut first = CaptureLog::open(writer()).expect("test: open");
        append(&mut first, submitted(&a, &request, 10));
        append(&mut first, submitted(&b, &request, 11));
        let mut index = CallIndex::new(writer());
        index.refresh().expect("test: refresh");
        assert_eq!(index.get(&a).and_then(|p| p.submitted).map(|p| p.cursor), Some(Cursor { generation: 0, entry: 0 }));
        assert!(index.get(&a).is_some_and(|p| p.terminal.is_none()), "not ended yet");

        let end = TerminalHead { key: a.clone(), status: CallStatus::Completed, elapsed_ms: 5, error: None };
        append(&mut first, Ending { head: end.clone(), response: None, wire: None }.record(Uuid::nil(), 12).expect("test: record"));
        drop(first);
        let mut second = CaptureLog::open(writer()).expect("test: reopen");
        append(&mut second, Ending { head: TerminalHead { key: b.clone(), ..end }, response: None, wire: None }.record(Uuid::nil(), 13).expect("test: record"));
        index.refresh().expect("test: refresh");
        index.refresh().expect("test: a second refresh reads nothing new");
        assert_eq!(index.get(&a).and_then(|p| p.terminal).map(|p| p.cursor), Some(Cursor { generation: 0, entry: 2 }), "appended since the last refresh");
        assert_eq!(index.get(&a).and_then(|p| p.ended), Some(CallStatus::Completed), "how it ended, from its end's header");
        assert_eq!(index.get(&b).and_then(|p| p.terminal).map(|p| p.cursor), Some(Cursor { generation: 1, entry: 0 }), "ended in the next generation, joined by key");
        let order: Vec<&str> = index.oldest_first().iter().map(|(k, _)| k.request_id.as_str()).collect();
        assert_eq!(order, ["z", "b"], "first captured first, whatever the keys' order");

        std::fs::remove_file(writer().path(std::path::Path::new("0000000000000000.index")).expect("test: path")).expect("test: evict");
        index.refresh().expect("test: refresh");
        assert!(index.get(&a).is_none(), "its generation is gone, and so is it");
        assert_eq!(index.get(&b).map(|p| (p.submitted, p.terminal.map(|t| t.cursor))), Some((None, Some(Cursor { generation: 1, entry: 0 }))), "its end outlives its submission");
        drop(second);
    }
}
