//! Command step execution — routes commands through TypeScript CommandRouterServer
//!
//! Pipeline commands are high-level TypeScript commands (e.g., `data/create` with
//! `collection` + `data`). They MUST go through TypeScript for context injection
//! (dbPath resolution, sessionId, userId). Routing through the Rust module registry
//! would hit low-level Rust handlers that expect internal protocol fields (like dbPath).

use serde_json::Value;
use std::time::Instant;

use crate::modules::sentinel::interpolation;
use crate::modules::sentinel::types::{ExecutionContext, PipelineContext, StepResult};

/// Execute a command step via TypeScript CommandRouterServer.
///
/// Always routes through TypeScript (bypasses Rust module registry) because:
/// 1. Pipeline commands are TypeScript-level commands needing context injection
/// 2. Rust modules (e.g., DataModule) expect internal protocol fields (dbPath)
///    that TypeScript auto-resolves via DatabaseHandleRegistry
/// 3. TypeScript CommandRouterServer injects context, sessionId, userId
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

    let json = runtime::command_executor::execute_ts_json(&interpolated_command, interpolated_params).await
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
