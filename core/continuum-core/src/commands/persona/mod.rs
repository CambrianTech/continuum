//! `persona/<verb>` and `persona/<subcat>/<verb>` — the persona command tree.
//!
//! - `persona/allocate` (dep-holding, GPU manager) + `persona/catalog` (stateless)
//!   — "which personas should exist on this machine" + the raw catalog.
//! - `persona/reassign-model` (dep-holding, continuum_root + executor) — durably
//!   assign a persona a new base model AND pin the host now (composes `serving/pin`).
//! - `persona/instances/*` (the live-citizen roster lifecycle).
//! - `persona/rag-inspect` (dep-holding, a `PersonaResolver`) — introspect what a
//!   persona's RAG pipeline would feed the model. It belongs to a SEPARATE module
//!   ([`PersonaRagInspectModule`](crate::modules::persona_rag_inspect)) which holds
//!   the resolver, so it is contributed by `rag_inspect::command_objects` from that
//!   module's `commands()`, NOT by the shared [`command_objects`] below.
//!
//! The dep-holding members are wired together by [`command_objects`], which the
//! owning [`PersonaInstanceManagerModule`](crate::modules::persona_instance_manager)
//! calls in one place — it holds the `continuum_root` + the late-bound executor that
//! `persona/reassign-model` needs, plus the registry the `instances/*` verbs share.

use std::path::PathBuf;
use std::sync::Arc;

use crate::modules::persona_instance_manager::PersonaBirth;
use crate::persona::PersonaAircRuntimeRegistry;
use crate::runtime::{CommandExecutor, LateBound};
use crate::sdk_codegen::DynCommand;

pub mod allocate;
pub mod catalog;
pub mod instances;
pub mod rag_inspect;
pub mod reassign_model;
pub mod spawn;
pub mod turn_frame;
pub mod wall;

use reassign_model::PersonaReassignModel;
use spawn::PersonaSpawn;

/// All dep-holding `persona/*` command objects the
/// [`PersonaInstanceManagerModule`](crate::modules::persona_instance_manager::PersonaInstanceManagerModule)
/// contributes to the kernel's typed object map: the `instances/*` roster verbs
/// (sharing the one live registry) plus `persona/reassign-model` (which resolves
/// persona homes under `continuum_root` and composes `serving/pin` through the
/// late-bound substrate `executor`).
pub fn command_objects(
    registry: PersonaAircRuntimeRegistry,
    continuum_root: PathBuf,
    executor: Arc<LateBound<CommandExecutor>>,
    birth: Arc<PersonaBirth>,
) -> Vec<Arc<dyn DynCommand>> {
    let mut objects = instances::command_objects(registry.clone());
    objects.extend(wall::command_objects(registry));
    objects.push(Arc::new(PersonaReassignModel {
        continuum_root,
        executor,
    }));
    // `persona/spawn` — on-demand birth over the SAME core as boot auto-seed.
    objects.push(Arc::new(PersonaSpawn { birth }));
    objects
}
