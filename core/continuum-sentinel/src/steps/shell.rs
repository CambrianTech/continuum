//! Shell step execution — runs a child process with isolation

use serde_json::json;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::process::Command;

use crate::interpolation;
use crate::types::{step_err, ExecutionContext, PipelineContext, StepResult};

/// Execute a shell step
///
/// Environment variables in `env` are interpolated and set on the child process.
/// This is the safe way to pass arbitrary data (code, JSON, etc.) to shell commands
/// without shell quoting issues — the data goes through env vars, not the command string.
#[allow(clippy::too_many_arguments)]
pub async fn execute(
    cmd: &str,
    args: &[String],
    timeout_secs: u64,
    working_dir_override: Option<&String>,
    allow_failure: bool,
    index: usize,
    ctx: &mut ExecutionContext,
    pipeline_ctx: &PipelineContext<'_>,
    env: Option<&std::collections::HashMap<String, String>>,
) -> Result<StepResult, String> {
    use continuum_kernel::runtime;
    let log = runtime::logger("sentinel");
    let start = Instant::now();

    let interpolated_cmd = interpolation::interpolate(cmd, ctx);
    let interpolated_args: Vec<String> = args
        .iter()
        .map(|arg| interpolation::interpolate(arg, ctx))
        .collect();

    let work_dir = working_dir_override
        .map(|p| PathBuf::from(interpolation::interpolate(p, ctx)))
        .unwrap_or_else(|| ctx.working_dir.clone());

    log.info(&format!(
        "[{}] Shell: {} {:?} in {:?}",
        pipeline_ctx.handle_id, interpolated_cmd, interpolated_args, work_dir
    ));

    // If cmd contains spaces and no args, run through shell
    let (actual_cmd, actual_args): (String, Vec<String>) =
        if interpolated_cmd.contains(' ') && interpolated_args.is_empty() {
            (
                "/bin/sh".to_string(),
                vec!["-c".to_string(), interpolated_cmd],
            )
        } else {
            (interpolated_cmd, interpolated_args)
        };

    let mut command = Command::new(&actual_cmd);
    command
        .args(&actual_args)
        .current_dir(&work_dir);

    // Set interpolated environment variables on the child process
    if let Some(env_map) = env {
        for (key, value) in env_map {
            let interpolated_value = interpolation::interpolate(value, ctx);
            command.env(key, &interpolated_value);
        }
    }

    // owned (continuum_process::run_owned): at the bound, or when the pipeline is dropped, the step's
    // command and everything it started are ended; kill_on_drop ended only the direct child
    let output = continuum_process::run_owned(command, Some(Duration::from_secs(timeout_secs)), "sentinel-shell", std::sync::Arc::default()).await;

    let duration_ms = start.elapsed().as_millis() as u64;

    use continuum_process::Ended;
    let output = match output {
        Ok(output) => output,
        Err(e) => return Err(step_err(pipeline_ctx.handle_id, &format!("Shell step failed to execute '{actual_cmd}'"), e)),
    };
    let note = output.capture_note(); // its output isn't whole: said beside the result, never hidden
    let status = match output.ended {
        Ended::Exited(status) => status,
        Ended::TimedOut { ref unconfirmed } => {
            // what it wrote before the bound stays in the error, and teardown it couldn't
            // confirm is named, never claimed complete (Codex root on #5035)
            let mut why = format!("timed out after {timeout_secs}s");
            if !unconfirmed.is_empty() {
                why.push_str(&format!("; not confirmed gone: {unconfirmed:?}"));
            }
            for (name, read) in [("stdout", &output.stdout), ("stderr", &output.stderr)] {
                if !read.bytes.is_empty() {
                    why.push_str(&format!("\n{name} before the bound:\n{}", String::from_utf8_lossy(&read.bytes).trim_end()));
                }
            }
            return Err(step_err(pipeline_ctx.handle_id, "Shell step", why));
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout.bytes).to_string();
    let mut stderr = String::from_utf8_lossy(&output.stderr.bytes).to_string();
    if let Some(note) = note {
        stderr.push_str(&format!("\n[{note}]"));
    }
    let exit_code = status.code().unwrap_or(-1); // unwrap_or: killed by a signal has no exit code; -1 names that
    let success = if allow_failure { true } else { exit_code == 0 };

    Ok(StepResult {
        step_index: index,
        step_type: "shell".to_string(),
        success,
        duration_ms,
        output: Some(stdout.clone()),
        error: if success { None } else { Some(stderr.clone()) },
        exit_code: Some(exit_code),
        data: json!({
            "stdout": stdout,
            "stderr": stderr,
            "exitCode": exit_code,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ExecutionContext;
    use continuum_kernel::runtime::{message_bus::MessageBus, ModuleRegistry};
    use std::collections::HashMap;
    use std::sync::Arc;

    fn test_ctx() -> ExecutionContext {
        ExecutionContext {
            step_results: Vec::new(),
            inputs: HashMap::new(),
            working_dir: PathBuf::from("/tmp"),
            named_outputs: HashMap::new(),
        }
    }

    fn test_pipeline_ctx<'a>(
        registry: &'a Arc<ModuleRegistry>,
        bus: &'a Arc<MessageBus>,
    ) -> PipelineContext<'a> {
        PipelineContext {
            handle_id: "test-001",
            registry,
            bus: Some(bus),
            steps_log_path: None,
            executor: None,
        }
    }

    #[tokio::test]
    async fn test_echo() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let result = execute(
            "echo",
            &["hello".into()],
            10,
            None,
            false,
            0,
            &mut ctx,
            &pipeline_ctx,
            None,
        )
        .await
        .unwrap();
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

        let result = execute(
            "/bin/sh",
            &["-c".into(), "exit 42".into()],
            10,
            None,
            false,
            0,
            &mut ctx,
            &pipeline_ctx,
            None,
        )
        .await
        .unwrap();
        assert!(!result.success);
        assert_eq!(result.exit_code, Some(42));
    }

    #[tokio::test]
    async fn test_allow_failure() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        // With allow_failure, non-zero exit code still marks success=true
        let result = execute(
            "/bin/sh",
            &["-c".into(), "exit 42".into()],
            10,
            None,
            true,
            0,
            &mut ctx,
            &pipeline_ctx,
            None,
        )
        .await
        .unwrap();
        assert!(result.success); // success=true despite exit code 42
        assert_eq!(result.exit_code, Some(42)); // exit code still recorded
        assert_eq!(result.data["exitCode"], 42); // also in data
    }

    #[tokio::test]
    async fn test_shell_passthrough_for_spaces() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        // cmd with spaces and no args should be passed through /bin/sh -c
        let result = execute(
            "echo hello world",
            &[],
            10,
            None,
            false,
            0,
            &mut ctx,
            &pipeline_ctx,
            None,
        )
        .await
        .unwrap();
        assert!(result.success);
        assert_eq!(result.output.as_deref(), Some("hello world\n"));
    }

    #[tokio::test]
    async fn test_timeout() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let result = execute(
            "sleep",
            &["10".into()],
            1,
            None,
            false,
            0,
            &mut ctx,
            &pipeline_ctx,
            None,
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("timed out"));
    }

    #[tokio::test]
    async fn test_invalid_command() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let result = execute(
            "/nonexistent/binary",
            &[],
            10,
            None,
            false,
            0,
            &mut ctx,
            &pipeline_ctx,
            None,
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("failed to execute"));
    }

    #[tokio::test]
    async fn test_cmd_interpolation() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();
        ctx.inputs
            .insert("msg".to_string(), serde_json::json!("interpolated"));

        let result = execute(
            "echo",
            &["{{input.msg}}".into()],
            10,
            None,
            false,
            0,
            &mut ctx,
            &pipeline_ctx,
            None,
        )
        .await
        .unwrap();
        assert!(result.success);
        assert_eq!(result.output.as_deref(), Some("interpolated\n"));
    }

    #[tokio::test]
    async fn test_data_has_stdout_stderr() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        let result = execute(
            "echo",
            &["data-test".into()],
            10,
            None,
            false,
            0,
            &mut ctx,
            &pipeline_ctx,
            None,
        )
        .await
        .unwrap();
        assert_eq!(result.data["stdout"], "data-test\n");
        assert_eq!(result.data["exitCode"], 0);
    }

    #[tokio::test]
    async fn test_env_vars() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        // Pass data via env vars — bypasses shell quoting
        let mut env = HashMap::new();
        env.insert("MY_CODE".to_string(), "print('hello world')".to_string());

        let result = execute(
            "/bin/sh",
            &["-c".into(), "echo $MY_CODE".into()],
            10,
            None,
            false,
            0,
            &mut ctx,
            &pipeline_ctx,
            Some(&env),
        )
        .await
        .unwrap();
        assert!(result.success);
        assert_eq!(result.output.as_deref(), Some("print('hello world')\n"));
    }

    #[tokio::test]
    async fn test_env_vars_with_special_chars() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let pipeline_ctx = test_pipeline_ctx(&registry, &bus);
        let mut ctx = test_ctx();

        // Code with shell metacharacters that would break heredocs
        let mut env = HashMap::new();
        env.insert(
            "CODE".to_string(),
            "assert f'{type(x).__name__}' == 'dict'".to_string(),
        );

        let result = execute(
            "python3",
            &["-c".into(), "import os; print(os.environ['CODE'])".into()],
            10,
            None,
            false,
            0,
            &mut ctx,
            &pipeline_ctx,
            Some(&env),
        )
        .await
        .unwrap();
        assert!(result.success);
        assert!(result
            .output
            .as_deref()
            .unwrap()
            .contains("assert f'{type(x).__name__}'"));
    }
}
