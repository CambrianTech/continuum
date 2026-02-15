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
