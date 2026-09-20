//! `system/launch-mode/<verb>` — the runtime launch-preference lever as typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s, one command per file
//! (mirror of the `code/git` family shape).
//!
//! `get` reads config.env and holds no deps, so it is a stateless command that
//! self-registers. `set` must emit the `system:launch-mode:changed` event, so it
//! captures the module's message bus via [`LaunchModeState`] (harvested at
//! `register`, the bus filled at `initialize` — the deferred-bus pattern
//! [`DataState`](crate::modules::data) uses). See [`crate::modules::launch_mode`].

use std::sync::Arc;

use crate::modules::launch_mode::LaunchModeState;
use crate::sdk_codegen::DynCommand;

pub mod get;
pub mod set;

/// The dep-holding launch-mode command objects. Only `set` needs deps (the bus);
/// `get` is stateless and self-registers, so it is NOT listed here. Assembled by
/// [`LaunchModeModule::commands`](crate::modules::launch_mode::LaunchModeModule).
pub fn command_objects(state: Arc<LaunchModeState>) -> Vec<Arc<dyn DynCommand>> {
    vec![Arc::new(set::SystemLaunchModeSet { state })]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: the two commands carry their namespaced `system/launch-mode/<verb>`
    // wire names — the routing keys every caller binds to. The name mirrors the file
    // path (underscore ⟺ hyphen); drift silently breaks "the tree IS the namespace".
    #[test]
    fn launch_mode_command_names_mirror_their_path() {
        assert_eq!(get::SystemLaunchModeGet::NAME, "system/launch-mode/get");
        assert_eq!(set::SystemLaunchModeSet::NAME, "system/launch-mode/set");
    }

    // what this catches: `set` is Privileged (it changes how the host launches), `get`
    // is AiSafe (read-only). A regression that widened `set` to AiSafe would let an
    // autonomous caller flip the launch mode — caught here.
    #[test]
    fn access_levels_are_deliberate() {
        use crate::sdk_codegen::AccessLevel;
        assert_eq!(get::SystemLaunchModeGet::ACCESS, AccessLevel::AiSafe);
        assert_eq!(set::SystemLaunchModeSet::ACCESS, AccessLevel::Privileged);
    }
}
