//! A confirmed generation becomes a candidate gene (card 4114a2c6; ONE-RESIDENT 9.2 "Snapshot"):
//! the completion sentinel's chain, for learning in place.
//!
//! A generation's `adapter.gguf` is an immutable gguf-lora of the shadow at an optimizer boundary,
//! published once, so it is the gene as it is (the live shadow keeps learning). It becomes a
//! candidate when the learner has learned a competence's worth since its last candidate
//! (`competence::MIN_EXAMPLES`) and that candidate's trial is no longer settling, so her cards are
//! never split across two trials of one competence. Each loss it learned with must be finite. Its
//! signature is stamped before the gene is live, its provenance is written the way the examiner
//! reads it (the job-directory layout, its request naming exactly the examples the generation
//! consumed), and it is adopted for trial.

use super::Seated;
use crate::genome::fine_tuning::types::{TrainingDataset, TrainingJobRequest, TrainingSource};
use crate::modules::training_trigger::durable::DurableStore;
use crate::modules::training_trigger::BucketKey;
use uuid::Uuid;

/// What a snapshot pass did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Snapshot {
    /// Not yet: too little learned since the last candidate, or its trial is still settling.
    NotYet,
    /// A loss it learned with was not finite: no candidate, her genome unchanged.
    Refused(String),
    /// Adopted for trial.
    Opened { trial: Uuid },
}

/// PURE: whether a learner whose generations hold `learned` examples, and whose last candidate was
/// taken at `candidate_at`, has learned a competence's worth since.
pub(crate) fn due(learned: u32, candidate_at: u32) -> bool {
    learned.saturating_sub(candidate_at) >= crate::genome::competence::MIN_EXAMPLES as u32
}

/// Take a candidate gene from `generation` (its directory) if `seated` has learned enough since its
/// last one.
pub(crate) async fn maybe(store: &DurableStore, key: &BucketKey, persona_name: &str, seated: &mut Seated, generation: &str) -> Result<Snapshot, String> {
    let learned: u32 = seated.checkpointed.values().sum();
    if !due(learned, seated.candidate_at) {
        return Ok(Snapshot::NotYet);
    }
    let adoption = crate::genome::gene_trial::Adoption::default_paths().map_err(|e| format!("her trial file has no place: {e}"))?;
    if let Some(open) = seated.candidate {
        let trials = adoption.trials().load()?;
        let now = continuum_base::time::EpochMs::now().millis();
        if trials.iter().any(|trial| trial.id == open && trial.is_settling(now)) {
            return Ok(Snapshot::NotYet);
        }
    }
    if let Some(loss) = seated.losses.iter().find(|loss| !loss.is_finite()) {
        let why = format!("a loss it learned with was not finite ({loss})");
        continuum_base::probe!(class = "genome.trial.refused", persona = %key.persona_id, gene = key.trait_kind.as_str(), refusal = %why,
            "the learned gene reported a non-finite loss: no trial opened, her genome unchanged");
        seated.losses.clear();
        return Ok(Snapshot::Refused(why));
    }
    let gene = format!("{generation}/adapter.gguf");
    // stamped before the gene is live, so a fill finds it by distance and awaits its trial
    if let Some(signature) = seated.signature.clone() {
        let stamped = crate::genome::signature::signature_store_path().and_then(|store| crate::genome::signature::SignatureStore::stamp_at(&store, &gene, signature));
        if let Err(e) = stamped {
            tracing::warn!(gene = %gene, error = %e, "the candidate's signature failed to stamp: it routes by fallback");
        }
    }
    let trained_from = provenance(store, key, persona_name, seated).await?;
    let trial = adoption
        .adopt(key.persona_id, &key.trait_kind, std::path::Path::new(&gene), &key.base_model, Some(&trained_from), continuum_base::time::EpochMs::now().millis())
        .map_err(|refusal| format!("the candidate was not adopted: {refusal}"))?;
    seated.candidate = Some(trial.id);
    seated.candidate_at = learned;
    seated.losses.clear();
    continuum_base::probe!(class = "genome.trial.opened", persona = %key.persona_id, gene = key.trait_kind.as_str(), trial = %trial.id,
        base = key.base_model.as_str(), learned = u64::from(learned), provenance = %trained_from.display(),
        "a gene learned in place opened a trial: it is in her head now, and the examiner's acceptance test decides it");
    Ok(Snapshot::Opened { trial: trial.id })
}

/// Write the candidate's provenance where the examiner reads it: a job directory whose request names
/// exactly the examples the generation consumed (each submission's first `learned`).
async fn provenance(store: &DurableStore, key: &BucketKey, persona_name: &str, seated: &Seated) -> Result<std::path::PathBuf, String> {
    let mut examples = Vec::new();
    for (submission, learned) in &seated.checkpointed {
        let carried = store.submission_examples(*submission).await?;
        examples.extend(carried.into_iter().take(*learned as usize));
    }
    let dir = crate::genome::fine_tuning::job_dir::default_job_dir(persona_name, &key.trait_kind, Uuid::new_v4())
        .ok_or("no continuum home: the candidate's provenance has no place")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let request = TrainingJobRequest {
        persona_id: key.persona_id,
        persona_name: persona_name.to_owned(),
        base_model: key.base_model.clone(),
        trait_kind: key.trait_kind.clone(),
        dataset: TrainingDataset { examples, source: TrainingSource::Raw, validation_split: 0.0 },
        eval_set: None,
        lora: None,
        schedule: None,
        local_artifact_dir: None,
        resume_from: None,
        parent: seated.signature.as_ref().and_then(|sig| sig.parent.clone()),
    };
    let text = serde_json::to_string_pretty(&request).map_err(|e| format!("the candidate's provenance did not serialize: {e}"))?; // disk boundary: the request the examiner reads
    std::fs::write(dir.join("request.json"), text).map_err(|e| format!("write {}: {e}", dir.join("request.json").display()))?;
    Ok(dir)
}
