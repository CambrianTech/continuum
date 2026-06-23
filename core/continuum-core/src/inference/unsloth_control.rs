//! Unsloth model-load keystone (#24) — ensure the gateway's ACTIVE model is the
//! one the caller is about to generate with.
//!
//! unsloth Studio starts **EMPTY** — `/v1/models` → `[]`, and any inference
//! (`/v1/chat`, `/v1/embeddings`) returns "No model loaded. Call POST
//! /inference/load first." until a model is loaded. Worse, the gateway serves a
//! SINGLE resident model and **ignores the `model` field** on
//! `/v1/chat/completions`: request M2 while M1 is resident and it silently
//! answers as M1 (verified 2026-06-23). And it idle-unloads to free VRAM, so a
//! live persona goes mute mid-life. So "is *something* loaded" is NOT safety —
//! only "is *the right* model active" is. This is that keystone: pure delegation
//! over unsloth's HTTP management surface (UNSLOTH-INTEGRATION.md §3.5), never a
//! CLI subprocess:
//!   - `GET  {base}/api/inference/status` — which model is ACTIVE (`active_model`)?
//!   - `POST {base}/api/inference/load`   — load `{model_path}` (loads synchronously)
//!
//! The model identifier is dynamic + API-driven: the SAME hub id the persona
//! generates with is what `/api/inference/load` accepts (verified) — no
//! filesystem path, no per-model config, works for any of N models. The id that
//! routes generation also drives the load (one source of truth).
//!
//! ## Why a trait (TDD)
//! The DECISION ("right model active? → skip; wrong/none → load; unreachable →
//! degrade") is separated from the HTTP I/O behind [`UnslothControl`], so
//! [`ensure_model_active`] is unit-tested against a fake with zero network — the
//! logic is the part that must be correct + reliable. [`UnslothHttp`] is the real
//! reqwest impl.
//!
//! ## Degrade, never panic ([[substrate-is-a-good-citizen-on-the-host]] / #26)
//! unsloth unreachable or a failed load → [`EnsureOutcome::Degraded`], logged,
//! the substrate keeps running (the persona's lexical-recall path stays live);
//! we never panic on a missing/owned-by-another-process engine.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use ts_rs::TS;

use crate::config_env;

/// Default unsloth host when `UNSLOTH_BASE_URL` is unset. THE one constant for
/// the default endpoint — referenced by [`unsloth_base_url`] and the provider
/// catalog, so the default lives in exactly one place.
pub const DEFAULT_HOST: &str = "http://127.0.0.1:8888";

/// The canonical location unsloth Studio writes the agent API key — the source of
/// truth for the gateway credential. Startup recovers from HERE when the key has
/// been deleted/expired out of `~/.continuum/config.env`.
fn studio_agent_key_path() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| {
        h.join(".unsloth")
            .join("studio")
            .join("auth")
            .join("agent_api_key.json")
    })
}

/// Outcome of [`ensure_api_key`] — announced loudly at boot, never silent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiKeyStatus {
    /// Already present in `~/.continuum/config.env` (the one owner).
    Present,
    /// Was missing; recovered from unsloth Studio and persisted to config.env.
    RecoveredFromStudio,
    /// Gone from config.env AND Studio — needs the operator (paste / regenerate).
    Missing,
}

/// Impervious-startup key resolution: make `UNSLOTH_API_KEY` resolvable for this
/// boot, self-healing the common screwup (the key vanished from config.env because
/// it expired / got deleted / a reinstall wiped it) by recovering it from the
/// canonical Studio source and persisting it through the ONE owner
/// ([`config_env::upsert`]). It NEVER silently proceeds without the key — a
/// truly-gone key returns [`ApiKeyStatus::Missing`] so the caller fails LOUD with
/// remediation ([[fallbacks-are-illegal-fail-loud]]). Single writer, one config.
pub fn ensure_api_key() -> ApiKeyStatus {
    if config_env::read("UNSLOTH_API_KEY")
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
    {
        return ApiKeyStatus::Present;
    }
    if let Some(path) = studio_agent_key_path() {
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                if let Some(key) = v
                    .get("keys")
                    .and_then(|ks| ks.get(0))
                    .and_then(|k| k.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    if config_env::upsert("UNSLOTH_API_KEY", key).is_ok() {
                        // Make it visible to secrets()'s env overlay too, in case the
                        // secrets cache loads later in this same process.
                        std::env::set_var("UNSLOTH_API_KEY", key);
                        return ApiKeyStatus::RecoveredFromStudio;
                    }
                }
            }
        }
    }
    ApiKeyStatus::Missing
}

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

/// What `ensure_model_active` did — the inspectable outcome.
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

/// The gateway's live model-residency snapshot — the `ai/inference/status`
/// payload. `active_model` is THE field that matters for correctness: the
/// gateway serves a single resident model and IGNORES the per-request `model`,
/// so only the active model actually answers. `loaded`/`loading` describe the
/// rest of the residency set; `gguf_variant`/`context_length` describe the
/// active model's shape (the live context window — task #46's "context window
/// comes from the engine, not a hardcoded per-tier cap").
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai_inference/InferenceStatus.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct InferenceStatus {
    /// The single model serving right now; `null`/absent when the engine is empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub active_model: Option<String>,
    /// Models resident in memory (the gateway can switch among these).
    pub loaded: Vec<String>,
    /// Models currently mid-load.
    pub loading: Vec<String>,
    /// GGUF quantization variant of the active model, when it is a GGUF.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub gguf_variant: Option<String>,
    /// The active model's context window in tokens, when the gateway reports it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub context_length: Option<u64>,
}

/// One model the gateway can LOAD — discovered on disk (HF cache + the models
/// dir). `id` is the identifier `ai/inference/load` accepts; `display_name` and
/// `source` are for humans + UIs choosing from the catalog. This is the
/// loadable set, distinct from the SERVING set returned by `/v1/models`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai_inference/LocalModel.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct LocalModel {
    /// The hub identifier / load id (what `load`/`unload` take as `model`).
    pub id: String,
    /// Human-friendly name for catalogs + pickers.
    pub display_name: String,
    /// Where the gateway found it — `hf_cache`, `models_dir`, `lmstudio`, …
    pub source: String,
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
    /// Which model is ACTIVE (resident + serving) right now, if any.
    /// (`GET /api/inference/status` → `active_model`; `None` when nothing is loaded.)
    ///
    /// "Which one" — not "is anything loaded" — because the gateway serves a
    /// single resident model and IGNORES the `model` field on
    /// `/v1/chat/completions`: ask for M2 while M1 is resident and it silently
    /// answers as M1 (verified 2026-06-23). So the only safe pre-flight is
    /// identity ("is the ACTIVE model the one I'm about to generate with"), which
    /// needs the active model's name, not a boolean.
    async fn active_model(&self) -> Result<Option<String>, UnslothError>;
    /// Load `model` (`POST /api/inference/load` with `model_path`). The gateway
    /// accepts the SAME hub identifier the persona generates with (verified) — no
    /// filesystem path, no per-model config; the id that routes generation also
    /// drives the load. Returns when unsloth reports it loaded (synchronous in
    /// practice).
    async fn load_model(&self, model: &str) -> Result<(), UnslothError>;
}

/// The keystone: ensure the gateway's ACTIVE model is `desired` before anyone
/// generates with it. Idempotent — `desired` already active → no-op (steady
/// state, no churn); a DIFFERENT model (or none) resident → load `desired`;
/// unsloth unreachable or a failed load → DEGRADE (never panic) so the substrate
/// stays up and the caller decides. Pure logic over the trait.
///
/// Model-AWARE on purpose: "is `desired` active" not "is anything loaded". With
/// many models sharing one gateway, "something is loaded" is not safety — the
/// gateway would silently answer as whatever IS resident — so only "the RIGHT
/// one is active" is. The inference path treats [`EnsureOutcome::Degraded`] as a
/// HARD, loud failure: it must never generate against an unguaranteed brain.
/// `desired` is the persona's own model id (the same string it generates with);
/// no path, no hardcode, works for any of N models.
pub async fn ensure_model_active(api: &dyn UnslothControl, desired: &str) -> EnsureOutcome {
    match api.active_model().await {
        Ok(Some(active)) if active == desired => EnsureOutcome::AlreadyLoaded,
        Ok(_) => match api.load_model(desired).await {
            Ok(()) => EnsureOutcome::Loaded {
                model: desired.to_string(),
            },
            Err(e) => EnsureOutcome::Degraded {
                reason: format!("load of {desired} failed: {e}"),
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
/// has it active (delegating to [`ensure_model_active`] over the real HTTP
/// surface). Degrade-safe end to end — logs the outcome and never panics, so it
/// can be spawned as a fire-and-forget startup task. Returns the action taken so
/// callers/tests can assert without scraping logs.
pub async fn ensure_startup_model() -> StartupModelAction {
    let action = startup_model_action(config_env::read("UNSLOTH_MODEL"));
    match &action {
        StartupModelAction::Ensure(model) => {
            let outcome = ensure_model_active(&UnslothHttp::from_config(), model).await;
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
        Self::with_client(reqwest::Client::new())
    }

    /// Like [`from_config`](Self::from_config) but reuses an existing pooled
    /// `reqwest::Client` (cheap to clone — shares the connection pool). The
    /// inference path pre-flights `active_model` on EVERY generate, so churning a
    /// fresh client + pool each call is waste; the adapter hands us its own.
    /// Host + key still come from the single config owner.
    pub fn with_client(client: reqwest::Client) -> Self {
        Self {
            host: unsloth_base_url(),
            api_key: config_env::read("UNSLOTH_API_KEY"),
            client,
        }
    }

    fn authed(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.api_key {
            Some(key) => rb.header("Authorization", format!("Bearer {key}")),
            None => rb,
        }
    }

    /// Discover the models the gateway currently serves: `GET /v1/models` →
    /// `data[].id`. This is the live runnable-set discovery for model-fit
    /// (DYNAMIC-PERSONA-AND-MODEL slice 1) — what unsloth can actually serve right
    /// now, queried live, NOT a static catalog. `Err` = gateway unreachable.
    pub async fn list_models(&self) -> Result<Vec<String>, UnslothError> {
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
        Ok(body["data"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|m| m["id"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default())
    }

    /// The gateway's live residency snapshot (active + loaded + loading + the
    /// active model's shape) — one `GET /api/inference/status`. Backs
    /// `ai/inference/status`. `Err` = gateway unreachable / refused.
    pub async fn status(&self) -> Result<InferenceStatus, UnslothError> {
        let url = format!("{}/api/inference/status", self.host);
        let resp = self
            .authed(self.client.get(&url))
            .send()
            .await
            .map_err(|e| UnslothError::Unreachable(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(UnslothError::Api(format!(
                "/api/inference/status {}",
                resp.status()
            )));
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| UnslothError::Api(e.to_string()))?;
        Ok(InferenceStatus {
            active_model: body["active_model"].as_str().map(str::to_string),
            loaded: json_str_array(&body["loaded"]),
            loading: json_str_array(&body["loading"]),
            gguf_variant: body["gguf_variant"].as_str().map(str::to_string),
            context_length: body["context_length"].as_u64(),
        })
    }

    /// The models the gateway can LOAD — discovered on disk (`GET
    /// /api/models/local`: HF cache + the models dir). Distinct from
    /// [`list_models`](Self::list_models), which is the SERVING set
    /// (`/v1/models`). Backs `ai/inference/models`. `Err` = unreachable /
    /// refused.
    pub async fn local_models(&self) -> Result<Vec<LocalModel>, UnslothError> {
        let url = format!("{}/api/models/local", self.host);
        let resp = self
            .authed(self.client.get(&url))
            .send()
            .await
            .map_err(|e| UnslothError::Unreachable(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(UnslothError::Api(format!(
                "/api/models/local {}",
                resp.status()
            )));
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| UnslothError::Api(e.to_string()))?;
        Ok(body["models"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|m| {
                        let id = m["id"].as_str()?.to_string();
                        Some(LocalModel {
                            display_name: m["display_name"]
                                .as_str()
                                .unwrap_or(&id)
                                .to_string(),
                            source: m["source"].as_str().unwrap_or_default().to_string(),
                            id,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Unload `model` from memory (`POST /api/inference/unload` with
    /// `model_path` — the gateway routes to the right backend). Frees VRAM.
    /// Backs `ai/inference/unload`. `Err` = unreachable / refused.
    pub async fn unload_model(&self, model: &str) -> Result<(), UnslothError> {
        let url = format!("{}/api/inference/unload", self.host);
        let resp = self
            .authed(self.client.post(&url))
            .json(&json!({ "model_path": model }))
            .send()
            .await
            .map_err(|e| UnslothError::Unreachable(e.to_string()))?;
        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(UnslothError::Api(format!(
                "/api/inference/unload {status}: {body}"
            )))
        }
    }
}

/// Collect a JSON array-of-strings into a `Vec<String>`, dropping non-strings;
/// a non-array (or absent) value yields an empty vec. Used to read the gateway
/// status's `loaded`/`loading` lists.
fn json_str_array(v: &serde_json::Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[async_trait]
impl UnslothControl for UnslothHttp {
    async fn active_model(&self) -> Result<Option<String>, UnslothError> {
        let url = format!("{}/api/inference/status", self.host);
        let resp = self
            .authed(self.client.get(&url))
            .send()
            .await
            .map_err(|e| UnslothError::Unreachable(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(UnslothError::Api(format!(
                "/api/inference/status {}",
                resp.status()
            )));
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| UnslothError::Api(e.to_string()))?;
        // `active_model` is the resident model's identifier, `null` when empty.
        Ok(body["active_model"].as_str().map(|s| s.to_string()))
    }

    async fn load_model(&self, model: &str) -> Result<(), UnslothError> {
        let url = format!("{}/api/inference/load", self.host);
        let resp = self
            .authed(self.client.post(&url))
            // The gateway's field is `model_path`, but it accepts the hub
            // identifier the persona generates with (verified) — not only a
            // filesystem path. We pass the identifier straight through.
            .json(&json!({ "model_path": model }))
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

    /// Fake control surface: scripted `active_model` + records the load calls
    /// made, so we assert the DECISION without any HTTP.
    struct FakeUnsloth {
        active: Result<Option<String>, UnslothError>,
        load_result: Result<(), UnslothError>,
        load_calls: Mutex<Vec<String>>,
    }
    impl FakeUnsloth {
        fn new(
            active: Result<Option<String>, UnslothError>,
            load_result: Result<(), UnslothError>,
        ) -> Self {
            Self {
                active,
                load_result,
                load_calls: Mutex::new(Vec::new()),
            }
        }
    }
    #[async_trait]
    impl UnslothControl for FakeUnsloth {
        async fn active_model(&self) -> Result<Option<String>, UnslothError> {
            self.active.clone()
        }
        async fn load_model(&self, model: &str) -> Result<(), UnslothError> {
            self.load_calls.lock().unwrap().push(model.to_string());
            self.load_result.clone()
        }
    }

    // what this catches: steady state — the DESIRED model already active must be
    // a NO-OP (no spurious reload/churn on every generate). Regression here = the
    // engine gets reloaded needlessly, dropping live work on the hot path.
    #[tokio::test]
    async fn desired_already_active_is_a_noop() {
        let fake = FakeUnsloth::new(Ok(Some("qwen.gguf".into())), Ok(()));
        let outcome = ensure_model_active(&fake, "qwen.gguf").await;
        assert_eq!(outcome, EnsureOutcome::AlreadyLoaded);
        assert!(
            fake.load_calls.lock().unwrap().is_empty(),
            "must NOT load when the desired model is already active"
        );
    }

    // what this catches: THE haunting bug — a DIFFERENT model is resident, so the
    // gateway would silently answer as the wrong brain. We must load the desired
    // model, not trust the resident one. Regression here = persona B silently
    // gets persona A's model with no error.
    #[tokio::test]
    async fn wrong_model_active_loads_the_desired_one() {
        let fake = FakeUnsloth::new(Ok(Some("other-model".into())), Ok(()));
        let outcome = ensure_model_active(&fake, "qwen.gguf").await;
        assert_eq!(
            outcome,
            EnsureOutcome::Loaded {
                model: "qwen.gguf".to_string()
            }
        );
        assert_eq!(*fake.load_calls.lock().unwrap(), vec!["qwen.gguf".to_string()]);
    }

    // what this catches: the keystone path — empty engine (or idle-unloaded) →
    // load the desired model. This is what makes "install → it's just running"
    // true, and what re-arms a persona after the gateway idle-unloads mid-life.
    #[tokio::test]
    async fn empty_engine_loads_the_desired_model() {
        let fake = FakeUnsloth::new(Ok(None), Ok(()));
        let outcome = ensure_model_active(&fake, "qwen.gguf").await;
        assert_eq!(
            outcome,
            EnsureOutcome::Loaded {
                model: "qwen.gguf".to_string()
            }
        );
        assert_eq!(*fake.load_calls.lock().unwrap(), vec!["qwen.gguf".to_string()]);
    }

    // what this catches: degrade-not-panic when unsloth is DOWN — the substrate
    // keeps running; we never panic on an unreachable engine, and we don't
    // blindly attempt a load against a dead host.
    #[tokio::test]
    async fn unreachable_engine_degrades_without_loading() {
        let fake = FakeUnsloth::new(Err(UnslothError::Unreachable("conn refused".into())), Ok(()));
        let outcome = ensure_model_active(&fake, "qwen.gguf").await;
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
    // e.g. bad model id. The inference path turns this into a loud generate
    // failure; it must never silently fall through to the wrong resident model.
    #[tokio::test]
    async fn failed_load_degrades_with_reason() {
        let fake = FakeUnsloth::new(Ok(None), Err(UnslothError::Api("bad path".into())));
        let outcome = ensure_model_active(&fake, "missing.gguf").await;
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
