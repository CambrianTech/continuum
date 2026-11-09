//! Learning in place on arrival (card 4114a2c6): the trigger's consumer of accepted examples.
//!
//! Each accepted example is learned by the engine that serves her, on the same resident weights,
//! inside her own learning memory (`genome::learner`), on her own slot when her page is warm
//! (`slots::learn_slot`): no trainer, lane, window or batch. A bucket's shadow stays seated while
//! its examples keep arriving. Each bucket's step runs through the trigger's owned operations
//! (`run_owned`), so a cancelled tick never drops a slot or a lease mid-step.
//!
//! The journal row's `learned` count is the cursor: an example counts only when the engine
//! committed its update with certain effects. Nothing committed: it is tried again on a later pass.
//! Effects uncertain: the shadow is quarantined and stopped, and the row's remaining examples are
//! learned by the next seat.

use super::durable::Arrival;
use super::{BucketKey, TrainingTriggerState};
use crate::genome::learner::{
    example::learn_messages,
    memory::EngineWorkspace,
    mind::{LearnFailure, MindLearner, StartFailure},
    policy::{learn_policy, LearnPolicy},
    seat::Authority,
};
use crate::inference::engine_residency::{EngineIncarnation, Liveness};
use crate::inference::llama_server::{FreshShadow, LearnOutcome, LearningEngine, LedgerAnswer};
use std::collections::HashMap;
use std::sync::Arc;

/// What an outcome means for the example's place in the journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// The engine committed its update with certain effects: the cursor moves past it.
    Learned,
    /// Nothing was committed (refused, deferred, or a step that ended before an update): again later.
    Again,
    /// A failure mid-step left the shadow's parameters uncertain: it is quarantined and stopped.
    Quarantined,
}

/// PURE: the verdict for one outcome.
pub(crate) fn verdict(outcome: &LearnOutcome) -> Verdict {
    if outcome.effects_uncertain {
        Verdict::Quarantined
    } else if outcome.committed_steps > 0 {
        Verdict::Learned
    } else {
        Verdict::Again
    }
}

/// The seated learners and the engines' workspaces, held for as long as the trigger lives.
#[derive(Default)]
pub(crate) struct InPlace {
    authority: std::sync::OnceLock<Authority>,
    held: tokio::sync::Mutex<Held>,
}

#[derive(Default)]
struct Held {
    minds: HashMap<BucketKey, Seated>,
    workspaces: HashMap<EngineIncarnation, Option<EngineWorkspace>>,
}

struct Seated {
    engine: EngineIncarnation,
    policy: LearnPolicy,
    mind: MindLearner,
}

/// What one bucket's pass did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BucketPass {
    pub(crate) learned: u32,
    pub(crate) deferred: bool,
    pub(crate) quarantined: bool,
}

impl InPlace {
    /// Install the authorities learning answers to; the first pass installs this core's own.
    #[cfg(test)]
    pub(crate) fn install(&self, authority: Authority) {
        let _ = self.authority.set(authority); // first install wins, like the core's
    }

    fn authority(&self) -> Result<&Authority, String> {
        if let Some(authority) = self.authority.get() {
            return Ok(authority);
        }
        let installed = Authority::installed().map_err(|why| why.to_string())?;
        Ok(self.authority.get_or_init(|| installed))
    }

    /// Learn bucket `key`'s pending `rows` (oldest first) on `engine`, until one defers.
    pub(crate) async fn learn_bucket(&self, state: &TrainingTriggerState, key: &BucketKey, rows: Vec<Arrival>, engine: EngineIncarnation) -> Result<BucketPass, String> {
        let authority = self.authority()?;
        let store = state.durable.require()?;
        let mut held = self.held.lock().await;
        let mut pass = BucketPass::default();
        for row in rows {
            let total = u32::try_from(row.examples.len()).map_err(|_| format!("submission {} holds too many examples to count", row.id))?;
            let mut learned = row.learned;
            while learned < total {
                if !seat(authority, &mut held, key, &row, engine).await? {
                    pass.deferred = true;
                    return Ok(pass);
                }
                let slot = match crate::inference::slots::learn_slot(engine, key.persona_id).await {
                    Ok(slot) => slot,
                    Err(_deferred) => {
                        pass.deferred = true;
                        return Ok(pass);
                    }
                };
                let framed = learn_messages(&row.examples[learned as usize]);
                let Held { minds, workspaces } = &mut *held;
                let Some(seated) = minds.get_mut(key) else {
                    return Err(format!("the learner seated for {} left before its step", key.persona_id));
                };
                let workspace = workspaces.entry(engine).or_default();
                let outcome = seated.mind.learn(authority, workspace, framed.messages, framed.tools, slot.slot(), seated.policy.learning_rate).await;
                match outcome {
                    Ok(outcome) => {
                        drop(slot);
                        match verdict(&outcome) {
                            Verdict::Learned => {
                                learned += 1;
                                pass.learned += 1;
                                store.record_learned(row.id, learned).await?;
                            }
                            Verdict::Again => {
                                pass.deferred = true;
                                return Ok(pass);
                            }
                            Verdict::Quarantined => {
                                stop(&mut held, key).await;
                                pass.quarantined = true;
                                return Ok(pass);
                            }
                        }
                    }
                    Err(LearnFailure::Engine(error)) => {
                        // A lost answer is not proof the step ended (it may not have begun). The slot
                        // stays held until her shadow's incarnation-bound unload is confirmed (a late
                        // step on that never-reused id is then refused) or the engine is verifiably dead;
                        // the example is learned again by the next seat.
                        if let Some(seated) = held.minds.remove(key) {
                            fence(seated.mind, engine).await;
                        }
                        drop(slot);
                        continuum_base::probe!(class = "training.in_place.answer_lost", persona = %key.persona_id, error = %error,
                            "a learning step's answer was lost; her shadow was unloaded before the slot was released");
                        pass.deferred = true;
                        return Ok(pass);
                    }
                    Err(failure) => {
                        drop(slot);
                        continuum_base::probe!(class = "training.in_place.deferred", persona = %key.persona_id, reason = %failure,
                            "a learning step did not run; it is tried again on a later pass");
                        pass.deferred = true;
                        return Ok(pass);
                    }
                }
            }
        }
        Ok(pass)
    }
}

/// Make sure a learner is seated for `key` on `engine`, seating one (stopping one seated on another
/// engine) when needed. `false` when it cannot be seated now; the bucket defers.
async fn seat(authority: &Authority, held: &mut Held, key: &BucketKey, row: &Arrival, engine: EngineIncarnation) -> Result<bool, String> {
    if held.minds.get(key).is_some_and(|seated| seated.engine != engine) {
        stop(held, key).await;
    }
    if !held.minds.contains_key(key) {
        let policy = match learn_policy(row.lora.as_ref(), row.schedule.as_ref()) {
            Ok(policy) => policy,
            Err(why) => {
                continuum_base::probe!(class = "training.in_place.unlearnable_policy", persona = %key.persona_id, reason = %why,
                    "this bucket's policy names nothing the engine can learn; it stays pending");
                return Ok(false);
            }
        };
        let learning = LearningEngine::of(engine);
        let expiry = match learning.learn_ledger().await {
            Ok(LedgerAnswer::Ledger(ledger)) => std::time::Duration::from_millis(ledger.max_expiry_ms),
            Ok(LedgerAnswer::NoLedger) => return Err("the serving engine has no learning ledger: nothing can learn in place".into()),
            Err(_unreachable) => return Ok(false),
        };
        let id = uuid::Uuid::new_v4();
        let shadow = FreshShadow {
            name: format!("{}-{}-{}.gguf", key.persona_id.simple(), file_safe(&key.trait_kind), id.simple()),
            rank: policy.rank,
            alpha: policy.alpha,
            targets: policy.targets.clone(),
            top_layers: policy.top_layers,
            seed: id.as_u128() as u32, // a seed per shadow; the truncation keeps 32 of its random bits
        };
        let consumer = format!("learning:{}:{}", key.persona_id, key.trait_kind);
        match MindLearner::start(authority, learning, &consumer, shadow, expiry).await {
            Ok(mind) => {
                held.minds.insert(key.clone(), Seated { engine, policy, mind });
            }
            Err(StartFailure::NotSeated(why)) => {
                continuum_base::probe!(class = "training.in_place.unseated", persona = %key.persona_id, reason = %why, "her shadow was not seated; tried again on a later pass");
                return Ok(false);
            }
            Err(StartFailure::Ungranted { why, seated }) => {
                continuum_base::probe!(class = "training.in_place.ungranted", persona = %key.persona_id, reason = %why, "seated but not granted; stopped, tried again on a later pass");
                let _orphaned = seated.stop().await; // what it holds stays in custody until the engine confirms each release
                return Ok(false);
            }
        }
    }
    Ok(true)
}

/// Stop `key`'s learner; whatever it still holds stays in custody until the engine confirms it.
async fn stop(held: &mut Held, key: &BucketKey) {
    if let Some(seated) = held.minds.remove(key) {
        if let Err((_orphaned, why)) = seated.mind.stop().await {
            continuum_base::probe!(class = "training.in_place.stop_unconfirmed", persona = %key.persona_id, reason = %why,
                "a learner's stop is unconfirmed; custody holds what it held until the engine releases it");
        }
    }
}

/// Stop `mind` until the engine confirms it (its shadow unloaded and released, bound to the
/// incarnation), or until that engine's process is verifiably dead.
async fn fence(mind: MindLearner, engine: EngineIncarnation) {
    let mut every = tokio::time::interval(std::time::Duration::from_secs(1));
    let mut mind = mind;
    loop {
        match mind.stop().await {
            Ok(()) => return,
            Err((unconfirmed, _why)) => mind = unconfirmed,
        }
        if engine.liveness() == Liveness::Dead {
            return; // the engine that held it is gone; custody keeps what the mind leased
        }
        every.tick().await;
    }
}

/// `trait_kind` as a file-name part (`code/owner` -> `code-owner`).
fn file_safe(text: &str) -> String {
    text.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

impl TrainingTriggerState {
    /// Learn every pending submission whose base the engine at `lane` serves, one bucket at a
    /// time, each through the trigger's owned operations.
    pub(crate) async fn learn_in_place(self: &Arc<Self>, engine: EngineIncarnation, served_model: &str) -> Result<(), String> {
        const PAGE: usize = 256;
        let store = self.durable.require()?;
        if let Ok(authority) = self.in_place.authority() {
            authority.custody.settle_orphans().await;
        }
        let mut buckets: Vec<(BucketKey, Vec<Arrival>)> = Vec::new();
        for row in store.arrivals(0, PAGE).await? {
            if !crate::model_registry::artifacts::names_the_same_base(served_model, &row.key.base_model) {
                continue;
            }
            match buckets.iter_mut().find(|(key, _)| *key == row.key) {
                Some((_, rows)) => rows.push(row),
                None => buckets.push((row.key.clone(), vec![row])),
            }
        }
        for (key, rows) in buckets {
            let pass = self.run_owned(key, move |state, key| async move { state.in_place.learn_bucket(&state, &key, rows, engine).await }).await??;
            if pass.learned > 0 || pass.quarantined {
                continuum_base::probe!(class = "training.in_place.pass", learned = u64::from(pass.learned), quarantined = pass.quarantined, deferred = pass.deferred,
                    "accepted examples learned in place");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
