//! Persona-RAID — the write-behind journal + replica cold store.
//!
//! Design: docs/architecture/PERSONA-RAID-WRITE-BEHIND.md (M5-approved shape).
//! Slice 1: every DURABLE memory admit (the `persist_memory` funnel — the one
//! place a persona's truth reaches `longterm.db`) is teed into an append-only
//! per-persona `journal.jsonl`. Slice 2a: `memory/replicate-batch` lands a peer
//! shipper's journal tail into this node's replica store and acks high-water.
//!
//! Shape: ONE owned [`ReplicationLedger`] held by `MemoryState` — no statics,
//! no ambient globals (CONCURRENCY-STYLE-GUIDE). Roots are constructor-injected
//! so tests build against temp dirs directly. The slice-2b shipper will be the
//! RTOS-shape consumer (own task + interval + `watch` snapshot) reading this
//! ledger; until its first peer ack, snapshots honestly report
//! `degraded_unreplicated: true`.
//!
//! Contracts:
//!  - Tee is BEST-EFFORT-LOUD: a journal failure warns + counts (visible in the
//!    snapshot) but NEVER errors the admit — the durable store already holds
//!    the truth; a lying "replicated" is worse than an honest "degraded".
//!  - `seq` is monotonic per (persona, origin_node); recovered from the last
//!    journal line on first touch after a restart, so restarts never fork the
//!    sequence ([[restarts-are-commonplace]]).
//!  - The replica store is idempotent by (origin_node, seq) high-water — the
//!    shipper retries blindly on a lost ack.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// One journaled durable admit. `record` is the exact JSON that went to the
/// durable store (embedding included) — replay re-issues it verbatim.
/// Crosses the wire in `memory/replicate-batch`, hence the schema derives.
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/memory/JournalEntry.ts")]
pub struct JournalEntry {
    pub seq: u64,
    pub persona_id: String,
    pub origin_node: String,
    pub ts_ms: u64,
    /// Admission kind — "memory" today; future durable kinds name themselves.
    pub kind: String,
    pub record: serde_json::Value,
}

/// Point-in-time replication state for one persona — the observability
/// surface (slice 2b's shipper publishes this via a `watch` channel).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationSnapshot {
    pub persona_id: String,
    pub last_seq: u64,
    pub last_ts_ms: u64,
    pub journal_bytes: u64,
    /// Admits that failed to journal since boot (loud degradation counter).
    pub dropped: u64,
    /// True until a peer has acked a shipped batch (slice 2b). An honest flag:
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

/// The one owner of persona-RAID state on a node: the outbound journals (this
/// node's personas) and the inbound replica high-waters (other nodes' personas
/// backed up here). Held by `MemoryState`; everything is instance state.
pub struct ReplicationLedger {
    origin_node: String,
    /// `<personas_root>/<persona>/journal.jsonl` — sibling of longterm.db.
    personas_root: Option<PathBuf>,
    /// `<replicas_root>/<persona>/journal.jsonl` — peers' memory held here.
    replicas_root: Option<PathBuf>,
    journals: Mutex<HashMap<String, JournalHandle>>,
    replica_hw: Mutex<HashMap<(String, String), u64>>,
}

impl ReplicationLedger {
    /// Construct with explicit roots — the testable constructor.
    pub fn new(personas_root: PathBuf, replicas_root: PathBuf, origin_node: String) -> Self {
        Self {
            origin_node,
            personas_root: Some(personas_root),
            replicas_root: Some(replicas_root),
            journals: Mutex::new(HashMap::new()),
            replica_hw: Mutex::new(HashMap::new()),
        }
    }

    /// Production constructor: roots under the user home, origin from the
    /// machine name. A homeless environment yields a ledger that WARNS and
    /// counts every admit as dropped (loud degradation, never a panic on the
    /// boot path).
    pub fn from_env() -> Self {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .ok()
            .map(PathBuf::from);
        let origin = std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "unknown-node".to_string());
        match home {
            Some(h) => Self::new(
                h.join(".continuum/personas"),
                h.join(".continuum/replicas"),
                origin,
            ),
            None => Self {
                origin_node: origin,
                personas_root: None,
                replicas_root: None,
                journals: Mutex::new(HashMap::new()),
                replica_hw: Mutex::new(HashMap::new()),
            },
        }
    }

    fn journal_path_for(&self, persona_id: &str) -> Option<PathBuf> {
        let bare = persona_id.strip_prefix("@persona:").unwrap_or(persona_id);
        Some(self.personas_root.as_ref()?.join(bare).join("journal.jsonl"))
    }

    fn replica_path_for(&self, persona_id: &str) -> Option<PathBuf> {
        let bare = persona_id.strip_prefix("@persona:").unwrap_or(persona_id);
        Some(self.replicas_root.as_ref()?.join(bare).join("journal.jsonl"))
    }

    /// Tee one durable admit into the persona's journal. Called from the
    /// `persist_memory` funnel AFTER the durable store accepted the write.
    /// Never fails the caller — failures warn + count in the snapshot.
    pub fn journal_admit(&self, persona_id: &str, kind: &str, record: &serde_json::Value) {
        let mut map = match self.journals.lock() {
            Ok(g) => g,
            Err(_) => return, // poisoned: journaling stands down, admit proceeds
        };
        if !map.contains_key(persona_id) {
            let Some(path) = self.journal_path_for(persona_id) else {
                tracing::warn!(
                    persona_id,
                    "persona-RAID: no home dir — journaling disabled, replication degraded"
                );
                return;
            };
            let (seq, last_ts_ms, bytes) = recover_seq(&path);
            map.insert(
                persona_id.to_string(),
                JournalHandle { path, seq, bytes, dropped: 0, last_ts_ms },
            );
        }
        let handle = map.get_mut(persona_id).expect("inserted above");
        let entry = JournalEntry {
            seq: handle.seq + 1,
            persona_id: persona_id.to_string(),
            origin_node: self.origin_node.clone(),
            ts_ms: now_ms(),
            kind: kind.to_string(),
            record: record.clone(),
        };
        match append_line(&handle.path, &entry) {
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

    /// This persona's replication state, or None if nothing journaled since boot.
    pub fn snapshot(&self, persona_id: &str) -> Option<ReplicationSnapshot> {
        let map = self.journals.lock().ok()?;
        let h = map.get(persona_id)?;
        Some(ReplicationSnapshot {
            persona_id: persona_id.to_string(),
            last_seq: h.seq,
            last_ts_ms: h.last_ts_ms,
            journal_bytes: h.bytes,
            dropped: h.dropped,
            degraded_unreplicated: true, // honest until slice 2b lands a peer ack
        })
    }

    /// Append a shipped batch to this node's replica journal for `persona_id`.
    /// All entries must share ONE origin_node (a batch is one shipper's tail).
    /// Entries at or below the current high-water are skipped (idempotent
    /// retry). Returns the acked high-water for that (persona, origin).
    pub fn replica_append_batch(
        &self,
        persona_id: &str,
        entries: &[JournalEntry],
    ) -> Result<u64, String> {
        let Some(first) = entries.first() else {
            // A batch names its origin via its entries; an empty one can't be
            // acked meaningfully. The shipper never sends empty — fail loud.
            return Err("empty replicate batch".into());
        };
        let origin = first.origin_node.clone();
        if entries.iter().any(|e| e.origin_node != origin) {
            return Err("mixed origin_node in one batch — a batch is one shipper's tail".into());
        }
        let path = self
            .replica_path_for(persona_id)
            .ok_or("no home dir for replica store")?;
        let mut map = self.replica_hw.lock().map_err(|e| e.to_string())?;
        let key = (persona_id.to_string(), origin);
        if !map.contains_key(&key) {
            recover_replica_hw(persona_id, &path, &mut map);
        }
        let hw = map.entry(key).or_insert(0);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        for e in entries {
            if e.seq <= *hw {
                continue; // already replicated — idempotent retry
            }
            let line = serde_json::to_string(e).map_err(|e| e.to_string())?;
            f.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
            f.write_all(b"\n").map_err(|e| e.to_string())?;
            *hw = e.seq;
        }
        Ok(*hw)
    }
}

// ─── file helpers (free of ledger state) ────────────────────────────────────

fn append_line(path: &Path, entry: &JournalEntry) -> std::io::Result<u64> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    let line = serde_json::to_string(entry)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    f.write_all(line.as_bytes())?;
    f.write_all(b"\n")?;
    Ok(line.len() as u64 + 1)
}

/// Recover the last seq from an existing journal so a restart continues the
/// sequence instead of forking it. O(file) once per persona per boot.
fn recover_seq(path: &Path) -> (u64, u64, u64) {
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

/// Recover this replica's high-water per origin_node from disk (once per boot).
fn recover_replica_hw(persona_id: &str, path: &Path, map: &mut HashMap<(String, String), u64>) {
    let Ok(f) = fs::File::open(path) else { return };
    for line in BufReader::new(f).lines().map_while(Result::ok) {
        if let Ok(e) = serde_json::from_str::<JournalEntry>(&line) {
            let key = (persona_id.to_string(), e.origin_node);
            let hw = map.entry(key).or_insert(0);
            if e.seq > *hw {
                *hw = e.seq;
            }
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_ledger(tag: &str) -> (ReplicationLedger, PathBuf) {
        let root = std::env::temp_dir().join(format!("raid-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        (
            ReplicationLedger::new(root.join("personas"), root.join("replicas"), "node-a".into()),
            root,
        )
    }

    // what this catches: the journal must be monotonic and replayable — an
    // entry that round-trips lossily or a seq that repeats would corrupt
    // resume-on-spawn's newest-wins replay.
    #[test]
    fn journal_appends_monotonic_and_round_trips() {
        let (ledger, root) = temp_ledger("mono");
        let pid = "persona-a";
        ledger.journal_admit(pid, "memory", &serde_json::json!({"id": "m1", "text": "hello"}));
        ledger.journal_admit(pid, "memory", &serde_json::json!({"id": "m2", "text": "world"}));
        let snap = ledger.snapshot(pid).expect("snapshot after admits");
        assert_eq!(snap.last_seq, 2);
        assert_eq!(snap.dropped, 0);
        assert!(snap.degraded_unreplicated);
        let path = root.join("personas").join(pid).join("journal.jsonl");
        let lines: Vec<JournalEntry> = std::io::BufRead::lines(std::io::BufReader::new(
            std::fs::File::open(&path).unwrap(),
        ))
        .map(|l| serde_json::from_str(&l.unwrap()).unwrap())
        .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].seq, 1);
        assert_eq!(lines[1].seq, 2);
        assert_eq!(lines[1].record["id"], "m2");
    }

    // what this catches: a restart must CONTINUE the sequence from the file,
    // not fork it back to 1 — forked seqs make newest-wins ambiguous and
    // silently lose memories on replay ([[restarts-are-commonplace]]).
    #[test]
    fn restart_recovers_seq_from_disk() {
        let (ledger, root) = temp_ledger("restart");
        let pid = "persona-b";
        ledger.journal_admit(pid, "memory", &serde_json::json!({"id": "m1"}));
        // Simulate restart: a FRESH ledger over the same roots.
        drop(ledger);
        let ledger2 =
            ReplicationLedger::new(root.join("personas"), root.join("replicas"), "node-a".into());
        ledger2.journal_admit(pid, "memory", &serde_json::json!({"id": "m2"}));
        let snap = ledger2.snapshot(pid).expect("snapshot after recovered admit");
        assert_eq!(snap.last_seq, 2, "seq must continue across restart, not fork");
    }

    // what this catches: a re-shipped batch (shipper retry after a lost ack)
    // must append NOTHING and re-ack the same high-water — duplicate replica
    // lines would double memories on slice-3 replay.
    #[test]
    fn replica_batch_is_idempotent() {
        let (ledger, root) = temp_ledger("replica");
        let pid = "persona-c";
        let e = |seq: u64| JournalEntry {
            seq,
            persona_id: pid.into(),
            origin_node: "node-remote".into(),
            ts_ms: 1,
            kind: "memory".into(),
            record: serde_json::json!({"id": format!("m{seq}")}),
        };
        let batch = vec![e(1), e(2)];
        assert_eq!(ledger.replica_append_batch(pid, &batch).unwrap(), 2);
        // Retry the same batch: same ack, no new lines.
        assert_eq!(ledger.replica_append_batch(pid, &batch).unwrap(), 2);
        let path = root.join("replicas").join(pid).join("journal.jsonl");
        let n = std::io::BufRead::lines(std::io::BufReader::new(
            std::fs::File::open(&path).unwrap(),
        ))
        .count();
        assert_eq!(n, 2, "retry must not duplicate replica lines");
        // Mixed-origin batch fails loud.
        let mut bad = vec![e(3)];
        bad.push(JournalEntry { origin_node: "node-other".into(), ..e(4) });
        assert!(ledger.replica_append_batch(pid, &bad).is_err());
        // Empty batch fails loud.
        assert!(ledger.replica_append_batch(pid, &[]).is_err());
    }
}
