//! Trace parsing + decode-token segmentation — the observation front
//! end of the RUN-2 driver, ported faithfully from the prototypes'
//! `decode_tokens` (BigMama's branch, validated against her
//! `k3-routed-access.trace` fixture: 83,968 records → 12 segments → 9
//! modal decode tokens of ~1472 experts).
//!
//! Trace record wire format (GGML_MOE_TRACE_FILE, confirmed by her
//! 2026-08-01): 12 bytes little-endian — `tkey: u64` (FNV-1a of the
//! canonical tensor name `blk.{layer}.ffn_{gate,up,down}_exps.weight`)
//! then `e: u32` (within-layer expert index). One EXPERT = (layer, e);
//! its three matrices share `e` under three distinct tkeys, so token
//! sets dedup at the (layer, e) level.
//!
//! Token boundary (her exact rule): within a token each tensor key's
//! group appears once; when a key ALREADY SEEN this token reappears,
//! the router cycle wrapped — that record starts the next token.
//!
//! The live tail reads only COMPLETE records: the writer is
//! stdio-buffered (~4KB flushes, ~13/token), so a partial 12-byte
//! record at the file tail is normal and must be left for the next
//! poll.

use std::collections::{HashMap, HashSet};

use crate::expert_id::ExpertId;

/// One parsed trace record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceRecord {
    pub tkey: u64,
    pub expert: u32,
}

pub const RECORD_BYTES: usize = 12;

/// Parse complete records from a byte slice. Returns the records and
/// the number of bytes CONSUMED (always a multiple of 12) — the caller
/// keeps the unconsumed tail bytes for the next read (partial-record
/// guard).
pub fn parse_records(bytes: &[u8]) -> (Vec<TraceRecord>, usize) {
    let n = bytes.len() / RECORD_BYTES;
    let mut records = Vec::with_capacity(n);
    for i in 0..n {
        let o = i * RECORD_BYTES;
        let tkey = u64::from_le_bytes(bytes[o..o + 8].try_into().unwrap_or([0; 8]));
        let expert = u32::from_le_bytes(bytes[o + 8..o + 12].try_into().unwrap_or([0; 4]));
        records.push(TraceRecord { tkey, expert });
    }
    (records, n * RECORD_BYTES)
}

/// tkey → layer map, loaded from her `tkey-to-layer-matrix.json`
/// (`{ "<tkey>": { "layer": N, "matrix": "gate|up|down" }, ... }`).
#[derive(Debug, Clone)]
pub struct TkeyTable {
    layer_of: HashMap<u64, u32>,
}

impl TkeyTable {
    pub fn from_json(json: &str) -> Result<Self, String> {
        #[derive(serde::Deserialize)]
        struct Entry {
            layer: u32,
        }
        let raw: HashMap<String, Entry> =
            serde_json::from_str(json).map_err(|e| format!("tkey table parse: {e}"))?;
        let mut layer_of = HashMap::with_capacity(raw.len());
        for (k, v) in raw {
            let tkey: u64 = k
                .parse()
                .map_err(|e| format!("tkey table key {k:?} not u64: {e}"))?;
            layer_of.insert(tkey, v.layer);
        }
        Ok(Self { layer_of })
    }

    pub fn layer(&self, tkey: u64) -> Option<u32> {
        self.layer_of.get(&tkey).copied()
    }

    pub fn len(&self) -> usize {
        self.layer_of.len()
    }

    pub fn is_empty(&self) -> bool {
        self.layer_of.is_empty()
    }
}

/// Streaming decode-token segmenter: feed records as they arrive,
/// receive completed token sets of (layer, expert). Unknown tkeys are
/// counted, never silently dropped without trace.
#[derive(Debug, Default)]
pub struct TokenSegmenter {
    seen_keys: HashSet<u64>,
    prev_key: Option<u64>,
    current: HashSet<ExpertId>,
    pub unknown_tkeys: u64,
}

impl TokenSegmenter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one record. Returns `Some(token)` when this record STARTS a
    /// new token (the previous token's expert set is complete).
    pub fn push(&mut self, rec: TraceRecord, table: &TkeyTable) -> Option<HashSet<ExpertId>> {
        let mut completed = None;
        if self.prev_key != Some(rec.tkey) {
            if self.seen_keys.contains(&rec.tkey) {
                // Cycle wrapped: previous token is complete.
                completed = Some(std::mem::take(&mut self.current));
                self.seen_keys.clear();
            }
            self.seen_keys.insert(rec.tkey);
            self.prev_key = Some(rec.tkey);
        }
        match table.layer(rec.tkey) {
            Some(layer) => {
                self.current.insert(ExpertId {
                    layer,
                    expert: rec.expert,
                });
            }
            None => self.unknown_tkeys += 1,
        }
        completed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table_for(layers: &[u32]) -> TkeyTable {
        // Synthetic table: tkey = layer*10 + matrix (0..3).
        let mut layer_of = HashMap::new();
        for &l in layers {
            for m in 0..3u64 {
                layer_of.insert(u64::from(l) * 10 + m, l);
            }
        }
        TkeyTable { layer_of }
    }

    fn rec(tkey: u64, expert: u32) -> TraceRecord {
        TraceRecord { tkey, expert }
    }

    /// what this catches: the wire parse + the partial-tail guard — the
    /// stdio-buffered writer routinely leaves a torn 12-byte record at
    /// the tail; consuming it would shift every subsequent record and
    /// corrupt the whole stream.
    #[test]
    fn parse_consumes_only_complete_records() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&7u64.to_le_bytes());
        bytes.extend_from_slice(&42u32.to_le_bytes());
        bytes.extend_from_slice(&[1, 2, 3, 4, 5]); // torn tail
        let (records, consumed) = parse_records(&bytes);
        assert_eq!(records, vec![rec(7, 42)]);
        assert_eq!(consumed, 12, "the torn tail must be left unconsumed");
    }

    /// what this catches: her exact boundary rule — a token completes
    /// when an already-seen key REAPPEARS (router cycle wrap), not on
    /// mere key change; and dedup lands at (layer, expert) so the three
    /// matrices of one expert count once.
    #[test]
    fn boundary_fires_on_key_reappearance_and_dedups_matrices() {
        let table = table_for(&[1, 2]);
        let mut seg = TokenSegmenter::new();
        // Token A: layer1 gate/up/down for expert 5 (3 keys), layer2 gate expert 9.
        let token_a = [rec(10, 5), rec(11, 5), rec(12, 5), rec(20, 9)];
        for r in token_a {
            assert!(seg.push(r, &table).is_none(), "no boundary inside token A");
        }
        // Reappearance of key 10 starts token B and completes A.
        let done = seg.push(rec(10, 6), &table).expect("token A completes");
        assert_eq!(done.len(), 2, "3 matrices of (1,5) dedup to one expert");
        assert!(done.contains(&ExpertId {
            layer: 1,
            expert: 5
        }));
        assert!(done.contains(&ExpertId {
            layer: 2,
            expert: 9
        }));
        // Consecutive same-key records are one group — NO boundary (her
        // exact rule: reappearance only counts after OTHER keys).
        assert!(seg.push(rec(10, 7), &table).is_none());
        // A different key, then key 10 again → cycle wrapped, B completes.
        assert!(seg.push(rec(11, 6), &table).is_none());
        let done_b = seg.push(rec(10, 8), &table).expect("token B completes");
        assert!(done_b.contains(&ExpertId {
            layer: 1,
            expert: 6
        }));
        assert!(done_b.contains(&ExpertId {
            layer: 1,
            expert: 7
        }));
    }

    /// what this catches: unknown tkeys are COUNTED, not silently
    /// dropped — a stale table against a new model layout must be
    /// visible in the driver's telemetry, not a mystery hit-rate dip.
    #[test]
    fn unknown_tkeys_are_counted() {
        let table = table_for(&[1]);
        let mut seg = TokenSegmenter::new();
        seg.push(rec(999, 1), &table);
        assert_eq!(seg.unknown_tkeys, 1);
    }

    /// what this catches: the table loader against her real JSON shape
    /// (string u64 keys, {layer, matrix} values — matrix ignored here).
    #[test]
    fn tkey_table_parses_her_json_shape() {
        let json = r#"{"16542725649459479844": {"layer": 5, "matrix": "up"}, "42": {"layer": 0, "matrix": "gate"}}"#;
        let table = TkeyTable::from_json(json).expect("parse");
        assert_eq!(table.len(), 2);
        assert_eq!(table.layer(16542725649459479844), Some(5));
        assert_eq!(table.layer(42), Some(0));
        assert_eq!(table.layer(7), None);
    }
}
