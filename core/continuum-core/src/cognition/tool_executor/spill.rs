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
//! The investigate logic ([`investigate`]) is what makes "find the error in the
//! crap" first-class: a regex grep with context lines (the failure-hunting path)
//! plus explicit line-range paging. It is a pure function over the spilled text
//! so it unit-tests without the command framework or the filesystem.

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

fn write_in(root: &Path, persona_id: Uuid, handle: &str, content: &str) -> std::io::Result<PathBuf> {
    let dir = persona_dir(root, persona_id);
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{handle}.log"));
    fs::write(&path, content.as_bytes())?;
    Ok(path)
}

fn whole_or_handle_in(root: &Path, full: String, share: usize, persona_id: Uuid, pin: &LivePin) -> Fitted {
    let chars = full.chars().count();
    match pin.save_in(root, persona_id, &full) {
        Ok(r) if chars <= share => Fitted::Complete {
            text: full,
            handle: r.handle,
        },
        Ok(r) => {
            crate::probe!(
                class = "tool.output.spilled",
                handle = %r.handle,
                chars = %chars,
                share = %share,
                "a tool result was larger than the window's share for one result: saved \
                 whole and handed to the mind as its handle"
            );
            Fitted::Stored {
                shown: format!(
                    "[this result is {} lines ({chars} chars), more than this window's \
                     {share}-char share for one result, so it is not shown in part. The whole \
                     result is saved as output `{}`: open what you need with `tool/output`. \
                     Jump straight to what broke with `{{\"outputId\":\"{}\",\"filter\":\"errors\"}}` \
                     (or `warnings`/`failures`/`summary`), hunt with `\"pattern\":\"<regex>\"`, or \
                     read a line range with `startLine`/`endLine`]",
                    r.lines, r.handle, r.handle,
                ),
                handle: r.handle,
            }
        }
        Err(e) => unsaved_whole(full, share, &e),
    }
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
    /// `true` when the rendered slice itself hit the budget and was cut — a cue
    /// to narrow the pattern or shrink the range.
    pub result_truncated: bool,
}

/// The default tail size (lines) when neither a pattern nor a range is given —
/// build/test logs put the verdict at the END, so the tail is the useful default.
const DEFAULT_TAIL_LINES: usize = 80;

/// Investigate spilled `content`: grep with `pattern` (+ `context_lines` of
/// surrounding context, the failure-hunting path), OR read an explicit
/// `[start, end]` 1-based line `range`, OR (neither) return the tail. `max_matches`
/// caps grep windows; `budget` bounds the rendered output in chars so the result
/// can't re-flood the context it was meant to protect.
///
/// A bad regex is a loud error (the caller's pattern is wrong; naming it lets her
/// fix it), never a silent empty result.
pub fn investigate(
    content: &str,
    pattern: Option<&str>,
    context_lines: usize,
    range: Option<(usize, usize)>,
    max_matches: usize,
    budget: usize,
) -> Result<Investigation, String> {
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();
    let total_bytes = content.len();

    let (rendered_raw, total_matches) = if let Some(pat) = pattern {
        let re = Regex::new(pat).map_err(|e| format!("invalid search pattern `{pat}`: {e}"))?;
        let matches: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| re.is_match(l))
            .map(|(i, _)| i)
            .collect();
        let windows = merge_windows(&matches, context_lines, total_lines, max_matches);
        (render_windows(&lines, &windows, &matches), matches.len())
    } else if let Some((start, end)) = range {
        // 1-based inclusive, clamped to the file.
        let s = start.max(1);
        let e = end.min(total_lines).max(s);
        let idxs: Vec<usize> = (s - 1..e).collect();
        (render_lines(&lines, &idxs, &[]), 0)
    } else {
        // No pattern, no range: the tail (where the verdict lives).
        let s = total_lines.saturating_sub(DEFAULT_TAIL_LINES);
        let idxs: Vec<usize> = (s..total_lines).collect();
        let head = format!(
            "(no pattern or range given — showing the last {} lines; grep with a \
             `pattern` like \"error\" or \"panic\" to find a specific failure)\n",
            idxs.len()
        );
        (format!("{head}{}", render_lines(&lines, &idxs, &[])), 0)
    };

    let (rendered, result_truncated) = bound(rendered_raw, budget);
    Ok(Investigation {
        total_lines,
        total_bytes,
        total_matches,
        rendered,
        result_truncated,
    })
}

/// Merge match line-indices into deduped, sorted [start, end] windows of
/// `±context` lines, capped to the windows around the first `max_matches` hits.
fn merge_windows(
    matches: &[usize],
    context: usize,
    total: usize,
    max_matches: usize,
) -> Vec<(usize, usize)> {
    let mut windows: Vec<(usize, usize)> = Vec::new();
    for &m in matches.iter().take(max_matches) {
        let start = m.saturating_sub(context);
        let end = (m + context).min(total.saturating_sub(1));
        match windows.last_mut() {
            // Overlapping or adjacent → extend the open window.
            Some(last) if start <= last.1 + 1 => last.1 = last.1.max(end),
            _ => windows.push((start, end)),
        }
    }
    windows
}

/// Render a set of windows with line numbers, `>` marking matched lines and a
/// `⋯` separator between non-contiguous windows.
fn render_windows(lines: &[&str], windows: &[(usize, usize)], matches: &[usize]) -> String {
    let mut out = String::new();
    for (wi, &(start, end)) in windows.iter().enumerate() {
        if wi > 0 {
            out.push_str("    ⋯\n");
        }
        let idxs: Vec<usize> = (start..=end).collect();
        out.push_str(&render_lines(lines, &idxs, matches));
    }
    out
}

/// Render the given 0-based line indices with 1-based numbers; lines whose index
/// is in `highlight` get a `>` gutter, the rest a space.
fn render_lines(lines: &[&str], idxs: &[usize], highlight: &[usize]) -> String {
    let mut out = String::new();
    for &i in idxs {
        let Some(line) = lines.get(i) else { continue };
        let mark = if highlight.contains(&i) { '>' } else { ' ' };
        out.push_str(&format!("{mark}{:>6} {line}\n", i + 1));
    }
    out
}

/// Bound `s` to `max` bytes on a char boundary, returning (bounded, was_cut).
fn bound(s: String, max: usize) -> (String, bool) {
    if s.len() <= max {
        return (s, false);
    }
    let mut end = max.min(s.len());
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    let mut t = s[..end].to_string();
    t.push_str("\n…[more — narrow the pattern or range]");
    (t, true)
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
        assert!(!shown.contains("HEAD-MARK") && !shown.contains("TAIL-MARK"), "no fragment: {shown}");
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

        // Multibyte bodies measure in chars, the unit the share is in, and never split.
        let (shown, _) = whole_or_handle_in(&tmp, "αβγδε".repeat(2000), 64, persona, &LivePin::new()).into_parts();
        assert!(!shown.contains('α'));
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
        assert!(inv.rendered.contains("narrow"));
    }
}
