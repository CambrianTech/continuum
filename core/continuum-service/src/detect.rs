//! Judging one window against what its configuration and workload measured before.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::baseline::{self, Baseline, BaselineKey};
use crate::sample::{ServiceSample, Warmup, SCHEMA};

/// The caller's bounds. None is a constant here: the node's owner (and, later, a learned
/// policy) sets them, and every test names the values it uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServicePolicy {
    /// Healthy windows each key remembers.
    pub history: usize,
    /// Healthy windows a key needs before a window can be judged against it.
    pub min_evidence: usize,
    /// Windows in a row outside the envelope before it is a regression, not a wobble.
    pub sustained: u32,
}

/// Every key's baseline, and per node, model and builds the prediction ratios its healthy
/// windows measured. The caller owns it (no global state) and may persist it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ServiceState {
    pub baselines: BTreeMap<BaselineKey, Baseline>,
    pub ratios: BTreeMap<RatioKey, RatioBaseline>,
}

/// What prediction ratios are compared across: one node, model and builds, any configuration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RatioKey {
    pub node: String,
    pub model: String,
    pub builds: crate::sample::Builds,
}

impl RatioKey {
    fn of(sample: &ServiceSample) -> Self {
        Self { node: sample.node.clone(), model: sample.model.clone(), builds: sample.builds.clone() }
    }
}

/// Healthy windows' observed-over-predicted ratios, newest last, and the run outside them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RatioBaseline {
    pub ratios: std::collections::VecDeque<f64>,
    pub streak: u32,
}

impl RatioBaseline {
    fn remember(&mut self, ratio: f64, history: usize) {
        self.ratios.push_back(ratio);
        while self.ratios.len() > history {
            self.ratios.pop_front();
        }
    }

    fn slowest(&self) -> Option<f64> {
        self.ratios.iter().copied().reduce(f64::max)
    }
}

/// Why a window could not be judged. Never read as healthy or as degraded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Insufficient {
    UnknownSchema(u16),
    /// Neither this configuration's history nor the node's prediction ratios are enough yet.
    NoReference,
    /// The window spans more than one incarnation or configuration.
    Mixed,
    WarmingUp,
    /// No reuse history to tell whether the engine is warm.
    UnknownWarmup,
    /// Nothing was asked, so nothing can be slow.
    NoDemand,
}

/// What may explain a regression, for the reaction to weigh. Candidates, never verdicts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CauseHint {
    /// The engine process changed since every healthy window: it came back different.
    EngineRestartedSinceBaseline,
    /// The engine refused or failed generations in this window.
    Failures(u32),
    /// Requests waited longer than any healthy window: admission, not compute.
    QueueGrew { observed_ms: u64, baseline_max_ms: u64 },
    /// Slower than the node's own measured engine rates predict, by more than any healthy window.
    SlowerThanItsRates { ratio_milli: u64, slowest_healthy_milli: u64 },
}

/// What a window was judged against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reference {
    /// This configuration and workload's own healthy windows.
    OwnHistory { fastest_ms: u64, slowest_ms: u64 },
    /// The node's prediction ratios, for a configuration without enough history (milli-units).
    PredictionRatio { observed_milli: u64, slowest_healthy_milli: u64 },
}

/// A sustained slowdown, with what it was measured against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Regression {
    pub key: BaselineKey,
    pub observed_p50_ms: u64,
    pub against: Reference,
    pub streak: u32,
    pub causes: Vec<CauseHint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    Healthy,
    /// Outside the envelope, not yet for long enough to be a regression.
    Suspect { streak: u32, sustained: u32 },
    Regressed(Regression),
    Insufficient(Insufficient),
}

/// Judge `sample` against its configuration's own history, or, while that is too short,
/// against the node's healthy prediction ratios; learn from it only when it is healthy. A slow
/// window never joins either baseline, so a degraded engine cannot become its own reference.
pub fn observe(state: &mut ServiceState, policy: &ServicePolicy, sample: &ServiceSample) -> Verdict {
    if let Some(reason) = unjudgeable(sample) {
        return Verdict::Insufficient(reason);
    }
    let key = BaselineKey::of(sample);
    let own = state.baselines.entry(key.clone()).or_default();
    let ratios = state.ratios.entry(RatioKey::of(sample)).or_default();
    let ratio = sample.prediction_ratio();
    let judged = if own.references.len() >= policy.min_evidence {
        own.envelope().map(|(fastest, slowest)| {
            (sample.latency_p50_ms > slowest, Reference::OwnHistory { fastest_ms: fastest, slowest_ms: slowest })
        })
    } else if ratios.ratios.len() >= policy.min_evidence {
        ratio.zip(ratios.slowest()).map(|(r, slowest)| {
            (r > slowest, Reference::PredictionRatio { observed_milli: milli(r), slowest_healthy_milli: milli(slowest) })
        })
    } else {
        None
    };
    let Some((slow, against)) = judged else {
        // Nothing to judge against yet, so nothing is judged: these first windows become the
        // reference. Known limit: a node degraded from its very first window on new builds
        // learns that as its normal. The prediction ratios keep a new CONFIGURATION on known
        // builds honest; a new build needs one healthy start to be judged against.
        own.remember(baseline::Reference::of(sample), policy.history);
        if let Some(r) = ratio {
            ratios.remember(r, policy.history);
        }
        return Verdict::Insufficient(Insufficient::NoReference);
    };
    if !slow {
        own.streak = 0;
        own.remember(baseline::Reference::of(sample), policy.history);
        if let Some(r) = ratio {
            ratios.remember(r, policy.history);
        }
        return Verdict::Healthy;
    }
    own.streak += 1;
    if own.streak < policy.sustained {
        return Verdict::Suspect { streak: own.streak, sustained: policy.sustained };
    }
    let causes = causes(own, ratios, sample);
    Verdict::Regressed(Regression { key, observed_p50_ms: sample.latency_p50_ms, against, streak: own.streak, causes })
}

fn milli(ratio: f64) -> u64 {
    (ratio * 1000.0).round().max(0.0) as u64
}

fn unjudgeable(sample: &ServiceSample) -> Option<Insufficient> {
    if sample.schema != SCHEMA {
        return Some(Insufficient::UnknownSchema(sample.schema));
    }
    if sample.mixed {
        return Some(Insufficient::Mixed);
    }
    match sample.warmup {
        Warmup::WarmingUp => return Some(Insufficient::WarmingUp),
        Warmup::Unknown => return Some(Insufficient::UnknownWarmup),
        Warmup::Warm => {}
    }
    (sample.demand.offered == 0 || sample.generations == 0).then_some(Insufficient::NoDemand)
}

fn causes(baseline: &Baseline, ratios: &RatioBaseline, sample: &ServiceSample) -> Vec<CauseHint> {
    let mut causes = Vec::new();
    if let (Some(r), Some(slowest)) = (sample.prediction_ratio(), ratios.slowest()) {
        if r > slowest {
            causes.push(CauseHint::SlowerThanItsRates { ratio_milli: milli(r), slowest_healthy_milli: milli(slowest) });
        }
    }
    if !baseline.has_incarnation(sample.incarnation) {
        causes.push(CauseHint::EngineRestartedSinceBaseline);
    }
    if sample.failures > 0 {
        causes.push(CauseHint::Failures(sample.failures));
    }
    if let Some(max) = baseline.max_queued_ms() {
        if sample.demand.queued_ms_p50 > max {
            causes.push(CauseHint::QueueGrew { observed_ms: sample.demand.queued_ms_p50, baseline_max_ms: max });
        }
    }
    causes
}

#[cfg(test)]
mod tests;
