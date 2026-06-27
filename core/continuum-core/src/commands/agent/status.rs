//! `agent/status` — progress snapshot for one agent by handle. Returns `null` when
//! the handle is unknown (or already evicted post-completion). Read → `AiSafe`.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::agent::{AgentService, AgentStatusInfo};

/// Inputs to `agent/status`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/agent/AgentStatusParams.ts")]
pub struct AgentStatusParams {
    /// The agent handle returned by `agent/start`.
    pub handle: String,
}

crate::action_command! {
    /// Get an agent's progress: status, iteration, files created/modified, and the
    /// final summary or error. Returns null when no agent has that handle (unknown,
    /// or already finished and evicted).
    pub struct AgentGetStatus { service: Arc<AgentService> }
    name: "agent/status",
    access: AiSafe,
    params: AgentStatusParams,
    output: Option<AgentStatusInfo>,
    run(this, _ctx, p) => {
        Ok(this.service.status_of(&p.handle))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: name/access wiring — status is a read on the AiSafe surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(AgentGetStatus::NAME, "agent/status");
        assert!(matches!(
            AgentGetStatus::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }

    // what this catches: an unknown handle yields None (→ JSON null), preserving the
    // legacy "agent not found, no hard error" contract instead of erroring.
    #[tokio::test]
    async fn unknown_handle_is_none() {
        let rt = tokio::runtime::Handle::current();
        let cmd = AgentGetStatus {
            service: Arc::new(AgentService::new(rt)),
        };
        let out = cmd
            .run(
                &Ctx::default(),
                AgentStatusParams {
                    handle: "agent-doesnotexist".into(),
                },
            )
            .await
            .unwrap();
        assert!(out.is_none());
    }
}
