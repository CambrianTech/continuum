//! CommandExecutor — Universal command execution for ALL continuum-core processes
//!
//! This is the foundational primitive that allows ANY spawned task (sentinels,
//! background jobs, etc.) to execute ANY command in the system, regardless of
//! whether it's implemented in Rust or TypeScript.
//!
//! Usage:
//! ```rust
//! // Works for Rust modules
//! runtime::execute_command_json("health-check", json!({})).await?;
//!
//! // Works for TypeScript commands (via CommandRouterServer)
//! runtime::execute_command_json("screenshot", json!({"querySelector": "body"})).await?;
//!
//! // Sentinel doesn't know or care where command is implemented
//! ```
//!
//! Architecture:
//! - Rust modules: Routed directly through ModuleRegistry
//! - TypeScript commands: Routed via Unix socket to CommandRouterServer
//!   (socket: /tmp/jtag-command-router.sock)

use std::sync::Arc;
use serde_json::Value;
use tokio::net::UnixStream;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::{ModuleRegistry, CommandResult};

/// Socket path for TypeScript command routing
const TS_COMMAND_SOCKET: &str = "/tmp/jtag-command-router.sock";

/// Universal command executor that routes to Rust modules or TypeScript
pub struct CommandExecutor {
    /// Rust module registry (for Rust-implemented commands)
    registry: Arc<ModuleRegistry>,
}

impl CommandExecutor {
    pub fn new(registry: Arc<ModuleRegistry>) -> Self {
        Self { registry }
    }

    /// Execute ANY command - routes to Rust or TypeScript automatically
    /// Returns CommandResult for consistency with ServiceModule pattern
    pub async fn execute(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        let log = super::logger("command-executor");

        // 1. Try Rust module registry first
        if let Some((module, cmd)) = self.registry.route_command(command) {
            log.debug(&format!("Routing '{}' to Rust module", command));
            return module.handle_command(&cmd, params).await;
        }

        // 2. Route to TypeScript via Unix socket (CommandRouterServer)
        log.debug(&format!("Routing '{}' to TypeScript via CommandRouterServer", command));
        let json = self.execute_ts_command(command, params).await?;
        Ok(CommandResult::Json(json))
    }

    /// Convenience: execute and extract JSON directly
    pub async fn execute_json(&self, command: &str, params: Value) -> Result<Value, String> {
        match self.execute(command, params).await? {
            CommandResult::Json(v) => Ok(v),
            CommandResult::Binary { metadata, .. } => Ok(metadata),
        }
    }

    /// Execute a command ONLY via TypeScript (bypasses Rust registry).
    /// Use this when a Rust module needs to forward to a TypeScript-implemented
    /// command that shares the same prefix (avoids infinite recursion).
    pub async fn execute_ts(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        let json = self.execute_ts_command(command, params).await?;
        Ok(CommandResult::Json(json))
    }

    /// Convenience: execute via TypeScript only and extract JSON directly
    pub async fn execute_ts_json(&self, command: &str, params: Value) -> Result<Value, String> {
        self.execute_ts_command(command, params).await
    }

    /// Execute command via TypeScript CommandRouterServer (Unix socket)
    ///
    /// Protocol:
    /// - Request: `{"command": "...", "params": {...}}\n`
    /// - Response: `{"success": true, "result": ...}\n` or `{"success": false, "error": "..."}\n`
    async fn execute_ts_command(&self, command: &str, params: Value) -> Result<Value, String> {
        let log = super::logger("command-executor");

        // Connect to CommandRouterServer
        log.debug(&format!("Connecting to TypeScript socket: {}", TS_COMMAND_SOCKET));
        let stream = UnixStream::connect(TS_COMMAND_SOCKET)
            .await
            .map_err(|e| format!("Failed to connect to CommandRouterServer at {}: {}", TS_COMMAND_SOCKET, e))?;

        let (reader, mut writer) = stream.into_split();
        let mut buf_reader = BufReader::new(reader);

        // Build and send request
        let request = serde_json::json!({
            "command": command,
            "params": params,
        });
        let request_line = format!("{}\n", request.to_string());

        log.debug(&format!("Sending: {}", command));
        writer.write_all(request_line.as_bytes())
            .await
            .map_err(|e| format!("Failed to send command: {}", e))?;
        writer.flush()
            .await
            .map_err(|e| format!("Failed to flush: {}", e))?;

        // Read response
        let mut response_line = String::new();
        buf_reader.read_line(&mut response_line)
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        log.debug(&format!("Received response: {} bytes", response_line.len()));

        // Parse response
        let response: Value = serde_json::from_str(&response_line)
            .map_err(|e| format!("Invalid response JSON: {} (raw: {})", e, response_line.trim()))?;

        // Check success
        if response.get("success").and_then(|v| v.as_bool()) == Some(true) {
            let result = response.get("result").cloned().unwrap_or(Value::Null);
            log.info(&format!("Command '{}' succeeded", command));
            Ok(result)
        } else {
            let error = response.get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error from TypeScript");
            log.error(&format!("Command '{}' failed: {}", command, error));
            Err(error.to_string())
        }
    }
}

// Global executor instance - initialized once at startup
static GLOBAL_EXECUTOR: std::sync::OnceLock<Arc<CommandExecutor>> = std::sync::OnceLock::new();

/// Initialize the global command executor (called once at startup)
pub fn init_executor(registry: Arc<ModuleRegistry>) {
    let log = super::logger("command-executor");
    let _ = GLOBAL_EXECUTOR.set(Arc::new(CommandExecutor::new(registry)));
    log.info(&format!("Initialized (TS bridge: {})", TS_COMMAND_SOCKET));
}

/// Get the global command executor
/// Panics if not initialized - this is intentional, executor MUST be initialized at startup
pub fn executor() -> Arc<CommandExecutor> {
    GLOBAL_EXECUTOR.get()
        .expect("CommandExecutor not initialized - call init_executor() at startup")
        .clone()
}

/// Execute a command from anywhere, returning CommandResult
///
/// Usage:
/// ```rust
/// use crate::runtime::command_executor;
///
/// let result = command_executor::execute("code/edit", params).await?;
/// ```
pub async fn execute(command: &str, params: Value) -> Result<CommandResult, String> {
    executor().execute(command, params).await
}

/// Execute a command and extract JSON result (convenience for most use cases)
pub async fn execute_json(command: &str, params: Value) -> Result<Value, String> {
    executor().execute_json(command, params).await
}

/// Execute a command ONLY via TypeScript, bypassing Rust registry.
/// Use when a Rust module needs to forward to a TypeScript command
/// that shares the same prefix (e.g., ai_provider forwarding ai/agent).
pub async fn execute_ts(command: &str, params: Value) -> Result<CommandResult, String> {
    executor().execute_ts(command, params).await
}

/// Execute via TypeScript only and extract JSON (convenience)
pub async fn execute_ts_json(command: &str, params: Value) -> Result<Value, String> {
    executor().execute_ts_json(command, params).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_executor_creation() {
        let registry = Arc::new(ModuleRegistry::new());
        let _executor = CommandExecutor::new(registry);
        // Just verify it compiles and can be created
    }
}
