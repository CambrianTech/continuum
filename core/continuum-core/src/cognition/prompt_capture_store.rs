//! Bounded derived offsets for the existing prompt capture owner. Payload JSONL
//! remains authoritative; the fixed-width sidecar only locates complete rows.
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use ts_rs::TS;
use uuid::Uuid;

const SLOT_BYTES: usize = 2048;
const MAX_ENTRIES: u32 = 1024;
// Rotate before the next row at this threshold; one row is independently
// bounded by the same amount. Each segment is therefore <= 2 * this bound,
// and two retained segments consume <= 256 MiB plus 4 MiB of fixed metadata.
const MAX_PAYLOAD_BYTES: u64 = 64 * 1024 * 1024;
pub const PAGE_LIMIT: usize = 40;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/PayloadRef.ts"
)]
pub struct PayloadRef {
    #[ts(type = "string")]
    pub generation: Uuid,
    #[ts(type = "number")]
    pub offset: u64,
    #[ts(type = "number")]
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/CallStatus.ts"
)]
pub enum CallStatus {
    Submitted,
    Completed,
    Failed,
    Cancelled,
    Incomplete,
    Legacy,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/CallHeader.ts"
)]
pub struct CallHeader {
    pub request_id: String,
    #[ts(type = "string")]
    pub session_id: Uuid,
    #[ts(type = "number | null")]
    pub cycle_id: Option<u64>,
    #[ts(type = "string")]
    pub room_id: Uuid,
    pub cause: String,
    #[ts(type = "string | null")]
    pub cause_root: Option<Uuid>,
    #[ts(type = "number")]
    pub captured_at_ms: u64,
    #[ts(type = "number")]
    pub started_at_ms: u64,
    pub status: CallStatus,
    pub model: Option<String>,
    /// References bind to retained generations, never caller-supplied paths.
    pub request: PayloadRef,
    pub terminal: Option<PayloadRef>,
    /// Stable event cursor; callers upsert lifecycle events by session/request ID.
    pub cursor: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/PlaybackPage.ts"
)]
pub struct PlaybackPage {
    /// Oldest first within a page. Lifecycle events share one request_id.
    pub entries: Vec<CallHeader>,
    pub older: Option<String>,
    pub newer: Option<String>,
    pub issues: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SegmentHeader {
    schema_version: u32,
    generation: Uuid,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
fn paths(dir: &Path, persona: Uuid, previous: bool) -> (PathBuf, PathBuf) {
    let suffix = if previous { ".prev" } else { "" };
    (
        dir.join(format!("{persona}{suffix}.jsonl")),
        dir.join(format!("{persona}{suffix}.pidx")),
    )
}
fn write_slot(file: &mut File, value: &impl Serialize) -> io::Result<()> {
    // Only small metadata is buffered; the request/response is written borrowed.
    let bytes = serde_json::to_vec(value)?; // Disk boundary: encode the small fixed index slot, never the full request/response payload.
    if bytes.len() >= SLOT_BYTES {
        return Err(invalid("capture index metadata exceeds slot"));
    }
    let mut slot = [b' '; SLOT_BYTES];
    slot[..bytes.len()].copy_from_slice(&bytes);
    slot[SLOT_BYTES - 1] = b'\n';
    file.write_all(&slot)
}
fn read_slot<T: serde::de::DeserializeOwned>(file: &mut File, slot: u32) -> io::Result<T> {
    file.seek(SeekFrom::Start(u64::from(slot) * SLOT_BYTES as u64))?;
    let mut bytes = [0; SLOT_BYTES];
    file.read_exact(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// One writer, two bounded segments. Rotation is part of this capture owner,
/// not a background task. An in-flight reference that ages out is explicit.
pub(super) struct Store {
    dir: PathBuf,
    persona: Uuid,
    data: Option<BufWriter<File>>,
    index: Option<File>,
    generation: Uuid,
    entries: u32,
    max_entries: u32,
    max_bytes: u64,
}
impl Store {
    pub fn open(dir: &Path, persona: Uuid) -> io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let mut store = Self {
            dir: dir.into(),
            persona,
            data: None,
            index: None,
            generation: Uuid::nil(),
            entries: 0,
            max_entries: MAX_ENTRIES,
            max_bytes: MAX_PAYLOAD_BYTES,
        };
        store.rotate()?;
        Ok(store)
    }
    fn rotate(&mut self) -> io::Result<()> {
        self.data.take();
        self.index.take();
        let (data, index) = paths(&self.dir, self.persona, false);
        let (previous_data, previous_index) = paths(&self.dir, self.persona, true);
        if data.try_exists()? {
            // Fixed, capture-owned files only. Refuse a partial roll instead of
            // appending a new session beneath an old index.
            if previous_data.try_exists()? {
                std::fs::remove_file(&previous_data)?;
            }
            if previous_index.try_exists()? {
                std::fs::remove_file(&previous_index)?;
            }
            std::fs::rename(&data, &previous_data)?;
            if index.try_exists()? {
                std::fs::rename(&index, &previous_index)?;
            }
        } else if index.try_exists()? {
            return Err(invalid("capture index exists without its payload"));
        }
        let generation = Uuid::new_v4();
        let header = SegmentHeader {
            schema_version: 4,
            generation,
        };
        let mut data_file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(data)?;
        serde_json::to_writer(&mut data_file, &header)?;
        data_file.write_all(b"\n")?;
        data_file.flush()?;
        let mut index_file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(index)?;
        write_slot(&mut index_file, &header)?;
        index_file.flush()?;
        self.data = Some(BufWriter::with_capacity(64 * 1024, data_file));
        self.index = Some(index_file);
        self.generation = generation;
        self.entries = 0;
        Ok(())
    }
    pub fn append(
        &mut self,
        value: &impl Serialize,
        mut header: CallHeader,
        terminal: bool,
    ) -> io::Result<PayloadRef> {
        let size = self
            .data
            .as_ref()
            .ok_or_else(|| invalid("capture writer unavailable"))?
            .get_ref()
            .metadata()?
            .len();
        if self.entries >= self.max_entries || size >= self.max_bytes {
            self.rotate()?;
        }
        let file = self
            .data
            .as_mut()
            .ok_or_else(|| invalid("capture writer unavailable"))?;
        let offset = file.stream_position()?;
        // A single oversized row cannot defeat retention. No partial request is
        // advertised: truncate the failed row, emit an error, retain earlier rows.
        let mut writer = LimitedWriter {
            file,
            remaining: self.max_bytes,
        };
        let write = serde_json::to_writer(&mut writer, value)
            .map_err(io::Error::from)
            .and_then(|()| writer.write_all(b"\n"));
        let write = write.and_then(|()| writer.file.flush());
        if let Err(error) = write {
            // Discard buffered bytes without flushing a partial failed row.
            let buffered = self
                .data
                .take()
                .ok_or_else(|| invalid("capture writer unavailable"))?;
            let (mut file, _) = buffered.into_parts();
            file.set_len(offset)?;
            file.seek(SeekFrom::Start(offset))?;
            self.data = Some(BufWriter::with_capacity(64 * 1024, file));
            return Err(error);
        }
        let file = self
            .data
            .as_mut()
            .ok_or_else(|| invalid("capture writer unavailable"))?;
        let reference = PayloadRef {
            generation: self.generation,
            offset,
            bytes: file.stream_position()? - offset,
        };
        if terminal {
            header.terminal = Some(reference.clone());
        } else {
            header.request = reference.clone();
        }
        header.cursor = format!("{}:{}", self.generation, self.entries);
        let index = self
            .index
            .as_mut()
            .ok_or_else(|| invalid("capture index unavailable"))?;
        // Payload precedes index. A crash/failed index write leaves a detectable
        // unindexed tail; no reader mistakes it for complete history.
        if let Err(error) = write_slot(index, &header).and_then(|()| index.flush()) {
            self.data.take();
            self.index.take();
            return Err(error);
        }
        self.entries += 1;
        Ok(reference)
    }
}
struct LimitedWriter<'a> {
    file: &'a mut BufWriter<File>,
    remaining: u64,
}
impl Write for LimitedWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            return Err(invalid(
                "capture row exceeds retained payload capacity; record not stored",
            ));
        }
        let written = self.file.write(bytes)?;
        self.remaining -= written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

struct Segment {
    data: File,
    index: File,
    generation: Uuid,
    entries: u32,
}
impl Segment {
    fn open(
        dir: &Path,
        persona: Uuid,
        previous: bool,
        issues: &mut Vec<String>,
    ) -> io::Result<Option<Self>> {
        let (data_path, index_path) = paths(dir, persona, previous);
        let mut data = match File::open(&data_path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let mut index = match File::open(index_path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                issues.push(format!("{} capture has no index (legacy or interrupted write); use cognition/prompt for bounded legacy reads", if previous { "previous" } else { "current" }));
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let index_len = index.metadata()?.len();
        if index_len < SLOT_BYTES as u64 {
            return Err(invalid("capture index header incomplete"));
        }
        let header: SegmentHeader = read_slot(&mut index, 0)?;
        let mut first = [0; SLOT_BYTES];
        let bytes = data.read(&mut first)?;
        let newline = first[..bytes]
            .iter()
            .position(|byte| *byte == b'\n')
            .ok_or_else(|| invalid("capture payload header incomplete"))?;
        let data_header: SegmentHeader = serde_json::from_slice(&first[..newline])?;
        if header.schema_version != 4
            || data_header.schema_version != 4
            || header.generation != data_header.generation
        {
            return Err(invalid(
                "capture payload/index generation mismatch; retry after rotation",
            ));
        }
        if index_len % SLOT_BYTES as u64 != 0 {
            issues.push("capture index has an incomplete tail".into());
        }
        let count = index_len / SLOT_BYTES as u64 - 1;
        if count > u64::from(MAX_ENTRIES) {
            return Err(invalid("capture index exceeds retained entry bound"));
        }
        let entries = count as u32;
        let indexed_end = if entries == 0 {
            newline as u64 + 1
        } else {
            let last: CallHeader = read_slot(&mut index, entries)?;
            let reference = last.terminal.as_ref().unwrap_or(&last.request); // A nonterminal entry owns its request bytes; a terminal entry owns its terminal bytes.
            reference
                .offset
                .checked_add(reference.bytes)
                .ok_or_else(|| invalid("invalid capture range"))?
        };
        if data.metadata()?.len() != indexed_end {
            issues.push(
                "payload has an unindexed or incomplete tail; recording is not complete".into(),
            );
        }
        Ok(Some(Self {
            data,
            index,
            generation: header.generation,
            entries,
        }))
    }
    fn read(&mut self, reference: &PayloadRef) -> io::Result<serde_json::Value> {
        if reference.generation != self.generation || reference.bytes > MAX_PAYLOAD_BYTES {
            return Err(invalid("capture reference is outside retained capacity"));
        }
        let end = reference
            .offset
            .checked_add(reference.bytes)
            .ok_or_else(|| invalid("invalid capture range"))?;
        if end > self.data.metadata()?.len() {
            return Err(invalid("capture payload incomplete"));
        }
        self.data.seek(SeekFrom::Start(reference.offset))?;
        let mut bytes = vec![0; reference.bytes as usize];
        self.data.read_exact(&mut bytes)?;
        Ok(serde_json::from_slice(&bytes)?)
    }
}
fn cursor(value: &str) -> io::Result<(Uuid, u32)> {
    let (generation, entry) = value
        .split_once(':')
        .ok_or_else(|| invalid("invalid playback cursor"))?;
    Ok((
        Uuid::parse_str(generation).map_err(|_| invalid("invalid playback generation"))?,
        entry
            .parse()
            .map_err(|_| invalid("invalid playback position"))?,
    ))
}
fn generations(dir: &Path, persona: Uuid) -> io::Result<[Option<Uuid>; 2]> {
    let mut result = [None, None];
    for (slot, previous) in [true, false].into_iter().enumerate() {
        let (_, path) = paths(dir, persona, previous);
        match File::open(path) {
            Ok(mut file) => {
                result[slot] = Some(read_slot::<SegmentHeader>(&mut file, 0)?.generation)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(result)
}

fn segments_with(
    dir: &Path,
    persona: Uuid,
    issues: &mut Vec<String>,
    mut open: impl FnMut(bool, &mut Vec<String>) -> io::Result<Option<Segment>>,
) -> io::Result<Vec<Segment>> {
    let before = generations(dir, persona)?;
    let mut result = Vec::with_capacity(2);
    for previous in [true, false] {
        if let Some(segment) = open(previous, issues)? {
            result.push(segment);
        }
    }
    // Pair integrity matters too: A(previous)+C(current) can each be valid while
    // a rotation between opens skipped retained B. Unique generation IDs cannot
    // ABA; refuse this snapshot so no cursor can silently advance past B.
    if before != generations(dir, persona)? {
        return Err(invalid("capture generations changed while opening playback; retry without advancing the cursor"));
    }
    Ok(result)
}
fn segments(dir: &Path, persona: Uuid, issues: &mut Vec<String>) -> io::Result<Vec<Segment>> {
    segments_with(dir, persona, issues, |previous, issues| {
        Segment::open(dir, persona, previous, issues)
    })
}

pub fn page(
    dir: &Path,
    persona: Uuid,
    position: Option<&str>,
    newer: bool,
    limit: usize,
) -> io::Result<PlaybackPage> {
    let mut issues = Vec::new();
    let mut segments = segments(dir, persona, &mut issues)?;
    let total: usize = segments.iter().map(|s| s.entries as usize).sum();
    let boundary = if let Some(position) = position {
        let (generation, entry) = cursor(position)?;
        let mut before = 0;
        let segment = segments
            .iter()
            .find(|s| {
                if s.generation == generation {
                    true
                } else {
                    before += s.entries as usize;
                    false
                }
            })
            .ok_or_else(|| invalid("playback cursor expired after capture rotation"))?;
        if entry >= segment.entries {
            return Err(invalid("playback cursor outside committed entries"));
        }
        before + entry as usize + usize::from(newer)
    } else if newer {
        0
    } else {
        total
    };
    let limit = limit.clamp(1, PAGE_LIMIT);
    let (start, end) = if newer {
        (boundary, (boundary + limit).min(total))
    } else {
        (boundary.saturating_sub(limit), boundary)
    };
    let mut entries = Vec::with_capacity(end - start);
    let mut before = 0;
    for segment in &mut segments {
        for i in
            start.saturating_sub(before)..end.saturating_sub(before).min(segment.entries as usize)
        {
            entries.push(read_slot::<CallHeader>(&mut segment.index, i as u32 + 1)?);
        }
        before += segment.entries as usize;
    }
    Ok(PlaybackPage {
        older: if start > 0 {
            entries.first().map(|e| e.cursor.clone())
        } else {
            None
        },
        // Keep a tail cursor even at EOF: a subsequent refresh only reads new headers.
        newer: entries
            .last()
            .map(|e| e.cursor.clone())
            .or_else(|| position.map(str::to_owned)),
        entries,
        issues,
    })
}

#[derive(Debug, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/PlaybackDetail.ts"
)]
pub struct PlaybackDetail {
    pub header: CallHeader,
    #[ts(type = "unknown")]
    pub submitted: Option<serde_json::Value>,
    #[ts(type = "unknown")]
    pub terminal: Option<serde_json::Value>,
    pub issues: Vec<String>,
}
pub fn detail(dir: &Path, persona: Uuid, position: &str) -> io::Result<PlaybackDetail> {
    let mut issues = Vec::new();
    let mut segments = segments(dir, persona, &mut issues)?;
    let (generation, entry) = cursor(position)?;
    let source = segments
        .iter_mut()
        .find(|s| s.generation == generation)
        .ok_or_else(|| invalid("selected capture expired after rotation"))?;
    if entry >= source.entries {
        return Err(invalid("selected capture outside committed index"));
    }
    let header: CallHeader = read_slot(&mut source.index, entry + 1)?;
    let mut read = |reference: &PayloadRef| -> Option<serde_json::Value> {
        match segments
            .iter_mut()
            .find(|s| s.generation == reference.generation)
        {
            Some(segment) => match segment.read(reference) {
                Ok(value) => Some(value),
                Err(error) => {
                    issues.push(error.to_string());
                    None
                }
            },
            None => {
                issues.push("request payload expired from retained capture segments".into());
                None
            }
        }
    };
    let submitted = read(&header.request);
    let terminal = header.terminal.as_ref().and_then(&mut read);
    // The index is untrusted disk data too: don't return a different call if a
    // partial/manual edit changed an otherwise in-range pointer.
    let session = header.session_id.to_string();
    let room = header.room_id.to_string();
    let persona = persona.to_string();
    for value in [&submitted, &terminal].into_iter().flatten() {
        if !matches!(header.status, CallStatus::Legacy) {
            if value.get("request_id").and_then(|v| v.as_str()) != Some(header.request_id.as_str())
                || value.get("session_id").and_then(|v| v.as_str()) != Some(session.as_str())
            {
                return Err(invalid("capture payload/session/request identity mismatch"));
            }
            for (field, expected) in [("persona_id", persona.as_str()), ("room_id", room.as_str())]
            {
                if value
                    .get(field)
                    .is_some_and(|value| value.as_str() != Some(expected))
                {
                    return Err(invalid("capture payload source identity mismatch"));
                }
            }
        }
    }

    Ok(PlaybackDetail {
        header,
        submitted,
        terminal,
        issues,
    })
}

pub(super) fn file_generation(path: &Path) -> io::Result<Uuid> {
    let mut file = File::open(path.with_extension("pidx"))?;
    Ok(read_slot::<SegmentHeader>(&mut file, 0)?.generation)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn header(id: usize) -> CallHeader {
        CallHeader {
            request_id: format!("request-{id}"),
            session_id: Uuid::nil(),
            cycle_id: Some(id as u64),
            room_id: Uuid::nil(),
            cause: "synthetic".into(),
            cause_root: None,
            captured_at_ms: id as u64,
            started_at_ms: id as u64,
            status: CallStatus::Submitted,
            model: None,
            request: PayloadRef {
                generation: Uuid::nil(),
                offset: 0,
                bytes: 0,
            },
            terminal: None,
            cursor: String::new(),
        }
    }
    fn row(id: usize) -> serde_json::Value {
        serde_json::json!({"request_id": format!("request-{id}"), "session_id": Uuid::nil(), "request": {"messages": []}})
    }

    // Real files, small fixture bounds on the actual writer: rotation, paging,
    // expiration and integrity are exercised without a live persona or daemon.
    #[test]
    fn playback_retention_and_cursors_are_bounded_across_rotation() {
        let dir = tempfile::tempdir().expect("capture directory");
        let persona = Uuid::new_v4();
        let mut store = Store::open(dir.path(), persona).expect("capture store");
        store.max_entries = 2;
        store.append(&row(0), header(0), false).expect("first row");
        let initial = page(dir.path(), persona, None, false, 999).expect("first page");
        let old = initial.newer.expect("tail cursor");
        for id in 1..7 {
            store
                .append(&row(id), header(id), false)
                .expect("append and rotate");
        }
        let latest = page(dir.path(), persona, None, false, 999).expect("bounded page");
        assert_eq!(latest.entries.len(), 3, "two bounded segments survive");
        assert_eq!(latest.entries[0].request_id, "request-4");
        assert!(page(dir.path(), persona, Some(&old), true, 10)
            .expect_err("expired cursor must be explicit")
            .to_string()
            .contains("expired"));
        let last = page(dir.path(), persona, None, false, 1).expect("latest single row");
        let previous = page(dir.path(), persona, last.older.as_deref(), false, 1)
            .expect("previous generation row");
        assert_eq!(previous.entries[0].request_id, "request-5");
        let forward = page(dir.path(), persona, previous.newer.as_deref(), true, 1)
            .expect("next row across generation");
        assert_eq!(forward.entries[0].request_id, "request-6");
        assert!(forward.issues.is_empty());
        assert_eq!(
            std::fs::read_dir(dir.path())
                .expect("capture files")
                .count(),
            4
        );
    }

    #[test]
    fn playback_refuses_a_generation_pair_changed_between_real_opens() {
        let dir = tempfile::tempdir().expect("capture directory");
        let persona = Uuid::new_v4();
        let mut store = Store::open(dir.path(), persona).expect("capture store");
        store.max_entries = 1;
        store
            .append(&row(0), header(0), false)
            .expect("generation A");
        store
            .append(&row(1), header(1), false)
            .expect("generation B");
        let error = segments_with(dir.path(), persona, &mut Vec::new(), |previous, issues| {
            let segment = Segment::open(dir.path(), persona, previous, issues)?;
            if previous {
                store.append(&row(2), header(2), false)?;
            }
            Ok(segment)
        })
        .err()
        .expect("mixed A+C must be refused");
        assert!(error.to_string().contains("generations changed"));
        let coherent = page(dir.path(), persona, None, false, 10).expect("retry retained B+C");
        assert_eq!(
            coherent
                .entries
                .iter()
                .map(|entry| entry.request_id.as_str())
                .collect::<Vec<_>>(),
            vec!["request-1", "request-2"]
        );
    }

    #[test]
    fn playback_reports_unindexed_tail_without_reading_it_as_a_call() {
        let dir = tempfile::tempdir().expect("capture directory");
        let persona = Uuid::new_v4();
        let mut store = Store::open(dir.path(), persona).expect("capture store");
        store
            .append(&row(0), header(0), false)
            .expect("committed row");
        let file = store.data.as_mut().expect("writer");
        file.write_all(b"{\"request_id\":")
            .expect("interrupted write fixture");
        file.flush().expect("flush partial bytes");
        let page = page(dir.path(), persona, None, false, 10)
            .expect("committed entries still inspectable");
        assert_eq!(page.entries.len(), 1);
        assert!(page.issues.iter().any(|issue| issue.contains("unindexed")));
    }

    #[test]
    fn playback_oversized_row_rolls_back_and_keeps_committed_records() {
        let dir = tempfile::tempdir().expect("capture directory");
        let persona = Uuid::new_v4();
        let mut store = Store::open(dir.path(), persona).expect("capture store");
        store.max_bytes = 512;
        store
            .append(&row(0), header(0), false)
            .expect("committed row");
        assert!(store.append(&"x".repeat(1000), header(1), false).is_err());
        let page = page(dir.path(), persona, None, false, 10).expect("intact index");
        assert_eq!(page.entries.len(), 1);
        assert!(
            page.issues.is_empty(),
            "failed row must not leak buffered partial bytes"
        );
    }

    #[test]
    fn playback_rejects_same_request_id_from_a_different_session() {
        let dir = tempfile::tempdir().expect("capture directory");
        let persona = Uuid::new_v4();
        let mut store = Store::open(dir.path(), persona).expect("capture store");
        let mut payload = row(0);
        payload["session_id"] = serde_json::json!(Uuid::new_v4());
        store
            .append(&payload, header(0), false)
            .expect("mismatched retained fixture");
        let page = page(dir.path(), persona, None, false, 1).expect("index page");
        assert!(detail(dir.path(), persona, &page.entries[0].cursor)
            .expect_err("source mismatch")
            .to_string()
            .contains("session/request identity"));
    }

    #[test]
    fn playback_expired_request_remains_an_explicit_incomplete_terminal() {
        let dir = tempfile::tempdir().expect("capture directory");
        let persona = Uuid::new_v4();
        let mut store = Store::open(dir.path(), persona).expect("capture store");
        store.max_entries = 1;
        let request = store.append(&row(0), header(0), false).expect("request");
        store
            .append(&row(1), header(1), false)
            .expect("next generation");
        let mut terminal = header(0);
        terminal.request = request;
        terminal.status = CallStatus::Completed;
        store
            .append(&row(0), terminal, true)
            .expect("terminal after request expires");
        let page = page(dir.path(), persona, None, false, 1).expect("terminal page");
        let detail = detail(dir.path(), persona, &page.entries[0].cursor).expect("partial detail");
        assert!(detail.submitted.is_none());
        assert!(detail.terminal.is_some());
        assert!(detail.issues.iter().any(|issue| issue.contains("expired")));
    }
}
