//! `commands/runtime/metrics/` — per-module timing introspection.
//!
//! `all` (every module's aggregate stats), `module` (one named module), and
//! `slow` (recent slow commands across all modules, descending). The runtime
//! tracks timing for every command automatically; these commands just query it.

pub mod all;
pub mod module;
pub mod slow;
