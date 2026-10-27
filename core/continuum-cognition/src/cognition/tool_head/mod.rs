//! Her learned tool head (plan goal 10, card ecd5dff3): which tool schemas the head of her
//! prompt carries for an epoch, learned online from her own calls, and which verbs she has
//! loaded into the tail since it turned over.
//!
//! # The rules it keeps
//!
//! - The head changes ONLY when her view starts a new epoch: the schemas render right after
//!   the system prompt, so any other change would re-prefill everything after them. Each act
//!   computes the head the NEXT epoch would hold ([`ToolHead::next`]), the budget reserves
//!   the larger of the two, and [`ToolHead::turn_over`] adopts it at the turnover.
//! - Every verb stays reachable. One she does not hold, she finds with `commands/help`; its
//!   schema is in that result (the tail) and the verb rides the request's `tail_tools`, which
//!   the engine's parser and grammar accept without rendering it in the head. It stays loaded
//!   until the epoch ends.
//! - The policy is the replay-tested one (`tool-working-set`, slice 1): an online logistic
//!   blend of her recent and standing habits, the activity's, her call-to-call chain and the
//!   relevance of her latest acts, deciding each verb by `p · miss > schema · (1 − p)`. It
//!   learns at every turnover from the epoch that just ended.
//!
//! Its state ([`ToolHeadState`]) is hers and survives a restart (see `entity`).

pub mod entity;
#[cfg(test)]
mod tests;

use continuum_base::time::EpochMs;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tool_working_set::features::{Signals, LONG_HALF_LIFE_MS};
use tool_working_set::head::{choose, HeadSet};
use tool_working_set::logistic::Logistic;
use tool_working_set::policy::blend::examples;
use tool_working_set::relevance::Lexical;
use tool_working_set::{ActivityId, Call, CostModel, Epoch, History, ToolId, Vocabulary};
use uuid::Uuid;

/// The verb that finds the others: always held, so every verb is one call away.
pub const FINDER: &str = "commands/help";

/// The online learner's step size: the rate the slice-1 replay ran (`--online-rate 0.05`, the
/// best held-out cost of every policy there).
const ONLINE_RATE: f64 = 0.05;

/// One call she made.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UseRecord {
    pub at_ms: u64,
    /// Canonical command name (`code/read`).
    pub verb: String,
    pub room: Option<Uuid>,
    /// Her act as recorded (the call line), the text relevance reads.
    pub text: String,
    /// The epoch it fell in.
    pub epoch: u64,
}

/// Where one of her epochs began.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpochStart {
    pub epoch: u64,
    pub start_ms: u64,
}

/// Everything the head knows: what she called, when her epochs turned over, what this epoch
/// holds and has loaded, and the learned weights.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ToolHeadState {
    pub uses: Vec<UseRecord>,
    /// Her epochs, oldest first; the last is the current one.
    pub epochs: Vec<EpochStart>,
    /// The verbs the current epoch's head holds; `None` until the first learned head is adopted
    /// (until then her surface is the activity's static one).
    pub held: Option<Vec<String>>,
    /// Verbs loaded into the tail this epoch.
    pub loaded: Vec<String>,
    pub model: Logistic,
}

/// What one ended epoch cost, the miss/waste receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EpochCost {
    pub calls: u64,
    /// Distinct verbs she called that the head did not hold: each cost one find.
    pub misses: u64,
    /// Head schema tokens of verbs she did not call.
    pub waste_tokens: u64,
    pub head_tokens: u64,
    /// Whether a learned head was held (before the first adoption her surface was static).
    pub learned: bool,
}

/// The vocabulary the head is chosen from and the relevance model built over it, rebuilt
/// whenever her tool set or model changes.
#[derive(Debug)]
struct Live {
    vocab: Vocabulary,
    lexical: Lexical,
}

/// Her tool head. Interior-mutable, one per mind, shared by the act path (which notes her
/// calls) and the deliberation faculty (which reads and adopts the head).
#[derive(Debug, Default)]
pub struct ToolHead {
    state: Mutex<ToolHeadState>,
    live: Mutex<Option<Live>>,
    /// The head the budget reserved this act: what a turnover in this act adopts.
    reserved: Mutex<Option<HeadSet>>,
    /// Her row in her own store, attached for a resident mind (never a measurement fork).
    store: Mutex<Option<entity::ToolHeadStore>>,
}

impl ToolHead {
    /// The verbs the head may hold, with each schema's price.
    pub fn set_vocabulary(&self, vocab: Vocabulary) {
        let lexical = Lexical::new(&vocab);
        *self.live.lock() = Some(Live { vocab, lexical });
    }

    /// Her calls of one act. A `commands/help` call naming a verb loads that verb into the
    /// tail; so do the verbs a help query returned (`found`).
    pub fn note(&self, at: EpochMs, room: Option<Uuid>, verbs: &[String], found: &[String], text: &str) {
        let mut state = self.state.lock();
        let epoch = state.epochs.last().map_or(0, |start| start.epoch); // map_or: calls before any turnover fall in epoch 0
        if state.epochs.is_empty() {
            state.epochs.push(EpochStart { epoch, start_ms: at.millis() });
        }
        for verb in verbs {
            state.uses.push(UseRecord { at_ms: at.millis(), verb: verb.clone(), room, text: text.to_owned(), epoch });
        }
        for verb in found {
            let held = state.held.as_ref().is_some_and(|held| held.contains(verb));
            if !held && !state.loaded.contains(verb) {
                state.loaded.push(verb.clone());
            }
        }
    }

    /// The head the next epoch would hold, were it to turn over now (her current epoch counted
    /// as ended), reserved for a turnover in this act to adopt. `None` when there is no
    /// vocabulary or no call to learn from yet.
    pub fn next(&self, at: EpochMs, activity: Option<Uuid>, budget_tokens: u32, cost: &CostModel) -> Option<HeadSet> {
        let live = self.live.lock();
        let live = live.as_ref()?;
        let state = self.state.lock();
        let (calls, epochs) = replayable(&state, &live.vocab, None);
        if calls.is_empty() {
            return None;
        }
        let history = History {
            vocab: &live.vocab,
            calls: &calls,
            epochs: &epochs,
            at,
            activity: activity.map(ActivityId),
            population: None,
        };
        let head = head_from(&history, &live.lexical, &state.model, cost, budget_tokens);
        *self.reserved.lock() = Some(head.clone());
        Some(head)
    }

    /// Her view started a new epoch at `at`: learn from the epoch that ended, report what it
    /// cost, hold the head the budget reserved this act, and keep the state in her store.
    pub fn turn_over_reserved(&self, at: EpochMs) -> Option<EpochCost> {
        let reserved = self.reserved.lock().take();
        let receipt = self.turn_over(at, reserved.as_ref());
        self.keep();
        receipt
    }

    /// [`Self::turn_over_reserved`] with an explicit head. Epochs are numbered here,
    /// monotonically across restarts (the view's own counter restarts).
    pub fn turn_over(&self, at: EpochMs, next: Option<&HeadSet>) -> Option<EpochCost> {
        let live = self.live.lock();
        let mut state = self.state.lock();
        let receipt = live.as_ref().and_then(|live| learn_from_ended(&mut state, live));
        if let Some(live) = live.as_ref() {
            if let Some(next) = next {
                state.held = Some(next.tools().iter().map(|tool| live.vocab.name(*tool).to_owned()).collect());
            }
        }
        state.loaded.clear();
        let epoch = state.epochs.last().map_or(0, |start| start.epoch + 1); // map_or: the first turnover opens epoch 0
        state.epochs.push(EpochStart { epoch, start_ms: at.millis() });
        prune(&mut state, at);
        receipt
    }

    /// The verbs the current epoch's head holds, once a learned head has been adopted.
    pub fn held(&self) -> Option<Vec<String>> {
        self.state.lock().held.clone()
    }

    /// The verbs loaded into the tail this epoch.
    pub fn loaded(&self) -> Vec<String> {
        self.state.lock().loaded.clone()
    }

    /// Attach her store and bring back what a previous life kept. A load that lands after she
    /// has already acted this life keeps the live state (the newer evidence) and says so.
    pub fn attach(self: &std::sync::Arc<Self>, store: entity::ToolHeadStore) {
        *self.store.lock() = Some(store.clone());
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            continuum_base::probe!(class = "delib.tool_head.unpersisted", "no runtime to read her saved tool head on");
            return;
        };
        let head = std::sync::Arc::clone(self);
        runtime.spawn(async move {
            match store.load().await {
                Ok(Some(saved)) => {
                    let mut state = head.state.lock();
                    if state.uses.is_empty() {
                        *state = saved;
                    } else {
                        continuum_base::probe!(
                            class = "delib.tool_head.restore_late",
                            live_uses = state.uses.len() as u64,
                            saved_uses = saved.uses.len() as u64,
                            "her saved tool head arrived after she acted; the live state stays"
                        );
                    }
                }
                Ok(None) => {}
                Err(error) => continuum_base::probe!(class = "delib.tool_head.load_failed", error = %error, "her saved tool head could not be read"),
            }
        });
    }

    /// Write her state to her store, off the act path.
    fn keep(&self) {
        let Some(store) = self.store.lock().clone() else {
            return;
        };
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let state = self.snapshot();
        runtime.spawn(async move {
            if let Err(error) = store.keep(state).await {
                continuum_base::probe!(class = "delib.tool_head.save_failed", error = %error, "her tool head could not be saved");
            }
        });
    }

    pub fn snapshot(&self) -> ToolHeadState {
        self.state.lock().clone()
    }

    pub fn restore(&self, state: ToolHeadState) {
        *self.state.lock() = state;
    }
}

/// The verbs a batch of calls found with the finder: `commands/help(name=X)` loads `X`, by its
/// canonical name whichever dialect she asked in (`read_file` loads `code/read`).
pub fn found_by_finder<'a>(calls: impl Iterator<Item = &'a crate::ai::types::ToolCall>) -> Vec<String> {
    calls
        .filter(|call| call.name == FINDER)
        .filter_map(|call| call.input.get("name").and_then(serde_json::Value::as_str))
        .map(|name| crate::ai::tool_dialect::resolve_wire_name(name).to_string())
        .collect()
}

/// Her calls as the policy reads them, and her epochs before `before` (an epoch seq; `None`:
/// every epoch, the current one counted as ended). Calls of verbs the vocabulary lacks are
/// left out (a verb her tool set no longer has cannot be held).
fn replayable(state: &ToolHeadState, vocab: &Vocabulary, before: Option<u64>) -> (Vec<Call>, Vec<Epoch>) {
    let mut calls = Vec::new();
    let mut epochs: Vec<Epoch> = Vec::new();
    for start in &state.epochs {
        if before.is_some_and(|before| start.epoch >= before) {
            break;
        }
        let first = calls.len();
        for record in state.uses.iter().filter(|record| record.epoch == start.epoch) {
            if let Some(tool) = vocab.id(&record.verb) {
                calls.push(Call {
                    at: EpochMs::from_millis(record.at_ms),
                    tool,
                    activity: record.room.map(ActivityId),
                    text: record.text.clone(),
                });
            }
        }
        if calls.len() > first {
            epochs.push(Epoch { start: EpochMs::from_millis(start.start_ms), calls: first..calls.len() });
        }
    }
    (calls, epochs)
}

/// The head the blend chooses for `history`, the finder always held.
fn head_from(history: &History<'_>, lexical: &Lexical, model: &Logistic, cost: &CostModel, budget_tokens: u32) -> HeadSet {
    let signals = Signals::of(history, lexical);
    let chance: Vec<f32> = (0..history.vocab.len())
        .map(|tool| if signals.any(tool) { model.chance(&signals.row(tool)) } else { 0.0 })
        .collect();
    let pinned: Vec<ToolId> = history.vocab.id(FINDER).into_iter().collect();
    choose(history.vocab, &chance, cost, budget_tokens, &pinned)
}

/// Teach the model the epoch that just ended (the last one), and what it cost against the
/// head it held. `None` when no epoch has ended with a call in it.
fn learn_from_ended(state: &mut ToolHeadState, live: &Live) -> Option<EpochCost> {
    let ended = *state.epochs.last()?;
    let mut used: Vec<ToolId> =
        state.uses.iter().filter(|record| record.epoch == ended.epoch).filter_map(|record| live.vocab.id(&record.verb)).collect();
    let calls = used.len() as u64;
    if used.is_empty() {
        return None;
    }
    used.sort_unstable();
    used.dedup();
    let (prior_calls, prior_epochs) = replayable(state, &live.vocab, Some(ended.epoch));
    let history = History {
        vocab: &live.vocab,
        calls: &prior_calls,
        epochs: &prior_epochs,
        at: EpochMs::from_millis(ended.start_ms),
        activity: state
            .uses
            .iter()
            .find(|record| record.epoch == ended.epoch)
            .and_then(|record| record.room)
            .map(ActivityId),
        population: None,
    };
    let signals = Signals::of(&history, &live.lexical);
    state.model.learn(&examples(&signals, &used), ONLINE_RATE);
    let held: Vec<ToolId> =
        state.held.iter().flatten().filter_map(|verb| live.vocab.id(verb)).collect();
    let held_set = HeadSet::of(&live.vocab, held);
    Some(EpochCost {
        calls,
        misses: used.iter().filter(|tool| !held_set.contains(**tool)).count() as u64,
        waste_tokens: held_set.tools().iter().filter(|tool| used.binary_search(tool).is_err()).map(|tool| u64::from(live.vocab.tokens(*tool))).sum(),
        head_tokens: held_set.tokens(),
        learned: state.held.is_some(),
    })
}

/// Drop calls older than four of the longest half-lives the policy reads (they weigh under a
/// sixteenth) and the epochs left with none, so her state stays bounded by her own pace.
fn prune(state: &mut ToolHeadState, at: EpochMs) {
    let horizon = at.millis().saturating_sub(4 * LONG_HALF_LIFE_MS);
    state.uses.retain(|record| record.at_ms >= horizon);
    let current = state.epochs.last().map(|start| start.epoch);
    let uses = &state.uses;
    state.epochs.retain(|start| Some(start.epoch) == current || uses.iter().any(|record| record.epoch == start.epoch));
}
