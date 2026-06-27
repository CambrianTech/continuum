//! `dataset/from-turns` — build an SFT dataset from recorded persona turns (the
//! recorder's per-turn JSON). The rooms→training-data bridge. Writes to disk →
//! `Privileged`.

use std::sync::Arc;

use crate::modules::dataset::{DatasetService, FromTurnsParams};
use crate::sdk_codegen::CommandError;

crate::action_command! {
    /// Convert recorded persona turns (the recorder's per-turn JSON, default
    /// `~/.continuum/fixtures/persona-respond`) into a chat SFT dataset. Only `spoke`
    /// turns become a (system?+history?+user→assistant) pair; silent/errored turns are
    /// skipped. Filterable by personaId / roomId. Errors loud if no spoke turns remain.
    pub struct DatasetFromTurns { service: Arc<DatasetService> }
    name: "dataset/from-turns",
    access: Privileged,
    params: FromTurnsParams,
    output: serde_json::Value,
    run(this, _ctx, p) => {
        this.service.from_turns(&p).map_err(CommandError::Internal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: name/access wiring — a dataset writer is Privileged.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(DatasetFromTurns::NAME, "dataset/from-turns");
        assert!(matches!(
            DatasetFromTurns::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }
}
