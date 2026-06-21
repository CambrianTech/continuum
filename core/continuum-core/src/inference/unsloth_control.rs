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

/// Default unsloth host when `UNSLOTH_BASE_URL` is unset. THE one constant for
/// the default endpoint — referenced by [`unsloth_base_url`] and the provider
/// catalog, so the default lives in exactly one place.
pub const DEFAULT_HOST: &str = "http://127.0.0.1:8888";

/// **THE one accessor for the unsloth endpoint.** The single place that reads the
/// `UNSLOTH_BASE_URL` setting (or [`DEFAULT_HOST`]) and returns the canonical
/// **host root** (no trailing `/v1`, no trailing slash). Every consumer asks
/// `unsloth_control` for this — the keystone's HTTP client AND the gateway
/// adapter registration — so the endpoint lives in ONE form, read in ONE place.
/// No other module reads `UNSLOTH_BASE_URL` or strips `/v1` on its own.
///
/// Callers append their own path: serving is `{base}/v1/...`, management is
/// `{base}/api/...`. Accepting an operator-pasted `…/v1` and normalizing it here
/// (once) is why there is no double-`/v1` 405 anywhere.
pub fn unsloth_base_url() -> String {
    let raw = config_env::read("UNSLOTH_BASE_URL").unwrap_or_else(|| DEFAULT_HOST.to_string());
    let trimmed = raw.trim_end_matches('/');
    trimmed
        .strip_suffix("/v1")
        .unwrap_or(trimmed)
        .trim_end_matches('/')
        .to_string()
}

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

/// Startup decision: should boot fuel the engine, and with which model?
/// Pure + inspectable so the config logic is TDD-tested apart from any I/O —
/// we never hardcode a machine-specific model path; the model is config-driven.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupModelAction {
    /// `UNSLOTH_MODEL` is set + non-empty → ensure this model is loaded at boot.
    Ensure(String),
    /// No model configured → skip auto-fuel (with the reason, for the log). The
    /// engine stays as-is; a persona's first inference will surface the empty
    /// engine, and the operator can set `UNSLOTH_MODEL` to auto-fuel next boot.
    Skip(String),
}

/// Decide the boot action from the configured `UNSLOTH_MODEL` value. Trims
/// whitespace; an unset OR blank value → [`StartupModelAction::Skip`]. No
/// default model is baked in — auto-picking from `/api/hub/cached-models` is a
/// future enhancement, not a hardcoded path.
pub fn startup_model_action(configured: Option<String>) -> StartupModelAction {
    match configured.map(|s| s.trim().to_string()) {
        Some(model) if !model.is_empty() => StartupModelAction::Ensure(model),
        _ => StartupModelAction::Skip(
            "UNSLOTH_MODEL unset — skipping auto-fuel; set it to load a model at boot".to_string(),
        ),
    }
}

/// Boot convenience: read `UNSLOTH_MODEL` from config, and if set, ensure unsloth
/// has it loaded (delegating to [`ensure_model_loaded`] over the real HTTP
/// surface). Degrade-safe end to end — logs the outcome and never panics, so it
/// can be spawned as a fire-and-forget startup task. Returns the action taken so
/// callers/tests can assert without scraping logs.
pub async fn ensure_startup_model() -> StartupModelAction {
    let action = startup_model_action(config_env::read("UNSLOTH_MODEL"));
    match &action {
        StartupModelAction::Ensure(model) => {
            let outcome = ensure_model_loaded(&UnslothHttp::from_config(), model).await;
            match &outcome {
                EnsureOutcome::AlreadyLoaded => {
                    tracing::info!(target: "unsloth", model = %model, "startup: model already loaded");
                }
                EnsureOutcome::Loaded { model } => {
                    tracing::info!(target: "unsloth", model = %model, "startup: loaded model into engine");
                }
                EnsureOutcome::Degraded { reason } => {
                    tracing::warn!(target: "unsloth", %reason, "startup: model auto-fuel degraded — substrate continues on fallback");
                }
            }
        }
        StartupModelAction::Skip(reason) => {
            tracing::info!(target: "unsloth", %reason, "startup: skipping model auto-fuel");
        }
    }
    action
}

/// Real reqwest impl of [`UnslothControl`] over unsloth's HTTP surface.
pub struct UnslothHttp {
    /// Host root (no `/v1`), e.g. `http://127.0.0.1:8888`.
    host: String,
    api_key: Option<String>,
    client: reqwest::Client,
}

impl UnslothHttp {
    /// Build from config via the single [`unsloth_base_url`] accessor (host root)
    /// + `UNSLOTH_API_KEY`. No env read or `/v1` handling here — that lives in the
    /// one accessor.
    pub fn from_config() -> Self {
        Self {
            host: unsloth_base_url(),
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

    // what this catches: a configured model name → Ensure(that model). This is
    // the "automatic after the key" path — boot fuels the engine from config.
    #[test]
    fn configured_model_means_ensure() {
        assert_eq!(
            startup_model_action(Some("qwen2.5-0.5b".to_string())),
            StartupModelAction::Ensure("qwen2.5-0.5b".to_string())
        );
    }

    // what this catches: NO hardcoded default — unset config must Skip, not load
    // some machine-specific path. Regression here = a baked-in model path that's
    // wrong on every box but the one it was written on.
    #[test]
    fn unset_model_skips_no_hardcoded_default() {
        match startup_model_action(None) {
            StartupModelAction::Skip(_) => {}
            other => panic!("expected Skip, got {other:?}"),
        }
    }

    // what this catches: a blank/whitespace UNSLOTH_MODEL is treated as unset
    // (config files love empty strings) — Skip, never attempt to load "".
    #[test]
    fn blank_model_is_treated_as_unset() {
        match startup_model_action(Some("   ".to_string())) {
            StartupModelAction::Skip(_) => {}
            other => panic!("expected Skip for blank, got {other:?}"),
        }
    }

    // what this catches: a configured value with surrounding whitespace is
    // trimmed before use — `UNSLOTH_MODEL=qwen \n` must not request " qwen ".
    #[test]
    fn configured_model_is_trimmed() {
        assert_eq!(
            startup_model_action(Some("  qwen2.5-0.5b  ".to_string())),
            StartupModelAction::Ensure("qwen2.5-0.5b".to_string())
        );
    }
}
