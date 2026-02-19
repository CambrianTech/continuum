//! LLM step execution — dual mode:
//!
//! agentMode=false (default): fast in-process Rust call to ai/generate via ModuleRegistry
//! agentMode=true: routes to TypeScript ai/agent via CommandExecutor (Unix socket IPC)
//!   for full agentic loop with tool calling, 243+ discoverable tools

use serde_json::json;
use std::time::Instant;

use crate::runtime::CommandResult;
use crate::modules::sentinel::interpolation;
use crate::modules::sentinel::types::{ExecutionContext, PipelineContext, StepResult};

/// LLM step configuration extracted from PipelineStep::Llm
pub struct LlmStepParams<'a> {
    pub prompt: &'a str,
    pub model: Option<&'a str>,
    pub provider: Option<&'a str>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub system_prompt: Option<&'a str>,
    pub tools: Option<&'a Vec<String>>,
    pub agent_mode: Option<bool>,
    pub max_iterations: Option<u32>,
}

/// Execute an LLM step — routes to Rust ai/generate or TypeScript ai/agent
pub async fn execute(
    params: LlmStepParams<'_>,
    index: usize,
    ctx: &mut ExecutionContext,
    pipeline_ctx: &PipelineContext<'_>,
) -> Result<StepResult, String> {
    let is_agent = params.agent_mode.unwrap_or(false);

    if is_agent {
        execute_agent_mode(params, index, ctx, pipeline_ctx).await
    } else {
        execute_generate_mode(params, index, ctx, pipeline_ctx).await
    }
}

/// agentMode=false: Fast in-process Rust call to ai/generate via ModuleRegistry
async fn execute_generate_mode(
    params: LlmStepParams<'_>,
    index: usize,
    ctx: &mut ExecutionContext,
    pipeline_ctx: &PipelineContext<'_>,
) -> Result<StepResult, String> {
    use crate::runtime;
    let log = runtime::logger("sentinel");
    let start = Instant::now();

    let interpolated_prompt = interpolation::interpolate(params.prompt, ctx);
    let interpolated_system = params.system_prompt.map(|s| interpolation::interpolate(s, ctx));

    log.info(&format!("[{}] LLM step (generate): model={:?}, provider={:?}, prompt_len={}",
        pipeline_ctx.handle_id, params.model, params.provider, interpolated_prompt.len()));

    let mut ai_params = json!({
        "prompt": interpolated_prompt,
    });

    if let Some(m) = params.model {
        ai_params["model"] = json!(m);
    }
    if let Some(p) = params.provider {
        ai_params["provider"] = json!(p);
    }
    if let Some(t) = params.max_tokens {
        ai_params["max_tokens"] = json!(t);
    }
    if let Some(temp) = params.temperature {
        ai_params["temperature"] = json!(temp);
    }
    if let Some(sys) = interpolated_system {
        ai_params["system_prompt"] = json!(sys);
    }

    let (module, cmd) = pipeline_ctx.registry.route_command("ai/generate")
        .ok_or_else(|| format!("[{}] ai module not found in registry", pipeline_ctx.handle_id))?;

    let result = module.handle_command(&cmd, ai_params).await
        .map_err(|e| format!("[{}] LLM step error: {}", pipeline_ctx.handle_id, e))?;

    let duration_ms = start.elapsed().as_millis() as u64;

    match result {
        CommandResult::Json(json) => {
            let success = json.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
            let text = json.get("text").and_then(|v| v.as_str()).map(|s| s.to_string());
            let error = json.get("error").and_then(|v| v.as_str()).map(|s| s.to_string());

            Ok(StepResult {
                step_index: index,
                step_type: "llm".to_string(),
                success,
                duration_ms,
                output: text,
                error,
                exit_code: None,
                data: json,
            })
        }
        CommandResult::Binary { .. } => {
            Err(format!("[{}] Unexpected binary response from ai/generate", pipeline_ctx.handle_id))
        }
    }
}

/// agentMode=true: Route to TypeScript ai/agent via CommandExecutor (Unix socket IPC)
/// for full agentic loop with tool calling
async fn execute_agent_mode(
    params: LlmStepParams<'_>,
    index: usize,
    ctx: &mut ExecutionContext,
    pipeline_ctx: &PipelineContext<'_>,
) -> Result<StepResult, String> {
    use crate::runtime;
    let log = runtime::logger("sentinel");
    let start = Instant::now();

    let interpolated_prompt = interpolation::interpolate(params.prompt, ctx);
    let interpolated_system = params.system_prompt.map(|s| interpolation::interpolate(s, ctx));

    log.info(&format!("[{}] LLM step (agent): model={:?}, provider={:?}, tools={:?}, prompt_len={}",
        pipeline_ctx.handle_id, params.model, params.provider, params.tools, interpolated_prompt.len()));

    // Build ai/agent command params
    let mut agent_params = json!({
        "prompt": interpolated_prompt,
        "sentinelHandle": pipeline_ctx.handle_id,
    });

    if let Some(m) = params.model {
        agent_params["model"] = json!(m);
    }
    if let Some(p) = params.provider {
        agent_params["provider"] = json!(p);
    }
    if let Some(t) = params.max_tokens {
        agent_params["maxTokens"] = json!(t);
    }
    if let Some(temp) = params.temperature {
        agent_params["temperature"] = json!(temp);
    }
    if let Some(sys) = interpolated_system {
        agent_params["systemPrompt"] = json!(sys);
    }
    if let Some(tools) = params.tools {
        agent_params["tools"] = json!(tools);
    }
    if let Some(max_iter) = params.max_iterations {
        agent_params["maxIterations"] = json!(max_iter);
    }

    // Route to TypeScript ai/agent directly via Unix socket (bypasses Rust registry).
    // MUST use execute_ts_json — the ai/ prefix is claimed by Rust's ai_provider module,
    // so execute_json would route back to Rust and never reach TypeScript.
    let json = runtime::command_executor::execute_ts_json("ai/agent", agent_params).await
        .map_err(|e| format!("[{}] LLM agent step error: {}", pipeline_ctx.handle_id, e))?;

    let duration_ms = start.elapsed().as_millis() as u64;

    let success = json.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
    let text = json.get("text").and_then(|v| v.as_str()).map(|s| s.to_string());
    let error = json.get("error").and_then(|v| v.as_str()).map(|s| s.to_string());
    let iterations = json.get("iterations").and_then(|v| v.as_u64()).unwrap_or(0);
    let tool_calls_count = json.get("toolCalls")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    log.info(&format!("[{}] LLM agent step complete: success={}, iterations={}, tool_calls={}, {}ms",
        pipeline_ctx.handle_id, success, iterations, tool_calls_count, duration_ms));

    Ok(StepResult {
        step_index: index,
        step_type: "llm".to_string(),
        success,
        duration_ms,
        output: text,
        error,
        exit_code: None,
        data: json, // Full ai/agent result including toolCalls array
    })
}
