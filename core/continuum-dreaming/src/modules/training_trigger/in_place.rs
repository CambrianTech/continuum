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
    // a non-finite loss is a poisoned update: never counted, never checkpointed, so every generation
    // holds only finite steps (Codex on 5f26871bc)
    if outcome.effects_uncertain || !outcome.loss.is_finite() {
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
pub(crate) struct InPlace {
    /// The authorities learning answers to, handed in by the core that built the trigger.
    authority: Authority,
    engines: dashmap::DashMap<EngineIncarnation, Arc<tokio::sync::Mutex<EngineLearning>>>,
}

/// One engine's learning: its seated learners and its one workspace.
#[derive(Default)]
struct EngineLearning {
    minds: HashMap<BucketKey, Seated>,
    workspace: Option<EngineWorkspace>,
}

struct Seated {
    /// This seat's identity, as her other competences see it in flight, and the id her shadow's
    /// file is named for.
    id: Uuid,
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
    /// Her name, for the provenance her candidates carry.
    persona_name: String,
    /// What this seating has cost so far, reported when she pages out.
    cost: SeatCost,
}

/// Where one seated life's time went: what paging her in and out costs against what she learned
/// in it, the receipt that decides whether paging per step is worth engine verbs (card de1a9e5a).
#[derive(Debug, Default, Clone, Copy)]
struct SeatCost {
    /// Seating: her competence decision, her shadow's memory and its load.
    seat: std::time::Duration,
    /// Restoring her generation onto the fresh shadow.
    restore: std::time::Duration,
    /// Her learning steps' requests, how many were sent, and how many the engine committed.
    learn: std::time::Duration,
    attempts: u32,
    learned: u32,
}

/// Whether the engine confirmed a release the trigger asked of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Released {
    Confirmed,
    /// Unconfirmed: custody keeps what it held until the engine releases it.
    Unconfirmed,
    /// Not this learner's to release (the engine's workspace while another learner holds it).
    NotHers,
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
    pub(crate) fn new(authority: Authority) -> Self {
        Self { authority, engines: Default::default() }
    }

    fn engine(&self, engine: EngineIncarnation) -> Arc<tokio::sync::Mutex<EngineLearning>> {
        Arc::clone(self.engines.entry(engine).or_default().value())
    }

    /// One step of bucket `key`, holding its gate, its engine and `slot`: seat her learner if needed,
    /// then learn her next pending example on `slot` (or restore her generation before it). A lost
    /// answer's fence runs on its own, holding this engine and the slot until the stop is confirmed.
    async fn step(&self, state: &Arc<TrainingTriggerState>, mut held: tokio::sync::OwnedMutexGuard<EngineLearning>, key: &BucketKey, rows: &[Arrival], engine: EngineIncarnation, slot: LearnSlot, renewed: &mut bool, pass: &mut BucketPass) -> Result<Next, String> {
        // Bound first: the step's borrow of `held` ends here, before the arm below moves `held` into the fence.
        let stepped = step_on(&self.authority, &mut held, key, rows, slot, renewed, pass).await?;
        match stepped {
            Stepped::Next(next) => Ok(next),
            Stepped::Lost { mind, slot } => {
                state.spawn_owned(async move {
                    fence(mind, engine).await;
                    drop(slot);
                    drop(held);
                })
                .await;
                Ok(Next::Fenced)
            }
        }
    }

    /// Seat `key`'s learner if needed and cover her next pending example's memory, waiting for this
    /// engine (holding no serving resource): the step that follows only revalidates. Seating's
    /// embeddings, competence decision and leases all happen here, never under her serving guards.
    async fn prepare(&self, state: &TrainingTriggerState, key: &BucketKey, rows: &[Arrival], engine: EngineIncarnation) -> Result<Prepared, String> {
        let mut held = self.engine(engine).lock_owned().await;
        for row in rows {
            let total = u32::try_from(row.examples.len()).map_err(|_| format!("submission {} holds too many examples to count", row.id))?;
            match seat(&self.authority, state, &mut held, key, row, rows, engine).await? {
                Seating::Seated => {}
                Seating::NotNow => return Ok(Prepared::Deferred),
                Seating::AwaitMemory(wait) => return Ok(Prepared::MemoryShort(wait)),
            }
            let EngineLearning { minds, workspace } = &mut *held;
            let Some(seated) = minds.get_mut(key) else {
                return Err(format!("the learner seated for {} left before its cover", key.persona_id));
            };
            let learned = start(row.learned, seated.checkpointed.get(&row.id).copied(), seated.progress.get(&row.id).copied());
            if learned >= total {
                continue;
            }
            let framed = learn_messages(&row.examples[learned as usize]);
            return match seated.mind.cover(&self.authority, workspace, &framed.messages, framed.tools.as_ref()).await {
                Ok(()) => Ok(Prepared::Ready),
                Err(LearnFailure::Uncovered(crate::genome::learner::seat::SeatRefusal::AwaitingMemory(wait))) => Ok(Prepared::MemoryShort(wait)),
                Err(failure) => {
                    continuum_base::probe!(class = "training.in_place.deferred", persona = %key.persona_id, reason = %failure,
                        "her next example's memory was not covered; it is tried again on a later pass");
                    Ok(Prepared::Deferred)
                }
            };
        }
        Ok(Prepared::Nothing)
    }

    /// The end of `key`'s pass: publish what her seated learner learned since its newest
    /// generation, then page her out (Joel 2026-11-11: "Why would you grab what you aren't actively
    /// using"). Her shadow unloads, which frees its optimizer, her leases release, and the engine's
    /// workspace releases with its last learner; her next pass seats her from the durable generation
    /// and restores it. Progress a checkpoint did not confirm keeps her seated, so the next pass
    /// publishes it instead of learning it again. `true` when a generation was published.
    async fn settle(&self, state: &TrainingTriggerState, key: &BucketKey, engine: EngineIncarnation) -> Result<bool, String> {
        let mut held = self.engine(engine).lock_owned().await;
        let Some(seated) = held.minds.get_mut(key) else {
            return Ok(false);
        };
        let published = checkpoint(state, key, seated).await?.is_some();
        if !seated.progress.is_empty() {
            continuum_base::probe!(class = "training.in_place.kept_seated", persona = %key.persona_id,
                "her progress is not yet in a confirmed generation; she stays seated and the next pass publishes it");
            return Ok(published);
        }
        let cost = seated.cost;
        let began = std::time::Instant::now();
        let stopped = stop(&mut held, key).await;
        // the workspace is the engine's: it goes with the last learner, and only then
        let workspace = match (held.minds.is_empty(), held.workspace.take()) {
            (true, Some(workspace)) => match workspace.release().await {
                Ok(()) => Released::Confirmed,
                Err((kept, why)) => {
                    held.workspace = Some(kept);
                    continuum_base::probe!(class = "training.in_place.workspace_unconfirmed", reason = %why,
                        "the engine's learning workspace release is unconfirmed; it stays held and the next settle tries again");
                    Released::Unconfirmed
                }
            },
            (_, kept) => {
                held.workspace = kept;
                Released::NotHers
            }
        };
        let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
        match paged_out(stopped, workspace) {
            true => continuum_base::probe!(class = "training.in_place.paged_out",
                persona = %key.persona_id, seat_ms = ms(cost.seat), restore_ms = ms(cost.restore), learn_ms = ms(cost.learn),
                attempts = u64::from(cost.attempts), learned = u64::from(cost.learned), page_out_ms = ms(began.elapsed()),
                workspace = ?workspace, "her learner paged out at the end of her pass, confirmed: what seating, restoring and paging out cost against what she learned"),
            false => continuum_base::probe!(class = "training.in_place.page_out_unconfirmed", persona = %key.persona_id, stop = ?stopped,
                workspace = ?workspace, page_out_ms = ms(began.elapsed()),
                "her page-out was attempted and not confirmed; custody holds what the engine did not release, and no cost receipt is taken from it"),
        }
        Ok(published)
    }

    /// How many learners `engine` holds seated: none between passes.
    #[cfg(test)]
    pub(crate) async fn seated_on(&self, engine: EngineIncarnation) -> usize {
        self.engine(engine).lock().await.minds.len()
    }
}

/// What a bucket does after one step.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Next {
    /// Learned or restored: her next example is admitted next.
    Again,
    /// Deferred, quarantined or nothing pending: the bucket's pass ends here.
    Stop,
    /// The answer was lost: her learner is gone and the fence holds the engine until its stop is
    /// confirmed, so nothing is published.
    Fenced,
    /// The step's memory was not free on this attempt: everything was let go and the pass ends.
    MemoryShort(crate::inference::training_admission::TrainingWait),
}

/// Where a bucket stands before its next step, decided holding no serving resource.
enum Prepared {
    /// Seated, her next example's memory covered: admit the step.
    Ready,
    /// Nothing pending for her learner.
    Nothing,
    /// Not now (not seated for a reason that is not memory, or the cover failed): the pass ends.
    Deferred,
    /// Her shadow's or her step's memory is not free: the pass ends holding nothing.
    MemoryShort(crate::inference::training_admission::TrainingWait),
}

/// What one step left: the bucket's next move, or a lost answer the fence must hold.
enum Stepped {
    Next(Next),
    Lost { mind: MindLearner, slot: LearnSlot },
}

/// One step of `key` on `slot`, under its gate and engine.
#[allow(clippy::too_many_arguments)] // the step's whole custody, each owned by its caller
async fn step_on(
    authority: &Authority,
    held: &mut EngineLearning,
    key: &BucketKey,
    rows: &[Arrival],
    mut slot: LearnSlot,
    renewed: &mut bool,
    pass: &mut BucketPass,
) -> Result<Stepped, String> {
    for row in rows {
        let total = u32::try_from(row.examples.len()).map_err(|_| format!("submission {} holds too many examples to count", row.id))?;
        let EngineLearning { minds, workspace } = held;
        let Some(seated) = minds.get_mut(key) else {
            // stopped since it was prepared: prepare again, holding nothing (seating waits on
            // embeddings and memory, never under her serving resources)
            return Ok(Stepped::Next(Next::Again));
        };
        let learned = start(row.learned, seated.checkpointed.get(&row.id).copied(), seated.progress.get(&row.id).copied());
        if learned >= total {
            continue;
        }
        let framed = learn_messages(&row.examples[learned as usize]);
        let rate = seated.policy.learning_rate;
        // revalidate its memory under these owners (the engine's shared workspace may have moved),
        // one attempt: short, everything is let go and the pass ends holding nothing
        match seated.mind.cover(authority, workspace, &framed.messages, framed.tools.as_ref()).await {
            Ok(()) => {}
            Err(LearnFailure::Uncovered(crate::genome::learner::seat::SeatRefusal::AwaitingMemory(wait))) => {
                return Ok(Stepped::Next(Next::MemoryShort(wait)));
            }
            Err(failure) => {
                continuum_base::probe!(class = "training.in_place.deferred", persona = %key.persona_id, reason = %failure,
                    "a learning step's memory was not covered; it is tried again on a later pass");
                pass.deferred = true;
                return Ok(Stepped::Next(Next::Stop));
            }
        }
        // the POST is about to be sent: from here the slot holds whatever the engine leaves
        slot.dispatching();
        let began = std::time::Instant::now();
        let restoring = seated.restore.is_some();
        let step = match seated.restore.as_ref() {
            Some(generation) => {
                let from = RestoreCheckpoint { name: generation.name.clone(), binding: generation.binding.clone(), steps: generation.steps };
                // a restore proves nothing about her served tail: her residency is forgotten
                seated.mind.restore(authority, workspace, framed.messages, framed.tools, slot.slot(), rate, &from).await.map(|outcome| (restored(&outcome), true, outcome.refusal))
            }
            None => seated.mind.learn(authority, workspace, framed.messages, framed.tools, slot.slot(), rate).await.map(|outcome| {
                slot.answered(&outcome);
                (verdict(&outcome), false, outcome.refusal)
            }),
        };
        if restoring {
            seated.cost.restore += began.elapsed();
        } else {
            seated.cost.learn += began.elapsed();
            seated.cost.attempts += 1;
        }
        return match step {
            Ok((Verdict::Learned, true, _)) => {
                seated.restore = None; // restored: the same example is learned next
                Ok(Stepped::Next(Next::Again))
            }
            Ok((Verdict::Learned, false, _)) => {
                seated.cost.learned += 1;
                seated.progress.insert(row.id, learned + 1);
                pass.learned += 1;
                Ok(Stepped::Next(Next::Again))
            }
            Ok((Verdict::Again, _, refusal)) if refusal == "unknown_grant" && !*renewed => {
                // her grant expired: renew it at this quiescent boundary and try the step again
                drop(slot);
                *renewed = true;
                if let Err(why) = seated.mind.renew_grant(seated.expiry).await {
                    continuum_base::probe!(class = "training.in_place.grant_unrenewed", persona = %key.persona_id, reason = %why, "her grant expired and was not renewed; tried again on a later pass");
                    pass.deferred = true;
                    return Ok(Stepped::Next(Next::Stop));
                }
                Ok(Stepped::Next(Next::Again))
            }
            Ok((Verdict::Again, _, _)) => {
                pass.deferred = true;
                Ok(Stepped::Next(Next::Stop))
            }
            Ok((Verdict::Quarantined, _, _)) => {
                drop(slot);
                stop(held, key).await;
                pass.quarantined = true;
                Ok(Stepped::Next(Next::Stop))
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
                Ok(Stepped::Lost { mind: seated.mind, slot })
            }
            Err(failure) => {
                continuum_base::probe!(class = "training.in_place.deferred", persona = %key.persona_id, reason = %failure,
                    "a learning step did not run; it is tried again on a later pass");
                pass.deferred = true;
                Ok(Stepped::Next(Next::Stop))
            }
        };
    }
    Ok(Stepped::Next(Next::Stop)) // nothing pending
}

/// Publish a checkpoint of what `seated` learned since its newest generation and move the journal's
/// cursor with it. Unconfirmed, the progress stays volatile and the next pass publishes again.
async fn checkpoint(state: &TrainingTriggerState, key: &BucketKey, seated: &mut Seated) -> Result<Option<String>, String> {
    if seated.progress.is_empty() || seated.restore.is_some() {
        return Ok(None);
    }
    let positions: Vec<Position> = seated.progress.iter().map(|(submission, learned)| Position { submission: *submission, learned: *learned }).collect();
    let name = format!("{}-{}.gguf", bucket_stem(key), Uuid::new_v4().simple());
    // the manifest records what the generation consumed (ONE-RESIDENT 9.2): each submission and how
    // many of its examples, merged over the generations before it
    let mut consumed: HashMap<Uuid, u32> = seated.checkpointed.clone();
    for position in &positions {
        let held = consumed.entry(position.submission).or_default();
        *held = (*held).max(position.learned);
    }
    let consumed: Vec<Position> = consumed.into_iter().map(|(submission, learned)| Position { submission, learned }).collect();
    let provenance = serde_json::json!({
        "personaId": key.persona_id, "traitKind": key.trait_kind, "baseModel": key.base_model,
        "consumed": consumed, "signature": seated.signature.as_ref().map(|sig| sig.corpus.content_hash.clone()),
    });
    match seated.mind.checkpoint(&name, &seated.binding, provenance.clone()).await {
        Ok(CheckpointAnswer::Saved(generation)) => {
            let store = state.durable.require()?;
            let published = super::durable::Generation {
                name,
                path: generation.path.clone(),
                binding: seated.binding.clone(),
                steps: generation.steps,
                provenance,
                persona_name: seated.persona_name.clone(),
                signature: seated.signature.clone(),
            };
            store.commit_checkpoint(key, published, &positions).await?;
            for position in &positions {
                let floor = seated.checkpointed.entry(position.submission).or_default();
                *floor = (*floor).max(position.learned);
            }
            seated.progress.clear();
            Ok(Some(generation.path))
        }
        Ok(CheckpointAnswer::NoOptimizerState) => Err(format!("{} learned {} examples but the engine holds no optimizer state for her shadow", key.persona_id, positions.len())),
        Err(error) => {
            continuum_base::probe!(class = "training.in_place.checkpoint_unconfirmed", persona = %key.persona_id, error = %error,
                "a checkpoint was not confirmed; its progress stays volatile and is published on a later pass");
            Ok(None)
        }
    }
}

/// Whether her learner is seated.
enum Seating {
    Seated,
    /// Not now (the competence does not learn, the policy is unlearnable, the engine is not
    /// reachable): the bucket's pass ends, the examples stay pending.
    NotNow,
    /// Her shadow's memory is not free on this attempt; nothing is held.
    AwaitMemory(crate::inference::training_admission::TrainingWait),
}

/// Make sure a learner is seated for `key` in `held`'s engine, restored from the bucket's newest
/// generation when it has one. `false` when it cannot be seated now; the bucket defers.
async fn seat(authority: &Authority, state: &TrainingTriggerState, held: &mut EngineLearning, key: &BucketKey, row: &Arrival, rows: &[Arrival], engine: EngineIncarnation) -> Result<Seating, String> {
    if held.minds.contains_key(key) {
        return Ok(Seating::Seated);
    }
    let began = std::time::Instant::now();
    // WHICH gene this competence trains, by the one rule (competence::decide): the bucket's pending
    // examples are the competence; nothing of hers learns it yet (no learner is seated for it).
    let examples: Vec<crate::genome::fine_tuning::types::TrainingExample> = rows.iter().flat_map(|r| r.examples.iter().cloned()).collect();
    let mut signature = super::competence::mint_signature(format!("inline:{}", key.trait_kind), &key.trait_kind, &examples).await;
    if let Some(sig) = signature.as_mut() {
        let asking = super::competence::Asking { persona_id: key.persona_id, persona_name: &row.persona_name, trait_kind: &key.trait_kind, base_model: &key.base_model, examples: examples.len() };
        let adoption = crate::genome::gene_trial::Adoption::default_paths();
        // her other learners seated on this base are her competences in flight: a near one joins it
        // rather than minting a second gene beside it (Codex on e473f72b1)
        let in_flight = in_flight(key, held.minds.iter().map(|(other, seated)| (other, seated.id, seated.signature.as_ref())));
        match super::competence::decide(&asking, sig, &adoption, &in_flight).await {
            Ok(crate::genome::competence::Decision::Mint) => {}
            Ok(crate::genome::competence::Decision::Fork { parent, .. }) => sig.parent = Some(parent), // learned from the base with its lineage, as the trainer it replaces did
            Ok(crate::genome::competence::Decision::Reuse { gene, similarity }) => {
                super::competence::reuse(&asking, &gene, similarity, &adoption).await; // probed either way; the examples wait for its trial
                return Ok(Seating::NotNow);
            }
            Ok(other) => {
                continuum_base::probe!(class = "training.in_place.not_learning", persona = %key.persona_id, decision = ?other,
                    "the competence decision does not learn now (another of her learners has it, a trial awaits, or memories suffice); the examples stay pending");
                return Ok(Seating::NotNow);
            }
            Err(why) => {
                continuum_base::probe!(class = "training.in_place.undecided", persona = %key.persona_id, reason = %why, "the competence could not be decided; nothing learns blind");
                return Ok(Seating::NotNow);
            }
        }
    }
    let policy = match learn_policy(row.lora.as_ref(), row.schedule.as_ref()) {
        Ok(policy) => policy,
        Err(why) => {
            continuum_base::probe!(class = "training.in_place.unlearnable_policy", persona = %key.persona_id, reason = %why,
                "this bucket's policy names nothing the engine can learn; it stays pending");
            return Ok(Seating::NotNow);
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
        Err(_unreachable) => return Ok(Seating::NotNow),
    };
    let id = Uuid::new_v4();
    let shadow = FreshShadow {
        name: format!("{}-{}.gguf", bucket_stem(key), id.simple()),
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
            held.minds.insert(
                key.clone(),
                Seated {
                    id, // the shadow's own: her file in the train dir is named for this seat
                    policy,
                    signature,
                    expiry,
                    mind,
                    binding,
                    restore: generation,
                    progress: HashMap::new(),
                    checkpointed,
                    persona_name: row.persona_name.clone(),
                    cost: SeatCost { seat: began.elapsed(), ..SeatCost::default() },
                },
            );
            Ok(Seating::Seated)
        }
        Err(StartFailure::NotSeated(crate::genome::learner::seat::SeatRefusal::AwaitingMemory(wait))) => Ok(Seating::AwaitMemory(wait)),
        Err(StartFailure::NotSeated(why)) => {
            continuum_base::probe!(class = "training.in_place.unseated", persona = %key.persona_id, reason = %why, "her shadow was not seated; tried again on a later pass");
            Ok(Seating::NotNow)
        }
        Err(StartFailure::Ungranted { why, seated }) => {
            continuum_base::probe!(class = "training.in_place.ungranted", persona = %key.persona_id, reason = %why, "seated but not granted; stopped, tried again on a later pass");
            let _orphaned = seated.stop().await; // what it holds stays in custody until the engine confirms each release
            Ok(Seating::NotNow)
        }
    }
}

/// What a shutdown left: each learner checkpointed and stopped, each engine's workspace released.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Quiesced {
    pub(crate) stopped: u32,
    /// Learners whose last progress could not be published: it is learned again from the journal.
    pub(crate) unpublished: u32,
    /// Stops and workspace releases the engine did not confirm: custody holds what they held.
    pub(crate) unconfirmed: u32,
    /// Engines a step held, left to it (`Busy::Skip`).
    pub(crate) busy: u32,
}

/// What a quiesce does with an engine a learning step holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Busy {
    /// Wait for the step (the join, after every operation has ended).
    Wait,
    /// Leave it to the step and count it (the save, while steps may still run).
    Skip,
}

impl InPlace {
    /// Shutdown, after every owned operation has drained: publish what each seated learner learned
    /// since its newest generation, stop it, and release each engine's workspace. Nothing is left
    /// seated in the trigger; what an engine did not confirm stays in custody.
    pub(crate) async fn quiesce(&self, state: &TrainingTriggerState, busy: Busy) -> Quiesced {
        let mut quiesced = Quiesced::default();
        let engines: Vec<_> = self.engines.iter().map(|entry| (*entry.key(), Arc::clone(entry.value()))).collect();
        for (incarnation, engine) in engines {
            let mut held = match busy {
                Busy::Wait => engine.lock_owned().await,
                Busy::Skip => match engine.try_lock_owned() {
                    Ok(held) => held,
                    Err(_) => {
                        quiesced.busy += 1; // a step holds it: its learner pages out when her pass ends
                        continue;
                    }
                },
            };
            self.engines.remove(&incarnation);
            let keys: Vec<BucketKey> = held.minds.keys().cloned().collect();
            for key in keys {
                if let Some(seated) = held.minds.get_mut(&key) {
                    if let Err(why) = checkpoint(state, &key, seated).await {
                        quiesced.unpublished += 1;
                        continuum_base::probe!(class = "training.in_place.shutdown_unpublished", persona = %key.persona_id, reason = %why,
                            "a learner's progress since its newest generation was not published at shutdown; the journal still holds it");
                    }
                }
                if let Some(seated) = held.minds.remove(&key) {
                    match seated.mind.stop().await {
                        Ok(()) => quiesced.stopped += 1,
                        Err((_orphaned, why)) => {
                            quiesced.unconfirmed += 1;
                            continuum_base::probe!(class = "training.in_place.stop_unconfirmed", persona = %key.persona_id, reason = %why,
                                "a learner's stop at shutdown is unconfirmed; custody holds what it held");
                        }
                    }
                }
            }
            if let Some(workspace) = held.workspace.take() {
                if let Err((_kept, why)) = workspace.release().await {
                    quiesced.unconfirmed += 1;
                    continuum_base::probe!(class = "training.in_place.workspace_unconfirmed", reason = %why,
                        "an engine's learning workspace release at shutdown is unconfirmed; custody holds it");
                }
            }
        }
        quiesced
    }
}

impl InPlace {
    /// Delete `key`'s generations older than `current` from its train dir: nothing reads them again.
    /// Run under the bucket's gate; every outcome is probed.
    async fn evict(&self, key: &BucketKey, current: &LearnCheckpoint) {
        let stem = bucket_stem(key);
        let Some(newest) = evict::named_id(&stem, &current.name) else {
            continuum_base::probe!(class = "training.in_place.evict_unparsed", persona = %key.persona_id, name = %current.name,
                "the bucket's generation name is not its own pattern; nothing of it is evicted");
            return;
        };
        let Some(dir) = std::path::Path::new(&current.path).parent().map(std::path::Path::to_path_buf) else {
            return; // a generation path with no directory names no train dir
        };
        let swept = tokio::task::spawn_blocking(move || evict::sweep(&dir, &stem, newest)).await;
        match swept {
            Ok(Ok(0)) => {}
            Ok(Ok(deleted)) => continuum_base::probe!(class = "training.in_place.evicted", persona = %key.persona_id, deleted = deleted as u64,
                "a bucket's superseded generations left the train dir"),
            Ok(Err(why)) => continuum_base::probe!(class = "training.in_place.evict_failed", persona = %key.persona_id, reason = %why,
                "a bucket's old files could not all be deleted; the next sweep tries again"),
            Err(join) => continuum_base::probe!(class = "training.in_place.evict_failed", persona = %key.persona_id, reason = %join,
                "a bucket's eviction task ended without an answer; the next sweep tries again"),
        }
    }
}

/// Her learning memory was not free: the pass ends holding nothing, and the next pass (the tick or a
/// serving change) asks again; a wait would hold her pass on memory measured for one moment.
fn memory_short(key: &BucketKey, wait: &crate::inference::training_admission::TrainingWait) {
    continuum_base::probe!(class = "training.in_place.memory_short", persona = %key.persona_id, wait = %wait,
        "her learning memory was not free; the pass ends holding nothing and the next pass asks again");
}

/// PURE: whether a page-out is confirmed, so its cost receipt may be taken: her stop confirmed, and
/// the workspace released with her or not hers to release. A failure is never success evidence.
fn paged_out(stopped: Released, workspace: Released) -> bool {
    matches!((stopped, workspace), (Released::Confirmed, Released::Confirmed | Released::NotHers))
}

/// Stop `key`'s learner; whatever it still holds stays in custody until the engine confirms it.
async fn stop(held: &mut EngineLearning, key: &BucketKey) -> Released {
    let Some(seated) = held.minds.remove(key) else {
        return Released::NotHers;
    };
    match seated.mind.stop().await {
        Ok(()) => Released::Confirmed,
        Err((_orphaned, why)) => {
            continuum_base::probe!(class = "training.in_place.stop_unconfirmed", persona = %key.persona_id, reason = %why,
                "a learner's stop is unconfirmed; custody holds what it held until the engine releases it");
            Released::Unconfirmed
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

/// PURE: the competences of `key`'s persona already learning on `key`'s base, from the seated
/// learners `seats` (bucket, seat id, signature): her others on this base, never this bucket, another
/// persona's, or another base's; a learner without a signature is not comparable and is left out.
pub(crate) fn in_flight<'a, S: Clone + 'a>(key: &BucketKey, seats: impl Iterator<Item = (&'a BucketKey, Uuid, Option<&'a S>)>) -> Vec<(Uuid, S)> {
    seats
        .filter(|(other, _, _)| *other != key && other.persona_id == key.persona_id && other.base_model == key.base_model)
        .filter_map(|(_, id, signature)| signature.cloned().map(|signature| (id, signature)))
        .collect()
}

/// PURE: the stem every file of bucket `key` in the train dir is named with: her id (legible) and a
/// digest of the WHOLE key (persona, trait, base), so two buckets never share one, whatever their
/// traits sanitize to or whichever base they learn on (Codex on f13a6abea).
pub(crate) fn bucket_stem(key: &BucketKey) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::new()
        .chain_update(key.persona_id.as_bytes())
        .chain_update([0])
        .chain_update(key.trait_kind.as_bytes())
        .chain_update([0])
        .chain_update(key.base_model.as_bytes())
        .finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect(); // 146-byte generation names, inside the engine's 200-byte confinement
    format!("{}-{hex}", key.persona_id.simple())
}

impl TrainingTriggerState {
    /// Learn every pending submission whose base `engine` serves (`served_model`), one bucket at a
    /// time, each through the trigger's owned operations. Pages the whole pending journal, so rows
    /// for other bases never hide eligible ones behind them.
    pub(crate) async fn learn_in_place(self: &Arc<Self>, engine: EngineIncarnation, served_model: &str) -> Result<(), String> {
        let store = self.durable.require()?;
        self.in_place.authority.custody.settle_orphans().await;
        let buckets = eligible_buckets(&store, served_model).await?;
        // learning and candidates progress together: a bucket waiting (for her act, her memory, her
        // slot) never holds a ready candidate from its trial (Codex on #5905). Both are joined here,
        // in this pass; each candidate is taken under its own bucket's gate.
        let served = served_model.to_owned();
        let learning = self.learn_each(buckets, move |state, key, rows| {
            let served = served.clone();
            async move {
                let pass = state.learn_bucket(key.clone(), rows, engine).await?;
                if pass.checkpointed {
                    // a generation published in this pass is offered as a candidate now, not a pass later
                    state.sweep_bucket(&key, &served).await?;
                }
                Ok(pass)
            }
        });
        let (learned, swept) = tokio::join!(learning, self.sweep_all(&store, served_model));
        learned?;
        swept
    }

    /// `key`'s newest generation, offered as a candidate the way the sweep offers every bucket's.
    async fn sweep_bucket(self: &Arc<Self>, key: &BucketKey, served_model: &str) -> Result<(), String> {
        let store = self.durable.require()?;
        match store.checkpoint_of(key).await? {
            Some(current) => self.sweep_candidates(vec![current], served_model).await,
            None => Ok(()),
        }
    }

    /// Every bucket's newest generation is candidate work of its own, retried until taken: a
    /// candidate interrupted after its generation, or a bucket with nothing pending, is not lost.
    async fn sweep_all(self: &Arc<Self>, store: &super::durable::DurableStore, served_model: &str) -> Result<(), String> {
        const PAGE: usize = 64;
        let mut after: Option<String> = None;
        loop {
            let page = store.checkpoints(after.as_deref(), PAGE).await?;
            let full = page.len() == PAGE;
            after = page.last().map(|row| row.cursor().to_owned());
            self.sweep_candidates(page, served_model).await?;
            if !full {
                return Ok(());
            }
        }
    }

    /// Run `step` on each bucket under its owned gate, in order. The owner refusing (shutdown) ends
    /// the pass; one bucket's own failure never does: it is probed, its durable work stays, and the
    /// next bucket runs (Codex on 086d60123: a persistent bucket error starved every later bucket).
    /// Every bucket's pass at once, joined here: one bucket waiting for her act or her slot never
    /// holds another's. A bucket's failure is its own; the others' passes still run.
    pub(crate) async fn learn_each<F, Fut>(self: &Arc<Self>, buckets: Vec<(BucketKey, Vec<Arrival>)>, step: F) -> Result<(), String>
    where
        F: Fn(Arc<Self>, BucketKey, Vec<Arrival>) -> Fut + Clone + Send + 'static,
        Fut: std::future::Future<Output = Result<BucketPass, String>> + Send + 'static,
    {
        let passes = buckets.into_iter().map(|(key, rows)| {
            let persona = key.persona_id;
            let pass = step.clone()(Arc::clone(self), key, rows);
            async move { (persona, pass.await) }
        });
        for (persona, pass) in futures::future::join_all(passes).await {
            match pass {
                Ok(pass) if pass.learned > 0 || pass.quarantined || pass.checkpointed => {
                    continuum_base::probe!(class = "training.in_place.pass", learned = u64::from(pass.learned), quarantined = pass.quarantined,
                        checkpointed = pass.checkpointed, deferred = pass.deferred, "accepted examples learned in place");
                }
                Ok(_) => {}
                Err(why) => continuum_base::probe!(class = "training.in_place.bucket_failed", persona = %persona, reason = %why,
                    "one bucket's pass failed; its durable work stays, the other buckets' passes ran and the next pass tries it again"),
            }
        }
        Ok(())
    }

    /// Learn bucket `key`'s pending `rows` on `engine` until it stops, then publish what it learned.
    /// Each step: (1) her acts finish, holding nothing; (2) her learner is seated and the example's
    /// memory covered under the bucket's gate and the engine, holding no serving resource, every
    /// memory lease one attempt, a shortfall ending the pass holding nothing; (3) her slot is admitted as
    /// a keyed dream (lane, permit, page); (4) the gate and the engine are only tried, and on
    /// contention everything is let go and waited for holding no serving resource; (5) the step
    /// revalidates its memory, marks custody and posts, inside the owned operations (Codex on B).
    /// The pass ends by settling: publish, then page her out (`settle`).
    pub(crate) async fn learn_bucket(self: &Arc<Self>, key: BucketKey, rows: Vec<Arrival>, engine: EngineIncarnation) -> Result<BucketPass, String> {
        let mut pass = BucketPass::default();
        if rows.iter().all(|row| row.learned as usize >= row.examples.len()) {
            return Ok(pass); // nothing pending: no slot is taken for nothing
        }
        let rows = Arc::new(rows);
        let mut renewed = false;
        loop {
            // 1. her acts first, holding nothing
            if let Err(why) = crate::inference::slots::her_acts_done(key.persona_id).await {
                continuum_base::probe!(class = "training.in_place.step_deferred", persona = %key.persona_id, reason = %why,
                    "her act gate could not be waited on; the examples stay pending");
                pass.deferred = true;
                break;
            }
            // 2. seat and cover her next example, holding no serving resource
            let prepare_rows = Arc::clone(&rows);
            let prepared = self
                .run_owned(key.clone(), move |state, key| async move { state.in_place.prepare(&state, &key, &prepare_rows, engine).await })
                .await??;
            match prepared {
                Prepared::Ready => {}
                Prepared::Nothing => break,
                Prepared::Deferred => {
                    pass.deferred = true;
                    break;
                }
                Prepared::MemoryShort(wait) => {
                    memory_short(&key, &wait);
                    pass.deferred = true;
                    break;
                }
            }
            // 3. her slot as a keyed dream (an act begun since step 1 refuses it: back to step 1)
            let slot = match crate::inference::slots::learn_slot(engine, key.persona_id).await {
                Ok(slot) => slot,
                Err(crate::inference::slots::LearnSlotRefusal::Busy) => continue, // her act began after the wait: it comes first
                Err(refusal) => {
                    // every deferral says why: a step that never runs is a miss, never a quiet slower path
                    continuum_base::probe!(class = "training.in_place.step_deferred", persona = %key.persona_id, reason = %refusal,
                        "her learning step was not given a slot; the examples stay pending");
                    pass.deferred = true;
                    break;
                }
            };
            let Some(lease) = self.submit_gates.try_acquire(&key) else {
                drop(slot);
                drop(self.submit_gates.acquire(&key).await);
                continue;
            };
            let engine_learning = self.in_place.engine(engine);
            let Ok(held) = Arc::clone(&engine_learning).try_lock_owned() else {
                drop((slot, lease));
                drop(engine_learning.lock_owned().await);
                continue;
            };
            let step_rows = Arc::clone(&rows);
            let (next, stepped, renewed_now) = self
                .run_leased(lease, key.clone(), move |state, key| async move {
                    let (mut stepped, mut renewed) = (BucketPass::default(), renewed);
                    let next = state.in_place.step(&state, held, &key, &step_rows, engine, slot, &mut renewed, &mut stepped).await;
                    (next, stepped, renewed)
                })
                .await?;
            renewed = renewed_now;
            pass.learned += stepped.learned;
            pass.deferred |= stepped.deferred;
            pass.quarantined |= stepped.quarantined;
            match next? {
                Next::Again => continue,
                Next::Stop => break,
                Next::Fenced => return Ok(pass),
                Next::MemoryShort(wait) => {
                    memory_short(&key, &wait);
                    pass.deferred = true;
                    break;
                }
            }
        }
        pass.checkpointed = self.run_owned(key, move |state, key| async move { state.in_place.settle(&state, &key, engine).await }).await??;
        Ok(pass)
    }

    /// Take each due candidate in `page` whose base `served_model` serves, under its bucket's gate.
    async fn sweep_candidates(self: &Arc<Self>, page: Vec<super::durable::LearnCheckpoint>, served_model: &str) -> Result<(), String> {
        for generation in page {
            if !crate::model_registry::artifacts::names_the_same_base(served_model, &generation.base_model) {
                continue;
            }
            // under the bucket's owned gate, so it is ordered with that bucket's learning
            let key = BucketKey { persona_id: generation.persona_id, trait_kind: generation.trait_kind.clone(), base_model: generation.base_model.clone() };
            let persona = key.persona_id;
            let swept = self
                .run_owned(key, move |state, key| async move {
                    let store = state.durable.require()?;
                    if let Some(current) = store.checkpoint_of(&key).await? {
                        snapshot::maybe(&store, &current).await;
                        state.in_place.evict(&key, &current).await;
                    }
                    Ok::<(), String>(())
                })
                .await?;
            if let Err(why) = swept {
                continuum_base::probe!(class = "training.in_place.candidate_sweep_failed", persona = %persona, reason = %why,
                    "one bucket's candidate could not be read; the sweep goes on and the next pass takes it again");
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

mod evict;
mod snapshot;

/// Whether `generation` is a due candidate (its state on the durable row).
#[cfg(test)]
pub(crate) fn snapshot_due(generation: &super::durable::LearnCheckpoint) -> bool {
    snapshot::due(generation.learned(), generation.candidate_at)
}

#[cfg(test)]
mod tests;
