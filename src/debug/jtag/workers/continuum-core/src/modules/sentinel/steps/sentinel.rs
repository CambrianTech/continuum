//! Sentinel step execution — executes a nested pipeline inline
//!
//! Enables recursive pipeline composition. The nested pipeline runs
//! within the same execution context, sharing the registry and bus.

use serde_json::json;
use std::path::PathBuf;
use std::time::Instant;

use crate::modules::sentinel::types::{ExecutionContext, Pipeline, PipelineContext, StepResult};

/// Execute a nested pipeline inline, returning its result as a step result
pub async fn execute(
    pipeline: &Pipeline,
    index: usize,
    ctx: &mut ExecutionContext,
    pipeline_ctx: &PipelineContext<'_>,
) -> Result<StepResult, String> {
    use crate::runtime;
    let log = runtime::logger("sentinel");
    let start = Instant::now();

    let pipeline_name = pipeline.name.as_deref().unwrap_or("nested");

    log.info(&format!("[{}] Sentinel step: executing nested pipeline '{}' with {} steps",
        pipeline_ctx.handle_id, pipeline_name, pipeline.steps.len()));

    // Create a child execution context inheriting from parent
    let working_dir = pipeline.working_dir.clone()
        .map(PathBuf::from)
        .unwrap_or_else(|| ctx.working_dir.clone());

    let mut child_ctx = ExecutionContext {
        step_results: Vec::new(),
        inputs: pipeline.inputs.clone(),
        working_dir,
        named_outputs: ctx.named_outputs.clone(),
    };

    // Inherit parent inputs where child doesn't override
    for (key, value) in &ctx.inputs {
        child_ctx.inputs.entry(key.clone()).or_insert_with(|| value.clone());
    }

    let mut success = true;
    let mut error_msg: Option<String> = None;

    for (i, step) in pipeline.steps.iter().enumerate() {
        match super::execute_step(step, i, &mut child_ctx, pipeline_ctx).await {
            Ok(result) => {
                if !result.success {
                    success = false;
                    error_msg = result.error.clone();
                    child_ctx.step_results.push(result);
                    break;
                }
                child_ctx.step_results.push(result);
            }
            Err(e) => {
                success = false;
                error_msg = Some(e);
                break;
            }
        }
    }

    let duration_ms = start.elapsed().as_millis() as u64;
    let steps_completed = child_ctx.step_results.len();

    // Last step output becomes this step's output
    let last_output = child_ctx.step_results.last()
        .and_then(|r| r.output.clone());

    log.info(&format!("[{}] Sentinel step '{}' completed: success={}, steps={}/{}, duration={}ms",
        pipeline_ctx.handle_id, pipeline_name, success,
        steps_completed, pipeline.steps.len(), duration_ms));

    Ok(StepResult {
        step_index: index,
        step_type: "sentinel".to_string(),
        success,
        duration_ms,
        output: last_output,
        error: error_msg,
        exit_code: None,
        data: json!({
            "pipelineName": pipeline_name,
            "stepsCompleted": steps_completed,
            "stepsTotal": pipeline.steps.len(),
            "stepResults": child_ctx.step_results,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::sentinel::types::{ExecutionContext, PipelineStep};
    use crate::runtime::{ModuleRegistry, message_bus::MessageBus};
    use std::sync::Arc;
    use std::collections::HashMap;

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
            handle_id: "test-sentinel",
            registry,
            bus: Some(bus),
        }
    }

    fn echo_step(msg: &str) -> PipelineStep {
        PipelineStep::Shell {
            cmd: "echo".to_string(),
            args: vec![msg.to_string()],
            timeout_secs: Some(10),
            working_dir: None,
        }
    }

    fn failing_step() -> PipelineStep {
        PipelineStep::Shell {
            cmd: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), "exit 1".to_string()],
            timeout_secs: Some(10),
            working_dir: None,
        }
    }

    #[tokio::test]
    async fn test_nested_pipeline_succeeds() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let pipeline = Pipeline {
            name: Some("child".to_string()),
            steps: vec![echo_step("child-step-1"), echo_step("child-step-2")],
            working_dir: None,
            timeout_secs: None,
            inputs: HashMap::new(),
        };

        let result = execute(&pipeline, 0, &mut ctx, &pipeline_ctx).await.unwrap();

        assert!(result.success);
        assert_eq!(result.data["pipelineName"], "child");
        assert_eq!(result.data["stepsCompleted"], 2);
        assert_eq!(result.data["stepsTotal"], 2);
        // Last step output becomes sentinel step output
        assert_eq!(result.output.as_deref(), Some("child-step-2\n"));
    }

    #[tokio::test]
    async fn test_nested_pipeline_inherits_inputs() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();
        ctx.inputs.insert("parent_var".to_string(), json!("inherited"));

        let pipeline = Pipeline {
            name: Some("child".to_string()),
            steps: vec![PipelineStep::Shell {
                cmd: "echo".to_string(),
                args: vec!["{{input.parent_var}}".to_string()],
                timeout_secs: Some(10),
                working_dir: None,
            }],
            working_dir: None,
            timeout_secs: None,
            inputs: HashMap::new(),
        };

        let result = execute(&pipeline, 0, &mut ctx, &pipeline_ctx).await.unwrap();

        assert!(result.success);
        assert_eq!(result.output.as_deref(), Some("inherited\n"));
    }

    #[tokio::test]
    async fn test_nested_pipeline_child_overrides() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();
        ctx.inputs.insert("var".to_string(), json!("parent_value"));

        let mut child_inputs = HashMap::new();
        child_inputs.insert("var".to_string(), json!("child_value"));

        let pipeline = Pipeline {
            name: Some("child".to_string()),
            steps: vec![PipelineStep::Shell {
                cmd: "echo".to_string(),
                args: vec!["{{input.var}}".to_string()],
                timeout_secs: Some(10),
                working_dir: None,
            }],
            working_dir: None,
            timeout_secs: None,
            inputs: child_inputs,
        };

        let result = execute(&pipeline, 0, &mut ctx, &pipeline_ctx).await.unwrap();

        assert!(result.success);
        assert_eq!(result.output.as_deref(), Some("child_value\n"));
    }

    #[tokio::test]
    async fn test_nested_pipeline_failure() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let pipeline = Pipeline {
            name: Some("failing-child".to_string()),
            steps: vec![echo_step("ok"), failing_step(), echo_step("never-reached")],
            working_dir: None,
            timeout_secs: None,
            inputs: HashMap::new(),
        };

        let result = execute(&pipeline, 0, &mut ctx, &pipeline_ctx).await.unwrap();

        assert!(!result.success);
        assert_eq!(result.data["stepsCompleted"], 2); // echo ok + failing step
        assert_eq!(result.data["stepsTotal"], 3);
    }

    #[tokio::test]
    async fn test_empty_nested_pipeline() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let pipeline = Pipeline {
            name: Some("empty".to_string()),
            steps: vec![],
            working_dir: None,
            timeout_secs: None,
            inputs: HashMap::new(),
        };

        let result = execute(&pipeline, 0, &mut ctx, &pipeline_ctx).await.unwrap();

        assert!(result.success);
        assert_eq!(result.data["stepsCompleted"], 0);
        assert!(result.output.is_none());
    }

    #[tokio::test]
    async fn test_unnamed_nested_pipeline() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let pipeline = Pipeline {
            name: None,
            steps: vec![echo_step("anon")],
            working_dir: None,
            timeout_secs: None,
            inputs: HashMap::new(),
        };

        let result = execute(&pipeline, 0, &mut ctx, &pipeline_ctx).await.unwrap();

        assert!(result.success);
        assert_eq!(result.data["pipelineName"], "nested");
    }
}
