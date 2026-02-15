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
