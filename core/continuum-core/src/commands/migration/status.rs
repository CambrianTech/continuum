//! `migration/status` — current progress of the active migration.

use std::sync::Arc;

use super::MigrationControlParams;
use crate::modules::data::DataState;
use crate::orm::migration::MigrationProgress;

crate::action_command! {
    /// Report the live progress of the active migration — total / migrated /
    /// failed counts, paused & running flags, and a per-collection breakdown.
    /// A read of operator state, but the migration surface is operator-only, so
    /// gated `Internal`. Errors if no migration is active.
    pub struct MigrationStatusCmd { state: Arc<DataState> }
    name: "migration/status",
    access: Internal,
    params: MigrationControlParams,
    output: MigrationProgress,
    run(this, _ctx, _p) => {
        let result = this.state.migration_status().await?;
        Ok(result)
    }
}
