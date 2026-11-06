//! The replay: one DEVICE queue (frames executed back-to-back, each assembled from ready work while
//! the previous one runs, so the host adds no gap between frames) and per-mind HOST encoders
//! (render and tool execution during her gap, page I/O on its own completion domain), on the
//! recorded arrivals. The device frame is the scheduler's decision; the costs are `costs.rs`'s fits.
//!
//! [`Pipeline`] states what the host may hand the device ahead of an act's go:
//! - [`Scope::AtGo`]: nothing (today): an act's fresh tokens reach the device at its submission,
//!   after its recorded restore.
//! - [`Scope::AppendOnly`] (e): her previous reply and the tool results appended after it stream
//!   into her live sequence during her gap (uniform over the gap: the captures record only submit
//!   and completion, no item entry times), when the act changes nothing inside its previous
//!   prompt; page-in ahead overlaps the gap (`max(0, restore - gap)` remains).
//! - [`Scope::StablePrompts`] (e*): as (e), with the act's in-prompt changes, view turnover and
//!   checkpoint-granularity tokens eliminated (the prompt-stability target), so every act streams.
//! - [`Scope::ReadyBeforeGo`] (e-ready, Joel: "One act is ready before it's given the go";
//!   "Continuous"): her slot is kept current continuously, so there is no stale restore and no
//!   slot taken by another; only her appended items remain, prefilled as they arrive, and the act
//!   is the moment decode starts. Host encoders run on independent cores (the M5 reports
//!   `perf_cores` 18, `serving.plan.branch`), so the device queue is the only shared stage.
#![cfg(test)]

use super::{BatchScheduler, Costs, Hour, Plan, SlotView, TaskClass, View};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    AtGo,
    AppendOnly,
    StablePrompts,
    ReadyBeforeGo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Pipeline {
    pub scope: Scope,
}

impl Pipeline {
    pub(crate) const TODAY: Self = Self { scope: Scope::AtGo };

    /// The act's fresh tokens under this scope.
    fn fresh(&self, hour: &Hour, i: usize, first: bool) -> u32 {
        let t = &hour.tasks[i];
        match (self.scope, t.class, first) {
            (Scope::StablePrompts, TaskClass::Resident, _) => t.fresh - t.unstable.min(t.fresh),
            (Scope::ReadyBeforeGo, TaskClass::Resident, false) => t.stream.min(t.fresh),
            _ => t.fresh,
        }
    }

    /// The tokens that may stream into her sequence during the gap before act `i`.
    fn streamable(&self, hour: &Hour, i: usize) -> u32 {
        let t = &hour.tasks[i];
        match (self.scope, t.class) {
            (_, TaskClass::Dream) | (Scope::AtGo, _) => 0,
            (Scope::AppendOnly, _) if t.unstable > 0 => 0,
            _ => t.stream.min(t.fresh),
        }
    }

    /// Restore time still on the act's path after its gap.
    fn restore_s(&self, restore_s: f64, gap_s: f64) -> f64 {
        match self.scope {
            Scope::AtGo => restore_s,
            Scope::ReadyBeforeGo => 0.0,
            _ => (restore_s - gap_s).max(0.0),
        }
    }
}

/// One task's outcome.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TaskOutcome {
    pub arrival_s: f64,
    pub prefill_s: f64,
    pub first_service_wait_s: f64,
    pub decode_s: f64,
    pub longest_gap_s: f64,
    pub release_s: f64,
    /// Fresh tokens prefilled before the go (streamed during the gap).
    pub before_go: u32,
    pub fresh: u32,
}

impl TaskOutcome {
    pub fn act_s(&self) -> f64 {
        self.release_s - self.arrival_s
    }

    fn unfinished() -> Self {
        Self { arrival_s: f64::NAN, prefill_s: f64::NAN, first_service_wait_s: f64::NAN, decode_s: f64::NAN, longest_gap_s: f64::NAN, release_s: f64::NAN, before_go: 0, fresh: 0 }
    }
}

/// The replay's device and host accounting.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Run {
    pub outcomes: Vec<TaskOutcome>,
    pub busy_s: f64,
    pub span_s: f64,
    /// Device idle while some mind had device-ready work (tokens emitted into her context and
    /// not yet prefilled; under today's scope those cannot be handed over until the go).
    pub idle_while_ready_s: f64,
    /// Device idle while a mind was on the host (render, tool, page I/O).
    pub device_blocked_on_host_s: f64,
    /// Sum over minds of time with device-handed work waiting outside the executing frame.
    pub host_blocked_on_device_s: f64,
    pub gap_prefill_s: f64,
    pub act_prefill_s: f64,
}

#[derive(Debug, Clone, Copy)]
enum Phase {
    Waiting,
    /// Her gap before act `next`: `total` tokens emitted uniformly over [start, end).
    Gap { start_s: f64, end_s: f64, total: u32, done: u32, depth: u64 },
    Restoring { until_s: f64 },
    Prefill { left: u32, depth: u64, ready_s: f64, served: bool },
    Generating { left: f64, depth: u64, since_s: f64, last_s: f64 },
}

fn gap_available(start_s: f64, end_s: f64, total: u32, now: f64) -> u32 {
    let frac = if end_s > start_s { ((now - start_s) / (end_s - start_s)).clamp(0.0, 1.0) } else { 1.0 };
    (f64::from(total) * frac).floor() as u32
}

/// When a slot's next task arrives: a resident's after her recorded think gap from the release of
/// her previous one (closed loop); dream traffic at its recorded launch, never before the slot frees.
fn next_arrival(hour: &Hour, queue: &[usize], k: usize, released_s: f64) -> f64 {
    let t = &hour.tasks[queue[k]];
    match (t.class, k) {
        (TaskClass::Dream, _) | (_, 0) => t.launch_s.max(released_s),
        (TaskClass::Resident, _) => {
            let p = &hour.tasks[queue[k - 1]];
            released_s + (t.launch_s - (p.launch_s + p.recorded_act_s)).max(0.0)
        }
    }
}

/// Today's replay: outcomes only.
pub(crate) fn replay(hour: &Hour, costs: &Costs, scheduler: &mut dyn BatchScheduler) -> Vec<TaskOutcome> {
    run(hour, costs, scheduler, Pipeline::TODAY).outcomes
}

/// Replay the hour's tasks, slot by slot, under `scheduler` and `pipeline`.
pub(crate) fn run(hour: &Hour, costs: &Costs, scheduler: &mut dyn BatchScheduler, pipeline: Pipeline) -> Run {
    let slots = hour.tasks.iter().map(|t| t.slot).max().map_or(0, |m| m + 1);
    let queues: Vec<Vec<usize>> = (0..slots).map(|s| (0..hour.tasks.len()).filter(|i| hour.tasks[*i].slot == s).collect()).collect();
    let mut next = vec![0usize; slots];
    let mut phase: Vec<Phase> = vec![Phase::Waiting; slots];
    let mut arrival: Vec<f64> = (0..slots).map(|s| if queues[s].is_empty() { f64::INFINITY } else { next_arrival(hour, &queues[s], 0, 0.0) }).collect();
    let mut gap_s = vec![0.0f64; slots];
    let mut streamed = vec![0u32; slots];
    let mut outcomes: Vec<Option<TaskOutcome>> = vec![None; hour.tasks.len()];
    let mut current: Vec<Option<(usize, TaskOutcome)>> = vec![None; slots];
    let mut run = Run { outcomes: Vec::new(), busy_s: 0.0, span_s: 0.0, idle_while_ready_s: 0.0, device_blocked_on_host_s: 0.0, host_blocked_on_device_s: 0.0, gap_prefill_s: 0.0, act_prefill_s: 0.0 };
    let mut now = 0.0f64;
    let mut done = 0usize;
    while done < hour.tasks.len() {
        for s in 0..slots {
            if let Phase::Gap { end_s, done: d, .. } = phase[s] {
                if end_s <= now {
                    streamed[s] = d;
                    phase[s] = Phase::Waiting;
                }
            }
            if let Phase::Waiting = phase[s] {
                if next[s] < queues[s].len() && arrival[s] <= now {
                    let i = queues[s][next[s]];
                    let fresh = pipeline.fresh(hour, i, next[s] == 0);
                    let o = TaskOutcome { arrival_s: arrival[s], prefill_s: 0.0, first_service_wait_s: 0.0, decode_s: 0.0, longest_gap_s: 0.0, release_s: f64::NAN, before_go: streamed[s].min(fresh), fresh };
                    current[s] = Some((i, o));
                    phase[s] = Phase::Restoring { until_s: arrival[s] + pipeline.restore_s(hour.tasks[i].restore_s, gap_s[s]) };
                }
            }
            if let Phase::Restoring { until_s } = phase[s] {
                if until_s <= now {
                    let (i, o) = current[s].expect("model: a restoring slot has a task");
                    let t = &hour.tasks[i];
                    let left = o.fresh.saturating_sub(o.before_go).max(1);
                    phase[s] = Phase::Prefill { left, depth: u64::from(t.cached) + u64::from(o.before_go), ready_s: until_s, served: false };
                    streamed[s] = 0;
                }
            }
        }
        let views: Vec<SlotView> = (0..slots).map(|s| {
            let cold = current[s].is_some_and(|(i, _)| hour.tasks[i].cached == 0);
            match phase[s] {
                Phase::Prefill { left, depth, ready_s, .. } => SlotView { slot: s, prompt_left: left, depth, cold, generating: false, streamed: false, ready_s },
                Phase::Generating { depth, .. } => SlotView { slot: s, prompt_left: 0, depth, cold, generating: true, streamed: false, ready_s: 0.0 },
                Phase::Gap { start_s, end_s, total, done: d, depth } => SlotView {
                    slot: s, prompt_left: gap_available(start_s, end_s, total, now).saturating_sub(d), depth: depth + u64::from(d),
                    cold: false, generating: false, streamed: true, ready_s: start_s,
                },
                _ => SlotView { slot: s, prompt_left: 0, depth: 0, cold, generating: false, streamed: false, ready_s: 0.0 },
            }
        }).collect();
        // Work the device may take now: anything handed to it, plus streamed output. A streamed
        // chunk below a full ubatch is taken only when the device would otherwise idle (it pays the
        // fitted per-chunk cost), or when its gap's output is complete.
        let other = views.iter().any(|v| (v.prompt_left > 0 && !v.streamed) || v.generating);
        let (decoders, decode_depth) = views.iter().filter(|v| v.generating).fold((0usize, 0u64), |(n, d), v| (n + 1, d + v.depth));
        let mut plan = Plan::default();
        if other || views.iter().any(|v| v.streamed && v.prompt_left > 0) {
            let view = View { slots: &views, n_batch: hour.n_batch, offsets: &hour.checkpoint_offsets };
            plan = scheduler.plan(&view);
            plan.prompt.retain(|(s, _)| !views[*s].streamed || !other || full_or_last(&phase[*s], hour.n_ubatch, now));
        }
        let mut gap_part = 0.0;
        let mut act_part = 0.0;
        for (s, n) in &plan.prompt {
            let c = costs.prompt_s(*n, views[*s].depth);
            match views[*s].streamed {
                true => gap_part += c,
                false => act_part += c,
            }
        }
        let step = gap_part + act_part + costs.decode_s(decoders, decode_depth);
        if step <= 0.0 {
            // Idle: wake at the next arrival, restore end, gap end, or the next streamed token.
            let wake = (0..slots).map(|s| match phase[s] {
                Phase::Waiting if next[s] < queues[s].len() => arrival[s],
                Phase::Restoring { until_s } => until_s,
                Phase::Gap { start_s, end_s, total, .. } => {
                    let emitted = gap_available(start_s, end_s, total, now);
                    match total {
                        0 => end_s,
                        _ => (start_s + (end_s - start_s) * f64::from(emitted + 1) / f64::from(total)).min(end_s).max(now),
                    }
                }
                _ => f64::INFINITY,
            }).fold(f64::INFINITY, f64::min);
            if !wake.is_finite() {
                break;
            }
            let span = (wake - now).max(0.0);
            let host = phase.iter().any(|p| matches!(p, Phase::Gap { .. } | Phase::Restoring { .. }))
                || (0..slots).any(|s| matches!(phase[s], Phase::Waiting) && next[s] > 0 && next[s] < queues[s].len() && hour.tasks[queues[s][next[s]]].class == TaskClass::Resident);
            // Device-ready work under the pipeline's dependency rules: tokens already emitted into
            // a mind's context (a gap with output not yet prefilled; under today's scope the gap
            // output exists but is only handed over at the go), or a rendered act on page I/O.
            let ready = (0..slots).any(|s| match phase[s] {
                Phase::Gap { start_s, end_s, total, done: d, .. } => gap_available(start_s, end_s, total, now) > d,
                Phase::Restoring { .. } => true,
                Phase::Waiting => next[s] > 0 && next[s] < queues[s].len()
                    && hour.tasks[queues[s][next[s]]].class == TaskClass::Resident && hour.tasks[queues[s][next[s]]].stream > 0,
                _ => false,
            });
            if host {
                run.device_blocked_on_host_s += span;
            }
            if ready {
                run.idle_while_ready_s += span;
            }
            now = wake.max(now + 1e-9);
            continue;
        }
        let waiting = views.iter().filter(|v| v.prompt_left > 0 && !v.streamed && !plan.prompt.iter().any(|(s, _)| *s == v.slot)).count();
        run.host_blocked_on_device_s += step * waiting as f64;
        run.busy_s += step;
        run.gap_prefill_s += gap_part;
        run.act_prefill_s += act_part;
        now += step;
        scheduler.served(&plan, step);
        for &(s, n) in &plan.prompt {
            match phase[s] {
                Phase::Gap { start_s, end_s, total, done: d, depth } => phase[s] = Phase::Gap { start_s, end_s, total, done: d + n, depth },
                Phase::Prefill { left, depth, ready_s, served } => {
                    let (i, o) = current[s].as_mut().expect("model: a prefilling slot has a task");
                    if !served {
                        o.first_service_wait_s = now - step - ready_s;
                    }
                    let (left, depth) = (left.saturating_sub(n), depth + u64::from(n));
                    phase[s] = match left {
                        0 => {
                            o.prefill_s = now - ready_s;
                            Phase::Generating { left: f64::from(hour.tasks[*i].decode.max(1)), depth, since_s: now, last_s: now }
                        }
                        _ => Phase::Prefill { left, depth, ready_s, served: true },
                    };
                }
                _ => {}
            }
        }
        for s in 0..slots {
            if !views[s].generating {
                continue;
            }
            if let Phase::Generating { left, depth, since_s, last_s } = phase[s] {
                let (i, mut o) = current[s].expect("model: a generating slot has a task");
                let accepted = hour.tasks[i].mean_len;
                o.longest_gap_s = o.longest_gap_s.max(now - last_s);
                if left <= accepted {
                    o.release_s = now;
                    o.decode_s = now - since_s;
                    outcomes[i] = Some(o);
                    done += 1;
                    current[s] = None;
                    next[s] += 1;
                    phase[s] = Phase::Waiting;
                    if next[s] < queues[s].len() {
                        arrival[s] = next_arrival(hour, &queues[s], next[s], now);
                        gap_s[s] = arrival[s] - now;
                        let n = queues[s][next[s]];
                        let total = pipeline.streamable(hour, n);
                        if total > 0 {
                            phase[s] = Phase::Gap { start_s: now, end_s: arrival[s], total, done: 0, depth: u64::from(hour.tasks[n].cached) };
                        }
                    }
                } else {
                    current[s] = Some((i, o));
                    phase[s] = Phase::Generating { left: left - accepted, depth: depth + accepted as u64, since_s, last_s: now };
                }
            }
        }
    }
    run.span_s = now;
    run.outcomes = outcomes.into_iter().map(|o| o.unwrap_or_else(TaskOutcome::unfinished)).collect(); // unwrap_or_else: a stalled policy leaves tasks unfinished, reported as NaN
    run
}

/// A streamed slot's available tokens form a full ubatch, or its gap is over (the rest of it).
fn full_or_last(phase: &Phase, n_ubatch: u32, now: f64) -> bool {
    match *phase {
        Phase::Gap { start_s, end_s, total, done, .. } => {
            let available = gap_available(start_s, end_s, total, now).saturating_sub(done);
            available >= n_ubatch || (now >= end_s && available > 0) || done + available == total
        }
        _ => true,
    }
}
