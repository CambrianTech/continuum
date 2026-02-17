//! Command step execution — routes to any command via CommandExecutor

use serde_json::Value;
use std::time::Instant;

use crate::modules::sentinel::interpolation;
use crate::modules::sentinel::types::{ExecutionContext, PipelineContext, StepResult};

/// Execute a command step via global CommandExecutor (routes to Rust OR TypeScript)
pub async fn execute(
    command: &str,
    params: &Value,
    index: usize,
    ctx: &mut ExecutionContext,
    pipeline_ctx: &PipelineContext<'_>,
) -> Result<StepResult, String> {
    use crate::runtime;
    let log = runtime::logger("sentinel");
    let start = Instant::now();

    let interpolated_command = interpolation::interpolate(command, ctx);
    let interpolated_params = interpolation::interpolate_value(params, ctx);

    log.info(&format!("[{}] Command step: {}", pipeline_ctx.handle_id, interpolated_command));

    let json = runtime::command_executor::execute_json(&interpolated_command, interpolated_params).await
        .map_err(|e| format!("[{}] Command '{}' failed: {}", pipeline_ctx.handle_id, interpolated_command, e))?;

    let duration_ms = start.elapsed().as_millis() as u64;

    let success = json.get("success").and_then(|v| v.as_bool()).unwrap_or(true);
    let error = json.get("error").and_then(|v| v.as_str()).map(|s| s.to_string());

    Ok(StepResult {
        step_index: index,
        step_type: "command".to_string(),
        success,
        duration_ms,
        output: None,
        error,
        exit_code: None,
        data: json,
    })
}
