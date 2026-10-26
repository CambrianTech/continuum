//! One window of service on one node: what was configured, what ran it, what was asked of it,
//! and how fast it answered.

use serde::{Deserialize, Serialize};

/// The sample's schema; a reader refuses a version it does not know.
pub const SCHEMA: u16 = 1;

/// The configuration an engine serves under: every setting that changes its speed. Its
/// [`ServingConfig::fingerprint`] is the configuration's identity, independent of which
/// process ran it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServingConfig {
    pub lanes: u32,
    pub ctx_per_lane: u32,
    pub cache_ram_mib: u32,
    /// The KV cache's element type as the engine reports it (`f16`, `q8_0`, ...).
    pub kv_type: String,
    /// The compute backend (`metal`, `cuda`, `vulkan`, `cpu`).
    pub backend: String,
    /// Expert residency settings for a MoE, as the engine reports them; empty for a dense model.
    pub expert_residency: String,
}

impl ServingConfig {
    /// A stable identity for this configuration: the same settings give the same value on every
    /// node and every run (FNV-1a over the fields in a fixed order; ballpark identity, not
    /// security).
    pub fn fingerprint(&self) -> u64 {
        let text = format!(
            "{}|{}|{}|{}|{}|{}",
            self.lanes, self.ctx_per_lane, self.cache_ram_mib, self.kv_type, self.backend, self.expert_residency
        );
        text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3))
    }
}

/// One running engine process. A restart is a new incarnation of the same configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineIncarnation {
    pub pid: u32,
    pub started_ms: u64,
}

/// The builds that served the window.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Builds {
    pub core: String,
    pub engine: String,
}

/// What the minds asked of the node in the window, as medians.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Workload {
    pub residents_active: u32,
    pub prompt_tokens_p50: u32,
    pub output_tokens_p50: u32,
    /// Share of the prompt the engine already held, 0..=1.
    pub cached_ratio_p50: f32,
}

/// Demand offered in the window, so a quiet hour never reads as a slow engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Demand {
    /// Generations requested.
    pub offered: u32,
    /// Median time a request waited before the engine took it.
    pub queued_ms_p50: u64,
}

/// Whether the window's numbers describe the engine at its working speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Warmup {
    /// Past warm-up: the minds' reuse has recovered since this incarnation started.
    Warm,
    /// Inside warm-up after a start: caches refilling, so slow by design.
    WarmingUp,
    /// No reuse history to tell: never judged either way.
    Unknown,
}

/// What the node's measured engine rates predict for a window's median generation
/// (`serving_rates` and the contention dyno, computed by the caller). It makes windows of
/// different configurations comparable. `calibration` names the predictor's version: ratios are
/// only compared under one calibration, so a recalibrated predictor never reads as an engine
/// that got faster or slower.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prediction {
    pub p50_ms: u64,
    pub calibration: String,
}

/// The window the sample covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    pub start_ms: u64,
    pub end_ms: u64,
}

impl Window {
    pub fn hours(&self) -> f64 {
        self.end_ms.saturating_sub(self.start_ms) as f64 / 3_600_000.0
    }
}

/// One window of one node's service.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServiceSample {
    pub schema: u16,
    pub node: String,
    pub model: String,
    pub builds: Builds,
    pub config: ServingConfig,
    pub incarnation: EngineIncarnation,
    /// The window spans more than one incarnation or configuration: never judged.
    pub mixed: bool,
    pub window: Window,
    pub workload: Workload,
    pub demand: Demand,
    /// Engine generations completed (a citizen's act may take several).
    pub generations: u32,
    /// Citizen acts completed.
    pub acts: u32,
    /// Generations the engine refused or failed.
    pub failures: u32,
    pub latency_p50_ms: u64,
    pub latency_p90_ms: u64,
    /// What the node's measured engine rates predict for this workload's median generation; `None`
    /// when the rates are unmeasured.
    pub prediction: Option<Prediction>,
    pub warmup: Warmup,
    pub memory_ceiling_bytes: u64,
}

impl ServiceSample {
    /// Observed over predicted median latency: 1.0 is the engine at its measured speed.
    pub fn prediction_ratio(&self) -> Option<f64> {
        let predicted = self.prediction.as_ref().map(|p| p.p50_ms).filter(|p| *p > 0)?;
        Some(self.latency_p50_ms as f64 / predicted as f64)
    }

    /// Generations completed per hour of the window.
    pub fn per_hour(&self) -> f64 {
        let hours = self.window.hours();
        if hours > 0.0 {
            f64::from(self.generations) / hours
        } else {
            0.0
        }
    }
}
