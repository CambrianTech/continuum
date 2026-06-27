//! `system/pressure` — memory-pressure snapshot from the autonomous monitor. A
//! non-secret read → `AiSafe`. Fails loud if the pressure monitor was never wired.

use std::sync::Arc;

use crate::modules::system_resources::SystemResourceService;
use crate::sdk_codegen::CommandError;

use super::SystemQuery;

crate::action_command! {
    /// Current memory-pressure snapshot from the autonomous pressure monitor. Errors if
    /// the monitor is not yet initialized (rather than reporting a fake zero).
    pub struct SystemPressure { service: Arc<SystemResourceService> }
    name: "system/pressure",
    access: AiSafe,
    params: SystemQuery,
    output: serde_json::Value,
    run(this, _ctx, _p) => {
        this.service.pressure().map_err(CommandError::Internal)
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
        assert_eq!(SystemPressure::NAME, "system/pressure");
        assert!(matches!(
            SystemPressure::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }

    // what this catches: with no pressure monitor wired the command fails loud (not a
    // fabricated zero) — the no-fallback rule holds at the command boundary.
    #[tokio::test]
    async fn unwired_monitor_fails_loud() {
        let cmd = SystemPressure {
            service: Arc::new(SystemResourceService::new(Arc::new(
                SystemResourceMonitor::new(),
            ))),
        };
        assert!(cmd.run(&Ctx::default(), SystemQuery {}).await.is_err());
    }
}
