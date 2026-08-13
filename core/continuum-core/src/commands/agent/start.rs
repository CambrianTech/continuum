//! `agent/start` — launch an autonomous coding agent in the background. Returns a
//! handle immediately; poll `agent/status` / block on `agent/wait`. Spawns an agent
//! that runs arbitrary shell, writes files, and drives git in `working_dir` → an
//! authority mutation, gated `Privileged`.

use std::path::PathBuf;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::agent::AgentService;

fn default_max_iterations() -> u32 {
    50
}

/// Inputs to `agent/start`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/agent/AgentStartParams.ts"
)]
pub struct AgentStartParams {
    /// The task for the agent to accomplish (free-form natural language).
    pub task: String,
    /// Absolute path to the workspace the agent operates in.
    pub working_dir: String,
    /// Model name to drive the agent loop, e.g. `deepseek-chat`,
    /// `claude-sonnet-4-5-20250929`, `gpt-4`. Must match an available provider.
    pub model: String,
    /// Maximum build/test/fix iterations before the agent gives up (default 50).
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
}

/// Result of `agent/start`: the handle to poll.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/agent/AgentStartResult.ts"
)]
pub struct AgentStartResult {
    /// The handle identifying the spawned agent — pass to `status`/`stop`/`wait`.
    pub handle: String,
}

crate::action_command! {
    /// Start an autonomous coding agent on a task. The agent explores the workspace,
    /// edits files, runs builds/tests, and iterates until done — running real shell
    /// commands and git in the given working directory. Returns a handle immediately;
    /// use `agent/status` to poll progress or `agent/wait` to block for the result.
    pub struct AgentStart { service: Arc<AgentService> }
    name: "agent/start",
    access: Privileged,
    params: AgentStartParams,
    output: AgentStartResult,
    run(this, _ctx, p) => {
        let handle = this.service.spawn_agent(
            p.task.clone(),
            PathBuf::from(&p.working_dir),
            p.max_iterations,
            p.model.clone(),
        );
        crate::log_info!("module", "agent", "Started agent {} for task: {}", &handle, p.task);
        Ok(AgentStartResult { handle })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: name/access wiring — starting a shell-running agent is a
    // Privileged authority mutation, never offered on the AiSafe surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(AgentStart::NAME, "agent/start");
        assert!(matches!(
            AgentStart::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }

    // what this catches: the doc comment ⟹ DESCRIPTION wiring stays live, so a
    // persona offered this tool gets the operating guidance.
    #[test]
    fn description_from_doc_comment() {
        assert!(AgentStart::DESCRIPTION.contains("autonomous coding agent"));
    }

    // what this catches: the optional `max_iterations` defaults to 50 when omitted —
    // the legacy `u64_or("max_iterations", 50)` contract, preserved via serde default.
    #[test]
    fn max_iterations_defaults_to_50() {
        let p: AgentStartParams = serde_json::from_value(serde_json::json!({
            "task": "do x",
            "working_dir": "/tmp/x",
            "model": "deepseek-chat"
        }))
        .unwrap();
        assert_eq!(p.max_iterations, 50);
    }
}
