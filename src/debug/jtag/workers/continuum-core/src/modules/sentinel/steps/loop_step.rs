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
