//! Tool-output spill: durable recovery for flood-sized tool results.
//!
//! Tier 2 of the three-tier flood protection (tier 1 = the head+tail cap in
//! [`super::command_executor::truncate_tool_output`]; tier 3 = the build hands
//! themselves). When a tool floods — an Xcode/gradle/cargo build log, a giant
//! file read, a chatty command — the cap protects the persona's context window,
//! but a cap alone THROWS AWAY the part she still needs to investigate. So before
//! truncating, the executor spills the WHOLE result to a file and hands back a
//! preview that names a `handle`; she then pages/greps the full output through
//! the `tool/output` command — exactly the bounded-preview + recoverable-on-disk
//! + grep-to-find-it shape Claude Code uses for its own large tool results.
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

/// Spill `content` for `persona_id` and return its [`SpillRef`]. Content-
/// addressed: the handle is derived from the bytes, so re-spilling identical
/// output is idempotent (same file, no duplication).
pub fn spill(persona_id: Uuid, content: &str) -> std::io::Result<SpillRef> {
    spill_in(&spill_root()?, persona_id, content)
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

fn spill_in(root: &Path, persona_id: Uuid, content: &str) -> std::io::Result<SpillRef> {
    let dir = persona_dir(root, persona_id);
    fs::create_dir_all(&dir)?;
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let handle = format!("{:x}", hasher.finalize())[..16].to_string();
    let path = dir.join(format!("{handle}.log"));
    fs::write(&path, content.as_bytes())?;
    Ok(SpillRef {
        handle,
        path,
        bytes: content.len(),
        lines: content.lines().count(),
    })
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

    // what this catches: a round-trip through the root-injected core — spill
    // writes a hex-handled file, resolve points back at it, and the content is
    // byte-identical. Uses a tempdir so it never touches ~/.continuum (task #7).
    #[test]
    fn spill_then_resolve_round_trips() {
        let tmp = std::env::temp_dir().join(format!("spill-test-{}", Uuid::new_v4()));
        let persona = Uuid::new_v4();
        let body = "line one\nline two\nthe error is here\n";
        let r = spill_in(&tmp, persona, body).expect("spill");
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
