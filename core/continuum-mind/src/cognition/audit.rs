//! Audit recorder — tamper-evident append-only log for refusals,
//! governor overrides, federation drift, and access denials
//! (MODULE-CATALOG: `audit-recorder`, PR-1 of the module-build sequence
//! claude-tab-1 ranked first in their 2026-05-16T22:10Z broadcast).
//!
//! ## Why this module exists
//!
//! Joel's "no silent fallback" rule + my recent `no_cpu_fallback_contract`
//! widening (#1341) ratchet REFUSALS at type-checking time. The
//! audit-recorder closes the next gap: making each individual refusal
//! event OBSERVABLE in a tamper-evident log. Without it, "Cuda check
//! refused at boot" / "governor overrode persona's chat lease" /
//! "MMU denied genome cell access" are decisions that happened but
//! nobody can prove in retrospect — the system did the right thing,
//! quietly. The substrate needs a paper trail.
//!
//! Per MODULE-CATALOG §VII `audit-recorder` row:
//! - Lane: `ResourceClass::Background`
//! - Target: `TargetSilicon::Disk`
//! - Cadence: `OnReady` (event-driven, subscribes to four typed events)
//! - Subscriptions: `[RefusalAudit, GovernorOverride, FederationPolicyDrift, AccessDenied]`
//! - Emissions: `[AuditEntryRecorded]`
//!
//! ## Scope of PR-1 (this module)
//!
//! Pure data + thin disk I/O + tamper-evident chain. Specifically:
//!
//! - `AuditEntry` typed struct with kind / payload / sequenced chain hash
//! - `AuditEntryKind` enum for the four subscription event types
//! - `AuditChain` — append-only with rolling hash that detects tampering
//! - JSON-Lines file format (`audit.jsonl` — one entry per line)
//! - `read_audit_log` to replay + verify chain integrity
//!
//! ## Out of scope for PR-1 (later)
//!
//! - MessageBus subscription wiring (depends on PIECE-2 PR-3 #1339's
//!   ArtifactSubscription surface that just landed; PR-2 of this stack)
//! - Asymmetric signing (PR-1 uses a tamper-detection chain hash;
//!   asymmetric attestation comes when continuum-core gets a per-node
//!   identity key — separate concern)
//! - Index for quick lookup by kind / time range (file is append-only;
//!   indexing is a PR-3 if/when the log grows large enough to matter)
//!
//! ## Tamper-evidence design
//!
//! Each entry's `prev_chain_hash` is SHA-256 of the PREVIOUS entry's
//! `(seq, timestamp_ms, kind, payload_json, prev_chain_hash)`. Tampering
//! with entry N invalidates the chain from N+1 onward; the verifier
//! catches it by recomputing the chain on read. Genesis entry uses the
//! all-zeros hash as `prev_chain_hash`.
//!
//! This is NOT cryptographic signing — anyone with write access to the
//! file can append valid entries. The contract is "tampering is
//! detectable," not "tampering is prevented." Asymmetric signing lands
//! when there's a per-node identity key to sign with.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use ts_rs::TS;

/// The four kinds of events the audit-recorder pins to disk per
/// MODULE-CATALOG's subscription list. New kinds extend this enum;
/// adding a kind is a non-breaking change to the wire format because
/// it's serialized as a tagged string (`kind: "refusal"`).
///
/// Today's set:
///
/// - `Refusal` — a turn / dispatch / inference call was refused with a
///   typed reason. Composes with the residency gate's `ResidencyBlock`
///   (#1338) — every Block emits a Refusal audit entry.
/// - `GovernorOverride` — the substrate governor overrode a module's
///   own lease request (e.g. lowered concurrency below what the module
///   asked for, evicted a working-set entry the module wanted to keep).
/// - `FederationPolicyDrift` — a peer node's federation policy diverged
///   from our local policy. The drift gets logged; resolution is a
///   policy concern.
/// - `AccessDenied` — the MMU-style genome permission table denied a
///   read / write / execute. Compartmentalization audit trail.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/AuditEntryKind.ts"
)]
pub enum AuditEntryKind {
    Refusal,
    GovernorOverride,
    FederationPolicyDrift,
    AccessDenied,
}

/// One audit log entry. Append-only — entries are written once, never
/// modified. The `chain_hash` is computed from the entry's content + the
/// previous entry's chain_hash, forming the tamper-detection chain.
///
/// The `payload` field is a free-form JSON value — each kind has its
/// own payload shape that downstream tooling decodes. Keeping the wire
/// format open-ended means new audit kinds can ship without a schema
/// migration; tooling that doesn't recognize a kind just records the
/// raw JSON.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/AuditEntry.ts"
)]
pub struct AuditEntry {
    /// Monotonic sequence number. Starts at 0 for the genesis entry.
    /// Verifier asserts seq == prev_seq + 1 — gap detection.
    #[ts(type = "number")]
    pub seq: u64,
    /// Unix-ms timestamp the entry was recorded. Caller's clock —
    /// verifier asserts monotonic-non-decreasing across entries.
    #[ts(type = "number")]
    pub timestamp_ms: u64,
    /// Which event kind this entry records.
    pub kind: AuditEntryKind,
    /// Free-form JSON payload for this entry. Shape per-kind; the
    /// recorder doesn't validate the inner shape (downstream tooling
    /// does). On the TS wire it surfaces as `unknown` — consumers
    /// narrow by `kind`.
    #[ts(type = "unknown")]
    pub payload: serde_json::Value,
    /// Hex-encoded SHA-256 chain hash:
    /// `sha256(seq || timestamp_ms || kind || payload || prev_chain_hash)`.
    /// Genesis entry's prev_chain_hash is the all-zeros string of length 64.
    pub chain_hash: String,
    /// The hash of the previous entry. Genesis = "0" * 64.
    pub prev_chain_hash: String,
}

/// Errors the audit chain can surface. Tamper detection lives in
/// `ChainBroken` — verifier saw a hash that doesn't match the recomputed
/// chain. The other variants are I/O or serde failures.
#[derive(Debug)]
pub enum AuditError {
    Io(std::io::Error),
    Serde(serde_json::Error),
    /// Verifier read entry N and the recomputed chain_hash didn't
    /// match the stored one. Tampering or corruption.
    ChainBroken {
        seq: u64,
        expected: String,
        got: String,
    },
    /// Sequence number out of order. Either gap detection or
    /// non-monotonic — both indicate write-side bug or tampering.
    SequenceGap {
        expected: u64,
        got: u64,
    },
    /// Timestamp moved backward across entries. Clock skew on the
    /// writer is the usual cause; surfaced so an operator can decide
    /// whether to trust the log.
    TimestampWentBackward {
        prev: u64,
        current: u64,
    },
}

impl std::fmt::Display for AuditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuditError::Io(e) => write!(f, "audit I/O: {e}"),
            AuditError::Serde(e) => write!(f, "audit serde: {e}"),
            AuditError::ChainBroken { seq, expected, got } => write!(
                f,
                "audit chain broken at seq {seq}: expected hash {expected}, got {got}"
            ),
            AuditError::SequenceGap { expected, got } => {
                write!(f, "audit sequence gap: expected {expected}, got {got}")
            }
            AuditError::TimestampWentBackward { prev, current } => write!(
                f,
                "audit timestamp went backward: prev={prev} current={current}"
            ),
        }
    }
}

impl std::error::Error for AuditError {}

impl From<std::io::Error> for AuditError {
    fn from(e: std::io::Error) -> Self {
        AuditError::Io(e)
    }
}

impl From<serde_json::Error> for AuditError {
    fn from(e: serde_json::Error) -> Self {
        AuditError::Serde(e)
    }
}

/// Genesis prev-hash: 64 zeros (matches SHA-256 output length).
pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Compute the chain hash for an entry. Pure function — same inputs
/// always produce the same hash.
fn compute_chain_hash(
    seq: u64,
    timestamp_ms: u64,
    kind: &AuditEntryKind,
    payload: &serde_json::Value,
    prev_chain_hash: &str,
) -> String {
    let kind_json =
        serde_json::to_string(kind).expect("AuditEntryKind serialization is infallible");
    let payload_json = payload.to_string();

    let mut hasher = Sha256::new();
    hasher.update(seq.to_le_bytes());
    hasher.update(timestamp_ms.to_le_bytes());
    hasher.update(kind_json.as_bytes());
    hasher.update(payload_json.as_bytes());
    hasher.update(prev_chain_hash.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn build_audit_entry(
    seq: u64,
    prev_chain_hash: String,
    timestamp_ms: u64,
    kind: AuditEntryKind,
    payload: serde_json::Value,
) -> AuditEntry {
    let chain_hash = compute_chain_hash(seq, timestamp_ms, &kind, &payload, &prev_chain_hash);

    AuditEntry {
        seq,
        timestamp_ms,
        kind,
        payload,
        chain_hash,
        prev_chain_hash,
    }
}

/// Append-only audit chain backed by an `audit.jsonl` file. One entry
/// per line — easy to grep, easy to tail. Caller holds the chain
/// in-memory between writes (it tracks the last seq + last hash so it
/// can chain correctly).
///
/// Thread-safety: NOT internally synchronized. Wrap in `Mutex` /
/// `parking_lot::Mutex` if multiple threads will write — the chain's
/// correctness depends on sequential append. PR-2 (MessageBus wiring)
/// will run inside a single tokio task to avoid the lock.
pub struct AuditChain {
    next_seq: u64,
    last_chain_hash: String,
}

impl AuditChain {
    /// Create a fresh chain (no entries yet). Genesis prev_chain_hash
    /// is GENESIS_HASH.
    pub fn new() -> Self {
        Self {
            next_seq: 0,
            last_chain_hash: GENESIS_HASH.to_string(),
        }
    }

    /// Reconstruct chain state by reading an existing log file. Reads
    /// every entry, validates chain integrity, and returns a chain
    /// positioned at the last entry's (seq + 1, chain_hash). If the
    /// chain is broken, returns the typed error so the caller can
    /// decide whether to refuse-startup, archive, or alert.
    pub fn load(path: &Path) -> Result<Self, AuditError> {
        let entries = read_audit_log(path)?;
        match entries.last() {
            None => Ok(Self::new()),
            Some(last) => Ok(Self {
                next_seq: last.seq + 1,
                last_chain_hash: last.chain_hash.clone(),
            }),
        }
    }

    /// Build the next entry with a given kind/payload/timestamp. Pure
    /// function — doesn't write. Returns the entry so caller can
    /// append + post-process (e.g. emit AuditEntryRecorded event).
    pub fn build_next(
        &mut self,
        timestamp_ms: u64,
        kind: AuditEntryKind,
        payload: serde_json::Value,
    ) -> AuditEntry {
        let seq = self.next_seq;
        let entry = build_audit_entry(
            seq,
            self.last_chain_hash.clone(),
            timestamp_ms,
            kind,
            payload,
        );

        self.next_seq += 1;
        self.last_chain_hash = entry.chain_hash.clone();
        entry
    }

    /// Convenience: build + append in one call. Returns the appended
    /// entry. Caller can then emit AuditEntryRecorded (PR-2).
    pub fn append(
        &mut self,
        path: &Path,
        timestamp_ms: u64,
        kind: AuditEntryKind,
        payload: serde_json::Value,
    ) -> Result<AuditEntry, AuditError> {
        let entry = build_audit_entry(
            self.next_seq,
            self.last_chain_hash.clone(),
            timestamp_ms,
            kind,
            payload,
        );
        let line = serde_json::to_string(&entry)?;
        let mut file = OpenOptions::new().append(true).create(true).open(path)?;
        writeln!(file, "{line}")?;

        self.next_seq += 1;
        self.last_chain_hash = entry.chain_hash.clone();
        Ok(entry)
    }

    /// Inspect the chain's current position (next seq + last hash).
    /// Useful for telemetry + tests.
    pub fn position(&self) -> (u64, &str) {
        (self.next_seq, &self.last_chain_hash)
    }
}

impl Default for AuditChain {
    fn default() -> Self {
        Self::new()
    }
}

/// Read every entry from a JSONL audit log + verify chain integrity.
/// Verification rules:
///
/// 1. Seq numbers are monotonic-strict (each entry's seq = prev + 1).
/// 2. Timestamps are monotonic-non-decreasing (clock skew tolerated as
///    equal; backward = error).
/// 3. Each entry's chain_hash equals recompute(seq, ts, kind, payload,
///    prev_chain_hash).
/// 4. Genesis entry's prev_chain_hash equals GENESIS_HASH.
///
/// Any violation returns the typed AuditError at the first failure;
/// the caller decides whether to truncate-and-recover, archive, or
/// alert.
pub fn read_audit_log(path: &Path) -> Result<Vec<AuditEntry>, AuditError> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut entries: Vec<AuditEntry> = Vec::new();
    let mut prev_seq: Option<u64> = None;
    let mut prev_ts: Option<u64> = None;
    let mut prev_hash: String = GENESIS_HASH.to_string();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let entry: AuditEntry = serde_json::from_str(&line)?;

        // 1. Seq monotonic-strict
        let expected_seq = prev_seq.map(|p| p + 1).unwrap_or(0);
        if entry.seq != expected_seq {
            return Err(AuditError::SequenceGap {
                expected: expected_seq,
                got: entry.seq,
            });
        }

        // 2. Timestamp monotonic-non-decreasing
        if let Some(p) = prev_ts {
            if entry.timestamp_ms < p {
                return Err(AuditError::TimestampWentBackward {
                    prev: p,
                    current: entry.timestamp_ms,
                });
            }
        }

        // 3. chain_hash matches recompute
        if entry.prev_chain_hash != prev_hash {
            return Err(AuditError::ChainBroken {
                seq: entry.seq,
                expected: prev_hash.clone(),
                got: entry.prev_chain_hash.clone(),
            });
        }
        let expected_hash = compute_chain_hash(
            entry.seq,
            entry.timestamp_ms,
            &entry.kind,
            &entry.payload,
            &entry.prev_chain_hash,
        );
        if entry.chain_hash != expected_hash {
            return Err(AuditError::ChainBroken {
                seq: entry.seq,
                expected: expected_hash,
                got: entry.chain_hash.clone(),
            });
        }

        prev_seq = Some(entry.seq);
        prev_ts = Some(entry.timestamp_ms);
        prev_hash = entry.chain_hash.clone();
        entries.push(entry);
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::NamedTempFile;

    // ===== AuditEntryKind serde =====

    /// What this catches: AuditEntryKind serializes as kebab-case
    /// strings ("refusal", "governor-override", ...). Wire stability
    /// — downstream tooling parses these strings.
    #[test]
    fn audit_entry_kind_serializes_kebab_case() {
        assert_eq!(
            serde_json::to_string(&AuditEntryKind::Refusal).unwrap(),
            "\"refusal\""
        );
        assert_eq!(
            serde_json::to_string(&AuditEntryKind::GovernorOverride).unwrap(),
            "\"governor-override\""
        );
        assert_eq!(
            serde_json::to_string(&AuditEntryKind::FederationPolicyDrift).unwrap(),
            "\"federation-policy-drift\""
        );
        assert_eq!(
            serde_json::to_string(&AuditEntryKind::AccessDenied).unwrap(),
            "\"access-denied\""
        );
    }

    // ===== AuditChain.build_next =====

    /// What this catches: a fresh chain produces a genesis entry with
    /// seq=0 + prev_chain_hash=GENESIS_HASH. If genesis drift, every
    /// downstream entry's chain validation breaks.
    #[test]
    fn fresh_chain_genesis_entry_is_correct() {
        let mut chain = AuditChain::new();
        let entry = chain.build_next(1000, AuditEntryKind::Refusal, json!({"reason": "test"}));
        assert_eq!(entry.seq, 0);
        assert_eq!(entry.timestamp_ms, 1000);
        assert_eq!(entry.kind, AuditEntryKind::Refusal);
        assert_eq!(entry.prev_chain_hash, GENESIS_HASH);
        assert_eq!(entry.chain_hash.len(), 64, "SHA-256 hex is 64 chars");
    }

    /// What this catches: seq increments by 1 across build_next calls.
    /// Off-by-one would mean later read_audit_log detects a gap.
    #[test]
    fn chain_seq_increments_monotonically() {
        let mut chain = AuditChain::new();
        for i in 0..5 {
            let entry = chain.build_next(1000 + i, AuditEntryKind::AccessDenied, json!({"i": i}));
            assert_eq!(entry.seq, i);
        }
    }

    /// What this catches: each entry's chain_hash references the
    /// previous entry's chain_hash. Tampering with entry N's payload
    /// changes entry N's hash, which means entry N+1's
    /// prev_chain_hash is now wrong — verifier catches it.
    #[test]
    fn chain_hashes_link_consecutive_entries() {
        let mut chain = AuditChain::new();
        let a = chain.build_next(1000, AuditEntryKind::Refusal, json!({"a": 1}));
        let b = chain.build_next(2000, AuditEntryKind::Refusal, json!({"b": 2}));
        assert_eq!(b.prev_chain_hash, a.chain_hash, "b must link to a");
    }

    /// What this catches: identical inputs across chain instances
    /// produce identical hashes. Pure function — no randomness, no
    /// hidden state.
    #[test]
    fn compute_chain_hash_is_deterministic() {
        let h1 = compute_chain_hash(
            0,
            1000,
            &AuditEntryKind::Refusal,
            &json!({"x": 1}),
            GENESIS_HASH,
        );
        let h2 = compute_chain_hash(
            0,
            1000,
            &AuditEntryKind::Refusal,
            &json!({"x": 1}),
            GENESIS_HASH,
        );
        assert_eq!(h1, h2);
    }

    /// What this catches: changing any input changes the hash.
    /// Sensitivity check — confirms the hash isn't accidentally
    /// constant under input variation.
    #[test]
    fn compute_chain_hash_sensitive_to_each_input() {
        let base = compute_chain_hash(0, 1000, &AuditEntryKind::Refusal, &json!({}), GENESIS_HASH);
        let diff_seq =
            compute_chain_hash(1, 1000, &AuditEntryKind::Refusal, &json!({}), GENESIS_HASH);
        let diff_ts =
            compute_chain_hash(0, 2000, &AuditEntryKind::Refusal, &json!({}), GENESIS_HASH);
        let diff_kind = compute_chain_hash(
            0,
            1000,
            &AuditEntryKind::AccessDenied,
            &json!({}),
            GENESIS_HASH,
        );
        let diff_payload = compute_chain_hash(
            0,
            1000,
            &AuditEntryKind::Refusal,
            &json!({"a": 1}),
            GENESIS_HASH,
        );
        let diff_prev = compute_chain_hash(
            0,
            1000,
            &AuditEntryKind::Refusal,
            &json!({}),
            "1111111111111111111111111111111111111111111111111111111111111111",
        );
        assert_ne!(base, diff_seq);
        assert_ne!(base, diff_ts);
        assert_ne!(base, diff_kind);
        assert_ne!(base, diff_payload);
        assert_ne!(base, diff_prev);
    }

    // ===== append + read round-trip =====

    /// What this catches: append → read returns the same entry.
    /// Smoke test for the JSONL serialization + file I/O happy path.
    #[test]
    fn append_then_read_returns_same_entry() {
        let tmp = NamedTempFile::new().unwrap();
        let mut chain = AuditChain::new();
        let written = chain
            .append(
                tmp.path(),
                1000,
                AuditEntryKind::Refusal,
                json!({"why": "test"}),
            )
            .unwrap();
        let read = read_audit_log(tmp.path()).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0], written);
    }

    /// What this catches: multiple appends produce a valid chain.
    /// End-to-end: write 5 entries, read them back, verify chain
    /// integrity passes.
    #[test]
    fn many_appends_form_valid_chain() {
        let tmp = NamedTempFile::new().unwrap();
        let mut chain = AuditChain::new();
        for i in 0..5 {
            chain
                .append(
                    tmp.path(),
                    1000 + i * 100,
                    AuditEntryKind::GovernorOverride,
                    json!({"step": i}),
                )
                .unwrap();
        }
        let read = read_audit_log(tmp.path()).unwrap();
        assert_eq!(read.len(), 5);
        for i in 0..5 {
            assert_eq!(read[i as usize].seq, i);
        }
    }

    /// What this catches: failed disk writes must not advance the
    /// in-memory chain. If append moves next_seq/last_hash before I/O
    /// succeeds, the next successful write no longer matches the file.
    #[test]
    fn append_failure_does_not_advance_chain_position() {
        let mut chain = AuditChain::new();
        let missing_dir = Path::new("/nonexistent/audit-recorder-dir/audit.jsonl");

        let result = chain.append(
            missing_dir,
            1000,
            AuditEntryKind::Refusal,
            json!({"why": "missing dir"}),
        );

        assert!(matches!(result, Err(AuditError::Io(_))));
        assert_eq!(chain.position(), (0, GENESIS_HASH));
    }

    /// What this catches: read_audit_log on a non-existent path
    /// returns empty Vec (not error). The recorder must handle
    /// "first-boot, no log yet" cleanly.
    #[test]
    fn read_nonexistent_path_returns_empty() {
        let path = Path::new("/nonexistent/audit-log-not-here.jsonl");
        let result = read_audit_log(path).unwrap();
        assert!(result.is_empty());
    }

    /// What this catches: load() on an existing log restores the
    /// chain's next_seq + last_hash to continue from there. Without
    /// this, a process restart would write seq=0 again — gap detection
    /// in the verifier would flag the duplicate.
    #[test]
    fn load_restores_chain_position_from_existing_log() {
        let tmp = NamedTempFile::new().unwrap();
        let mut chain = AuditChain::new();
        for i in 0..3 {
            chain
                .append(
                    tmp.path(),
                    1000 + i,
                    AuditEntryKind::Refusal,
                    json!({"i": i}),
                )
                .unwrap();
        }
        let restored = AuditChain::load(tmp.path()).unwrap();
        assert_eq!(restored.position().0, 3, "next_seq after 3 entries is 3");
        // Continue appending — should chain cleanly
        let mut restored = restored;
        let next = restored.build_next(2000, AuditEntryKind::Refusal, json!({"i": 99}));
        assert_eq!(next.seq, 3);
    }

    // ===== tamper detection =====

    /// What this catches: changing an entry's payload after-the-fact
    /// breaks the chain. Verifier returns ChainBroken at the tampered
    /// seq. This is the WHOLE POINT of the chain — if this regresses,
    /// the audit log is just an unprotected JSON file.
    #[test]
    fn tampered_entry_payload_breaks_chain() {
        let tmp = NamedTempFile::new().unwrap();
        let mut chain = AuditChain::new();
        for i in 0..3 {
            chain
                .append(
                    tmp.path(),
                    1000 + i,
                    AuditEntryKind::Refusal,
                    json!({"i": i}),
                )
                .unwrap();
        }
        // Tamper: rewrite entry 1's payload on disk
        let content = std::fs::read_to_string(tmp.path()).unwrap();
        let tampered = content.replace("\"i\":1", "\"i\":999");
        std::fs::write(tmp.path(), tampered).unwrap();

        match read_audit_log(tmp.path()) {
            Err(AuditError::ChainBroken { seq, .. }) => {
                assert!(seq <= 2, "tampering at seq 1 should break at seq 1 or 2");
            }
            other => panic!("expected ChainBroken, got {other:?}"),
        }
    }

    /// What this catches: out-of-order seq numbers (e.g. seq=0 then
    /// seq=2 with gap) return SequenceGap. Defends against a tampered
    /// log that removed an entry (renumbering would also break chain
    /// hash, but gap detection is the first signal).
    #[test]
    fn sequence_gap_detected() {
        let tmp = NamedTempFile::new().unwrap();
        let mut chain = AuditChain::new();
        chain
            .append(tmp.path(), 1000, AuditEntryKind::Refusal, json!({}))
            .unwrap();
        // Skip seq 1: manually craft a seq=2 entry that would link to
        // seq=0's hash (impossible chain, but tests the gap detector).
        let entry_2 = AuditEntry {
            seq: 2,
            timestamp_ms: 2000,
            kind: AuditEntryKind::Refusal,
            payload: json!({}),
            chain_hash: "deadbeef".repeat(8),
            prev_chain_hash: chain.last_chain_hash.clone(),
        };
        let mut file = OpenOptions::new().append(true).open(tmp.path()).unwrap();
        writeln!(file, "{}", serde_json::to_string(&entry_2).unwrap()).unwrap();

        match read_audit_log(tmp.path()) {
            Err(AuditError::SequenceGap { expected, got }) => {
                assert_eq!(expected, 1);
                assert_eq!(got, 2);
            }
            other => panic!("expected SequenceGap, got {other:?}"),
        }
    }

    /// What this catches: timestamp moving backward returns the typed
    /// TimestampWentBackward. Clock skew on the writer is common; the
    /// verifier flags it instead of silently accepting.
    #[test]
    fn backward_timestamp_detected() {
        let tmp = NamedTempFile::new().unwrap();
        let mut chain = AuditChain::new();
        chain
            .append(
                tmp.path(),
                5000,
                AuditEntryKind::Refusal,
                json!({"first": true}),
            )
            .unwrap();
        // Append with earlier timestamp via build_next (chain hash is
        // correct, but ts violates monotonic-non-decreasing)
        chain
            .append(
                tmp.path(),
                1000,
                AuditEntryKind::Refusal,
                json!({"second": true}),
            )
            .unwrap();

        match read_audit_log(tmp.path()) {
            Err(AuditError::TimestampWentBackward { prev, current }) => {
                assert_eq!(prev, 5000);
                assert_eq!(current, 1000);
            }
            other => panic!("expected TimestampWentBackward, got {other:?}"),
        }
    }

    /// What this catches: equal timestamps across entries are
    /// ACCEPTED (only strict backward is rejected). Fast writers can
    /// produce two entries in the same ms; rejecting that would break
    /// burst-write paths.
    #[test]
    fn equal_timestamps_accepted() {
        let tmp = NamedTempFile::new().unwrap();
        let mut chain = AuditChain::new();
        for _ in 0..3 {
            chain
                .append(tmp.path(), 5000, AuditEntryKind::Refusal, json!({}))
                .unwrap();
        }
        let read = read_audit_log(tmp.path()).unwrap();
        assert_eq!(read.len(), 3);
    }

    // ===== AuditError =====

    /// What this catches: AuditError implements Display + Error so it
    /// works in `?` chains + dyn Error contexts.
    #[test]
    fn audit_error_implements_error_trait() {
        let e = AuditError::ChainBroken {
            seq: 5,
            expected: "abc".into(),
            got: "def".into(),
        };
        let _: &dyn std::error::Error = &e;
        let display = format!("{e}");
        assert!(display.contains("5"));
        assert!(display.contains("abc"));
        assert!(display.contains("def"));
    }

    /// What this catches: From<std::io::Error> + From<serde_json::Error>
    /// for AuditError. Lets callers use `?` to propagate without manual
    /// .map_err() boilerplate.
    #[test]
    fn audit_error_from_io_and_serde() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let audit_err: AuditError = io_err.into();
        assert!(matches!(audit_err, AuditError::Io(_)));

        let serde_err = serde_json::from_str::<AuditEntry>("not json").unwrap_err();
        let audit_err: AuditError = serde_err.into();
        assert!(matches!(audit_err, AuditError::Serde(_)));
    }

    // ===== AuditEntry serde =====

    /// What this catches: AuditEntry round-trips with camelCase wire.
    /// Field names must match what TypeScript consumers expect once
    /// PR-2 wires the recorder to emit AuditEntryRecorded events to
    /// the TS layer.
    #[test]
    fn audit_entry_serde_camelcase() {
        let mut chain = AuditChain::new();
        let entry = chain.build_next(1234, AuditEntryKind::Refusal, json!({"foo": "bar"}));
        let j = serde_json::to_string(&entry).unwrap();
        assert!(j.contains("\"timestampMs\":1234"));
        assert!(j.contains("\"prevChainHash\":"));
        assert!(j.contains("\"chainHash\":"));
        let back: AuditEntry = serde_json::from_str(&j).unwrap();
        assert_eq!(back, entry);
    }
}
