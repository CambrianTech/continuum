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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::sentinel::types::ExecutionContext;
    use crate::runtime::{ModuleRegistry, message_bus::MessageBus};
    use std::sync::Arc;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn test_ctx() -> ExecutionContext {
        ExecutionContext {
            step_results: Vec::new(),
            inputs: HashMap::new(),
            working_dir: PathBuf::from("/tmp"),
            named_outputs: HashMap::new(),
        }
    }

    fn test_pipeline_ctx<'a>(registry: &'a Arc<ModuleRegistry>, bus: &'a Arc<MessageBus>) -> PipelineContext<'a> {
        PipelineContext {
            handle_id: "test-emit",
            registry,
            bus: Some(bus),
        }
    }

    #[tokio::test]
    async fn test_emit_publishes_event() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let mut receiver = bus.receiver();
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let payload = json!({"status": "done"});
        let result = execute("build:complete", &payload, 0, &mut ctx, &pipeline_ctx).await.unwrap();

        assert!(result.success);
        assert_eq!(result.output.as_deref(), Some("build:complete"));
        assert_eq!(result.data["event"], "build:complete");
        assert_eq!(result.data["payload"]["status"], "done");

        // Verify the event was actually published on the bus
        let received = receiver.try_recv().unwrap();
        assert_eq!(received.name, "build:complete");
    }

    #[tokio::test]
    async fn test_emit_interpolates_event_name() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();
        ctx.inputs.insert("phase".to_string(), json!("deploy"));

        let result = execute(
            "{{input.phase}}:complete",
            &json!({}),
            0, &mut ctx, &pipeline_ctx,
        ).await.unwrap();

        assert!(result.success);
        assert_eq!(result.output.as_deref(), Some("deploy:complete"));
    }

    #[tokio::test]
    async fn test_emit_interpolates_payload() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();
        ctx.inputs.insert("name".to_string(), json!("my-pipeline"));

        let result = execute(
            "test:event",
            &json!({"pipeline": "{{input.name}}"}),
            0, &mut ctx, &pipeline_ctx,
        ).await.unwrap();

        assert!(result.success);
        assert_eq!(result.data["payload"]["pipeline"], "my-pipeline");
    }

    #[tokio::test]
    async fn test_emit_requires_bus() {
        let registry = Arc::new(ModuleRegistry::new());
        let pipeline_ctx = PipelineContext {
            handle_id: "test-emit",
            registry: &registry,
            bus: None,
        };
        let mut ctx = test_ctx();

        let result = execute("test:event", &json!({}), 0, &mut ctx, &pipeline_ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("requires MessageBus"));
    }
}
