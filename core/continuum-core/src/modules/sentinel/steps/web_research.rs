//! WebResearch step — delegates to TypeScript sentinel/web-research command.
//!
//! Rust dispatches to TS which uses headless browser for search + extraction.

use serde_json::json;
use std::time::Instant;

use super::super::interpolation;
use super::super::types::{ExecutionContext, PipelineContext, StepResult};

/// Execute a WebResearch step by dispatching to TypeScript
pub async fn execute(
    query: &str,
    max_pages: Option<u32>,
    extract: Option<&str>,
    index: usize,
    ctx: &mut ExecutionContext,
    pipeline_ctx: &PipelineContext<'_>,
) -> Result<StepResult, String> {
    let start = Instant::now();
    let handle_id = pipeline_ctx.handle_id;

    // Interpolate query and extract
    let query = interpolation::interpolate(query, ctx);
    let extract = extract.map(|e| interpolation::interpolate(e, ctx));

    let log = crate::runtime::logger("sentinel");
    log.info(&format!(
        "[{handle_id}] WebResearch step {index}: query=\"{query}\""
    ));

    let mut params = json!({
        "query": query,
        "maxPages": max_pages.unwrap_or(3),
    });
    if let Some(ref ext) = extract {
        params["extract"] = json!(ext);
    }

    // Dispatch to TypeScript — bypasses Rust registry (sentinel/ prefix collision)
    let result = match pipeline_ctx.executor {
        Some(exec) => exec.execute_ts_json("sentinel/web-research", params).await,
        None => Err("WebResearch step: no CommandExecutor in pipeline context".to_string()),
    };

    let duration_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(data) => {
            let success = data
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let output = data
                .get("summary")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            log.info(&format!(
                "[{handle_id}] WebResearch step {index}: completed, {} chars summary",
                output.len()
            ));

            Ok(StepResult {
                step_index: index,
                step_type: "webresearch".to_string(),
                success,
                duration_ms,
                output: Some(output),
                error: if success {
                    None
                } else {
                    data.get("error")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                },
                exit_code: None,
                data,
            })
        }
        Err(e) => {
            log.error(&format!(
                "[{handle_id}] WebResearch step {index} failed: {e}"
            ));
            Ok(StepResult {
                step_index: index,
                step_type: "webresearch".to_string(),
                success: false,
                duration_ms,
                output: None,
                error: Some(e),
                exit_code: None,
                data: serde_json::Value::Null,
            })
        }
    }
}
