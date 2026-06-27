//! `system/cpu` — CPU stats (cores, usage, brand). A non-secret read → `AiSafe`.

use std::sync::Arc;

use crate::modules::system_resources::SystemResourceService;
use crate::sdk_codegen::CommandError;

use super::SystemQuery;

crate::action_command! {
    /// Report CPU stats from a fresh reading: physical/logical core counts, the CPU
    /// brand string, and global usage as a normalized 0..1 fraction.
    pub struct SystemCpu { service: Arc<SystemResourceService> }
    name: "system/cpu",
    access: AiSafe,
    params: SystemQuery,
    output: serde_json::Value,
    run(this, _ctx, _p) => {
        this.service.cpu().map_err(CommandError::Internal)
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
        assert_eq!(SystemCpu::NAME, "system/cpu");
        assert!(matches!(
            SystemCpu::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }

    // what this catches: the command body actually projects a cpu snapshot (core
    // counts present) — it wires to the live service, not a stub.
    #[tokio::test]
    async fn returns_cpu_snapshot() {
        let cmd = SystemCpu {
            service: Arc::new(SystemResourceService::new(Arc::new(
                SystemResourceMonitor::new(),
            ))),
        };
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let out = cmd.run(&Ctx::default(), SystemQuery {}).await.unwrap();
        assert!(out["physical_cores"].as_u64().unwrap() >= 1);
    }
}
