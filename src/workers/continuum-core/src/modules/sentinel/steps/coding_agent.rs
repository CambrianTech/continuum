//! CodingAgent step execution — delegates to TypeScript sentinel/coding-agent command
//!
//! Routes entirely through TypeScript via execute_ts_json("sentinel/coding-agent", params).
//! TypeScript side resolves the provider (claude-code, codex, etc.) and executes.
//! Same delegation pattern as LLM agentMode → execute_ts_json("ai/agent").

use serde_json::json;
use std::time::Instant;

use crate::modules::sentinel::interpolation;
use crate::modules::sentinel::types::{step_err, ExecutionContext, PipelineContext, StepResult};

/// Execute a coding agent step via TypeScript provider architecture.
///
/// All string fields are interpolated before sending to TypeScript.
/// TypeScript resolves the provider from CodingAgentRegistry and calls provider.execute().
#[allow(clippy::too_many_arguments)]
pub async fn execute(
    prompt: &str,
    provider: Option<&str>,
    working_dir: Option<&str>,
    system_prompt: Option<&str>,
    model: Option<&str>,
    allowed_tools: Option<&Vec<String>>,
    max_turns: Option<u32>,
    max_budget_usd: Option<f64>,
    permission_mode: Option<&str>,
    resume_session_id: Option<&str>,
    capture_training: Option<bool>,
    persona_id: Option<&str>,
    index: usize,
    ctx: &mut ExecutionContext,
    pipeline_ctx: &PipelineContext<'_>,
) -> Result<StepResult, String> {
    use crate::runtime;
    let log = runtime::logger("sentinel");
    let start = Instant::now();

    let interpolated_prompt = interpolation::interpolate(prompt, ctx);
    let interpolated_system = system_prompt.map(|s| interpolation::interpolate(s, ctx));
    let interpolated_working_dir = working_dir.map(|s| interpolation::interpolate(s, ctx));
    let interpolated_persona_id = persona_id.map(|s| interpolation::interpolate(s, ctx));

    let provider_name = provider.unwrap_or("claude-code");

    log.info(&format!(
        "[{}] CodingAgent step: provider={}, model={:?}, maxTurns={:?}, prompt_len={}",
        pipeline_ctx.handle_id,
        provider_name,
        model,
        max_turns,
        interpolated_prompt.len()
    ));

    // Build params for TypeScript sentinel/coding-agent command
    let mut params = json!({
        "prompt": interpolated_prompt,
        "provider": provider_name,
        "cwd": interpolated_working_dir.as_deref().unwrap_or_else(|| ctx.working_dir.to_str().unwrap_or(".")),
        "sentinelHandle": pipeline_ctx.handle_id,
    });

    if let Some(sys) = interpolated_system {
        params["systemPrompt"] = json!(sys);
    }
    if let Some(m) = model {
        params["model"] = json!(m);
    }
    if let Some(tools) = allowed_tools {
        params["allowedTools"] = json!(tools);
    }
    if let Some(turns) = max_turns {
        params["maxTurns"] = json!(turns);
    }
    if let Some(budget) = max_budget_usd {
        params["maxBudgetUsd"] = json!(budget);
    }
    if let Some(perm) = permission_mode {
        params["permissionMode"] = json!(perm);
    }
    if let Some(session) = resume_session_id {
        params["resumeSessionId"] = json!(session);
    }
    if let Some(capture) = capture_training {
        params["captureTraining"] = json!(capture);
    }
    if let Some(pid) = interpolated_persona_id {
        params["personaId"] = json!(pid);
    }

    // Route to TypeScript via Unix socket — sentinel/ prefix is NOT claimed by a Rust module,
    // but we use execute_ts_json for consistency with the provider architecture living in TypeScript.
    let json = runtime::command_executor::execute_ts_json("sentinel/coding-agent", params)
        .await
        .map_err(|e| step_err(pipeline_ctx.handle_id, "CodingAgent step", e))?;

    let duration_ms = start.elapsed().as_millis() as u64;

    let success = json
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let text = json
        .get("text")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let error = json
        .get("error")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let session_id = json.get("sessionId").and_then(|v| v.as_str()).unwrap_or("");
    let num_turns = json.get("numTurns").and_then(|v| v.as_u64()).unwrap_or(0);
    let tool_calls_count = json
        .get("toolCalls")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let cost = json
        .get("totalCostUsd")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    log.info(&format!(
        "[{}] CodingAgent step complete: success={}, provider={}, session={}, turns={}, tools={}, cost=${:.4}, {}ms",
        pipeline_ctx.handle_id, success, provider_name, session_id, num_turns, tool_calls_count, cost, duration_ms
    ));

    Ok(StepResult {
        step_index: index,
        step_type: "codingagent".to_string(),
        success,
        duration_ms,
        output: text,
        error,
        exit_code: None,
        data: json,
    })
}
