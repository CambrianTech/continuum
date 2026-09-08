//! WHO HOLDS THE MEMORY — a reader that can see processes the in-process scan cannot.
//!
//! `sysinfo`'s per-process `memory()` on macOS reads task info the calling user may not
//! be allowed to see: on 2026-09-08 fseventsd (root) sat at 26.6 GB resident beside a
//! 7 GB model server, `memory.pressure` read Critical every minute, and the anomaly scan
//! named nothing — every root-owned process reported 0. `ps` sees them all. This module
//! is the fallback: one bounded `ps` per anomaly window, parsed by a pure function.

use std::process::Command;
use std::time::Duration;

/// A process holding memory, as `ps -axo rss=,pid=,comm=` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidentProcess {
    pub name: String,
    pub pid: u32,
    pub rss_bytes: u64,
}

/// Parse `ps -axo rss=,pid=,comm=` output (rss in KiB). Malformed rows are skipped.
pub fn parse_ps_rss(out: &str) -> Vec<ResidentProcess> {
    out.lines()
        .filter_map(|line| {
            let mut it = line.split_whitespace();
            let rss_kib: u64 = it.next()?.parse().ok()?;
            let pid: u32 = it.next()?.parse().ok()?;
            let comm = it.collect::<Vec<_>>().join(" ");
            if comm.is_empty() {
                return None;
            }
            let name = comm.rsplit('/').next().unwrap_or(&comm).to_string(); // unwrap_or: rsplit always yields at least the whole string
            Some(ResidentProcess { name, pid, rss_bytes: rss_kib * 1024 })
        })
        .collect()
}

/// Why a `ps` read produced no rows — a failed read is never an empty list (card
/// c7ae34b2: a read that could not complete must not return the empty value of its
/// success type; five instances on 2026-09-08).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PsReadFailed {
    /// `ps` could not be spawned or exited with an error.
    Spawn,
    /// `ps` did not finish inside the bound; the reader thread was abandoned.
    Timeout,
}

impl std::fmt::Display for PsReadFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn => f.write_str("ps could not be run"),
            Self::Timeout => f.write_str("ps did not finish inside the bound"),
        }
    }
}

/// Processes above `floor_bytes`, largest first. Bounded: the read runs on its own
/// thread and is abandoned after `timeout`. A failed or slow read is an `Err` naming
/// why — never an empty list that reads as "nothing to name".
///
/// The first cut polled `try_wait` and read stdout only after exit. `ps -axo` on a
/// developer Mac prints more than the 64 KB pipe buffer (700 processes with full comm
/// paths), so ps blocked on a full pipe, never exited, the 2 s bound killed it, and the
/// monitor named nothing while fseventsd sat at 21 GB (2026-09-08 03:3xZ, batch13).
/// `output()` drains stdout while waiting; the timeout wraps the whole call.
pub fn residents_above(floor_bytes: u64, timeout: Duration) -> Result<Vec<ResidentProcess>, PsReadFailed> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let out = Command::new("ps")
            .args(["-axo", "rss=,pid=,comm="])
            .stderr(std::process::Stdio::null())
            .output();
        let _ = tx.send(out);
    });
    let text = match rx.recv_timeout(timeout) {
        Ok(Ok(out)) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
        Ok(_) => return Err(PsReadFailed::Spawn),
        Err(_) => return Err(PsReadFailed::Timeout),
    };
    let mut rows: Vec<ResidentProcess> = parse_ps_rss(&text)
        .into_iter()
        .filter(|p| p.rss_bytes >= floor_bytes)
        .collect();
    rows.sort_by(|a, b| b.rss_bytes.cmp(&a.rss_bytes));
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the parser losing the process that matters (a root daemon with a
    // path-qualified comm) or reading KiB as bytes — the 26.6 GB fseventsd row of 2026-09-08.
    #[test]
    fn ps_rows_parse_with_paths_and_kib_and_skip_garbage() {
        let out = "27893760 340 /System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/FSEvents.framework/Versions/A/Support/fseventsd\n\
                   7549952 60469 /Users/joel/.continuum/bin/llama-server\n\
                   garbage line\n\
                   1024 1 launchd\n";
        let rows = parse_ps_rss(out);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].name, "fseventsd");
        assert_eq!(rows[0].pid, 340);
        assert_eq!(rows[0].rss_bytes / (1024 * 1024 * 1024), 26, "KiB → bytes: 26 GB, not 26 TB");
        assert_eq!(rows[1].name, "llama-server");
    }

    // what this catches: the reader returning empty on a real machine (the 64 KB pipe
    // deadlock of batch13) — with floor 0 and a generous bound, ps must name at least
    // this test process.
    #[cfg(unix)]
    #[test]
    fn the_real_ps_read_names_this_process() {
        let rows = residents_above(0, Duration::from_secs(5)).expect("ps must run on this box"); // expect: the test's premise
        let me = std::process::id();
        assert!(rows.iter().any(|r| r.pid == me), "ps read returned {} rows, none is pid {me}", rows.len());
    }

}
