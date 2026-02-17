//! Loop step execution — iterates over sub-steps with flexible termination
//!
//! Supports four modes:
//! - **count**: fixed N iterations (original behavior)
//! - **while**: condition checked before each iteration, continues while truthy
//! - **until**: condition checked after each iteration, stops when truthy
//! - **continuous**: no condition, runs until maxIterations (safety limit)

use serde_json::json;
use std::time::Instant;

use crate::modules::sentinel::interpolation;
use crate::modules::sentinel::types::{
    ExecutionContext, PipelineContext, PipelineStep, StepResult, DEFAULT_MAX_ITERATIONS,
};

/// Execute a loop step with flexible termination
pub async fn execute(
    count: Option<usize>,
    while_condition: Option<&str>,
    until: Option<&str>,
    max_iterations: Option<usize>,
    steps: &[PipelineStep],
    index: usize,
    ctx: &mut ExecutionContext,
    pipeline_ctx: &PipelineContext<'_>,
) -> Result<StepResult, String> {
    use crate::runtime;
    let log = runtime::logger("sentinel");
    let start = Instant::now();

    // Determine loop mode and iteration limit
    let mode: LoopMode = if let Some(n) = count {
        LoopMode::Count(n)
    } else if let Some(cond) = while_condition {
        LoopMode::While(cond.to_string())
    } else if let Some(cond) = until {
        LoopMode::Until(cond.to_string())
    } else {
        LoopMode::Continuous
    };

    // Safety limit: count mode uses exact count, all others use maxIterations or default
    let limit = match &mode {
        LoopMode::Count(n) => *n,
        _ => max_iterations.unwrap_or(DEFAULT_MAX_ITERATIONS),
    };

    log.info(&format!("[{}] Loop step: mode={}, limit={}",
        pipeline_ctx.handle_id, mode.name(), limit));

    let mut iteration: usize = 0;

    loop {
        if iteration >= limit {
            log.info(&format!("[{}] Loop reached iteration limit {}",
                pipeline_ctx.handle_id, limit));
            break;
        }

        // While mode: check condition BEFORE executing
        if let LoopMode::While(ref cond) = mode {
            let interpolated = interpolation::interpolate(cond, ctx);
            if !interpolation::evaluate_condition(&interpolated) {
                log.info(&format!("[{}] While condition false at iteration {}",
                    pipeline_ctx.handle_id, iteration));
                break;
            }
        }

        // Set iteration variable for interpolation: {{input.iteration}}
        ctx.inputs.insert("iteration".to_string(), json!(iteration));

        // Execute sub-steps
        for step in steps {
            let sub_result = super::execute_step(step, ctx.step_results.len(), ctx, pipeline_ctx).await?;
            if !sub_result.success {
                return Ok(StepResult {
                    step_index: index,
                    step_type: "loop".to_string(),
                    success: false,
                    duration_ms: start.elapsed().as_millis() as u64,
                    output: None,
                    error: sub_result.error,
                    exit_code: None,
                    data: json!({
                        "mode": mode.name(),
                        "iteration": iteration,
                        "iterationsCompleted": iteration,
                    }),
                });
            }
            ctx.step_results.push(sub_result);
        }

        iteration += 1;

        // Until mode: check condition AFTER executing
        if let LoopMode::Until(ref cond) = mode {
            let interpolated = interpolation::interpolate(cond, ctx);
            if interpolation::evaluate_condition(&interpolated) {
                log.info(&format!("[{}] Until condition met at iteration {}",
                    pipeline_ctx.handle_id, iteration));
                break;
            }
        }
    }

    Ok(StepResult {
        step_index: index,
        step_type: "loop".to_string(),
        success: true,
        duration_ms: start.elapsed().as_millis() as u64,
        output: None,
        error: None,
        exit_code: None,
        data: json!({
            "mode": mode.name(),
            "iterationsCompleted": iteration,
        }),
    })
}

/// Internal enum to classify the loop's termination strategy
enum LoopMode {
    Count(usize),
    While(String),
    Until(String),
    Continuous,
}

impl LoopMode {
    fn name(&self) -> &'static str {
        match self {
            Self::Count(_) => "count",
            Self::While(_) => "while",
            Self::Until(_) => "until",
            Self::Continuous => "continuous",
        }
    }
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
            handle_id: "test-loop",
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

    fn echo_iteration_step() -> PipelineStep {
        PipelineStep::Shell {
            cmd: "echo".to_string(),
            args: vec!["iter-{{input.iteration}}".to_string()],
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
    async fn test_count_mode() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let result = execute(
            Some(3), None, None, None,
            &[echo_step("counted")],
            0, &mut ctx, &pipeline_ctx,
        ).await.unwrap();

        assert!(result.success);
        assert_eq!(result.data["mode"], "count");
        assert_eq!(result.data["iterationsCompleted"], 3);
        assert_eq!(ctx.step_results.len(), 3);
    }

    #[tokio::test]
    async fn test_count_zero() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let result = execute(
            Some(0), None, None, None,
            &[echo_step("should-not-run")],
            0, &mut ctx, &pipeline_ctx,
        ).await.unwrap();

        assert!(result.success);
        assert_eq!(result.data["iterationsCompleted"], 0);
        assert_eq!(ctx.step_results.len(), 0);
    }

    #[tokio::test]
    async fn test_while_false_immediate_exit() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let result = execute(
            None, Some("false"), None, None,
            &[echo_step("should-not-run")],
            0, &mut ctx, &pipeline_ctx,
        ).await.unwrap();

        assert!(result.success);
        assert_eq!(result.data["mode"], "while");
        assert_eq!(result.data["iterationsCompleted"], 0);
    }

    #[tokio::test]
    async fn test_while_true_hits_limit() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let result = execute(
            None, Some("true"), None, Some(5),
            &[echo_step("looping")],
            0, &mut ctx, &pipeline_ctx,
        ).await.unwrap();

        assert!(result.success);
        assert_eq!(result.data["mode"], "while");
        assert_eq!(result.data["iterationsCompleted"], 5);
        assert_eq!(ctx.step_results.len(), 5);
    }

    #[tokio::test]
    async fn test_until_true_one_iteration() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        // until "true" → executes once, then condition is true, so stops
        let result = execute(
            None, None, Some("true"), None,
            &[echo_step("once")],
            0, &mut ctx, &pipeline_ctx,
        ).await.unwrap();

        assert!(result.success);
        assert_eq!(result.data["mode"], "until");
        assert_eq!(result.data["iterationsCompleted"], 1);
        assert_eq!(ctx.step_results.len(), 1);
    }

    #[tokio::test]
    async fn test_until_false_hits_limit() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let result = execute(
            None, None, Some("false"), Some(4),
            &[echo_step("repeating")],
            0, &mut ctx, &pipeline_ctx,
        ).await.unwrap();

        assert!(result.success);
        assert_eq!(result.data["mode"], "until");
        assert_eq!(result.data["iterationsCompleted"], 4);
    }

    #[tokio::test]
    async fn test_continuous_mode() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        // No count, no while, no until → continuous, uses maxIterations
        let result = execute(
            None, None, None, Some(3),
            &[echo_step("continuous")],
            0, &mut ctx, &pipeline_ctx,
        ).await.unwrap();

        assert!(result.success);
        assert_eq!(result.data["mode"], "continuous");
        assert_eq!(result.data["iterationsCompleted"], 3);
    }

    #[tokio::test]
    async fn test_iteration_variable_interpolated() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let result = execute(
            Some(3), None, None, None,
            &[echo_iteration_step()],
            0, &mut ctx, &pipeline_ctx,
        ).await.unwrap();

        assert!(result.success);
        assert_eq!(ctx.step_results.len(), 3);
        assert_eq!(ctx.step_results[0].output.as_deref(), Some("iter-0\n"));
        assert_eq!(ctx.step_results[1].output.as_deref(), Some("iter-1\n"));
        assert_eq!(ctx.step_results[2].output.as_deref(), Some("iter-2\n"));
    }

    #[tokio::test]
    async fn test_failing_substep_stops_loop() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let result = execute(
            Some(5), None, None, None,
            &[failing_step()],
            0, &mut ctx, &pipeline_ctx,
        ).await.unwrap();

        assert!(!result.success);
        assert_eq!(result.data["mode"], "count");
        // Should stop after first iteration's failure
        assert_eq!(result.data["iteration"], 0);
    }

    #[tokio::test]
    async fn test_multiple_steps_per_iteration() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let result = execute(
            Some(2), None, None, None,
            &[echo_step("step-a"), echo_step("step-b")],
            0, &mut ctx, &pipeline_ctx,
        ).await.unwrap();

        assert!(result.success);
        assert_eq!(result.data["iterationsCompleted"], 2);
        // 2 iterations × 2 steps = 4 step results
        assert_eq!(ctx.step_results.len(), 4);
    }
}
