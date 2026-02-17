//! Shell step execution — runs a child process with isolation

use serde_json::json;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::process::Command;

use crate::modules::sentinel::interpolation;
use crate::modules::sentinel::types::{ExecutionContext, PipelineContext, StepResult};

/// Execute a shell step
pub async fn execute(
    cmd: &str,
    args: &[String],
    timeout_secs: u64,
    working_dir_override: Option<&String>,
    index: usize,
    ctx: &mut ExecutionContext,
    pipeline_ctx: &PipelineContext<'_>,
) -> Result<StepResult, String> {
    use crate::runtime;
    let log = runtime::logger("sentinel");
    let start = Instant::now();

    let interpolated_cmd = interpolation::interpolate(cmd, ctx);
    let interpolated_args: Vec<String> = args.iter()
        .map(|arg| interpolation::interpolate(arg, ctx))
        .collect();

    let work_dir = working_dir_override
        .map(|p| PathBuf::from(interpolation::interpolate(p, ctx)))
        .unwrap_or_else(|| ctx.working_dir.clone());

    log.info(&format!("[{}] Shell: {} {:?} in {:?}",
        pipeline_ctx.handle_id, interpolated_cmd, interpolated_args, work_dir));

    // If cmd contains spaces and no args, run through shell
    let (actual_cmd, actual_args): (String, Vec<String>) = if interpolated_cmd.contains(' ') && interpolated_args.is_empty() {
        ("/bin/sh".to_string(), vec!["-c".to_string(), interpolated_cmd])
    } else {
        (interpolated_cmd, interpolated_args)
    };

    let output = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        Command::new(&actual_cmd)
            .args(&actual_args)
            .current_dir(&work_dir)
            .kill_on_drop(true)
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
            Err(format!("[{}] Shell step failed to execute '{}': {}",
                pipeline_ctx.handle_id, actual_cmd, e))
        }
        Err(_) => {
            Err(format!("[{}] Shell step timed out after {}s",
                pipeline_ctx.handle_id, timeout_secs))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::sentinel::types::ExecutionContext;
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
            handle_id: "test-001",
            registry,
            bus: Some(bus),
        }
    }

    #[tokio::test]
    async fn test_echo() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let result = execute("echo", &["hello".into()], 10, None, 0, &mut ctx, &pipeline_ctx).await.unwrap();
        assert!(result.success);
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.output.as_deref(), Some("hello\n"));
    }

    #[tokio::test]
    async fn test_nonzero_exit() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let result = execute("/bin/sh", &["-c".into(), "exit 42".into()], 10, None, 0, &mut ctx, &pipeline_ctx).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.exit_code, Some(42));
    }

    #[tokio::test]
    async fn test_shell_passthrough_for_spaces() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        // cmd with spaces and no args should be passed through /bin/sh -c
        let result = execute("echo hello world", &[], 10, None, 0, &mut ctx, &pipeline_ctx).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output.as_deref(), Some("hello world\n"));
    }

    #[tokio::test]
    async fn test_timeout() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let result = execute("sleep", &["10".into()], 1, None, 0, &mut ctx, &pipeline_ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("timed out"));
    }

    #[tokio::test]
    async fn test_invalid_command() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let result = execute("/nonexistent/binary", &[], 10, None, 0, &mut ctx, &pipeline_ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("failed to execute"));
    }

    #[tokio::test]
    async fn test_cmd_interpolation() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();
        ctx.inputs.insert("msg".to_string(), serde_json::json!("interpolated"));

        let result = execute("echo", &["{{input.msg}}".into()], 10, None, 0, &mut ctx, &pipeline_ctx).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output.as_deref(), Some("interpolated\n"));
    }

    #[tokio::test]
    async fn test_data_has_stdout_stderr() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let result = execute("echo", &["data-test".into()], 10, None, 0, &mut ctx, &pipeline_ctx).await.unwrap();
        assert_eq!(result.data["stdout"], "data-test\n");
        assert_eq!(result.data["exitCode"], 0);
    }
}
