//! `migration/start` — begin a streaming migration between two storage adapters.

use std::sync::Arc;

use crate::data::DataState;
use crate::orm::migration::MigrationProgress;

fn default_batch_size() -> std::num::NonZeroUsize {
    crate::orm::migration::DEFAULT_BATCH_SIZE
}

/// Params for `migration/start`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/migration/MigrationStartParams.ts"
)]
pub struct MigrationStartParams {
    /// Source connection string to read from.
    pub source: String,
    /// Target connection string to write to.
    pub target: String,
    /// Records per page, positive (a zero page is refused at the wire); defaults to the migration owner's.
    #[serde(default = "default_batch_size")]
    #[ts(type = "number")]
    #[schemars(with = "u64")]
    pub batch_size: std::num::NonZeroUsize,
    /// Optional explicit collection list. Omit to migrate every collection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub collections: Option<Vec<String>>,
}

crate::action_command! {
    /// Begin a non-destructive streaming migration from the `source` adapter
    /// connection to the `target`, batch by batch, as a background task. Returns
    /// the initial progress snapshot immediately; poll `migration/status` for
    /// updates. Swaps storage backends, so gated `Internal` (operator-only).
    pub struct MigrationStart { state: Arc<DataState> }
    name: "migration/start",
    access: Internal,
    params: MigrationStartParams,
    output: MigrationProgress,
    run(this, _ctx, p) => {
        let result = this
            .state
            .migration_start(&p.source, &p.target, crate::orm::migration::MigrationConfig { batch_size: p.batch_size, collections: p.collections })
            .await?;
        Ok(result)
    }
}
