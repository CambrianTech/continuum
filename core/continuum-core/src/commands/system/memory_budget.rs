//! `system/memory-budget` — per-consumer allocation vs actual usage. A non-secret
//! read → `AiSafe`. Fails loud if the pressure monitor was never wired.

use std::sync::Arc;

use crate::modules::system_resources::SystemResourceService;
use crate::sdk_codegen::CommandError;

use super::SystemQuery;

crate::action_command! {
    /// Budget snapshot — per-consumer priority, budget, usage, headroom, and warnings.
    /// Errors if the pressure monitor is not yet initialized.
    pub struct SystemMemoryBudget { service: Arc<SystemResourceService> }
    name: "system/memory-budget",
    access: AiSafe,
    params: SystemQuery,
    output: serde_json::Value,
    run(this, _ctx, _p) => {
        this.service.memory_budget().map_err(CommandError::Internal)
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
        assert_eq!(SystemMemoryBudget::NAME, "system/memory-budget");
        assert!(matches!(
            SystemMemoryBudget::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }

    // what this catches: with no pressure monitor wired the budget read fails loud
    // rather than fabricating an empty budget.
    #[tokio::test]
    async fn unwired_monitor_fails_loud() {
        let cmd = SystemMemoryBudget {
            service: Arc::new(SystemResourceService::new(Arc::new(
                SystemResourceMonitor::new(),
            ))),
        };
        assert!(cmd.run(&Ctx::default(), SystemQuery {}).await.is_err());
    }
}
