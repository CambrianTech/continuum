//! Tool-output spill: durable recovery for flood-sized tool results.
//!
//! [`whole_or_handle`] is THE custody of a tool result at execution: STORAGE ONLY. Every result
//! is saved whole here and handed back whole with its `handle`; what she sees of it is decided
//! later by her view's levels, never by a cut at execution. A head, a tail, or a head+tail of a
//! source file, a JSON document or a build log is broken, not short (Joel, 2026-10-12: "the
//! clipping I've yelled about for months"; Kimi read one file 25+ times and landed no edit
//! because each read reached her later as its last ~2,100 chars). She opens what she needs
//! through `tool/output`: a prebuilt failure filter, a regex hunt, or a line range. Re-running
//! a tool to recover what it already returned is not a recovery: the world may have moved and
//! a mutating tool must not repeat (Astra, 2026-10-12).
//!
//! Per-persona by construction: spills land under a directory keyed by the
//! persona's id, and the read command resolves by the SAME id (`ctx.caller`'s
//! `peer_id`, which equals `persona_id` for a local persona). A persona can only
//! ever read back its OWN spilled output — the directory layout IS the scope.
//!
//! The investigate logic ([`investigate_file`]) is what makes "find the error in the
//! crap" first-class: a regex grep with context lines (the failure-hunting path)
//! plus explicit line-range paging. It streams through retained bytes
//! with bounded memory, and exact byte extents remain reachable between pages.

use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::ai::types::ResultCustody;

/// The handles live minds still point at, one set per holder (a working memory's instance).
/// Disk-pressure eviction of the spill directory never deletes one of these
/// ([`is_live`]): saving a result is custody only if the file outlives every pointer to it
/// (Astra on #4963). One owner: working memory republishes its whole set whenever its ring
/// changes and withdraws it when it drops, so no reference count can drift.
static LIVE_HANDLES: std::sync::OnceLock<
    parking_lot::Mutex<std::collections::HashMap<LiveHolder, std::collections::HashSet<String>>>,
> = std::sync::OnceLock::new();

/// Who holds a set of live handles: a working memory (its instance), or a [`LivePin`] holding
/// results saved but not yet published by a memory, in separate key spaces so neither can
/// replace the other's set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum LiveHolder {
    Memory(u64),
    Pin(u64),
}

fn live_handles(
) -> &'static parking_lot::Mutex<std::collections::HashMap<LiveHolder, std::collections::HashSet<String>>> {
    LIVE_HANDLES.get_or_init(Default::default)
}

/// Replace `holder`'s set of referenced handles.
pub fn publish_live_handles(holder: u64, handles: std::collections::HashSet<String>) {
    let mut live = live_handles().lock();
    if handles.is_empty() {
        live.remove(&LiveHolder::Memory(holder));
    } else {
        live.insert(LiveHolder::Memory(holder), handles);
    }
}

/// `holder` references nothing any more (its working memory dropped).
pub fn withdraw_live_handles(holder: u64) {
    live_handles().lock().remove(&LiveHolder::Memory(holder));
}

/// THE only way a result is saved: a save that is live from before its file exists until a
/// working memory has published a set naming it. [`LivePin::save_in`] enters the handle (a
/// content hash, known before the write) into the same live set eviction reads, under the same
/// lock, and only then writes the file; [`evict_unless_live`] decides and deletes under that same
/// lock. So no evictor, sequential or concurrent, ever deletes a file a pin holds, new or
/// existing (Codex on #4982). Dropping the pin withdraws its set: hold it until the memory that
/// references the results has published them. One pin per batch: the act seam (card 12b080c3)
/// and the restore path.
#[must_use = "the saved handles are unprotected once this drops; hold it until a memory has published them"]
pub struct LivePin(LiveHolder);

impl LivePin {
    pub fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        LivePin(LiveHolder::Pin(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)))
    }

    /// Save the result whole and hand it back whole with its handle: custody only, no size
    /// decision. EVERY result is saved, because a result leaves view later (a settlement
    /// collapse, a restore into a smaller window) and the pointer it leaves must resolve
    /// (Astra on #4963). Saves are content-addressed, so a repeated read reuses its file, and
    /// the directory is evicted under disk pressure like every spill.
    pub fn whole_or_handle(&self, full: String, persona_id: Uuid) -> Fitted {
        match spill_root() {
            Ok(root) => whole_or_handle_in(&root, full, persona_id, self),
            Err(e) => unsaved_whole(full, &e),
        }
    }

    /// Save `content` exactly as given and name its custody: `Whole{handle}` (it resolves
    /// byte-identical through `tool/output`), or `Unsaved` when the save failed (probed).
    /// Working memory's results ring keeps a batch's rendered text; this saves THAT text, so a
    /// pointer the ring leaves later resolves to what the mind saw, every call of the batch
    /// included (Astra on #4963).
    pub fn retain(&self, persona_id: Uuid, content: &str) -> ResultCustody {
        match spill_root() {
            Ok(root) => retain_in(&root, persona_id, content, self),
            Err(e) => unretained(content, &e),
        }
    }

    /// THE save: content-addressed, pinned before it is written, unpinned again if the write
    /// fails (only an entry this save added; an identical result this pin already holds stays).
    fn save_in(&self, root: &Path, persona_id: Uuid, content: &str) -> std::io::Result<SpillRef> {
        let handle = handle_of(content);
        let entered = self.hold(&handle);
        match write_in(root, persona_id, &handle, content) {
            Ok(path) => Ok(SpillRef { bytes: content.len(), lines: content.lines().count(), handle, path }),
            Err(e) => {
                if entered {
                    if let Some(set) = live_handles().lock().get_mut(&self.0) {
                        set.remove(&handle);
                    }
                }
                Err(e)
            }
        }
    }

    /// Enter `handle` into this pin's live set; whether it was newly entered.
    fn hold(&self, handle: &str) -> bool {
        live_handles().lock().entry(self.0).or_default().insert(handle.to_owned())
    }
}

impl Default for LivePin {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for LivePin {
    fn drop(&mut self) {
        live_handles().lock().remove(&self.0);
    }
}

/// Save every restored result that has no handle: those from a snapshot written before results
/// were saved (#4963). They predate custody; they are not failed saves. Each is saved exactly as
/// the mind saw it through one [`LivePin`], pinned before its write (the persisted checkpoint is
/// the old snapshot and names none of them). A save that fails leaves its handle `None` and its
/// text whole (probed `Unsaved`).
pub(crate) fn retain_handleless<'a>(
    persona_id: Uuid,
    results: impl IntoIterator<Item = (&'a str, &'a mut Option<String>)>,
) -> LivePin {
    let pin = LivePin::new();
    match spill_root() {
        Ok(root) => retain_handleless_with(|text| retain_in(&root, persona_id, text, &pin), results),
        Err(e) => retain_handleless_with(|text| unretained(text, &e), results),
    }
    pin
}

fn retain_handleless_with<'a>(
    mut save: impl FnMut(&str) -> ResultCustody,
    results: impl IntoIterator<Item = (&'a str, &'a mut Option<String>)>,
) {
    for (text, handle) in results {
        if handle.is_none() {
            *handle = save(text).handle().map(str::to_owned);
        }
    }
}

/// Whether a spill file at `path` is referenced by a mind. The eviction guard the spill
/// directory's pool is built with: a file `<persona>/<handle>.log` is never deleted under
/// pressure while a live working memory references its handle, OR while that persona's
/// persisted checkpoint does. The second closes the restart gap: across the restart a deploy
/// performs, before her mind restores, her references are on disk (Astra on card 9197cd01).
/// An unreadable checkpoint fails closed: nothing of that persona's is evicted.
pub fn is_live(path: &Path) -> bool {
    let Some(handle) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    live_handles().lock().values().any(|set| set.contains(handle)) || checkpoint_protects(path, handle)
}

/// Evict one spill file unless a mind points at it: the pool's remover. Liveness in memory and
/// the delete happen in ONE critical section of the live-set lock, the lock [`LivePin`] enters a
/// handle under. So a pin either lands first (the file is protected) or after the delete (and
/// its write recreates the file): it can never land between the check and the delete, which is
/// how a content-addressed file that already existed could be deleted while pinned (Codex on
/// #4982). The persisted checkpoint is read first, outside the lock.
pub fn evict_unless_live(path: &Path) -> crate::paging::filesystem_tier_pool::Removal {
    evict_unless_live_with(path, || {})
}

fn evict_unless_live_with(
    path: &Path,
    inside_the_section: impl FnOnce(),
) -> crate::paging::filesystem_tier_pool::Removal {
    use crate::paging::filesystem_tier_pool::Removal;
    let Some(handle) = path.file_stem().and_then(|s| s.to_str()) else {
        return fs::remove_file(path).into();
    };
    if checkpoint_protects(path, handle) {
        return Removal::Protected;
    }
    let live = live_handles().lock();
    if live.values().any(|set| set.contains(handle)) {
        return Removal::Protected;
    }
    inside_the_section();
    let removal = fs::remove_file(path).into();
    drop(live);
    removal
}

/// Whether the persisted checkpoint of the persona owning `path` names `handle`. An unreadable
/// checkpoint fails closed.
fn checkpoint_protects(path: &Path, handle: &str) -> bool {
    let Some(persona_id) = path
        .parent()
        .and_then(|dir| dir.file_name())
        .and_then(|name| name.to_str())
        .and_then(|name| Uuid::parse_str(name).ok())
    else {
        return false;
    };
    match crate::cognition::persona_workspace::persisted_result_handles(persona_id) {
        Ok(handles) => handles.contains(handle),
        Err(error) => {
            continuum_base::probe!(
                class = "tool.output.evict_fail_closed",
                persona_id = %persona_id,
                error = %error,
                "a persona's checkpoint could not be read; its saved results are not evicted"
            );
            true
        }
    }
}

/// A spilled tool result: the content-addressed handle (the filename stem the
/// persona quotes back to `tool/output`), where it landed, and how big it was.
#[derive(Debug, Clone)]
pub struct SpillRef {
    /// 16 hex chars of the content SHA-256 — the id the preview names and the
    /// persona passes to `tool/output --handle`. Content-addressed, so an
    /// identical result reuses the same file (idempotent write, free dedup).
    pub handle: String,
    /// Absolute path to the spilled `.log` file.
    pub path: PathBuf,
    /// Byte length of the full (pre-truncation) output.
    pub bytes: usize,
    /// Line count of the full output — surfaced in the preview so she knows the
    /// scale of what she's about to grep.
    pub lines: usize,
}

/// Root for all spilled tool output, under the established `~/.continuum`
/// convention. Fails loud when there is no home directory — we genuinely cannot
/// spill without one, and a silent fallback to `.` would scatter logs into the
/// cwd (the repo) [[fallbacks-are-illegal-fail-loud]].
///
/// Public so the boot path can register this exact directory with the
/// `PressureBroker` for space-pressure eviction (single source for the path — no
/// re-typed string to drift against).
pub fn spill_root() -> std::io::Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no home directory — cannot spill flood-sized tool output",
        )
    })?;
    Ok(home.join(".continuum").join("tool-output"))
}

/// How a tool result reached a mind: the two custody states, and the only text it is handed at
/// execution. A consumer renders these; it never invents a recovery from a raw string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fitted {
    /// Saved whole under `handle` and handed back whole, so it stays reachable after it leaves
    /// view (a settlement collapse or a restore leaves a pointer that resolves, never
    /// "re-run if needed"). What she sees of it is her view's levels, not this seam.
    Complete { text: String, handle: String },
    /// The save failed: `shown` says so and then carries the WHOLE result, whatever its size.
    /// Nothing else holds those bytes, and re-running a tool to recover them is not a recovery
    /// (Astra, 2026-10-12). A failure to save, never a failure of the action; once it leaves
    /// view it cannot be reopened, and the pointer it leaves says exactly that.
    RetentionFailed { shown: String },
}

impl Fitted {
    /// The text handed back, and its typed custody.
    pub fn into_parts(self) -> (String, ResultCustody) {
        match self {
            Fitted::Complete { text, handle } => (text, ResultCustody::Whole { handle }),
            Fitted::RetentionFailed { shown } => (shown, ResultCustody::Unsaved),
        }
    }
}

fn retain_in(root: &Path, persona_id: Uuid, content: &str, pin: &LivePin) -> ResultCustody {
    match pin.save_in(root, persona_id, content) {
        Ok(r) => ResultCustody::Whole { handle: r.handle },
        Err(e) => unretained(content, &e),
    }
}

fn unretained(content: &str, error: &std::io::Error) -> ResultCustody {
    continuum_base::probe!(
        class = "tool.output.unsaved",
        chars = %content.chars().count(),
        error = %error,
        "a batch's rendered result could not be saved; working memory keeps it whole"
    );
    ResultCustody::Unsaved
}

/// Resolve the spill file for `persona_id`/`handle`, rejecting any handle that
/// isn't a bare hex stem — that is the path-traversal guard (no `/`, no `..`, no
/// absolute path can survive the hex-only check), so we never canonicalize or
/// touch a path outside the persona's own spill directory.
pub fn resolve(persona_id: Uuid, handle: &str) -> std::io::Result<PathBuf> {
    resolve_in(&spill_root()?, persona_id, handle)
}

/// THE read of a saved result as her view loads it back: the saved output, trimmed as the
/// record keeps its result (`record_receipt_inner`). Her view's `load` and the memory owner's
/// proof ([`reproduces`]) both read through here, so "verified" means "loads back to this text".
pub fn saved_whole(persona_id: Uuid, handle: &str) -> std::io::Result<String> {
    let text = fs::read_to_string(resolve(persona_id, handle)?)?;
    Ok(text.trim().to_owned())
}

/// Whether `persona_id`'s saved output for `handle` loads back ([`saved_whole`]) to exactly
/// `body`: the proof that the copy can stand in for a body leaving RAM. `Err` when it cannot be
/// read back at all.
pub fn reproduces(persona_id: Uuid, handle: &str, body: &str) -> std::io::Result<bool> {
    Ok(saved_whole(persona_id, handle)? == body)
}

// ── root-injected cores (the filesystem seam; unit-tested against a tempdir so
//    tests never pollute ~/.continuum — the isolation lesson of task #7) ──────

fn persona_dir(root: &Path, persona_id: Uuid) -> PathBuf {
    root.join(persona_id.to_string())
}

/// Hex digits of a content address: 64 bits of SHA-256, an identity's collision space (no
/// two of one persona's saved results collide in practice), not a size of anything she reads.
const ADDRESS_HEX_DIGITS: usize = 16;

/// The content address a result is saved under: the leading [`ADDRESS_HEX_DIGITS`] of its
/// SHA-256, known before any write, which is what lets [`LivePin`] pin a file before it exists.
fn handle_of(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let mut address = format!("{:x}", hasher.finalize());
    address.truncate(ADDRESS_HEX_DIGITS);
    address
}

fn write_in(
    root: &Path,
    persona_id: Uuid,
    handle: &str,
    content: &str,
) -> std::io::Result<PathBuf> {
    let dir = persona_dir(root, persona_id);
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{handle}.log"));
    if existing_matches(&path, content.as_bytes())? {
        return Ok(path);
    }
    use std::io::Write;
    // The staging filename has the SAME stem as the final .log, so the
    // existing content-address pin protects both throughout publication.
    let staging = dir.join(format!("{handle}.{}", Uuid::new_v4()));
    // Do not remove anything if create_new fails: that path is not ours.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staging)?;
    let publish = || -> std::io::Result<()> {
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        drop(file);
        continuum_host::durable::publish_new(&staging, &path)
    };
    if let Err(error) = publish() {
        let _ = fs::remove_file(&staging);
        // Another publisher may have installed the same immutable object.
        // Verify bytes before reusing it; never treat a publication error alone
        // as evidence that the output was retained.
        if error.kind() != std::io::ErrorKind::AlreadyExists {
            return Err(error);
        }
        return match existing_matches(&path, content.as_bytes()) {
            Ok(true) => Ok(path),
            Ok(false) => Err(error),
            Err(verification) => Err(std::io::Error::new(
                error.kind(),
                format!(
                    "publication failed: {error}; existing object verification: {verification}"
                ),
            )),
        };
    }
    let _ = fs::remove_file(&staging);
    Ok(path)
}

/// Existing content addresses are immutable. Compare using the standard reader's
/// bounded buffer; never duplicate a flood-sized result just to deduplicate it.
fn existing_matches(path: &Path, expected: &[u8]) -> std::io::Result<bool> {
    use std::io::BufRead;
    let file = match fs::OpenOptions::new().read(true).write(true).open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    let mismatch = || {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "saved output content address {} contains different bytes; refusing replacement",
                path.display()
            ),
        )
    };
    if file.metadata()?.len() != expected.len() as u64 {
        return Err(mismatch());
    }
    let mut reader = std::io::BufReader::new(file);
    let mut offset = 0;
    loop {
        let bytes = reader.fill_buf()?;
        if bytes.is_empty() {
            break;
        }
        let end = offset + bytes.len();
        if expected.get(offset..end) != Some(bytes) {
            return Err(mismatch());
        }
        offset = end;
        let n = bytes.len();
        reader.consume(n);
    }
    if offset != expected.len() {
        return Err(mismatch());
    }
    continuum_host::durable::fence(reader.get_ref(), path)?;
    Ok(true)
}
fn whole_or_handle_in(root: &Path, full: String, persona_id: Uuid, pin: &LivePin) -> Fitted {
    match pin.save_in(root, persona_id, &full) {
        Ok(r) => Fitted::Complete { text: full, handle: r.handle },
        Err(e) => unsaved_whole(full, &e),
    }
}

/// The text a tool result goes on as. A result object whose bytes are mostly ONE string field
/// (a file's `content`, a shell's `stdout`) is rendered as its scalar fields on one line, then
/// that field's text raw, with its real newlines: what the mind reads, counts and pages by
/// lines is the text itself, not its JSON escape. Any other shape goes on as compact JSON.
pub fn result_text(value: &serde_json::Value) -> String {
    let compact = value.to_string();
    // A result with no text body that holds a collection (an array of cards, of matches) goes on
    // pretty-printed: one element per line, so its summary level shows the first elements and the
    // rest pages by line. Compact, a board of 776 cards was ONE 198k-char line (the M5,
    // 2026-10-16). Its shape decides, never its length: flat scalars stay on one line.
    let pretty_when_long = |v: &serde_json::Value, compact: String| {
        if holds_collection(v) {
            serde_json::to_string_pretty(v).unwrap_or(compact) // unwrap_or: a value that serialized compact serializes pretty; the compact form is the fallback
        } else {
            compact
        }
    };
    let Some(object) = value.as_object() else { return pretty_when_long(value, compact) };
    let Some((body_key, body)) = object
        .iter()
        .filter_map(|(k, v)| v.as_str().map(|s| (k, s)))
        .max_by_key(|(_, s)| s.len())
    else {
        return compact;
    };
    if body.len() * 2 < compact.len() || !body.contains('\n') {
        return pretty_when_long(value, compact);
    }
    let mut head = serde_json::Map::new();
    for (k, v) in object {
        if k != body_key {
            head.insert(k.clone(), v.clone());
        }
    }
    head.insert(body_key.clone(), serde_json::Value::String(format!("<{} lines below>", body.lines().count())));
    format!("{}\n{body}", serde_json::Value::Object(head))
}

/// The scalar fields [`result_text`] put on a result's first line (a file read's
/// `file_path`, `total_lines`, `start_line`…), read back from the text a mind was shown.
/// The header is the result's first line. `None` for a result that went on as something other
/// than one JSON object line (pretty-printed, or not an object).
pub fn result_scalars(shown: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
    let header = shown.split_once('\n').map_or(shown, |(header, _)| header).trim_start(); // map_or: a one-line result is all header
    match serde_json::from_str::<serde_json::Value>(header) {
        Ok(serde_json::Value::Object(map)) => Some(map),
        _ => None,
    }
}

/// Whether a value holds elements that page by line: an array, or an object with a nested
/// array or object (see [`result_text`]).
fn holds_collection(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Array(_) => true,
        serde_json::Value::Object(map) => map.values().any(|v| v.is_array() || v.is_object()),
        _ => false,
    }
}

/// [`Fitted::RetentionFailed`]: the failure said first, then the whole result.
fn unsaved_whole(full: String, error: &std::io::Error) -> Fitted {
    let chars = full.chars().count();
    continuum_base::probe!(
        class = "tool.output.unsaved",
        chars = %chars,
        error = %error,
        "a tool result could not be saved; it goes on whole and cannot be reopened later"
    );
    Fitted::RetentionFailed {
        shown: format!(
            "[saving this {chars}-char result failed ({error}): it is shown whole, and \
             once it leaves view it cannot be reopened]\n{full}"
        ),
    }
}

fn resolve_in(root: &Path, persona_id: Uuid, handle: &str) -> std::io::Result<PathBuf> {
    if handle.is_empty() || !handle.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("invalid output handle '{handle}': expected a hex output id"),
        ));
    }
    Ok(persona_dir(root, persona_id).join(format!("{handle}.log")))
}

/// Outcome of investigating a spilled result — the bounded, line-numbered slice
/// she asked for, plus the scale figures so she can decide whether to narrow.
#[derive(Debug, Clone)]
pub struct Investigation {
    /// Total lines in the full spilled output.
    pub total_lines: usize,
    /// Total bytes in the full spilled output.
    pub total_bytes: usize,
    /// How many lines matched `pattern` across the WHOLE file (pre-cap), so she
    /// knows if there were more hits than rendered. `0` when not grepping.
    pub total_matches: usize,
    /// The rendered, line-numbered selection: grep windows, an explicit range, or the whole
    /// output. Never sized here: her view shows it at the level her window allows, with the
    /// handle that reopens it (`fit_newest_result`).
    pub rendered: String,
    /// `true` when her `max_matches` left matches unreturned.
    pub result_truncated: bool,
    /// First unrendered selected line or first match beyond max_matches; use byteOffset.
    pub next_byte_offset: Option<u64>,
}

/// Read a retained byte extent without consuming a shared cursor: `length` bytes from
/// `offset`, or to the end when she gives no length. No byte is discarded:
/// `next_byte_offset` names the next unread source byte.
pub fn read_extent(path: &Path, offset: u64, length: Option<usize>) -> Result<ByteExtent, String> {
    use base64::Engine;
    let file = fs::File::open(path).map_err(|e| format!("open saved output: {e}"))?;
    let total_bytes = file.metadata().map_err(|e| e.to_string())?.len();
    if offset > total_bytes {
        return Err(format!(
            "byteOffset {offset} exceeds saved length {total_bytes}"
        ));
    }
    if length == Some(0) {
        return Err("byteLength must be positive; omit it to read to the end".into());
    }
    let rest = (total_bytes - offset).min(usize::MAX as u64) as usize;
    let len = length.map_or(rest, |asked| asked.min(rest)); // map_or: no length asked reads to the end
    let mut bytes = vec![0; len];
    continuum_host::positioned::read_exact_at(&file, &mut bytes, offset)
        .map_err(|e| format!("read saved extent at {offset}: {e}"))?;
    let end = offset + len as u64;
    let (content, encoding) = match String::from_utf8(bytes) {
        Ok(text) => (text, "utf8"),
        Err(e) => (
            base64::engine::general_purpose::STANDARD.encode(e.as_bytes()), // tool/output JSON wire: preserve exact non-UTF8 page bytes in its string field
            "base64",
        ),
    };
    Ok(ByteExtent {
        content,
        encoding,
        total_bytes,
        start: offset,
        end,
        next_byte_offset: (end < total_bytes).then_some(end),
    })
}

pub struct ByteExtent {
    pub content: String,
    pub encoding: &'static str,
    pub total_bytes: u64,
    pub start: u64,
    pub end: u64,
    pub next_byte_offset: Option<u64>,
}

/// File-backed investigation. Three streaming passes replace the whole file, line-index and
/// match-index allocations. The count pass examines every line; the other two render the
/// selection with streaming lookahead. A regex operates on a WHOLE line. Scan work is
/// O(file bytes): exact total line/match counts require a full scan. The selection is her
/// query alone; its size is decided by her view, never here.
pub fn investigate_file(
    path: &Path,
    pattern: Option<&str>,
    context_lines: usize,
    range: Option<(usize, usize)>,
    max_matches: Option<usize>,
) -> Result<Investigation, String> {
    let file = fs::File::open(path).map_err(|e| format!("open saved output: {e}"))?;
    let end = file.metadata().map_err(|e| e.to_string())?.len();
    let cursor = || {
        std::io::BufReader::new(ExtentReader {
            file: &file,
            offset: 0,
            end,
        })
    };
    investigate_readers(
        cursor(),
        cursor(),
        cursor(),
        pattern,
        context_lines,
        range,
        max_matches,
    )
}

// Independent positioned cursors over ONE opened file, never try_clone + seek.
// Publication creates an immutable file, so subsequent writes to the same
// content address only verify and reuse this reader's already-open source.
struct ExtentReader<'a> {
    file: &'a fs::File,
    offset: u64,
    end: u64,
}
impl std::io::Read for ExtentReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = buf
            .len()
            .min(self.end.saturating_sub(self.offset).min(usize::MAX as u64) as usize);
        continuum_host::positioned::read_exact_at(self.file, &mut buf[..n], self.offset)?;
        self.offset += n as u64;
        Ok(n)
    }
}
struct Scan<R> {
    reader: R,
    offset: u64,
    line: usize,
}
struct Line {
    text: String,
    offset: u64,
    number: usize,
}
impl<R: std::io::BufRead> Scan<R> {
    fn next(&mut self, retain: bool) -> Result<Option<Line>, String> {
        let offset = self.offset;
        let mut bytes = Vec::new();
        loop {
            let chunk = self
                .reader
                .fill_buf()
                .map_err(|e| format!("scan saved output at byteOffset {}: {e}", self.offset))?;
            if chunk.is_empty() {
                break;
            }
            let newline = chunk.iter().position(|&b| b == b'\n');
            let n = newline.map_or(chunk.len(), |i| i + 1);
            if retain {
                bytes.extend_from_slice(&chunk[..n]);
            }
            self.reader.consume(n);
            self.offset += n as u64;
            if newline.is_some() {
                break;
            }
        }
        if self.offset == offset {
            return Ok(None);
        }
        self.line += 1;
        if bytes.last() == Some(&b'\n') {
            bytes.pop();
            if bytes.last() == Some(&b'\r') {
                bytes.pop();
            }
        }
        let text = String::from_utf8(bytes).map_err(|e| {
            format!(
                "invalid UTF-8 at byteOffset {}: use byte pages; {e}",
                offset + e.utf8_error().valid_up_to() as u64
            )
        })?;
        Ok(Some(Line {
            text,
            offset,
            number: self.line,
        }))
    }
}

fn investigate_readers<R: std::io::BufRead>(
    count: R,
    render: R,
    lookahead: R,
    pattern: Option<&str>,
    context: usize,
    range: Option<(usize, usize)>,
    max_matches: Option<usize>,
) -> Result<Investigation, String> {
    let regex = pattern
        .map(Regex::new)
        .transpose()
        .map_err(|e| format!("invalid search pattern: {e}"))?;
    let scan = |reader| Scan { reader, offset: 0, line: 0 };
    let within = |seen: usize| max_matches.is_none_or(|cap| seen < cap);
    let mut count = scan(count);
    let mut total_matches = 0;
    let mut first_unselected = None;
    while let Some(line) = count.next(regex.is_some())? {
        if regex.as_ref().is_some_and(|r| r.is_match(&line.text)) {
            total_matches += 1;
            if first_unselected.is_none() && !within(total_matches - 1) {
                first_unselected = Some(line.offset);
            }
        }
    }
    let total_lines = count.line;
    let total_bytes = usize::try_from(count.offset)
        .map_err(|_| "saved output length cannot be represented on this platform")?;
    let mut render = scan(render);
    let mut lookahead = scan(lookahead);
    let mut seen_matches = 0;
    let mut next_match = |lookahead: &mut Scan<R>| -> Result<Option<usize>, String> {
        if !within(seen_matches) || regex.is_none() {
            return Ok(None);
        }
        while let Some(line) = lookahead.next(true)? {
            if regex.as_ref().is_some_and(|r| r.is_match(&line.text)) {
                seen_matches += 1;
                return Ok(Some(line.number));
            }
        }
        Ok(None)
    };
    let mut next = next_match(&mut lookahead)?;
    let mut previous = None::<usize>;
    let mut rendered = String::new();
    let mut last_rendered = None::<usize>;
    loop {
        let number = render.line + 1;
        while next.is_some_and(|n| n < number) {
            previous = next;
            next = next_match(&mut lookahead)?;
        }
        let selected = if regex.is_some() {
            previous.is_some_and(|n| n.saturating_add(context) >= number)
                || next.is_some_and(|n| n <= number.saturating_add(context))
        } else if let Some((start, end)) = range {
            number >= start.max(1) && number <= end.max(start.max(1))
        } else {
            true
        };
        // A long unrelated line must not make a later short range/tail unreadable.
        // Counting and skipping use bounded chunks without materializing the line.
        let Some(line) = render.next(selected)? else {
            break;
        };
        if !selected {
            continue;
        }
        let separator = if last_rendered.is_some_and(|n| n + 1 < line.number) {
            "    ⋯\n"
        } else {
            ""
        };
        let mark = if regex.as_ref().is_some_and(|r| r.is_match(&line.text)) {
            '>'
        } else {
            ' '
        };
        rendered.push_str(&format!("{separator}{mark}{:>6} {}\n", line.number, line.text));
        last_rendered = Some(line.number);
    }
    Ok(Investigation {
        total_lines,
        total_bytes,
        total_matches,
        rendered,
        result_truncated: first_unselected.is_some(),
        next_byte_offset: first_unselected,
    })
}

#[cfg(test)]
fn investigate(
    content: &str,
    pattern: Option<&str>,
    context: usize,
    range: Option<(usize, usize)>,
    max_matches: Option<usize>,
) -> Result<Investigation, String> {
    let reader = || std::io::Cursor::new(content.as_bytes());
    investigate_readers(
        reader(),
        reader(),
        reader(),
        pattern,
        context,
        range,
        max_matches,
    )
}

#[cfg(test)]
mod tests;
