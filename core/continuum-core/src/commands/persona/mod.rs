//! `persona/<subcat>/<verb>` — the persona lifecycle command tree.
//!
//! Today this holds `persona/instances/*` (the live-citizen roster lifecycle).
//! The family `command_objects` is re-exported so the owning module wires it in
//! one call.

pub mod instances;

pub use instances::command_objects;
