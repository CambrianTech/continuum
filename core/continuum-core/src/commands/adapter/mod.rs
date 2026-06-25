//! `adapter/<verb>` — storage-adapter introspection as typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s.
//!
//! These describe the storage backend behind a handle rather than its contents.
//! [`DataModule`](crate::modules::data::DataModule) owns the adapter pool, so it
//! contributes these objects (each sharing its `Arc<DataState>`) alongside the
//! `data/*` family.

use std::sync::Arc;

use crate::modules::data::DataState;
use crate::sdk_codegen::DynCommand;

pub mod info;

use info::AdapterInfoCommand;

/// The dep-holding `adapter/*` command objects, sharing `DataModule`'s
/// `Arc<DataState>`.
pub fn command_objects(state: Arc<DataState>) -> Vec<Arc<dyn DynCommand>> {
    vec![Arc::new(AdapterInfoCommand { state })]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: adapter/info carries its `adapter/info` wire name — the
    // routing key every caller binds to. The name mirrors the file path; drift
    // silently de-registers it from the typed registry.
    #[test]
    fn adapter_command_names_mirror_their_path() {
        assert_eq!(AdapterInfoCommand::NAME, "adapter/info");
    }
}
