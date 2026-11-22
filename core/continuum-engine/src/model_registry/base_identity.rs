//! One base model, one name: every spelling a training request may carry for a catalog row (its
//! id, its trainable HF repo, the file stem of its resolved local GGUF) resolves to the row's
//! catalog id. Matching a request against a served row stays `artifacts::names_the_same_base`.

use std::path::Path;

/// The catalog id a training request's base names, when the registry knows the spelling: the row's
/// own id, the trainable HF repo it declares (`Model::hf_source`), or the file stem of its resolved
/// local GGUF (an in-process engine names its model by the file it loaded). `Ok(None)`: no row
/// knows that spelling, and the caller keeps it as it came. The 5090, 2026-11-22: Kimi's reviewed
/// correction (b38e0149, 8 examples) was keyed `Qwen3.8-27B-Q4_K_M`, the stem of the file inside
/// `ggml-org/Qwen3.8-27B-GGUF`, and no learner on that row could ever match it.
pub fn canonical_base(requested: &str) -> Result<Option<String>, AmbiguousBase> {
    canonical_base_in(crate::model_registry::try_global(), requested)
}

/// A base spelling that names more than one catalog row: never resolved by picking one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmbiguousBase {
    pub requested: String,
    pub rows: Vec<String>,
}

impl std::fmt::Display for AmbiguousBase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "base model '{}' names {} catalog rows ({}); name the row by its id", self.requested, self.rows.len(), self.rows.join(", "))
    }
}

impl std::error::Error for AmbiguousBase {}

fn canonical_base_in(registry: Option<&crate::model_registry::Registry>, requested: &str) -> Result<Option<String>, AmbiguousBase> {
    let Some(registry) = registry else { return Ok(None) };
    if registry.model(requested).is_some() {
        return Ok(Some(requested.to_owned()));
    }
    let stem_of = |path: &Path| path.file_stem().and_then(|stem| stem.to_str()).map(str::to_owned);
    let mut rows: Vec<&str> = registry
        .models()
        .filter(|row| {
            row.hf_source.as_deref() == Some(requested)
                || row.gguf_local_path.as_deref().and_then(stem_of).as_deref() == Some(requested)
        })
        .map(|row| row.id.as_str())
        .collect();
    rows.sort_unstable();
    rows.dedup();
    match rows.as_slice() {
        [] => Ok(None),
        [one] => Ok(Some((*one).to_owned())),
        _ => Err(AmbiguousBase { requested: requested.to_owned(), rows: rows.into_iter().map(str::to_owned).collect() }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (the 5090, 2026-11-22: Kimi's reviewed correction b38e0149 keyed by the GGUF
    // stem `Qwen3.8-27B-Q4_K_M`, unlearnable beside 26,407 examples keyed by the catalog id): every
    // spelling of one row resolves to its catalog id, an unknown spelling is left to the caller, and
    // a spelling two rows share is refused rather than picking one.
    #[test]
    fn every_spelling_of_a_row_resolves_to_its_catalog_id_and_a_shared_one_is_refused() {
        const SERVED: &str = "ggml-org/Qwen3.8-27B-GGUF";
        let home = tempfile::tempdir().expect("test: home"); // test fixture
        crate::paths::test_home::with_test_home(home.path(), || {
            // the row's file where resolution finds it, as on the 5090: the repo's snapshot in the HF cache
            let snap = home.path().join(".cache/huggingface/hub/models--ggml-org--Qwen3.8-27B-GGUF/snapshots/0669b98");
            std::fs::create_dir_all(&snap).expect("test: snapshot dir"); // test fixture
            crate::model_registry::artifacts::write_empty_gguf(&snap.join("Qwen3.8-27B-Q4_K_M.gguf"));
            let row = || crate::model_registry::catalog::models().into_iter().find(|m| m.id == SERVED).expect("the catalog serves Qwen3.8-27B"); // test fixture
            let hf = row().hf_source.expect("the row declares its trainable base"); // test fixture
            let registry = crate::model_registry::Registry::from_catalog(vec![row()], crate::model_registry::catalog::providers()).expect("one catalog row loads"); // test fixture
            let served = Ok(Some(SERVED.to_owned()));
            assert_eq!(canonical_base_in(Some(&registry), SERVED), served, "the catalog id is itself");
            assert_eq!(canonical_base_in(Some(&registry), &hf), served, "the row's HF base resolves to it");
            assert_eq!(canonical_base_in(Some(&registry), "Qwen3.8-27B-Q4_K_M"), served, "the stem of its resolved local GGUF resolves to it");
            assert_eq!(canonical_base_in(Some(&registry), "qwen"), Ok(None), "a spelling no row knows is left as it came");
            assert_eq!(canonical_base_in(None, "Qwen3.8-27B-Q4_K_M"), Ok(None), "with no registry nothing is resolved");
            let mut twin = row();
            twin.id = "elsewhere/Qwen3.8-27B-GGUF".into();
            let shared = crate::model_registry::Registry::from_catalog(vec![row(), twin], crate::model_registry::catalog::providers()).expect("two rows load"); // test fixture
            let refused = canonical_base_in(Some(&shared), &hf).expect_err("two rows share the HF base: never pick one"); // test fixture
            assert_eq!(refused.rows, vec!["elsewhere/Qwen3.8-27B-GGUF".to_owned(), SERVED.to_owned()]);
        });
    }
}
