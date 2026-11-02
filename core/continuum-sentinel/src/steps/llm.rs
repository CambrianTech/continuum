//! LLM step execution — dual mode:
//!
//! agentMode=false (default): fast in-process Rust call to ai/generate via ModuleRegistry
//! agentMode=true: routes to TypeScript ai/agent via CommandExecutor (Unix socket IPC)
//!   for full agentic loop with tool calling, 243+ discoverable tools

use serde_json::{json, Value};
use std::time::Instant;

use crate::interpolation;
use crate::types::{step_err, ExecutionContext, PipelineContext, StepResult};
use continuum_kernel::runtime::CommandResult;

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
    /// Active LoRA adapters to apply during inference (values are interpolated)
    pub active_adapters: Option<&'a Vec<Value>>,
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

/// agentMode=false: Fast in-process Rust call to ai/generate via ModuleRegistry.
async fn execute_generate_mode(
    params: LlmStepParams<'_>,
    index: usize,
    ctx: &mut ExecutionContext,
    pipeline_ctx: &PipelineContext<'_>,
) -> Result<StepResult, String> {
    use continuum_kernel::runtime;
    let log = runtime::logger("sentinel");
    let start = Instant::now();

    let interpolated_prompt = interpolation::interpolate(params.prompt, ctx);
    let interpolated_system = params
        .system_prompt
        .map(|s| interpolation::interpolate(s, ctx));

    log.info(&format!(
        "[{}] LLM step (generate): model={:?}, provider={:?}, prompt_len={}, max_tokens={:?}, temperature={:?}",
        pipeline_ctx.handle_id,
        params.model,
        params.provider,
        interpolated_prompt.len(),
        params.max_tokens,
        params.temperature,
    ));

    // The typed request ai/generate decodes: one wire shape, every field name checked by the
    // compiler. An adapter config that does not decode after interpolation is an error, never a
    // silent fall back to the uninterpolated one.
    let active_adapters = params
        .active_adapters
        .filter(|adapters| !adapters.is_empty())
        .map(|adapters| {
            adapters
                .iter()
                .map(|adapter| {
                    let raw = serde_json::to_string(adapter)
                        .map_err(|e| format!("[{}] active adapter does not encode: {e}", pipeline_ctx.handle_id))?;
                    let interpolated = interpolation::interpolate(&raw, ctx);
                    serde_json::from_str::<continuum_engine::ai::ActiveAdapterRequest>(&interpolated) // decode boundary: the step's adapter config, typed
                        .map_err(|e| format!("[{}] active adapter does not decode after interpolation: {e}", pipeline_ctx.handle_id))
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .transpose()?;
    let request = continuum_engine::ai::TextGenerationRequest {
        messages: vec![continuum_engine::ai::ChatMessage {
            role: "user".to_string(),
            content: continuum_engine::ai::MessageContent::Text(interpolated_prompt.clone()),
            name: None,
        }],
        system_prompt: interpolated_system,
        model: params.model.map(str::to_string),
        provider: params.provider.map(str::to_string),
        max_tokens: params.max_tokens,
        temperature: params.temperature,
        active_adapters,
        ..Default::default()
    };
    let ai_params = serde_json::to_value(&request).map_err(|e| format!("[{}] request does not encode: {e}", pipeline_ctx.handle_id))?;

    let (module, cmd) = pipeline_ctx
        .registry
        .route_command("ai/generate")
        .ok_or_else(|| {
            format!(
                "[{}] ai module not found in registry",
                pipeline_ctx.handle_id
            )
        })?;

    // One call. Whether an inference failure is worth retrying is the inference owner's typed
    // judgment (`InferenceError::is_retryable_unchanged`), never re-derived here by matching
    // substrings of error text under a guessed count; a failed generation fails the step,
    // visibly (2026-11-02).
    let result = module.handle_command(&cmd, ai_params).await;
    let duration_ms = start.elapsed().as_millis() as u64;
    match result {
        Ok(CommandResult::Json(json)) => {
            let success = json.get("success").and_then(Value::as_bool) == Some(true);
            let text = json.get("text").and_then(Value::as_str).map(str::to_string);
            let error = json.get("error").and_then(Value::as_str).map(str::to_string);
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
        Ok(CommandResult::Binary { .. }) => Err(step_err(
            pipeline_ctx.handle_id,
            "LLM step",
            "unexpected binary response from ai/generate",
        )),
        // Cell shapes from MODULE-ARCHITECTURE.md §5.1: ai/generate always returns Json; any
        // other shape is a contract violation surfaced as a step error, never dropped.
        Ok(CommandResult::Handle(h)) => Err(step_err(
            pipeline_ctx.handle_id,
            "LLM step",
            format!(
                "ai/generate must return Json, got Handle (owner={}, type={}); streaming \
                 inference belongs on a different command, not the one-shot generate path",
                h.owner, h.type_tag
            ),
        )),
        Ok(CommandResult::Stream(_)) => {
            Err(step_err(pipeline_ctx.handle_id, "LLM step", CommandResult::stream_protocol_error()))
        }
        Ok(CommandResult::Lambda(_)) => {
            Err(step_err(pipeline_ctx.handle_id, "LLM step", CommandResult::lambda_protocol_error()))
        }
        Err(e) => Err(step_err(pipeline_ctx.handle_id, "LLM step", e)),
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
    use continuum_kernel::runtime;
    let log = runtime::logger("sentinel");
    let start = Instant::now();

    let interpolated_prompt = interpolation::interpolate(params.prompt, ctx);
    let interpolated_system = params
        .system_prompt
        .map(|s| interpolation::interpolate(s, ctx));

    log.info(&format!(
        "[{}] LLM step (agent): model={:?}, provider={:?}, tools={:?}, prompt_len={}",
        pipeline_ctx.handle_id,
        params.model,
        params.provider,
        params.tools,
        interpolated_prompt.len()
    ));

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
    let json = match pipeline_ctx.executor {
        Some(exec) => exec
            .execute_ts_json("ai/agent", agent_params)
            .await
            .map_err(|e| step_err(pipeline_ctx.handle_id, "LLM agent step", e))?,
        None => {
            return Err(step_err(
                pipeline_ctx.handle_id,
                "LLM agent step",
                "no CommandExecutor in pipeline context".to_string(),
            ));
        }
    };

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
    let iterations = json.get("iterations").and_then(|v| v.as_u64()).unwrap_or(0);
    let tool_calls_count = json
        .get("toolCalls")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    log.info(&format!(
        "[{}] LLM agent step complete: success={}, iterations={}, tool_calls={}, {}ms",
        pipeline_ctx.handle_id, success, iterations, tool_calls_count, duration_ms
    ));

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
