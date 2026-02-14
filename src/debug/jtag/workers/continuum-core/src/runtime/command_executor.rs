//! CommandExecutor — Universal command execution for ALL continuum-core processes
//!
//! This is the foundational primitive that allows ANY spawned task (sentinels,
//! background jobs, etc.) to execute ANY command in the system, regardless of
//! whether it's implemented in Rust or TypeScript.
//!
//! Usage:
//! ```rust
//! let executor = CommandExecutor::new(registry.clone(), ws_url);
//!
//! // Works for Rust modules
//! executor.execute("ai/generate", params).await?;
//!
//! // Works for TypeScript commands
//! executor.execute("code/edit", params).await?;
//!
//! // Sentinel doesn't know or care where command is implemented
//! ```

use std::sync::Arc;
use serde_json::Value;
use tokio::sync::RwLock;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use super::{ModuleRegistry, CommandResult};

/// Universal command executor that routes to Rust modules or TypeScript
pub struct CommandExecutor {
    /// Rust module registry (for Rust-implemented commands)
    registry: Arc<ModuleRegistry>,

    /// WebSocket URL for TypeScript commands (e.g., "ws://localhost:9001")
    ts_ws_url: String,

    /// Cached WebSocket connection to TypeScript server
    ws_connection: RwLock<Option<WsConnection>>,
}

struct WsConnection {
    // We'll use a simple request-response pattern
    // Each command gets a unique ID, we wait for the response
}

impl CommandExecutor {
    pub fn new(registry: Arc<ModuleRegistry>, ts_ws_url: &str) -> Self {
        Self {
            registry,
            ts_ws_url: ts_ws_url.to_string(),
            ws_connection: RwLock::new(None),
        }
    }

    /// Execute ANY command - routes to Rust or TypeScript automatically
    /// Returns CommandResult for consistency with ServiceModule pattern
    pub async fn execute(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        // 1. Try Rust module registry first
        if let Some((module, cmd)) = self.registry.route_command(command) {
            return module.handle_command(&cmd, params).await;
        }

        // 2. Route to TypeScript via WebSocket
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

    /// Execute command via TypeScript WebSocket bridge
    async fn execute_ts_command(&self, command: &str, params: Value) -> Result<Value, String> {
        use tokio_tungstenite::tungstenite::protocol::Message;

        // Connect to TypeScript WebSocket server
        let url = format!("{}/ws", self.ts_ws_url);
        let (ws_stream, _) = connect_async(&url)
            .await
            .map_err(|e| format!("Failed to connect to TS server: {}", e))?;

        let (mut write, mut read) = ws_stream.split();

        // Build command request
        let request_id = uuid::Uuid::new_v4().to_string();
        let request = serde_json::json!({
            "type": "command",
            "requestId": request_id,
            "command": command,
            "params": params,
        });

        // Send command
        write.send(Message::Text(request.to_string()))
            .await
            .map_err(|e| format!("Failed to send command: {}", e))?;

        // Wait for response
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    let response: Value = serde_json::from_str(&text)
                        .map_err(|e| format!("Invalid response JSON: {}", e))?;

                    // Check if this is our response
                    if response.get("requestId").and_then(|v| v.as_str()) == Some(&request_id) {
                        if response.get("success").and_then(|v| v.as_bool()) == Some(true) {
                            return Ok(response.get("data").cloned().unwrap_or(Value::Null));
                        } else {
                            let error = response.get("error")
                                .and_then(|v| v.as_str())
                                .unwrap_or("Unknown error");
                            return Err(error.to_string());
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    return Err("WebSocket closed unexpectedly".to_string());
                }
                Err(e) => {
                    return Err(format!("WebSocket error: {}", e));
                }
                _ => continue,
            }
        }

        Err("No response received from TypeScript server".to_string())
    }
}

// Global executor instance - initialized once at startup
static GLOBAL_EXECUTOR: std::sync::OnceLock<Arc<CommandExecutor>> = std::sync::OnceLock::new();

/// Initialize the global command executor (called once at startup)
pub fn init_executor(registry: Arc<ModuleRegistry>, ts_ws_url: &str) {
    let _ = GLOBAL_EXECUTOR.set(Arc::new(CommandExecutor::new(registry, ts_ws_url)));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_executor_creation() {
        let registry = Arc::new(ModuleRegistry::new());
        let executor = CommandExecutor::new(registry, "ws://localhost:9001");
        assert_eq!(executor.ts_ws_url, "ws://localhost:9001");
    }
}
