//! **The wall clock is read once, by `time::EpochMs`.**
//!
//! Joel, 2026-10-12: "Utils are lazy. Think Java." Measured on canary 73e4ae13a with
//! this rule's own predicate: continuum-core had 176 `SystemTime::now()` chains that
//! reached `UNIX_EPOCH`, 24 `Utc::now().timestamp*` reads and 69 private `now_ms` /
//! `now_millis` / `epoch_ms` functions, across 153 files. Twelve more wrappers used
//! other names. Other modules called persona's private `trace::now_ms` by path 55
//! times. Each copy chose its own meaning for a clock before 1970. One comment declined
//! to write a fourth private copy, because none was importable, and read the clock
//! through chrono instead.
//!
//! `time::EpochMs` owns the read and names each pre-1970 policy (`now`,
//! `now_or_latest`, `read`). This rule keeps every other read at zero, in test code
//! as well as production, because the test copies were copies too.
//!
//! A `SystemTime` VALUE stays legal: comparing a file's mtime against now, setting
//! one, or hashing one into a change fingerprint is not an epoch read.

use super::{SourceFile, SourceRule, Violation};

/// The module that owns the read.
const OWNER: &str = "continuum-base/time/";

/// Lines a `SystemTime::now()` chain may span before its `UNIX_EPOCH`. The longest
/// chain this replaced split its `.duration_since` onto the next line.
const CHAIN_LINES: usize = 3;

pub struct WallClockReadOnlyByEpochMs;

impl SourceRule for WallClockReadOnlyByEpochMs {
    fn name(&self) -> &'static str {
        "wall_clock_is_read_only_by_epoch_ms"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        if file.rel.starts_with(OWNER) {
            return Vec::new();
        }
        let lines: Vec<&str> = file.raw.lines().collect();
        let code = |l: &str| !l.trim_start().starts_with("//");
        let mut out = Vec::new();
        for (i, l) in lines.iter().enumerate() {
            if !code(l) {
                continue;
            }
            let epoch_read = l.contains("SystemTime::now()")
                && lines[i..(i + CHAIN_LINES).min(lines.len())]
                    .iter()
                    .any(|c| code(c) && c.contains("UNIX_EPOCH"));
            let chrono_read = l.contains("Utc::now().timestamp");
            let private_clock = ["fn now_ms(", "fn now_millis(", "fn epoch_ms("]
                .iter()
                .any(|d| l.contains(d))
                || is_wrapper(&lines, i);
            if epoch_read || chrono_read || private_clock {
                out.push(Violation {
                    rule: self.name(),
                    file: file.rel.clone(),
                    line: i + 1,
                    source: l.trim().to_string(),
                });
            }
        }
        out
    }
}

/// A fn whose whole body is a bare `EpochMs` read: a private clock under another name
/// (`now_unix_ms`, `wall_clock_now_ms`, `epoch_secs`, … twelve of them before this rule).
fn is_wrapper(lines: &[&str], i: usize) -> bool {
    let head = lines[i].trim_start();
    let head = head.strip_prefix("pub(crate) ").or_else(|| head.strip_prefix("pub ")).unwrap_or(head); // unwrap_or: no visibility prefix
    if !(head.starts_with("fn ") && head.contains("() -> u64 {")) {
        return false;
    }
    let body: Vec<&str> = lines[i + 1..].iter().take(2).map(|l| l.trim()).collect();
    let read = body.first().map(|l| l.split("//").next().unwrap_or("").trim()); // unwrap_or: split always yields one piece
    let bare = [
        "EpochMs::now().millis()",
        "EpochMs::now().secs()",
        "crate::time::EpochMs::now().millis()",
        "crate::time::EpochMs::now().secs()",
        "continuum_base::time::EpochMs::now().millis()",
        "continuum_base::time::EpochMs::now().secs()",
    ];
    body.len() == 2 && read.is_some_and(|r| bare.contains(&r)) && body[1] == "}"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;

    // what this catches: a new private clock, inline epoch read or chrono millis read
    // anywhere in continuum-core, tests included. Fix it by calling
    // `continuum_base::time::EpochMs::now()` (or `read()` / `now_or_latest()` when a broken
    // clock needs its own policy); never by raising a baseline.
    #[test]
    fn the_wall_clock_is_read_only_by_epoch_ms() {
        // The crate's integration tests too: four of them kept their own clocks after the
        // library moved onto EpochMs, because the scan covered src/ only.
        let mut violations = scan(&[&WallClockReadOnlyByEpochMs]);
        for file in crate::collect_files(&crate::scanned_checkout(), &format!("core/{}/tests", crate::AUDITED_CRATE)) {
            violations.extend(WallClockReadOnlyByEpochMs.check(&file).into_iter().map(|v| Violation {
                file: format!("tests/{}", v.file),
                ..v
            }));
        }
        assert!(
            violations.is_empty(),
            "{} wall-clock reads outside time::EpochMs:\n{}",
            violations.len(),
            violations
                .iter()
                .map(|v| format!("  {}:{} — {}", v.file, v.line, v.source))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    // what this catches: the predicate drifting either way. It must flag a chain split
    // across lines and a private clock, and must leave a SystemTime value, a comment,
    // and the owner alone.
    #[test]
    fn the_predicate_flags_reads_and_spares_values() {
        let rule = WallClockReadOnlyByEpochMs;
        let flagged = |rel: &str, src: &str| rule.check(&SourceFile::for_test(rel, src)).len();
        assert_eq!(flagged("a.rs", "let t = std::time::SystemTime::now()\n    .duration_since(std::time::UNIX_EPOCH)"), 1);
        assert_eq!(flagged("a.rs", "x: chrono::Utc::now().timestamp_millis(),"), 1);
        assert_eq!(flagged("a.rs", "fn now_ms() -> u64 {"), 1);
        assert_eq!(flagged("a.rs", "fn epoch_secs() -> u64 {
    crate::time::EpochMs::now().secs()
}"), 1, "a wrapper");
        assert_eq!(flagged("a.rs", "fn deadline() -> u64 {
    EpochMs::now().millis() + TTL
}"), 0, "a computation, not a wrapper");
        assert_eq!(flagged("a.rs", "e.last_active = SystemTime::now();"), 0, "a SystemTime value");
        assert_eq!(flagged("a.rs", "if let Ok(age) = SystemTime::now().duration_since(snap.captured_at) {"), 0);
        assert_eq!(flagged("a.rs", "// SystemTime::now().duration_since(UNIX_EPOCH)"), 0, "a comment");
        assert_eq!(flagged("continuum-base/time/mod.rs", "Self::of(SystemTime::now())\nUNIX_EPOCH"), 0, "the owner");
    }
}
