use crate::airc::types::AircQueueScanErrorKind;
use async_trait::async_trait;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command as TokioCommand;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AircInvocation {
    pub program: String,
    pub args: Vec<String>,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AircCommandOutput {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AircCommandError {
    pub kind: AircQueueScanErrorKind,
    pub message: String,
}

#[async_trait]
pub trait AircCommandRunner: Send + Sync {
    async fn run(&self, invocation: AircInvocation) -> Result<AircCommandOutput, AircCommandError>;
}

#[derive(Debug, Default, Clone)]
pub struct TokioAircCommandRunner;

#[async_trait]
impl AircCommandRunner for TokioAircCommandRunner {
    async fn run(&self, invocation: AircInvocation) -> Result<AircCommandOutput, AircCommandError> {
        let mut command = TokioCommand::new(&invocation.program);
        command
            .args(&invocation.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = match tokio::time::timeout(
            Duration::from_millis(invocation.timeout_ms),
            command.output(),
        )
        .await
        {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                return Err(AircCommandError {
                    kind: AircQueueScanErrorKind::SpawnFailed,
                    message: format!("failed to spawn airc: {e}"),
                });
            }
            Err(_) => {
                return Err(AircCommandError {
                    kind: AircQueueScanErrorKind::TimedOut,
                    message: format!("timed out after {}ms", invocation.timeout_ms),
                });
            }
        };

        Ok(AircCommandOutput {
            success: output.status.success(),
            exit_code: output.status.code(),
            stdout: output.stdout,
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}
