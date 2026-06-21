//! Unsloth model-load keystone (#24) — ensure the engine has a model loaded.
//!
//! unsloth Studio starts **EMPTY** — `/v1/models` → `[]`, and any inference
//! (`/v1/chat`, `/v1/embeddings`) returns "No GGUF model loaded" until a model
//! is loaded. So nothing a persona needs serves until *something* tells unsloth
//! which model to load. This is that something: the reliable-startup keystone —
//! "automatic after the key." It's pure delegation over unsloth's HTTP
//! management surface (UNSLOTH-INTEGRATION.md §3.5), never a CLI subprocess:
//!   - `GET  {base}/v1/models`            — is a model loaded?
//!   - `POST {base}/api/inference/load`   — load `{model_path}` (loads synchronously)
//!
//! ## Why a trait (TDD)
//! The DECISION ("loaded? → skip; empty? → load; unreachable? → degrade") is
//! separated from the HTTP I/O behind [`UnslothControl`], so [`ensure_model_loaded`]
//! is unit-tested against a fake with zero network — the logic is the part that
//! must be correct + reliable. [`UnslothHttp`] is the real reqwest impl.
//!
//! ## Degrade, never panic ([[substrate-is-a-good-citizen-on-the-host]] / #26)
//! unsloth unreachable or a failed load → [`EnsureOutcome::Degraded`], logged,
//! the substrate keeps running (the persona's lexical-recall path stays live);
//! we never panic on a missing/owned-by-another-process engine.

use async_trait::async_trait;
use serde_json::json;

use crate::config_env;

/// Default unsloth host when `UNSLOTH_BASE_URL` is unset.
const DEFAULT_HOST: &str = "http://127.0.0.1:8888";

/// What `ensure_model_loaded` did — the inspectable outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnsureOutcome {
    /// A model was already serving — no-op (steady state, the common case).
    AlreadyLoaded,
    /// We loaded `model` (unsloth was empty).
    Loaded { model: String },
    /// unsloth unreachable or the load failed — degraded with the reason. The
    /// caller keeps running on its fallback path; this is NOT fatal.
    Degraded { reason: String },
}

/// Failure talking to unsloth. `Unreachable` = transport (engine down / wrong
/// URL); `Api` = reached it but it refused/erred.
#[derive(Debug, Clone)]
pub enum UnslothError {
    Unreachable(String),
    Api(String),
}

impl std::fmt::Display for UnslothError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreachable(m) => write!(f, "unreachable: {m}"),
            Self::Api(m) => write!(f, "api error: {m}"),
        }
    }
}

/// The minimal unsloth control surface the keystone needs — only the
/// model-lifecycle calls. Behind a trait so the decision logic is TDD-tested
/// apart from the HTTP I/O.
#[async_trait]
pub trait UnslothControl: Send + Sync {
    /// Is a model currently loaded + serving? (`GET /v1/models`, non-empty.)
    async fn model_loaded(&self) -> Result<bool, UnslothError>;
    /// Load the model at `model_path` (`POST /api/inference/load`). Returns when
    /// unsloth reports it loaded (the load call is synchronous in practice).
    async fn load_model(&self, model_path: &str) -> Result<(), UnslothError>;
}

/// The keystone: ensure unsloth has a model serving. Idempotent — if one is
/// already loaded it's a no-op (steady state); if empty it loads `desired_model`;
/// if unsloth is unreachable or the load fails it DEGRADES (never panics), so
/// the substrate stays up and the caller falls back. Pure logic over the trait.
pub async fn ensure_model_loaded(api: &dyn UnslothControl, desired_model: &str) -> EnsureOutcome {
    match api.model_loaded().await {
        Ok(true) => EnsureOutcome::AlreadyLoaded,
        Ok(false) => match api.load_model(desired_model).await {
            Ok(()) => EnsureOutcome::Loaded {
                model: desired_model.to_string(),
            },
            Err(e) => EnsureOutcome::Degraded {
                reason: format!("load of {desired_model} failed: {e}"),
            },
        },
        Err(e) => EnsureOutcome::Degraded {
            reason: format!("unsloth status check failed: {e}"),
        },
    }
}

/// Real reqwest impl of [`UnslothControl`] over unsloth's HTTP surface.
pub struct UnslothHttp {
    /// Host root (no `/v1`), e.g. `http://127.0.0.1:8888`.
    host: String,
    api_key: Option<String>,
    client: reqwest::Client,
}

impl UnslothHttp {
    /// Build from config: `UNSLOTH_BASE_URL` (the `/v1` URL) + `UNSLOTH_API_KEY`.
    /// We derive the host root by stripping a trailing `/v1`, since the model
    /// management lives under `/api/*` and serving under `/v1/*` on the same host.
    pub fn from_config() -> Self {
        let base = config_env::read("UNSLOTH_BASE_URL").unwrap_or_else(|| DEFAULT_HOST.to_string());
        let host = base
            .trim_end_matches('/')
            .trim_end_matches("/v1")
            .trim_end_matches('/')
            .to_string();
        Self {
            host,
            api_key: config_env::read("UNSLOTH_API_KEY"),
            client: reqwest::Client::new(),
        }
    }

    fn authed(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.api_key {
            Some(key) => rb.header("Authorization", format!("Bearer {key}")),
            None => rb,
        }
    }
}

#[async_trait]
impl UnslothControl for UnslothHttp {
    async fn model_loaded(&self) -> Result<bool, UnslothError> {
        let url = format!("{}/v1/models", self.host);
        let resp = self
            .authed(self.client.get(&url))
            .send()
            .await
            .map_err(|e| UnslothError::Unreachable(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(UnslothError::Api(format!("/v1/models {}", resp.status())));
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| UnslothError::Api(e.to_string()))?;
        Ok(body["data"].as_array().map(|a| !a.is_empty()).unwrap_or(false))
    }

    async fn load_model(&self, model_path: &str) -> Result<(), UnslothError> {
        let url = format!("{}/api/inference/load", self.host);
        let resp = self
            .authed(self.client.post(&url))
            .json(&json!({ "model_path": model_path }))
            .send()
            .await
            .map_err(|e| UnslothError::Unreachable(e.to_string()))?;
        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(UnslothError::Api(format!("/api/inference/load {status}: {body}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Fake control surface: scripted responses + records the calls made, so we
    /// assert the DECISION without any HTTP.
    struct FakeUnsloth {
        loaded: Result<bool, UnslothError>,
        load_result: Result<(), UnslothError>,
        load_calls: Mutex<Vec<String>>,
    }
    impl FakeUnsloth {
        fn new(loaded: Result<bool, UnslothError>, load_result: Result<(), UnslothError>) -> Self {
            Self {
                loaded,
                load_result,
                load_calls: Mutex::new(Vec::new()),
            }
        }
    }
    #[async_trait]
    impl UnslothControl for FakeUnsloth {
        async fn model_loaded(&self) -> Result<bool, UnslothError> {
            self.loaded.clone()
        }
        async fn load_model(&self, model_path: &str) -> Result<(), UnslothError> {
            self.load_calls.lock().unwrap().push(model_path.to_string());
            self.load_result.clone()
        }
    }

    // what this catches: steady state — a model already serving must be a NO-OP
    // (no spurious reload/churn on every startup). Regression here = the engine
    // gets reloaded needlessly, dropping live work.
    #[tokio::test]
    async fn already_loaded_is_a_noop() {
        let fake = FakeUnsloth::new(Ok(true), Ok(()));
        let outcome = ensure_model_loaded(&fake, "qwen.gguf").await;
        assert_eq!(outcome, EnsureOutcome::AlreadyLoaded);
        assert!(
            fake.load_calls.lock().unwrap().is_empty(),
            "must NOT load when a model is already serving"
        );
    }

    // what this catches: the keystone path — empty engine → load the desired
    // model. This is what makes "install → it's just running" true.
    #[tokio::test]
    async fn empty_engine_loads_the_desired_model() {
        let fake = FakeUnsloth::new(Ok(false), Ok(()));
        let outcome = ensure_model_loaded(&fake, "qwen.gguf").await;
        assert_eq!(
            outcome,
            EnsureOutcome::Loaded {
                model: "qwen.gguf".to_string()
            }
        );
        assert_eq!(*fake.load_calls.lock().unwrap(), vec!["qwen.gguf".to_string()]);
    }

    // what this catches: degrade-not-panic when unsloth is DOWN — the substrate
    // keeps running on its fallback; we never panic on an unreachable engine,
    // and we don't blindly attempt a load against a dead host.
    #[tokio::test]
    async fn unreachable_engine_degrades_without_loading() {
        let fake = FakeUnsloth::new(Err(UnslothError::Unreachable("conn refused".into())), Ok(()));
        let outcome = ensure_model_loaded(&fake, "qwen.gguf").await;
        match outcome {
            EnsureOutcome::Degraded { reason } => assert!(reason.contains("status check failed")),
            other => panic!("expected Degraded, got {other:?}"),
        }
        assert!(
            fake.load_calls.lock().unwrap().is_empty(),
            "must not attempt a load when the engine is unreachable"
        );
    }

    // what this catches: a failed load degrades (not panics) with the reason —
    // e.g. bad model path. Persona falls back; substrate stays up.
    #[tokio::test]
    async fn failed_load_degrades_with_reason() {
        let fake = FakeUnsloth::new(Ok(false), Err(UnslothError::Api("bad path".into())));
        let outcome = ensure_model_loaded(&fake, "missing.gguf").await;
        match outcome {
            EnsureOutcome::Degraded { reason } => {
                assert!(reason.contains("load of missing.gguf failed"))
            }
            other => panic!("expected Degraded, got {other:?}"),
        }
    }
}
