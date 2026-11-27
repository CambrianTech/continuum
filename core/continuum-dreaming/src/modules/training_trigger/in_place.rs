//! What every learner of the trigger shares, whichever engine she learns on: where a lesson starts
//! in the journal ([`start`]), whether a bucket learns now and with what ([`decide`]), which buckets
//! wait ([`waiting_by_base`]), her other competences in flight ([`in_flight`]), the name every file
//! of a bucket carries ([`bucket_stem`]), and a committed generation's candidate sweep and eviction.
//!
//! The learning itself is the node engine's, driven by `on_engine` (IN-PROCESS-LEARNING.md L3, card
//! 28aac827): durability is a committed generation, so the journal's cursor moves only with one, and
//! progress since the newest is volatile and learned again after a restart.

use super::durable::{Arrival, LearnCheckpoint};
use super::{BucketKey, TrainingTriggerState};
use crate::genome::learner::policy::{learn_policy, LearnPolicy};
use std::sync::Arc;
use uuid::Uuid;

/// PURE: the example a row's next step starts at: past everything its journal row, its learner's
/// newest generation and its volatile progress hold (a row neither names holds none there).
pub(crate) fn start(journal: u32, checkpointed: Option<u32>, progress: Option<u32>) -> u32 {
    journal.max(checkpointed.unwrap_or(0)).max(progress.unwrap_or(0)) // unwrap_or: a row the generation or progress does not name holds none there
}


/// What a bucket learns with, decided before any engine is touched: the competence decision, its
/// policy, and the bucket's newest generation (projected, so a crash between a commit and its
/// projection is repaired here).
pub(crate) struct Decided {
    pub(crate) policy: LearnPolicy,
    pub(crate) signature: Option<crate::genome::signature::GeneSignature>,
    pub(crate) generation: Option<LearnCheckpoint>,
}

/// Whether a bucket learns now.
pub(crate) enum Decision {
    Learn(Decided),
    /// The competence does not learn now, or its policy names nothing learnable: the examples stay
    /// pending. Probed with why.
    NotNow,
}

/// What a bucket has waiting, for [`decide`]: its first pending row, how many examples wait, and
/// the competence signature minted from them.
pub(crate) struct Pending<'a> {
    pub(crate) row: &'a Arrival,
    pub(crate) examples: usize,
    pub(crate) signature: Option<crate::genome::signature::GeneSignature>,
}

/// Decide whether bucket `key` learns now and with what. `in_flight` holds her other competences
/// already learning on this base: a near one joins it rather than minting a second gene beside it
/// (Codex on e473f72b1). The one owner of this decision for every path that seats a learner.
pub(crate) async fn decide(
    state: &TrainingTriggerState,
    key: &BucketKey,
    pending: Pending<'_>,
    in_flight: &[(Uuid, crate::genome::signature::GeneSignature)],
) -> Result<Decision, String> {
    let Pending { row, examples, mut signature } = pending;
    // WHICH gene this competence trains, by the one rule (competence::decide): the bucket's pending
    // examples are the competence; nothing of hers learns it yet (no learner is seated for it).
    if let Some(sig) = signature.as_mut() {
        let asking = super::competence::Asking { persona_id: key.persona_id, persona_name: &row.persona_name, trait_kind: &key.trait_kind, base_model: &key.base_model, examples };
        let adoption = crate::genome::gene_trial::Adoption::default_paths();
        let started = std::time::Instant::now();
        let decided = super::competence::decide(&asking, sig, &adoption, in_flight).await;
        // timed, so what a pass spends deciding is measured, never inferred from gaps between probes
        // (card d1e5675c: a 442 s gap on the 5090 first read as slow decisions; 11 had landed in 2.4 s)
        continuum_base::probe!(class = "training.in_place.decided", persona = %key.persona_id, examples, in_flight = in_flight.len(),
            elapsed_ms = started.elapsed().as_millis() as u64, ok = decided.is_ok(), "one competence was decided for a bucket");
        match decided {
            Ok(crate::genome::competence::Decision::Mint) => {}
            Ok(crate::genome::competence::Decision::Fork { parent, .. }) => sig.parent = Some(parent), // learned from the base with its lineage, as the trainer it replaces did
            Ok(crate::genome::competence::Decision::Reuse { gene, similarity }) => {
                super::competence::reuse(&asking, &gene, similarity, &adoption).await; // probed either way; the examples wait for its trial
                return Ok(Decision::NotNow);
            }
            Ok(other) => {
                continuum_base::probe!(class = "training.in_place.not_learning", persona = %key.persona_id, decision = ?other,
                    "the competence decision does not learn now (another of her learners has it, a trial awaits, or memories suffice); the examples stay pending");
                return Ok(Decision::NotNow);
            }
            Err(why) => {
                continuum_base::probe!(class = "training.in_place.undecided", persona = %key.persona_id, reason = %why, "the competence could not be decided; nothing learns blind");
                return Ok(Decision::NotNow);
            }
        }
    }
    let policy = match learn_policy(row.lora.as_ref(), row.schedule.as_ref()) {
        Ok(policy) => policy,
        Err(why) => {
            continuum_base::probe!(class = "training.in_place.unlearnable_policy", persona = %key.persona_id, reason = %why,
                "this bucket's policy names nothing the engine can learn; it stays pending");
            return Ok(Decision::NotNow);
        }
    };
    let store = state.durable.require()?;
    let generation = store.checkpoint_of(key).await?;
    if let Some(generation) = &generation {
        store.project(generation).await?; // a crash between a commit and its projection is repaired here
    }
    Ok(Decision::Learn(Decided { policy, signature, generation }))
}


/// The buckets with a pending submission, split by whether `served_model` names their base: keys
/// only (a projected scan of the pending journal), so a pass never holds another bucket's rows. A
/// pass reads one bucket's rows when it takes that bucket's gate (`DurableStore::arrivals_in`).
pub(crate) struct Waiting {
    /// On the base this engine serves: what a pass may seat.
    pub(crate) here: Vec<BucketKey>,
    /// On a base it does not serve: nothing here learns them.
    pub(crate) elsewhere: Vec<BucketKey>,
}

pub(crate) async fn waiting_by_base(store: &super::durable::DurableStore, served_model: &str) -> Result<Waiting, String> {
    let keys = store.pending_keys().await?;
    let (here, elsewhere) = keys.into_iter().partition(|key| crate::model_registry::artifacts::names_the_same_base(served_model, &key.base_model));
    Ok(Waiting { here, elsewhere })
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
    /// `key`'s newest generation, offered as a candidate the way the sweep offers every bucket's.
    pub(crate) async fn sweep_bucket(self: &Arc<Self>, key: &BucketKey, served_model: &str) -> Result<(), String> {
        let store = self.durable.require()?;
        match store.checkpoint_of(key).await? {
            Some(current) => self.sweep_candidates(vec![current], served_model).await,
            None => Ok(()),
        }
    }

    /// Every bucket's newest generation is candidate work of its own, retried until taken: a
    /// candidate interrupted after its generation, or a bucket with nothing pending, is not lost.
    pub(crate) async fn sweep_all(self: &Arc<Self>, served_model: &str) -> Result<(), String> {
        const PAGE: usize = 64;
        let store = self.durable.require()?;
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

    /// Take each due candidate in `page` whose base `served_model` serves, under its bucket's gate.
    async fn sweep_candidates(self: &Arc<Self>, page: Vec<LearnCheckpoint>, served_model: &str) -> Result<(), String> {
        for generation in page {
            if !crate::model_registry::artifacts::names_the_same_base(served_model, &generation.base_model) {
                continue;
            }
            // under the bucket's owned gate, so it is ordered with that bucket's learning
            let key = BucketKey { persona_id: generation.persona_id, trait_kind: generation.trait_kind.clone(), base_model: generation.base_model.clone() };
            let persona = key.persona_id;
            let swept = self
                .run_owned(key, super::OwnedKind::Sweep, move |state, key| async move {
                    let store = state.durable.require()?;
                    if let Some(current) = store.checkpoint_of(&key).await? {
                        snapshot::maybe(&store, &current).await;
                        evict(&key, &current).await;
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
}

/// Delete `key`'s generations older than `current` from its train dir: nothing reads them again.
/// Run under the bucket's gate; every outcome is probed.
async fn evict(key: &BucketKey, current: &LearnCheckpoint) {
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

mod evict;
mod snapshot;

/// Whether `generation` is a due candidate (its state on the durable row).
#[cfg(test)]
pub(crate) fn snapshot_due(generation: &super::durable::LearnCheckpoint) -> bool {
    snapshot::due(generation.learned(), generation.candidate_at)
}

#[cfg(test)]
mod tests;
