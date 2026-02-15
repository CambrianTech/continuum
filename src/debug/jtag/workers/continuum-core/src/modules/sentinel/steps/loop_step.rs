//! Loop step execution — iterates N times over sub-steps

use serde_json::json;
use std::time::Instant;

use crate::modules::sentinel::types::{ExecutionContext, PipelineContext, PipelineStep, StepResult};

/// Execute a loop step
pub async fn execute(
    count: usize,
    steps: &[PipelineStep],
    index: usize,
    ctx: &mut ExecutionContext,
    pipeline_ctx: &PipelineContext<'_>,
) -> Result<StepResult, String> {
    let start = Instant::now();

    for iteration in 0..count {
        ctx.inputs.insert("iteration".to_string(), json!(iteration));

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
                        "iteration": iteration,
                        "totalIterations": count,
                    }),
                });
            }
            ctx.step_results.push(sub_result);
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
            "iterationsCompleted": count,
        }),
    })
}
