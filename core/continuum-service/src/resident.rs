//! One resident's service: her settled turns, a window of them, and the alarm a stall raises.
//!
//! The node-wide sample asks whether the ENGINE slowed. A mind can stall on a healthy engine:
//! Kimi on the 5090, 2026-10-26, ran her loop for 17 hours and every turn was refused before
//! inference (required 26,101 tokens > budget 17,829 at a 49,152 window), and Sahar stalled
//! from 20:05Z on 63,840 > 49,152. Nothing noticed until Joel asked. Here each resident's turns
//! become her own window, judged on her own stream by [`detect::observe`], and a stall or a
//! regression becomes a [`ResidentAlarm`] the node publishes on its beacon.
//!
//! Pure like the rest of the crate: the caller owns one [`ResidentLedger`] and one
//! [`ServiceState`] per resident, feeds the ledger at the turn's settle, and closes a window on
//! its own cadence.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::detect::{self, Reference, ServicePolicy, ServiceState, Verdict};
use crate::sample::{Builds, Demand, EngineIncarnation, ServiceSample, ServingConfig, Warmup, Window, Workload, SCHEMA};

/// Why a resident's turn ended without completing, or ended parked after progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "class", rename_all = "snake_case")]
#[ts(export, rename = "ResidentRefusal", export_to = "../../../protocol/typescript/grid/ResidentRefusal.ts")]
pub enum Refusal {
    /// Her required prompt input did not fit the budget: refused before inference. She is
    /// parked until the binding, the placement or the window changes. `context_window` is the
    /// window the refusal's readiness observed; `None` when no readiness owner reported one.
    PromptCapacity { required_tokens: u32, budget_tokens: u32, context_window: Option<u32> },
    /// The model call faulted and its retries were spent.
    InferenceFailed,
    /// Inference was served, but the observed task end could not be checkpointed.
    ReceiptPersistenceFailed,
    /// She waited at the serving gate and never reached the model.
    LaneStarved,
}

impl Refusal {
    fn same_class(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }

    /// The one refusal that parks her until something changes: no new turn asks again.
    fn parks(&self) -> bool {
        matches!(self, Self::PromptCapacity { .. })
    }
}

/// What one completed turn cost, as the turn's own metrics and receipts measured it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub acts: u32,
    /// The turn's wall time, settle to settle.
    pub wall_ms: u64,
    pub prompt_tokens: u32,
    pub output_tokens: u32,
    /// Prompt tokens the engine served from its cache, and the ones it prefilled; both 0 when
    /// the lane reported no timings.
    pub cached_tokens: u32,
    pub prefilled_tokens: u32,
    /// The model that answered (the served receipt's, never the configured one).
    pub model: String,
}

/// One settled turn: completed, refused, or completed work and then parked on a refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    pub completion: Option<Completion>,
    pub refusal: Option<Refusal>,
}

/// Her turns since the last window closed, and the refusal she is parked on, if any.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResidentLedger {
    turns: Vec<Turn>,
    parked_on: Option<Refusal>,
    /// Generations served since the last window closed, counted when each was served, whether
    /// or not its turn has settled. A resident's turn may run unlimited acts (her own agency), so
    /// a window that counted only settled turns read a mind serving dozens of generations inside
    /// one long turn as stalled (Aris on the M5, 2026-10-31: one turn open from 22:00:35Z, ~77
    /// generations served, "demand 1, completions 0").
    served: u32,
    /// The last model that answered her; empty until one has.
    model: String,
}

impl ResidentLedger {
    /// Fold one settled turn. A turn with neither a completion nor a refusal asked nothing of
    /// the engine (no generation was dispatched), so it is not demand.
    pub fn note(&mut self, turn: Turn) {
        if turn.completion.is_none() && turn.refusal.is_none() {
            return;
        }
        if let Some(completion) = &turn.completion {
            self.model.clone_from(&completion.model);
        }
        // The turn's ending decides whether she is parked: a capacity refusal waits on a
        // change, anything else either finished or comes back next beat as a new turn.
        self.parked_on = turn.refusal.filter(Refusal::parks);
        self.turns.push(turn);
    }

    /// Her loop stopped with the refusal still parked: a stopped mind asks nothing, so it is
    /// never read as a stalled one.
    pub fn unpark(&mut self) {
        self.parked_on = None;
    }

    /// One generation was served to her, now: counted in the window it happened in.
    pub fn note_generation(&mut self) {
        self.served = self.served.saturating_add(1);
    }

    /// Close the window `span`: its turns move into the summary, the parked refusal stays
    /// standing. `in_flight_since_ms` is when her unsettled turn started, if one is in flight:
    /// a turn that has run since before the window opened is demand the window never saw settle
    /// (Eitan, enrico 2026-10-26: one turn, 4,500 s in flight on slot 1). A turn that began INSIDE
    /// the window is not counted yet, so a long turn that will complete never alarms its own
    /// window; if it never settles, it spans the next window whole and is counted there.
    pub fn close(&mut self, span: Window, in_flight_since_ms: Option<u64>) -> ResidentWindow {
        let spanning = in_flight_since_ms.filter(|start| *start <= span.start_ms);
        let served = std::mem::take(&mut self.served);
        ResidentWindow::of(std::mem::take(&mut self.turns), self.parked_on, spanning, &self.model, served)
    }
}

/// One closed window of one resident's turns.
#[derive(Debug, Clone, PartialEq)]
pub struct ResidentWindow {
    /// Turns that asked the engine for something; with none settled, one for a refusal parked
    /// across the window (a parked mind asks again only when capacity changes, and it may
    /// never) or for a turn in flight across the whole window.
    pub offered: u32,
    pub completions: u32,
    pub generations: u32,
    pub acts: u32,
    pub refused: u32,
    pub latency_p50_ms: u64,
    pub latency_p90_ms: u64,
    pub prompt_tokens_p50: u32,
    pub output_tokens_p50: u32,
    pub cached_ratio_p50: f32,
    pub warmup: Warmup,
    pub model: String,
    /// Still parked on a refusal when the window closed.
    pub parked: bool,
    /// When her unsettled turn started, if it has been in flight since before the window.
    pub in_flight_since_ms: Option<u64>,
    /// The refusal class she met most this window, at its latest numbers, and how often; the
    /// parked refusal with a count of 0 when she met none this window but is still parked.
    pub dominant: Option<(Refusal, u32)>,
}

impl ResidentWindow {
    fn of(turns: Vec<Turn>, parked_on: Option<Refusal>, in_flight_since_ms: Option<u64>, model: &str, served: u32) -> Self {
        let completed: Vec<&Completion> = turns.iter().filter_map(|t| t.completion.as_ref()).collect();
        let refusals: Vec<Refusal> = turns.iter().filter_map(|t| t.refusal).collect();
        let mut wall: Vec<u64> = completed.iter().map(|c| c.wall_ms).collect();
        let mut prompt: Vec<u32> = completed.iter().map(|c| c.prompt_tokens).collect();
        let mut output: Vec<u32> = completed.iter().map(|c| c.output_tokens).collect();
        let timed: Vec<&&Completion> = completed.iter().filter(|c| c.cached_tokens + c.prefilled_tokens > 0).collect();
        let mut reuse_milli: Vec<u32> = timed
            .iter()
            .map(|c| (u64::from(c.cached_tokens) * 1000 / u64::from(c.cached_tokens + c.prefilled_tokens)) as u32)
            .collect();
        let warmup = if timed.is_empty() {
            Warmup::Unknown
        } else if timed.iter().any(|c| c.cached_tokens > 0) {
            Warmup::Warm
        } else {
            Warmup::WarmingUp
        };
        let dominant = dominant(&refusals).or(parked_on.map(|r| (r, 0)));
        Self {
            offered: (turns.len() as u32)
                + u32::from(turns.is_empty() && (parked_on.is_some() || in_flight_since_ms.is_some() || served > 0)),
            completions: completed.len() as u32,
            generations: served,
            acts: completed.iter().map(|c| c.acts).sum(),
            refused: refusals.len() as u32,
            latency_p50_ms: quantile(&mut wall, 1, 2),
            latency_p90_ms: quantile(&mut wall, 9, 10),
            prompt_tokens_p50: quantile(&mut prompt, 1, 2),
            output_tokens_p50: quantile(&mut output, 1, 2),
            cached_ratio_p50: quantile(&mut reuse_milli, 1, 2) as f32 / 1000.0,
            warmup,
            model: model.to_string(),
            parked: parked_on.is_some(),
            in_flight_since_ms,
            dominant,
        }
    }

    /// The window as a [`ServiceSample`] on her own stream: the node's facts, her numbers.
    pub fn sample(&self, stream: &StreamFacts, span: Window) -> ServiceSample {
        ServiceSample {
            schema: SCHEMA,
            node: stream.node.clone(),
            model: self.model.clone(),
            builds: stream.builds.clone(),
            config: stream.config.clone(),
            incarnation: stream.incarnation,
            mixed: false,
            window: span,
            workload: Workload {
                residents_active: stream.residents_active,
                prompt_tokens_p50: self.prompt_tokens_p50,
                output_tokens_p50: self.output_tokens_p50,
                cached_ratio_p50: self.cached_ratio_p50,
            },
            // A resident's queue wait is inside her turn's wall time; the node's own sample
            // carries the queue.
            demand: Demand { offered: self.offered, queued_ms_p50: 0 },
            generations: self.generations,
            acts: self.acts,
            failures: self.refused,
            latency_p50_ms: self.latency_p50_ms,
            latency_p90_ms: self.latency_p90_ms,
            prediction: None,
            warmup: self.warmup,
            memory_ceiling_bytes: stream.memory_ceiling_bytes,
        }
    }
}

/// The node's facts a resident's stream runs under, read by the caller once per window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamFacts {
    pub node: String,
    pub builds: Builds,
    pub config: ServingConfig,
    pub incarnation: EngineIncarnation,
    pub residents_active: u32,
    pub memory_ceiling_bytes: u64,
}

/// Judge one resident's closed window on her own stream.
pub fn judge(state: &mut ServiceState, policy: &ServicePolicy, window: &ResidentWindow, span: Window, stream: &StreamFacts) -> Verdict {
    detect::observe(state, policy, &window.sample(stream, span))
}

/// What a stalled or regressed resident looks like on the beacon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, rename = "ResidentAlarmState", export_to = "../../../protocol/typescript/grid/ResidentAlarmState.ts")]
pub enum AlarmState {
    /// Work was asked and nothing completed.
    Stalled,
    /// Her turns ran slower than her own healthy windows, for `streak` windows in a row.
    Regressed {
        #[ts(type = "number")]
        observed_p50_ms: u64,
        #[ts(type = "number")]
        slowest_healthy_p50_ms: u64,
        streak: u32,
    },
}

/// One resident's alarm: who, over which window, what was asked and done, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/grid/ResidentAlarm.ts")]
pub struct ResidentAlarm {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    pub name: String,
    pub state: AlarmState,
    #[ts(type = "number")]
    pub window_start_ms: u64,
    #[ts(type = "number")]
    pub window_end_ms: u64,
    pub demand: u32,
    pub completions: u32,
    /// Turns refused this window (every class).
    pub refusals: u32,
    pub parked: bool,
    /// When her unsettled turn started, if one has been in flight since before the window.
    #[ts(type = "number | null")]
    pub in_flight_since_ms: Option<u64>,
    /// The refusal class she met most, at its latest numbers.
    pub refusal: Option<Refusal>,
}

impl ResidentAlarm {
    /// The alarm a verdict raises: `Some` for a stall or a regression, `None` otherwise.
    pub fn of(persona_id: Uuid, name: &str, window: &ResidentWindow, span: Window, verdict: &Verdict) -> Option<Self> {
        let state = match verdict {
            Verdict::Stalled { .. } => AlarmState::Stalled,
            Verdict::Regressed(r) => AlarmState::Regressed {
                observed_p50_ms: r.observed_p50_ms,
                slowest_healthy_p50_ms: match r.against {
                    Reference::OwnHistory { slowest_ms, .. } => slowest_ms,
                    // A resident's sample carries no prediction, so her own history is the
                    // only reference; a ratio judgement has no healthy milliseconds to name.
                    Reference::PredictionRatio { .. } => 0,
                },
                streak: r.streak,
            },
            Verdict::Healthy | Verdict::Suspect { .. } | Verdict::Insufficient(_) => return None,
        };
        Some(Self {
            persona_id,
            name: name.to_string(),
            state,
            window_start_ms: span.start_ms,
            window_end_ms: span.end_ms,
            demand: window.offered,
            completions: window.completions,
            refusals: window.refused,
            parked: window.parked,
            in_flight_since_ms: window.in_flight_since_ms,
            refusal: window.dominant.map(|(r, _)| r),
        })
    }

    /// One line a human or a peer acts on.
    pub fn line(&self) -> String {
        let what = match self.state {
            AlarmState::Stalled => format!("stalled: {} asked, {} completed", self.demand, self.completions),
            AlarmState::Regressed { observed_p50_ms, slowest_healthy_p50_ms, streak } => format!(
                "regressed: median turn {} s vs {} s healthy, {streak} windows",
                observed_p50_ms / 1000,
                slowest_healthy_p50_ms / 1000
            ),
        };
        let why = match self.refusal {
            Some(Refusal::PromptCapacity { required_tokens, budget_tokens, context_window }) => {
                let at = match context_window {
                    Some(window) => format!(" at context {window}"),
                    None => String::new(),
                };
                format!(", prompt capacity {required_tokens} > {budget_tokens}{at}")
            }
            Some(Refusal::InferenceFailed) => ", inference failed".to_string(),
            Some(Refusal::ReceiptPersistenceFailed) => ", receipt persistence failed".to_string(),
            Some(Refusal::LaneStarved) => ", lane starved".to_string(),
            None => String::new(),
        };
        let parked = if self.parked { ", parked" } else { "" };
        let in_flight = match self.in_flight_since_ms {
            Some(start) => format!(", one turn in flight {} s", self.window_end_ms.saturating_sub(start) / 1000),
            None => String::new(),
        };
        format!("{} {what}{why}{parked}{in_flight}", self.name)
    }
}

/// The class met most often, at its latest numbers; ties go to the class met last.
fn dominant(refusals: &[Refusal]) -> Option<(Refusal, u32)> {
    let mut classes: Vec<(Refusal, u32)> = Vec::new();
    for refusal in refusals {
        match classes.iter_mut().find(|(r, _)| r.same_class(refusal)) {
            Some(entry) => *entry = (*refusal, entry.1 + 1),
            None => classes.push((*refusal, 1)),
        }
    }
    let last = refusals.last()?;
    classes.into_iter().max_by_key(|(r, count)| (*count, r.same_class(last)))
}

/// The `num/den` quantile of `values` (nearest rank); 0 for an empty window.
fn quantile<T: Copy + Ord + Default>(values: &mut [T], num: usize, den: usize) -> T {
    values.sort_unstable();
    match values.len() {
        0 => T::default(),
        n => values[((n - 1) * num) / den],
    }
}

#[cfg(test)]
mod tests;
