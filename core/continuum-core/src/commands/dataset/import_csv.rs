//! `dataset/import-csv` — import a generic CSV into a split JSONL training
//! dataset. Writes to an arbitrary `outputDir` → `Privileged`.

use std::sync::Arc;

use crate::modules::dataset::{DatasetService, ImportCsvParams};
use crate::sdk_codegen::CommandError;

crate::action_command! {
    /// Import a generic CSV (one user-column + one assistant-column) into a
    /// `{messages:[{role,content}]}` JSONL dataset, split into train/eval, with a
    /// manifest. Returns the manifest JSON. Empty rows are skipped.
    pub struct DatasetImportCsv { service: Arc<DatasetService> }
    name: "dataset/import-csv",
    access: Privileged,
    params: ImportCsvParams,
    output: serde_json::Value,
    run(this, _ctx, p) => {
        this.service.import_csv(&p).map_err(CommandError::Internal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: name/access wiring — a dataset writer is Privileged, not on
    // the AiSafe surface (it writes to an arbitrary outputDir).
    #[test]
    fn name_and_access_wired() {
        assert_eq!(DatasetImportCsv::NAME, "dataset/import-csv");
        assert!(matches!(
            DatasetImportCsv::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }
}
