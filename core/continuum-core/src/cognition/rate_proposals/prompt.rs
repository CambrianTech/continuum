//! Pure prompt builder for the peer-review rater. Mirrors `buildRatingPrompt`
//! from `system/user/server/modules/cognition/ProposalRatingAdapter.ts`.
//!
//! Pure function — no AI call, no I/O. Same string output as TS for the
//! same input. PR-2 wires this into the IPC handler.

use crate::cognition::rate_proposals::types::RatingContext;

/// Build the rating prompt the AI sees. Output is byte-for-byte identical
/// to the TS `buildRatingPrompt` function so behavior parity is provable
/// against captured TS-side fixtures.
///
/// The format intentionally pins the response shape (PROPOSAL N: / Score:
/// / ShouldPost: / Reasoning:) so the parser in `parser.rs` has stable
/// anchors to extract from. Don't reword without updating both sides.
pub fn build_rating_prompt(context: &RatingContext, reviewer_name: &str) -> String {
    let conversation_history = context
        .recent_messages
        .iter()
        .map(|m| format!("[{}]: {}", m.sender_name, m.content))
        .collect::<Vec<_>>()
        .join("\n");

    let proposals_text = context
        .proposals
        .iter()
        .enumerate()
        .map(|(idx, p)| {
            format!(
                "\nPROPOSAL {} (by {}, confidence: {:.2}):\n\"{}\"\n",
                idx + 1,
                p.proposer_name,
                p.confidence,
                p.response_text,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "You are {reviewer_name}. Multiple AIs (including yourself) have proposed responses to this message. Rate each proposal.\n\
\n\
ORIGINAL MESSAGE (from {orig_sender}):\n\
\"{orig_content}\"\n\
\n\
RECENT CONVERSATION:\n\
{conversation_history}\n\
\n\
ALL PROPOSALS:\n\
{proposals_text}\n\
\n\
RATING CRITERIA:\n\
1. Relevance (0.0-1.0): How relevant is this response to the original question?\n\
2. Quality (0.0-1.0): Is this a high-quality, well-formed response?\n\
3. Redundancy (0.0-1.0): How redundant is this with other proposals? (0=unique, 1=duplicate)\n\
4. Added Value (0.0-1.0): Does this add new information or perspective?\n\
5. Correctness (0.0-1.0): Is this factually correct?\n\
\n\
For each proposal, provide:\n\
- Overall score (0.0-1.0)\n\
- Should this post? (yes/no)\n\
- Brief reasoning\n\
\n\
FORMAT YOUR RESPONSE EXACTLY LIKE THIS:\n\
\n\
PROPOSAL 1:\n\
Score: 0.85\n\
ShouldPost: yes\n\
Reasoning: High quality response with good technical detail, adds unique perspective\n\
\n\
PROPOSAL 2:\n\
Score: 0.60\n\
ShouldPost: no\n\
Reasoning: Redundant with Proposal 1, doesn't add new information\n\
\n\
PROPOSAL 3:\n\
Score: 0.75\n\
ShouldPost: yes\n\
Reasoning: Different approach than Proposal 1, valuable alternative perspective\n\
\n\
Rate honestly - it's OK if multiple proposals should post (quality control, not competition).\n\
It's also OK if NONE should post (all redundant/low quality).\n\
You may rate your own proposal - be objective.",
        reviewer_name = reviewer_name,
        orig_sender = context.original_message.sender_name,
        orig_content = context.original_message.content,
        conversation_history = conversation_history,
        proposals_text = proposals_text,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::rate_proposals::types::{RatingMessage, ResponseProposal};

    fn fixture_ctx() -> RatingContext {
        RatingContext {
            original_message: RatingMessage {
                sender_name: "operator".into(),
                content: "what is the meaning of life?".into(),
                timestamp: 1_700_000_000_000,
            },
            recent_messages: vec![
                RatingMessage {
                    sender_name: "alice".into(),
                    content: "hello everyone".into(),
                    timestamp: 1_699_999_900_000,
                },
                RatingMessage {
                    sender_name: "operator".into(),
                    content: "anyone here philosophical?".into(),
                    timestamp: 1_699_999_950_000,
                },
            ],
            proposals: vec![
                ResponseProposal {
                    proposal_id: "p-1".into(),
                    proposer_name: "alice".into(),
                    response_text: "42, per Adams.".into(),
                    confidence: 0.9,
                },
                ResponseProposal {
                    proposal_id: "p-2".into(),
                    proposer_name: "bob".into(),
                    response_text: "to give meaning to others.".into(),
                    confidence: 0.7,
                },
            ],
        }
    }

    /// What this catches: prompt header + reviewer-name interpolation.
    /// Drift here would change what the AI sees about its own role and
    /// could shift rating behavior.
    #[test]
    fn prompt_starts_with_reviewer_role_header() {
        let ctx = fixture_ctx();
        let p = build_rating_prompt(&ctx, "claude");
        assert!(
            p.starts_with("You are claude. Multiple AIs"),
            "header missing or wrong"
        );
    }

    /// What this catches: original message section quotes the content
    /// verbatim with the sender name. Pin the format because the AI's
    /// "what am I rating against?" anchor depends on it.
    #[test]
    fn prompt_contains_original_message_section() {
        let ctx = fixture_ctx();
        let p = build_rating_prompt(&ctx, "claude");
        assert!(p.contains("ORIGINAL MESSAGE (from operator):"));
        assert!(p.contains("\"what is the meaning of life?\""));
    }

    /// What this catches: each recent-conversation message renders as
    /// `[name]: content` on its own line. The format is what the AI uses
    /// to model conversational state.
    #[test]
    fn prompt_renders_conversation_history_per_message() {
        let ctx = fixture_ctx();
        let p = build_rating_prompt(&ctx, "claude");
        assert!(p.contains("[alice]: hello everyone"));
        assert!(p.contains("[operator]: anyone here philosophical?"));
    }

    /// What this catches: each proposal renders with PROPOSAL N: header,
    /// proposer name, confidence to 2 decimal places, and quoted response
    /// text. The numbering is what the parser will key off — drift here
    /// breaks the parser without surfacing as a build error.
    #[test]
    fn prompt_renders_proposals_with_index_proposer_confidence_quoted_text() {
        let ctx = fixture_ctx();
        let p = build_rating_prompt(&ctx, "claude");
        assert!(p.contains("PROPOSAL 1 (by alice, confidence: 0.90):"));
        assert!(p.contains("\"42, per Adams.\""));
        assert!(p.contains("PROPOSAL 2 (by bob, confidence: 0.70):"));
        assert!(p.contains("\"to give meaning to others.\""));
    }

    /// What this catches: the output-format example block stays intact
    /// (Score: / ShouldPost: / Reasoning:). The parser depends on these
    /// anchors; if the example drifts, the AI's response format drifts,
    /// and the parser silently misses fields.
    #[test]
    fn prompt_pins_output_format_anchors() {
        let ctx = fixture_ctx();
        let p = build_rating_prompt(&ctx, "claude");
        assert!(p.contains("Score: 0.85"));
        assert!(p.contains("ShouldPost: yes"));
        assert!(p.contains("Reasoning: "));
    }

    /// What this catches: empty recent-messages and empty proposals
    /// produce a well-formed prompt (no panic, no malformed sections).
    /// Edge case for first-message-in-room scenarios.
    #[test]
    fn prompt_handles_empty_history_and_proposals() {
        let mut ctx = fixture_ctx();
        ctx.recent_messages.clear();
        ctx.proposals.clear();
        let p = build_rating_prompt(&ctx, "claude");
        assert!(p.contains("RECENT CONVERSATION:\n\n"));
        assert!(p.contains("ALL PROPOSALS:\n\n"));
    }

    /// What this catches: the closing nudges (multiple-may-post + none-may-
    /// post + objectivity) survive verbatim. These shape the AI's
    /// behavior — losing them shifts rating distribution.
    #[test]
    fn prompt_keeps_behavior_nudges() {
        let ctx = fixture_ctx();
        let p = build_rating_prompt(&ctx, "claude");
        assert!(p.contains("Rate honestly"));
        assert!(p.contains("OK if multiple proposals should post"));
        assert!(p.contains("OK if NONE should post"));
        assert!(p.contains("be objective"));
    }
}
