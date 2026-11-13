//! The per-tool signals every policy is built from, each read from her past alone.

use crate::history::{ActivityId, History};
use crate::relevance::Relevance;

pub const DAY_MS: u64 = 86_400_000;

/// The decayed fraction of her past epochs that called each tool: an epoch `age` ms old
/// weighs `0.5^(age / half_life)` (`None`: every epoch weighs the same). With `activity`, only
/// the epochs that turned over in that activity count.
pub fn occurrence(history: &History<'_>, half_life_ms: Option<u64>, activity: Option<ActivityId>) -> Vec<f32> {
    let mut weight_of = vec![0.0f64; history.vocab.len()];
    let mut total = 0.0f64;
    for epoch in history.epochs {
        if activity.is_some() && history.calls[epoch.calls.start].activity != activity {
            continue;
        }
        let age = history.at.since(epoch.start).millis() as f64;
        let weight = half_life_ms.map_or(1.0, |half_life| 0.5f64.powf(age / half_life as f64)); // map_or: no half-life is the undecayed count
        let mut tools: Vec<usize> = history.calls[epoch.calls.clone()].iter().map(|call| call.tool.index()).collect();
        tools.sort_unstable();
        tools.dedup();
        for tool in tools {
            weight_of[tool] += weight;
        }
        total += weight;
    }
    weight_of.into_iter().map(|weight| if total > 0.0 { (weight / total) as f32 } else { 0.0 }).collect()
}

/// The chance she calls each tool in the coming epoch, running a Markov chain of her own
/// call-to-call transitions forward from her last call, for as many steps as her epochs
/// average calls.
///
/// Each row is her observed next calls plus one pseudo-call spread as her overall call mix, so
/// a call she has made only once does not fix her next one.
pub fn transition(history: &History<'_>) -> Vec<f32> {
    let width = history.vocab.len();
    let Some(last) = history.last_call() else {
        return vec![0.0; width];
    };
    let mut follows: Vec<Vec<(usize, f64)>> = vec![Vec::new(); width];
    let mut leaves = vec![0.0f64; width];
    let mut mix = vec![0.0f64; width];
    for pair in history.calls.windows(2) {
        let (from, to) = (pair[0].tool.index(), pair[1].tool.index());
        match follows[from].iter_mut().find(|(next, _)| *next == to) {
            Some((_, count)) => *count += 1.0,
            None => follows[from].push((to, 1.0)),
        }
        leaves[from] += 1.0;
    }
    for call in history.calls {
        mix[call.tool.index()] += 1.0 / history.calls.len() as f64;
    }
    // One pseudo-call of her overall mix per row: the smallest prior that still lets every
    // tool she has ever called follow any other.
    const PRIOR_CALLS: f64 = 1.0;
    let steps = if history.epochs.is_empty() {
        1
    } else {
        let calls: usize = history.epochs.iter().map(|epoch| epoch.calls.len()).sum();
        (calls as f64 / history.epochs.len() as f64).round().max(1.0) as usize
    };
    let mut state = vec![0.0f64; width];
    state[last.tool.index()] = 1.0;
    let mut never = vec![1.0f64; width];
    for _ in 0..steps {
        let mut next = vec![0.0f64; width];
        let mut spread = 0.0f64;
        for (from, mass) in state.iter().enumerate().filter(|(_, mass)| **mass > 0.0) {
            let denominator = leaves[from] + PRIOR_CALLS;
            for (to, count) in &follows[from] {
                next[*to] += mass * count / denominator;
            }
            spread += mass * PRIOR_CALLS / denominator;
        }
        for (tool, share) in mix.iter().enumerate() {
            next[tool] += spread * share;
        }
        for (tool, chance) in next.iter().enumerate() {
            never[tool] *= 1.0 - chance.min(1.0);
        }
        state = next;
    }
    never.into_iter().map(|never| (1.0 - never) as f32).collect()
}

/// The time scales the learned policies read her usage at: recent work (3 days), her
/// standing habits (30 days), and one activity's habits (7 days). The frequency policies sweep
/// half-lives around these; the blend learns how much each matters.
pub const SHORT_HALF_LIFE_MS: u64 = 3 * DAY_MS;
pub const LONG_HALF_LIFE_MS: u64 = 30 * DAY_MS;
pub const ACTIVITY_HALF_LIFE_MS: u64 = 7 * DAY_MS;

/// How many of her latest acts the relevance model reads: enough to name what she is on,
/// few enough that the oldest is still this task.
pub const RECENT_ACTS: usize = 5;

/// Every signal for every tool at one turnover.
#[derive(Debug, Clone)]
pub struct Signals {
    pub short: Vec<f32>,
    pub long: Vec<f32>,
    pub activity: Vec<f32>,
    pub transition: Vec<f32>,
    pub population: Vec<f32>,
    pub relevance: Vec<f32>,
}

/// How many inputs a blend row carries, its bias included.
pub const FEATURES: usize = 7;

impl Signals {
    pub fn of(history: &History<'_>, relevance: &dyn Relevance) -> Self {
        let width = history.vocab.len();
        let context = history.recent_text(RECENT_ACTS);
        Self {
            short: occurrence(history, Some(SHORT_HALF_LIFE_MS), None),
            long: occurrence(history, Some(LONG_HALF_LIFE_MS), None),
            activity: match history.activity {
                Some(activity) => occurrence(history, Some(ACTIVITY_HALF_LIFE_MS), Some(activity)),
                None => vec![0.0; width],
            },
            transition: transition(history),
            population: history.population.map_or_else(|| vec![0.0; width], <[f32]>::to_vec), // map_or_else: no society known reads as no evidence from it
            relevance: if context.is_empty() { vec![0.0; width] } else { relevance.relevance(&context) },
        }
    }

    /// Whether any evidence names `tool` at all; a tool no one has ever called and no text
    /// mentions is left to be found.
    pub fn any(&self, tool: usize) -> bool {
        self.long[tool] > 0.0 || self.population[tool] > 0.0 || self.transition[tool] > 0.0
    }

    /// The blend's input row for `tool`: each chance as a log-odds, the relevance as is, and a
    /// constant 1 for the bias.
    pub fn row(&self, tool: usize) -> [f32; FEATURES] {
        [
            log_odds(self.short[tool]),
            log_odds(self.long[tool]),
            log_odds(self.activity[tool]),
            log_odds(self.transition[tool]),
            log_odds(self.population[tool]),
            self.relevance[tool],
            1.0,
        ]
    }
}

/// Log-odds of a chance, held inside (1e-4, 1 - 1e-4): an estimate of exactly 0 or 1 from a
/// finite past is a count, not a certainty, and must stay a finite input.
pub fn log_odds(chance: f32) -> f32 {
    let chance = chance.clamp(1e-4, 1.0 - 1e-4);
    (chance / (1.0 - chance)).ln()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::epoch::{segment, Epoch};
    use crate::history::{EpochMs, Call};
    use crate::vocab::{ToolId, ToolSchema, Vocabulary};

    fn vocab() -> Vocabulary {
        let schema = |name: &str| ToolSchema { name: name.into(), description: String::new(), tokens: 10 };
        Vocabulary::new(vec![schema("code/edit"), schema("code/read"), schema("work/get")]).expect("small vocabulary")
    }

    fn history<'a>(vocab: &'a Vocabulary, calls: &'a [Call], epochs: &'a [Epoch], at: u64) -> History<'a> {
        History { vocab, calls, epochs, at: EpochMs::from_millis(at), activity: None, population: None }
    }

    // what this catches: decay running the wrong way (old habits outweighing this week's), and
    // an epoch that called a tool twice counting it twice (the head is per epoch, not per call).
    #[test]
    fn occurrence_counts_epochs_not_calls_and_weighs_the_recent_ones_more() {
        let vocab = vocab();
        let (read, get) = (vocab.id("code/read").expect("listed"), vocab.id("work/get").expect("listed"));
        let call = |at: u64, tool: ToolId| Call { at: EpochMs::from_millis(at), tool, activity: None, text: String::new() };
        let calls = vec![call(0, read), call(1, read), call(10 * DAY_MS, get)];
        let epochs = segment(&calls, DAY_MS);
        let at = 10 * DAY_MS + 1;
        let flat = occurrence(&history(&vocab, &calls, &epochs, at), None, None);
        assert_eq!((flat[read.index()], flat[get.index()]), (0.5, 0.5));
        let decayed = occurrence(&history(&vocab, &calls, &epochs, at), Some(DAY_MS), None);
        assert!(decayed[get.index()] > 0.99 && decayed[read.index()] < 0.01, "{decayed:?}");
    }

    // what this catches: the chain ignoring her habit (read is always followed by edit), or its
    // chances leaving [0, 1].
    #[test]
    fn transition_runs_her_habit_forward_from_her_last_call() {
        let vocab = vocab();
        let (edit, read, get) =
            (vocab.id("code/edit").expect("listed"), vocab.id("code/read").expect("listed"), vocab.id("work/get").expect("listed"));
        let call = |at: u64, tool: ToolId| Call { at: EpochMs::from_millis(at), tool, activity: None, text: String::new() };
        let mut calls = Vec::new();
        for round in 0..20u64 {
            calls.extend([call(round * 3, get), call(round * 3 + 1, read), call(round * 3 + 2, edit)]);
        }
        calls.push(call(100, read));
        let epochs = vec![Epoch { start: EpochMs::from_millis(0), calls: 0..1 }];
        let chance = transition(&history(&vocab, &calls, &epochs, 101));
        assert!(chance[edit.index()] > 0.9, "after read comes edit: {chance:?}");
        assert!(chance[edit.index()] > chance[get.index()]);
        assert!(chance.iter().all(|p| (0.0..=1.0).contains(p)), "{chance:?}");
        let empty = transition(&history(&vocab, &[], &[], 0));
        assert!(empty.iter().all(|p| *p == 0.0));
    }
}
