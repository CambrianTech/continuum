//! Shell step execution — runs a child process

use serde_json::json;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::process::Command;

use crate::modules::sentinel::interpolation;
use crate::modules::sentinel::types::{ExecutionContext, StepResult};

/// Execute a shell step
pub async fn execute(
    cmd: &str,
    args: &[String],
    timeout_secs: u64,
    working_dir_override: Option<&String>,
    index: usize,
    ctx: &mut ExecutionContext,
    handle_id: &str,
) -> Result<StepResult, String> {
    use crate::runtime;
    let log = runtime::logger("sentinel");
    let start = Instant::now();

    // Interpolate args
    let interpolated_args: Vec<String> = args.iter()
        .map(|arg| interpolation::interpolate(arg, ctx))
        .collect();

    let work_dir = working_dir_override
        .map(|p| PathBuf::from(interpolation::interpolate(p, ctx)))
        .unwrap_or_else(|| ctx.working_dir.clone());

    log.info(&format!("[{handle_id}] Shell: {cmd} {interpolated_args:?} in {work_dir:?}"));

    // If cmd contains spaces and no args, run through shell
    let (actual_cmd, actual_args): (&str, Vec<String>) = if cmd.contains(' ') && interpolated_args.is_empty() {
        ("/bin/sh", vec!["-c".to_string(), cmd.to_string()])
    } else {
        (cmd, interpolated_args)
    };

    let output = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        Command::new(actual_cmd)
            .args(&actual_args)
            .current_dir(&work_dir)
            .output()
    ).await;

    let duration_ms = start.elapsed().as_millis() as u64;

    match output {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let exit_code = output.status.code().unwrap_or(-1);
            let success = exit_code == 0;

            Ok(StepResult {
                step_index: index,
                step_type: "shell".to_string(),
                success,
                duration_ms,
                output: Some(stdout.clone()),
                error: if success { None } else { Some(stderr) },
                exit_code: Some(exit_code),
                data: json!({
                    "stdout": stdout,
                    "stderr": String::from_utf8_lossy(&output.stderr),
                    "exitCode": exit_code,
                }),
            })
        }
        Ok(Err(e)) => {
            Err(format!("Failed to execute command: {e}"))
        }
        Err(_) => {
            Err(format!("Command timed out after {timeout_secs}s"))
        }
    }
}
