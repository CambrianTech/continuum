//! `genome/job-create` — pick a capable adapter via the coordinator, hand it the
//! typed [`TrainingJobRequest`], return the [`JobHandle`] plus the provider picked.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::genome::fine_tuning::{
    coordinator::FineTuningCoordinator, JobHandle, TrainingJobRequest,
};

use super::fine_tuning_error_kind;
use crate::genome::training_outcome::Took;
use crate::genome::competence::{Decision, GeneRef};
use crate::genome::gene_trial::{Adoption, TrialState};

/// Wire shape for `genome/job-create` params. Mirrors [`TrainingJobRequest`]
/// verbatim (flattened), plus the optional `preferredProvider` hint the coordinator
/// honors — or rejects, surfacing the rejection as `success=false` rather than
/// silently routing elsewhere.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/JobCreateParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct JobCreateParams {
    /// Correlates the trigger's durable intent with the actual created handle.
    /// Evidence only: provider creation is not made idempotent by this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub trigger_dispatch_id: Option<uuid::Uuid>,
    /// The trigger's attempt (card 55d4b9e3). When present the create is admitted in the
    /// attempt ledger BEFORE the trainer runs: a repeat returns the same job, a create for an
    /// attempt reconciliation already closed is refused before anything starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub trigger_attempt_id: Option<uuid::Uuid>,
    #[serde(flatten)]
    pub request: TrainingJobRequest,
    /// Force a specific provider (e.g. `"openai"`, `"local-candle"`). Honored only
    /// if that provider is in the capable set; otherwise the outcome is
    /// `success=false` — never a silent fallback to a different provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub preferred_provider: Option<String>,
    /// Name of an on-disk dataset under the datasets root
    /// (`~/.continuum/datasets/<name>/train.jsonl`, the chat `{messages}` JSONL
    /// that `dataset/from-captures` / `dataset/from-turns` write). Loaded into the
    /// request's `dataset` before adapter selection, so adapters always see a
    /// populated dataset. Mutually exclusive with inlining `dataset` examples —
    /// exactly one of the two must be provided.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub dataset_name: Option<String>,
}

/// The created job: its handle plus the provider the coordinator selected. The
/// provider is surfaced for telemetry + operators validating that locality
/// preference fired.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/JobCreateResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct JobCreateResult {
    pub handle: JobHandle,
    pub selected_provider: String,
}

/// Outcome envelope for `genome/job-create`. `success=true` carries `result`;
/// `success=false` carries `error` (+ `errorKind` when the failure came from the
/// adapter rather than the coordinator). See the module docs for why expected
/// domain failures are data, not a transport `Err`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/JobCreateOutcome.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct JobCreateOutcome {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub result: Option<JobCreateResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error_kind: Option<String>,
    /// What the full bucket's decision did INSTEAD of creating a job (`result` is `None`
    /// and `success` is true). Absent when a job was created (a mint, or a fork).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub took: Option<Took>,
    /// Set with `errorKind` [`ERROR_KIND_CAPACITY_PARKED`]: what the refused job waits for, so
    /// the trigger parks its examples with the wake key instead of retrying or failing them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub capacity_wait: Option<crate::genome::fine_tuning::types::CapacityWait>,
}

/// A reuse that could not pull its gene: no job, the bucket keeps its examples, the next
/// fill retries (the trigger decodes it as retryable, never as recovery).
pub const ERROR_KIND_REUSE_PULL_FAILED: &str = "ReusePullFailed";
/// A reuse whose gene was not adopted (manifest, trial file, or already decided): same.
pub const ERROR_KIND_REUSE_ADOPT_FAILED: &str = "ReuseAdoptFailed";
/// Her trial file could not be read: nothing decided blind; the next fill retries.
pub const ERROR_KIND_TRIAL_FILE_UNREADABLE: &str = "TrialFileUnreadable";
/// No room for the smallest window that trains one of her examples, by measurement: no job;
/// the trigger parks the batch with the outcome's `capacityWait` (card de9e1342).
pub const ERROR_KIND_CAPACITY_PARKED: &str = "CapacityParked";
/// The trainer refused before creating anything (missing interpreter, a host it cannot run
/// on): certain, so the trigger retries on a later fill, never recovery (card a9bc56ea).
pub const ERROR_KIND_TRAINER_UNAVAILABLE: &str = "TrainerUnavailable";
/// Reconciliation already closed this attempt: nothing was or will be started (certain).
pub const ERROR_KIND_ATTEMPT_FENCED: &str = "AttemptFenced";
/// The attempt ledger could not record the admission, so the trainer was never called
/// (certain: a create the ledger cannot witness is never started).
pub const ERROR_KIND_ATTEMPT_UNRECORDED: &str = "AttemptUnrecorded";
/// This attempt was admitted before and never settled: a create may be running or may have
/// crashed. Deliberately not a retryable kind, so the trigger reads it as uncertain.
pub const ERROR_KIND_ATTEMPT_UNSETTLED: &str = "AttemptUnsettled";
/// This attempt's existing record could not be read, so it may name a job already started.
/// Deliberately not a retryable kind: the trigger reads it as uncertain.
pub const ERROR_KIND_ATTEMPT_UNREADABLE: &str = "AttemptUnreadable";

/// What a recorded attempt answers a create with (card 55d4b9e3): `Ok(Some(job))` the job it
/// already made, `Ok(None)` go ahead, or `Err(outcome)`, a typed refusal returned as is.
fn recorded_answer(
    admission: Result<Option<super::attempt_ledger::Admission>, super::attempt_ledger::AdmitError>,
    attempt: uuid::Uuid,
    request: &TrainingJobRequest,
) -> Result<Option<crate::genome::fine_tuning::JobHandle>, Result<JobCreateOutcome, crate::sdk_codegen::CommandError>> {
    use super::attempt_ledger::{AdmitError, Admission};
    match admission {
        Ok(None) | Ok(Some(Admission::Fresh)) => Ok(None),
        Ok(Some(Admission::AlreadyCreated(handle))) => Ok(Some(handle)),
        Ok(Some(Admission::Fenced)) => Err(refused_decision(ERROR_KIND_ATTEMPT_FENCED, request, &format!("attempt {attempt} was closed by reconciliation; nothing started"))),
        Ok(Some(Admission::Unsettled)) => Err(refused_decision(ERROR_KIND_ATTEMPT_UNSETTLED, request, &format!("attempt {attempt} was admitted before and never settled"))),
        Err(AdmitError::Unreadable(e)) => Err(refused_decision(ERROR_KIND_ATTEMPT_UNREADABLE, request, &format!("attempt {attempt}'s record could not be read: {e}"))),
        Err(AdmitError::Unrecorded(e)) => Err(refused_decision(ERROR_KIND_ATTEMPT_UNRECORDED, request, &format!("attempt {attempt} could not be admitted: {e}"))),
    }
}

/// Whether a trainer's refusal proves it created nothing, so its attempt settles as failed
/// (fenced) instead of staying admitted. The same created-nothing kinds the trigger retries;
/// `LocalTrainerFailed`, `Transient` and the rest can follow a trainer that started.
pub(super) fn settles_as_not_created(e: &crate::genome::fine_tuning::FineTuningError) -> bool {
    use crate::genome::fine_tuning::FineTuningError as E;
    matches!(
        e,
        E::TrainerUnavailable(_) | E::InvalidRequest(_) | E::MissingCredentials(_) | E::ProviderRejected(_) | E::CapacityParked(_)
    )
}

/// The decision could not be made: a typed refusal the trigger retries on the next fill,
/// probed so a bucket that keeps refusing is readable.
fn refused_decision(error_kind: &str, request: &TrainingJobRequest, error: &str) -> Result<JobCreateOutcome, crate::sdk_codegen::CommandError> {
    continuum_base::probe!(
        class = "genome.decision.refused",
        persona = %request.persona_name,
        trait_kind = %request.trait_kind,
        error_kind,
        error,
        "a full bucket could not decide: no job, the bucket keeps its examples, the next fill retries"
    );
    Ok(JobCreateOutcome { success: false, result: None, error: Some(error.to_string()), error_kind: Some(error_kind.to_string()), took: None, capacity_wait: None })
}

crate::action_command! {
    /// Create a LoRA fine-tuning job. The coordinator picks a capable adapter
    /// (honoring `preferredProvider` if given and capable), the adapter starts the
    /// job, and the handle + selected provider come back. On no capable adapter, an
    /// unsatisfiable preference, or an adapter rejection, the outcome is
    /// `success=false` with the reason (and an `errorKind` slug for adapter
    /// failures) — branch on it; this is never a silent fallback.
    pub struct GenomeJobCreate {
        coordinator: Arc<FineTuningCoordinator>,
        attempts: Arc<super::attempt_ledger::AttemptLedger>,
        #[cfg(test)]
        test_job_board: Arc<crate::genome::fine_tuning::TrainingJobBoard>,
        #[cfg(test)]
        test_artifacts: Arc<tempfile::TempDir>,
    }
    name: "genome/job-create",
    access: Privileged,
    params: JobCreateParams,
    output: JobCreateOutcome,
    run(this, _ctx, p) => {
        let mut p = p;
        // Real adapter tests must never choose a native genome output directory.
        // The fixture owns the fallback; an explicit test request still wins.
        #[cfg(test)]
        p.request.local_artifact_dir.get_or_insert_with(|| this.test_artifacts.path().to_path_buf());
        // 0. Resolve the dataset: by name from disk, or inline — exactly one.
        //    An empty dataset must never reach an adapter (it would "train"
        //    on nothing and burn a job slot).
        match (&p.dataset_name, p.request.dataset.examples.is_empty()) {
            (Some(name), true) => {
                let root = match crate::modules::dataset::default_datasets_root() {
                    Ok(root) => root,
                    Err(e) => {
                        return Ok(JobCreateOutcome {
                            success: false,
                            result: None,
                            error: Some(format!("datasetName {name:?}: no datasets root ({e})")),
                            error_kind: None,
                            took: None,
                            capacity_wait: None,
                        });
                    }
                };
                let path = root.join(name).join("train.jsonl");
                p.request.dataset = match crate::genome::fine_tuning::TrainingDataset::from_chat_jsonl(
                    &path,
                    crate::genome::fine_tuning::TrainingSource::OperatorCurated,
                ) {
                    Ok(ds) => ds,
                    Err(e) => {
                        return Ok(JobCreateOutcome {
                            success: false,
                            result: None,
                            error: Some(format!(
                                "datasetName {name:?} did not load: {e}. Datasets live under \
                                 ~/.continuum/datasets/<name>/train.jsonl — `dataset/list` shows \
                                 what exists."
                            )),
                            error_kind: None,
                            took: None,
                            capacity_wait: None,
                        });
                    }
                };
            }
            (Some(_), false) => {
                return Ok(JobCreateOutcome {
                    success: false,
                    result: None,
                    error: Some(
                        "provide EITHER datasetName OR inline dataset examples, not both — \
                         two datasets for one job is ambiguous"
                            .into(),
                    ),
                    error_kind: None,
                    took: None,
                    capacity_wait: None,
                });
            }
            (None, true) => {
                return Ok(JobCreateOutcome {
                    success: false,
                    result: None,
                    error: Some(
                        "no training data: pass datasetName (an on-disk dataset from \
                         dataset/from-captures — see dataset/list) or inline dataset examples"
                            .into(),
                    ),
                    error_kind: None,
                    took: None,
                    capacity_wait: None,
                });
            }
            (None, false) => {}
        }

        // card 55d4b9e3: an attempt that already made its job, or was fenced, answers BEFORE the
        // reuse/join decisions below, which would otherwise "join" the attempt's own job and hand
        // its batch back to the bucket (to be trained twice). Read BEFORE a trainer is chosen
        // (card 975d27fa, binding first): a repeat answers from its own record even when the
        // trainer that made its job is no longer selectable (a peer whose advertisement lapsed).
        let attempt = p.trigger_attempt_id;
        // One create per attempt at a time, for the whole command (see AttemptLedger::run).
        let _attempt_run = match attempt {
            Some(attempt) => Some(this.attempts.run(attempt).await),
            None => None,
        };
        let mut prior: Option<crate::genome::fine_tuning::JobHandle> = None;
        if let Some(attempt) = attempt {
            match recorded_answer(this.attempts.peek(attempt).await, attempt, &p.request) {
                Ok(job) => prior = job,
                Err(refusal) => return refusal,
            }
        }

        // 1. Coordinator picks a provider. Any CoordinatorError (no capable
        //    adapter, preference unsatisfiable) surfaces as success=false with the
        //    error's diagnostic text.
        let (selected_provider, adapter) = match prior.as_ref() {
            // A repeat answers with the job its attempt already made, under that job's own
            // provider: no trainer is chosen, so none can be reported for it.
            Some(job) => (job.provider_id.clone(), None),
            None => match this.coordinator.select(&p.request, p.preferred_provider.as_deref()) {
                Ok((provider, adapter)) => (provider, Some(adapter)),
                Err(e) => {
                    return Ok(JobCreateOutcome {
                        success: false,
                        result: None,
                        error: Some(e.to_string()),
                        error_kind: None,
                        took: None,
                        capacity_wait: None,
                    });
                }
            },
        };

        // Capture the genome-paging context BEFORE the request moves into the
        // adapter — the L3 completion sentinel needs exactly these four facts to run
        // the register→trial chain when the job completes, without re-deriving any.
        let watched_persona_id = p.request.persona_id;
        let watched_persona_name = p.request.persona_name.clone();
        let watched_base_model = p.request.base_model.clone();
        let watched_trait_kind = p.request.trait_kind.clone();
        let watched_eval_set = p.request.eval_set.clone();
        // MINT the gene's embedding-space signature NOW — the training corpus is
        // in hand at exactly this moment and never again (the audited 2026-08-22
        // break: an adopted gene's corpus was unreferenceable). Best-effort: a
        // failed mint warns and trains anyway — the gene routes by the fallback
        // path; it never blocks the training the persona is owed.
        let mut watched_signature = {
            let texts: Vec<String> = p
                .request
                .dataset
                .examples
                .iter()
                .map(|ex| format!("{}\n{}", ex.prompt, ex.completion))
                .collect();
            let joined = texts.join("\n");
            let corpus = crate::genome::corpus_ref::CorpusRef {
                name: p
                    .dataset_name
                    .clone()
                    .unwrap_or_else(|| format!("inline:{}", p.request.trait_kind)), // inline datasets have no on-disk name; the trait names the mint
                content_hash: crate::cognition::inbox_admission::content_hash_sha256(&joined),
                size_bytes: joined.len() as u64,
                source_url: None,
            };
            let embedder = crate::ai::embedding::resolve_recall_embedder_local().await;
            match crate::genome::signature::GeneSignature::mint(&texts, corpus, &embedder, continuum_base::time::EpochMs::now().millis())
                .await
            {
                Ok(sig) => Some(sig),
                Err(e) => {
                    tracing::warn!(
                        trait_kind = %p.request.trait_kind,
                        error = %e,
                        "gene signature mint failed — training proceeds, gene will route by fallback"
                    );
                    None
                }
            }
        };

        // 1b. THE DECISION (GENE-REUSE-FORK-MINT.md, step 2's receipt): with the corpus's
        //     signature in hand, is this competence one an existing gene already carries
        //     (reuse), a child of one (fork), or new (mint)? Decided against her signature
        //     store and her OWN trials (residency = a gene her work holds open or promoted,
        //     never the node's manifest, which carries every citizen's genes), probed as
        //     `genome.decision`, recorded on the job, and ACTED on below: join, await,
        //     reuse (adopt for trial), fork (lineage on the request), mint.
        // The node's serving manifest and her trial file, resolved once: residency and her
        // verdicts are read from them, and a reuse adopts through them.
        #[cfg(not(test))]
        let adoption = Adoption::default_paths();
        #[cfg(test)]
        let adoption: Result<Adoption, crate::genome::gene_trial::AdoptRefusal> = Ok(Adoption::at(
            this.test_artifacts.path().join("adapters.json"),
            crate::genome::gene_trial::GeneTrials::at(this.test_artifacts.path().join("trials.json")),
        ));
        let decision = match watched_signature.as_ref().filter(|_| prior.is_none()) {
            None => None,
            Some(sig) => {
            use crate::genome::competence::{decide_with_pending, nearest_in_store, nearest_pending, Competence, Surprise, SIM_FORK};
            let competence = Competence {
                centroid: sig.centroid.clone(),
                members: (0..p.request.dataset.examples.len()).collect(),
                cohesion: 1.0, // one bucket is one competence here; clustering within it is step 2's follow-on
                representative: 0,
            };
            // The signature store lives beside the manifest the adoption serves through: ONE
            // derivation of the path, in tests and in production alike.
            let adoption = match adoption.as_ref() {
                Ok(a) => a,
                Err(refusal) => return refused_decision(ERROR_KIND_TRIAL_FILE_UNREADABLE, &p.request, &format!("her trial file has no place: {refusal}")),
            };
            let store_path = adoption.manifest().with_file_name("signatures.json");
            let store = crate::genome::signature::SignatureStore::load_at(&store_path)
                .unwrap_or_default(); // unwrap_or_default: no store yet = nothing near, which decides "mint" honestly
            // HER TRIALS, read once and LOUD when unreadable (Cormac on #4794): a corrupt
            // trial file read as empty would let a fork or a mint go ahead beside her open
            // trial, and would offer a retired gene back. Nothing is decided blind.
            let trials = match adoption.trials().load() {
                Ok(t) => t,
                Err(e) => return refused_decision(ERROR_KIND_TRIAL_FILE_UNREADABLE, &p.request, &format!("her trial file could not be read: {e}")),
            };
            // A pre-1970 clock reads 0: every tally falls outside its window (not measured,
            // which never halts training) and every open trial reads live (held, never
            // forked beside).
            let now_ms = continuum_base::time::EpochMs::now().millis();
            let hers = |t: &&crate::genome::gene_trial::GeneTrial| t.persona_id == watched_persona_id;
            // Residency is HERS: a gene her work holds open or promoted on this base. The
            // node's manifest carries every citizen's genes; a teammate's promoted gene near
            // her competence is a reuse for her, never a fork (Cormac on #4794, point 7).
            let resident: Vec<std::path::PathBuf> = trials
                .iter()
                .filter(hers)
                .filter(|t| t.base_model_id == p.request.base_model && matches!(t.state, TrialState::Trial | TrialState::Promoted))
                .map(|t| t.path.clone())
                .collect();
            // A gene her work already decided against, or left unjudged, is never offered
            // back as a reuse, from the store or the hub.
            let retired: Vec<std::path::PathBuf> = trials
                .iter()
                .filter(hers)
                .filter(|t| matches!(t.state, TrialState::Retired | TrialState::Expired))
                .map(|t| t.path.clone())
                .collect();
            // ONE LOOKUP, THREE SOURCES (GENE-REUSE-FORK-MINT.md §5): her store first; the
            // hub only when the store has nothing within fork distance, since a hub
            // probe costs a network read per candidate and local knowledge, when it is
            // near, is the answer. The mesh (peers' stores over airc) is the source
            // between them, not yet wired. Only a Reuse decision ever pulls weights.
            let local = nearest_in_store(&competence, &store, &sig.embedder, &resident, &retired);
            let (nearest, source) = match local {
                Some(n) if n.similarity >= SIM_FORK => (Some(n), "store"),
                local => match crate::commands::genome_share::nearest_on_hub(&competence, &sig.embedder, &p.request.base_model, None, 20, &retired).await {
                    Some(hub) if local.as_ref().is_none_or(|l| hub.similarity > l.similarity) => (Some(hub), "hub"),
                    _ => (local, "store"),
                },
            };
            // A JOB OF HERS ALREADY TRAINING THIS COMPETENCE is checked before any branch
            // that would train: the board's watched jobs for this persona and base, by the
            // signature each was minted with. Four Mints for one card's credit (the 5090,
            // 2026-10-05) is the falsifier this closes.
            #[cfg(not(test))]
            let board_jobs = crate::genome::fine_tuning::TrainingJobBoard::global().pending();
            #[cfg(test)]
            let board_jobs = this.test_job_board.as_ref().pending();
            let in_flight = nearest_pending(
                &competence,
                &sig.embedder,
                board_jobs
                    .iter()
                    .filter(|j| j.persona_id == watched_persona_id && j.base_model == p.request.base_model)
                    .filter_map(|j| j.signature.as_ref().map(|s| (j.handle.local_id, s))),
            );
            // A GENE THAT JUST LANDED for this competence and is still settling: ONE
            // matching rule, the bucket's key (her, this trait, this base), the same rule
            // the trigger holds the bucket by (Cormac on #4794, point 4: two rules churned
            // a bucket between them). The signature similarity rides along as a measurement,
            // never as the decision; past its settling window a gene holds nothing and is
            // simply resident (`nearest`).
            let on_trial = trials
                .iter()
                .filter(hers)
                .filter(|t| t.base_model_id == p.request.base_model && t.alias == p.request.trait_kind && t.is_settling(now_ms))
                .map(|t| crate::genome::competence::Pending {
                    id: t.id,
                    similarity: store.by_path.get(&t.path.display().to_string()).and_then(|s| s.similarity_in(&sig.embedder, &competence.centroid)).unwrap_or(1.0), // 1.0 = the key already says it is this competence; the number is a measurement when the signature exists
                })
                .max_by(|a, b| a.similarity.total_cmp(&b.similarity));
            // HER SURPRISE, read from her own strip: the verdict surprise (#4774, her stated
            // expectations against the room's verdicts, every activity folded by counts).
            // `S(C)` is this until the model surprise is measured per competence
            // (GENE-REUSE-FORK-MINT.md §3). Not yet judged is not low.
            let surprise = crate::persona::perception_feed::awareness_of(watched_persona_id, now_ms)
                .and_then(|a| a.verdict_surprise());
            let decision = decide_with_pending(
                surprise.map_or(Surprise::NotYetMeasured, |v| Surprise::Measured { s: v.s }),
                nearest.as_ref(),
                in_flight.as_ref(),
                on_trial.as_ref(),
            );
            continuum_base::probe!(
                class = "genome.decision",
                persona = %p.request.persona_name,
                trait_kind = %p.request.trait_kind,
                examples = p.request.dataset.examples.len() as u64,
                branch = ?decision,
                nearest = %nearest.as_ref().map(|n| n.gene.to_string()).unwrap_or_default(), // "" = nothing in any source
                similarity = nearest.as_ref().map(|n| n.similarity).unwrap_or(0.0), // 0.0 = nothing in any source
                nearest_resident = nearest.as_ref().is_some_and(|n| n.resident),
                source,
                in_flight = %in_flight.as_ref().map(|j| j.id.to_string()).unwrap_or_default(), // "" = no job of hers training nearby
                on_trial = %on_trial.as_ref().map(|t| t.id.to_string()).unwrap_or_default(), // "" = no gene of hers on trial nearby
                retired = retired.len() as u64,
                surprise = if surprise.is_some() { "verdict" } else { "not_measured" },
                s = surprise.map(|v| v.s).unwrap_or(0.0), // 0.0 = not measured; read `surprise` first
                judged = surprise.map(|v| v.judged).unwrap_or(0) as u64, // 0 = not measured; read `surprise` first
                "a full bucket decided: join, await, reuse, fork or mint, against her surprise, her store, her trials and the hub"
            );
            Some(decision)
            }
        };

        // THE ACTIONS THAT CREATE NO JOB. The caller (the trigger) keeps the examples in
        // her bucket for each of them: they are the competence's evidence, nothing trained
        // on them, and the fill after the pending thing settles decides again.
        let examples = p.request.dataset.examples.len() as u64;
        let without_job = |took: Took| Ok(JobCreateOutcome { success: true, result: None, error: None, error_kind: None, took: Some(took), capacity_wait: None });
        match &decision {
            // A competence already training is never minted twice.
            Some(Decision::Join { job, similarity }) => {
                continuum_base::probe!(
                    class = "genome.joined",
                    persona = %p.request.persona_name,
                    trait_kind = %p.request.trait_kind,
                    examples,
                    job = %job,
                    similarity = *similarity,
                    "a job of hers already trains this competence — these examples wait for it; no second job"
                );
                return without_job(Took::Joined { job: *job });
            }
            // A competence already being judged in her work is never forked beside its trial.
            Some(Decision::Await { trial, similarity }) => {
                continuum_base::probe!(
                    class = "genome.awaited",
                    persona = %p.request.persona_name,
                    trait_kind = %p.request.trait_kind,
                    examples,
                    trial = %trial,
                    similarity = *similarity,
                    "a gene for this competence is on trial in her work — these examples wait for her verdict"
                );
                return without_job(Took::Awaited { trial: *trial });
            }
            // A gene that already carries this competence is adopted for trial: pulled first
            // when it lives on the hub, then registered dormant and handed to the examiner.
            // No training. Her work decides it; a retired one is never offered again.
            Some(Decision::Reuse { gene, similarity }) => {
                let refused = |error_kind: &str, error: String| {
                    Ok(JobCreateOutcome { success: false, result: None, error: Some(error), error_kind: Some(error_kind.into()), took: None, capacity_wait: None })
                };
                let path = match gene {
                    GeneRef::Local { path } => path.clone(),
                    GeneRef::Hub { repo } => {
                        let pulled = crate::commands::genome_share::pull_gene(&crate::commands::genome_share::GenomePullParams {
                            repo: repo.clone(),
                            base_model: p.request.base_model.clone(),
                            alias: Some(p.request.trait_kind.clone()),
                        })
                        .await;
                        match pulled {
                            Ok(r) => std::path::PathBuf::from(r.path),
                            Err(e) => {
                                continuum_base::probe!(
                                    class = "genome.reuse.refused",
                                    persona = %p.request.persona_name,
                                    trait_kind = %p.request.trait_kind,
                                    gene = %gene,
                                    error = %e,
                                    "the hub gene this competence would reuse did not pull: no trial, no job; the bucket keeps its examples"
                                );
                                return refused(ERROR_KIND_REUSE_PULL_FAILED, format!("reuse of {gene}: pull failed: {e}"));
                            }
                        }
                    }
                };
                let adopted = adoption
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(|a| a.adopt(watched_persona_id, &p.request.trait_kind, &path, &p.request.base_model, None, continuum_base::time::EpochMs::now().millis()));
                match adopted {
                    Ok(trial) => {
                        continuum_base::probe!(
                            class = "genome.reused",
                            persona = %p.request.persona_name,
                            trait_kind = %p.request.trait_kind,
                            examples,
                            gene = %gene,
                            path = %path.display(),
                            similarity = *similarity,
                            trial = %trial.id,
                            "an existing gene carries this competence: adopted, no training; with no training job here the examiner receipts it unexaminable"
                        );
                        return without_job(Took::Reused { trial: trial.id, gene: gene.clone() });
                    }
                    Err(refusal) => {
                        continuum_base::probe!(
                            class = "genome.reuse.refused",
                            persona = %p.request.persona_name,
                            trait_kind = %p.request.trait_kind,
                            gene = %gene,
                            refusal = %refusal,
                            "the gene this competence would reuse was not adopted: no trial, no job; the bucket keeps its examples"
                        );
                        return refused(ERROR_KIND_REUSE_ADOPT_FAILED, format!("reuse of {gene}: {refusal}"));
                    }
                }
            }
            // A child of a near gene: trained on her examples with its lineage on the
            // request (the trainer warm-starts if it can, and says so if it cannot) and
            // on the signature the child is adopted with.
            Some(Decision::Fork { parent, similarity }) => {
                continuum_base::probe!(
                    class = "genome.forked",
                    persona = %p.request.persona_name,
                    trait_kind = %p.request.trait_kind,
                    examples,
                    parent = %parent,
                    similarity = *similarity,
                    "a near gene is resident or a cousin: a child trains on her examples with it as parent"
                );
                p.request.parent = Some(parent.clone());
                if let Some(sig) = watched_signature.as_mut() {
                    sig.parent = Some(parent.clone());
                }
            }
            // The room confirms what she expects here: memories suffice. No gene, and the
            // bucket keeps its examples for the fill after her surprise rises.
            Some(Decision::Nothing { why: crate::genome::competence::NothingBecause::SurpriseLow { s } }) => {
                continuum_base::probe!(
                    class = "genome.unsurprised",
                    persona = %p.request.persona_name,
                    trait_kind = %p.request.trait_kind,
                    examples,
                    s = *s,
                    floor = crate::genome::competence::SURPRISE_FLOOR,
                    "her expectations hold here — memories suffice, no gene minted; the bucket waits for her surprise to rise"
                );
                return without_job(Took::Unsurprised { s: *s });
            }
            Some(Decision::Mint) | None => {}
        }

        // 2. Admit the attempt (card 55d4b9e3), then the adapter creates the job.
        //    FineTuningError carries a stable errorKind slug callers branch on.
        // Admit the attempt under its lock, unless the early read already found its job.
        let needs_admission = prior.is_none();
        // Where a new job runs, chosen ONCE by its trainer before anything is recorded or sent
        // (card 975d27fa): recorded with the attempt, handed to the trainer with it, never
        // chosen again by a retry.
        let destination = match (&adapter, needs_admission) {
            (Some(adapter), true) => match adapter.destination(&p.request) {
                Ok(destination) => destination,
                Err(e) => {
                    return Ok(JobCreateOutcome {
                        success: false,
                        result: None,
                        error: Some(e.to_string()),
                        error_kind: Some(fine_tuning_error_kind(&e).to_string()),
                        took: None,
                        capacity_wait: None,
                    });
                }
            },
            _ => crate::genome::fine_tuning::Destination::Local,
        };
        if let (Some(attempt), true) = (attempt, needs_admission) {
            let admitted = this
                .attempts
                .admit(attempt, p.trigger_dispatch_id, &selected_provider, &destination.to_ledger())
                .await
                .map(Some);
            match recorded_answer(admitted, attempt, &p.request) {
                Ok(job) => prior = job,
                Err(refusal) => return refusal,
            }
        }
        let repeated = prior.is_some();
        let created = match (prior, adapter) {
            (Some(handle), _) => Ok(handle),
            (None, Some(adapter)) => {
                let binding = crate::genome::fine_tuning::JobBinding {
                    attempt,
                    dispatch: p.trigger_dispatch_id,
                    destination,
                };
                adapter.create_bound(p.request, &binding).await
            }
            // Unreachable by construction (no trainer is only accepted for a repeat), and
            // still answered as a refusal rather than a panic.
            (None, None) => Err(crate::genome::fine_tuning::FineTuningError::InvalidRequest(
                "no trainer was selected for a new job".into(),
            )),
        };
        if let (Some(attempt), false) = (attempt, repeated) {
            use super::attempt_ledger::AttemptState;
            let settled = match &created {
                // Created is settled only AFTER the board holds the job (below): a crash in
                // between leaves it Admitted, reconciled Unknown, never a created job nobody
                // watches (Codex review of c2c955174).
                Ok(_) => None,
                Err(e) if settles_as_not_created(e) => Some(AttemptState::Failed { kind: fine_tuning_error_kind(e).to_string() }),
                // Anything else may follow a trainer that started: left Admitted, reconciled Unknown.
                Err(_) => None,
            };
            if let Some(state) = settled {
                if let Err(error) = this.attempts.settle(attempt, state).await {
                    continuum_base::probe!(
                        class = "genome.attempt.unsettled",
                        attempt = %attempt,
                        error = %error,
                        "the trainer answered but the attempt ledger could not record it: it stays admitted and reconciles as unknown"
                    );
                }
            }
        }
        match created {
            Ok(handle) => {
                // L2→L3 seam (the ONE birth-seam): every training job is born here —
                // the trigger's batch path dispatches THIS command, a direct
                // `uu genome/job-create` lands here, and so will any future caller.
                // Registering the in-flight handle on the board at this single point
                // is what lets the completion sentinel poll it and open an in-room gene trial
                // after artifact registration and the finite-loss sanity gate. Without it the handle drops on
                // the floor and the loop stops at "trained", never "measured +
                // adopted" ([[dev-task-learning-loop-gap-map]] L3,
                // docs/genome/DEV-TASK-LOOP-CLOSURE-PLAN.md).
                #[cfg(not(test))]
                let job_board = crate::genome::fine_tuning::TrainingJobBoard::global();
                #[cfg(test)]
                let job_board = this.test_job_board.as_ref();
                // A repeated create returns the job its attempt already made. It is registered
                // again only if the board does not hold it (a crash between create and
                // registration), never journaled twice.
                let on_board = repeated
                    && job_board.pending().iter().any(|job| job.handle.local_id == handle.local_id);
                if !on_board {
                    job_board.register(
                        crate::genome::fine_tuning::WatchedJob {
                            trigger_dispatch_id: p.trigger_dispatch_id,
                            handle: handle.clone(),
                            persona_id: watched_persona_id,
                            persona_name: watched_persona_name,
                            base_model: watched_base_model,
                            trait_kind: watched_trait_kind,
                            eval_set: watched_eval_set,
                            signature: watched_signature,
                            decision,
                        },
                    );
                }
                // Now that the board watches it, the attempt is settled Created.
                if let (Some(attempt), false) = (attempt, repeated) {
                    let state = super::attempt_ledger::AttemptState::Created { handle: handle.clone() };
                    if let Err(error) = this.attempts.settle(attempt, state).await {
                        continuum_base::probe!(
                            class = "genome.attempt.unsettled",
                            attempt = %attempt,
                            error = %error,
                            "the job is created and watched but the ledger could not record it: the attempt reconciles as unknown"
                        );
                    }
                }
                Ok(JobCreateOutcome {
                    success: true,
                    result: Some(JobCreateResult {
                        handle,
                        selected_provider,
                    }),
                    error: None,
                    error_kind: None,
                    took: None,
                    capacity_wait: None,
                })
            }
            Err(e) => Ok(JobCreateOutcome {
                success: false,
                result: None,
                error: Some(e.to_string()),
                error_kind: Some(fine_tuning_error_kind(&e).to_string()),
                took: None,
                capacity_wait: match e {
                    crate::genome::fine_tuning::FineTuningError::CapacityParked(wait) => Some(wait),
                    _ => None,
                },
            }),
        }
    }
}

#[cfg(test)]
mod tests;
