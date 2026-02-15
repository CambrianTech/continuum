//! Condition step execution — evaluates expression and runs branch

use serde_json::json;
use std::time::Instant;

use crate::modules::sentinel::interpolation;
use crate::modules::sentinel::types::{ExecutionContext, PipelineContext, PipelineStep, StepResult};

/// Execute a condition step
pub async fn execute(
    condition: &str,
    then_steps: &[PipelineStep],
    else_steps: &[PipelineStep],
    index: usize,
    ctx: &mut ExecutionContext,
    pipeline_ctx: &PipelineContext<'_>,
) -> Result<StepResult, String> {
    let start = Instant::now();

    let interpolated = interpolation::interpolate(condition, ctx);
    let condition_result = interpolation::evaluate_condition(&interpolated);

    let steps_to_run = if condition_result { then_steps } else { else_steps };

    for (i, step) in steps_to_run.iter().enumerate() {
        let sub_result = super::execute_step(step, ctx.step_results.len(), ctx, pipeline_ctx).await?;
        if !sub_result.success {
            return Ok(StepResult {
                step_index: index,
                step_type: "condition".to_string(),
                success: false,
                duration_ms: start.elapsed().as_millis() as u64,
                output: None,
                error: sub_result.error,
                exit_code: None,
                data: json!({
                    "conditionResult": condition_result,
                    "branch": if condition_result { "then" } else { "else" },
                    "failedStep": i,
                }),
            });
        }
        ctx.step_results.push(sub_result);
    }

    Ok(StepResult {
        step_index: index,
        step_type: "condition".to_string(),
        success: true,
        duration_ms: start.elapsed().as_millis() as u64,
        output: None,
        error: None,
        exit_code: None,
        data: json!({
            "conditionResult": condition_result,
            "branch": if condition_result { "then" } else { "else" },
            "stepsExecuted": steps_to_run.len(),
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
            handle_id: "test-cond",
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
    async fn test_true_branch_executes() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let result = execute(
            "true",
            &[echo_step("then-branch")],
            &[echo_step("else-branch")],
            0, &mut ctx, &pipeline_ctx,
        ).await.unwrap();

        assert!(result.success);
        assert_eq!(result.data["conditionResult"], true);
        assert_eq!(result.data["branch"], "then");
        assert_eq!(result.data["stepsExecuted"], 1);
        // The shell step result should be pushed to ctx.step_results
        assert_eq!(ctx.step_results.len(), 1);
        assert_eq!(ctx.step_results[0].output.as_deref(), Some("then-branch\n"));
    }

    #[tokio::test]
    async fn test_false_branch_executes() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let result = execute(
            "false",
            &[echo_step("then-branch")],
            &[echo_step("else-branch")],
            0, &mut ctx, &pipeline_ctx,
        ).await.unwrap();

        assert!(result.success);
        assert_eq!(result.data["conditionResult"], false);
        assert_eq!(result.data["branch"], "else");
        assert_eq!(ctx.step_results.len(), 1);
        assert_eq!(ctx.step_results[0].output.as_deref(), Some("else-branch\n"));
    }

    #[tokio::test]
    async fn test_empty_else_branch() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let result = execute(
            "false",
            &[echo_step("then-branch")],
            &[],  // empty else
            0, &mut ctx, &pipeline_ctx,
        ).await.unwrap();

        assert!(result.success);
        assert_eq!(result.data["branch"], "else");
        assert_eq!(result.data["stepsExecuted"], 0);
        assert_eq!(ctx.step_results.len(), 0);
    }

    #[tokio::test]
    async fn test_interpolated_condition() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();
        ctx.inputs.insert("flag".to_string(), serde_json::json!("true"));

        let result = execute(
            "{{input.flag}}",
            &[echo_step("flag-was-true")],
            &[echo_step("flag-was-false")],
            0, &mut ctx, &pipeline_ctx,
        ).await.unwrap();

        assert!(result.success);
        assert_eq!(result.data["conditionResult"], true);
        assert_eq!(ctx.step_results[0].output.as_deref(), Some("flag-was-true\n"));
    }

    #[tokio::test]
    async fn test_falsy_input_takes_else() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();
        ctx.inputs.insert("flag".to_string(), serde_json::json!("0"));

        let result = execute(
            "{{input.flag}}",
            &[echo_step("flag-truthy")],
            &[echo_step("flag-falsy")],
            0, &mut ctx, &pipeline_ctx,
        ).await.unwrap();

        assert!(result.success);
        assert_eq!(result.data["conditionResult"], false);
        assert_eq!(ctx.step_results[0].output.as_deref(), Some("flag-falsy\n"));
    }

    #[tokio::test]
    async fn test_failing_substep_returns_failure() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let result = execute(
            "true",
            &[failing_step()],
            &[],
            0, &mut ctx, &pipeline_ctx,
        ).await.unwrap();

        assert!(!result.success);
        assert_eq!(result.data["conditionResult"], true);
        assert_eq!(result.data["branch"], "then");
        assert_eq!(result.data["failedStep"], 0);
    }

    #[tokio::test]
    async fn test_multiple_then_steps() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let result = execute(
            "true",
            &[echo_step("step-a"), echo_step("step-b"), echo_step("step-c")],
            &[],
            0, &mut ctx, &pipeline_ctx,
        ).await.unwrap();

        assert!(result.success);
        assert_eq!(result.data["stepsExecuted"], 3);
        assert_eq!(ctx.step_results.len(), 3);
    }
}
