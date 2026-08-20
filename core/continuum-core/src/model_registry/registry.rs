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
    /// Models whose ARTIFACT could not be hydrated, id → why. Kept, never
    /// silently dropped: a model that exists in the catalog and cannot be used
    /// is a different fact from a model that was never registered, and an
    /// operator has to be able to tell them apart. See `from_catalog`.
    unhydratable: std::collections::BTreeMap<String, String>,
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
        let mut unhydratable: std::collections::BTreeMap<String, String> = Default::default();
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
            // ONE BAD ARTIFACT MUST NOT TAKE DOWN THE SUBSTRATE (#63).
            //
            // This used to `return Err(GgufHydration)`, which aborted the whole registry
            // and panicked the core at ipc/mod.rs — before a single ServiceModule was
            // constructed. Measured live 2026-08-07: a catalog entry for an IQ2_XXS GGUF
            // our reader cannot parse ("unknown dtype for tensor 16") made the core
            // UNBOOTABLE. `continuum ping` answered "no core running", and nothing
            // recovered it but editing the catalog — while citizens were mid-work.
            //
            // The panic's justification is sound for what it was written about: a missing
            // registry config IS a boot-order/packaging bug, not a runtime condition. But
            // "a model's on-disk artifact does not parse" is EXACTLY a runtime condition —
            // an operator downloaded a file, and files are not our code. Conflating the two
            // made every acquirable artifact a potential boot-killer on a substrate whose
            // whole job is keeping citizens alive.
            //
            // So: degrade THIS model, keep the core. Deliberately NOT inserted half-
            // hydrated — a model carrying an unhydrated context_window would hand a
            // planner a plausible wrong number, which is worse than absent. It is recorded
            // in `unhydratable` WITH the parser's own words, so the fact surfaces from a
            // living system instead of being inferred from a dead process.
            if let Err(detail) = super::hydrate::hydrate_model_from_gguf(&mut m) {
                unhydratable.insert(m.id.clone(), detail);
                continue;
            }
            models.insert(m.id.clone(), m);
        }

        Ok(Self {
            models,
            providers,
            unhydratable,
        })
    }

    /// Models present in the catalog whose artifact could not be hydrated, id → reason.
    /// Empty on a healthy node. Non-empty is a REPORTABLE condition, not a fatal one.
    pub fn unhydratable(&self) -> &std::collections::BTreeMap<String, String> {
        &self.unhydratable
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
    #[error(
        "model `{model_id}` left a queryable field absent and its GGUF could not supply it: {detail}"
    )]
    GgufHydration { model_id: String, detail: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_registry::catalog;
    // Canonical valid-empty-GGUF stand-in writer lives with the resolver it
    // supports (artifacts.rs) — one truth, shared by every resolution test.
    use crate::model_registry::artifacts::write_empty_gguf;
    use crate::model_registry::types::{Arch, Capability};

    // what this catches: ONE unparseable artifact must NOT take down the substrate (#63).
    // Live incident 2026-08-07 — a catalog entry whose GGUF our reader cannot parse
    // ("unknown dtype for tensor 16") aborted the whole registry and PANICKED the core at
    // ipc/mod.rs, before any ServiceModule was constructed. `continuum ping` answered "no
    // core running" and nothing recovered it but editing the catalog, while citizens were
    // mid-work. A booting core with N-1 usable models is the requirement; a panic is not.
    //
    // The degraded model must also be REPORTABLE, not merely skipped: "registered but
    // unusable" and "never registered" are different facts, and an operator has to be able
    // to tell them apart. So it lands in `unhydratable` with the parser's own words.
    #[test]
    fn one_unparseable_artifact_degrades_that_model_and_never_the_core() {
        let mut good = catalog::models()
            .into_iter()
            .find(|m| m.gguf_local_path.is_none())
            .expect("a catalog model with no local artifact hydrates trivially");
        good.id = "healthy/model".to_string();

        // A file that EXISTS and is not a parseable GGUF — the live shape. An absent
        // artifact is a different (already-tolerated) case; this is bytes we cannot read.
        let dir = tempfile::tempdir().expect("tempdir");
        let bad_path = dir.path().join("unreadable.gguf");
        std::fs::write(&bad_path, b"GGUF\x03\x00\x00\x00not-a-real-header").expect("write");
        let mut bad = good.clone();
        bad.id = "broken/artifact".to_string();
        bad.gguf_local_path = Some(bad_path);

        let reg = Registry::from_catalog(vec![good, bad], catalog::providers())
            .expect("a bad artifact must not abort registry construction");

        assert!(
            reg.model("healthy/model").is_some(),
            "every other model stays usable"
        );
        assert!(
            reg.model("broken/artifact").is_none(),
            "an unhydratable model is NOT served half-hydrated — a plausible wrong \
             context_window is worse than an absent model"
        );
        let why = reg
            .unhydratable()
            .get("broken/artifact")
            .expect("the degraded model is REPORTED, never silently dropped");
        assert!(
            !why.is_empty(),
            "the reason carries the parser's own words: {why}"
        );
    }

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
        // Run under a clean, serialized HOME. `catalog::registry()` resolves
        // each local model's GGUF from the HF cache under HOME and hydrates its
        // header at load (#74 Slice 1). Reading the *ambient* HOME made this
        // test environment-dependent (#72): a concurrent `with_test_home` in
        // another test points HOME at a temp dir seeded with a fake forged-Qwen
        // GGUF, which this test would then resolve and hydrate mid-run. Sharing
        // `with_test_home`'s lock + empty HOME both serializes against every
        // HOME mutation and pins a deterministic environment where no local
        // GGUF resolves, so the hand-authored catalog values (arch, context
        // window, sensory caps) stand on their own.
        let home = tempfile::tempdir().unwrap();
        crate::model_registry::artifacts::with_test_home(home.path(), || {
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
        });
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
            write_empty_gguf(&gguf);

            let mut forged = catalog::models()
                .into_iter()
                .find(|m| m.id == "continuum-ai/qwen3.5-4b-code-forged-GGUF")
                .expect("forged qwen3.5 in catalog");
            // Pin a stale path that does not exist; resolution must fall
            // through to the HF cache discovered via gguf_hint.
            forged.gguf_local_path = Some(std::path::PathBuf::from(
                "~/missing/docker/bundle/model.gguf",
            ));

            let reg = Registry::from_catalog(vec![forged], catalog::providers())
                .expect("registry should load");
            let model = reg
                .model("continuum-ai/qwen3.5-4b-code-forged-GGUF")
                .expect("model registered");
            assert_eq!(model.gguf_local_path.as_deref(), Some(gguf.as_path()));
        });
    }

    // what this catches: the coder-14b teacher resolves its serving GGUF by DERIVING
    // the path from its id under ~/.continuum/genome/models/ — with NO hardcoded
    // gguf_local_path in the catalog (removed). Proves the unhardcoding: drop the
    // baked absolute path + quant, and the registry still finds the model by id
    // (where download-models.sh lands it).
    #[test]
    fn coder_14b_resolves_by_id_derivation_without_hardcoded_path() {
        let home = tempfile::tempdir().unwrap();
        crate::model_registry::artifacts::with_test_home(home.path(), || {
            // Place the GGUF where the provisioner lands it: genome/models/<dir>/.
            let dir = home
                .path()
                .join(".continuum/genome/models/qwen2.5-coder-14b-instruct");
            std::fs::create_dir_all(&dir).unwrap();
            write_empty_gguf(&dir.join("Qwen2.5-Coder-14B-Instruct-Q4_K_M.gguf"));

            let spec = catalog::models()
                .into_iter()
                .find(|m| m.id == "continuum-ai/qwen2.5-coder-14b-instruct-GGUF")
                .expect("coder-14b in catalog");
            // The catalog carries NO hardcoded path anymore.
            assert!(
                spec.gguf_local_path.is_none(),
                "coder-14b catalog spec must have no hardcoded gguf_local_path"
            );

            let reg =
                Registry::from_catalog(vec![spec], catalog::providers()).expect("registry loads");
            let model = reg
                .model("continuum-ai/qwen2.5-coder-14b-instruct-GGUF")
                .expect("coder-14b registered");
            let resolved = model
                .gguf_local_path
                .as_deref()
                .expect("must resolve a GGUF by id-derivation");
            assert!(
                resolved.ends_with("Qwen2.5-Coder-14B-Instruct-Q4_K_M.gguf"),
                "resolved by id-derivation under genome/models, got {resolved:?}"
            );
        });
    }
}
