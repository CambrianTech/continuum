/// Messages for SQL Executor Worker
///
/// This worker is a PURE SQL EXECUTOR - it receives complete SQL strings
/// from TypeScript and executes them. TypeScript owns all ORM logic:
/// - Schema generation from decorators
/// - Query building (universal filters → SQL)
/// - Entity serialization/deserialization
/// - Type conversions
///
/// The worker provides:
/// - Fast rusqlite execution
/// - Connection pooling
/// - Concurrent query handling
use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ============================================================================
// Universal Protocol Messages
// ============================================================================

/// Database handle (for multi-database support)
pub type DbHandle = String;

/// Base request structure
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct JTAGRequest<T> {
    pub id: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub timestamp: String,
    pub payload: T,
}

/// Base response structure
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct JTAGResponse<T> {
    pub id: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub timestamp: String,
    pub payload: T,
    pub request_id: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type: Option<JTAGErrorType>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum JTAGErrorType {
    Validation,
    Internal,
    NotFound,
}

impl<T> JTAGResponse<T> {
    pub fn success(request_id: String, msg_type: String, payload: T) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            r#type: msg_type,
            timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            payload,
            request_id,
            success: true,
            error: None,
            error_type: None,
        }
    }

    pub fn error(
        request_id: String,
        msg_type: String,
        payload: T,
        error: String,
        error_type: JTAGErrorType,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            r#type: msg_type,
            timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            payload,
            request_id,
            success: false,
            error: Some(error),
            error_type: Some(error_type),
        }
    }
}

// ============================================================================
// SQL Execution Messages - The ONLY data operations
// ============================================================================

/// Execute SQL query (SELECT) - returns rows
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SqlQueryPayload {
    /// Complete SQL string (built by TypeScript ORM)
    pub sql: String,
    /// Bind parameters (already converted to SQL types by TypeScript)
    #[ts(type = "Array<any>")]
    pub params: Vec<serde_json::Value>,
    /// Optional database path (default: .continuum/jtag/data/database.sqlite)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_path: Option<String>,
    /// Optional database handle (for multi-database routing)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_handle: Option<DbHandle>,
}

/// Result from SQL query - raw rows as JSON
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SqlQueryResult {
    /// Rows as JSON objects (TypeScript will deserialize to entities)
    #[ts(type = "Array<Record<string, any>>")]
    pub rows: Vec<serde_json::Value>,
}

/// Execute SQL statement (INSERT/UPDATE/DELETE) - returns changes count
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SqlExecutePayload {
    /// Complete SQL string (built by TypeScript ORM)
    pub sql: String,
    /// Bind parameters (already converted to SQL types by TypeScript)
    #[ts(type = "Array<any>")]
    pub params: Vec<serde_json::Value>,
    /// Optional database path (default: .continuum/jtag/data/database.sqlite)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_path: Option<String>,
    /// Optional database handle (for multi-database routing)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_handle: Option<DbHandle>,
}

/// Result from SQL statement - rows affected
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SqlExecuteResult {
    /// Number of rows affected
    pub changes: usize,
    /// Last inserted row ID (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_insert_id: Option<i64>,
}

// ============================================================================
// Health Check Messages
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct PingResult {
    pub uptime_ms: u64,
    pub queue_depth: usize,
    pub processed_total: u64,
    pub errors_total: u64,
    pub memory_mb: f64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct StatusPayload {
    pub verbose: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct StatusResult {
    pub uptime_ms: u64,
    pub requests_total: u64,
    pub errors_total: u64,
    pub connections_total: u64,
    pub queue_depth: usize,
    pub memory_mb: f64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ShutdownPayload {
    pub graceful: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ShutdownResult {
    pub queue_drained: usize,
    pub shutdown_time_ms: u64,
}

// ============================================================================
// TypeScript Export Test
// ============================================================================

#[cfg(test)]
mod export_typescript {
    use super::*;

    #[test]
    fn export_bindings() {
        SqlQueryPayload::export().expect("Failed to export SqlQueryPayload");
        SqlQueryResult::export().expect("Failed to export SqlQueryResult");
        SqlExecutePayload::export().expect("Failed to export SqlExecutePayload");
        SqlExecuteResult::export().expect("Failed to export SqlExecuteResult");
        PingResult::export().expect("Failed to export PingResult");
        StatusPayload::export().expect("Failed to export StatusPayload");
        StatusResult::export().expect("Failed to export StatusResult");
        ShutdownPayload::export().expect("Failed to export ShutdownPayload");
        ShutdownResult::export().expect("Failed to export ShutdownResult");
        println!("✅ TypeScript bindings exported to bindings/");
    }
}
