//! Recipe execution runtime — the pipeline executor from
//! docs/architecture/RECIPE-EXECUTION-RUNTIME.md, built at last.
//!
//! **Recipes are data. Commands are kernel-level capabilities.** A recipe is a
//! row: a `pipeline[]` of command invocations with `$var` interpolation,
//! output binding, and skip-conditions. Adding a new recipe — or a new recipe
//! CONCEPT — is authoring data, never committing Rust. The extension surface
//! is the command system itself: every discoverable command is a legal step,
//! so the recipe layer inherits the whole modular command architecture instead
//! of re-encoding fragments of it as serde fields (the static-plumbing trap
//! this module replaces — `reviewers: N` on the benchmark recipe format needed
//! a recompile; a pipeline step calling `persona/roster` does not).
//!
//! Layout (one concern per file):
//! - [`types`] — `Recipe` + `RecipeStep`, serde-tolerant wire shapes
//! - [`state`] — `ExecutionState`, the append-only binding map steps read/write
//! - [`interpolate`] — pure `$var` / `${var.path}` substitution over params
//! - [`condition`] — the minimal skip-condition evaluator
//! - [`executor`] — `PipelineExecutor`, the kernel loop that walks steps

pub mod condition;
pub mod executor;
pub mod interpolate;
pub mod state;
pub mod types;

pub use executor::PipelineExecutor;
pub use state::ExecutionState;
pub use types::{Recipe, RecipeStep};
