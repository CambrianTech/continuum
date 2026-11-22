//! One base model, one name: every spelling a training request may carry for a catalog row (its
//! id, its trainable HF repo, the file stem of its resolved local GGUF) resolves to the row's
//! catalog id. Matching a request against a served row stays `artifacts::names_the_same_base`.

use std::path::Path;

/// The catalog id a training request's base names, read from the LIVE catalog (`live::ModelCatalog`,
/// mutated at runtime by `models/pull`, `models/register` and `attach_local_artifact`), so a model
/// pulled or registered after boot resolves at once with no restart. A first user's base is pulled
/// after boot, so the boot registry alone would refuse it until a reboot. The spellings: the row's
/// id, the trainable HF repo it declares (`Model::hf_source`), or the file stem of its local main
/// GGUF (an in-process engine names its model by the file it loaded). Anything else is refused, never
/// kept as given: a kept spelling is a second bucket for one base that no learner on that row can
/// match. The 5090, 2026-11-22: Kimi's reviewed correction (b38e0149, 8 examples) was keyed
/// `Qwen3.8-27B-Q4_K_M`, beside 26,407 keyed by the catalog id `ggml-org/Qwen3.8-27B-GGUF`.
pub fn canonical_base(catalog: &crate::model_registry::live::CatalogSnapshot, requested: &str) -> Result<String, BaseRefused> {
    if requested.trim().is_empty() {
        return Err(BaseRefused::Empty);
    }
    if catalog.get(requested).is_some() {
        return Ok(requested.to_owned());
    }
    let stem_of = |path: &Path| path.file_stem().and_then(|stem| stem.to_str()).map(str::to_owned);
    let mut rows: Vec<&str> = catalog
        .models
        .values()
        .map(|live| &live.model)
        .filter(|row| {
            row.hf_source.as_deref() == Some(requested)
                || row.gguf_local_path.as_deref().and_then(stem_of).as_deref() == Some(requested)
        })
        .map(|row| row.id.as_str())
        .collect();
    rows.sort_unstable();
    rows.dedup();
    match rows.as_slice() {
        [] => Err(BaseRefused::Unknown { requested: requested.to_owned() }),
        [one] => Ok((*one).to_owned()),
        _ => Err(BaseRefused::Ambiguous { requested: requested.to_owned(), rows: rows.into_iter().map(str::to_owned).collect() }),
    }
}

/// Why a base spelling names no single catalog row.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BaseRefused {
    #[error("no base model was named")]
    Empty,
    #[error("base model '{requested}' names no catalog row (by its id, its HF repo, or its local GGUF's file stem)")]
    Unknown { requested: String },
    #[error("base model '{requested}' names {} catalog rows ({}); name the row by its id", rows.len(), rows.join(", "))]
    Ambiguous { requested: String, rows: Vec<String> },
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (the 5090, 2026-11-22: Kimi's reviewed correction b38e0149 keyed by the GGUF
    // stem `Qwen3.8-27B-Q4_K_M`, beside 26,407 examples keyed by the catalog id; Joel: a first user
    // would always fail, and it must work without a restart): every spelling of a row resolves to its
    // catalog id on the LIVE catalog, a model's file stem resolves the moment its pull lands (no
    // restart), and an empty, unknown or shared spelling is refused rather than kept or guessed.
    #[test]
    fn every_spelling_resolves_on_the_live_catalog_and_a_pulled_model_needs_no_restart() {
        const SERVED: &str = "ggml-org/Qwen3.8-27B-GGUF";
        const STEM: &str = "Qwen3.8-27B-Q4_K_M";
        let home = tempfile::tempdir().expect("test: home"); // test fixture
        crate::paths::test_home::with_test_home(home.path(), || {
            // a fresh install: the catalog knows the row, and nothing of it is on this disk yet
            let row = || crate::model_registry::catalog::models().into_iter().find(|m| m.id == SERVED).expect("the catalog serves Qwen3.8-27B"); // test fixture
            let hf = row().hf_source.expect("the row declares its trainable base"); // test fixture
            let registry = crate::model_registry::Registry::from_catalog(vec![row()], crate::model_registry::catalog::providers()).expect("one catalog row loads"); // test fixture
            let live = crate::model_registry::live::ModelCatalog::from_registry(&registry);
            let served = Ok(SERVED.to_owned());
            assert_eq!(canonical_base(&live.snapshot(), SERVED), served, "the catalog id is itself");
            assert_eq!(canonical_base(&live.snapshot(), &hf), served, "the row's HF base resolves to it");
            assert_eq!(canonical_base(&live.snapshot(), STEM), Err(BaseRefused::Unknown { requested: STEM.into() }), "before the pull, no file names it");
            // the model's pull lands, as models/pull attaches it: the same process, no restart
            let pulled = home.path().join("models").join(format!("{STEM}.gguf"));
            std::fs::create_dir_all(pulled.parent().expect("test: a parent")).expect("test: models dir"); // test fixture
            crate::model_registry::artifacts::write_empty_gguf(&pulled);
            assert!(live.attach_local_artifact(SERVED, pulled, None), "test: the pull attaches its file");
            assert_eq!(canonical_base(&live.snapshot(), STEM), served, "after the pull its file stem resolves, with no restart");
            assert_eq!(canonical_base(&live.snapshot(), " "), Err(BaseRefused::Empty), "an empty spelling is refused");
            assert_eq!(canonical_base(&live.snapshot(), "qwen"), Err(BaseRefused::Unknown { requested: "qwen".into() }), "a spelling no row knows is refused, never kept");
            let mut twin = row();
            twin.id = "elsewhere/Qwen3.8-27B-GGUF".into();
            let shared = crate::model_registry::Registry::from_catalog(vec![row(), twin], crate::model_registry::catalog::providers()).expect("two rows load"); // test fixture
            assert_eq!(
                canonical_base(&crate::model_registry::live::ModelCatalog::from_registry(&shared).snapshot(), &hf),
                Err(BaseRefused::Ambiguous { requested: hf.clone(), rows: vec!["elsewhere/Qwen3.8-27B-GGUF".to_owned(), SERVED.to_owned()] }),
                "two rows share the HF base: never pick one"
            );
        });
    }
}
