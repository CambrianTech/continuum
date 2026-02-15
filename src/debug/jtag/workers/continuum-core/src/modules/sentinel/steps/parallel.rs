//! Parallel step execution — runs multiple branch pipelines concurrently
//!
//! Each branch receives a snapshot of the execution context at fork time.
//! Branches execute independently and results are collected.

use serde_json::json;
use std::time::Instant;

use crate::modules::sentinel::types::{ExecutionContext, PipelineContext, PipelineStep, StepResult};

/// Execute branches concurrently, collecting results from each.
///
/// Each branch is an independent sequence of steps. They share a read-only
/// snapshot of the context at fork time but diverge independently.
pub async fn execute(
    branches: &[Vec<PipelineStep>],
    fail_fast: bool,
    index: usize,
    ctx: &mut ExecutionContext,
    pipeline_ctx: &PipelineContext<'_>,
) -> Result<StepResult, String> {
    use crate::runtime;
    let log = runtime::logger("sentinel");
    let start = Instant::now();

    if branches.is_empty() {
        return Ok(StepResult {
            step_index: index,
            step_type: "parallel".to_string(),
            success: true,
            duration_ms: 0,
            output: None,
            error: None,
            exit_code: None,
            data: json!({ "branchCount": 0, "branchResults": [] }),
        });
    }

    log.info(&format!("[{}] Parallel step: {} branches, failFast={}",
        pipeline_ctx.handle_id, branches.len(), fail_fast));

    // Snapshot the context for each branch (clone at fork point)
    let branch_contexts: Vec<ExecutionContext> = (0..branches.len())
        .map(|_| ctx.clone())
        .collect();

    // Build futures for each branch
    let mut handles = Vec::with_capacity(branches.len());

    for (branch_idx, (branch_steps, mut branch_ctx)) in branches.iter().zip(branch_contexts).enumerate() {
        // Each branch needs its own copy of the steps reference and pipeline context fields.
        // Since we can't move pipeline_ctx into multiple futures, we extract what we need.
        let handle_id = pipeline_ctx.handle_id.to_string();
        let registry = pipeline_ctx.registry.clone();
        let bus = pipeline_ctx.bus.cloned();
        let branch_steps = branch_steps.clone();

        let handle = tokio::spawn(async move {
            let pipeline_ctx = PipelineContext {
                handle_id: &handle_id,
                registry: &registry,
                bus: bus.as_ref(),
            };

            let mut branch_results: Vec<StepResult> = Vec::new();
            let mut branch_success = true;
            let mut branch_error: Option<String> = None;

            for (step_idx, step) in branch_steps.iter().enumerate() {
                match super::execute_step(step, branch_ctx.step_results.len(), &mut branch_ctx, &pipeline_ctx).await {
                    Ok(result) => {
                        if !result.success {
                            branch_success = false;
                            branch_error = result.error.clone();
                            branch_results.push(result);
                            break;
                        }
                        branch_ctx.step_results.push(result.clone());
                        branch_results.push(result);
                    }
                    Err(e) => {
                        branch_success = false;
                        branch_error = Some(e.clone());
                        branch_results.push(StepResult {
                            step_index: step_idx,
                            step_type: "error".to_string(),
                            success: false,
                            duration_ms: 0,
                            output: None,
                            error: Some(e),
                            exit_code: None,
                            data: serde_json::Value::Null,
                        });
                        break;
                    }
                }
            }

            (branch_idx, branch_success, branch_error, branch_results)
        });

        handles.push(handle);
    }

    // Collect results from all branches
    let mut branch_summaries = Vec::with_capacity(handles.len());
    let mut all_success = true;
    let mut first_error: Option<String> = None;

    for handle in handles {
        match handle.await {
            Ok((branch_idx, success, error, results)) => {
                if !success {
                    all_success = false;
                    if first_error.is_none() {
                        first_error = error.clone();
                    }
                }
                branch_summaries.push(json!({
                    "branch": branch_idx,
                    "success": success,
                    "stepsCompleted": results.len(),
                    "error": error,
                }));
            }
            Err(e) => {
                all_success = false;
                if first_error.is_none() {
                    first_error = Some(format!("Branch panicked: {e}"));
                }
                branch_summaries.push(json!({
                    "branch": branch_summaries.len(),
                    "success": false,
                    "error": format!("Branch panicked: {e}"),
                }));
            }
        }
    }

    let duration_ms = start.elapsed().as_millis() as u64;

    log.info(&format!("[{}] Parallel step completed: success={}, duration={}ms",
        pipeline_ctx.handle_id, all_success, duration_ms));

    Ok(StepResult {
        step_index: index,
        step_type: "parallel".to_string(),
        success: all_success,
        duration_ms,
        output: None,
        error: first_error,
        exit_code: None,
        data: json!({
            "branchCount": branches.len(),
            "branchResults": branch_summaries,
        }),
    })
}
