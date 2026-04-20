//! Registry loader — parses `models.toml` + `providers.toml` into typed
//! `Model` / `Provider` records, validates cross-references, and
//! resolves local GGUF paths from DMR's on-disk manifest when possible.
//!
//! Entry points:
//! - [`load_registry`] — single call, returns a validated `Registry`.
//! - [`load_models`] / [`load_providers`] — lower-level, parse one file.
//!
//! Errors are typed. A missing file, a malformed row, or a model whose
//! `provider` doesn't resolve to a registered `Provider` — each gets its
//! own variant so the caller's logs pinpoint the issue.

use super::types::{Model, Provider};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Runtime registry. One process loads this once at startup. Everything
/// downstream looks things up here; the hash maps give O(1) lookups by
/// id.
#[derive(Debug, Clone)]
pub struct Registry {
    models: HashMap<String, Model>,
    providers: HashMap<String, Provider>,
}

impl Registry {
    pub fn model(&self, id: &str) -> Option<&Model> {
        self.models.get(id)
    }

    pub fn provider(&self, id: &str) -> Option<&Provider> {
        self.providers.get(id)
    }

    pub fn models(&self) -> impl Iterator<Item = &Model> {
        self.models.values()
    }

    pub fn providers(&self) -> impl Iterator<Item = &Provider> {
        self.providers.values()
    }

    pub fn models_for_provider<'a>(&'a self, provider_id: &'a str) -> impl Iterator<Item = &'a Model> + 'a {
        self.models.values().filter(move |m| m.provider == provider_id)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("reading {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("parsing {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error(
        "model `{model_id}` references provider `{provider_id}` which is not registered. \
         Add the provider to providers.toml or correct the model's `provider` field."
    )]
    UnknownProvider {
        model_id: String,
        provider_id: String,
    },
    #[error("duplicate model id `{id}` — each model must appear exactly once in models.toml")]
    DuplicateModel { id: String },
    #[error("duplicate provider id `{id}` — each provider must appear exactly once in providers.toml")]
    DuplicateProvider { id: String },
}

// Envelope structs — TOML files use a top-level `[[model]]` / `[[provider]]`
// array-of-tables. The envelope is private; consumers receive `Vec<Model>`.
#[derive(Deserialize)]
struct ModelsFile {
    #[serde(rename = "model", default)]
    models: Vec<Model>,
}

#[derive(Deserialize)]
struct ProvidersFile {
    #[serde(rename = "provider", default)]
    providers: Vec<Provider>,
}

pub fn load_models(path: impl AsRef<Path>) -> Result<Vec<Model>, RegistryError> {
    let path = path.as_ref().to_path_buf();
    let text = fs::read_to_string(&path).map_err(|source| RegistryError::Io {
        path: path.clone(),
        source,
    })?;
    let file: ModelsFile = toml::from_str(&text).map_err(|source| RegistryError::Parse {
        path: path.clone(),
        source,
    })?;
    Ok(file.models)
}

pub fn load_providers(path: impl AsRef<Path>) -> Result<Vec<Provider>, RegistryError> {
    let path = path.as_ref().to_path_buf();
    let text = fs::read_to_string(&path).map_err(|source| RegistryError::Io {
        path: path.clone(),
        source,
    })?;
    let file: ProvidersFile = toml::from_str(&text).map_err(|source| RegistryError::Parse {
        path: path.clone(),
        source,
    })?;
    Ok(file.providers)
}

/// Load + validate both files into a `Registry`. Ensures:
/// - no duplicate model ids
/// - no duplicate provider ids
/// - every `Model.provider` resolves to a registered provider
///
/// Does NOT attempt to resolve `gguf_local_path` — that's a DMR-manifest
/// concern handled after load. See [`resolve_local_gguf_paths`] for the
/// optional post-load pass that does it.
pub fn load_registry(
    models_path: impl AsRef<Path>,
    providers_path: impl AsRef<Path>,
) -> Result<Registry, RegistryError> {
    let raw_models = load_models(models_path)?;
    let raw_providers = load_providers(providers_path)?;

    let mut providers: HashMap<String, Provider> = HashMap::with_capacity(raw_providers.len());
    for p in raw_providers {
        if providers.contains_key(&p.id) {
            return Err(RegistryError::DuplicateProvider { id: p.id });
        }
        providers.insert(p.id.clone(), p);
    }

    let mut models: HashMap<String, Model> = HashMap::with_capacity(raw_models.len());
    for m in raw_models {
        if models.contains_key(&m.id) {
            return Err(RegistryError::DuplicateModel { id: m.id });
        }
        if !providers.contains_key(&m.provider) {
            return Err(RegistryError::UnknownProvider {
                model_id: m.id,
                provider_id: m.provider,
            });
        }
        models.insert(m.id.clone(), m);
    }

    Ok(Registry { models, providers })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_registry::types::{Arch, AuthKind, Capability};

    fn write(dir: &Path, name: &str, contents: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, contents).unwrap();
        p
    }

    #[test]
    fn parses_and_validates_canonical_pair() {
        let dir = tempfile::tempdir().unwrap();
        let mp = write(
            dir.path(),
            "models.toml",
            r#"
[[model]]
id = "continuum-ai/qwen3.5-4b-code-forged-GGUF"
provider = "docker-model-runner"
arch = "qwen35"
context_window = 262144
max_output_tokens = 32768
tokens_per_second = 33.0
capabilities = ["text-generation", "chat", "tool-use"]
gguf_hint = "huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf"

[[model]]
id = "claude-sonnet-4-5-20250929"
provider = "anthropic"
arch = "claude"
context_window = 200000
max_output_tokens = 8192
tokens_per_second = 80.0
capabilities = ["text-generation", "chat", "tool-use", "vision", "streaming"]
cost_input_per_1k = 0.003
cost_output_per_1k = 0.015
"#,
        );
        let pp = write(
            dir.path(),
            "providers.toml",
            r#"
[[provider]]
id = "docker-model-runner"
base_url = "http://localhost:12434/engines/llama.cpp"
auth = "none"

[[provider]]
id = "anthropic"
base_url = "https://api.anthropic.com/v1"
api_key_env = "ANTHROPIC_API_KEY"
default_model = "claude-sonnet-4-5-20250929"
auth = "api_key"
"#,
        );
        let reg = load_registry(mp, pp).expect("registry should load");
        let qwen = reg
            .model("continuum-ai/qwen3.5-4b-code-forged-GGUF")
            .expect("qwen registered");
        assert_eq!(qwen.arch, Arch::Qwen35);
        assert!(qwen.has(Capability::ToolUse));
        assert!(!qwen.has(Capability::Vision));
        assert_eq!(qwen.context_window, 262144);

        let claude = reg.model("claude-sonnet-4-5-20250929").expect("claude registered");
        assert!(claude.has(Capability::Vision));
        assert_eq!(claude.cost_input_per_1k, 0.003);

        let anthropic = reg.provider("anthropic").expect("anthropic provider");
        assert_eq!(anthropic.auth, AuthKind::ApiKey);
        assert_eq!(anthropic.api_key_env.as_deref(), Some("ANTHROPIC_API_KEY"));

        let dmr = reg.provider("docker-model-runner").expect("dmr provider");
        assert_eq!(dmr.auth, AuthKind::None);
        assert!(dmr.default_model.is_none());
    }

    #[test]
    fn rejects_duplicate_model_ids() {
        let dir = tempfile::tempdir().unwrap();
        let mp = write(
            dir.path(),
            "models.toml",
            r#"
[[model]]
id = "dup"
provider = "p"
arch = "unknown"
context_window = 1
max_output_tokens = 1
tokens_per_second = 1.0

[[model]]
id = "dup"
provider = "p"
arch = "unknown"
context_window = 2
max_output_tokens = 2
tokens_per_second = 2.0
"#,
        );
        let pp = write(
            dir.path(),
            "providers.toml",
            r#"
[[provider]]
id = "p"
base_url = "http://x"
auth = "none"
"#,
        );
        match load_registry(mp, pp) {
            Err(RegistryError::DuplicateModel { id }) => assert_eq!(id, "dup"),
            other => panic!("expected DuplicateModel, got {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_provider_ref() {
        let dir = tempfile::tempdir().unwrap();
        let mp = write(
            dir.path(),
            "models.toml",
            r#"
[[model]]
id = "orphan"
provider = "missing"
arch = "unknown"
context_window = 1
max_output_tokens = 1
tokens_per_second = 1.0
"#,
        );
        let pp = write(dir.path(), "providers.toml", "");
        match load_registry(mp, pp) {
            Err(RegistryError::UnknownProvider {
                model_id,
                provider_id,
            }) => {
                assert_eq!(model_id, "orphan");
                assert_eq!(provider_id, "missing");
            }
            other => panic!("expected UnknownProvider, got {other:?}"),
        }
    }
}
