//! The live `Registry` — the in-memory lookup container the whole process
//! reads from. Built ONCE at boot from the Rust catalog
//! ([`super::catalog::registry`]) via [`Registry::from_catalog`], then
//! exposed immutably through the [`super::singleton`] global.
//!
//! The Rust catalog (`catalog.rs`) is the single source of truth for model
//! and provider facts. There is no TOML loader: model data is hand-authored
//! for the residue no query can supply (cost, tokens/sec, curated capability
//! overrides) and — for the rest — hydrated from each artifact's own
//! authoritative metadata (GGUF headers, provider `/v1/models`). `Registry`
//! is just the validated index over that catalog.

use super::artifacts::resolve_model_artifacts;
use super::types::{Model, Provider};
use std::collections::HashMap;

/// Runtime registry. One process loads this once at startup. Everything
/// downstream looks things up here; the hash maps give O(1) lookups by
/// id.
#[derive(Debug, Clone)]
pub struct Registry {
    models: HashMap<String, Model>,
    providers: HashMap<String, Provider>,
}

impl Registry {
    /// Validate a raw catalog into an indexed `Registry`. Ensures:
    /// - no duplicate provider ids
    /// - no duplicate model ids
    /// - every `Model.provider` resolves to a registered provider
    ///
    /// Resolves each model's local GGUF/mmproj paths from an explicit
    /// `gguf_local_path` or the Hugging Face cache implied by `gguf_hint`.
    /// A hand-pinned local path is only authoritative when it exists; a
    /// stale machine-specific Docker bundle path must not make an
    /// already-downloaded model invisible.
    pub fn from_catalog(
        raw_models: Vec<Model>,
        raw_providers: Vec<Provider>,
    ) -> Result<Self, RegistryError> {
        let mut providers: HashMap<String, Provider> = HashMap::with_capacity(raw_providers.len());
        for p in raw_providers {
            if providers.contains_key(&p.id) {
                return Err(RegistryError::DuplicateProvider { id: p.id });
            }
            providers.insert(p.id.clone(), p);
        }

        let mut models: HashMap<String, Model> = HashMap::with_capacity(raw_models.len());
        for mut m in raw_models {
            if models.contains_key(&m.id) {
                return Err(RegistryError::DuplicateModel { id: m.id });
            }
            if !providers.contains_key(&m.provider) {
                return Err(RegistryError::UnknownProvider {
                    model_id: m.id,
                    provider_id: m.provider,
                });
            }
            resolve_model_artifacts(&mut m);
            models.insert(m.id.clone(), m);
        }

        Ok(Self { models, providers })
    }

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

    pub fn models_for_provider<'a>(
        &'a self,
        provider_id: &'a str,
    ) -> impl Iterator<Item = &'a Model> + 'a {
        self.models
            .values()
            .filter(move |m| m.provider == provider_id)
    }
}

/// Catalog validation failures. Every variant names the offending id so a
/// catalog typo points straight at the row to fix in `catalog.rs`.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error(
        "model `{model_id}` references provider `{provider_id}` which is not registered. \
         Add the provider to the Rust catalog (catalog.rs) or correct the model's `provider` field."
    )]
    UnknownProvider {
        model_id: String,
        provider_id: String,
    },
    #[error("duplicate model id `{id}` — each model must appear exactly once in the Rust catalog (catalog.rs)")]
    DuplicateModel { id: String },
    #[error(
        "duplicate provider id `{id}` — each provider must appear exactly once in the Rust catalog (catalog.rs)"
    )]
    DuplicateProvider { id: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_registry::catalog;
    use crate::model_registry::types::{Arch, Capability};

    // what this catches: from_catalog must reject two models sharing an id
    // rather than silently letting the second clobber the first.
    #[test]
    fn rejects_duplicate_model_ids() {
        let m = catalog::models()
            .into_iter()
            .next()
            .expect("catalog has at least one model");
        match Registry::from_catalog(vec![m.clone(), m], catalog::providers()) {
            Err(RegistryError::DuplicateModel { id }) => assert!(!id.is_empty()),
            other => panic!("expected DuplicateModel, got {other:?}"),
        }
    }

    // what this catches: from_catalog must reject a provider id that appears
    // twice (a copy-paste error in catalog.rs would otherwise pass silently).
    #[test]
    fn rejects_duplicate_provider_ids() {
        let p = catalog::providers()
            .into_iter()
            .next()
            .expect("catalog has at least one provider");
        match Registry::from_catalog(vec![], vec![p.clone(), p]) {
            Err(RegistryError::DuplicateProvider { id }) => assert!(!id.is_empty()),
            other => panic!("expected DuplicateProvider, got {other:?}"),
        }
    }

    // what this catches: a model whose `provider` doesn't resolve to a
    // registered provider must fail loud at load, not ship a dangling ref.
    #[test]
    fn rejects_unknown_provider_ref() {
        let mut m = catalog::models()
            .into_iter()
            .next()
            .expect("catalog has at least one model");
        m.provider = "definitely-not-a-registered-provider".to_string();
        let orphan_id = m.id.clone();
        match Registry::from_catalog(vec![m], catalog::providers()) {
            Err(RegistryError::UnknownProvider {
                model_id,
                provider_id,
            }) => {
                assert_eq!(model_id, orphan_id);
                assert_eq!(provider_id, "definitely-not-a-registered-provider");
            }
            other => panic!("expected UnknownProvider, got {other:?}"),
        }
    }

    // what this catches: the forged Qwen3.5-4B row must carry a chatml
    // chat_template (the llamacpp adapter reads it through the registry
    // instead of a per-model const). If the catalog edit drops it or breaks
    // the boundary tokens, special-token fragments bleed into chat output.
    #[test]
    fn forged_qwen35_carries_chatml_chat_template() {
        let reg = catalog::registry().expect("Rust catalog must validate");
        let forged = reg
            .model("continuum-ai/qwen3.5-4b-code-forged-GGUF")
            .expect("forged qwen3.5 in catalog");
        let tmpl = forged
            .chat_template
            .as_deref()
            .expect("forged qwen3.5 must carry a chat_template — adapter depends on it");
        assert!(
            tmpl.contains("<|im_start|>"),
            "chatml template missing <|im_start|>: {tmpl}"
        );
        assert!(
            tmpl.contains("<|im_end|>"),
            "chatml template missing <|im_end|>: {tmpl}"
        );
        assert!(
            tmpl.contains("add_generation_prompt"),
            "chatml template missing add_generation_prompt branch: {tmpl}"
        );
    }

    // what this catches: the Rust catalog must validate end-to-end and keep
    // the anchor models with their known archs/capabilities. A catalog edit
    // that drops Sonnet's Vision, the forged Qwen's context window, or the
    // Omni sensory caps gets caught here.
    #[test]
    fn rust_catalog_validates_with_anchor_models() {
        let reg = catalog::registry().expect("Rust catalog must always validate");
        assert!(reg.providers().count() >= 8);
        assert!(reg.models().count() >= 12);

        let sonnet = reg
            .model("claude-sonnet-4-5-20250929")
            .expect("Claude Sonnet 4.5 must be in the catalog");
        assert_eq!(sonnet.arch, Arch::Claude);
        assert!(sonnet.has(Capability::Vision));
        assert!(sonnet.has(Capability::ToolUse));

        let forged = reg
            .model("continuum-ai/qwen3.5-4b-code-forged-GGUF")
            .expect("forged Qwen3.5-4B must be in the catalog");
        assert_eq!(forged.arch, Arch::Qwen35);
        assert_eq!(forged.context_window, 262144);

        let omni = reg
            .model("qwen2.5-omni-7b-instruct")
            .expect("Qwen2.5-Omni-7B sensory-input model must be in the catalog");
        assert_eq!(omni.provider, "llamacpp-local");
        assert_eq!(omni.arch, Arch::Qwen2);
        assert!(omni.has(Capability::Vision));
        assert!(omni.has(Capability::AudioInput));
        assert!(
            !omni.has(Capability::AudioOutput),
            "GGUF admission must not claim native audio output until it is validated"
        );
        assert!(
            omni.mmproj_local_path.is_some(),
            "local sensory-input admission requires an mmproj path"
        );

        assert!(
            reg.model("qwen2-vl-7b-instruct").is_some(),
            "Rust catalog must own the vetted local vision model"
        );
    }

    // what this catches: from_catalog resolves a model's local GGUF from the
    // Hugging Face cache via `gguf_hint` when the hand-pinned
    // `gguf_local_path` is stale/absent — a stale Docker bundle path must not
    // hide an already-downloaded model.
    #[test]
    fn resolves_gguf_hint_from_hf_cache_when_local_path_stale() {
        let home = tempfile::tempdir().unwrap();
        crate::model_registry::artifacts::with_test_home(home.path(), || {
            let cached = home.path().join(
                ".cache/huggingface/hub/models--continuum-ai--qwen3.5-4b-code-forged-GGUF/snapshots/abc",
            );
            std::fs::create_dir_all(&cached).unwrap();
            let gguf = cached.join("qwen3.5-4b-code-forged-Q4_K_M.gguf");
            std::fs::write(&gguf, b"gguf").unwrap();

            let mut forged = catalog::models()
                .into_iter()
                .find(|m| m.id == "continuum-ai/qwen3.5-4b-code-forged-GGUF")
                .expect("forged qwen3.5 in catalog");
            // Pin a stale path that does not exist; resolution must fall
            // through to the HF cache discovered via gguf_hint.
            forged.gguf_local_path = Some(std::path::PathBuf::from("~/missing/docker/bundle/model.gguf"));

            let reg = Registry::from_catalog(vec![forged], catalog::providers())
                .expect("registry should load");
            let model = reg
                .model("continuum-ai/qwen3.5-4b-code-forged-GGUF")
                .expect("model registered");
            assert_eq!(model.gguf_local_path.as_deref(), Some(gguf.as_path()));
        });
    }
}
