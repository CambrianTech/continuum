//! `persona/*` turn-frame commands (Lane D) — the persona-loop substrate verbs migrated
//! off the legacy `CognitionModule::handle_command` match onto the typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand) path.
//!
//! These carry the `persona/` wire prefix (they act on a persona's inbox + turn frame)
//! but are owned by [`CognitionModule`](crate::modules::cognition::CognitionModule),
//! which holds the per-persona [`CognitionState`]. Following the
//! [`rag_inspect`](super::rag_inspect) precedent, they live under `commands/persona/`
//! (path mirrors the wire name) yet are contributed by `CognitionModule::commands`, NOT
//! by the shared [`command_objects`](super::command_objects) that
//! `PersonaInstanceManagerModule` assembles.

use std::sync::Arc;

use crate::modules::cognition::CognitionState;
use crate::sdk_codegen::DynCommand;

pub mod drain;
pub mod execute;

use drain::DrainTurnFrame;
use execute::TurnExecute;

/// The dep-holding Lane D turn-frame command objects over the module's shared
/// [`CognitionState`]. Called from
/// [`CognitionModule::commands`](crate::modules::cognition::CognitionModule) so they
/// reach `command_registry()`, the ACL, codegen, and `uu`.
///
/// - [`DrainTurnFrame`](drain::DrainTurnFrame) (`persona/drain-turn-frame`) stops at the
///   replay-stable turn frame.
/// - [`TurnExecute`](execute::TurnExecute) (`persona/turn-execute`) carries that frame
///   through Rust inference in one hop.
pub fn command_objects(state: Arc<CognitionState>) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(DrainTurnFrame {
            state: state.clone(),
        }),
        Arc::new(TurnExecute { state }),
    ]
}
