//! continuum-serving: the serving planner and lane lifecycle, cut out of continuum-core (plan
//! phase 2, `docs/architecture/MODULE-ARCHITECTURE.md` §9.2: Serving is its own module).
//! `serving_daemon` is the `ServingDaemonModule` (planner, reconciler, publication, pins) and its
//! `serving/<verb>` commands; `serving_consumer` is its memory-authority consumer. Core
//! re-exports both under their old `modules::` / `commands::serving` paths.
//!
//! The lower crates are re-exported under the names the moved files already use
//! (`crate::inference`, `crate::persona`, ...), so a move needs import paths only.

pub use continuum_engine::{
    ai, capacity, inference, model_registry,
};
pub use continuum_kernel::{
    action_command, gpu, identity, paths, resources, runtime, sdk_codegen, system_resources,
};
pub use continuum_mind::{cognition, genome, persona};

pub mod modules;
