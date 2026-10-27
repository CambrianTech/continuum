//! Judging one window against what its configuration and workload measured before.
//!
//! Windows arrive as one stream per node. A window is ADJACENT to the last one judged when it
//! starts where that one ended and runs on the same engine incarnation; only adjacent slow
//! windows make a streak. A duplicate, overlapping or out-of-order window is refused before it
//! can be judged or learned, and a gap, a restart or a window that cannot be judged ends a
//! streak, so "sustained" always means consecutive real time on one engine.

use std::collections::{BTreeMap, VecDeque};

use serde::{Deserialize, Serialize};

use crate::baseline::{self, Baseline, BaselineKey};
use crate::sample::{Builds, EngineIncarnation, ServiceSample, Warmup, SCHEMA};

/// The caller's bounds. None is a constant here: the node's owner (and, later, a learned
/// policy) sets them, and every test names the values it uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServicePolicy {
    /// Healthy windows each key remembers.
    pub history: usize,
    /// Healthy windows a key needs before a window can be judged against it.
    pub min_evidence: usize,
    /// Adjacent windows in a row outside the envelope before it is a regression, not a wobble.
    pub sustained: u32,
}

/// Every key's baseline, the node's healthy prediction ratios, and each node stream's position.
/// The caller owns it (no global state) and may persist it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ServiceState {
    pub baselines: BTreeMap<BaselineKey, Baseline>,
    pub ratios: BTreeMap<RatioKey, RatioBaseline>,
    pub streams: BTreeMap<String, StreamPosition>,
}

/// Where one node's stream of windows stands, and the slow run it is in. A run is bound to one
/// comparison: the same engine incarnation, baseline key (model, builds, configuration,
/// workload) and reference (own history, or prediction ratios under one calibration).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamPosition {
    pub last_end_ms: u64,
    pub incarnation: EngineIncarnation,
    pub streak: u32,
    /// What the run's windows were compared against; `None` until a window is judged.
    pub comparison: Option<Comparison>,
}

/// The identity of what a slow window was judged against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comparison {
    pub key: BaselineKey,
    /// `None` when judged by the configuration's own history; the calibration when judged by ratios.
    pub calibration: Option<String>,
}

/// What prediction ratios are compared across: one node, model, builds and predictor calibration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RatioKey {
    pub node: String,
    pub model: String,
    pub builds: Builds,
    pub calibration: String,
}

impl RatioKey {
    fn of(sample: &ServiceSample) -> Option<Self> {
        let calibration = sample.prediction.as_ref()?.calibration.clone();
        Some(Self { node: sample.node.clone(), model: sample.model.clone(), builds: sample.builds.clone(), calibration })
    }
}

/// Healthy windows' observed-over-predicted ratios, newest last.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RatioBaseline {
    pub ratios: VecDeque<f64>,
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
    /// The window has no duration (it ends at or before it starts).
    EmptyWindow { start_ms: u64, end_ms: u64 },
    /// The window starts before the node's last window ended: a duplicate, an overlap or out of order.
    NotAfterLast { start_ms: u64, last_end_ms: u64 },
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
    /// Work was asked for and nothing completed: the worst slowdown, never idle, never healthy.
    Stalled { offered: u32, failures: u32 },
    Insufficient(Insufficient),
}

/// Judge `sample` against its configuration's own history, or, while that is too short,
/// against the node's healthy prediction ratios; learn from it only when it is healthy. A slow
/// window never joins either baseline, so a degraded engine cannot become its own reference.
pub fn observe(state: &mut ServiceState, policy: &ServicePolicy, sample: &ServiceSample) -> Verdict {
    if sample.schema != SCHEMA {
        return Verdict::Insufficient(Insufficient::UnknownSchema(sample.schema));
    }
    if sample.window.end_ms <= sample.window.start_ms {
        return Verdict::Insufficient(Insufficient::EmptyWindow { start_ms: sample.window.start_ms, end_ms: sample.window.end_ms });
    }
    let streak = match advance(state, sample) {
        Ok(streak) => streak,
        Err(refused) => return Verdict::Insufficient(refused),
    };
    if let Some(verdict) = unjudgeable(sample) {
        set_streak(state, sample, 0);
        return verdict;
    }
    let key = BaselineKey::of(sample);
    let ratio_key = RatioKey::of(sample);
    let ratio = sample.prediction_ratio();
    let own = state.baselines.entry(key.clone()).or_default();
    let ratios = match ratio_key.as_ref() {
        Some(k) => state.ratios.entry(k.clone()).or_default().clone(),
        None => RatioBaseline::default(),
    };
    let calibration = sample.prediction.as_ref().map(|p| p.calibration.clone());
    let judged = if own.references.len() >= policy.min_evidence {
        own.envelope().map(|(fastest, slowest)| {
            (sample.latency_p50_ms > slowest, Reference::OwnHistory { fastest_ms: fastest, slowest_ms: slowest }, None)
        })
    } else if ratios.ratios.len() >= policy.min_evidence {
        ratio.zip(ratios.slowest()).map(|(r, slowest)| {
            (r > slowest, Reference::PredictionRatio { observed_milli: milli(r), slowest_healthy_milli: milli(slowest) }, calibration)
        })
    } else {
        None
    };
    let Some((slow, against, judged_calibration)) = judged else {
        // Nothing to judge against yet, so nothing is judged: these first windows become the
        // reference. Known limit: a node degraded from its very first window on new builds
        // learns that as its normal. The prediction ratios keep a new CONFIGURATION on known
        // builds honest; a new build needs one healthy start to be judged against.
        learn(state, policy, sample, &key, ratio_key.as_ref(), ratio);
        set_streak(state, sample, 0);
        return Verdict::Insufficient(Insufficient::NoReference);
    };
    if !slow {
        learn(state, policy, sample, &key, ratio_key.as_ref(), ratio);
        set_streak(state, sample, 0);
        return Verdict::Healthy;
    }
    // A run continues only against the same comparison: a change of configuration, builds,
    // model, workload stratum or reference starts a new one.
    let comparison = Comparison { key: key.clone(), calibration: judged_calibration };
    let continued = state.streams.get(&sample.node).and_then(|p| p.comparison.as_ref()) == Some(&comparison);
    let streak = if continued { streak + 1 } else { 1 };
    set_streak(state, sample, streak);
    if let Some(position) = state.streams.get_mut(&sample.node) {
        position.comparison = Some(comparison);
    }
    if streak < policy.sustained {
        return Verdict::Suspect { streak, sustained: policy.sustained };
    }
    let own = state.baselines.entry(key.clone()).or_default().clone();
    let causes = causes(&own, &ratios, sample);
    Verdict::Regressed(Regression { key, observed_p50_ms: sample.latency_p50_ms, against, streak, causes })
}

/// Move the node's stream to `sample`, returning the slow streak it continues (0 after a gap or
/// a restart), or refuse a window that does not come after the last one.
fn advance(state: &mut ServiceState, sample: &ServiceSample) -> Result<u32, Insufficient> {
    let previous = state.streams.get(&sample.node).cloned();
    if let Some(last) = previous.as_ref() {
        if sample.window.start_ms < last.last_end_ms {
            return Err(Insufficient::NotAfterLast { start_ms: sample.window.start_ms, last_end_ms: last.last_end_ms });
        }
    }
    let continuing = previous
        .filter(|last| last.last_end_ms == sample.window.start_ms && last.incarnation == sample.incarnation);
    let streak = continuing.as_ref().map_or(0, |last| last.streak);
    let comparison = continuing.and_then(|last| last.comparison);
    state.streams.insert(
        sample.node.clone(),
        StreamPosition { last_end_ms: sample.window.end_ms, incarnation: sample.incarnation, streak, comparison },
    );
    Ok(streak)
}

fn set_streak(state: &mut ServiceState, sample: &ServiceSample, streak: u32) {
    if let Some(position) = state.streams.get_mut(&sample.node) {
        position.streak = streak;
    }
}

fn learn(
    state: &mut ServiceState,
    policy: &ServicePolicy,
    sample: &ServiceSample,
    key: &BaselineKey,
    ratio_key: Option<&RatioKey>,
    ratio: Option<f64>,
) {
    state.baselines.entry(key.clone()).or_default().remember(baseline::Reference::of(sample), policy.history);
    if let (Some(k), Some(r)) = (ratio_key, ratio) {
        state.ratios.entry(k.clone()).or_default().remember(r, policy.history);
    }
}

fn milli(ratio: f64) -> u64 {
    (ratio * 1000.0).round().max(0.0) as u64
}

/// A window that cannot describe the engine's speed, with what it is instead.
///
/// Demand is read FIRST: a window where work was asked and nothing completed is a stall
/// whatever else it is. Warm-up and a mixed window qualify a SPEED; they cannot excuse zero
/// completions (a resident refused before inference has no reuse to warm, and a stall that
/// spans a restart is still a stall to the mind waiting on it).
fn unjudgeable(sample: &ServiceSample) -> Option<Verdict> {
    match (sample.demand.offered, sample.generations) {
        (0, _) => return Some(Verdict::Insufficient(Insufficient::NoDemand)),
        (offered, 0) => return Some(Verdict::Stalled { offered, failures: sample.failures }),
        _ => {}
    }
    if sample.mixed {
        return Some(Verdict::Insufficient(Insufficient::Mixed));
    }
    match sample.warmup {
        Warmup::WarmingUp => Some(Verdict::Insufficient(Insufficient::WarmingUp)),
        Warmup::Unknown => Some(Verdict::Insufficient(Insufficient::UnknownWarmup)),
        Warmup::Warm => None,
    }
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
