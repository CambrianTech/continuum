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

/// Processes above `floor_bytes`, largest first. Bounded: `ps` is killed after
/// `timeout`; a failed or slow read returns an EMPTY list, never a guess.
pub fn residents_above(floor_bytes: u64, timeout: Duration) -> Vec<ResidentProcess> {
    let Ok(mut child) = Command::new("ps")
        .args(["-axo", "rss=,pid=,comm="])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return Vec::new();
    };
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < timeout => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                return Vec::new();
            }
        }
    }
    let Some(mut stdout) = child.stdout.take() else { return Vec::new() };
    let mut text = String::new();
    if std::io::Read::read_to_string(&mut stdout, &mut text).is_err() {
        return Vec::new();
    }
    let mut rows: Vec<ResidentProcess> = parse_ps_rss(&text)
        .into_iter()
        .filter(|p| p.rss_bytes >= floor_bytes)
        .collect();
    rows.sort_by(|a, b| b.rss_bytes.cmp(&a.rss_bytes));
    rows
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
}
