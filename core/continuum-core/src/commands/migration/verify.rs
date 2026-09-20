//! `migration/verify` — compare source/target record counts post-migration.

use std::sync::Arc;

use super::MigrationControlParams;
use crate::modules::data::DataState;
use crate::orm::migration::MigrationVerification;

crate::action_command! {
    /// Verify migration integrity by comparing record counts between source and
    /// target for every migrated collection. Returns whether all match plus a
    /// per-collection breakdown. Operator-only, so gated `Internal`. Errors if no
    /// migration is active.
    pub struct MigrationVerifyCmd { state: Arc<DataState> }
    name: "migration/verify",
    access: Internal,
    params: MigrationControlParams,
    output: MigrationVerification,
    run(this, _ctx, _p) => {
        let result = this.state.migration_verify().await?;
        Ok(result)
    }
}
