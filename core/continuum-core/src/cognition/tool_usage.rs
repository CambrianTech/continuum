//! tool_usage — the miss-tracking instrument. Every tool-call name that flows
//! through the ONE resolution section ([`crate::cognition::tool_dialect::from_wire_name`])
//! is tallied here by outcome: it hit a declared ALIAS, it was our CANONICAL
//! name, or it MISSED (no command answers to it).
//!
//! ## Why (Joel, 2026-07-19)
//!
//! When you kick off a benchmark across a variety of models, the FIRST place to
//! look is: what did they reach for that we don't have? Most misses are a
//! one-minute fix — find the tool, add the alias string (its command owns it now,
//! [[tool-naming-meet-their-training-alias-or-redirect]]), restart. The miss log
//! is also a training signal: it names WHICH model reached wrong, so an exam can
//! drill exactly that. And a first-class citizen (Claude Code, over MCP) gets the
//! same ergonomics — its own fumbles surface here too.
//!
//! ## Shape: bounded, in-memory, resets on deploy
//!
//! A process-global counter keyed by the raw wire name — ONE row per distinct
//! name, so it can't grow unbounded (the disk-governance trap, [[disk-is-a-governed-resource]]).
//! It resets when the core restarts, which is exactly the right granularity: each
//! build/deploy gets a FRESH usage picture, so after you add aliases and reboot,
//! the report reflects the NEW surface, not last session's ghosts (the
//! stale-capture problem that nearly caused a misdiagnosis this same day).

use std::collections::HashMap;
use std::sync::Mutex;

/// How a wire tool-call name resolved through the dialect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Hit a command's declared trained/former alias (`read_file` → `code/read`).
    Alias,
    /// Was our canonical name already (`code/read`, or the charset-legal `code_read`).
    Canonical,
    /// No command answers to it — a miss (alias candidate or a command we lack).
    Miss,
}

/// Per-name tally.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stat {
    pub alias_hits: u64,
    pub canonical: u64,
    pub misses: u64,
}

impl Stat {
    pub fn total(&self) -> u64 {
        self.alias_hits + self.canonical + self.misses
    }
}

static USAGE: Mutex<Option<HashMap<String, Stat>>> = Mutex::new(None);

/// Tally one resolution. Called from the ONE resolution seam so every surface
/// (persona tool-call, CLI, MCP) folds into the same picture. Cheap: a lock + a
/// counter bump, and tool calls are low-frequency (a few per turn).
pub fn record(name: &str, outcome: Outcome) {
    // Under test, recording is a no-op UNLESS a test opted in via `test_recording`.
    // Without this, any test that resolves a tool name (`from_wire_name` records)
    // pollutes the process-global tally in parallel with the tests that ASSERT on
    // it — the shared-global-state trap. Production always records.
    #[cfg(test)]
    if !REC_ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let mut guard = USAGE.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    let stat = map.entry(name.to_string()).or_default();
    match outcome {
        Outcome::Alias => stat.alias_hits += 1,
        Outcome::Canonical => stat.canonical += 1,
        Outcome::Miss => stat.misses += 1,
    }
}

/// A snapshot of the tally since the last deploy, for the `tool/usage` report.
pub fn snapshot() -> Vec<(String, Stat)> {
    USAGE
        .lock()
        .unwrap()
        .as_ref()
        .map(|m| m.iter().map(|(k, v)| (k.clone(), *v)).collect())
        .unwrap_or_default()
}

/// Clear the tally (test isolation + an explicit "start fresh" for a benchmark run).
pub fn reset() {
    *USAGE.lock().unwrap() = None;
}

/// Serializes tests that touch the process-global tally, so one test's `reset`
/// can't wipe another's records mid-assert (the shared-global-state trap).
#[cfg(test)]
static TEST_GUARD: Mutex<()> = Mutex::new(());

/// Whether `record` is live. Off by default under test so a stray `from_wire_name`
/// in a parallel test doesn't pollute the tally; a test opts in via [`test_recording`].
#[cfg(test)]
static REC_ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// RAII test scope: acquires the guard (serializing with any other recording
/// test), enables recording, and starts from a clean tally. On drop it resets and
/// disables — so the global is clean for the next test. Hold it for the whole
/// test body: `let _rec = tool_usage::test_recording();`.
#[cfg(test)]
pub struct TestRecording(#[allow(dead_code)] std::sync::MutexGuard<'static, ()>);

#[cfg(test)]
impl Drop for TestRecording {
    fn drop(&mut self) {
        reset();
        REC_ENABLED.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(test)]
pub fn test_recording() -> TestRecording {
    let guard = TEST_GUARD.lock().unwrap_or_else(|p| p.into_inner());
    REC_ENABLED.store(true, std::sync::atomic::Ordering::Relaxed);
    reset();
    TestRecording(guard)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the recorder tallies by outcome and the snapshot reflects
    // it — the data the `tool/usage` report and any training-exam builder read.
    #[test]
    fn records_and_snapshots_by_outcome() {
        let _rec = test_recording();
        record("read_file", Outcome::Alias);
        record("read_file", Outcome::Alias);
        record("code/read", Outcome::Canonical);
        record("sed_replace", Outcome::Miss);
        let snap: HashMap<String, Stat> = snapshot().into_iter().collect();
        assert_eq!(snap["read_file"].alias_hits, 2);
        assert_eq!(snap["code/read"].canonical, 1);
        assert_eq!(snap["sed_replace"].misses, 1);
        assert_eq!(snap["sed_replace"].total(), 1);
        reset();
        assert!(snapshot().is_empty(), "reset clears the tally");
    }
}
