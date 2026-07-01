//! `cognition/<verb>` — the cognition command family (typed, self-routing).
//!
//! These are the persona cognitive-pipeline commands migrated off the legacy
//! `CognitionModule::handle_command` match onto the typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand) path. The gating/generation
//! decisions here call free functions in [`crate::cognition`] (no `CognitionModule`
//! state), so each is a **stateless** `action_command!` unit struct: `inventory`
//! publishes both the descriptor and the runtime object, no module `commands()`
//! ceremony. Stateful cognition commands (per-persona genome, inbox, engram store)
//! migrate as dep-holding families in later slices.
//!
//! Everything here is `access: Internal` — substrate cognition IPC the host drives,
//! registered and grid-routable but not remote-callable persona toolbelt verbs.

use std::sync::Arc;

use crate::modules::cognition::CognitionState;
use crate::sdk_codegen::DynCommand;

pub mod cache_message;
pub mod check_content_dedup;
pub mod check_redundancy;
pub mod record_content;
pub mod should_respond;

use cache_message::CacheMessage;
use check_content_dedup::CheckContentDedup;
use record_content::RecordContent;

/// The dep-holding `cognition/*` command objects that capture the module's shared
/// [`CognitionState`]. Called from
/// [`CognitionModule::commands`](crate::modules::cognition::CognitionModule) so they
/// reach `command_registry()`, the persona tool surface, the ACL, codegen, and `cu`.
///
/// The stateless gating commands ([`should_respond`], [`check_redundancy`]) self-route
/// via `inventory` and are NOT listed here.
pub fn command_objects(state: Arc<CognitionState>) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(CacheMessage {
            state: state.clone(),
        }),
        Arc::new(CheckContentDedup {
            state: state.clone(),
        }),
        Arc::new(RecordContent { state }),
    ]
}
