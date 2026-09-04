//! `agent/status` — progress snapshot for one agent by handle. Returns `null` when
//! the handle is unknown (or already evicted post-completion). Read → `AiSafe`.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::agent::{AgentService, AgentStatusInfo};

/// Inputs to `agent/status`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/agent/AgentStatusParams.ts"
)]
pub struct AgentStatusParams {
    /// The agent handle returned by `agent/start`.
    pub handle: String,
}

/// Result of `agent/status` — the agent's progress snapshot, or absent when no
/// agent has that handle (unknown, or already finished and evicted). A named
/// wrapper so the wire type is a struct, not a bare `T | null`.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/agent/AgentStatusLookup.ts"
)]
pub struct AgentStatusLookup {
    /// The progress snapshot, or absent when the handle is unknown/evicted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub status: Option<AgentStatusInfo>,
}

crate::action_command! {
    /// Get an agent's progress: status, iteration, files created/modified, and the
    /// final summary or error. The `status` field is absent when no agent has that
    /// handle (unknown, or already finished and evicted).
    pub struct AgentGetStatus { service: Arc<AgentService> }
    name: "agent/status",
    access: AiSafe,
    params: AgentStatusParams,
    output: AgentStatusLookup,
    run(this, _ctx, p) => {
        Ok(AgentStatusLookup {
            status: this.service.status_of(&p.handle),
        })
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
        assert!(out.status.is_none());
    }
}
