//! `migration/pause` — pause the active migration.

use std::sync::Arc;

use super::MigrationControlParams;
use crate::modules::data::DataState;
use crate::orm::migration::MigrationProgress;

crate::action_command! {
    /// Pause the active migration (sets an atomic flag the migration loop checks
    /// between batches). Returns the progress snapshot at pause. Operator-only, so
    /// gated `Internal`. Errors if no migration is active.
    pub struct MigrationPause { state: Arc<DataState> }
    name: "migration/pause",
    access: Internal,
    params: MigrationControlParams,
    output: MigrationProgress,
    run(this, _ctx, _p) => {
        let result = this.state.migration_pause().await?;
        Ok(result)
    }
}
