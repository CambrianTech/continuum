//! Emit step execution — publishes events on the MessageBus
//!
//! Enables inter-sentinel composition: one sentinel emits an event,
//! another sentinel's Watch step receives it.

use serde_json::json;
use std::time::Instant;

use crate::modules::sentinel::interpolation;
use crate::modules::sentinel::types::{ExecutionContext, PipelineContext, StepResult};

/// Publish an event on the MessageBus with an interpolated payload
pub async fn execute(
    event: &str,
    payload: &serde_json::Value,
    index: usize,
    ctx: &mut ExecutionContext,
    pipeline_ctx: &PipelineContext<'_>,
) -> Result<StepResult, String> {
    use crate::runtime;
    let log = runtime::logger("sentinel");
    let start = Instant::now();

    let interpolated_event = interpolation::interpolate(event, ctx);
    let interpolated_payload = interpolation::interpolate_value(payload, ctx);

    log.info(&format!("[{}] Emit step: event={}", pipeline_ctx.handle_id, interpolated_event));

    let bus = pipeline_ctx.bus
        .ok_or_else(|| format!("[{}] Emit step requires MessageBus", pipeline_ctx.handle_id))?;

    bus.publish_async_only(&interpolated_event, interpolated_payload.clone());

    let duration_ms = start.elapsed().as_millis() as u64;

    Ok(StepResult {
        step_index: index,
        step_type: "emit".to_string(),
        success: true,
        duration_ms,
        output: Some(interpolated_event.clone()),
        error: None,
        exit_code: None,
        data: json!({
            "event": interpolated_event,
            "payload": interpolated_payload,
        }),
    })
}
