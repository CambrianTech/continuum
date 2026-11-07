//! The cold-prefill knee (ONE-ACT 5c) and bounded warm decode under mixed load (5d), from the
//! recorded curves (`fixtures/dyno/2026-11-04-prefill-knee.json`, every value beside its receipt).
//!
//! Two things kept apart, as Codex specified:
//! - the POLICY invariant: active cold prefills never exceed the node's knee, and the knee is
//!   DERIVED from the measured aggregate prefill curve (the concurrency past which the aggregate
//!   stops rising beyond its noise), never a number; with no measured noise band a rising step is
//!   a typed unknown ([`Knee::NeedsNoiseBand`]), not a guess;
//! - the OUTCOME gates of a schedule: a warm decoder is eligible at the next scheduling boundary
//!   (its longest decode gap bounded by the measured chunk time at the knee, never zero wait),
//!   every cold prefill completes within its named aging bound (FIFO service at the knee), and the
//!   makespan does not exceed the measured serialized makespan.
//!
//! The service-curve model ([`Engine`]) is a prediction from measured costs, validated against
//! the recorded runs within a stated error (`knee/tests.rs`); it is not a measurement and it
//! models no acceleration. Admission is TODAY's production gate
//! (`resource_admission::LaneAdmission`), driven through its own request futures.
//! Classification: production-policy replay (admission) beside a calibrated prediction (engine).
#![cfg(test)]

use serde::Deserialize;

use super::recorded::Receipted;

const EVIDENCE_JSON: &str = include_str!("../../../fixtures/dyno/2026-11-04-prefill-knee.json");

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Evidence {
    pub intelmac_prefill_only: Receipted<PrefillOnly>,
    pub intelmac_decode_only: Receipted<DecodeOnly>,
    pub intelmac_mixed: Receipted<Mixed>,
    pub intelmac_live_hour: Receipted<LiveHour>,
    pub m5_chunk_sweep: Receipted<ChunkSweep>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct PrefillOnly {
    pub prompt_tokens: u32,
    pub ubatch: u32,
    pub served_slots: usize,
    pub trials: Vec<PrefillTrial>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct PrefillTrial {
    pub concurrency: u32,
    pub wall_s: f64,
    pub aggregate_tok_per_s: f64,
    pub completions_s: Vec<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct DecodeOnly {
    pub trials: Vec<DecodeTrial>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct DecodeTrial {
    pub concurrency: u32,
    pub aggregate_tok_per_s: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Mixed {
    pub cold_prompt_tokens: u32,
    pub warm_prompt_tokens: u32,
    pub decode_tokens: u32,
    pub trials: Vec<MixedTrial>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct MixedTrial {
    pub cold: u32,
    pub warm: u32,
    pub wall_s: f64,
    pub cold_completions_s: Vec<f64>,
    pub warm_completions_s: Vec<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct LiveHour {
    pub served_lanes: usize,
    pub lane_grants_s: Vec<f64>,
    pub first_completion: LiveCompletion,
    pub regrant_s: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct LiveCompletion {
    pub cached_tokens: u32,
    pub prefill_tokens: u32,
    pub prefill_ms: u64,
    pub decode_tokens: u32,
    pub decode_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ChunkSweep {
    pub warm_prompt_tokens: u32,
    pub warm_cached_tokens: u32,
    pub cold_prompt_tokens: u32,
    pub decode_tokens: u32,
    pub cold_starts_after_s: f64,
    pub points: Vec<ChunkPoint>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ChunkPoint {
    pub ubatch: u32,
    pub solo_prefill_tok_per_s: Vec<f64>,
    pub solo_decode_tok_per_s: Vec<f64>,
    pub warm_decode_tok_per_s: Vec<f64>,
    pub warm_max_gap_s: Vec<f64>,
    pub cold_wall_s: Vec<f64>,
}

impl Evidence {
    pub(crate) fn load() -> Result<Self, serde_json::Error> {
        serde_json::from_str(EVIDENCE_JSON) // file format: the recorded knee evidence, 2026-11-04-prefill-knee.json
    }

    /// The IntelMac's aggregate prefill curve (prefill-only trials).
    pub(crate) fn intelmac_prefill(&self) -> ServiceCurve {
        ServiceCurve::new(self.intelmac_prefill_only.value.trials.iter().map(|t| (t.concurrency, t.aggregate_tok_per_s)).collect())
    }

    /// The IntelMac's aggregate decode curve (decode-only trials).
    pub(crate) fn intelmac_decode(&self) -> ServiceCurve {
        ServiceCurve::new(self.intelmac_decode_only.value.trials.iter().map(|t| (t.concurrency, t.aggregate_tok_per_s)).collect())
    }
}

/// The mean of a point's repeats.
pub(crate) fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len().max(1) as f64
}

/// Aggregate tokens per second against concurrency, as measured. Between two measured
/// concurrencies it is linear; past the measured ends it is held flat (no invented physics).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ServiceCurve {
    points: Vec<(u32, f64)>,
}

impl ServiceCurve {
    pub(crate) fn new(mut points: Vec<(u32, f64)>) -> Self {
        points.sort_by_key(|(n, _)| *n);
        Self { points }
    }

    pub(crate) fn points(&self) -> &[(u32, f64)] {
        &self.points
    }

    /// The aggregate rate at `n` concurrent streams (`n >= 1`).
    pub(crate) fn at(&self, n: u32) -> f64 {
        let n = n.max(1);
        let (Some(first), Some(last)) = (self.points.first(), self.points.last()) else { return 0.0 };
        if n <= first.0 {
            return first.1;
        }
        if n >= last.0 {
            return last.1;
        }
        self.points
            .windows(2)
            .find(|w| w[0].0 <= n && n <= w[1].0)
            .map(|w| w[0].1 + (w[1].1 - w[0].1) * f64::from(n - w[0].0) / f64::from(w[1].0 - w[0].0))
            .unwrap_or(last.1) // unwrap_or: a sorted curve brackets every n between its ends
    }
}

/// How far an aggregate reading moves between repeats of the same configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum NoiseBand {
    /// Measured from repeats, tokens per second.
    Measured(f64),
    /// No repeats were recorded.
    Unmeasured,
}

/// The knee: the most concurrent streams worth running at once on this node and configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Knee {
    Derived(u32),
    /// The step `below -> above` raised the aggregate by `rise`, and whether that is past the
    /// noise cannot be decided without a measured band. Typed, never assumed.
    NeedsNoiseBand { below: u32, above: u32, rise: f64 },
}

/// The concurrency past which the measured aggregate stops rising beyond its noise.
pub(crate) fn derive_knee(curve: &ServiceCurve, noise: NoiseBand) -> Knee {
    let points = curve.points();
    let Some(&(mut knee, _)) = points.first() else { return Knee::Derived(1) };
    for pair in points.windows(2) {
        let ((below, at_below), (above, at_above)) = (pair[0], pair[1]);
        let rise = at_above - at_below;
        if rise <= 0.0 {
            return Knee::Derived(knee);
        }
        match noise {
            NoiseBand::Measured(band) if rise <= band => return Knee::Derived(knee),
            NoiseBand::Measured(_) => knee = above,
            NoiseBand::Unmeasured => return Knee::NeedsNoiseBand { below, above, rise },
        }
    }
    Knee::Derived(knee)
}

/// The engine as the model serves it: the measured aggregate prefill and decode curves and the
/// configured ubatch. One scheduling step serves one ubatch of prompt tokens (filled from the
/// prefilling requests in slot order, which is admission order) at the aggregate rate for the
/// number of requests still prefilling, then one token for every decoding request at the
/// decode curve's per-step cost. So a decoding stream's longest gap is one chunk plus one decode
/// step (validated on the M5 sweep), and a warm request whose short tail sits behind cold
/// prompts in slot order waits for all of them (the IntelMac mixed trial). It is an upper bound
/// on a warm stream's completion latency: the M5's fork interleaves less often than every token.
#[derive(Debug, Clone)]
pub(crate) struct Engine {
    pub prefill: ServiceCurve,
    pub decode: ServiceCurve,
    pub ubatch: u32,
}

/// One request of a trace.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Request {
    pub arrival_s: f64,
    /// Prompt tokens to prefill (a warm request's uncached tail).
    pub prompt_tokens: u32,
    pub decode_tokens: u32,
    pub warm: bool,
}

/// What one request saw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Outcome {
    pub admitted_s: f64,
    pub first_token_s: f64,
    /// The longest gap between two consecutive tokens of hers (0 with one token).
    pub longest_gap_s: f64,
    pub completion_s: f64,
}

/// A trace's outcome under one admission.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Run {
    pub outcomes: Vec<Outcome>,
    pub makespan_s: f64,
    pub aggregate_tok_per_s: f64,
    /// The oldest age any request reached before its first token.
    pub oldest_waiter_s: f64,
    /// The most cold requests prefilling at once.
    pub max_active_cold: u32,
}

/// The gate that admits a request to the engine: today's is `LaneAdmission`.
pub(crate) trait Admission {
    /// Try to admit request `index` now; `true` once admitted.
    fn try_admit(&mut self, index: usize) -> bool;
    /// Request `index` completed: its lane is released.
    fn release(&mut self, index: usize);
}

#[derive(Debug, Clone, Copy)]
struct Live {
    prompt_left: u32,
    decoded: u32,
    last_token_s: f64,
}

impl Engine {
    /// Serve `trace` under `admission` until every request completes. Admission is retried at
    /// every scheduling boundary, in arrival order; the clock jumps to the next arrival when the
    /// engine is idle.
    pub(crate) fn run(&self, trace: &[Request], admission: &mut dyn Admission) -> Run {
        let n = trace.len();
        let mut now = 0.0f64;
        let mut admitted: Vec<usize> = Vec::new();
        let mut live: Vec<Option<Live>> = vec![None; n];
        let mut outcomes: Vec<Option<Outcome>> = vec![None; n];
        let mut done = 0usize;
        let mut max_active_cold = 0u32;
        while done < n {
            for i in 0..n {
                if live[i].is_none() && outcomes[i].is_none() && trace[i].arrival_s <= now && admission.try_admit(i) {
                    admitted.push(i);
                    live[i] = Some(Live { prompt_left: trace[i].prompt_tokens, decoded: 0, last_token_s: now });
                    outcomes[i] = Some(Outcome { admitted_s: now, first_token_s: f64::NAN, longest_gap_s: 0.0, completion_s: f64::NAN });
                }
            }
            let active: Vec<usize> = admitted.iter().copied().filter(|i| live[*i].is_some()).collect();
            if active.is_empty() {
                let next = trace.iter().zip(&outcomes).filter(|(_, o)| o.is_none()).map(|(r, _)| r.arrival_s).fold(f64::INFINITY, f64::min);
                if !next.is_finite() || next <= now {
                    break; // admission refuses everything arrived: nothing more can run
                }
                now = next;
                continue;
            }
            let prefilling: Vec<usize> = active.iter().copied().filter(|i| live[*i].is_some_and(|l| l.prompt_left > 0)).collect();
            let decoding: Vec<usize> = active.iter().copied().filter(|i| live[*i].is_some_and(|l| l.prompt_left == 0)).collect();
            max_active_cold = max_active_cold.max(prefilling.iter().filter(|i| !trace[**i].warm).count() as u32);
            let mut budget = self.ubatch;
            let mut finished_prompt = Vec::new();
            let mut served = 0u32;
            for i in &prefilling {
                let Some(l) = live[*i].as_mut() else { continue };
                let take = l.prompt_left.min(budget);
                l.prompt_left -= take;
                budget -= take;
                served += take;
                if l.prompt_left == 0 {
                    finished_prompt.push(*i);
                }
                if budget == 0 {
                    break;
                }
            }
            let mut step = 0.0;
            if served > 0 {
                // The curve was measured on cold prompts: a warm tail rides the chunk.
                let cold = prefilling.iter().filter(|i| !trace[**i].warm).count().max(1);
                step += f64::from(served) / self.prefill.at(cold as u32);
            }
            let tokens: Vec<usize> = decoding.iter().chain(&finished_prompt).copied().collect();
            if !decoding.is_empty() {
                step += decoding.len() as f64 / self.decode.at(decoding.len() as u32);
            }
            now += step;
            for i in tokens {
                let (Some(l), Some(o)) = (live[i].as_mut(), outcomes[i].as_mut()) else { continue };
                if l.decoded == 0 {
                    o.first_token_s = now;
                } else {
                    o.longest_gap_s = o.longest_gap_s.max(now - l.last_token_s);
                }
                l.decoded += 1;
                l.last_token_s = now;
                if l.decoded >= trace[i].decode_tokens.max(1) {
                    o.completion_s = now;
                    live[i] = None;
                    done += 1;
                    admission.release(i);
                }
            }
        }
        let outcomes: Vec<Outcome> = outcomes.into_iter().map(|o| o.expect("model: every request was admitted and completed")).collect();
        let makespan_s = outcomes.iter().map(|o| o.completion_s).fold(0.0, f64::max);
        let tokens: u64 = trace.iter().map(|r| u64::from(r.prompt_tokens) + u64::from(r.decode_tokens)).sum();
        let oldest_waiter_s = trace.iter().zip(&outcomes).map(|(r, o)| o.first_token_s - r.arrival_s).fold(0.0, f64::max);
        Run { outcomes, makespan_s, aggregate_tok_per_s: tokens as f64 / makespan_s, oldest_waiter_s, max_active_cold }
    }
}

/// TODAY's admission: the production lane gate, its served lane count set as the engine's
/// `--parallel` is published, each request one `serving_lane_request` future polled at every
/// scheduling boundary (no runtime timer is involved on the directed path).
pub(crate) struct TodaysLanes {
    gate: &'static crate::inference::resource_admission::LaneAdmission,
    priority: crate::inference::resource_admission::LanePriority,
    pending: std::collections::BTreeMap<usize, std::pin::Pin<Box<dyn std::future::Future<Output = crate::inference::resource_admission::ServingLanePermit> + Send>>>,
    held: std::collections::BTreeMap<usize, crate::inference::resource_admission::ServingLanePermit>,
}

impl TodaysLanes {
    pub(crate) fn new(lanes: usize, priority: crate::inference::resource_admission::LanePriority) -> Self {
        let gate: &'static _ = Box::leak(Box::new(crate::inference::resource_admission::LaneAdmission::new()));
        gate.set_served_lane_count(lanes);
        Self { gate, priority, pending: Default::default(), held: Default::default() }
    }
}

impl Admission for TodaysLanes {
    fn try_admit(&mut self, index: usize) -> bool {
        if self.held.contains_key(&index) {
            return true;
        }
        let (gate, priority) = (self.gate, self.priority);
        let pending = self.pending.entry(index).or_insert_with(|| {
            let persona = uuid::Uuid::from_u128(index as u128 + 1);
            Box::pin(gate.serving_lane_request(Some(persona), priority, None).1)
        });
        let mut cx = std::task::Context::from_waker(futures::task::noop_waker_ref());
        match pending.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(permit) => {
                self.pending.remove(&index);
                self.held.insert(index, permit);
                true
            }
            std::task::Poll::Pending => false,
        }
    }

    fn release(&mut self, index: usize) {
        self.held.remove(&index);
    }
}

#[cfg(test)]
mod tests;
