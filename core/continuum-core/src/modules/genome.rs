//! GenomeModule — substrate-side dispatch for `genome/*` commands.
//!
//! Owns a [`FineTuningRegistry`] + [`FineTuningCoordinator`] and
//! handles three commands today:
//!
//! - `genome/job-create` — pick an adapter via the coordinator, hand
//!   the typed [`TrainingJobRequest`] to it, return the [`JobHandle`]
//!   plus the provider that was picked.
//! - `genome/job-status` — look up the adapter from the handle's
//!   `provider_id`, poll, return the typed [`TrainingStatus`].
//! - `genome/job-cancel` — same lookup, then cancel.
//!
//! Per [[commands-are-dumb-daemons-are-smart]] the module is narrow:
//! validate → look up adapter → dispatch. All the smart bits
//! (capability filtering, locality preference, the actual training
//! work) live in the coordinator + adapters.
//!
//! ## Boot
//!
//! `start_server` builds a `GenomeModule` AFTER reading credentials,
//! seeds the registry with whichever cloud adapters have keys
//! (OpenAIFineTuningAdapter when `OPENAI_API_KEY` is set, etc.) and
//! always registers [`LocalCandleFineTuner`] so the architectural
//! slot is visible to the coordinator even before #231-#233 land
//! the optimizer loop. See `ipc::mod::start_server`.

use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::genome::fine_tuning::{
    coordinator::{CoordinatorError, FineTuningCoordinator},
    FineTuningError, FineTuningRegistry, JobHandle, TrainingJobRequest,
};
use crate::runtime::{
    CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule,
};

/// Wire shape for `genome/job-create` results. The JSON serialized
/// version is what the caller (Rust ServiceModule consumer or
/// TS-side `Commands.execute('genome/job-create', ...)`) reads.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobCreateResult {
    handle: JobHandle,
    /// Which provider the coordinator picked. Surfaced so callers
    /// see the selection — important for telemetry + for operators
    /// validating that the locality preference is firing.
    selected_provider: String,
}

/// Wire shape for `genome/job-create` params. Mirrors
/// [`TrainingJobRequest`] verbatim, plus the optional
/// `preferred_provider` hint the coordinator honors (or rejects
/// with a typed error if the preference can't be satisfied).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobCreateParams {
    #[serde(flatten)]
    request: TrainingJobRequest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    preferred_provider: Option<String>,
}

/// Wire shape for `genome/job-status` + `genome/job-cancel`.
/// Single handle field; adapter lookup keys on `handle.provider_id`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobLookupParams {
    handle: JobHandle,
}

pub struct GenomeModule {
    registry: Arc<FineTuningRegistry>,
    coordinator: Arc<FineTuningCoordinator>,
}

impl GenomeModule {
    /// Construct with a pre-populated registry. The boot path
    /// (`start_server`) registers all adapters BEFORE building the
    /// module, so the coordinator's view of registered providers is
    /// stable across the module's lifetime.
    pub fn new(registry: Arc<FineTuningRegistry>) -> Self {
        let coordinator = Arc::new(FineTuningCoordinator::new(Arc::clone(&registry)));
        Self {
            registry,
            coordinator,
        }
    }

    /// Visible to tests + boot. Returns the inner registry so a
    /// caller can introspect or register adapters after
    /// construction (substrate hot-reload future work; not used in
    /// the boot path today).
    pub fn registry(&self) -> Arc<FineTuningRegistry> {
        Arc::clone(&self.registry)
    }
}

#[async_trait]
impl ServiceModule for GenomeModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "genome",
            priority: ModulePriority::Normal,
            command_prefixes: &["genome/job-"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        // No state to lazy-init. Adapters are registered by the
        // boot path; the coordinator + registry are constructed in
        // GenomeModule::new and don't require async setup.
        Ok(())
    }

    async fn handle_command(
        &self,
        command: &str,
        params: Value,
    ) -> Result<CommandResult, String> {
        match command {
            "genome/job-create" => self.handle_job_create(params).await,
            "genome/job-status" => self.handle_job_status(params).await,
            "genome/job-cancel" => self.handle_job_cancel(params).await,
            other => Err(format!("unknown genome command: {other}")),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl GenomeModule {
    async fn handle_job_create(&self, params: Value) -> Result<CommandResult, String> {
        let p: JobCreateParams =
            serde_json::from_value(params).map_err(|e| format!("invalid job-create params: {e}"))?;

        // 1. Coordinator picks a provider.
        let (selected_provider, adapter) = match self
            .coordinator
            .select(&p.request, p.preferred_provider.as_deref())
        {
            Ok(pair) => pair,
            Err(CoordinatorError::PreferredUnavailable { .. }) => {
                // Caller gave a preference we can't honor. Surface
                // as a typed result with success=false so the
                // caller knows the difference between "no provider
                // available" and "your preference isn't valid here."
                // The coordinator's error's Display impl carries
                // the diagnostic text.
                let coord_err = self
                    .coordinator
                    .select(&p.request, p.preferred_provider.as_deref())
                    .err()
                    .expect("just produced this error");
                return Ok(CommandResult::Json(json!({
                    "success": false,
                    "error": coord_err.to_string(),
                })));
            }
            Err(CoordinatorError::NoCapableAdapter { .. }) => {
                let coord_err = self
                    .coordinator
                    .select(&p.request, p.preferred_provider.as_deref())
                    .err()
                    .expect("just produced this error");
                return Ok(CommandResult::Json(json!({
                    "success": false,
                    "error": coord_err.to_string(),
                })));
            }
        };

        // 2. Adapter creates the job. Translate FineTuningError
        // back to a wire-friendly result. The variant set is the
        // contract; the inner string is telemetry text.
        match adapter.create_job(p.request).await {
            Ok(handle) => {
                let result = JobCreateResult {
                    handle,
                    selected_provider,
                };
                Ok(CommandResult::Json(json!({
                    "success": true,
                    "result": serde_json::to_value(&result)
                        .map_err(|e| format!("serialize JobCreateResult: {e}"))?,
                })))
            }
            Err(e) => Ok(CommandResult::Json(json!({
                "success": false,
                "error": e.to_string(),
                "errorKind": fine_tuning_error_kind(&e),
            }))),
        }
    }

    async fn handle_job_status(&self, params: Value) -> Result<CommandResult, String> {
        let p: JobLookupParams = serde_json::from_value(params)
            .map_err(|e| format!("invalid job-status params: {e}"))?;

        let adapter = match self.registry.get(&p.handle.provider_id) {
            Some(a) => a,
            None => {
                return Ok(CommandResult::Json(json!({
                    "success": false,
                    "error": format!(
                        "no adapter registered for provider {:?}",
                        p.handle.provider_id
                    ),
                    "errorKind": "UnknownHandle",
                })));
            }
        };

        match adapter.poll(&p.handle).await {
            Ok(status) => Ok(CommandResult::Json(json!({
                "success": true,
                "status": serde_json::to_value(&status)
                    .map_err(|e| format!("serialize TrainingStatus: {e}"))?,
            }))),
            Err(e) => Ok(CommandResult::Json(json!({
                "success": false,
                "error": e.to_string(),
                "errorKind": fine_tuning_error_kind(&e),
            }))),
        }
    }

    async fn handle_job_cancel(&self, params: Value) -> Result<CommandResult, String> {
        let p: JobLookupParams = serde_json::from_value(params)
            .map_err(|e| format!("invalid job-cancel params: {e}"))?;

        let adapter = match self.registry.get(&p.handle.provider_id) {
            Some(a) => a,
            None => {
                return Ok(CommandResult::Json(json!({
                    "success": false,
                    "error": format!(
                        "no adapter registered for provider {:?}",
                        p.handle.provider_id
                    ),
                    "errorKind": "UnknownHandle",
                })));
            }
        };

        match adapter.cancel(&p.handle).await {
            Ok(()) => Ok(CommandResult::Json(json!({ "success": true }))),
            Err(e) => Ok(CommandResult::Json(json!({
                "success": false,
                "error": e.to_string(),
                "errorKind": fine_tuning_error_kind(&e),
            }))),
        }
    }
}

/// Stable string slug per [`FineTuningError`] variant. Callers branch
/// on this to decide retry-vs-surface without parsing free-form
/// messages. Mirrors the variant taxonomy 1:1; a future error
/// variant must add a slug here too — caught at compile time by the
/// exhaustive match.
fn fine_tuning_error_kind(e: &FineTuningError) -> &'static str {
    match e {
        FineTuningError::InvalidRequest(_) => "InvalidRequest",
        FineTuningError::MissingCredentials(_) => "MissingCredentials",
        FineTuningError::ProviderRejected(_) => "ProviderRejected",
        FineTuningError::Transient(_) => "Transient",
        FineTuningError::MalformedResponse(_) => "MalformedResponse",
        FineTuningError::LocalTrainerFailed(_) => "LocalTrainerFailed",
        FineTuningError::UnknownHandle(_) => "UnknownHandle",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::fine_tuning::adapter::{
        FineTuningAdapter, FineTuningCapabilities, TrainerHardware,
    };
    use crate::genome::fine_tuning::types::{
        JobMetrics, TrainingArtifact, TrainingDataset, TrainingSource, TrainingStatus,
    };
    use uuid::Uuid;

    /// Adapter that returns predictable Ok values — used to test the
    /// module's wire shape, not the adapters themselves.
    struct OkStubAdapter(&'static str);

    #[async_trait]
    impl FineTuningAdapter for OkStubAdapter {
        fn capabilities(&self) -> FineTuningCapabilities {
            FineTuningCapabilities {
                provider_id: self.0.to_string(),
                supports_lora: true,
                supports_validation: true,
                produces_local_artifact: false,
                supported_base_model_prefixes: vec![],
                requires: TrainerHardware::Any,
            }
        }
        async fn create_job(
            &self,
            _r: TrainingJobRequest,
        ) -> Result<JobHandle, FineTuningError> {
            Ok(JobHandle {
                provider_id: self.0.to_string(),
                provider_job_id: format!("{}-job-1", self.0),
                local_id: Uuid::nil(),
            })
        }
        async fn poll(&self, _h: &JobHandle) -> Result<TrainingStatus, FineTuningError> {
            Ok(TrainingStatus::Completed {
                artifact: TrainingArtifact {
                    model_id: format!("{}:trained", self.0),
                    local_path: None,
                    metrics: JobMetrics::default(),
                },
            })
        }
        async fn cancel(&self, _h: &JobHandle) -> Result<(), FineTuningError> {
            Ok(())
        }
    }

    fn dataset() -> TrainingDataset {
        TrainingDataset {
            examples: vec![],
            source: TrainingSource::OperatorCurated,
            validation_split: 0.0,
        }
    }

    fn module_with(adapters: Vec<&'static str>) -> GenomeModule {
        let reg = Arc::new(FineTuningRegistry::new());
        for id in adapters {
            reg.register(Arc::new(OkStubAdapter(id)));
        }
        GenomeModule::new(reg)
    }

    fn params_for(base: &str, preferred: Option<&str>) -> Value {
        let mut v = json!({
            "personaId": Uuid::nil().to_string(),
            "personaName": "test",
            "baseModel": base,
            "traitKind": "test-trait",
            "dataset": dataset(),
        });
        if let Some(p) = preferred {
            v["preferredProvider"] = json!(p);
        }
        v
    }

    // what this catches: end-to-end happy path. genome/job-create
    // dispatches through the coordinator to the registered adapter,
    // returns the wire shape with success=true + handle +
    // selectedProvider. A future refactor that changes the result
    // envelope breaks every TS-side / Rust-side caller; pin it here.
    #[tokio::test]
    async fn job_create_happy_path_returns_handle_and_selected_provider() {
        let module = module_with(vec!["openai"]);
        let result = module
            .handle_job_create(params_for("gpt-4o-mini", None))
            .await
            .unwrap();
        let value: Value = result.to_json_value().unwrap();
        assert_eq!(value["success"], true);
        assert_eq!(value["result"]["selectedProvider"], "openai");
        assert_eq!(value["result"]["handle"]["providerId"], "openai");
        assert_eq!(value["result"]["handle"]["providerJobId"], "openai-job-1");
    }

    // what this catches: empty registry → success=false with the
    // NoCapableAdapter error text, NOT an Err return. The substrate
    // wire contract is: handle_command always returns Ok with a
    // JSON envelope; errors come through `success: false`. A future
    // refactor that returns Err here would surface as a substrate
    // dispatch error instead of a domain-typed failure.
    #[tokio::test]
    async fn job_create_with_empty_registry_returns_no_capable_envelope() {
        let module = module_with(vec![]);
        let result = module
            .handle_job_create(params_for("gpt-4o-mini", None))
            .await
            .unwrap();
        let value: Value = result.to_json_value().unwrap();
        assert_eq!(value["success"], false);
        let err = value["error"].as_str().unwrap();
        assert!(err.contains("no fine-tuning adapter"));
    }

    // what this catches: preferredProvider is honored and surfaced
    // in selectedProvider. A future refactor that drops the
    // preference parameter would silently route to whichever
    // adapter the coordinator's rank function preferred — exactly
    // the silent-fallback class the coordinator's typed
    // PreferredUnavailable was added to prevent.
    #[tokio::test]
    async fn preferred_provider_is_honored_when_capable() {
        let module = module_with(vec!["openai", "mistral"]);
        let result = module
            .handle_job_create(params_for("gpt-4o-mini", Some("mistral")))
            .await
            .unwrap();
        let value: Value = result.to_json_value().unwrap();
        assert_eq!(value["success"], true);
        assert_eq!(value["result"]["selectedProvider"], "mistral");
    }

    // what this catches: error kind taxonomy on the wire. The
    // substrate's TS-side caller branches on errorKind to decide
    // retry behavior; a future refactor that changes a slug
    // breaks every retry policy silently. Pin the variant -> slug
    // mapping by exercising one representative path.
    #[tokio::test]
    async fn job_status_unknown_provider_returns_unknown_handle_slug() {
        let module = module_with(vec!["openai"]);
        let params = json!({
            "handle": {
                "providerId": "no-such-adapter",
                "providerJobId": "x",
                "localId": Uuid::nil().to_string(),
            }
        });
        let result = module.handle_job_status(params).await.unwrap();
        let value: Value = result.to_json_value().unwrap();
        assert_eq!(value["success"], false);
        assert_eq!(value["errorKind"], "UnknownHandle");
    }

    // what this catches: cancel routes by handle.providerId, not by
    // any module-side cached selection. The OkStubAdapter returns
    // Ok(()) on cancel; the wire shape on success is just
    // {success: true} — no extra fields.
    #[tokio::test]
    async fn cancel_routes_by_handle_provider_id() {
        let module = module_with(vec!["openai"]);
        let params = json!({
            "handle": {
                "providerId": "openai",
                "providerJobId": "openai-job-1",
                "localId": Uuid::nil().to_string(),
            }
        });
        let result = module.handle_job_cancel(params).await.unwrap();
        let value: Value = result.to_json_value().unwrap();
        assert_eq!(value["success"], true);
    }
}
