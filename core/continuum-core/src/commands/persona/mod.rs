//! `persona/<verb>` and `persona/<subcat>/<verb>` — the persona command tree.
//!
//! - `persona/allocate` (dep-holding, GPU manager) + `persona/catalog` (stateless)
//!   — "which personas should exist on this machine" + the raw catalog.
//! - `persona/instances/*` (the live-citizen roster lifecycle); its family
//!   `command_objects` is re-exported so the owning module wires it in one call.

pub mod allocate;
pub mod catalog;
pub mod instances;

pub use instances::command_objects;
