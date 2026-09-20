//! `system/memory-gate` — whether the global memory gate is closed (critical pressure
//! sustained), plus current pressure / RSS. A non-secret read → `AiSafe`.

use std::sync::Arc;

use crate::modules::system_resources::{MemoryGateState, SystemResourceService};

use super::SystemQuery;

crate::action_command! {
    /// Memory-gate state for callers to check before expensive operations: `closed`
    /// (critical pressure sustained), `pressure`, and `rss_bytes`. Always succeeds — an
    /// unwired pressure monitor reports `0.0` / `0` because the gate is global state.
    pub struct SystemMemoryGate { service: Arc<SystemResourceService> }
    name: "system/memory-gate",
    access: AiSafe,
    params: SystemQuery,
    output: MemoryGateState,
    run(this, _ctx, _p) => {
        Ok(this.service.memory_gate())
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
        assert_eq!(SystemMemoryGate::NAME, "system/memory-gate");
        assert!(matches!(
            SystemMemoryGate::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }

    // what this catches: the gate read always returns the full shape (closed/pressure/
    // rss_bytes) even with no pressure monitor wired — it never errors.
    #[tokio::test]
    async fn returns_full_shape_unwired() {
        let cmd = SystemMemoryGate {
            service: Arc::new(SystemResourceService::new(Arc::new(
                SystemResourceMonitor::new(),
            ))),
        };
        let out = cmd.run(&Ctx::default(), SystemQuery {}).await.unwrap();
        // closed is a bool by type; unwired monitor reports zeros.
        assert_eq!(out.pressure, 0.0);
        assert_eq!(out.rss_bytes, 0);
    }
}
