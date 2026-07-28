//! Persona-RAID slice 1 — the write-behind journal tee.
//!
//! Design: docs/architecture/PERSONA-RAID-WRITE-BEHIND.md (M5-approved shape).
//! Every DURABLE memory admit (the `persist_memory` funnel — the one place a
//! persona's truth reaches `longterm.db`) is teed into an append-only
//! per-persona `journal.jsonl` next to that db. The journal is the unit of
//! truth-in-motion: replayable (idempotent by record id, newest-wins) and
//! shippable (slice 2's RTOS shipper reads the tail past the peer's acked seq).
//!
//! Slice-1 contract:
//!  - Tee is BEST-EFFORT-LOUD: a journal failure warns + counts (visible in the
//!    snapshot) but NEVER errors the admit — the durable store already holds
//!    the truth; the journal is the replication leg, and a lying "replicated"
//!    is worse than an honest "degraded".
//!  - `seq` is monotonic per (persona, origin_node); recovered from the last
//!    journal line on first touch after a restart, so restarts never fork the
//!    sequence ([[restarts-are-commonplace]]).
//!  - No shipping, no background task yet — `snapshot()` is the observability
//!    surface (`degraded_unreplicated: true` until slice 2 lands a peer ack).

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

/// One journaled durable admit. `record` is the exact JSON that went to the
/// durable store (embedding included) — replay re-issues it verbatim.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub seq: u64,
    pub persona_id: String,
    pub origin_node: String,
    pub ts_ms: u64,
    /// Admission kind — "memory" today; future durable kinds name themselves.
    pub kind: String,
    pub record: serde_json::Value,
}

/// Point-in-time replication state for one persona — the slice-1 observability
/// surface (slice 2's shipper publishes this via a `watch` channel instead).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationSnapshot {
    pub persona_id: String,
    pub last_seq: u64,
    pub last_ts_ms: u64,
    pub journal_bytes: u64,
    /// Admits that failed to journal since boot (loud degradation counter).
    pub dropped: u64,
    /// True until a peer has acked a shipped batch (slice 2). An honest flag:
    /// this persona's memory currently lives on ONE machine.
    pub degraded_unreplicated: bool,
}

struct JournalHandle {
    path: PathBuf,
    seq: u64,
    bytes: u64,
    dropped: u64,
    last_ts_ms: u64,
}

static JOURNALS: OnceLock<Mutex<HashMap<String, JournalHandle>>> = OnceLock::new();

fn journals() -> &'static Mutex<HashMap<String, JournalHandle>> {
    JOURNALS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// This node's stable name for journal provenance. Hostname is enough for
/// slice 1 (the grid's peer_id joins in slice 2's ship envelope).
fn origin_node() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown-node".to_string())
}

/// `~/.continuum/personas/<id>/journal.jsonl` — sibling of the persona's
/// `longterm.db` data dir. Bare id (any `@persona:` prefix stripped) matches
/// the on-disk persona dir naming.
fn journal_path_for(persona_id: &str) -> Option<PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()?;
    let bare = persona_id.strip_prefix("@persona:").unwrap_or(persona_id);
    Some(
        PathBuf::from(home)
            .join(".continuum/personas")
            .join(bare)
            .join("journal.jsonl"),
    )
}

/// Recover the last seq from an existing journal so a restart continues the
/// sequence instead of forking it. O(file) once per persona per boot.
fn recover_seq(path: &PathBuf) -> (u64, u64, u64) {
    let Ok(f) = fs::File::open(path) else {
        return (0, 0, 0);
    };
    let bytes = f.metadata().map(|m| m.len()).unwrap_or(0);
    let mut last_seq = 0;
    let mut last_ts = 0;
    for line in BufReader::new(f).lines().map_while(Result::ok) {
        if let Ok(e) = serde_json::from_str::<JournalEntry>(&line) {
            last_seq = e.seq;
            last_ts = e.ts_ms;
        }
    }
    (last_seq, last_ts, bytes)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Tee one durable admit into the persona's journal. Called from the
/// `persist_memory` funnel AFTER the durable store accepted the write.
/// Never fails the caller — failures warn + count in the snapshot.
pub fn journal_admit(persona_id: &str, kind: &str, record: &serde_json::Value) {
    let mut map = match journals().lock() {
        Ok(g) => g,
        Err(_) => return, // poisoned lock: journaling stands down, admit proceeds
    };
    let handle = match map.get_mut(persona_id) {
        Some(h) => h,
        None => {
            let Some(path) = journal_path_for(persona_id) else {
                return;
            };
            let (seq, last_ts_ms, bytes) = recover_seq(&path);
            map.insert(
                persona_id.to_string(),
                JournalHandle { path, seq, bytes, dropped: 0, last_ts_ms },
            );
            map.get_mut(persona_id).expect("just inserted")
        }
    };
    let entry = JournalEntry {
        seq: handle.seq + 1,
        persona_id: persona_id.to_string(),
        origin_node: origin_node(),
        ts_ms: now_ms(),
        kind: kind.to_string(),
        record: record.clone(),
    };
    let appended = (|| -> std::io::Result<u64> {
        if let Some(dir) = handle.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut f = OpenOptions::new().create(true).append(true).open(&handle.path)?;
        let line = serde_json::to_string(&entry)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        f.write_all(line.as_bytes())?;
        f.write_all(b"\n")?;
        Ok(line.len() as u64 + 1)
    })();
    match appended {
        Ok(n) => {
            handle.seq = entry.seq;
            handle.bytes += n;
            handle.last_ts_ms = entry.ts_ms;
        }
        Err(e) => {
            handle.dropped += 1;
            tracing::warn!(
                persona_id,
                dropped = handle.dropped,
                "persona-RAID journal append failed (admit unaffected; replication degraded): {e}"
            );
        }
    }
}

/// The slice-1 observability read: this persona's replication state, or None
/// if nothing has been journaled (or recovered) since boot.
pub fn snapshot(persona_id: &str) -> Option<ReplicationSnapshot> {
    let map = journals().lock().ok()?;
    let h = map.get(persona_id)?;
    Some(ReplicationSnapshot {
        persona_id: persona_id.to_string(),
        last_seq: h.seq,
        last_ts_ms: h.last_ts_ms,
        journal_bytes: h.bytes,
        dropped: h.dropped,
        degraded_unreplicated: true, // honest until slice 2 lands a peer ack
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_temp_home<T>(f: impl FnOnce() -> T) -> T {
        // Serialize env mutation across tests touching HOME.
        static ENV_LOCK: Mutex<()> = Mutex::new(());
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = std::env::temp_dir().join(format!("raid-test-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let old = std::env::var("HOME").ok();
        std::env::set_var("HOME", &tmp);
        let out = f();
        match old {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
        out
    }

    // what this catches: the journal must be monotonic and replayable — an
    // entry that round-trips lossily or a seq that repeats would corrupt
    // resume-on-spawn's newest-wins replay.
    #[test]
    fn journal_appends_monotonic_and_round_trips() {
        with_temp_home(|| {
            let pid = "raid-test-persona-a";
            journal_admit(pid, "memory", &serde_json::json!({"id": "m1", "text": "hello"}));
            journal_admit(pid, "memory", &serde_json::json!({"id": "m2", "text": "world"}));
            let snap = snapshot(pid).expect("snapshot after admits");
            assert_eq!(snap.last_seq, 2);
            assert_eq!(snap.dropped, 0);
            assert!(snap.degraded_unreplicated);
            let path = journal_path_for(pid).unwrap();
            let lines: Vec<JournalEntry> = std::io::BufRead::lines(std::io::BufReader::new(
                std::fs::File::open(&path).unwrap(),
            ))
            .map(|l| serde_json::from_str(&l.unwrap()).unwrap())
            .collect();
            assert_eq!(lines.len(), 2);
            assert_eq!(lines[0].seq, 1);
            assert_eq!(lines[1].seq, 2);
            assert_eq!(lines[1].record["id"], "m2");
        });
    }

    // what this catches: a restart must CONTINUE the sequence from the file,
    // not fork it back to 1 — forked seqs make newest-wins ambiguous and
    // silently lose memories on replay ([[restarts-are-commonplace]]).
    #[test]
    fn restart_recovers_seq_from_disk() {
        with_temp_home(|| {
            let pid = "raid-test-persona-b";
            journal_admit(pid, "memory", &serde_json::json!({"id": "m1"}));
            // Simulate restart: drop the in-memory handle, keeping the file.
            journals().lock().unwrap().remove(pid);
            journal_admit(pid, "memory", &serde_json::json!({"id": "m2"}));
            let snap = snapshot(pid).expect("snapshot after recovered admit");
            assert_eq!(snap.last_seq, 2, "seq must continue across restart, not fork");
        });
    }
}
