//! The replay: every mind's epochs in one global time order, each policy asked for a head at
//! each turnover with only the past in hand, and scored on the calls that epoch then held.
//!
//! Nothing reaches a policy before it happened. A mind's own past ends at the turnover; the
//! society's usage and a learning policy's lesson from an epoch arrive only once that epoch
//! has ENDED (its last call), so a mind turning over mid-way through another mind's epoch
//! never sees that epoch's later calls.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::cost::CostModel;
use crate::epoch::Epoch;
use crate::head::HeadSet;
use crate::history::{EpochMs, Call, History};
use crate::vocab::{ToolId, Vocabulary};
use crate::WorkingSetPolicy;

/// One mind's record, oldest call first, cut into epochs.
#[derive(Debug, Clone)]
pub struct Mind {
    /// A display label for the report; minds are identified by their index.
    pub label: String,
    pub calls: Vec<Call>,
    pub epochs: Vec<Epoch>,
}

impl Mind {
    /// How many of her epochs, the earliest, are training history.
    pub fn train_epochs(&self, train_fraction: f64) -> usize {
        (self.epochs.len() as f64 * train_fraction).floor() as usize
    }
}

/// A turnover being decided, or an epoch that has ended, with everything known at its turnover.
pub struct Turn<'a> {
    pub mind: usize,
    pub epoch: usize,
    /// Whether the epoch is in the later, held-out part of her history.
    pub test: bool,
    pub history: History<'a>,
    /// The calls the epoch held: read only to score, or once it has ended.
    pub calls: &'a [Call],
    /// The distinct tools those calls used, in id order.
    pub used: &'a [ToolId],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Decide,
    Ended,
}

/// Walk every epoch of every mind in time order, calling `visit` at each turnover and again
/// when the epoch has ended.
///
/// The society signal is the other minds' ended epochs, each weighing `0.5^(age / half_life)`.
pub fn walk(vocab: &Vocabulary, minds: &[Mind], train_fraction: f64, population_half_life_ms: u64, mut visit: impl FnMut(Event, &Turn<'_>)) {
    let mut order: Vec<(EpochMs, usize, usize)> = minds
        .iter()
        .enumerate()
        .flat_map(|(mind, record)| record.epochs.iter().enumerate().map(move |(epoch, e)| (e.start, mind, epoch)))
        .collect();
    order.sort_unstable();
    let Some(&(origin, _, _)) = order.first() else {
        return;
    };
    let mut society = Society::new(vocab.len(), minds.len(), origin, population_half_life_ms);
    let used: Vec<Vec<Vec<ToolId>>> = minds
        .iter()
        .map(|mind| {
            mind.epochs
                .iter()
                .map(|epoch| {
                    let mut tools: Vec<ToolId> = mind.calls[epoch.calls.clone()].iter().map(|call| call.tool).collect();
                    tools.sort_unstable();
                    tools.dedup();
                    tools
                })
                .collect()
        })
        .collect();
    // Epochs decided but not yet ended: (end, mind, epoch), with the society as it stood at the turnover.
    let mut open: BinaryHeap<Reverse<(EpochMs, usize, usize)>> = BinaryHeap::new();
    let mut known_at_turnover: Vec<Vec<Option<Vec<f32>>>> = minds.iter().map(|mind| vec![None; mind.epochs.len()]).collect();
    let end_until = |until: Option<EpochMs>,
                     society: &mut Society,
                     open: &mut BinaryHeap<Reverse<(EpochMs, usize, usize)>>,
                     known: &mut [Vec<Option<Vec<f32>>>],
                     visit: &mut dyn FnMut(Event, &Turn<'_>)| {
        while let Some(Reverse((end, mind, epoch))) = open.peek().copied() {
            if until.is_some_and(|until| end > until) {
                break;
            }
            open.pop();
            let population = known[mind][epoch].take();
            let (history, calls) = turn_of(vocab, minds, mind, epoch, population.as_deref());
            let test = epoch >= minds[mind].train_epochs(train_fraction);
            visit(Event::Ended, &Turn { mind, epoch, test, history, calls, used: &used[mind][epoch] });
            society.add(mind, minds[mind].epochs[epoch].start, &used[mind][epoch]);
        }
    };
    for (start, mind, epoch) in order {
        end_until(Some(start), &mut society, &mut open, &mut known_at_turnover, &mut visit);
        let population = society.others(mind);
        let (history, calls) = turn_of(vocab, minds, mind, epoch, population.as_deref());
        let test = epoch >= minds[mind].train_epochs(train_fraction);
        visit(Event::Decide, &Turn { mind, epoch, test, history, calls, used: &used[mind][epoch] });
        let end = calls.last().map_or(start, |call| call.at); // map_or: an epoch always holds a call; its start is its end otherwise
        open.push(Reverse((end, mind, epoch)));
        known_at_turnover[mind][epoch] = population;
    }
    end_until(None, &mut society, &mut open, &mut known_at_turnover, &mut visit);
}

/// Mind `mind`'s past at the turnover of her epoch `epoch`, and the calls that epoch held.
fn turn_of<'a>(vocab: &'a Vocabulary, minds: &'a [Mind], mind: usize, epoch: usize, population: Option<&'a [f32]>) -> (History<'a>, &'a [Call]) {
    let record = &minds[mind];
    let span = &record.epochs[epoch];
    let history = History {
        vocab,
        calls: &record.calls[..span.calls.start],
        epochs: &record.epochs[..epoch],
        at: span.start,
        activity: record.calls[span.calls.start].activity,
        population,
    };
    (history, &record.calls[span.calls.clone()])
}

/// The minds' ended epochs as decayed per-tool weights, so each mind can read the others'.
///
/// Weights are kept relative to `origin` (an epoch at `t` adds `2^((t - origin) / half_life)`)
/// so no stored sum ever needs re-decaying; the ratio of two sums is the decayed fraction.
/// An f64 holds that growth for ~1,000 half-lives of history.
struct Society {
    origin: EpochMs,
    half_life_ms: f64,
    total: Vec<f64>,
    total_weight: f64,
    own: Vec<Vec<f64>>,
    own_weight: Vec<f64>,
}

impl Society {
    fn new(tools: usize, minds: usize, origin: EpochMs, half_life_ms: u64) -> Self {
        Self {
            origin,
            half_life_ms: half_life_ms.max(1) as f64,
            total: vec![0.0; tools],
            total_weight: 0.0,
            own: vec![vec![0.0; tools]; minds],
            own_weight: vec![0.0; minds],
        }
    }

    fn add(&mut self, mind: usize, start: EpochMs, used: &[ToolId]) {
        let weight = 2f64.powf(start.since(self.origin).millis() as f64 / self.half_life_ms);
        for tool in used {
            self.total[tool.index()] += weight;
            self.own[mind][tool.index()] += weight;
        }
        self.total_weight += weight;
        self.own_weight[mind] += weight;
    }

    /// The other minds' decayed chance of calling each tool in an epoch, when any has ended one.
    fn others(&self, mind: usize) -> Option<Vec<f32>> {
        let weight = self.total_weight - self.own_weight[mind];
        (weight > 0.0).then(|| {
            self.total.iter().zip(&self.own[mind]).map(|(total, own)| ((total - own) / weight) as f32).collect()
        })
    }
}

/// What a policy cost over some epochs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Score {
    pub epochs: u64,
    pub calls: u64,
    /// Calls whose verb was neither in the head nor already loaded into the tail this epoch.
    pub misses: u64,
    /// Head schema tokens of verbs she did not call in the epoch.
    pub waste_tokens: u64,
    pub head_tokens: u64,
    pub head_tools: u64,
    /// Head changes away from a turnover: zero by construction, the replay asks once per epoch.
    pub churn: u64,
    /// Turnovers whose head differs from her previous epoch's (free: the prefix restarts anyway).
    pub turnover_changes: u64,
}

impl Score {
    /// Score one epoch's `calls` against `head`.
    pub fn epoch(vocab: &Vocabulary, head: &HeadSet, calls: &[Call], used: &[ToolId], changed: bool) -> Self {
        let mut loaded: Vec<ToolId> = Vec::new();
        let mut misses = 0;
        for call in calls {
            if !head.contains(call.tool) && !loaded.contains(&call.tool) {
                misses += 1;
                loaded.push(call.tool);
            }
        }
        let waste_tokens = head.tools().iter().filter(|tool| used.binary_search(tool).is_err()).map(|tool| u64::from(vocab.tokens(*tool))).sum();
        Self {
            epochs: 1,
            calls: calls.len() as u64,
            misses,
            waste_tokens,
            head_tokens: head.tokens(),
            head_tools: head.tools().len() as u64,
            churn: 0,
            turnover_changes: u64::from(changed),
        }
    }

    pub fn add(&mut self, other: &Score) {
        self.epochs += other.epochs;
        self.calls += other.calls;
        self.misses += other.misses;
        self.waste_tokens += other.waste_tokens;
        self.head_tokens += other.head_tokens;
        self.head_tools += other.head_tools;
        self.churn += other.churn;
        self.turnover_changes += other.turnover_changes;
    }

    /// Waste plus one extra act per miss, in prompt-token equivalents.
    pub fn cost_tokens(&self, cost: &CostModel) -> f64 {
        self.waste_tokens as f64 + self.misses as f64 * cost.miss_tokens()
    }

    pub fn per_epoch(&self, value: f64) -> f64 {
        if self.epochs == 0 { 0.0 } else { value / self.epochs as f64 }
    }
}

/// Scores of each policy (outer), each mind (middle), train and test (inner, test second).
pub type Scores = Vec<Vec<[Score; 2]>>;

/// Replay every policy over every mind, and the hindsight oracle (the head that holds exactly
/// what she called) as the last row.
pub fn replay(
    vocab: &Vocabulary,
    minds: &[Mind],
    policies: &mut [Box<dyn WorkingSetPolicy>],
    budget_tokens: u32,
    train_fraction: f64,
    population_half_life_ms: u64,
) -> Scores {
    let rows = policies.len() + 1;
    let mut scores: Scores = vec![vec![[Score::default(); 2]; minds.len()]; rows];
    let mut previous: Vec<Vec<Option<HeadSet>>> = vec![vec![None; minds.len()]; rows];
    walk(vocab, minds, train_fraction, population_half_life_ms, |event, turn| match event {
        Event::Decide => {
            let oracle = HeadSet::of(vocab, turn.used.to_vec());
            for (row, policy) in policies.iter().map(Some).chain([None]).enumerate() {
                let head = match policy {
                    Some(policy) => policy.head_for_epoch(&turn.history, budget_tokens),
                    None => oracle.clone(),
                };
                let changed = previous[row][turn.mind].as_ref().is_some_and(|last| *last != head);
                let score = Score::epoch(vocab, &head, turn.calls, turn.used, changed);
                scores[row][turn.mind][usize::from(turn.test)].add(&score);
                previous[row][turn.mind] = Some(head);
            }
        }
        Event::Ended => {
            for policy in policies.iter_mut() {
                policy.observe(&turn.history, turn.used);
            }
        }
    });
    scores
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::epoch::segment;
    use crate::vocab::ToolSchema;

    fn vocab() -> Vocabulary {
        let schema = |name: &str, tokens| ToolSchema { name: name.into(), description: String::new(), tokens };
        Vocabulary::new(vec![schema("code/read", 10), schema("work/get", 20)]).expect("small vocabulary")
    }

    fn mind(vocab: &Vocabulary, calls: &[(u64, &str)]) -> Mind {
        let calls: Vec<Call> = calls
            .iter()
            .map(|(at, name)| Call { at: EpochMs::from_millis(*at), tool: vocab.id(name).expect("listed"), activity: None, text: String::new() })
            .collect();
        let epochs = segment(&calls, 100);
        Mind { label: String::new(), calls, epochs }
    }

    // what this catches: the replay leaking the future. A's epoch runs from 0 to 90; B turns
    // over at 50, inside it, and must not see it in the society; B's next turnover at 300 does.
    // Her own history at a turnover ends before the turnover.
    #[test]
    fn the_society_and_the_lessons_arrive_only_after_an_epoch_has_ended() {
        let vocab = vocab();
        let a = mind(&vocab, &[(0, "work/get"), (90, "work/get")]);
        let b = mind(&vocab, &[(50, "code/read"), (300, "code/read")]);
        let get = vocab.id("work/get").expect("listed").index();
        let mut seen = Vec::new();
        walk(&vocab, &[a, b], 0.5, 1_000, |event, turn| {
            seen.push((event, turn.mind, turn.epoch, turn.history.population.map(|p| p[get]), turn.history.calls.len()));
        });
        let decided: Vec<_> = seen.iter().filter(|row| row.0 == Event::Decide).map(|row| (row.1, row.2, row.3, row.4)).collect();
        assert_eq!(decided, vec![(0, 0, None, 0), (1, 0, None, 0), (1, 1, Some(1.0), 1)], "{seen:?}");
        assert_eq!(seen.iter().filter(|row| row.0 == Event::Ended).count(), 3);
    }

    // what this catches: a verb loaded into the tail by a miss being charged again on its
    // second call, and waste counting a held verb she did call.
    #[test]
    fn a_miss_loads_the_verb_for_the_rest_of_the_epoch_and_waste_is_only_unused_schemas() {
        let vocab = vocab();
        let record = mind(&vocab, &[(0, "work/get"), (1, "work/get"), (2, "code/read")]);
        let read = vocab.id("code/read").expect("listed");
        let mut used = vec![read, vocab.id("work/get").expect("listed")];
        used.sort_unstable();
        let head = HeadSet::of(&vocab, vec![read]);
        let score = Score::epoch(&vocab, &head, &record.calls, &used, false);
        assert_eq!((score.misses, score.waste_tokens, score.head_tokens), (1, 0, 10));
        let wide = HeadSet::of(&vocab, vec![read, vocab.id("work/get").expect("listed")]);
        let only_read = Score::epoch(&vocab, &wide, &record.calls[2..], &[read], false);
        assert_eq!((only_read.misses, only_read.waste_tokens), (0, 20));
    }
}
