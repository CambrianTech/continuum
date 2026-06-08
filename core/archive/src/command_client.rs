/// Command Client - Rust calls Commands.execute() via TypeScript
///
/// PURPOSE: Non-SQL operations (events, config, session state, etc.)
/// - Data operations: Use DataAdapter (direct SQL or Rust DataDaemon)
/// - Everything else: Use CommandClient (events/emit, config/get, etc.)
///
/// This enables Rust workers to be FIRST-CLASS CITIZENS in the JTAG system.
use crate::messages::{CommandExecutionRequest, CommandExecutionResponse};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

pub struct CommandClient {
    stream: UnixStream,
}

impl CommandClient {
    /// Connect to TypeScript command router (placeholder - needs implementation)
    pub fn connect(_socket_path: &str) -> std::io::Result<Self> {
        // TODO: Implement actual connection to TypeScript command router
        // For now, this is a stub to prove the architecture
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Command client not yet implemented - skeleton only",
        ))
    }

    /// Execute a command via TypeScript Commands.execute()
    ///
    /// This is bidirectional communication:
    /// Rust → TypeScript: "Execute data/list for me"
    /// TypeScript → Rust: "Here are the results"
    pub fn execute(&mut self, command: &str, params: Value) -> std::io::Result<Value> {
        // Generate request ID
        let request_id = uuid::Uuid::new_v4().to_string();

        // Create request
        let request = CommandExecutionRequest {
            request_id: request_id.clone(),
            command: command.to_string(),
            params,
        };

        // Send request as JSON line
        let json = serde_json::to_string(&request)?;
        writeln!(self.stream, "{}", json)?;
        self.stream.flush()?;

        // Read response
        let mut reader = BufReader::new(&self.stream);
        let mut response_line = String::new();
        reader.read_line(&mut response_line)?;

        // Parse response
        let response: CommandExecutionResponse = serde_json::from_str(&response_line)?;

        // Verify request ID matches
        if response.request_id != request_id {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "Request ID mismatch: expected {}, got {}",
                    request_id, response.request_id
                ),
            ));
        }

        // Check for errors
        if !response.success {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                response.error.unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        // Return result
        Ok(response.result.unwrap_or(Value::Null))
    }
}

// ============================================================================
// Helper Functions (Non-SQL Operations)
// ============================================================================

impl CommandClient {
    /// Emit an event via events/emit
    pub fn emit_event(&mut self, event: &str, data: Value) -> std::io::Result<Value> {
        self.execute(
            "events/emit",
            serde_json::json!({
                "event": event,
                "data": data
            }),
        )
    }

    /// Get configuration value
    pub fn get_config(&mut self, key: &str) -> std::io::Result<Value> {
        self.execute(
            "config/get",
            serde_json::json!({
                "key": key
            }),
        )
    }

    /// Get session state
    pub fn get_session(&mut self, session_id: &str) -> std::io::Result<Value> {
        self.execute(
            "session/get",
            serde_json::json!({
                "sessionId": session_id
            }),
        )
    }

    /// Ping system (health check)
    pub fn ping(&mut self) -> std::io::Result<Value> {
        self.execute("ping", serde_json::json!({}))
    }
}
