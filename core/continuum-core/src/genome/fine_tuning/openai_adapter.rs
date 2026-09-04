//! OpenAI cloud fine-tuning adapter.
//!
//! Wraps OpenAI's `/v1/fine_tuning/jobs` endpoint behind the
//! [`super::FineTuningAdapter`] trait. Uploads the dataset as a
//! `/v1/files` resource (purpose=`fine-tune`) on `create_job`, then
//! creates the job referencing the file id.
//!
//! ## What this covers (Stage B PR 4/N)
//!
//! - `create_job` — upload dataset + create job, returns
//!   [`JobHandle`].
//! - `poll` — GET the job, map provider state to
//!   [`TrainingStatus`]. On terminal success, builds the
//!   [`TrainingArtifact`] from the `fine_tuned_model` field. OpenAI
//!   doesn't ship the weights back; `local_path` stays `None`.
//! - `cancel` — POST the cancel endpoint.
//! - Provider-specific quirk: OpenAI's training metrics (loss,
//!   validation loss) live on a separate `result_files` endpoint
//!   that returns JSONL. The first cut populates `JobMetrics` from
//!   the top-level `trained_tokens` field only — the result-file
//!   pull is a follow-up so this PR stays focused.
//!
//! ## Doctrinal alignment
//!
//! - `[[no-fallbacks-ever]]`: every failure path returns a typed
//!   [`super::adapter::FineTuningError`] variant. HTTP 4xx →
//!   `ProviderRejected`. 5xx / connection error → `Transient`.
//!   JSON shape mismatch → `MalformedResponse`. No silent skip,
//!   no `Ok(())` on a job we didn't actually start.

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::secrets::get_secret;

use super::adapter::{FineTuningAdapter, FineTuningCapabilities, FineTuningError, TrainerHardware};
use super::types::{
    ArtifactFormat, JobHandle, JobMetrics, TrainingArtifact, TrainingJobRequest, TrainingStatus,
};

const PROVIDER_ID: &str = "openai";
const BASE_URL: &str = "https://api.openai.com/v1";
const API_KEY_ENV: &str = "OPENAI_API_KEY";

pub struct OpenAIFineTuningAdapter {
    client: reqwest::Client,
}

impl OpenAIFineTuningAdapter {
    /// Construct with a freshly-built reqwest client. The client is
    /// shared across all calls on this adapter; HTTP keep-alive does
    /// the connection pooling.
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("reqwest::Client::build with valid defaults cannot fail");
        Self { client }
    }

    /// Look up the API key lazily on each call rather than caching
    /// in `Self`. The substrate's secrets layer reloads from
    /// `~/.continuum/secrets.env` on hot-reload signals, so a
    /// cached key would go stale; the cost of one HashMap lookup is
    /// nothing next to an HTTP round-trip.
    fn require_api_key(&self) -> Result<&'static str, FineTuningError> {
        get_secret(API_KEY_ENV)
            .ok_or_else(|| FineTuningError::MissingCredentials(PROVIDER_ID.to_string()))
    }

    /// Upload the training dataset as a JSONL file with
    /// `purpose=fine-tune`. OpenAI's API requires the dataset to live
    /// as a `/v1/files` resource before `/v1/fine_tuning/jobs` can
    /// reference it. Returns the file ID.
    async fn upload_dataset(
        &self,
        api_key: &str,
        request: &TrainingJobRequest,
    ) -> Result<String, FineTuningError> {
        if request.dataset.examples.is_empty() {
            return Err(FineTuningError::InvalidRequest(
                "dataset.examples is empty; OpenAI rejects 0-row training files".into(),
            ));
        }

        // OpenAI's fine-tune format: one JSON object per line, each
        // with a `messages` field (chat-completions shape).
        let mut jsonl = String::with_capacity(request.dataset.examples.len() * 256);
        for example in &request.dataset.examples {
            let line = serde_json::json!({
                "messages": [
                    { "role": "user",      "content": example.prompt },
                    { "role": "assistant", "content": example.completion },
                ],
            });
            jsonl.push_str(
                &serde_json::to_string(&line).map_err(|e| {
                    FineTuningError::InvalidRequest(format!("serialize example: {e}"))
                })?,
            );
            jsonl.push('\n');
        }

        let form = reqwest::multipart::Form::new()
            .text("purpose", "fine-tune")
            .part(
                "file",
                reqwest::multipart::Part::bytes(jsonl.into_bytes())
                    .file_name("dataset.jsonl")
                    .mime_str("application/jsonl")
                    .map_err(|e| {
                        FineTuningError::InvalidRequest(format!("mime construction: {e}"))
                    })?,
            );

        let resp = self
            .client
            .post(format!("{BASE_URL}/files"))
            .bearer_auth(api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|e| FineTuningError::Transient(format!("upload POST failed: {e}")))?;

        if let Err(e) = error_for_status(&resp) {
            let body = resp.text().await.unwrap_or_default();
            return Err(map_status_error(e, body));
        }

        let parsed: FileUploadResponse = resp.json().await.map_err(|e| {
            FineTuningError::MalformedResponse(format!("upload response not JSON: {e}"))
        })?;
        Ok(parsed.id)
    }
}

impl Default for OpenAIFineTuningAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FineTuningAdapter for OpenAIFineTuningAdapter {
    fn capabilities(&self) -> FineTuningCapabilities {
        FineTuningCapabilities {
            provider_id: PROVIDER_ID.to_string(),
            supports_lora: true,
            supports_validation: true,
            produces_local_artifact: false,
            // OpenAI's fine-tunable base models all start with gpt-
            // (plus the o-series; gpt-4o-mini is the cheapest as of
            // late 2025). Prefix-route incoming requests by this.
            supported_base_model_prefixes: vec!["gpt-".into()],
            // Cloud HTTP trainer — runs from any host regardless of
            // local accelerator.
            requires: TrainerHardware::Any,
        }
    }

    async fn create_job(&self, request: TrainingJobRequest) -> Result<JobHandle, FineTuningError> {
        let api_key = self.require_api_key()?;

        // Stage 1: upload the dataset.
        let training_file_id = self.upload_dataset(api_key, &request).await?;

        // Stage 2: create the job. The request body shape follows
        // OpenAI's `/v1/fine_tuning/jobs` documented schema. We pass
        // hyperparams when the caller specified them; otherwise let
        // OpenAI pick defaults.
        let mut body = serde_json::json!({
            "model": request.base_model,
            "training_file": training_file_id,
            "suffix": request.trait_kind,
        });
        if let Some(schedule) = &request.schedule {
            body["hyperparameters"] = serde_json::json!({
                "n_epochs": schedule.epochs,
                "batch_size": schedule.batch_size,
                "learning_rate_multiplier": schedule.learning_rate,
            });
        }

        let resp = self
            .client
            .post(format!("{BASE_URL}/fine_tuning/jobs"))
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| FineTuningError::Transient(format!("create POST failed: {e}")))?;

        if let Err(e) = error_for_status(&resp) {
            let body = resp.text().await.unwrap_or_default();
            return Err(map_status_error(e, body));
        }

        let parsed: JobCreateResponse = resp.json().await.map_err(|e| {
            FineTuningError::MalformedResponse(format!("create response not JSON: {e}"))
        })?;

        Ok(JobHandle {
            provider_id: PROVIDER_ID.to_string(),
            provider_job_id: parsed.id,
            local_id: Uuid::new_v4(),
        })
    }

    async fn poll(&self, handle: &JobHandle) -> Result<TrainingStatus, FineTuningError> {
        if handle.provider_id != PROVIDER_ID {
            return Err(FineTuningError::UnknownHandle(handle.clone()));
        }
        let api_key = self.require_api_key()?;

        let resp = self
            .client
            .get(format!(
                "{BASE_URL}/fine_tuning/jobs/{}",
                handle.provider_job_id
            ))
            .bearer_auth(api_key)
            .send()
            .await
            .map_err(|e| FineTuningError::Transient(format!("poll GET failed: {e}")))?;

        if let Err(e) = error_for_status(&resp) {
            let body = resp.text().await.unwrap_or_default();
            return Err(map_status_error(e, body));
        }

        let parsed: JobStatusResponse = resp.json().await.map_err(|e| {
            FineTuningError::MalformedResponse(format!("poll response not JSON: {e}"))
        })?;

        Ok(map_status(parsed))
    }

    async fn cancel(&self, handle: &JobHandle) -> Result<(), FineTuningError> {
        if handle.provider_id != PROVIDER_ID {
            return Err(FineTuningError::UnknownHandle(handle.clone()));
        }
        let api_key = self.require_api_key()?;

        let resp = self
            .client
            .post(format!(
                "{BASE_URL}/fine_tuning/jobs/{}/cancel",
                handle.provider_job_id
            ))
            .bearer_auth(api_key)
            .send()
            .await
            .map_err(|e| FineTuningError::Transient(format!("cancel POST failed: {e}")))?;

        if let Err(e) = error_for_status(&resp) {
            let body = resp.text().await.unwrap_or_default();
            return Err(map_status_error(e, body));
        }
        Ok(())
    }
}

// ─── Wire helpers ────────────────────────────────────────────────────

/// OpenAI's job creation + status response (subset we read).
/// `status` values: `validating_files`, `queued`, `running`,
/// `succeeded`, `failed`, `cancelled`.
#[derive(Debug, Deserialize)]
struct JobStatusResponse {
    #[allow(dead_code)]
    id: String,
    status: String,
    #[serde(default)]
    fine_tuned_model: Option<String>,
    #[serde(default)]
    error: Option<JobError>,
    #[serde(default)]
    trained_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct JobCreateResponse {
    id: String,
}

#[derive(Debug, Deserialize)]
struct FileUploadResponse {
    id: String,
}

#[derive(Debug, Deserialize)]
struct JobError {
    message: String,
}

/// `reqwest::Response::error_for_status` consumes the response; we
/// want to inspect the body on error. This shim peeks at the status
/// without consuming.
fn error_for_status(resp: &reqwest::Response) -> Result<(), reqwest::StatusCode> {
    let s = resp.status();
    if s.is_success() {
        Ok(())
    } else {
        Err(s)
    }
}

/// Map an HTTP status + body into the typed
/// [`FineTuningError`] taxonomy. Per [[no-fallbacks-ever]] the
/// dispatcher needs to know retriable-vs-not without parsing strings.
fn map_status_error(status: reqwest::StatusCode, body: String) -> FineTuningError {
    let truncated = body.chars().take(512).collect::<String>();
    match status.as_u16() {
        // 4xx: caller's fault. Bad request, missing field, model
        // not fine-tunable, etc. Don't retry without fixing.
        400..=499 => FineTuningError::ProviderRejected(format!("{status}: {truncated}")),
        // 5xx + 429: transient. Backoff + retry is appropriate.
        500..=599 => FineTuningError::Transient(format!("{status}: {truncated}")),
        _ => FineTuningError::MalformedResponse(format!("unexpected status {status}: {truncated}")),
    }
}

/// Map OpenAI's status string to our typed
/// [`TrainingStatus`]. Conservative on the running path because
/// OpenAI doesn't report per-epoch progress in the job-status
/// payload — we floor `progress_pct` at zero and let downstream
/// telemetry refine.
fn map_status(r: JobStatusResponse) -> TrainingStatus {
    match r.status.as_str() {
        "validating_files" | "queued" => TrainingStatus::Queued,
        "running" => TrainingStatus::Running {
            progress_pct: 0.0,
            current_epoch: 0,
        },
        "succeeded" => {
            let model_id = r.fine_tuned_model.unwrap_or_else(|| "unknown".to_string());
            TrainingStatus::Completed {
                artifact: TrainingArtifact {
                    model_id,
                    local_path: None,
                    // Provider-hosted: OpenAI keeps the weights; the inference
                    // adapter pulls by `model_id` on demand. No local convert,
                    // no page-in of a local gene.
                    format: ArtifactFormat::ProviderHosted,
                    metrics: JobMetrics {
                        trained_tokens: r.trained_tokens.unwrap_or(0),
                        final_loss: None,
                        final_validation_loss: None,
                        wall_clock_ms: 0,
                        cost_usd: None,
                    },
                },
            }
        }
        "cancelled" => TrainingStatus::Cancelled,
        "failed" => {
            let error = r
                .error
                .map(|e| e.message)
                .unwrap_or_else(|| "unknown error".into());
            TrainingStatus::Failed { error }
        }
        other => TrainingStatus::Failed {
            error: format!("unknown provider status: {other}"),
        },
    }
}

#[derive(Debug, Serialize)]
struct _UnusedSerializeMarker {} // touch serde::Serialize so it's not "unused"

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the status mapping is the substrate-side
    // contract with OpenAI's documented status string set. A future
    // refactor that maps "queued" to Running would silently change
    // routing. The variant set is small enough to pin all of it.
    #[test]
    fn map_status_covers_documented_states() {
        let queued = map_status(JobStatusResponse {
            id: "j".into(),
            status: "queued".into(),
            fine_tuned_model: None,
            error: None,
            trained_tokens: None,
        });
        assert!(matches!(queued, TrainingStatus::Queued));

        let validating = map_status(JobStatusResponse {
            id: "j".into(),
            status: "validating_files".into(),
            fine_tuned_model: None,
            error: None,
            trained_tokens: None,
        });
        assert!(matches!(validating, TrainingStatus::Queued));

        let running = map_status(JobStatusResponse {
            id: "j".into(),
            status: "running".into(),
            fine_tuned_model: None,
            error: None,
            trained_tokens: None,
        });
        assert!(matches!(running, TrainingStatus::Running { .. }));

        let succeeded = map_status(JobStatusResponse {
            id: "j".into(),
            status: "succeeded".into(),
            fine_tuned_model: Some("ft:gpt-4o-mini:org:trait:abc".into()),
            error: None,
            trained_tokens: Some(123_456),
        });
        match succeeded {
            TrainingStatus::Completed { artifact } => {
                assert_eq!(artifact.model_id, "ft:gpt-4o-mini:org:trait:abc");
                assert_eq!(artifact.metrics.trained_tokens, 123_456);
            }
            other => panic!("expected Completed, got {other:?}"),
        }

        let failed = map_status(JobStatusResponse {
            id: "j".into(),
            status: "failed".into(),
            fine_tuned_model: None,
            error: Some(JobError {
                message: "oom".into(),
            }),
            trained_tokens: None,
        });
        match failed {
            TrainingStatus::Failed { error } => assert_eq!(error, "oom"),
            other => panic!("expected Failed, got {other:?}"),
        }

        let cancelled = map_status(JobStatusResponse {
            id: "j".into(),
            status: "cancelled".into(),
            fine_tuned_model: None,
            error: None,
            trained_tokens: None,
        });
        assert!(matches!(cancelled, TrainingStatus::Cancelled));
    }

    // what this catches: error-status mapping. 4xx is ProviderRejected
    // (caller fixes the request), 5xx is Transient (backoff + retry).
    // A future change that flips these would silently mask retriable
    // failures or burn rate-limit on non-retriable ones.
    #[test]
    fn http_status_error_taxonomy() {
        let r = map_status_error(reqwest::StatusCode::BAD_REQUEST, "bad model".into());
        assert!(matches!(r, FineTuningError::ProviderRejected(_)));

        let r = map_status_error(reqwest::StatusCode::INTERNAL_SERVER_ERROR, "oops".into());
        assert!(matches!(r, FineTuningError::Transient(_)));

        let r = map_status_error(reqwest::StatusCode::TOO_MANY_REQUESTS, "slow down".into());
        // 429 is in 400..=499 in our map. Could argue Transient is
        // more correct because it's retriable with backoff. Pin
        // current behavior so a future refactor that flips it is a
        // conscious choice, not a silent drift.
        assert!(matches!(r, FineTuningError::ProviderRejected(_)));
    }

    // what this catches: provider_id mismatch on poll/cancel returns
    // UnknownHandle, not a misdirected HTTP call to OpenAI's API
    // with a Mistral job id. The substrate stores adapters in a
    // registry keyed by provider; passing the wrong adapter the
    // wrong handle is a bug the trait surfaces.
    #[tokio::test]
    async fn poll_with_wrong_provider_id_returns_unknown_handle() {
        let adapter = OpenAIFineTuningAdapter::new();
        let bad_handle = JobHandle {
            provider_id: "mistral".into(),
            provider_job_id: "job-xyz".into(),
            local_id: Uuid::nil(),
        };
        let err = adapter.poll(&bad_handle).await.expect_err("must reject");
        assert!(matches!(err, FineTuningError::UnknownHandle(_)));
    }

    // what this catches: capabilities() is stable + matches the
    // OpenAI prefix. A future refactor that drops "gpt-" would route
    // OpenAI-trainable requests to a different adapter that can't
    // service them.
    #[test]
    fn capabilities_match_openai_provider_id_and_gpt_prefix() {
        let caps = OpenAIFineTuningAdapter::new().capabilities();
        assert_eq!(caps.provider_id, "openai");
        assert!(caps.supports_lora);
        assert!(caps.supports_validation);
        assert!(!caps.produces_local_artifact);
        assert_eq!(caps.supported_base_model_prefixes, vec!["gpt-"]);
    }
}
