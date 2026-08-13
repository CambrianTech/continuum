use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vdd/HarnessStatus.ts"
)]
#[serde(rename_all = "kebab-case")]
pub enum HarnessStatus {
    Pass,
    Fail,
    PrerequisiteMissing,
}

#[derive(Debug, Error)]
pub enum VddError {
    #[error("io error at {path:?}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("json serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("toml serialization failed: {0}")]
    Toml(#[from] toml::ser::Error),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StandardVddRecord {
    pub scenario: String,
    pub platform: String,
    pub hardware: String,
    pub backend: String,
    pub git_sha: String,
    pub command: String,
    pub model: Option<String>,
    pub gpu_layers: Option<u32>,
    pub unsupported_layers: Vec<String>,
    pub cold_start_ms: Option<u64>,
    pub first_token_ms: Option<u64>,
    pub first_response_ms: Option<u64>,
    pub all_responses_ms: Option<u64>,
    pub responses_expected: u32,
    pub responses_observed: u32,
    pub silence_reasons: Vec<String>,
    pub tok_per_sec: Option<f64>,
    pub cpu_pct_avg: Option<f64>,
    pub cpu_pct_peak: Option<f64>,
    pub rss_mb: Option<u64>,
    pub gpu_util_pct_avg: Option<f64>,
    pub gpu_memory_mb: Option<u64>,
    pub queue_wait_ms: Option<u64>,
    pub execution_ms: Option<u64>,
    pub coalesced_count: u32,
    pub deferred_count: u32,
    pub stale_drop_count: u32,
    pub error_count: u32,
    pub degraded_reason: Option<String>,
    pub log_refs: Vec<String>,
    pub next_bottleneck: Option<String>,
    pub policy_version: Option<String>,
    pub cascade_step: Option<u8>,
    pub status: HarnessStatus,
}

impl StandardVddRecord {
    /// A minimal record for a single measured command (the per-module VDD case,
    /// e.g. `ModuleHarness::measure`). Latency/score fields start `None` and the
    /// caller fills what it measured (`execution_ms`, `tok_per_sec`, …). Hardware/
    /// backend come from the same env vars as `chat_roundtrip`, so records from
    /// any harness compare apples-to-apples. Status starts `Pass` (a clean
    /// measurement); flip to `Fail` if a gate/assertion was violated.
    pub fn minimal(
        scenario: impl Into<String>,
        command: impl Into<String>,
        git_sha: impl Into<String>,
    ) -> Self {
        Self {
            scenario: scenario.into(),
            platform: std::env::consts::OS.to_string(),
            hardware: std::env::var("CONTINUUM_HARNESS_HARDWARE_CLASS")
                .unwrap_or_else(|_| "unknown".to_string()),
            backend: std::env::var("CONTINUUM_HARNESS_BACKEND")
                .unwrap_or_else(|_| "unknown".to_string()),
            git_sha: git_sha.into(),
            command: command.into(),
            model: None,
            gpu_layers: None,
            unsupported_layers: Vec::new(),
            cold_start_ms: None,
            first_token_ms: None,
            first_response_ms: None,
            all_responses_ms: None,
            responses_expected: 0,
            responses_observed: 0,
            silence_reasons: Vec::new(),
            tok_per_sec: None,
            cpu_pct_avg: None,
            cpu_pct_peak: None,
            rss_mb: None,
            gpu_util_pct_avg: None,
            gpu_memory_mb: None,
            queue_wait_ms: None,
            execution_ms: None,
            coalesced_count: 0,
            deferred_count: 0,
            stale_drop_count: 0,
            error_count: 0,
            degraded_reason: None,
            log_refs: Vec::new(),
            next_bottleneck: None,
            policy_version: None,
            cascade_step: None,
            status: HarnessStatus::Pass,
        }
    }

    pub fn chat_roundtrip(
        git_sha: impl Into<String>,
        command: impl Into<String>,
        expected: u32,
    ) -> Self {
        Self {
            scenario: "chat-roundtrip-live-harness".to_string(),
            platform: std::env::consts::OS.to_string(),
            hardware: std::env::var("CONTINUUM_HARNESS_HARDWARE_CLASS")
                .unwrap_or_else(|_| "unknown".to_string()),
            backend: std::env::var("CONTINUUM_HARNESS_BACKEND")
                .unwrap_or_else(|_| "unknown".to_string()),
            git_sha: git_sha.into(),
            command: command.into(),
            model: None,
            gpu_layers: None,
            unsupported_layers: Vec::new(),
            cold_start_ms: None,
            first_token_ms: None,
            first_response_ms: None,
            all_responses_ms: None,
            responses_expected: expected,
            responses_observed: 0,
            silence_reasons: Vec::new(),
            tok_per_sec: None,
            cpu_pct_avg: None,
            cpu_pct_peak: None,
            rss_mb: None,
            gpu_util_pct_avg: None,
            gpu_memory_mb: None,
            queue_wait_ms: None,
            execution_ms: None,
            coalesced_count: 0,
            deferred_count: 0,
            stale_drop_count: 0,
            error_count: 0,
            degraded_reason: None,
            log_refs: Vec::new(),
            next_bottleneck: None,
            policy_version: None,
            cascade_step: None,
            status: HarnessStatus::Fail,
        }
    }
}
