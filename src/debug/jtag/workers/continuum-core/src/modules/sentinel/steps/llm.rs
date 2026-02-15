//! LLM step execution — calls AIProviderModule via registry

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
}

/// Execute an LLM step
pub async fn execute(
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

    log.info(&format!("[{}] LLM step: model={:?}, provider={:?}, prompt_len={}",
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
