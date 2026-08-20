//! `agent/stop` — request a running agent halt at its next iteration boundary.
//! Controlling background compute is an authority op → `Privileged`.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::agent::AgentService;

/// Inputs to `agent/stop`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/agent/AgentStopParams.ts"
)]
pub struct AgentStopParams {
    /// The agent handle returned by `agent/start`.
    pub handle: String,
}

/// Result of `agent/stop`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/agent/AgentStopResult.ts"
)]
pub struct AgentStopResult {
    /// `true` if the handle was found and flagged to stop; `false` if no such agent.
    pub stop_requested: bool,
}

crate::action_command! {
    /// Request a running agent stop. The agent checks the flag at its next iteration
    /// boundary and winds down to a `stopped` state. Returns whether the handle was
    /// found (`false` = no such agent).
    pub struct AgentStop { service: Arc<AgentService> }
    name: "agent/stop",
    access: Privileged,
    params: AgentStopParams,
    output: AgentStopResult,
    run(this, _ctx, p) => {
        Ok(AgentStopResult { stop_requested: this.service.request_stop(&p.handle) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: name/access wiring — controlling background compute is a
    // Privileged authority op, not the AiSafe read surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(AgentStop::NAME, "agent/stop");
        assert!(matches!(
            AgentStop::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }

    // what this catches: stopping an unknown handle reports `stop_requested: false`
    // (soft miss), preserving the legacy "agent not found, no panic" contract.
    #[tokio::test]
    async fn unknown_handle_reports_not_requested() {
        let rt = tokio::runtime::Handle::current();
        let cmd = AgentStop {
            service: Arc::new(AgentService::new(rt)),
        };
        let out = cmd
            .run(
                &Ctx::default(),
                AgentStopParams {
                    handle: "agent-doesnotexist".into(),
                },
            )
            .await
            .unwrap();
        assert!(!out.stop_requested);
    }
}
