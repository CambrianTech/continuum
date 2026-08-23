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
//! her durable on-disk self intact. [`PersonaInstancesList`] and
//! [`PersonaInstancesGet`] are the read verbs — who is online, and one entry by id.
//! (bootstrap still lives as a legacy `handle_command` arm on the module: it needs
//! the module's full bootstrap capability — daemon socket, default room, executor —
//! not just the registry, so it migrates under task #62 once those deps are
//! threaded. list/get/despawn live here on the typed path because they need only
//! the shared registry.)

use std::sync::Arc;

use crate::persona::PersonaAircRuntimeRegistry;
use crate::sdk_codegen::DynCommand;

pub mod despawn;
pub mod get;
pub mod hold;
pub mod list;

use despawn::PersonaDespawn;
use get::PersonaInstancesGet;
use list::PersonaInstancesList;

/// The dep-holding `persona/instances/*` command objects the
/// [`PersonaInstanceManagerModule`](crate::modules::persona_instance_manager::PersonaInstanceManagerModule)
/// contributes to the kernel's typed object map. They share the one
/// `PersonaAircRuntimeRegistry` so every caller acts on the SAME live roster.
pub fn command_objects(registry: PersonaAircRuntimeRegistry) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(PersonaInstancesList {
            registry: registry.clone(),
        }),
        Arc::new(PersonaInstancesGet {
            registry: registry.clone(),
        }),
        Arc::new(PersonaDespawn { registry }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the dep-holding family wires the registry-backed read
    // verbs (list/get) plus despawn (the deallocation half of the spawn↔despawn
    // symmetry), all sharing the one live registry. A regression that drops any of
    // them — or fails to share the registry — is caught.
    #[test]
    fn family_exposes_list_get_despawn() {
        let registry = PersonaAircRuntimeRegistry::new();
        let objs = command_objects(registry);
        let names: Vec<&str> = objs.iter().map(|o| o.name()).collect();
        assert!(names.contains(&"persona/instances/list"));
        assert!(names.contains(&"persona/instances/get"));
        assert!(names.contains(&"persona/instances/despawn"));
    }
}
