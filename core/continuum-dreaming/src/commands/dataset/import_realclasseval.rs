//! `dataset/import-realclasseval` — import the RealClassEval benchmark
//! (arxiv:2510.26130) into a structured JSONL dataset. Writes to disk → `Privileged`.

use std::sync::Arc;

use crate::modules::dataset::{DatasetManifest, DatasetService, ImportRealClassEvalParams};
use crate::sdk_codegen::CommandError;

crate::action_command! {
    /// Import RealClassEval (arxiv:2510.26130) from a cloned repo dir (auto-discovers
    /// the csn/post_cut-off CSVs + PYNGUIN tests) or a legacy single CSV + tests dir.
    /// Emits a class-implementation SFT dataset with cyclomatic-complexity / LOC
    /// metrics and pre/post-cutoff counts. Returns the manifest JSON.
    pub struct DatasetImportRealClassEval { service: Arc<DatasetService> }
    name: "dataset/import-realclasseval",
    access: Privileged,
    params: ImportRealClassEvalParams,
    output: DatasetManifest,
    run(this, _ctx, p) => {
        this.service.import_realclasseval(&p).map_err(CommandError::Internal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: name/access wiring — a dataset writer is Privileged.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(
            DatasetImportRealClassEval::NAME,
            "dataset/import-realclasseval"
        );
        assert!(matches!(
            DatasetImportRealClassEval::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }
    // The command boundary admits exactly one complete source, before disk access.
    #[test]
    fn realclasseval_source_is_complete_at_command_boundary() {
        use serde_json::json;
        for invalid in [
            json!({}),
            json!({"source": {"kind": "csv", "csvPath": "rows.csv"}}),
            json!({"source": {"kind": "repository"}}),
            json!({"source": {"kind": "csv", "csvPath": "rows.csv", "testsDir": null}}),
            json!({"source": {"kind": "repository", "repoDir": "repo", "csvPath": "rows.csv"}}),
        ] {
            assert!(serde_json::from_value::<ImportRealClassEvalParams>(invalid).is_err());
        }
        for source in [
            json!({"kind": "repository", "repoDir": "repo"}),
            json!({"kind": "csv", "csvPath": "rows.csv", "testsDir": "tests"}),
        ] {
            let params: ImportRealClassEvalParams =
                serde_json::from_value(json!({"source": source.clone()})).unwrap();
            assert_eq!(serde_json::to_value(params).unwrap()["source"], source);
        }
        let schema = serde_json::to_value(schemars::schema_for!(ImportRealClassEvalParams)).unwrap();
        assert!(schema["required"].as_array().unwrap().contains(&json!("source")));
    }

}
