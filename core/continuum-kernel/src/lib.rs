//! The continuum microkernel: what every node links, and what depends on nothing above it.
//!
//! Identity and paths, paging and pressure, logging and probes, the ORM, system resources,
//! the runtime (ServiceModule, the bus, dispatch and its extension hooks), the governor,
//! resource pools, GPU accounting, routing and the command contract. Every layer above
//! extends it through the contracts it owns, installed at boot; see
//! `docs/architecture/MICROKERNEL-DECOMPOSITION.md`.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod concurrency;
pub mod docker_tier;
pub mod docker_tier_pool;
pub mod entity_schemas;
pub mod governor;
pub mod gpu;
pub mod identity;
pub mod logging;
pub mod orm;
pub mod paging;
// `paths::continuum_root` expects the continuum home until card 51ef6b00 (resolve the home once at
// boot and pass it to its 30 callers in 9 crates) lands; then delete this allow. expect_used only:
// unwrap and panic stay denied inside paths/.
#[allow(clippy::expect_used)]
pub mod paths;
pub mod probe_query;
pub mod probe_stream;
pub mod resource_broker;
pub mod resources;
pub mod routing;
pub mod runtime;
pub mod sdk_codegen;
pub mod system_resources;

pub use concurrency::*;
