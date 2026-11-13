//! `dataset/from-captures` — build an SFT dataset from LIVE prompt-captures (the
//! glass-box turns the WorkspaceCycle writes). The rooms→training bridge for the
//! current cognition path. Writes to disk → `Privileged`.

use std::sync::Arc;

use crate::modules::dataset::{DatasetManifest, DatasetService, FromCapturesParams};
use crate::sdk_codegen::CommandError;

crate::action_command! {
    /// Convert live prompt-captures (default `~/.continuum/fixtures/prompt-captures`,
    /// one `<persona>.jsonl` per persona) into a chat SFT dataset. Structural curation
    /// only: drops empty responses, bare un-acted tool-call JSON envelopes, and
    /// system-only turns. Filterable by personaId / roomId. Errors loud if nothing
    /// usable remains.
    pub struct DatasetFromCaptures { service: Arc<DatasetService> }
    name: "dataset/from-captures",
    access: Privileged,
    params: FromCapturesParams,
    output: DatasetManifest,
    run(this, _ctx, p) => {
        this.service.from_captures(&p).map_err(CommandError::Internal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: name/access wiring — a dataset writer is Privileged.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(DatasetFromCaptures::NAME, "dataset/from-captures");
        assert!(matches!(
            DatasetFromCaptures::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }
}
