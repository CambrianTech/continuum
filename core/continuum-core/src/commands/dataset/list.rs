//! `dataset/list` — list the datasets under the datasets root. A pure read →
//! `AiSafe`.

use std::sync::Arc;

use crate::modules::dataset::{DatasetListResult, DatasetService, ListDatasetsParams};
use crate::sdk_codegen::CommandError;

crate::action_command! {
    /// List the datasets under the datasets root (default `~/.continuum/datasets`),
    /// reading each subdirectory's `manifest.json`. Returns `{datasets, count, root}`.
    pub struct DatasetList { service: Arc<DatasetService> }
    name: "dataset/list",
    access: AiSafe,
    params: ListDatasetsParams,
    output: DatasetListResult,
    run(this, _ctx, p) => {
        this.service.list_datasets(&p).map_err(CommandError::Internal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: name/access wiring — a pure read is on the AiSafe surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(DatasetList::NAME, "dataset/list");
        assert!(matches!(
            DatasetList::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }
}
