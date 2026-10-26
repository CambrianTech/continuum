//! continuum-models: the `models/*` domain, cut out of continuum-core (commands by module).
//! `module` is the `ModelsModule` service module; `commands` are its typed `models/<verb>`
//! commands. Depends on the engine (model registry, adapters, serving) and the kernel only;
//! core re-exports both under their old `modules::models` / `commands::models` paths.

pub use continuum_engine::*;

pub mod commands;
pub mod module;
