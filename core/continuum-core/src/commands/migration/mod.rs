//! `migration/<verb>` — the adapter-migration control surface of the data layer as
//! typed [`ActionCommand`](crate::sdk_codegen::ActionCommand)s, one per file.
//!
//! These once lived ONLY in [`DataModule::handle_command`](crate::modules::data)'s
//! stringly `match` (the `migration/` prefix arms) — dispatchable, but with no
//! descriptor in the registry, so invisible to the grid ACL, codegen, and `uu`. As
//! typed commands they get a descriptor AND route through the O(1) lock-free typed
//! object map. The wire name mirrors the file path — `commands/migration/start.rs`
//! ⟺ `migration/start`.
//!
//! ## Access
//!
//! Migrating storage backends is a system-operator concern, never a persona one:
//! every command here declares [`AccessLevel::Internal`](crate::sdk_codegen::AccessLevel),
//! which the ACL maps to `Owner` (default-deny for personas and remote peers). The
//! migration compute stays on [`DataState`](crate::modules::data::DataState); each
//! command holds the same `Arc<DataState>` and drives it.

use std::sync::Arc;

use crate::modules::data::DataState;
use crate::sdk_codegen::DynCommand;

pub mod cutover;
pub mod pause;
pub mod resume;
pub mod rollback;
pub mod start;
pub mod status;
pub mod verify;

use cutover::MigrationCutoverCmd;
use pause::MigrationPause;
use resume::MigrationResume;
use rollback::MigrationRollbackCmd;
use start::MigrationStart;
use status::MigrationStatusCmd;
use verify::MigrationVerifyCmd;

/// Shared params for the no-argument control commands (`status`/`pause`/`resume`/
/// `verify`): they all operate on the single active migration and take no input.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/migration/MigrationControlParams.ts"
)]
pub struct MigrationControlParams {}

/// The dep-holding `migration/*` command objects [`DataModule`](crate::modules::data::DataModule)
/// contributes to the kernel's typed object map, each sharing the module's
/// `Arc<DataState>`. The executor routes each name straight here; the legacy
/// `migration/` prefix arm is deleted.
pub fn command_objects(state: Arc<DataState>) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(MigrationStart {
            state: state.clone(),
        }),
        Arc::new(MigrationStatusCmd {
            state: state.clone(),
        }),
        Arc::new(MigrationPause {
            state: state.clone(),
        }),
        Arc::new(MigrationResume {
            state: state.clone(),
        }),
        Arc::new(MigrationVerifyCmd {
            state: state.clone(),
        }),
        Arc::new(MigrationCutoverCmd {
            state: state.clone(),
        }),
        Arc::new(MigrationRollbackCmd { state }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: each migration command carries its `migration/<verb>` wire
    // name — the routing key the grid ACL, cu, and every SDK bind to. The name mirrors
    // the file path; drift silently de-registers the command and an operator loses the
    // ability to drive a storage migration.
    #[test]
    fn migration_command_names_mirror_their_path() {
        assert_eq!(MigrationStart::NAME, "migration/start");
        assert_eq!(MigrationStatusCmd::NAME, "migration/status");
        assert_eq!(MigrationPause::NAME, "migration/pause");
        assert_eq!(MigrationResume::NAME, "migration/resume");
        assert_eq!(MigrationVerifyCmd::NAME, "migration/verify");
        assert_eq!(MigrationCutoverCmd::NAME, "migration/cutover");
        assert_eq!(MigrationRollbackCmd::NAME, "migration/rollback");
    }
}
