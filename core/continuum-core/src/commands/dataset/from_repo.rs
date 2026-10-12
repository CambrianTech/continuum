//! `dataset/from-repo` — a repository's source at one revision as a training dataset: the
//! source of a gene she did not produce herself. Writes to an arbitrary `outputDir` and
//! reads any local repository → `Privileged`.

use std::sync::Arc;

use crate::modules::dataset::{DatasetManifest, DatasetService, FromRepoParams};
use crate::sdk_codegen::CommandError;

crate::action_command! {
    /// Build a `{messages:[{role,content}]}` JSONL dataset from a repository's files at one
    /// revision (default HEAD), filtered by pathspecs and extensions: each example names the
    /// repository, commit, path and line span, and its completion is that span, cut on whole
    /// top-level items. Train it with `genome/job-create --datasetName <name>`.
    pub struct DatasetFromRepo { service: Arc<DatasetService> }
    name: "dataset/from-repo",
    access: Privileged,
    params: FromRepoParams,
    output: DatasetManifest,
    run(this, _ctx, p) => {
        // thousands of `git show`s: on a blocking thread, never a runtime worker
        let service = this.service.clone();
        tokio::task::spawn_blocking(move || service.from_repo(&p))
            .await
            .map_err(|e| CommandError::Internal(format!("dataset/from-repo did not finish: {e}")))?
            .map_err(CommandError::Internal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: name/access wiring — a reader of any local repo that writes to an
    // arbitrary outputDir is Privileged, never on the AiSafe surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(DatasetFromRepo::NAME, "dataset/from-repo");
        assert!(matches!(DatasetFromRepo::ACCESS, crate::sdk_codegen::AccessLevel::Privileged));
    }
}
