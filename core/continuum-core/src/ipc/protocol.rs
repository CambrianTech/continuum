//! IPC protocol types — request/response surface shared by every command.
//!
//! Split out of `ipc/mod.rs` (was 1288 LOC single-file dir, parallel-dir
//! smell flagged in claude-tab-1's audit broadcast 2026-05-18 19:40Z).
//! Per Joel's zero-users no-migration-ceremony directive, no separate
//! re-export ceremony — `ipc/mod.rs` `pub use`s these types so existing
//! call sites resolve unchanged.

use serde::{Deserialize, Serialize};

// All commands route through ServiceModule implementations in src/modules/.

/// Wire response for every command. `request_id` round-trips to let
/// the TS client correlate concurrent requests.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Response {
    pub(crate) success: bool,
    pub(crate) result: Option<serde_json::Value>,
    pub(crate) error: Option<String>,
    #[serde(rename = "requestId")]
    pub(crate) request_id: Option<u64>,
}

impl Response {
    pub(crate) fn success(result: serde_json::Value) -> Self {
        Self {
            success: true,
            result: Some(result),
            error: None,
            request_id: None,
        }
    }

    pub(crate) fn error(msg: String) -> Self {
        Self {
            success: false,
            result: None,
            error: Some(msg),
            request_id: None,
        }
    }

    pub(crate) fn with_request_id(mut self, request_id: Option<u64>) -> Self {
        self.request_id = request_id;
        self
    }
}
