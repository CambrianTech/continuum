//! `health/` — the substrate liveness command family.
//!
//! Two verbs, migrated off [`HealthModule`](crate::modules::health)'s legacy
//! `handle_command` arms (#62):
//!
//! - [`check`] — `health-check`, dep-holding (captures the module's boot `Instant`
//!   for uptime), contributed to the registry via the module's `commands()`.
//! - [`stats`] — `get-stats`, stateless: it self-registers via the unit-struct
//!   `action_command!` form, so it is NOT in [`command_objects`] — declaring
//!   `pub mod stats;` here is what links its inventory submission.
//!
//! (`ping` — the third liveness verb — is a stateless command that already lives in
//! the module file; it is untouched by this migration.)

pub mod check;
pub mod stats;

use std::sync::Arc;
use std::time::Instant;

use crate::sdk_codegen::DynCommand;

/// Build the dep-holding `health/*` runtime command objects over the module's live
/// boot instant. Returns only `health-check` (the stateful verb); `get-stats`
/// self-registers statelessly.
pub fn command_objects(started_at: Instant) -> Vec<Arc<dyn DynCommand>> {
    vec![Arc::new(check::HealthCheck { started_at })]
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the family exposes the dep-holding liveness verb. A
    // regression that drops health-check from command_objects (leaving the persona
    // surface without the uptime probe) is caught here. `get-stats` is asserted
    // separately in its own file (it's stateless, not in this Vec).
    #[test]
    fn family_exposes_the_health_check_verb() {
        let objs = command_objects(Instant::now());
        let names: Vec<&str> = objs.iter().map(|c| c.name()).collect();
        assert!(names.contains(&"health-check"), "got {names:?}");
    }
}
