//! Pager capture — the control loop's per-token observability contract
//! (#276, docs/architecture/EXPERT-PAGING-CONTROL-LAW.md § "the
//! CaptureEvent schema").
//!
//! The expert pager is a classic control system, and this is its sensor
//! record: ONE event per decode token, carrying the performance signals
//! (hit-rate, fault stall, realized tok/s, fetch throughput, quality,
//! composite reward) and the decision state (active bandit arm, per-arm
//! belief, tier distribution, promotion/demotion activity, preference
//! weight). The Rust struct is the single source of truth; ts-rs
//! generates the TS binding so Positron widgets cannot drift from it.
//!
//! Per OBSERVABILITY-AS-SUBSTRATE: emission goes through
//! [`PagerCaptureSink`], Noop by default = zero hot-path cost. The
//! durable JSONL drain reuses `routing::probe_file_sink` facilities
//! (bounded/rolling by facility, never a bespoke cap); the C++ fork's
//! `GGML_MOE_CAPTURE_FILE` emitter is only the RAW feed for direct
//! tailing during a single serve — this schema is what BOTH feeds
//! project into, which is exactly what the cross-node parity tests pin.
//!
//! Field semantics validated live 2026-08-01 (BigMama's fitted-K3 run,
//! warm decode 0.33 tok/s matching WASTE 0.32 at rung-1): `fetch_mb_s`
//! caught the 385→2458 MB/s pressure-relief jump, `resident_experts`
//! pinned the 4416 per-token working set, and `reward` is the surface
//! the rung-2 bandit climbs.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One decode-token frame of the pager control loop. See module doc.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/capacity/PagerCaptureEvent.ts"
)]
pub struct PagerCaptureEvent {
    /// Monotonic decode-token index.
    #[ts(type = "number")]
    pub token: u64,
    // --- performance / reward (graph-control time-series) ---
    /// Realized residency hit-rate this token [0..1].
    pub hit_rate: f32,
    /// Synchronous NVMe stall this token — the negative reward (GPU idle).
    pub fault_wait_ms: f32,
    /// Realized token rate (EMA).
    pub tok_per_s: f32,
    /// Host→VRAM bytes moved this token.
    pub bytes_fetched_mb: f32,
    /// Effective fetch bandwidth this token.
    pub fetch_mb_s: f32,
    /// 1 − distortion [0..1] (perplexity/KL-derived when live). Defaults
    /// when absent: the RAW C++ emitter feed carries only the perf
    /// fields — quality/reward/decision state exist once the Rust
    /// controller runs, so a raw line must still decode (verified
    /// against BigMama's live emitter 2026-08-01: perf field names
    /// match exactly; the decision fields are the additive delta).
    #[serde(default)]
    pub quality: f32,
    /// The composite `w·tok/s_norm + (1−w)·quality` the loop maximizes.
    #[serde(default)]
    pub reward: f32,
    // --- policy / decision ("which" widgets) — all default-tolerant
    // for the same raw-feed reason as `quality` above ---
    /// Active bandit arm (recency↔frequency dial).
    #[serde(default)]
    pub chosen_decay: f32,
    /// Each candidate arm's EMA reward — the bandit's belief state.
    #[serde(default)]
    pub per_arm_reward: Vec<f32>,
    #[ts(type = "number")]
    pub resident_experts: u32,
    /// Experts resident per precision tier [all-star .. cruft].
    #[serde(default)]
    pub tier_counts: Vec<u32>,
    /// LTP this token (warming → higher tier).
    #[ts(type = "number")]
    #[serde(default)]
    pub promotions: u32,
    /// LTD this token (cooling → lower tier / evict).
    #[ts(type = "number")]
    #[serde(default)]
    pub demotions: u32,
    /// Current speed↔quality preference weight.
    #[serde(default)]
    pub preference_w: f32,
}

/// Where per-token capture frames go. Noop by default so the decode hot
/// path pays nothing when nobody is watching — same CaptureSink doctrine
/// as the RAG capture sinks.
pub trait PagerCaptureSink: Send + Sync {
    fn emit(&self, event: &PagerCaptureEvent);
}

/// The zero-cost default.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopPagerCaptureSink;

impl PagerCaptureSink for NoopPagerCaptureSink {
    fn emit(&self, _event: &PagerCaptureEvent) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PagerCaptureEvent {
        PagerCaptureEvent {
            token: 4,
            hit_rate: 0.62,
            fault_wait_ms: 1500.0,
            tok_per_s: 0.326,
            bytes_fetched_mb: 4096.0,
            fetch_mb_s: 2458.0,
            quality: 0.97,
            reward: 0.71,
            chosen_decay: 0.85,
            per_arm_reward: vec![0.61, 0.71, 0.55],
            resident_experts: 4416,
            tier_counts: vec![184, 3100, 1132],
            promotions: 12,
            demotions: 9,
            preference_w: 0.5,
        }
    }

    /// what this catches (#276 parity anchor): the JSONL wire shape is
    /// the CROSS-NODE contract — BigMama's capture slices must decode
    /// into this struct field-for-field. A rename or type drift breaks
    /// this test before it breaks the parity run.
    #[test]
    fn jsonl_round_trip_is_field_stable() {
        let event = sample();
        let line = serde_json::to_string(&event).expect("serialize");
        // Pin the load-bearing field names literally — serde renames
        // would round-trip fine while silently breaking her feed.
        for key in [
            "\"token\":4",
            "\"hit_rate\":",
            "\"fault_wait_ms\":",
            "\"tok_per_s\":",
            "\"bytes_fetched_mb\":",
            "\"fetch_mb_s\":",
            "\"quality\":",
            "\"reward\":",
            "\"chosen_decay\":",
            "\"per_arm_reward\":",
            "\"resident_experts\":4416",
            "\"tier_counts\":",
            "\"promotions\":12",
            "\"demotions\":9",
            "\"preference_w\":",
        ] {
            assert!(line.contains(key), "wire line missing {key}: {line}");
        }
        let back: PagerCaptureEvent = serde_json::from_str(&line).expect("deserialize");
        assert_eq!(back, event);
    }

    /// what this catches: forward tolerance — a capture line from a
    /// NEWER emitter carrying extra fields must still decode (cross-node
    /// version skew is normal mesh weather; a hard decode failure would
    /// brick the parity reader on the first schema addition).
    #[test]
    fn extra_fields_from_newer_emitters_are_tolerated() {
        let mut value = serde_json::to_value(sample()).expect("to_value");
        value["future_field"] = serde_json::json!(42);
        let decoded: PagerCaptureEvent =
            serde_json::from_value(value).expect("decode with unknown field");
        assert_eq!(decoded.resident_experts, 4416);
    }

    /// what this catches (#276 CROSS-NODE PARITY, the real thing):
    /// three VERBATIM lines from BigMama's live fitted-K3 capture feed
    /// (2026-08-01 warm-decode run — cold token 0 at 20GB fetched, warm
    /// tokens 4/26 at the 4416 working set) must decode. Pins every
    /// live-feed quirk at once: integer literals in f32 fields
    /// (`"fetch_mb_s":2458`), her two extra counters (`experts`,
    /// `misses` — forward tolerance), and absent decision state
    /// (defaults). If this breaks, her feed and our reader have
    /// diverged — fix the CONTRACT, not the fixture.
    #[test]
    fn bigmama_live_feed_lines_decode_verbatim() {
        let live_lines = [
            r#"{"token":0,"hit_rate":1.0000,"fault_wait_ms":8178.0,"tok_per_s":0.0211,"bytes_fetched_mb":20098.2,"fetch_mb_s":2458,"resident_experts":8037,"experts":8037,"misses":0}"#,
            r#"{"token":4,"hit_rate":1.0000,"fault_wait_ms":1670.3,"tok_per_s":0.3295,"bytes_fetched_mb":4809.7,"fetch_mb_s":2879,"resident_experts":4416,"experts":4416,"misses":0}"#,
            r#"{"token":26,"hit_rate":1.0000,"fault_wait_ms":1545.6,"tok_per_s":0.3434,"bytes_fetched_mb":3891.6,"fetch_mb_s":2518,"resident_experts":4416,"experts":4416,"misses":0}"#,
        ];
        let decoded: Vec<PagerCaptureEvent> = live_lines
            .iter()
            .map(|line| serde_json::from_str(line).expect("live feed line decodes"))
            .collect();
        // The measured shape of the run: cold token pulls the whole
        // resident set; warm tokens settle on the 4416 working set.
        assert_eq!(decoded[0].token, 0);
        assert_eq!(decoded[0].resident_experts, 8037);
        assert_eq!(decoded[1].resident_experts, 4416);
        assert_eq!(decoded[2].resident_experts, 4416);
        assert!(decoded[2].tok_per_s > 0.3, "warm decode rate present");
        assert_eq!(decoded[1].fetch_mb_s, 2879.0, "int literal → f32");
    }

    /// what this catches: RAW-feed tolerance — the C++ emitter
    /// (GGML_MOE_CAPTURE_FILE) carries ONLY the perf fields; the
    /// decision state exists once the Rust controller runs. A perf-only
    /// line — exactly the shape BigMama's live fitted-K3 feed emits —
    /// must decode with zeroed decision fields, or the parity reader
    /// rejects every real capture slice she sends.
    #[test]
    fn raw_perf_only_line_decodes_with_defaulted_decision_state() {
        let raw = r#"{
            "token": 4,
            "hit_rate": 0.62,
            "fault_wait_ms": 1500.0,
            "tok_per_s": 0.326,
            "bytes_fetched_mb": 4096.0,
            "fetch_mb_s": 2458.0,
            "resident_experts": 4416
        }"#;
        let decoded: PagerCaptureEvent =
            serde_json::from_str(raw).expect("raw perf-only line decodes");
        assert_eq!(decoded.resident_experts, 4416);
        assert_eq!(decoded.fetch_mb_s, 2458.0);
        assert_eq!(decoded.reward, 0.0, "absent decision state defaults");
        assert!(decoded.per_arm_reward.is_empty());
        assert!(decoded.tier_counts.is_empty());
    }
}
