//! Tool-output spill: durable recovery for flood-sized tool results.
//!
//! [`whole_or_handle`] is THE decision for how a tool result reaches a mind: WHOLE when the
//! window's share for one result holds it, otherwise saved whole here and handed back as a
//! pointer naming its `handle`, never as a fragment. A head, a tail, or a head+tail of a
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

    /// Whole, or saved whole and pointed at: the one place a tool result is fitted to a mind's
    /// window. `share` is the window's share for one result
    /// ([`crate::cognition::context_budget::ContextBudget::latest_action_chars`]), the same share
    /// working memory shows it at, so nothing reaches working memory too big for its view without
    /// a handle. EVERY result is saved, not only the oversized ones: a result that fits now still
    /// leaves view later (a settlement collapse, a restore into a smaller window), and the pointer
    /// it leaves must resolve (Astra on #4963). Saves are content-addressed, so a repeated read
    /// reuses its file, and the directory is evicted under disk pressure like every spill.
    pub fn whole_or_handle(&self, full: String, share: usize, persona_id: Uuid) -> Fitted {
        match spill_root() {
            Ok(root) => whole_or_handle_in(&root, full, share, persona_id, self),
            Err(e) => unsaved_whole(full, share, &e),
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
            crate::probe!(
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

/// How a tool result reached a mind: the three custody states, and the only text a mind is
/// ever shown for one. A consumer renders these; it never invents a recovery from a raw string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fitted {
    /// Within the window's share for one result: the result itself, also saved under
    /// `handle` so it stays reachable after it leaves view (a settlement collapse or a
    /// restore leaves a pointer that resolves, never "re-run if needed").
    Complete { text: String, handle: String },
    /// Over the share and saved whole under `handle`: `shown` is the pointer to it.
    Stored { shown: String, handle: String },
    /// The save failed: `shown` says so and then carries the WHOLE result, whatever its size.
    /// Nothing else holds those bytes, and re-running a tool to recover them is not a recovery
    /// (Astra, 2026-10-12). A failure to save, never a failure of the action; once it leaves
    /// view it cannot be reopened, and the pointer it leaves says exactly that.
    RetentionFailed { shown: String },
}

impl Fitted {
    /// The text the mind is shown, and its typed custody.
    pub fn into_parts(self) -> (String, ResultCustody) {
        match self {
            Fitted::Complete { text, handle } => (text, ResultCustody::Whole { handle }),
            Fitted::Stored { shown, handle } => (shown, ResultCustody::Pointer { handle }),
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
    crate::probe!(
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

// ── root-injected cores (the filesystem seam; unit-tested against a tempdir so
//    tests never pollute ~/.continuum — the isolation lesson of task #7) ──────

fn persona_dir(root: &Path, persona_id: Uuid) -> PathBuf {
    root.join(persona_id.to_string())
}

/// The content address a result is saved under: 16 hex chars of its SHA-256, known before any
/// write, which is what lets [`LivePin`] pin a file before it exists.
fn handle_of(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())[..16].to_string()
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
        crate::fs_portable::durable_publish_new(&staging, &path)
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
    crate::fs_portable::sync_publication(reader.get_ref(), path)?;
    Ok(true)
}
fn whole_or_handle_in(root: &Path, full: String, share: usize, persona_id: Uuid, pin: &LivePin) -> Fitted {
    let chars = full.chars().count();
    match pin.save_in(root, persona_id, &full) {
        Ok(r) if chars <= share => Fitted::Complete {
            text: full,
            handle: r.handle,
        },
        Ok(r) => {
            // The END rides with the pointer, whole: errors and outcomes sit at the end, and a view
            // that stops before the newest line is a world where time never moved (Joel,
            // 2026-10-20). The beginning collapses to its first line and an omitted-range count.
            let (first, tail, tail_lines) = ends_of(&full, share / HEAD_SHARE_DENOM);
            crate::probe!(
                class = "tool.output.spilled",
                handle = %r.handle,
                chars = %chars,
                share = %share,
                tail_lines = %tail_lines,
                "a tool result was larger than the window's share for one result: saved \
                 whole and handed to the mind as its first line, its END and a handle"
            );
            Fitted::Stored {
                shown: format!(
                    "[this result is {} lines ({chars} chars), more than this window's \
                     {share}-char share for one result: its first line and its LAST {tail_lines} \
                     lines are shown below and the WHOLE result is saved as output `{}`. Open the \
                     rest with `tool/output`: a line range with `startLine`/`endLine`, a \
                     `\"pattern\":\"<regex>\"` hunt, or \
                     `{{\"outputId\":\"{}\",\"filter\":\"errors\"}}` (or `warnings`/`failures`/`summary`). \
                     For a file, a `code/read` with `start_line`/`end_line` reads a range directly]\n{first}\n\
                     [lines 2-{} of {} are not shown: open them as above]\n{tail}",
                    r.lines, r.handle, r.handle, r.lines.saturating_sub(tail_lines), r.lines,
                ),
                handle: r.handle,
            }
        }
        Err(e) => unsaved_whole(full, share, &e),
    }
}

/// The text a tool result goes on as. A result object whose bytes are mostly ONE string field
/// (a file's `content`, a shell's `stdout`) is rendered as its scalar fields on one line, then
/// that field's text raw, with its real newlines: what the mind reads, counts and pages by
/// lines is the text itself, not its JSON escape. Any other shape goes on as compact JSON.
pub fn result_text(value: &serde_json::Value) -> String {
    let compact = value.to_string();
    // A long result with no text body (an array of cards, of matches) goes on pretty-printed:
    // one element per lines, so its head shows the first elements and the rest pages by line.
    // Compact, a board of 776 cards was ONE 198k-char line (the M5, 2026-10-16).
    let pretty_when_long = |v: &serde_json::Value, compact: String| {
        if compact.len() > PRETTY_JSON_OVER_CHARS {
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
/// When the result was stored, that text opens with the one-line pointer and the head
/// follows it, so the header is the first or second line. `None` for a result that went
/// on as something other than one JSON object line (pretty-printed, or not an object).
pub fn result_scalars(shown: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
    shown.lines().take(2).find_map(|line| {
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

/// A result with no text body longer than this goes on pretty-printed (see [`result_text`]).
// context-budget-exempt: a readability threshold, not a window bound: compact JSON past a
// couple of thousand chars is one unreadable line at any window
const PRETTY_JSON_OVER_CHARS: usize = 2_000;

/// A spilled result's shown head takes this fraction of the one-result share: the share
/// itself is what the mind's view holds for a result, and the pointer plus a quarter of it
/// leaves the view room for the next result beside it.
const HEAD_SHARE_DENOM: usize = 4;

/// A result's first line (empty when the tail reaches it) and its END: the last whole lines
/// that fit `max_chars` (at least one; a longer last line keeps its END chars), and their count.
fn ends_of(full: &str, max_chars: usize) -> (String, String, usize) {
    let all: Vec<&str> = full.lines().collect();
    let mut tail: Vec<&str> = Vec::new();
    let mut used = 0usize;
    for line in all.iter().rev() {
        let n = line.chars().count() + 1;
        if !tail.is_empty() && used + n > max_chars {
            break;
        }
        tail.push(line);
        used += n;
    }
    tail.reverse();
    let tail_lines = tail.len();
    let mut tail_text = tail.join("\n");
    let len = tail_text.chars().count();
    if tail_lines == 1 && len > max_chars {
        tail_text = tail_text.chars().skip(len - max_chars).collect();
    }
    let first = match all.first() {
        Some(f) if tail_lines < all.len() => f.chars().take(max_chars).collect(),
        _ => String::new(),
    };
    (first, tail_text, tail_lines)
}

/// [`Fitted::RetentionFailed`]: the failure said first, then the whole result.
fn unsaved_whole(full: String, share: usize, error: &std::io::Error) -> Fitted {
    let chars = full.chars().count();
    crate::probe!(
        class = "tool.output.unsaved",
        chars = %chars,
        share = %share,
        error = %error,
        "a tool result could not be saved; it goes on whole and cannot be reopened later"
    );
    let over = if chars > share {
        format!(", over this window's {share}-char share for one result")
    } else {
        String::new()
    };
    Fitted::RetentionFailed {
        shown: format!(
            "[saving this {chars}-char result failed ({error}): it is shown whole{over}, and \
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
    /// The rendered, line-numbered slice (grep windows, an explicit range, or
    /// the tail), already bounded to the char budget.
    pub rendered: String,
    /// `true` when the render allowance or match count leaves content unreturned — a cue
    /// to narrow the pattern or shrink the range.
    pub result_truncated: bool,
    /// First unrendered selected line or first match beyond max_matches; use byteOffset.
    pub next_byte_offset: Option<u64>,
}

/// The default tail size (lines) when neither a pattern nor a range is given —
/// build/test logs put the verdict at the END, so the tail is the useful default.
const DEFAULT_TAIL_LINES: usize = 80;

/// Read a retained byte extent without consuming a shared cursor. `budget` bounds
/// the encoded result, including base64 expansion if the extent is not UTF-8.
/// No byte is discarded: `next_byte_offset` names the next unread source byte.
pub fn read_extent(
    path: &Path,
    offset: u64,
    length: Option<usize>,
    budget: usize,
) -> Result<ByteExtent, String> {
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
        .unwrap_or(capacity)
        .min(capacity)
        .min((total_bytes - offset).min(usize::MAX as u64) as usize);
    let mut bytes = vec![0; len];
    crate::fs_portable::read_exact_at(&file, &mut bytes, offset)
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
    range: Option<(usize, usize)>,
    max_matches: usize,
    budget: usize,
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
        budget,
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
        crate::fs_portable::read_exact_at(self.file, &mut buf[..n], self.offset)?;
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
                    return Err(format!("line {} at byteOffset {offset} exceeds scan allowance {} bytes; use tool/output with byteOffset {offset} for exact byte pages (regex was not evaluated on a partial line)", self.line + 1, self.allowance));
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
    range: Option<(usize, usize)>,
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
    while let Some(line) = count.next(regex.is_some())? {
        if regex.as_ref().is_some_and(|r| r.is_match(&line.text)) {
            total_matches += 1;
            if total_matches == max_matches.saturating_add(1) {
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
    if pattern.is_none() && range.is_none() {
        rendered = format!("(no pattern or range given — showing the last {} lines; grep with a `pattern` like \"error\" or \"panic\" to find a specific failure)\n", total_lines.min(DEFAULT_TAIL_LINES));
        if rendered.len() > budget {
            rendered.clear();
        }
    }
    let mut last_rendered = None::<usize>;
    let mut next_byte_offset = first_unselected;
    let mut result_truncated = first_unselected.is_some();
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
            number > total_lines.saturating_sub(DEFAULT_TAIL_LINES)
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
    range: Option<(usize, usize)>,
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
#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (Codex on #4982): the restore retain loop untested, a restored
    // handle that does not resolve to the exact bytes the mind saw, a failed save that drops
    // the text or leaves a pin, and a new file visible to eviction before it is pinned.
    #[test]
    fn a_restore_saves_handleless_results_exactly_and_pins_them_until_published() {
        let tmp = std::env::temp_dir().join(format!("spill-restore-{}", Uuid::new_v4()));
        let persona = Uuid::new_v4();
        let mut results = vec![
            ("first result\nline two".to_string(), None),
            ("already saved".to_string(), Some("feedfacefeedface".to_string())),
            ("third result".to_string(), None),
        ];
        let pin = LivePin::new();
        retain_handleless_with(|t| retain_in(&tmp, persona, t, &pin), results.iter_mut().map(|(t, h)| (t.as_str(), h)));
        for (text, handle) in [&results[0], &results[2]] {
            let handle = handle.as_deref().expect("a handle-less result is saved");
            assert_eq!(std::fs::read_to_string(resolve_in(&tmp, persona, handle).unwrap()).unwrap(), *text, "byte-identical");
        }
        assert_eq!(results[1].1.as_deref(), Some("feedfacefeedface"), "a saved result keeps its handle");
        let live: std::collections::HashSet<String> = live_handles().lock().get(&pin.0).cloned().unwrap_or_default();
        assert!(results.iter().filter(|(t, _)| t != "already saved").all(|(_, h)| live.contains(h.as_ref().unwrap())), "each is pinned as it is saved");
        let holder = pin.0;
        drop(pin);
        assert!(!live_handles().lock().contains_key(&holder), "the pin withdraws once the memory has published");

        // A root that is a FILE: every save fails, and the text stays whole with no handle.
        let blocked = std::env::temp_dir().join(format!("spill-blocked-{}", Uuid::new_v4()));
        std::fs::write(&blocked, b"not a directory").unwrap();
        let mut failed = vec![("kept whole".to_string(), None::<String>)];
        let failing = LivePin::new();
        retain_handleless_with(|t| retain_in(&blocked, persona, t, &failing), failed.iter_mut().map(|(t, h)| (t.as_str(), h)));
        assert_eq!(failed[0], ("kept whole".to_string(), None), "a failed save keeps its text and claims no handle");
        assert!(live_handles().lock().get(&failing.0).is_none_or(|set| set.is_empty()), "and pins nothing");

        // THE ORDER (Codex on #4982): the handle is live before its file exists, under the lock
        // eviction reads, so no evictor can see the new file unpinned.
        let ordered = LivePin::new();
        let handle = handle_of("pinned first");
        assert!(ordered.hold(&handle), "entered");
        let path = persona_dir(&tmp, persona).join(format!("{handle}.log"));
        assert!(!path.exists() && live_handles().lock().values().any(|set| set.contains(&handle)), "live before written");
        assert!(matches!(retain_in(&tmp, persona, "pinned first", &ordered), ResultCustody::Whole { .. }) && path.exists());

        // THE INTERLEAVING (Codex on #4982): a content-addressed file that already exists is
        // never deleted while pinned. Eviction decides and deletes in one critical section, so a
        // pin cannot enter between them: inside it, the lock a pin needs is held.
        use crate::paging::filesystem_tier_pool::Removal;
        assert!(matches!(evict_unless_live(&path), Removal::Protected) && path.exists(), "pinned: protected");
        drop(ordered);
        let removal = evict_unless_live_with(&path, || {
            assert!(live_handles().try_lock().is_none(), "no pin can enter between the check and the delete");
        });
        assert!(matches!(removal, Removal::Removed) && !path.exists(), "unpinned: removed");
        let again = LivePin::new();
        assert!(matches!(retain_in(&tmp, persona, "pinned first", &again), ResultCustody::Whole { .. }) && path.exists(), "a pin after the delete rewrites the file");
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_file(&blocked);
    }

    // what this catches: the head+tail fold (the executor's tier 1 until 2026-10-12) and
    // every other fragment. A result within its share is returned byte-identical; one over
    // it is saved whole, resolvable by the returned handle, and the text shown carries no
    // piece of it, neither the head nor the tail.
    #[test]
    fn a_result_is_whole_or_a_handle_never_a_fragment() {
        let tmp = std::env::temp_dir().join(format!("spill-test-{}", Uuid::new_v4()));
        let persona = Uuid::new_v4();
        let small = "ok: 2 passed".to_string();
        let Fitted::Complete { text, handle } = whole_or_handle_in(&tmp, small.clone(), 600, persona, &LivePin::new())
        else {
            panic!("a fitting result is complete")
        };
        assert_eq!(text, small, "shown byte-identical");
        let path = resolve_in(&tmp, persona, &handle).expect("resolve");
        assert_eq!(std::fs::read_to_string(path).unwrap(), small, "and saved, so it stays reachable");

        let body = format!("HEAD-MARK\n{}TAIL-MARK error: boom\n", "compiling module\n".repeat(5000));
        let Fitted::Stored { shown, handle } = whole_or_handle_in(&tmp, body.clone(), 600, persona, &LivePin::new())
        else {
            panic!("an over-share result is saved")
        };
        // the END rides along whole (the newest line is never dropped), the first line names what
        // it was, and the pointer names the unseen range, so a fragment is never passed off as
        // the whole
        assert!(shown.contains("TAIL-MARK error: boom") && shown.contains("HEAD-MARK"), "first line + whole end: {shown}");
        assert!(shown.contains("are not shown"), "the unseen range is named: {shown}");
        assert!(shown.contains(&handle) && shown.contains("tool/output"), "names the way in: {shown}");
        assert!(shown.contains("\"filter\":\"errors\""), "points at the failure filter: {shown}");
        let path = resolve_in(&tmp, persona, &handle).expect("resolve");
        assert_eq!(std::fs::read_to_string(path).unwrap(), body, "saved byte-identical");

        // A failed save keeps the WHOLE result and says why: nothing else holds those bytes.
        let blocked = tmp.join("not-a-dir");
        std::fs::write(&blocked, b"file where the persona dir must go").unwrap();
        let unsaved_body = "UNSAVED-HEAD ".repeat(100);
        let Fitted::RetentionFailed { shown } =
            whole_or_handle_in(&blocked, unsaved_body.clone(), 64, persona, &LivePin::new())
        else {
            panic!("a failed save is its own state")
        };
        assert!(shown.contains("saving this") && shown.ends_with(&unsaved_body), "kept whole: {shown}");
        let Fitted::RetentionFailed { shown } =
            whole_or_handle_in(&blocked, "ok".to_string(), 64, persona, &LivePin::new())
        else {
            panic!("a fitting result whose save failed says so too")
        };
        assert!(shown.contains("cannot be reopened") && shown.ends_with("ok"), "{shown}");

        // Multibyte bodies measure in chars, the unit the share is in, and never split: the
        // head of a one-line body is cut at a CHAR boundary, at most a quarter of the share.
        let (shown, _) = whole_or_handle_in(&tmp, "αβγδε".repeat(2000), 64, persona, &LivePin::new()).into_parts();
        let greek = shown.chars().filter(|c| "αβγδε".contains(*c)).count();
        assert!(greek > 0 && greek <= 64 / HEAD_SHARE_DENOM, "a char-bounded end: {greek} chars in {shown}");
        std::fs::remove_dir_all(&tmp).ok();
    }

    // what this catches: a multi-call batch that one call's handle claimed to cover (Astra
    // on #4963). The batch's rendered text, two distinct results inside it, is saved as one
    // unit and resolves byte-identical, both results included.
    #[test]
    fn a_batch_is_retained_whole_both_results_byte_identical() {
        let tmp = std::env::temp_dir().join(format!("spill-test-{}", Uuid::new_v4()));
        let persona = Uuid::new_v4();
        let batch = "code/read(a.rs)\nResult:\nfn a() {}\n\ncode/run(t)\nResult:\nerror[E0601]\n";
        let ResultCustody::Whole { handle } = retain_in(&tmp, persona, batch, &LivePin::new()) else {
            panic!("the batch is saved")
        };
        let saved = std::fs::read_to_string(resolve_in(&tmp, persona, &handle).unwrap()).unwrap();
        assert_eq!(saved, batch, "both results, byte-identical");
        let blocked = tmp.join("not-a-dir");
        std::fs::write(&blocked, b"x").unwrap();
        assert_eq!(retain_in(&blocked, persona, batch, &LivePin::new()), ResultCustody::Unsaved);
        std::fs::remove_dir_all(&tmp).ok();
    }

    // what this catches: eviction erasing evidence a mind still points at (Astra on #4963).
    // A published handle is live; withdrawing the holder releases it; a path that is not a
    // spill file is never mistaken for one.
    #[test]
    fn a_referenced_handle_is_live_until_its_holder_withdraws() {
        let holder = u64::MAX - 7;
        let handle = format!("{:016x}", Uuid::new_v4().as_u128() as u64);
        let path = std::path::PathBuf::from(format!("/spill/p/{handle}.log"));
        assert!(!is_live(&path));
        publish_live_handles(holder, [handle.clone()].into_iter().collect());
        assert!(is_live(&path));
        withdraw_live_handles(holder);
        assert!(!is_live(&path));
        assert!(!is_live(std::path::Path::new("/")));
    }

    // what this catches: a round-trip through the root-injected core — spill
    // writes a hex-handled file, resolve points back at it, and the content is
    // byte-identical. Uses a tempdir so it never touches ~/.continuum (task #7).
    #[test]
    fn spill_then_resolve_round_trips() {
        let tmp = std::env::temp_dir().join(format!("spill-test-{}", Uuid::new_v4()));
        let persona = Uuid::new_v4();
        let body = "line one\nline two\nthe error is here\n";
        let r = LivePin::new().save_in(&tmp, persona, body).expect("spill");
        assert_eq!(r.handle.len(), 16);
        assert!(r.handle.chars().all(|c| c.is_ascii_hexdigit()));
        let path = resolve_in(&tmp, persona, &r.handle).expect("resolve");
        assert_eq!(path, r.path);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), body);
        std::fs::remove_dir_all(&tmp).ok();
    }

    // what this catches: the path-traversal guard. A handle with a slash, a
    // `..`, or a non-hex char must be rejected so a persona can never escape its
    // own spill directory by crafting a handle.
    #[test]
    fn resolve_rejects_non_hex_handles() {
        let tmp = std::env::temp_dir().join("spill-guard");
        let persona = Uuid::new_v4();
        for bad in ["../secret", "a/b", "..", "deadbeefZZ", ""] {
            assert!(
                resolve_in(&tmp, persona, bad).is_err(),
                "must reject handle {bad:?}"
            );
        }
        // a well-formed hex handle is accepted (existence is the reader's concern)
        assert!(resolve_in(&tmp, persona, "deadbeefcafe0000").is_ok());
    }

    // what this catches: grep finds the failure line and carries context around
    // it — the core "find the error in the flood" affordance. The matched line
    // gets the `>` gutter; a neighbor rides along as context.
    #[test]
    fn investigate_greps_the_error_with_context() {
        let body = (1..=200)
            .map(|n| {
                if n == 137 {
                    "error[E0432]: unresolved import".to_string()
                } else {
                    format!("noise line {n}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let inv = investigate(&body, Some("error\\["), 1, None, 50, 8000).expect("ok");
        assert_eq!(inv.total_matches, 1);
        assert!(inv.rendered.contains("error[E0432]"));
        assert!(
            inv.rendered.contains(">   137"),
            "matched line is gutter-marked"
        );
        assert!(
            inv.rendered.contains("136"),
            "context line above is present"
        );
        assert_eq!(inv.total_lines, 200);
    }

    // what this catches: a bad regex fails LOUD naming the pattern, never a
    // silent empty result that reads as "no error found".
    #[test]
    fn investigate_bad_regex_is_a_loud_error() {
        let err = investigate("x", Some("("), 0, None, 10, 100).unwrap_err();
        assert!(err.contains("invalid search pattern"));
    }

    // what this catches: with neither pattern nor range, the tail is returned
    // (the verdict lives at the end of a build/test log) with a nudge to grep.
    #[test]
    fn investigate_defaults_to_the_tail() {
        let body = (1..=500)
            .map(|n| format!("row {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let inv = investigate(&body, None, 0, None, 50, 8000).expect("ok");
        assert!(inv.rendered.contains("row 500"), "tail present");
        assert!(!inv.rendered.contains("row 1\n"), "head dropped");
        assert!(
            inv.rendered.contains("grep with a `pattern`"),
            "nudges narrowing"
        );
    }

    // what this catches: an explicit line range reads exactly that slice,
    // 1-based and clamped to the file.
    #[test]
    fn investigate_reads_an_explicit_range() {
        let body = (1..=100)
            .map(|n| format!("L{n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let inv = investigate(&body, None, 0, Some((10, 12)), 50, 8000).expect("ok");
        assert!(inv.rendered.contains("    10 L10"));
        assert!(inv.rendered.contains("    12 L12"));
        assert!(!inv.rendered.contains("L13"));
        assert!(!inv.rendered.contains("L9\n"));
    }

    // what this catches: the rendered slice is itself bounded so investigating
    // can't re-flood the context the spill was meant to protect.
    #[test]
    fn investigate_bounds_its_own_output() {
        let body = (1..=10000)
            .map(|n| format!("matchme {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let inv = investigate(&body, Some("matchme"), 0, None, 10000, 500).expect("ok");
        assert!(inv.result_truncated);
        assert!(inv.rendered.len() <= 600, "bounded near the budget");
        assert!(inv.next_byte_offset.is_some());
    }
    // what this catches: independent cursors retain one opened source across
    // publication at the same path, and all continue from their own byte offsets.
    #[test]
    fn positioned_investigation_keeps_one_source_across_reuse() {
        use std::io::Read;
        let dir = tempfile::tempdir().unwrap();
        let persona = Uuid::new_v4();
        let handle = "deadbeef";
        let path = write_in(dir.path(), persona, handle, "original\n").unwrap();
        let file = fs::File::open(&path).unwrap();
        let mut first = ExtentReader {
            file: &file,
            offset: 0,
            end: 9,
        };
        let mut second = ExtentReader {
            file: &file,
            offset: 0,
            end: 9,
        };
        let mut bytes = [0; 3];
        first.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"ori");
        write_in(dir.path(), persona, handle, "original\n").unwrap();
        assert_eq!(
            write_in(dir.path(), persona, handle, "replacement\n")
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::InvalidData
        );
        second.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"ori");
        first.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"gin");
        assert_eq!(fs::read_to_string(&path).unwrap(), "original\n");
    }

    // what this catches: unselected huge lines do not block a small range or
    // tail; regex still refuses to misrepresent a partial-line search as complete.
    #[test]
    fn long_unselected_lines_are_counted_without_materialization() {
        let body = format!("{}\n{}", "x".repeat(10000), "short\n".repeat(100));
        let range = investigate(&body, None, 0, Some((2, 3)), 50, 256).unwrap();
        assert_eq!(range.total_lines, 101);
        assert_eq!(range.total_bytes, body.len());
        assert!(range.rendered.contains("     2 short"));
        let tail = investigate(&body, None, 0, None, 50, 256).unwrap();
        assert!(tail.rendered.contains("short"));
        assert!(investigate(&body, Some("short"), 0, None, 50, 256)
            .unwrap_err()
            .contains("byteOffset 0"));
    }

    // what this catches: the existing pin/remover contract protects staging as
    // well as published bytes, including publication followed by a holder release.
    #[test]
    fn staging_and_published_objects_share_the_existing_pin() {
        use crate::paging::filesystem_tier_pool::Removal;
        let dir = tempfile::tempdir().unwrap();
        let persona = Uuid::new_v4();
        let parent = persona_dir(dir.path(), persona);
        fs::create_dir_all(&parent).unwrap();
        let handle = "deadbeef";
        let pin = LivePin::new();
        pin.hold(handle);
        let staging = parent.join(format!("{handle}.{}", Uuid::new_v4()));
        fs::write(&staging, "retained").unwrap();
        assert!(matches!(evict_unless_live(&staging), Removal::Protected));
        let published = parent.join(format!("{handle}.log"));
        crate::fs_portable::durable_publish_new(&staging, &published).unwrap();
        let _ = fs::remove_file(&staging);
        assert!(matches!(evict_unless_live(&published), Removal::Protected));
        drop(pin);
        let _ = evict_unless_live(&published);
        assert!(!published.exists());
    }

    // Regression for #5036: a failed publisher leaves visible bytes, not a
    // durability receipt. Actual spill reuse must independently complete the fence.
    #[test]
    fn visible_publication_requires_a_successful_reuse_fence() {
        let dir = tempfile::tempdir().unwrap();
        let persona = Uuid::new_v4();
        let body = "retained output\n";
        let handle = handle_of(body);
        let pin = LivePin::new();
        pin.hold(&handle);
        let path = persona_dir(dir.path(), persona).join(format!("{handle}.log"));
        crate::fs_portable::tests::with_failed_publication_fence(|| {
            assert!(write_in(dir.path(), persona, &handle, body).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), body);
            // The object is already visible; byte equality must not bypass the fence.
            assert!(write_in(dir.path(), persona, &handle, body).is_err());
        });
        assert_eq!(write_in(dir.path(), persona, &handle, body).unwrap(), path);
    }

    #[cfg(feature = "stress-tests")]
    mod stress {
        use super::*;

        // Competing writers must converge on the same complete immutable object,
        // and neither publisher may leave a staging object behind.
        #[test]
        fn concurrent_duplicate_publishers_reuse_one_object() {
            let dir = tempfile::tempdir().unwrap();
            let persona = Uuid::new_v4();
            let content = "shared result\n".repeat(8192);
            let handle = handle_of(&content);
            let pin = LivePin::new();
            pin.hold(&handle);
            let ready = std::sync::Barrier::new(2);
            std::thread::scope(|scope| {
                let publish = || {
                    ready.wait();
                    write_in(dir.path(), persona, &handle, &content).unwrap()
                };
                let first = scope.spawn(publish);
                let second = scope.spawn(publish);
                let path = first.join().unwrap();
                assert_eq!(path, second.join().unwrap());
                assert_eq!(fs::read_to_string(path).unwrap(), content);
            });
            assert_eq!(fs::read_dir(persona_dir(dir.path(), persona)).unwrap().count(), 1);
        }
    }
}
