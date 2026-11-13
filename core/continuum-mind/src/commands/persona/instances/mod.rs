//! `persona/instances/<verb>` — the live-citizen lifecycle as typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s.
//!
//! ## The concern this owns
//!
//! "Who is online right now, and bring one offline cleanly." The roster lives in
//! the [`PersonaAircRuntimeRegistry`](crate::persona::PersonaAircRuntimeRegistry)
//! owned by [`PersonaInstanceManagerModule`](crate::persona::instance_manager).
//!
//! ## Allocation symmetry
//!
//! `persona/instances/bootstrap` allocates a citizen (mints/resumes her identity,
//! spawns her service loop, seats her in the room). [`PersonaDespawn`] is the
//! deallocation counterpart — anything spawned can be despawned at runtime, no
//! reboot, freeing the tokio task, airc subscription, and room seat while leaving
//! her durable on-disk self intact. [`PersonaInstancesList`] and
//! [`PersonaInstancesGet`] are the read verbs — who is online, and one entry by id.
//! `despawn --dormant` and `wake` change a mind's durable wakefulness (`persona::dormancy`).
//! (bootstrap still lives as a legacy `handle_command` arm on the module: it needs
//! the module's full bootstrap capability — daemon socket, default room, executor —
//! not just the registry, so it migrates under task #62 once those deps are
//! threaded. list/get/despawn live here on the typed path because they need only
//! the shared registry.)

use std::sync::Arc;

use crate::persona::dormancy::DormancyLedger;
use crate::persona::PersonaAircRuntimeRegistry;
use crate::runtime::LateBound;
use crate::sdk_codegen::DynCommand;

pub mod despawn;
pub mod get;
pub mod hold;
pub mod list;
pub mod wake;

use despawn::PersonaDespawn;
use get::PersonaInstancesGet;
use list::PersonaInstancesList;
use wake::PersonaWake;

/// The dep-holding `persona/instances/*` command objects the
/// [`PersonaInstanceManagerModule`](crate::persona::instance_manager::PersonaInstanceManagerModule)
/// contributes to the kernel's typed object map. They share the one
/// `PersonaAircRuntimeRegistry` so every caller acts on the SAME live roster, and the one
/// late-bound [`DormancyLedger`] so despawn's sleep and wake's wake write the same rows.
pub fn command_objects(
    registry: PersonaAircRuntimeRegistry,
    dormancy: Arc<LateBound<DormancyLedger>>,
) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(PersonaInstancesList {
            registry: registry.clone(),
        }),
        Arc::new(PersonaInstancesGet {
            registry: registry.clone(),
        }),
        Arc::new(PersonaDespawn { registry, dormancy: Arc::clone(&dormancy) }),
        Arc::new(PersonaWake { dormancy }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the dep-holding family wires the registry-backed read
    // verbs (list/get), despawn (the deallocation half of the spawn↔despawn
    // symmetry) and wake (the durable dormancy's way back). A regression that drops
    // any of them is caught.
    #[test]
    fn family_exposes_list_get_despawn_wake() {
        let registry = PersonaAircRuntimeRegistry::new();
        let objs = command_objects(registry, Arc::new(LateBound::new("test dormancy")));
        let names: Vec<&str> = objs.iter().map(|o| o.name()).collect();
        assert!(names.contains(&"persona/instances/list"));
        assert!(names.contains(&"persona/instances/get"));
        assert!(names.contains(&"persona/instances/despawn"));
        assert!(names.contains(&"persona/instances/wake"));
    }
}
