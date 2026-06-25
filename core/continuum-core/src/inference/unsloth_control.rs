//! Unsloth gateway HTTP control surface — the typed client continuum uses to
//! drive the unsloth Studio inference gateway's management + serving endpoints.
//!
//! Pure delegation over unsloth's HTTP surface (UNSLOTH-INTEGRATION.md §3.5),
//! never a CLI subprocess. [`UnslothHttp`] wraps the gateway's `/v1/...`
//! (serving) and `/api/...` (management) routes; [`unsloth_base_url`] is the ONE
//! accessor for the endpoint so the URL lives in a single normalized form.
//! Failures surface as [`UnslothError`] (`Unreachable` = transport down / wrong
//! URL; `Api` = reached but refused) so callers degrade, never panic, on a
//! missing / owned-by-another-process engine
//! ([[substrate-is-a-good-citizen-on-the-host]] / #26).
//!
//! The gateway serves a SINGLE resident model and IGNORES the per-request
//! `model` field on `/v1/chat/completions` (verified 2026-06-23): ask for M2
//! while M1 is resident and it silently answers as M1. So "which model is
//! ACTIVE" — the `active_model` field on [`InferenceStatus`] — is the only safe
//! identity check. This surface backs the `ai/inference/*` commands in
//! [`model_commands`](crate::inference::model_commands::AiInferenceStatus).
//!
//! Boot auto-fuel and API-key self-recovery used to live here too; the live
//! serving path is now [`llama_server`](crate::inference::llama_server) +
//! [`serving_daemon`](crate::modules::serving_daemon), so those unsloth-Studio-era
//! helpers were removed rather than left as dead code.

use serde::{Deserialize, Serialize};
use serde_json::json;
use ts_rs::TS;

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
/// `{base}/api/...`.
pub fn unsloth_base_url() -> String {
    normalize_base(&config_env::read("UNSLOTH_BASE_URL").unwrap_or_else(|| DEFAULT_HOST.to_string()))
}

/// Normalize an endpoint to the canonical host root: no trailing slash, no
/// trailing `/v1`. Pure (split from the config read) so the normalization is
/// unit-tested apart from the environment. Accepting an operator-pasted `…/v1`
/// and stripping it here, once, is why there is no double-`/v1` 405 anywhere.
fn normalize_base(raw: &str) -> String {
    let trimmed = raw.trim_end_matches('/');
    trimmed
        .strip_suffix("/v1")
        .unwrap_or(trimmed)
        .trim_end_matches('/')
        .to_string()
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

/// Typed reqwest client over unsloth's HTTP surface — the management (`/api/...`)
/// and serving-discovery (`/v1/models`) calls the `ai/inference/*` commands need.
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
    /// `reqwest::Client` (cheap to clone — shares the connection pool). Host + key
    /// still come from the single config owner.
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

    /// Load `model` (`POST /api/inference/load` with `model_path`). The gateway
    /// accepts the SAME hub identifier the persona generates with (verified) — no
    /// filesystem path, no per-model config; the id that routes generation also
    /// drives the load. Returns when unsloth reports it loaded (synchronous in
    /// practice). Backs `ai/inference/load`. `Err` = unreachable / refused.
    pub async fn load_model(&self, model: &str) -> Result<(), UnslothError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the `/v1` + trailing-slash normalization that prevents
    // double-`/v1` 405s. An operator-pasted `…/v1/` (or bare host, or trailing
    // slash) must all collapse to the same host root so `{base}/v1/...` is built
    // exactly once. Regression here = the 405 storm this single strip exists to kill.
    #[test]
    fn normalize_base_strips_v1_and_trailing_slash() {
        assert_eq!(normalize_base("http://h:8888"), "http://h:8888");
        assert_eq!(normalize_base("http://h:8888/"), "http://h:8888");
        assert_eq!(normalize_base("http://h:8888/v1"), "http://h:8888");
        assert_eq!(normalize_base("http://h:8888/v1/"), "http://h:8888");
        assert_eq!(normalize_base("http://h:8888/v1///"), "http://h:8888");
    }
}
