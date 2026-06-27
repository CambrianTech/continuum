//! `system/memory` — memory stats (total, used, pressure, swap). A non-secret
//! read → `AiSafe`.

use std::sync::Arc;

use crate::modules::system_resources::SystemResourceService;
use crate::sdk_codegen::CommandError;
use crate::system_resources::MemoryStats;

use super::SystemQuery;

crate::action_command! {
    /// Report memory stats from a fresh reading: total/used bytes, swap, and memory
    /// pressure as a normalized 0..1 fraction.
    pub struct SystemMemory { service: Arc<SystemResourceService> }
    name: "system/memory",
    access: AiSafe,
    params: SystemQuery,
    output: MemoryStats,
    run(this, _ctx, _p) => {
        this.service.memory().map_err(CommandError::Internal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};
    use crate::system_resources::SystemResourceMonitor;

    // what this catches: name/access wiring — a system read is on the AiSafe surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(SystemMemory::NAME, "system/memory");
        assert!(matches!(
            SystemMemory::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }

    // what this catches: the body projects a memory snapshot with positive total bytes
    // — it wires to the live service.
    #[tokio::test]
    async fn returns_memory_snapshot() {
        let cmd = SystemMemory {
            service: Arc::new(SystemResourceService::new(Arc::new(
                SystemResourceMonitor::new(),
            ))),
        };
        let out = cmd.run(&Ctx::default(), SystemQuery {}).await.unwrap();
        assert!(out.total_bytes > 0);
    }
}
