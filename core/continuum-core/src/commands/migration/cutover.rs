//! `migration/cutover` — swap the active backend to the migrated connection.

use std::sync::Arc;

use crate::modules::data::{DataState, MigrationCutover};

/// Params for `migration/cutover`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/migration/MigrationCutoverParams.ts"
)]
pub struct MigrationCutoverParams {
    /// The connection string currently in use, to swap out (kept for rollback).
    pub current: String,
    /// The new connection string to make active.
    pub target: String,
}

crate::action_command! {
    /// Cut over the active storage backend from `current` to `target`, evicting
    /// the old adapter from cache and pre-warming the new one. Records `current`
    /// so `migration/rollback` can revert. Redirects all subsequent operations, so
    /// gated `Internal` (operator-only).
    pub struct MigrationCutoverCmd { state: Arc<DataState> }
    name: "migration/cutover",
    access: Internal,
    params: MigrationCutoverParams,
    output: MigrationCutover,
    run(this, _ctx, p) => {
        let result = this.state.migration_cutover(p.current, p.target).await?;
        Ok(result)
    }
}
