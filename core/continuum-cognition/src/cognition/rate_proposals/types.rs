//! Wire types for `cognition/rate-proposals`. ts-rs exports keep TS in sync.
//!
//! Mirror of the TS types in `system/user/server/modules/cognition/PeerReviewTypes.ts`
//! (ResponseProposal, ProposalRating) and the local `RatingContext` from
//! `ProposalRatingAdapter.ts`. ts-rs handles the camelCase wire format on
//! both sides; UUIDs serialize as strings.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One message in the recent-conversation context the rater sees.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/RatingMessage.ts"
)]
pub struct RatingMessage {
    pub sender_name: String,
    pub content: String,
    /// Unix milliseconds.
    #[ts(type = "number")]
    pub timestamp: i64,
}

/// One proposed response competing in a peer-review pass.
///
/// Mirror of TS `ResponseProposal` from PeerReviewTypes.ts. The TS version
/// has more fields (proposer_id, room_id, etc.) but the rater only consumes
/// the fields here; carrying extras through Rust would couple this slice to
/// fields it doesn't use. PR-2's IPC contract will accept the full
/// `ResponseProposal` from TS and project to this rater-shape internally.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ResponseProposal.ts"
)]
pub struct ResponseProposal {
    pub proposal_id: String,
    pub proposer_name: String,
    pub response_text: String,
    /// 0.0..1.0 — how confident the proposer is in this response.
    pub confidence: f64,
}

/// The original message + recent conversation + competing proposals the
/// rater needs to score. Pure data; no behavior.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/RatingContext.ts"
)]
pub struct RatingContext {
    pub original_message: RatingMessage,
    pub recent_messages: Vec<RatingMessage>,
    pub proposals: Vec<ResponseProposal>,
}

/// One rater's score for one proposal. Mirror of TS `ProposalRating` from
/// PeerReviewTypes.ts (rater-side fields only — full ProposalRating in TS
/// adds rating_id/rated_at which the IPC layer fills in PR-2).
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ProposalRating.ts"
)]
pub struct ProposalRating {
    pub proposal_id: String,
    /// 0.0..1.0 — clamped during parsing.
    pub score: f64,
    pub should_post: bool,
    pub reasoning: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: serde camelCase round-trip preserves field
    /// names. The TS shim that calls `Commands.execute` with these
    /// shapes reads `senderName` not `sender_name`; drift here would
    /// silently break the IPC contract.
    #[test]
    fn rating_message_serde_camelcase() {
        let m = RatingMessage {
            sender_name: "alice".into(),
            content: "hi".into(),
            timestamp: 1_700_000_000_000,
        };
        let j = serde_json::to_string(&m).unwrap();
        assert!(j.contains("\"senderName\":\"alice\""), "got: {j}");
        assert!(j.contains("\"timestamp\":1700000000000"), "got: {j}");
        let back: RatingMessage = serde_json::from_str(&j).unwrap();
        assert_eq!(back, m);
    }

    /// What this catches: ResponseProposal field names match TS exactly.
    /// Particularly proposer_name → proposerName and response_text →
    /// responseText (the prompt builder reads these for proposal display).
    #[test]
    fn response_proposal_serde_camelcase() {
        let p = ResponseProposal {
            proposal_id: "p-1".into(),
            proposer_name: "bob".into(),
            response_text: "the answer is 42".into(),
            confidence: 0.85,
        };
        let j = serde_json::to_string(&p).unwrap();
        assert!(j.contains("\"proposalId\":\"p-1\""));
        assert!(j.contains("\"proposerName\":\"bob\""));
        assert!(j.contains("\"responseText\":\"the answer is 42\""));
        assert!(j.contains("\"confidence\":0.85"));
        let back: ResponseProposal = serde_json::from_str(&j).unwrap();
        assert_eq!(back, p);
    }

    /// What this catches: ProposalRating wire format matches the TS
    /// consumer. Drift on `shouldPost` (camelCase) would mean every
    /// rating round-trip flips to `should_post: false` silently because
    /// the TS deserializer wouldn't find `should_post`.
    #[test]
    fn proposal_rating_serde_camelcase() {
        let r = ProposalRating {
            proposal_id: "p-1".into(),
            score: 0.75,
            should_post: true,
            reasoning: "good answer".into(),
        };
        let j = serde_json::to_string(&r).unwrap();
        assert!(j.contains("\"proposalId\":\"p-1\""));
        assert!(j.contains("\"shouldPost\":true"));
        let back: ProposalRating = serde_json::from_str(&j).unwrap();
        assert_eq!(back, r);
    }
}
