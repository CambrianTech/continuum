//! `migration/resume` — resume a paused migration.

use std::sync::Arc;

use super::MigrationControlParams;
use crate::modules::data::DataState;
use crate::orm::migration::MigrationProgress;

crate::action_command! {
    /// Resume a paused migration (clears the pause flag; the migration loop
    /// re-checks and continues). Returns the progress snapshot at resume.
    /// Operator-only, so gated `Internal`. Errors if no migration is active.
    pub struct MigrationResume { state: Arc<DataState> }
    name: "migration/resume",
    access: Internal,
    params: MigrationControlParams,
    output: MigrationProgress,
    run(this, _ctx, _p) => {
        let result = this.state.migration_resume().await?;
        Ok(result)
    }
}
