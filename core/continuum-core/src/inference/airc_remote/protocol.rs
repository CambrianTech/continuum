//! Wire types for remote inference over airc.
//!
//! These are the typed envelopes that flow through
//! `AircInferenceTransport`. Both directions serialize via serde; the
//! transport (production impl is task #108 follow-up) frames them
//! into airc events with a routing header.
//!
//! ts-rs exports let TypeScript consumers (and the eventual
//! airc-side handler) share the same shapes without hand-written
//! duplicate types.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::ai::types::{TextGenerationRequest, TextGenerationResponse};

/// One inference request from the requester to a remote peer.
///
/// Includes:
/// - `correlation_id` — a freshly-minted UUID the transport uses to
///   pair the response to this request. Required because the
///   transport may multiplex many requests across one airc
///   connection.
/// - `text_request` — the substrate's canonical inference request
///   (same type local adapters take).
/// - `target_peer` — optional explicit peer hint. None = let the
///   transport / scheduler pick a peer with capacity. Set explicitly
///   when the substrate has reason (persona stickiness, model
///   preference, capability filter).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc_remote/RemoteInferenceRequest.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct RemoteInferenceRequest {
    #[ts(type = "string")]
    pub correlation_id: Uuid,
    pub text_request: TextGenerationRequest,
    /// Optional explicit peer the requester wants. Stringified peer
    /// id; the transport resolves it. None = transport / scheduler
    /// picks based on capacity + capability.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub target_peer: Option<String>,
}

impl RemoteInferenceRequest {
    /// Construct with a fresh correlation_id. Caller supplies the
    /// text request; transport pickier callers set `target_peer`
    /// via the builder method after.
    pub fn new(text_request: TextGenerationRequest) -> Self {
        Self {
            correlation_id: Uuid::new_v4(),
            text_request,
            target_peer: None,
        }
    }

    pub fn with_target_peer(mut self, peer: impl Into<String>) -> Self {
        self.target_peer = Some(peer.into());
        self
    }
}

/// One inference response from the remote peer back to the
/// requester. Correlation_id matches the request that produced it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc_remote/RemoteInferenceResponse.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct RemoteInferenceResponse {
    #[ts(type = "string")]
    pub correlation_id: Uuid,
    /// The peer's own peer_id (stringified). Lets the requester
    /// confirm which peer actually served the request — useful when
    /// the transport's peer-pick logic isn't deterministic.
    pub served_by: String,
    /// The peer's inference produced this. Local adapter trait
    /// shape, fully populated. When the peer errored, this is
    /// surfaced via `RemoteInferenceError` from the transport;
    /// when the peer responded with a typed-but-failed result
    /// (e.g. cloud rate limit), the error field on the response
    /// carries it.
    pub text_response: TextGenerationResponse,
}

/// Errors specific to the remote inference transport layer.
/// Distinct from `TextGenerationResponse.error` (which is the
/// model's own error) — these are transport / discovery /
/// correlation failures the substrate-as-transport detected.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc_remote/RemoteInferenceError.ts"
)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RemoteInferenceError {
    /// Transport (airc) refused or failed mid-send. Wraps the
    /// underlying message; the substrate's grid-discovery layer
    /// should respond by re-routing or re-queueing.
    Transport { message: String },
    /// Discovery couldn't find a reachable peer. Typically retried
    /// after a substrate backoff window.
    NoPeerReachable { message: String },
    /// Transport sent the request but no response arrived before
    /// the timeout. Coordinator decides to retry / fall back to
    /// local heuristic / surface to caller.
    Timeout { elapsed_ms: u64 },
    /// Response arrived but its correlation_id doesn't match any
    /// outstanding request. Substrate bug — transport's pairing
    /// logic broke. Caller surfaces; substrate logs loudly.
    CorrelationMismatch {
        expected: String,
        actual: String,
    },
    /// Adapter-level failure on the peer side (the peer's local
    /// adapter returned an error). Wraps the peer's error string
    /// so the requester can decide whether to retry or surface.
    PeerAdapterFailed { message: String },
    /// The substrate's policy denied the request (e.g. persona
    /// not authorized on this peer per
    /// [[personas-are-citizens-airc-is-identity-provider]],
    /// quota exceeded, target peer not accepting remote inference).
    PolicyDenied { reason: String },
}

impl std::fmt::Display for RemoteInferenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport { message } => write!(f, "remote inference transport: {message}"),
            Self::NoPeerReachable { message } => {
                write!(f, "no remote peer reachable for inference: {message}")
            }
            Self::Timeout { elapsed_ms } => {
                write!(f, "remote inference timed out after {elapsed_ms}ms")
            }
            Self::CorrelationMismatch { expected, actual } => write!(
                f,
                "remote inference correlation mismatch (expected {expected}, got {actual})"
            ),
            Self::PeerAdapterFailed { message } => {
                write!(f, "remote peer's adapter failed: {message}")
            }
            Self::PolicyDenied { reason } => {
                write!(f, "remote inference policy denied: {reason}")
            }
        }
    }
}

impl std::error::Error for RemoteInferenceError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::types::{ChatMessage, MessageContent, TextGenerationRequest};

    fn dummy_request() -> TextGenerationRequest {
        TextGenerationRequest {
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text("hello".to_string()),
                name: None,
            }],
            system_prompt: None,
            model: None,
            provider: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            top_k: None,
            repeat_penalty: None,
            frequency_penalty: None,
            repeat_last_n: None,
            stop_sequences: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            active_adapters: None,
            request_id: None,
            user_id: None,
            room_id: None,
            purpose: None,
            persona_id: None,
        }
    }

    #[test]
    fn new_request_assigns_fresh_correlation_id_each_time() {
        let r1 = RemoteInferenceRequest::new(dummy_request());
        let r2 = RemoteInferenceRequest::new(dummy_request());
        assert_ne!(r1.correlation_id, r2.correlation_id);
    }

    #[test]
    fn new_request_defaults_target_peer_to_none() {
        let r = RemoteInferenceRequest::new(dummy_request());
        assert!(r.target_peer.is_none());
    }

    #[test]
    fn with_target_peer_sets_the_field() {
        let r = RemoteInferenceRequest::new(dummy_request()).with_target_peer("peer-abc");
        assert_eq!(r.target_peer.as_deref(), Some("peer-abc"));
    }

    #[test]
    fn request_serializes_and_round_trips() {
        let r = RemoteInferenceRequest::new(dummy_request()).with_target_peer("peer-abc");
        let json = serde_json::to_string(&r).unwrap();
        let back: RemoteInferenceRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.correlation_id, r.correlation_id);
        assert_eq!(back.target_peer.as_deref(), Some("peer-abc"));
    }

    // ── error variants ──────────────────────────────────────────

    #[test]
    fn error_display_is_human_readable() {
        let cases = vec![
            RemoteInferenceError::Transport {
                message: "socket closed".to_string(),
            },
            RemoteInferenceError::NoPeerReachable {
                message: "all peers down".to_string(),
            },
            RemoteInferenceError::Timeout { elapsed_ms: 5_000 },
            RemoteInferenceError::PeerAdapterFailed {
                message: "OOM".to_string(),
            },
            RemoteInferenceError::PolicyDenied {
                reason: "persona scope".to_string(),
            },
        ];
        for err in cases {
            let s = err.to_string();
            assert!(!s.is_empty());
            // Each carries the descriptive prefix.
            assert!(s.contains("remote") || s.contains("no remote") || s.contains("policy"));
        }
    }

    #[test]
    fn error_correlation_mismatch_displays_both_ids() {
        let err = RemoteInferenceError::CorrelationMismatch {
            expected: "uuid-A".to_string(),
            actual: "uuid-B".to_string(),
        };
        let s = err.to_string();
        assert!(s.contains("uuid-A"));
        assert!(s.contains("uuid-B"));
    }

    #[test]
    fn errors_round_trip_via_serde() {
        let original = RemoteInferenceError::Timeout { elapsed_ms: 1234 };
        let json = serde_json::to_string(&original).unwrap();
        let back: RemoteInferenceError = serde_json::from_str(&json).unwrap();
        assert_eq!(original, back);
    }
}
