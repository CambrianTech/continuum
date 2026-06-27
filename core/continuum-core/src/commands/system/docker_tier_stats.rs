//! `system/docker-tier-stats` — Docker storage-tier capacity/usage probe. A
//! non-secret read → `AiSafe`.

use std::sync::Arc;

use crate::modules::system_resources::SystemResourceService;
use crate::sdk_codegen::CommandError;

use super::SystemQuery;

crate::action_command! {
    /// Docker storage-tier stats from one probe: `capacityBytes`, `usedBytes`,
    /// `pressure`, `detected`. Always returns the full shape even when Docker is absent
    /// (`detected: false` + zeros), so callers can structurally pattern-match it.
    pub struct SystemDockerTierStats { service: Arc<SystemResourceService> }
    name: "system/docker-tier-stats",
    access: AiSafe,
    params: SystemQuery,
    output: serde_json::Value,
    run(this, _ctx, _p) => {
        this.service.docker_tier_stats().map_err(CommandError::Internal)
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
        assert_eq!(SystemDockerTierStats::NAME, "system/docker-tier-stats");
        assert!(matches!(
            SystemDockerTierStats::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }

    // what this catches: the command always returns the full four-field shape so callers
    // can match it even on a host without Docker (CI: detected=false, zeros).
    #[tokio::test]
    async fn always_returns_full_shape() {
        let cmd = SystemDockerTierStats {
            service: Arc::new(SystemResourceService::new(Arc::new(
                SystemResourceMonitor::new(),
            ))),
        };
        let out = cmd.run(&Ctx::default(), SystemQuery {}).await.unwrap();
        assert!(out["capacityBytes"].is_number());
        assert!(out["usedBytes"].is_number());
        assert!(out["pressure"].is_number());
        assert!(out["detected"].is_boolean());
    }
}
