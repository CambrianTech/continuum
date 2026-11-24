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
use continuum_kernel::system_resources::disk_reporters::ClassWriter;
use uuid::Uuid;

use crate::ai::types::ResultCustody;
pub use super::output_id::OutputId;

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
        match store() {
            Ok(store) => whole_or_handle_in(&store, full, persona_id, self),
            Err(e) => unsaved_whole(full, &e),
        }
    }

    /// Save `content` exactly as given and name its custody: `Whole{handle}` (it resolves
    /// byte-identical through `tool/output`), or `Unsaved` when the save failed (probed).
    /// Working memory's results ring keeps a batch's rendered text; this saves THAT text, so a
    /// pointer the ring leaves later resolves to what the mind saw, every call of the batch
    /// included (Astra on #4963).
    pub fn retain(&self, persona_id: Uuid, content: &str) -> ResultCustody {
        match store() {
            Ok(store) => retain_in(&store, persona_id, content, self),
            Err(e) => unretained(content, &e),
        }
    }

    /// THE save: content-addressed, pinned before it is written, unpinned again if the write
    /// fails (only an entry this save added; an identical result this pin already holds stays).
    fn save_in(&self, store: &ClassWriter, persona_id: Uuid, content: &str) -> std::io::Result<SpillRef> {
        let handle = handle_of(content);
        let entered = self.hold(&handle);
        match write_in(store, persona_id, &handle, content) {
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
    match store() {
        Ok(store) => retain_handleless_with(|text| retain_in(&store, persona_id, text, &pin), results),
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
    // A staging file is a save in flight (`ClassWriter::publish_new` stages beside its target),
    // which no content address names yet, or one a crashed life left. Written in this life, it may
    // be mid-save: protected. Older than this life's first save, it is a leftover: evicted.
    if path.file_name().is_some_and(continuum_host::durable::is_staging_name) {
        let this_life = LIFE_START.get().is_some_and(|start| fs::metadata(path).and_then(|meta| meta.modified()).is_ok_and(|written| written >= *start));
        return if this_life { Removal::Protected } else { fs::remove_file(path).into() };
    }
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

/// The disk class every tool result is saved in (`standard_tracked_dirs`): a measured root with
/// an eviction owner, resolved from the boot registry at each use.
const CLASS: &str = "tool-output";

/// The writer of the tool-output class. Refused before boot installs the registry: there is no
/// directory to guess, and a result that cannot be saved is handed back whole as `Unsaved`.
pub(crate) fn store() -> std::io::Result<ClassWriter> {
    LIFE_START.get_or_init(std::time::SystemTime::now);
    ClassWriter::of(CLASS)
}

/// When this life first reached the store: a staging file written since may be a save in flight.
static LIFE_START: std::sync::OnceLock<std::time::SystemTime> = std::sync::OnceLock::new();

/// The tool-output class's root, for the pressure pool that evicts it (`pressure_broker_wiring`):
/// one source for the path, the class row's.
pub fn spill_root() -> std::io::Result<PathBuf> {
    continuum_kernel::system_resources::disk_reporters::tracked_dir(CLASS)
        .map(|dir| dir.path().to_path_buf())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, format!("disk class {CLASS} is not registered")))
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

fn retain_in(store: &ClassWriter, persona_id: Uuid, content: &str, pin: &LivePin) -> ResultCustody {
    match pin.save_in(store, persona_id, content) {
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

/// Resolve the spill file for `persona_id`/`handle`, rejecting any handle that is not an
/// [`OutputId`] (the path-traversal guard is that type's parse).
pub fn resolve(persona_id: Uuid, handle: &str) -> std::io::Result<PathBuf> {
    resolve_in(&store()?, persona_id, handle)
}

/// The spill file of `persona_id`'s output `id`: an id already parsed, so nothing to refuse but
/// the store itself.
pub fn resolve_output(persona_id: Uuid, id: &OutputId) -> std::io::Result<PathBuf> {
    resolve_id_in(&store()?, persona_id, id)
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

// ── store-injected cores (the class seam; unit-tested against a class rooted in a tempdir
//    so tests never pollute ~/.continuum — the isolation lesson of task #7) ──────

/// The content address a result is saved under ([`ContentHash::address`]), known before any
/// write, which is what lets [`LivePin`] pin a file before it exists.
///
/// [`ContentHash::address`]: continuum_context::cause::ContentHash::address
fn handle_of(content: &str) -> String {
    OutputId::of(content).into()
}

/// A result's file name in its persona's directory of the class.
fn file_name(handle: &str) -> PathBuf {
    PathBuf::from(format!("{handle}.log"))
}

/// Publish `content` under `handle` in `persona_id`'s directory of the class, once: an object
/// already there is reused only after its bytes are verified and its fence completed, never
/// replaced.
fn write_in(store: &ClassWriter, persona_id: Uuid, handle: &str, content: &str) -> std::io::Result<PathBuf> {
    let mine = store.owned_by(persona_id);
    let name = file_name(handle);
    let path = mine.path(&name)?;
    if existing_matches(&name, &path, content.as_bytes())? {
        return Ok(path);
    }
    match mine.publish_new(&name, content.as_bytes()) {
        Ok(()) => Ok(path),
        // Another publisher may have installed the same immutable object. Verify bytes before
        // reusing it; never treat a publication error alone as evidence that the output was
        // retained.
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => match existing_matches(&name, &path, content.as_bytes()) {
            Ok(true) => Ok(path),
            Ok(false) => Err(error),
            Err(verification) => Err(std::io::Error::new(
                error.kind(),
                format!("publication failed: {error}; existing object verification: {verification}"),
            )),
        },
        Err(error) => Err(error),
    }
}

/// Existing content addresses are immutable. Compare using the standard reader's bounded buffer;
/// never duplicate a flood-sized result just to deduplicate it. A match is reused only once its
/// durability fence is complete (#5036: visible bytes are not a durability receipt).
fn existing_matches(name: &Path, path: &Path, expected: &[u8]) -> std::io::Result<bool> {
    use std::io::BufRead;
    let file = match fs::File::open(path) {
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
    // The fence of a name another publisher placed, completed in its own held directory (the
    // durable owner's), before the bytes are trusted as retained.
    let dir = path.parent().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("{} has no directory", path.display())))?;
    continuum_host::durable::fence_existing(&continuum_host::confine::open_root(dir)?, name)?;
    Ok(true)
}

fn whole_or_handle_in(store: &ClassWriter, full: String, persona_id: Uuid, pin: &LivePin) -> Fitted {
    match pin.save_in(store, persona_id, &full) {
        Ok(r) => Fitted::Complete { text: full, handle: r.handle },
        Err(e) => unsaved_whole(full, &e),
    }
}

/// The text a tool result goes on as. A result object whose bytes are mostly ONE string field
/// (a file's `content`, a shell's `stdout`) is rendered as its scalar fields on one line, then
/// that field's text raw, with its real newlines: what the mind reads, counts and pages by
/// lines is the text itself, not its JSON escape. A result that holds structure (a list of cards,
/// of matches) goes on one field and one record per line ([`structured_lines`]). Anything else
/// goes on as compact JSON.
pub fn result_text(value: &serde_json::Value) -> String {
    let compact = value.to_string();
    let Some(object) = value.as_object() else { return structured_lines(value).unwrap_or(compact) }; // unwrap_or: a value with no structure is its own compact line
    let body = object.iter().filter_map(|(k, v)| v.as_str().map(|s| (k, s))).max_by_key(|(_, s)| s.len());
    let Some((body_key, body)) = body.filter(|(_, body)| body.len() * 2 >= compact.len() && body.contains('\n')) else {
        return structured_lines(value).unwrap_or(compact); // unwrap_or: a value with no structure is its own compact line
    };
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
/// The header has two places, named, never a count of lines: a result handed back whole opens
/// with it, and a stored head (`ResultCustody::Pointer`) opens with its one-line pointer and then
/// the header. `None` for a result that went on as something other than one JSON object line
/// (one record per line, or not an object).
pub fn result_scalars(shown: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
    let (opening, rest) = shown.split_once('\n').unwrap_or((shown, "")); // unwrap_or: a one-line result is all opening
    let after_pointer = rest.split('\n').next();
    std::iter::once(opening).chain(after_pointer).find_map(|line| {
        let line = line.trim_start();
        if !line.starts_with('{') {
            return None;
        }
        match serde_json::from_str::<serde_json::Value>(line) {
            Ok(serde_json::Value::Object(map)) => Some(map),
            _ => None,
        }
    })
}

/// A value that holds structure, one field and one element per line: an object's fields each on a
/// line, an array's elements (records, or scalars such as paths and ids) one compact element per
/// line. A SHAPE rule, never a size: a line is then one card, one match or one path, so a level of
/// the result keeps whole elements and the rest pages by line (compact, a board of 776 cards was
/// ONE 198k-char line she could read none of, the M5, 2026-10-16). `None` for a value with no
/// structure inside (scalars only): its compact line is already its smallest legible form.
fn structured_lines(value: &serde_json::Value) -> Option<String> {
    use serde_json::Value;
    let structured = |v: &Value| v.is_object() || v.is_array();
    let records = |items: &[Value]| -> String {
        let lines: Vec<String> = items.iter().map(Value::to_string).collect();
        format!("[\n{}\n]", lines.join(",\n"))
    };
    match value {
        Value::Array(items) if !items.is_empty() => Some(records(items)),
        Value::Object(map) if map.values().any(structured) => {
            let fields: Vec<String> = map
                .iter()
                .map(|(key, field)| {
                    let rendered = match field {
                        Value::Array(items) if !items.is_empty() => records(items),
                        other => other.to_string(),
                    };
                    format!("{}:{rendered}", Value::String(key.clone()))
                })
                .collect();
            Some(format!("{{\n{}\n}}", fields.join(",\n")))
        }
        _ => None,
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

fn resolve_in(store: &ClassWriter, persona_id: Uuid, handle: &str) -> std::io::Result<PathBuf> {
    let id = OutputId::parse(handle).map_err(|refused| std::io::Error::new(std::io::ErrorKind::InvalidInput, refused))?;
    resolve_id_in(store, persona_id, &id)
}

fn resolve_id_in(store: &ClassWriter, persona_id: Uuid, id: &OutputId) -> std::io::Result<PathBuf> {
    store.owned_by(persona_id).path(&file_name(id.as_str()))
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
    /// The rendered, line-numbered slice (grep windows, an explicit range, or
    /// the tail), already bounded to the char budget.
    pub rendered: String,
    /// `true` when the render allowance or match count leaves content unreturned — a cue
    /// to narrow the pattern or shrink the range.
    pub result_truncated: bool,
    /// First unrendered selected line or first match beyond max_matches; use byteOffset.
    pub next_byte_offset: Option<u64>,
}

/// The tail a read with neither pattern nor range shows (build/test logs put the verdict at the
/// END): the newest lines whose rows fit the read's budget, measured in the count pass as each
/// line is counted, never a count of lines. Holds only the lines that could still fit, so its
/// memory is the budget's, not the file's.
#[derive(Default)]
struct TailFit {
    /// The newest lines that fit so far: each line's number and the bytes its row can take.
    rows: std::collections::VecDeque<(usize, usize)>,
    bytes: usize,
    /// The newest line counted, by its byte offset: where she reads on when not even it fits.
    newest_offset: Option<u64>,
}

/// Where a tail read starts.
enum Tail {
    /// From this line to the last.
    From(usize),
    /// Not even the newest line fits the budget: she reads it by bytes from its offset.
    NewestTooLong { offset: u64 },
}

impl TailFit {
    /// Count line `number` at byte `offset`, of `line_bytes` (its newline included), keeping the
    /// newest run within `budget`. A row is the line's text, its gutter number (six columns or
    /// more), a mark, a space and a newline: at most its bytes plus its number's width plus three.
    fn push(&mut self, number: usize, offset: u64, line_bytes: usize, budget: usize) {
        let row = line_bytes + number.to_string().len().max(6) + 3;
        self.rows.push_back((number, row));
        self.bytes += row;
        self.newest_offset = Some(offset);
        self.shed(budget);
    }

    fn shed(&mut self, room: usize) {
        while self.bytes > room {
            let Some((_, row)) = self.rows.pop_front() else { break };
            self.bytes -= row;
        }
    }

    fn lines(&self) -> usize {
        self.rows.len()
    }

    /// Where the tail that fits `room` starts. When not even the newest line does, the read says
    /// so with that line's offset: her newest end is never dropped silently. An empty output has
    /// no line to start from, so its tail is past its end.
    fn fit(&mut self, room: usize, total_lines: usize) -> Tail {
        self.shed(room);
        match (self.rows.front(), self.newest_offset) {
            (Some(&(number, _)), _) => Tail::From(number),
            (None, Some(offset)) => Tail::NewestTooLong { offset },
            (None, None) => Tail::From(total_lines.saturating_add(1)),
        }
    }
}

/// How much a read of a saved output may return. `Whole`: what was asked, bounded only by the
/// saved file itself (it was held in memory once, when saved); her view sizes what she sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Allowance {
    Whole,
    Chars(usize),
}

impl Allowance {
    fn chars(self) -> usize {
        match self { Allowance::Whole => usize::MAX, Allowance::Chars(chars) => chars } // Whole: no cap of the read's own; the file bounds it
    }
}

/// Read a retained byte extent without consuming a shared cursor. `allowance` bounds
/// the encoded result, including base64 expansion if the extent is not UTF-8.
/// No byte is discarded: `next_byte_offset` names the next unread source byte.
pub fn read_extent(path: &Path, offset: u64, length: Option<usize>, allowance: Allowance) -> Result<ByteExtent, String> {
    let budget = allowance.chars();
    use base64::Engine;
    let file = fs::File::open(path).map_err(|e| format!("open saved output: {e}"))?;
    let total_bytes = file.metadata().map_err(|e| e.to_string())?.len();
    if offset > total_bytes {
        return Err(format!(
            "byteOffset {offset} exceeds saved length {total_bytes}"
        ));
    }
    let capacity = (budget / 4) * 3; // exact worst-case base64 expansion, not a content clip
    if capacity == 0 || length == Some(0) {
        return Err(
            "byte retrieval needs a positive length and an encoding budget of at least four bytes"
                .into(),
        );
    }
    let len = length
        .unwrap_or(capacity) // unwrap_or: no length asked for reads as much as the encoding budget holds
        .min(capacity)
        .min((total_bytes - offset).min(usize::MAX as u64) as usize);
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

/// The lines a range read selects (1-based): from `start` through `end`, or through the output's
/// last line when `end` is `None` (an end she left out is the output's own, never a guess).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRange {
    pub start: usize,
    pub end: Option<usize>,
}

impl LineRange {
    fn holds(self, number: usize) -> bool {
        let start = self.start.max(1);
        number >= start && self.end.is_none_or(|end| number <= end.max(start))
    }
}

/// File-backed investigation. Three bounded streaming passes replace the whole
/// file, line-index and match-index allocations. The count pass examines every
/// line; the other two render the requested windows with streaming lookahead.
/// A regex operates on a WHOLE line. A line too large for the scan allowance is
/// refused with its byte position, never searched as a misleading fragment.
/// Scan work is O(file bytes), independent of the returned-output allowance:
/// exact total line/match counts require a full scan. Memory is O(budget).
pub fn investigate_file(
    path: &Path,
    pattern: Option<&str>,
    context_lines: usize,
    range: Option<LineRange>,
    max_matches: Option<usize>,
    allowance: Allowance,
) -> Result<Investigation, String> {
    let file = fs::File::open(path).map_err(|e| format!("open saved output: {e}"))?;
    let end = file.metadata().map_err(|e| e.to_string())?.len();
    let cursor = || std::io::BufReader::new(ExtentReader { file: &file, offset: 0, end });
    let max_matches = max_matches.unwrap_or(usize::MAX); // unwrap_or: none asked for = every match
    investigate_readers(cursor(), cursor(), cursor(), pattern, context_lines, range, max_matches, allowance.chars())
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
    allowance: usize,
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
                if n > self.allowance.saturating_sub(bytes.len()) {
                    return Err(format!("line {} at byteOffset {offset} exceeds scan allowance {} bytes; use {} with byteOffset {offset} for exact byte pages (regex was not evaluated on a partial line)", self.line + 1, self.allowance, crate::cognition::tool_executor::tool_output::TOOL_OUTPUT));
                }
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
    range: Option<LineRange>,
    max_matches: usize,
    budget: usize,
) -> Result<Investigation, String> {
    if budget == 0 {
        return Err("investigation needs a positive scan and render allowance".into());
    }
    let regex = pattern
        .map(Regex::new)
        .transpose()
        .map_err(|e| format!("invalid search pattern: {e}"))?;
    let scan = |reader| Scan {
        reader,
        offset: 0,
        line: 0,
        allowance: budget,
    };
    let mut count = scan(count);
    let mut total_matches = 0;
    let mut first_unselected = None;
    let tail = pattern.is_none() && range.is_none();
    let mut tail_fit = TailFit::default();
    while let Some(line) = count.next(regex.is_some())? {
        if regex.as_ref().is_some_and(|r| r.is_match(&line.text)) {
            total_matches += 1;
            if total_matches == max_matches.saturating_add(1) {
                first_unselected = Some(line.offset);
            }
        }
        if tail {
            tail_fit.push(line.number, line.offset, (count.offset - line.offset) as usize, budget);
        }
    }
    let total_lines = count.line;
    let total_bytes = usize::try_from(count.offset)
        .map_err(|_| "saved output length cannot be represented on this platform")?;
    let mut render = scan(render);
    let mut lookahead = scan(lookahead);
    let mut seen_matches = 0;
    let mut next_match = |lookahead: &mut Scan<R>| -> Result<Option<usize>, String> {
        if seen_matches >= max_matches || regex.is_none() {
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
    // The tail: the newest lines this read's budget holds beside its header, never a count.
    let mut next_byte_offset = first_unselected;
    let mut result_truncated = first_unselected.is_some();
    let tail_from = if tail {
        let header = |lines: usize| format!("(no pattern or range given — showing the newest {lines} of {total_lines} lines; grep with a `pattern` like \"error\" or \"panic\" to find a specific failure)\n");
        let from = match tail_fit.fit(budget.saturating_sub(header(tail_fit.lines()).len()), total_lines) {
            Tail::From(number) => {
                rendered = header(tail_fit.lines());
                number
            }
            Tail::NewestTooLong { offset } => {
                rendered = format!("(no pattern or range given — the newest line alone is longer than this read's budget; read it by bytes from byteOffset {offset})\n");
                result_truncated = true;
                next_byte_offset = Some(offset);
                total_lines.saturating_add(1)
            }
        };
        if rendered.len() > budget {
            rendered.clear();
        }
        from
    } else {
        total_lines.saturating_add(1)
    };
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
        } else if let Some(range) = range {
            range.holds(number)
        } else {
            number >= tail_from
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
        let row = format!("{separator}{mark}{:>6} {}\n", line.number, line.text);
        if row.len() > budget.saturating_sub(rendered.len()) {
            result_truncated = true;
            next_byte_offset = Some(next_byte_offset.map_or(line.offset, |n| n.min(line.offset)));
            break;
        }
        rendered.push_str(&row);
        last_rendered = Some(line.number);
    }
    Ok(Investigation {
        total_lines,
        total_bytes,
        total_matches,
        rendered,
        result_truncated,
        next_byte_offset,
    })
}

#[cfg(test)]
fn investigate(
    content: &str,
    pattern: Option<&str>,
    context: usize,
    range: Option<LineRange>,
    max_matches: usize,
    budget: usize,
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
        budget,
    )
}

/// The store's class for a test that saves through the production path ([`LivePin`], working
/// memory, the executor): the kernel's writer fixture, the real class layout rooted in the
/// fixture's own temporary home, so no test writes into `~/.continuum`. Callable from a sync test
/// or inside an async one: the fixture is prepared on its own thread, on a runtime that lives for
/// the process (the fixture's store outlives the test that first asked for it).
#[cfg(test)]
pub(crate) fn prepare_store_fixture() {
    static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    let runtime = RUNTIME.get_or_init(|| tokio::runtime::Runtime::new().expect("test: the store fixture's runtime"));
    std::thread::scope(|scope| {
        scope
            .spawn(|| runtime.block_on(continuum_kernel::system_resources::disk_reporters::prepare_writer_fixture()))
            .join()
            .expect("test: the store fixture prepares");
    });
}

#[cfg(test)]
mod tests;
