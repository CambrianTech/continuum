//! A confirmed generation becomes a candidate gene (card 4114a2c6; ONE-RESIDENT 9.2 "Snapshot"):
//! the completion sentinel's chain, for learning in place.
//!
//! A generation's `adapter.gguf` is an immutable gguf-lora of the shadow at an optimizer boundary,
//! published once, so it is the gene as it is (the live shadow keeps learning). The candidate is a
//! copy of it placed in the genome store beside its provenance, so evicting the engine's old
//! generations never touches a gene on trial. It becomes a
//! candidate when its bucket has learned a competence's worth since its last candidate
//! (`competence::MIN_EXAMPLES`) and that candidate's trial is no longer settling, so her cards are
//! never split across two trials of one competence. Every step a generation holds had a finite loss:
//! a non-finite one quarantines the learner before it can be checkpointed (`in_place::verdict`).
//!
//! The candidate's state lives on the bucket's durable checkpoint row, never in a learner, so a
//! candidate interrupted after its generation (a failed provenance write, a refused adoption, a
//! stopped process) is taken again by the next sweep, whether or not the bucket has anything
//! pending. Each step is idempotent: the provenance directory is named by the generation, and
//! adoption of a gene already on trial returns that trial.

use crate::genome::fine_tuning::types::{TrainingDataset, TrainingJobRequest, TrainingSource};
use crate::modules::training_trigger::durable::{DurableStore, LearnCheckpoint};
use uuid::Uuid;

/// PURE: whether a bucket whose generations hold `learned` examples, and whose last candidate was
/// taken at `candidate_at`, has learned a competence's worth since.
pub(crate) fn due(learned: u32, candidate_at: u32) -> bool {
    learned.saturating_sub(candidate_at) >= crate::genome::competence::MIN_EXAMPLES as u32
}

/// Take `generation` as a candidate gene if it is due; every outcome is probed, none stops the pass.
pub(crate) async fn maybe(store: &DurableStore, generation: &LearnCheckpoint) {
    if !due(generation.learned(), generation.candidate_at) {
        return;
    }
    if let Err(why) = take(store, generation).await {
        continuum_base::probe!(class = "training.in_place.candidate_deferred", persona = %generation.persona_id, gene = generation.trait_kind.as_str(),
            reason = %why, "a due candidate gene was not taken; the next sweep takes it again");
    }
}

async fn take(store: &DurableStore, generation: &LearnCheckpoint) -> Result<(), String> {
    let adoption = crate::genome::gene_trial::Adoption::default_paths().map_err(|e| format!("her trial file has no place: {e}"))?;
    if let Some(open) = generation.candidate_trial {
        let trials = adoption.trials().load()?;
        let now = continuum_base::time::EpochMs::now().millis();
        if trials.iter().any(|trial| trial.id == open && trial.is_settling(now)) {
            return Ok(()); // her cards are judging the last candidate of this competence
        }
    }
    let trained_from = provenance(store, generation).await?;
    let gene = place_gene(generation, &trained_from)?;
    // stamped before the gene is live, so a fill finds it by distance and awaits its trial
    if let Some(signature) = generation.signature.clone() {
        let stamped = crate::genome::signature::signature_store_path().and_then(|path| crate::genome::signature::SignatureStore::stamp_at(&path, &gene.to_string_lossy(), signature));
        if let Err(e) = stamped {
            tracing::warn!(gene = %gene.display(), error = %e, "the candidate's signature failed to stamp: it routes by fallback");
        }
    }
    let trial = adoption
        .adopt(generation.persona_id, &generation.trait_kind, &gene, &generation.base_model, Some(&trained_from), continuum_base::time::EpochMs::now().millis())
        .map_err(|refusal| format!("the candidate was not adopted: {refusal}"))?;
    store.record_candidate(generation, trial.id).await?;
    continuum_base::probe!(class = "genome.trial.opened", persona = %generation.persona_id, gene = generation.trait_kind.as_str(), trial = %trial.id,
        base = generation.base_model.as_str(), learned = u64::from(generation.learned()), provenance = %trained_from.display(),
        "a gene learned in place opened a trial: it is in her head now, and the examiner's acceptance test decides it");
    Ok(())
}

/// Place the generation's adapter in the candidate's job directory, beside its provenance: the gene
/// on trial lives in the genome store, never in the engine's train dir, whose old generations are
/// evicted. Published once; a retry finds the same bytes there and takes it.
fn place_gene(generation: &LearnCheckpoint, job: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let source = std::path::Path::new(&generation.path).join("adapter.gguf");
    let bytes = std::fs::read(&source).map_err(|e| format!("the generation's adapter {}: {e}", source.display()))?;
    match crate::genome::fine_tuning::job_dir::publish_job_file(job, "adapter.gguf", &bytes)? {
        None => Ok(job.join("adapter.gguf")),
        Some(existing) if existing == bytes => Ok(job.join("adapter.gguf")),
        Some(_) => Err(format!("{} already holds another adapter than generation {}", job.display(), generation.name)),
    }
}

/// Write the candidate's provenance where the examiner reads it: a job directory, named by the
/// generation (so taking it again rewrites the same place), whose request names exactly the examples
/// the generation consumed (each submission's first `learned`).
async fn provenance(store: &DurableStore, generation: &LearnCheckpoint) -> Result<std::path::PathBuf, String> {
    let mut examples = Vec::new();
    for position in &generation.positions {
        let carried = store.submission_examples(position.submission).await?;
        examples.extend(carried.into_iter().take(position.learned as usize));
    }
    let id = Uuid::new_v5(&generation.persona_id, generation.name.as_bytes());
    let dir = crate::genome::fine_tuning::job_dir::default_job_dir(&generation.persona_name, &generation.trait_kind, id)
        .ok_or("no continuum home: the candidate's provenance has no place")?;
    let request = TrainingJobRequest {
        persona_id: generation.persona_id,
        persona_name: generation.persona_name.clone(),
        base_model: generation.base_model.clone(),
        trait_kind: generation.trait_kind.clone(),
        dataset: TrainingDataset { examples, source: TrainingSource::Raw, validation_split: 0.0 },
        eval_set: None,
        lora: None,
        schedule: None,
        local_artifact_dir: None,
        resume_from: None,
        parent: generation.signature.as_ref().and_then(|sig| sig.parent.clone()),
    };
    let text = serde_json::to_string_pretty(&request).map_err(|e| format!("the candidate's provenance did not serialize: {e}"))?; // disk boundary: the request the examiner reads
    // the job directory's one writer: published once and immutable, so a retry finds the record a
    // first attempt already wrote (named by the generation, it holds the same examples)
    crate::genome::fine_tuning::job_dir::publish_job_file(&dir, "request.json", text.as_bytes())?;
    Ok(dir)
}
