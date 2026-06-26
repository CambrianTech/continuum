//! `persona/instances/<verb>` — the live-citizen lifecycle as typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s.
//!
//! ## The concern this owns
//!
//! "Who is online right now, and bring one offline cleanly." The roster lives in
//! the [`PersonaAircRuntimeRegistry`](crate::persona::PersonaAircRuntimeRegistry)
//! owned by [`PersonaInstanceManagerModule`](crate::modules::persona_instance_manager).
//!
//! ## Allocation symmetry
//!
//! `persona/instances/bootstrap` allocates a citizen (mints/resumes her identity,
//! spawns her service loop, seats her in the room). [`PersonaDespawn`] is the
//! deallocation counterpart — anything spawned can be despawned at runtime, no
//! reboot, freeing the tokio task, airc subscription, and room seat while leaving
//! her durable on-disk self intact. (bootstrap/list/get still live as legacy
//! `handle_command` arms on the module; they migrate onto this typed path under
//! task #62 — despawn is born here on the good path because it is new.)

use std::sync::Arc;

use crate::persona::PersonaAircRuntimeRegistry;
use crate::sdk_codegen::DynCommand;

pub mod despawn;

use despawn::PersonaDespawn;

/// The dep-holding `persona/instances/*` command objects the
/// [`PersonaInstanceManagerModule`](crate::modules::persona_instance_manager::PersonaInstanceManagerModule)
/// contributes to the kernel's typed object map. They share the one
/// `PersonaAircRuntimeRegistry` so every caller acts on the SAME live roster.
pub fn command_objects(registry: PersonaAircRuntimeRegistry) -> Vec<Arc<dyn DynCommand>> {
    vec![Arc::new(PersonaDespawn { registry })]
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the dep-holding family wires the registry-backed despawn
    // command (the deallocation half of the spawn↔despawn symmetry). A regression
    // that drops it — or fails to share the live registry — is caught.
    #[test]
    fn family_exposes_despawn() {
        let registry = PersonaAircRuntimeRegistry::new();
        let objs = command_objects(registry);
        let names: Vec<&str> = objs.iter().map(|o| o.name()).collect();
        assert!(names.contains(&"persona/instances/despawn"));
    }
}
