//! Learning in place on arrival (card 4114a2c6): the trigger's consumer of accepted examples.
//!
//! Each accepted example is learned by the engine that serves her, on the same resident weights,
//! inside her own learning memory (`genome::learner`), on her own slot when her page is warm
//! (`slots::learn_slot`): no trainer, lane, window or batch. A bucket's shadow stays seated while
//! its examples keep arriving.
//!
//! Durability: an update the engine committed lives in its memory until a checkpoint generation
//! holding it is published, so the journal's cursor moves only with a confirmed generation
//! (`durable::LearnCheckpoint`). Progress since the newest one is volatile: after a restart the
//! bucket's fresh shadow is restored from its generation and the examples past its positions are
//! learned again. Effects uncertain: the shadow is stopped and its volatile progress dropped.
//!
//! Ownership: each engine's learning is serialized by its own lock (one context learns one step at
//! a time); a bucket that finds its engine busy defers rather than queueing, so one wedged engine
//! never holds another's buckets. Each step runs inside the trigger's owned operations, and a lost
//! answer's fence (a confirmed incarnation-bound stop, or the engine's verified death) runs as its
//! own owned operation holding that engine's lock and the slot, never on the tick.

use super::durable::{Arrival, LearnCheckpoint, Position};
use super::{BucketKey, TrainingTriggerState};
use crate::genome::learner::{
    example::learn_messages,
    memory::EngineWorkspace,
    mind::{LearnFailure, MindLearner, StartFailure},
    policy::{learn_policy, LearnPolicy},
    seat::Authority,
};
use crate::inference::engine_residency::{EngineIncarnation, Liveness};
use crate::inference::llama_server::{CheckpointAnswer, FreshShadow, LearnOutcome, LearningEngine, LedgerAnswer, RestoreCheckpoint, RestoreOutcome};
use crate::inference::slots::LearnSlot;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// What an outcome means for the example's place in the journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// The engine committed its update with certain effects: volatile progress moves past it.
    Learned,
    /// Nothing was committed (refused, deferred, or a step that ended before an update): again later.
    Again,
    /// A failure mid-step left the shadow's parameters uncertain: it is quarantined and stopped.
    Quarantined,
}

/// PURE: the verdict for one learning outcome.
pub(crate) fn verdict(outcome: &LearnOutcome) -> Verdict {
    if outcome.effects_uncertain {
        Verdict::Quarantined
    } else if outcome.committed_steps > 0 {
        Verdict::Learned
    } else {
        Verdict::Again
    }
}

/// PURE: the verdict for one restore (restored is learned's analogue).
pub(crate) fn restored(outcome: &RestoreOutcome) -> Verdict {
    if outcome.effects_uncertain {
        Verdict::Quarantined
    } else if outcome.restored {
        Verdict::Learned
    } else {
        Verdict::Again
    }
}

/// PURE: the example a row's next step starts at: past everything its journal row, its learner's
/// newest generation and its volatile progress hold (a row neither names holds none there).
pub(crate) fn start(journal: u32, checkpointed: Option<u32>, progress: Option<u32>) -> u32 {
    journal.max(checkpointed.unwrap_or(0)).max(progress.unwrap_or(0)) // unwrap_or: a row the generation or progress does not name holds none there
}

/// The learners seated in place, per engine, held for as long as the trigger lives.
#[derive(Default)]
pub(crate) struct InPlace {
    authority: std::sync::OnceLock<Authority>,
    engines: dashmap::DashMap<EngineIncarnation, Arc<tokio::sync::Mutex<EngineLearning>>>,
}

/// One engine's learning: its seated learners and its one workspace.
#[derive(Default)]
struct EngineLearning {
    minds: HashMap<BucketKey, Seated>,
    workspace: Option<EngineWorkspace>,
}

struct Seated {
    policy: LearnPolicy,
    /// The signature of the competence she learns here, minted from the examples that seated her
    /// (a fork's parent on it): stamped on the gene her generation becomes.
    signature: Option<crate::genome::signature::GeneSignature>,
    /// The longest grant the engine accepts, as it reported it: what a renewal asks for.
    expiry: std::time::Duration,
    mind: MindLearner,
    /// The binding her checkpoints carry: the bucket's identity.
    binding: String,
    /// A generation to restore before her first step (a reseat after a restart or a stop).
    restore: Option<LearnCheckpoint>,
    /// Examples learned since the newest confirmed generation, per submission: volatile.
    progress: HashMap<Uuid, u32>,
    /// The positions her newest confirmed generation holds: the floor every step starts from, so a
    /// row read before its projection (a crash between a commit and its projection) is never
    /// learned again (Codex on 76f829e2b).
    checkpointed: HashMap<Uuid, u32>,
}

/// What one bucket's pass did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BucketPass {
    pub(crate) learned: u32,
    pub(crate) deferred: bool,
    pub(crate) quarantined: bool,
    pub(crate) checkpointed: bool,
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

    fn engine(&self, engine: EngineIncarnation) -> Arc<tokio::sync::Mutex<EngineLearning>> {
        Arc::clone(self.engines.entry(engine).or_default().value())
    }

    /// Learn bucket `key`'s pending `rows` (oldest first) on `engine` until one defers, then publish a
    /// checkpoint of what was learned and move the journal's cursor with it.
    pub(crate) async fn learn_bucket(&self, state: &Arc<TrainingTriggerState>, key: &BucketKey, rows: Vec<Arrival>, engine: EngineIncarnation) -> Result<BucketPass, String> {
        let authority = self.authority()?;
        let Ok(mut held) = self.engine(engine).try_lock_owned() else {
            return Ok(BucketPass { deferred: true, ..Default::default() }); // a step or a fence holds this engine: later
        };
        let mut pass = BucketPass::default();
        match steps(authority, state, &mut held, key, &rows, engine, &mut pass).await? {
            Lost::No => {}
            Lost::Yes { mind, slot } => {
                // the fence runs on its own, holding this engine and the slot; the tick moves on
                state.spawn_owned(async move {
                    fence(mind, engine).await;
                    drop(slot);
                    drop(held);
                })
                .await;
                return Ok(pass);
            }
        }
        if let Some(seated) = held.minds.get_mut(key) {
            pass.checkpointed = checkpoint(state, key, seated).await?;
        }
        Ok(pass)
    }
}

/// Whether a step's answer was lost, with what the fence must hold.
enum Lost {
    No,
    Yes { mind: MindLearner, slot: LearnSlot },
}

/// The steps of one bucket's pass, until one defers.
async fn steps(
    authority: &Authority,
    state: &TrainingTriggerState,
    held: &mut EngineLearning,
    key: &BucketKey,
    rows: &[Arrival],
    engine: EngineIncarnation,
    pass: &mut BucketPass,
) -> Result<Lost, String> {
    let mut renewed = false;
    for row in rows {
        let total = u32::try_from(row.examples.len()).map_err(|_| format!("submission {} holds too many examples to count", row.id))?;
        loop {
            if !seat(authority, state, held, key, row, rows, engine).await? {
                pass.deferred = true;
                return Ok(Lost::No);
            }
            let EngineLearning { minds, workspace } = held;
            let Some(seated) = minds.get_mut(key) else {
                return Err(format!("the learner seated for {} left before its step", key.persona_id));
            };
            let learned = start(row.learned, seated.checkpointed.get(&row.id).copied(), seated.progress.get(&row.id).copied());
            if learned >= total {
                break;
            }
            let slot = match crate::inference::slots::learn_slot(engine, key.persona_id).await {
                Ok(slot) => slot,
                Err(_deferred) => {
                    pass.deferred = true;
                    return Ok(Lost::No);
                }
            };
            let framed = learn_messages(&row.examples[learned as usize]);
            let rate = seated.policy.learning_rate;
            let step = match seated.restore.as_ref() {
                Some(generation) => {
                    let from = RestoreCheckpoint { name: generation.name.clone(), binding: generation.binding.clone(), steps: generation.steps };
                    seated.mind.restore(authority, workspace, framed.messages, framed.tools, slot.slot(), rate, &from).await.map(|outcome| (restored(&outcome), true, outcome.refusal))
                }
                None => seated.mind.learn(authority, workspace, framed.messages, framed.tools, slot.slot(), rate).await.map(|outcome| (verdict(&outcome), false, outcome.refusal)),
            };
            match step {
                Ok((Verdict::Learned, true, _)) => seated.restore = None, // restored: the same example is learned next
                Ok((Verdict::Learned, false, _)) => {
                    seated.progress.insert(row.id, learned + 1);
                    pass.learned += 1;
                }
                Ok((Verdict::Again, _, refusal)) if refusal == "unknown_grant" && !renewed => {
                    // her grant expired: renew it at this quiescent boundary and try the step again
                    drop(slot);
                    renewed = true;
                    if let Err(why) = seated.mind.renew_grant(seated.expiry).await {
                        continuum_base::probe!(class = "training.in_place.grant_unrenewed", persona = %key.persona_id, reason = %why, "her grant expired and was not renewed; tried again on a later pass");
                        pass.deferred = true;
                        return Ok(Lost::No);
                    }
                }
                Ok((Verdict::Again, _, _)) => {
                    pass.deferred = true;
                    return Ok(Lost::No);
                }
                Ok((Verdict::Quarantined, _, _)) => {
                    drop(slot);
                    stop(held, key).await;
                    pass.quarantined = true;
                    return Ok(Lost::No);
                }
                Err(LearnFailure::Engine(error)) => {
                    // A lost answer is not proof the step ended (it may not have begun): the slot is
                    // held until her shadow's incarnation-bound stop is confirmed, or the engine is
                    // verifiably dead; her volatile progress goes with the shadow.
                    continuum_base::probe!(class = "training.in_place.answer_lost", persona = %key.persona_id, error = %error,
                        "a learning step's answer was lost; her shadow is stopped before the slot is released");
                    pass.deferred = true;
                    let Some(seated) = held.minds.remove(key) else {
                        return Err(format!("the learner seated for {} left before its fence", key.persona_id));
                    };
                    return Ok(Lost::Yes { mind: seated.mind, slot });
                }
                Err(failure) => {
                    continuum_base::probe!(class = "training.in_place.deferred", persona = %key.persona_id, reason = %failure,
                        "a learning step did not run; it is tried again on a later pass");
                    pass.deferred = true;
                    return Ok(Lost::No);
                }
            }
        }
    }
    Ok(Lost::No)
}

/// Publish a checkpoint of what `seated` learned since its newest generation and move the journal's
/// cursor with it. Unconfirmed, the progress stays volatile and the next pass publishes again.
async fn checkpoint(state: &TrainingTriggerState, key: &BucketKey, seated: &mut Seated) -> Result<bool, String> {
    if seated.progress.is_empty() || seated.restore.is_some() {
        return Ok(false);
    }
    let positions: Vec<Position> = seated.progress.iter().map(|(submission, learned)| Position { submission: *submission, learned: *learned }).collect();
    let name = format!("{}-{}-{}.gguf", key.persona_id.simple(), file_safe(&key.trait_kind), Uuid::new_v4().simple());
    let provenance = serde_json::json!({ "personaId": key.persona_id, "traitKind": key.trait_kind, "baseModel": key.base_model, "positions": positions.len() });
    match seated.mind.checkpoint(&name, &seated.binding, provenance.clone()).await {
        Ok(CheckpointAnswer::Saved(generation)) => {
            let store = state.durable.require()?;
            store.commit_checkpoint(key, name, seated.binding.clone(), generation.steps, provenance, &positions).await?;
            for position in &positions {
                let floor = seated.checkpointed.entry(position.submission).or_default();
                *floor = (*floor).max(position.learned);
            }
            seated.progress.clear();
            Ok(true)
        }
        Ok(CheckpointAnswer::NoOptimizerState) => Err(format!("{} learned {} examples but the engine holds no optimizer state for her shadow", key.persona_id, positions.len())),
        Err(error) => {
            continuum_base::probe!(class = "training.in_place.checkpoint_unconfirmed", persona = %key.persona_id, error = %error,
                "a checkpoint was not confirmed; its progress stays volatile and is published on a later pass");
            Ok(false)
        }
    }
}

/// Make sure a learner is seated for `key` in `held`'s engine, restored from the bucket's newest
/// generation when it has one. `false` when it cannot be seated now; the bucket defers.
async fn seat(authority: &Authority, state: &TrainingTriggerState, held: &mut EngineLearning, key: &BucketKey, row: &Arrival, rows: &[Arrival], engine: EngineIncarnation) -> Result<bool, String> {
    if held.minds.contains_key(key) {
        return Ok(true);
    }
    // WHICH gene this competence trains, by the one rule (competence::decide): the bucket's pending
    // examples are the competence; nothing of hers learns it yet (no learner is seated for it).
    let examples: Vec<crate::genome::fine_tuning::types::TrainingExample> = rows.iter().flat_map(|r| r.examples.iter().cloned()).collect();
    let mut signature = super::competence::mint_signature(format!("inline:{}", key.trait_kind), &key.trait_kind, &examples).await;
    if let Some(sig) = signature.as_mut() {
        let asking = super::competence::Asking { persona_id: key.persona_id, persona_name: &row.persona_name, trait_kind: &key.trait_kind, base_model: &key.base_model, examples: examples.len() };
        let adoption = crate::genome::gene_trial::Adoption::default_paths();
        match super::competence::decide(&asking, sig, &adoption, &[]).await {
            Ok(crate::genome::competence::Decision::Mint) => {}
            Ok(crate::genome::competence::Decision::Fork { parent, .. }) => sig.parent = Some(parent), // learned from the base with its lineage, as the trainer it replaces did
            Ok(crate::genome::competence::Decision::Reuse { gene, similarity }) => {
                let _reused_or_refused = super::competence::reuse(&asking, &gene, similarity, &adoption).await; // probed either way; the examples wait for its trial
                return Ok(false);
            }
            Ok(other) => {
                continuum_base::probe!(class = "training.in_place.not_learning", persona = %key.persona_id, decision = ?other,
                    "the competence decision does not learn now (awaiting a trial, or memories suffice); the examples stay pending");
                return Ok(false);
            }
            Err(why) => {
                continuum_base::probe!(class = "training.in_place.undecided", persona = %key.persona_id, reason = %why, "the competence could not be decided; nothing learns blind");
                return Ok(false);
            }
        }
    }
    let policy = match learn_policy(row.lora.as_ref(), row.schedule.as_ref()) {
        Ok(policy) => policy,
        Err(why) => {
            continuum_base::probe!(class = "training.in_place.unlearnable_policy", persona = %key.persona_id, reason = %why,
                "this bucket's policy names nothing the engine can learn; it stays pending");
            return Ok(false);
        }
    };
    let store = state.durable.require()?;
    let generation = store.checkpoint_of(key).await?;
    if let Some(generation) = &generation {
        store.project(generation).await?; // a crash between a commit and its projection is repaired here
    }
    let learning = LearningEngine::of(engine);
    let expiry = match learning.learn_ledger().await {
        Ok(LedgerAnswer::Ledger(ledger)) => std::time::Duration::from_millis(ledger.max_expiry_ms),
        Ok(LedgerAnswer::NoLedger) => return Err("the serving engine has no learning ledger: nothing can learn in place".into()),
        Err(_unreachable) => return Ok(false),
    };
    let id = Uuid::new_v4();
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
            let binding = format!("{}:{}:{}", key.persona_id, key.trait_kind, key.base_model);
            let checkpointed = generation.iter().flat_map(|g| g.positions.iter()).map(|p| (p.submission, p.learned)).collect();
            held.minds.insert(key.clone(), Seated { policy, signature, expiry, mind, binding, restore: generation, progress: HashMap::new(), checkpointed });
            Ok(true)
        }
        Err(StartFailure::NotSeated(why)) => {
            continuum_base::probe!(class = "training.in_place.unseated", persona = %key.persona_id, reason = %why, "her shadow was not seated; tried again on a later pass");
            Ok(false)
        }
        Err(StartFailure::Ungranted { why, seated }) => {
            continuum_base::probe!(class = "training.in_place.ungranted", persona = %key.persona_id, reason = %why, "seated but not granted; stopped, tried again on a later pass");
            let _orphaned = seated.stop().await; // what it holds stays in custody until the engine confirms each release
            Ok(false)
        }
    }
}

/// Stop `key`'s learner; whatever it still holds stays in custody until the engine confirms it.
async fn stop(held: &mut EngineLearning, key: &BucketKey) {
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

/// Every pending submission whose base `served_model` names, grouped by bucket, oldest first. Pages
/// the whole pending journal, so rows for other bases never hide eligible ones behind them.
pub(crate) async fn eligible_buckets(store: &super::durable::DurableStore, served_model: &str) -> Result<Vec<(BucketKey, Vec<Arrival>)>, String> {
    const PAGE: usize = 256;
    let mut buckets: Vec<(BucketKey, Vec<Arrival>)> = Vec::new();
    let mut after = 0u64;
    loop {
        let page = store.arrivals(after, PAGE).await?;
        let last = page.last().map(|row| row.sequence);
        let full = page.len() == PAGE;
        for row in page {
            if !crate::model_registry::artifacts::names_the_same_base(served_model, &row.key.base_model) {
                continue;
            }
            match buckets.iter_mut().find(|(key, _)| *key == row.key) {
                Some((_, rows)) => rows.push(row),
                None => buckets.push((row.key.clone(), vec![row])),
            }
        }
        match (full, last) {
            (true, Some(last)) => after = last,
            _ => return Ok(buckets),
        }
    }
}

/// `trait_kind` as a file-name part (`code/owner` -> `code-owner`).
fn file_safe(text: &str) -> String {
    text.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

impl TrainingTriggerState {
    /// Learn every pending submission whose base `engine` serves (`served_model`), one bucket at a
    /// time, each through the trigger's owned operations. Pages the whole pending journal, so rows
    /// for other bases never hide eligible ones behind them.
    pub(crate) async fn learn_in_place(self: &Arc<Self>, engine: EngineIncarnation, served_model: &str) -> Result<(), String> {
        let store = self.durable.require()?;
        if let Ok(authority) = self.in_place.authority() {
            authority.custody.settle_orphans().await;
        }
        let buckets = eligible_buckets(&store, served_model).await?;
        for (key, rows) in buckets {
            let pass = self.run_owned(key, move |state, key| async move { state.in_place.learn_bucket(&state, &key, rows, engine).await }).await??;
            if pass.learned > 0 || pass.quarantined || pass.checkpointed {
                continuum_base::probe!(class = "training.in_place.pass", learned = u64::from(pass.learned), quarantined = pass.quarantined,
                    checkpointed = pass.checkpointed, deferred = pass.deferred, "accepted examples learned in place");
            }
        }
        Ok(())
    }

    /// Run `operation` as one of the trigger's owned operations, outliving the caller; shutdown
    /// drains it like every other.
    async fn spawn_owned(&self, operation: impl std::future::Future<Output = ()> + Send + 'static) {
        self.operations.lock().await.spawn(operation);
    }
}

#[cfg(test)]
mod tests;
