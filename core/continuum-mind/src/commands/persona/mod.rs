//! `persona/<verb>` and `persona/<subcat>/<verb>` — the persona command tree's movable part (cut out of
//! continuum-core, plan phase 2). Core's `commands/persona/mod.rs` keeps `command_objects`, `vitals` and
//! `wall`, which still need core's IPC vitals sampler and `modules::work`, and re-exports these.

pub mod allocate;
pub mod catalog;
pub mod identity;
pub mod instances;
pub mod live_state;
pub mod placement_reserve;
pub mod rag_inspect;
pub mod reassign_model;
pub mod spawn;
pub mod turn_frame;
