//! `agent/list` — snapshot every live agent (running and just-completed-but-not-yet-
//! evicted). Read → `AiSafe`.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::agent::{AgentService, AgentStatusInfo};

/// `agent/list` takes no input.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/agent/AgentListParams.ts"
)]
pub struct AgentListParams {}

/// Result of `agent/list` — the live set of tracked agents. A named wrapper so the
/// wire type is a struct, not a bare `Array<T>`.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/agent/AgentStatusList.ts"
)]
pub struct AgentStatusList {
    /// Every agent the runtime is currently tracking (running + not-yet-evicted).
    pub agents: Vec<AgentStatusInfo>,
}

crate::action_command! {
    /// List every agent the runtime is tracking — each with its status, iteration,
    /// and file changes. Completed agents are evicted to free memory, so this shows
    /// the live set, not full history.
    pub struct AgentList { service: Arc<AgentService> }
    name: "agent/list",
    access: AiSafe,
    params: AgentListParams,
    output: AgentStatusList,
    run(this, _ctx, _p) => {
        Ok(AgentStatusList {
            agents: this.service.list(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: name/access wiring — listing agents is a read on AiSafe.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(AgentList::NAME, "agent/list");
        assert!(matches!(
            AgentList::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }

    // what this catches: a fresh service tracks zero agents — the empty case returns
    // an empty Vec (not an error), so callers can always `.len()`.
    #[tokio::test]
    async fn empty_service_lists_nothing() {
        let rt = tokio::runtime::Handle::current();
        let cmd = AgentList {
            service: Arc::new(AgentService::new(rt)),
        };
        let out = cmd.run(&Ctx::default(), AgentListParams {}).await.unwrap();
        assert!(out.agents.is_empty());
    }
}
