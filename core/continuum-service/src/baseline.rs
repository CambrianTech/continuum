//! What one configuration, under one kind of workload, has measured before.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::sample::{Builds, EngineIncarnation, ServiceSample};

/// The workload a window carried, by what it costs the engine: fresh prefill tokens, output
/// tokens and the minds asking. Token counts fall in powers of two (ballpark: an act twice as
/// large is a different workload, one 10% larger is not), so an hour of bigger acts gets its own
/// baseline instead of reading as a slower engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Stratum {
    pub residents_active: u32,
    pub fresh_tokens_log2: u32,
    pub output_tokens_log2: u32,
}

impl Stratum {
    pub fn of(sample: &ServiceSample) -> Self {
        let w = sample.workload;
        let fresh = (f64::from(w.prompt_tokens_p50) * f64::from(1.0 - w.cached_ratio_p50.clamp(0.0, 1.0))) as u64;
        Self {
            residents_active: w.residents_active,
            fresh_tokens_log2: log2_bucket(fresh),
            output_tokens_log2: log2_bucket(u64::from(w.output_tokens_p50)),
        }
    }
}

fn log2_bucket(n: u64) -> u32 {
    u64::BITS - n.leading_zeros()
}

/// Everything a window's speed may honestly be compared across: the same node, model, builds,
/// configuration and workload. An engine restart is NOT part of it (the same configuration
/// after a restart is the same baseline), so an engine that came back slower stands out.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BaselineKey {
    pub node: String,
    pub model: String,
    pub builds: Builds,
    pub config: u64,
    pub stratum: Stratum,
}

impl BaselineKey {
    pub fn of(sample: &ServiceSample) -> Self {
        Self {
            node: sample.node.clone(),
            model: sample.model.clone(),
            builds: sample.builds.clone(),
            config: sample.config.fingerprint(),
            stratum: Stratum::of(sample),
        }
    }
}

/// One healthy window this key measured.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Reference {
    pub latency_p50_ms: u64,
    pub queued_ms_p50: u64,
    pub incarnation: EngineIncarnation,
}

impl Reference {
    pub fn of(sample: &ServiceSample) -> Self {
        Self {
            latency_p50_ms: sample.latency_p50_ms,
            queued_ms_p50: sample.demand.queued_ms_p50,
            incarnation: sample.incarnation,
        }
    }
}

/// The healthy windows one key remembers, newest last.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Baseline {
    pub references: VecDeque<Reference>,
}

impl Baseline {
    pub fn remember(&mut self, reference: Reference, history: usize) {
        self.references.push_back(reference);
        while self.references.len() > history {
            self.references.pop_front();
        }
    }

    /// The slowest and fastest healthy windows: the envelope this key has measured.
    pub fn envelope(&self) -> Option<(u64, u64)> {
        let lows = self.references.iter().map(|r| r.latency_p50_ms);
        Some((lows.clone().min()?, lows.max()?))
    }

    pub fn max_queued_ms(&self) -> Option<u64> {
        self.references.iter().map(|r| r.queued_ms_p50).max()
    }

    pub fn has_incarnation(&self, incarnation: EngineIncarnation) -> bool {
        self.references.iter().any(|r| r.incarnation == incarnation)
    }
}
