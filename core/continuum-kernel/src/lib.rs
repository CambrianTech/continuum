//! The continuum microkernel: what every node links, and what depends on nothing above it.
//!
//! Identity and paths, paging and pressure, logging and probes, the ORM, system resources,
//! the runtime (ServiceModule, the bus, dispatch and its extension hooks), the governor,
//! resource pools, GPU accounting, routing and the command contract. Every layer above
//! extends it through the contracts it owns, installed at boot; see
//! `docs/architecture/MICROKERNEL-DECOMPOSITION.md`.

pub mod concurrency;
pub mod governor;
pub mod gpu;
pub mod identity;
pub mod logging;
pub mod orm;
pub mod paging;
pub mod paths;
pub mod resources;
pub mod routing;
pub mod runtime;
pub mod sdk_codegen;
pub mod system_resources;

pub use concurrency::*;
