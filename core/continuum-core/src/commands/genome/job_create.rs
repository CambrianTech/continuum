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
#[derive(Debug, Clone, Serialize, TS)]
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
#[derive(Debug, Clone, Serialize, TS)]
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
}

crate::action_command! {
    /// Create a LoRA fine-tuning job. The coordinator picks a capable adapter
    /// (honoring `preferredProvider` if given and capable), the adapter starts the
    /// job, and the handle + selected provider come back. On no capable adapter, an
    /// unsatisfiable preference, or an adapter rejection, the outcome is
    /// `success=false` with the reason (and an `errorKind` slug for adapter
    /// failures) — branch on it; this is never a silent fallback.
    pub struct GenomeJobCreate { coordinator: Arc<FineTuningCoordinator> }
    name: "genome/job-create",
    access: Privileged,
    params: JobCreateParams,
    output: JobCreateOutcome,
    run(this, _ctx, p) => {
        let mut p = p;
        // 0. Resolve the dataset: by name from disk, or inline — exactly one.
        //    An empty dataset must never reach an adapter (it would "train"
        //    on nothing and burn a job slot).
        match (&p.dataset_name, p.request.dataset.examples.is_empty()) {
            (Some(name), true) => {
                let path = crate::modules::dataset::default_datasets_root()
                    .join(name)
                    .join("train.jsonl");
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
                });
            }
        };

        // Capture the genome-paging context BEFORE the request moves into the
        // adapter — the L3 completion sentinel needs exactly these four facts to run
        // the eval→page-in chain when the job completes, without re-deriving any.
        let watched_persona_id = p.request.persona_id;
        let watched_persona_name = p.request.persona_name.clone();
        let watched_base_model = p.request.base_model.clone();
        let watched_trait_kind = p.request.trait_kind.clone();
        let watched_eval_set = p.request.eval_set.clone();

        // 2. Adapter creates the job. FineTuningError carries a stable errorKind
        //    slug callers branch on for retry-vs-surface.
        match adapter.create_job(p.request).await {
            Ok(handle) => {
                // L2→L3 seam (the ONE birth-seam): every training job is born here —
                // the trigger's batch path dispatches THIS command, a direct
                // `uu genome/job-create` lands here, and so will any future caller.
                // Registering the in-flight handle on the board at this single point
                // is what lets the completion sentinel poll it, run `cognition/eval`,
                // and page the gene in on `lift > 0`. Without it the handle drops on
                // the floor and the loop stops at "trained", never "measured +
                // adopted" ([[dev-task-learning-loop-gap-map]] L3,
                // docs/genome/DEV-TASK-LOOP-CLOSURE-PLAN.md).
                crate::genome::fine_tuning::TrainingJobBoard::global().register(
                    crate::genome::fine_tuning::WatchedJob {
                        handle: handle.clone(),
                        persona_id: watched_persona_id,
                        persona_name: watched_persona_name,
                        base_model: watched_base_model,
                        trait_kind: watched_trait_kind,
                        eval_set: watched_eval_set,
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
                })
            }
            Err(e) => Ok(JobCreateOutcome {
                success: false,
                result: None,
                error: Some(e.to_string()),
                error_kind: Some(fine_tuning_error_kind(&e).to_string()),
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
        }
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
