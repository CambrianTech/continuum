//! `dataset/info` — read one named dataset's manifest. A pure read → `AiSafe`.

use std::sync::Arc;

use crate::modules::dataset::{DatasetInfoParams, DatasetManifest, DatasetService};
use crate::sdk_codegen::CommandError;

crate::action_command! {
    /// Read the manifest for a named dataset under the datasets root. Returns the
    /// manifest JSON. Errors loud if the dataset is not found.
    pub struct DatasetInfo { service: Arc<DatasetService> }
    name: "dataset/info",
    access: AiSafe,
    params: DatasetInfoParams,
    output: DatasetManifest,
    run(this, _ctx, p) => {
        this.service.dataset_info(&p).map_err(CommandError::Internal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: name/access wiring — a pure read is on the AiSafe surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(DatasetInfo::NAME, "dataset/info");
        assert!(matches!(
            DatasetInfo::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }
}
