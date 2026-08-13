//! `search/*` — text and vector ranking, as stateless typed commands on the one
//! `DynCommand` registry.
//!
//! Migrated out of the old `modules/search.rs` `ServiceModule` (a `handle_command`
//! `match` block invisible to `command_registry()`, the persona tool surface, and
//! the ACL). The four verbs now self-register via `inventory` — every one is a
//! persona tool, a `uu` CLI verb, a grid-callable contract, and a widget control
//! by construction. The OpenCV-style algorithm registry (bow / bm25 / cosine)
//! lives in [`engine`], co-located with the verbs that run it.

pub mod engine;
pub mod execute;
pub mod list;
pub mod params;
pub mod vector;
