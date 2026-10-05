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
}

/// A decision's action that creates no job. The caller (the trigger) keeps the
/// examples in her bucket for every arm: they are the competence's evidence, no gene
/// was trained on them, and the fill after the pending thing settles decides again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/genome/Took.ts")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Took {
    /// `Decision::Join`: a job of hers already trains this competence (its local id).
    Joined {
        #[ts(type = "string")]
        job: uuid::Uuid,
    },
    /// `Decision::Await`: a gene for this competence is on trial in her work; her cards
    /// decide it.
    Awaited {
        #[ts(type = "string")]
        trial: uuid::Uuid,
    },
    /// `Decision::Reuse`: a gene that already existed was adopted for trial, no training.
    Reused {
        #[ts(type = "string")]
        trial: uuid::Uuid,
        gene: GeneRef,
    },
    /// Her trial file could not be read: the bucket holds until it can, since nothing may
    /// dispatch beside a trial nobody can see. Set by the trigger's gate, never by a
    /// decision (job-create refuses with `TrialFileUnreadable` instead).
    TrialFileUnreadable,
}

/// A reuse that could not pull its gene: no job, the bucket keeps its examples, the next
/// fill retries (the trigger decodes it as retryable, never as recovery).
pub const ERROR_KIND_REUSE_PULL_FAILED: &str = "ReusePullFailed";
/// A reuse whose gene was not adopted (manifest, trial file, or already decided): same.
pub const ERROR_KIND_REUSE_ADOPT_FAILED: &str = "ReuseAdoptFailed";
/// Her trial file could not be read: nothing decided blind; the next fill retries.
pub const ERROR_KIND_TRIAL_FILE_UNREADABLE: &str = "TrialFileUnreadable";

/// The decision could not be made: a typed refusal the trigger retries on the next fill,
/// probed so a bucket that keeps refusing is readable.
fn refused_decision(error_kind: &str, request: &TrainingJobRequest, error: &str) -> Result<JobCreateOutcome, crate::sdk_codegen::CommandError> {
    crate::probe!(
        class = "genome.decision.refused",
        persona = %request.persona_name,
        trait_kind = %request.trait_kind,
        error_kind,
        error,
        "a full bucket could not decide: no job, the bucket keeps its examples, the next fill retries"
    );
    Ok(JobCreateOutcome { success: false, result: None, error: Some(error.to_string()), error_kind: Some(error_kind.to_string()), took: None })
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
                });
            }
            (None, false) => {}
        }

        // 1. Coordinator picks a provider. Any CoordinatorError (no capable
        //    adapter, preference unsatisfiable) surfaces as success=false with the
        //    error's diagnostic text.
        let (selected_provider, adapter) = match this
            .coordinator
            .select(&p.request, p.preferred_provider.as_deref())
        {
            Ok(pair) => pair,
            Err(e) => {
                return Ok(JobCreateOutcome {
                    success: false,
                    result: None,
                    error: Some(e.to_string()),
                    error_kind: None,
                    took: None,
                });
            }
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
            let corpus = crate::forge::recipe::CorpusRef {
                name: p
                    .dataset_name
                    .clone()
                    .unwrap_or_else(|| format!("inline:{}", p.request.trait_kind)), // inline datasets have no on-disk name; the trait names the mint
                content_hash: crate::persona::inbox_admission::content_hash_sha256(&joined),
                size_bytes: joined.len() as u64,
                source_url: None,
            };
            let embedder = crate::cognition::embedding::resolve_recall_embedder_local().await;
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0); // pre-epoch clock: mint stamps 0 rather than refusing the gene its training
            match crate::genome::signature::GeneSignature::mint(&texts, corpus, &embedder, now_ms)
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
        let decision = match watched_signature.as_ref() {
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
                Err(refusal) => return refused_decision("TrialFileUnreadable", &p.request, &format!("her trial file has no place: {refusal}")),
            };
            let store_path = adoption.manifest().with_file_name("signatures.json");
            let store = crate::genome::signature::SignatureStore::load_at(&store_path)
                .unwrap_or_default(); // unwrap_or_default: no store yet = nothing near, which decides "mint" honestly
            // HER TRIALS, read once and LOUD when unreadable (Cormac on #4794): a corrupt
            // trial file read as empty would let a fork or a mint go ahead beside her open
            // trial, and would offer a retired gene back. Nothing is decided blind.
            let trials = match adoption.trials().load() {
                Ok(t) => t,
                Err(e) => return refused_decision("TrialFileUnreadable", &p.request, &format!("her trial file could not be read: {e}")),
            };
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0); // pre-epoch clock: every open trial then reads as live, the conservative side (held, never forked beside)
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
            let board_jobs = crate::genome::fine_tuning::TrainingJobBoard::global().snapshot();
            #[cfg(test)]
            let board_jobs = this.test_job_board.as_ref().snapshot();
            let in_flight = nearest_pending(
                &competence,
                &sig.embedder,
                board_jobs
                    .iter()
                    .filter(|j| j.persona_id == watched_persona_id && j.base_model == p.request.base_model)
                    .filter_map(|j| j.signature.as_ref().map(|s| (j.handle.local_id, s))),
            );
            // A GENE ALREADY ON TRIAL for this competence: ONE matching rule, the bucket's
            // key (her, this trait, this base), the same rule the trigger holds the bucket
            // by (Cormac on #4794, point 4: two rules churned a bucket between them). The
            // signature similarity rides along as a measurement, never as the decision; a
            // trial past its window is ended already (`is_live`).
            let on_trial = trials
                .iter()
                .filter(hers)
                .filter(|t| t.base_model_id == p.request.base_model && t.alias == p.request.trait_kind && t.is_live(now_ms))
                .map(|t| crate::genome::competence::Pending {
                    id: t.id,
                    similarity: store.by_path.get(&t.path.display().to_string()).and_then(|s| s.similarity_in(&sig.embedder, &competence.centroid)).unwrap_or(1.0), // 1.0 = the key already says it is this competence; the number is a measurement when the signature exists
                })
                .max_by(|a, b| a.similarity.total_cmp(&b.similarity));
            let decision = decide_with_pending(Surprise::NotYetMeasured, nearest.as_ref(), in_flight.as_ref(), on_trial.as_ref());
            crate::probe!(
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
                surprise = "not_measured",
                "a full bucket decided: join, await, reuse, fork or mint, against her store, her trials and the hub"
            );
            Some(decision)
            }
        };

        // THE ACTIONS THAT CREATE NO JOB. The caller (the trigger) keeps the examples in
        // her bucket for each of them: they are the competence's evidence, nothing trained
        // on them, and the fill after the pending thing settles decides again.
        let examples = p.request.dataset.examples.len() as u64;
        let without_job = |took: Took| Ok(JobCreateOutcome { success: true, result: None, error: None, error_kind: None, took: Some(took) });
        match &decision {
            // A competence already training is never minted twice.
            Some(Decision::Join { job, similarity }) => {
                crate::probe!(
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
                crate::probe!(
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
            // when it lives on the hub, then registered dormant and trialled on her cards.
            // No training. Her work decides it; a retired one is never offered again.
            Some(Decision::Reuse { gene, similarity }) => {
                let refused = |error_kind: &str, error: String| {
                    Ok(JobCreateOutcome { success: false, result: None, error: Some(error), error_kind: Some(error_kind.into()), took: None })
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
                                crate::probe!(
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
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0); // pre-epoch clock: the trial opens at 0 rather than refusing her the gene
                let adopted = adoption
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(|a| a.adopt(watched_persona_id, &p.request.trait_kind, &path, &p.request.base_model, now_ms));
                match adopted {
                    Ok(trial) => {
                        crate::probe!(
                            class = "genome.reused",
                            persona = %p.request.persona_name,
                            trait_kind = %p.request.trait_kind,
                            examples,
                            gene = %gene,
                            path = %path.display(),
                            similarity = *similarity,
                            trial = %trial.id,
                            share_milli = trial.share_milli as u64,
                            "an existing gene carries this competence: adopted for trial on her cards, no training"
                        );
                        return without_job(Took::Reused { trial: trial.id, gene: gene.clone() });
                    }
                    Err(refusal) => {
                        crate::probe!(
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
                crate::probe!(
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
            Some(Decision::Mint) | Some(Decision::Nothing { .. }) | None => {}
        }

        // 2. Adapter creates the job. FineTuningError carries a stable errorKind
        //    slug callers branch on for retry-vs-surface.
        match adapter.create_job(p.request).await {
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
                Ok(JobCreateOutcome {
                    success: true,
                    result: Some(JobCreateResult {
                        handle,
                        selected_provider,
                    }),
                    error: None,
                    error_kind: None,
                    took: None,
                })
            }
            Err(e) => Ok(JobCreateOutcome {
                success: false,
                result: None,
                error: Some(e.to_string()),
                error_kind: Some(fine_tuning_error_kind(&e).to_string()),
                took: None,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::genome::test_support::{registry_with, request_for};
    use crate::sdk_codegen::{ActionCommand, Ctx};

    fn cmd(ids: &[&'static str]) -> GenomeJobCreate {
        let registry = registry_with(ids);
        GenomeJobCreate {
            coordinator: Arc::new(FineTuningCoordinator::new(registry)),
            test_job_board: Arc::new(crate::genome::fine_tuning::TrainingJobBoard::default()),
            test_artifacts: Arc::new(tempfile::tempdir().unwrap()),
        }
    }

    /// The decision's world for one test: a recording adapter (so a created job is
    /// visible and its request readable), the test artifacts dir (manifest, trial file
    /// and signature store beside each other, as in production), and a signature for a
    /// gene at `gene_path` minted from the SAME texts the request carries, so the store's
    /// nearest gene is this competence exactly (similarity 1.0 in the lexical space the
    /// tests embed in).
    struct DecisionWorld {
        command: GenomeJobCreate,
        adapter: Arc<crate::genome::fine_tuning::RecordingFineTuningAdapter>,
        board: Arc<crate::genome::fine_tuning::TrainingJobBoard>,
        artifacts: Arc<tempfile::TempDir>,
        persona: uuid::Uuid,
        request: TrainingJobRequest,
        gene_path: std::path::PathBuf,
    }

    async fn decision_world() -> DecisionWorld {
        use crate::genome::fine_tuning::{FineTuningRegistry, RecordingFineTuningAdapter, TrainingJobBoard, RECORDING_BASE_PREFIX};
        let artifacts = Arc::new(tempfile::tempdir().unwrap());
        let board = Arc::new(TrainingJobBoard::with_test_storage(artifacts.clone()));
        let adapter = Arc::new(RecordingFineTuningAdapter::new());
        let registry = Arc::new(FineTuningRegistry::new());
        registry.register(adapter.clone());
        let command = GenomeJobCreate {
            coordinator: Arc::new(FineTuningCoordinator::new(registry)),
            test_job_board: board.clone(),
            test_artifacts: artifacts.clone(),
        };
        let persona = uuid::Uuid::from_u128(0x17dc0a7b);
        let mut request = request_for(RECORDING_BASE_PREFIX);
        request.persona_id = persona;
        let texts: Vec<String> = request.dataset.examples.iter().map(|ex| format!("{}\n{}", ex.prompt, ex.completion)).collect();
        let embedder = crate::cognition::embedding::resolve_recall_embedder_local().await;
        let sig = crate::genome::signature::GeneSignature::mint(
            &texts,
            crate::forge::recipe::CorpusRef { name: "near".into(), content_hash: "sha256:near".into(), size_bytes: 1, source_url: None },
            &embedder,
            1,
        )
        .await
        .expect("test: a signature mints in the lexical space");
        let gene_path = artifacts.path().join("genes").join("near.gguf");
        crate::genome::signature::SignatureStore::stamp_at(&artifacts.path().join("signatures.json"), &gene_path.display().to_string(), sig)
            .expect("test: the store takes the signature");
        DecisionWorld { command, adapter, board, artifacts, persona, request, gene_path }
    }

    impl DecisionWorld {
        fn trials(&self) -> crate::genome::gene_trial::GeneTrials {
            crate::genome::gene_trial::GeneTrials::at(self.artifacts.path().join("trials.json"))
        }
        fn manifest(&self) -> std::path::PathBuf {
            self.artifacts.path().join("adapters.json")
        }
        async fn fill(&self) -> JobCreateOutcome {
            self.command
                .run(
                    &Ctx::default(),
                    JobCreateParams { trigger_dispatch_id: None, request: self.request.clone(), preferred_provider: None, dataset_name: None },
                )
                .await
                .unwrap()
        }
        fn open_trial(&self, state: crate::genome::gene_trial::TrialState) -> crate::genome::gene_trial::GeneTrial {
            let trials = self.trials();
            let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
            let t = trials.open(self.persona, &self.request.trait_kind, &self.gene_path, &self.request.base_model, now_ms).unwrap();
            if state != crate::genome::gene_trial::TrialState::Trial {
                let mut all = trials.load().unwrap();
                all[0].state = state;
                trials.save_for_test(&all).unwrap();
            }
            t
        }
    }

    // what this catches (card 17dc0a7b, Reuse ACTS): a gene near her competence that her
    // work does not hold is adopted for trial, no job created: the trial file has the row,
    // the serving manifest has the path, the outcome names both, and the adapter saw
    // nothing.
    #[tokio::test]
    async fn a_near_gene_she_lacks_is_adopted_for_trial_and_nothing_trains() {
        let w = decision_world().await;
        let out = w.fill().await;
        assert!(out.success, "{out:?}");
        let Some(Took::Reused { trial, gene }) = out.took else { panic!("reuse expected: {out:?}") };
        assert_eq!(gene, GeneRef::Local { path: w.gene_path.clone() });
        let rows = w.trials().load().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].id, rows[0].persona_id, rows[0].state), (trial, w.persona, crate::genome::gene_trial::TrialState::Trial));
        let registered = crate::forge::adapter_manifest::load_from(&w.manifest()).unwrap();
        assert!(registered.iter().any(|a| a.path == w.gene_path), "the gene serves, dormant: {registered:?}");
        assert_eq!(w.adapter.captured_job_count(), 0, "a reuse trains nothing");
        assert!(w.board.snapshot().is_empty());
    }

    // what this catches (Cormac on #4794, point 4: ONE matching rule): a trial open for the
    // bucket's key holds the fill as Awaited, and the fill after her verdict decides again.
    #[tokio::test]
    async fn a_trial_open_for_the_key_holds_the_fill_until_it_is_decided() {
        let w = decision_world().await;
        let t = w.open_trial(crate::genome::gene_trial::TrialState::Trial);
        let out = w.fill().await;
        assert_eq!(out.took, Some(Took::Awaited { trial: t.id }), "{out:?}");
        assert_eq!(w.adapter.captured_job_count(), 0);
    }

    // what this catches (card 17dc0a7b, Fork carries lineage): a gene her work holds
    // (promoted) near her competence is a parent: the job is created with the parent on
    // its request, and the signature the child is adopted with records it.
    #[tokio::test]
    async fn a_resident_gene_is_forked_with_its_lineage_on_the_request_and_the_signature() {
        let w = decision_world().await;
        w.open_trial(crate::genome::gene_trial::TrialState::Promoted);
        let out = w.fill().await;
        assert!(out.success && out.result.is_some() && out.took.is_none(), "a fork trains: {out:?}");
        let parent = GeneRef::Local { path: w.gene_path.clone() };
        let captured = w.adapter.captures();
        let captured = captured.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].parent, Some(parent.clone()), "the trainer is told the parent");
        let jobs = w.board.snapshot();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].signature.as_ref().and_then(|s| s.parent.clone()), Some(parent), "the child's signature carries its lineage");
        assert!(matches!(jobs[0].decision, Some(Decision::Fork { .. })), "{:?}", jobs[0].decision);
    }

    // what this catches (Cormac on #4794, point 2): a gene her work retired (or left
    // unjudged) is never a candidate again and is never adopted again: the decision mints
    // past it, and a direct adoption refuses before touching the manifest.
    #[tokio::test]
    async fn a_decided_gene_is_never_offered_or_adopted_again() {
        let w = decision_world().await;
        w.open_trial(crate::genome::gene_trial::TrialState::Retired);
        let out = w.fill().await;
        assert!(out.result.is_some() && out.took.is_none(), "a retired gene is not reused: {out:?}");
        assert!(matches!(w.board.snapshot()[0].decision, Some(Decision::Mint)));
        let adoption = Adoption::at(w.manifest(), w.trials());
        let refused = adoption.adopt(w.persona, &w.request.trait_kind, &w.gene_path, &w.request.base_model, 2);
        assert_eq!(refused, Err(crate::genome::gene_trial::AdoptRefusal::AlreadyDecided(crate::genome::gene_trial::TrialState::Retired)));
        assert!(!w.manifest().exists(), "a refused adoption never registered the gene");
    }

    // what this catches (Cormac on #4794, point 5): a trial file that cannot be read
    // refuses the decision loudly and retryably; nothing is minted or reused blind.
    #[tokio::test]
    async fn an_unreadable_trial_file_refuses_the_decision_rather_than_deciding_blind() {
        let w = decision_world().await;
        std::fs::write(w.artifacts.path().join("trials.json"), b"{not json").unwrap();
        let out = w.fill().await;
        assert!(!out.success);
        assert_eq!(out.error_kind.as_deref(), Some(ERROR_KIND_TRIAL_FILE_UNREADABLE), "{out:?}");
        assert_eq!(w.adapter.captured_job_count(), 0);
    }

    // what this catches: name/access wiring — creating a training job spends compute +
    // touches provider credentials, so it lives on the Privileged surface, not AiSafe.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(GenomeJobCreate::NAME, "genome/job-create");
        assert!(matches!(
            GenomeJobCreate::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }

    // what this catches: end-to-end happy path. The verb dispatches through the
    // coordinator to the registered adapter and returns success=true + handle +
    // selectedProvider. A refactor that changes the outcome shape breaks every caller.
    #[tokio::test]
    async fn happy_path_returns_handle_and_selected_provider() {
        let out = cmd(&["openai"])
            .run(
                &Ctx::default(),
                JobCreateParams {
                    trigger_dispatch_id: None,
                    request: request_for("gpt-4o-mini"),
                    preferred_provider: None,
                    dataset_name: None,
                },
            )
            .await
            .unwrap();
        assert!(out.success);
        let result = out.result.expect("success carries a result");
        assert_eq!(result.selected_provider, "openai");
        assert_eq!(result.handle.provider_id, "openai");
        assert_eq!(result.handle.provider_job_id, "openai-job-1");
    }

    // What this catches (278afa6c): the real command must register to its owned
    // ledger and pass fixture output to the provider, preserving explicit paths.
    #[tokio::test]
    async fn job_fixture_owns_ledger_and_supplies_artifact_directory() {
        use crate::genome::fine_tuning::{
            job_board::DispatchLookup, FineTuningRegistry, RecordingFineTuningAdapter,
            TrainingJobBoard, RECORDING_BASE_PREFIX,
        };
        let artifacts = Arc::new(tempfile::tempdir().unwrap());
        let ledger = artifacts.path().join("jobs-ledger.jsonl");
        let board = Arc::new(TrainingJobBoard::with_test_storage(artifacts.clone()));
        let adapter = Arc::new(RecordingFineTuningAdapter::new());
        let registry = Arc::new(FineTuningRegistry::new());
        registry.register(adapter.clone());
        let command = GenomeJobCreate {
            coordinator: Arc::new(FineTuningCoordinator::new(registry)),
            test_job_board: board.clone(),
            test_artifacts: artifacts.clone(),
        };
        let explicit_output = artifacts.path().join("explicit");
        for output in [None, Some(explicit_output.clone())] {
            let mut request = request_for(RECORDING_BASE_PREFIX);
            request.local_artifact_dir = output;
            // Each round is its own citizen: the first round's job is still on the board,
            // and a second fill of the same competence by the same persona JOINS it (#4791)
            // instead of creating the job this round asserts on.
            request.persona_id = uuid::Uuid::new_v4();
            let dispatch_id = uuid::Uuid::new_v4();
            let outcome = command
                .run(
                    &Ctx::default(),
                    JobCreateParams {
                        trigger_dispatch_id: Some(dispatch_id),
                        request,
                        preferred_provider: None,
                        dataset_name: None,
                    },
                )
                .await
                .unwrap();
            assert!(outcome.success, "{outcome:?}");
            let handle = outcome.result.unwrap().handle;
            assert!(
                matches!(board.lookup_trigger_dispatch(dispatch_id, 0).unwrap(),
                DispatchLookup::Observed(ref observed) if observed.local_id == handle.local_id)
            );
            // A second board can find the receipt only from this fixture's journal.
            let replay = TrainingJobBoard::with_ledger(Some(ledger.clone()));
            assert!(
                matches!(replay.lookup_trigger_dispatch(dispatch_id, 0).unwrap(),
                DispatchLookup::Observed(ref observed) if observed.local_id == handle.local_id)
            );
        }
        let captures = adapter.captures();
        let captures = captures.lock().unwrap();
        assert_eq!(captures.len(), 2);
        assert_eq!(
            captures[0].local_artifact_dir.as_deref(),
            Some(artifacts.path())
        );
        assert_eq!(
            captures[1].local_artifact_dir.as_deref(),
            Some(explicit_output.as_path())
        );
    }

    // what this catches: empty registry → success=false with the NoCapableAdapter
    // text, NOT a transport Err. The outcome-as-data contract: expected domain
    // failures come through success=false, not an Err that would read as a substrate
    // dispatch fault.
    #[tokio::test]
    async fn empty_registry_returns_no_capable_outcome() {
        let out = cmd(&[])
            .run(
                &Ctx::default(),
                JobCreateParams {
                    trigger_dispatch_id: None,
                    request: request_for("gpt-4o-mini"),
                    preferred_provider: None,
                    dataset_name: None,
                },
            )
            .await
            .unwrap();
        assert!(!out.success);
        assert!(out.result.is_none());
        assert!(out.error.unwrap().contains("no fine-tuning adapter"));
    }

    // what this catches: the dataset resolution gate — no data at all, or BOTH
    // an inline dataset and a datasetName, is rejected as success=false BEFORE any
    // adapter is selected. An empty dataset reaching an adapter would burn a real
    // training job on nothing; two datasets is ambiguous.
    #[tokio::test]
    async fn dataset_gate_rejects_empty_and_ambiguous() {
        let mut empty_req = request_for("gpt-4o-mini");
        empty_req.dataset.examples.clear();
        let out = cmd(&["openai"])
            .run(
                &Ctx::default(),
                JobCreateParams {
                    trigger_dispatch_id: None,
                    request: empty_req,
                    preferred_provider: None,
                    dataset_name: None,
                },
            )
            .await
            .unwrap();
        assert!(!out.success);
        assert!(out.error.unwrap().contains("no training data"));

        let out = cmd(&["openai"])
            .run(
                &Ctx::default(),
                JobCreateParams {
                    trigger_dispatch_id: None,
                    request: request_for("gpt-4o-mini"),
                    preferred_provider: None,
                    dataset_name: Some("also-named".into()),
                },
            )
            .await
            .unwrap();
        assert!(!out.success);
        assert!(out.error.unwrap().contains("not both"));
    }

    // what this catches: a datasetName that doesn't exist on disk fails loud with
    // the path convention + the discovery pointer (dataset/list) in the message —
    // the denial teaches the path.
    #[tokio::test]
    async fn missing_dataset_name_fails_with_guidance() {
        let mut req = request_for("gpt-4o-mini");
        req.dataset.examples.clear();
        let out = cmd(&["openai"])
            .run(
                &Ctx::default(),
                JobCreateParams {
                    trigger_dispatch_id: None,
                    request: req,
                    preferred_provider: None,
                    dataset_name: Some("no-such-dataset-xyz".into()),
                },
            )
            .await
            .unwrap();
        assert!(!out.success);
        let err = out.error.unwrap();
        assert!(
            err.contains("no-such-dataset-xyz") && err.contains("dataset/list"),
            "{err}"
        );
    }

    // what this catches: preferredProvider is honored and surfaced in
    // selectedProvider. A refactor that drops the preference would silently route to
    // whichever adapter the rank function preferred — exactly the silent-fallback the
    // coordinator's typed PreferredUnavailable exists to prevent.
    #[tokio::test]
    async fn preferred_provider_is_honored_when_capable() {
        let out = cmd(&["openai", "mistral"])
            .run(
                &Ctx::default(),
                JobCreateParams {
                    trigger_dispatch_id: None,
                    request: request_for("gpt-4o-mini"),
                    preferred_provider: Some("mistral".into()),
                    dataset_name: None,
                },
            )
            .await
            .unwrap();
        assert!(out.success);
        assert_eq!(out.result.unwrap().selected_provider, "mistral");
    }
}
