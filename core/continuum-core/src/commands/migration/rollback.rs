//! `migration/rollback` — revert a cutover to the previous connection.

use std::sync::Arc;

use crate::modules::data::{DataState, MigrationRollback};

/// Params for `migration/rollback`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/migration/MigrationRollbackParams.ts"
)]
pub struct MigrationRollbackParams {
    /// The connection string that was swapped in (to remove and revert from).
    pub current: String,
}

crate::action_command! {
    /// Revert the last `migration/cutover`: evict `current` and restore the
    /// previously-active connection recorded by that cutover. Redirects all
    /// subsequent operations, so gated `Internal` (operator-only). Errors if no
    /// previous connection was recorded.
    pub struct MigrationRollbackCmd { state: Arc<DataState> }
    name: "migration/rollback",
    access: Internal,
    params: MigrationRollbackParams,
    output: MigrationRollback,
    run(this, _ctx, p) => {
        let result = this.state.migration_rollback(p.current).await?;
        Ok(result)
    }
}
