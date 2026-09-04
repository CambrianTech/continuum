//! `agent/wait` — block until an agent finishes (or a timeout elapses), returning
//! its final status. Blocking read → `AiSafe`.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::agent::{AgentService, AgentStatusInfo};
use crate::sdk_codegen::CommandError;

fn default_timeout_ms() -> u64 {
    300_000
}

/// Inputs to `agent/wait`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/agent/AgentWaitParams.ts"
)]
pub struct AgentWaitParams {
    /// The agent handle returned by `agent/start`.
    pub handle: String,
    /// Max milliseconds to wait before returning a timeout error (default 300000).
    #[serde(default = "default_timeout_ms")]
    #[ts(type = "number")]
    pub timeout_ms: u64,
}

crate::action_command! {
    /// Block until the agent with this handle finishes, then return its final status
    /// (summary or error). Errors on timeout, or if no agent has that handle. Use
    /// after `agent/start` to wait for a synchronous result.
    pub struct AgentWait { service: Arc<AgentService> }
    name: "agent/wait",
    access: AiSafe,
    params: AgentWaitParams,
    output: AgentStatusInfo,
    run(this, _ctx, p) => {
        match this.service.wait_for(&p.handle, p.timeout_ms).await {
            Ok(Some(info)) => Ok(info),
            // Unknown handle (or evicted before we could read the final state).
            Ok(None) => Err(CommandError::NotFound(format!(
                "no agent with handle '{}'",
                p.handle
            ))),
            // Timed out waiting — a runtime condition, not bad input.
            Err(e) => Err(CommandError::Internal(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: name/access wiring — waiting is a (blocking) read on AiSafe.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(AgentWait::NAME, "agent/wait");
        assert!(matches!(
            AgentWait::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }

    // what this catches: the optional `timeout_ms` defaults to 300000 when omitted —
    // the legacy `u64_or("timeout_ms", 300000)` contract, preserved via serde default.
    #[test]
    fn timeout_defaults_to_300s() {
        let p: AgentWaitParams = serde_json::from_value(serde_json::json!({
            "handle": "agent-abc"
        }))
        .unwrap();
        assert_eq!(p.timeout_ms, 300_000);
    }

    // what this catches: waiting on an unknown handle is a NotFound error (not a hang
    // and not a silent success) — the typed mapping of the service's `Ok(None)`.
    #[tokio::test]
    async fn unknown_handle_is_not_found() {
        let rt = tokio::runtime::Handle::current();
        let cmd = AgentWait {
            service: Arc::new(AgentService::new(rt)),
        };
        let err = cmd
            .run(
                &Ctx::default(),
                AgentWaitParams {
                    handle: "agent-doesnotexist".into(),
                    timeout_ms: 1000,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, CommandError::NotFound(_)));
    }
}
