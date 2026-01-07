/// Chat Drain Worker - Message Types using JTAGProtocol
///
/// This uses the universal JTAGProtocol from workers/shared/jtag_protocol.rs
/// which mirrors shared/ipc/JTAGProtocol.ts on the TypeScript side.
use serde::{Deserialize, Serialize};
use ts_rs::TS;

// Import shared JTAGProtocol types
#[path = "../../shared/jtag_protocol.rs"]
mod jtag_protocol;

// Re-export JTAG protocol types for library users
pub use jtag_protocol::{JTAGErrorType, JTAGRequest, JTAGResponse};

// ============================================================================
// Chat-Specific Types (owned by chat-drain worker)
// ============================================================================

/// Chat message payload for processing
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessagePayload {
    pub room_id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "any", optional)]
    pub metadata: Option<serde_json::Value>,
}

/// Chat processing result
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ChatProcessResult {
    pub message_id: String,
    pub processed_at: u64,
    pub personas_notified: usize,
}

// ============================================================================
// TypeScript Export Test
// ============================================================================

#[cfg(test)]
mod export_typescript {
    use super::*;

    #[test]
    fn export_bindings() {
        ChatMessagePayload::export().expect("Failed to export ChatMessagePayload");
        ChatProcessResult::export().expect("Failed to export ChatProcessResult");
        println!("✅ TypeScript bindings exported to bindings/");
    }
}
