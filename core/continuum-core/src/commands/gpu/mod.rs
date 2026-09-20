//! `gpu/<verb>` — GPU memory authority as typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s, one verb per file,
//! sharing the one [`GpuMemoryManager`](crate::gpu::GpuMemoryManager).
//!
//! These were `GpuModule::handle_command` match arms (Registry A); migrating them
//! onto the typed registry makes them visible to the persona tool surface, the ACL,
//! codegen, and `uu` by construction.
//!
//! ## Access levels reflect the resource-authority boundary
//!
//! VRAM is ONE per-machine authority (continuum task #56): consumers LEASE, they
//! don't own. So the split is deliberate:
//!   - **reads are `AiSafe`** — `gpu/stats`, `gpu/pressure`, the two eviction
//!     queries. A mind reasoning about its own resource situation is harmless.
//!   - **`gpu/set-budget` is `Privileged`** — it mutates the authority itself, an
//!     operator-level act, never an arbitrary persona's.
//!   - **`gpu/register-consumer` / `gpu/unregister-consumer` are `Internal`** —
//!     substrate plumbing: a compute consumer (e.g. a training run) announcing its
//!     footprint to the authority, invoked in-process, not a remote/persona tool.
//!
//! The manager is owned by [`GpuModule`](crate::modules::gpu::GpuModule), which
//! contributes these command objects through its `commands()`.

use std::sync::Arc;

use crate::gpu::GpuMemoryManager;
use crate::sdk_codegen::DynCommand;

pub mod budget;
pub mod consumer;
pub mod eviction_candidates;
pub mod eviction_registry;
pub mod pressure;
pub mod stats;

/// The dep-holding GPU command objects [`GpuModule`](crate::modules::gpu::GpuModule)
/// contributes to the kernel's typed object map, one per verb, sharing the one
/// `Arc<GpuMemoryManager>`.
pub fn command_objects(manager: Arc<GpuMemoryManager>) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(stats::GpuStatsCmd {
            manager: manager.clone(),
        }),
        Arc::new(pressure::GpuPressureCmd {
            manager: manager.clone(),
        }),
        Arc::new(eviction_registry::GpuEvictionRegistry {
            manager: manager.clone(),
        }),
        Arc::new(eviction_candidates::GpuEvictionCandidates {
            manager: manager.clone(),
        }),
        Arc::new(budget::GpuSetBudget {
            manager: manager.clone(),
        }),
        Arc::new(consumer::GpuRegisterConsumer {
            manager: manager.clone(),
        }),
        Arc::new(consumer::GpuUnregisterConsumer { manager }),
    ]
}
