//! Pure response parser for the peer-review rater. Mirrors
//! `parseRatingsFromAIResponse` from
//! `system/user/server/modules/cognition/ProposalRatingAdapter.ts`.
//!
//! Pure function — no AI call, no I/O. Same fallback semantics as TS:
//! score parse-fail defaults to 0.5 (neutral), shouldPost parse-fail
//! defaults to false (conservative), reasoning parse-fail defaults to
//! "No reasoning provided". When the AI returns fewer ratings than
//! proposals, missing positions get the same defaults so callers always
//! receive `proposals.len()` ratings.

use crate::cognition::rate_proposals::types::{ProposalRating, ResponseProposal};
use regex::Regex;

/// Configuration knobs for the parser. Defaults match the TS behavior so
/// migration consumers get byte-identical fallback semantics.
#[derive(Debug, Clone)]
pub struct ParseConfig {
    /// Score returned when the `Score:` line is missing or unparseable.
    /// Default 0.5 — neutral, matching TS.
    pub default_score: f64,
    /// `shouldPost` returned when the line is missing or unparseable.
    /// Default false — conservative, matching TS.
    pub default_should_post: bool,
    /// Reasoning string when the `Reasoning:` line is missing.
    /// Default "No reasoning provided" — matches TS.
    pub default_reasoning: String,
    /// Reasoning string for the per-proposal default when the AI returned
    /// fewer ratings than proposals (one of the most common failure
    /// modes). Default "Parse error - default rating applied" — matches TS.
    pub missing_rating_reasoning: String,
}

impl Default for ParseConfig {
    fn default() -> Self {
        Self {
            default_score: 0.5,
            default_should_post: false,
            default_reasoning: "No reasoning provided".to_string(),
            missing_rating_reasoning: "Parse error - default rating applied".to_string(),
        }
    }
}

/// Parse the AI's free-text rating response into typed `ProposalRating`s.
///
/// Always returns exactly `proposals.len()` ratings; positions the AI
/// didn't cover get filled with the `missing_rating_reasoning` default.
///
/// Section split is `PROPOSAL N:` (case-insensitive) — same as TS. The
/// first split chunk before any PROPOSAL marker is discarded (TS
/// `.split(...).slice(1)`).
pub fn parse_ratings_from_ai_response(
    response_text: &str,
    proposals: &[ResponseProposal],
    config: &ParseConfig,
) -> Vec<ProposalRating> {
    let mut ratings: Vec<ProposalRating> = Vec::with_capacity(proposals.len());

    // Split on `PROPOSAL N:` markers (case-insensitive). Drop the first
    // segment (preamble before the first PROPOSAL marker, often empty).
    let split_re = Regex::new(r"(?i)PROPOSAL\s+\d+:").expect("static regex");
    let sections: Vec<&str> = split_re.split(response_text).skip(1).collect();

    let take_n = sections.len().min(proposals.len());
    for i in 0..take_n {
        let section = sections[i];
        let proposal = &proposals[i];
        ratings.push(parse_one_section(section, proposal, config));
    }

    // Fill missing positions (AI returned fewer ratings than proposals).
    for proposal in proposals.iter().skip(ratings.len()) {
        ratings.push(ProposalRating {
            proposal_id: proposal.proposal_id.clone(),
            score: config.default_score,
            should_post: config.default_should_post,
            reasoning: config.missing_rating_reasoning.clone(),
        });
    }

    ratings
}

fn parse_one_section(
    section: &str,
    proposal: &ResponseProposal,
    config: &ParseConfig,
) -> ProposalRating {
    // Score: floating-point, clamped to [0, 1] per TS.
    let score_re = Regex::new(r"(?i)Score:\s*([0-9.]+)").expect("static regex");
    let score = score_re
        .captures(section)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<f64>().ok())
        .unwrap_or(config.default_score)
        .clamp(0.0, 1.0);

    // ShouldPost: yes/no, case-insensitive.
    let should_post_re = Regex::new(r"(?i)ShouldPost:\s*(yes|no)").expect("static regex");
    let should_post = should_post_re
        .captures(section)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().eq_ignore_ascii_case("yes"))
        .unwrap_or(config.default_should_post);

    // Reasoning: text after `Reasoning:` up to the next blank line OR
    // end of section. The `regex` crate doesn't support lookahead, so
    // do this in two stages: locate the Reasoning: marker, then take
    // until the first `\n\n` (or end). Mirrors TS
    // `/Reasoning:\s*(.+?)(?=\n\n|$)/is` semantics.
    let reasoning_re = Regex::new(r"(?i)Reasoning:\s*").expect("static regex");
    let reasoning = reasoning_re
        .find(section)
        .map(|m| {
            let after = &section[m.end()..];
            let end = after.find("\n\n").unwrap_or(after.len());
            after[..end].trim().to_string()
        })
        .unwrap_or_else(|| config.default_reasoning.clone());

    ProposalRating {
        proposal_id: proposal.proposal_id.clone(),
        score,
        should_post,
        reasoning,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(id: &str, name: &str) -> ResponseProposal {
        ResponseProposal {
            proposal_id: id.to_string(),
            proposer_name: name.to_string(),
            response_text: "irrelevant for parser tests".to_string(),
            confidence: 0.5,
        }
    }

    /// What this catches: happy-path well-formed AI response. Three
    /// proposals, three sections, all fields parse correctly.
    #[test]
    fn parses_well_formed_three_proposal_response() {
        let proposals = vec![p("p-1", "alice"), p("p-2", "bob"), p("p-3", "carol")];
        let response = "\
Some preamble the AI wrote.

PROPOSAL 1:
Score: 0.85
ShouldPost: yes
Reasoning: High quality response with good technical detail

PROPOSAL 2:
Score: 0.60
ShouldPost: no
Reasoning: Redundant with Proposal 1

PROPOSAL 3:
Score: 0.75
ShouldPost: yes
Reasoning: Different approach, valuable alternative
";
        let ratings = parse_ratings_from_ai_response(response, &proposals, &ParseConfig::default());
        assert_eq!(ratings.len(), 3);
        assert_eq!(ratings[0].proposal_id, "p-1");
        assert!((ratings[0].score - 0.85).abs() < 1e-9);
        assert!(ratings[0].should_post);
        assert_eq!(
            ratings[0].reasoning,
            "High quality response with good technical detail"
        );
        assert_eq!(ratings[1].proposal_id, "p-2");
        assert!((ratings[1].score - 0.60).abs() < 1e-9);
        assert!(!ratings[1].should_post);
        assert_eq!(ratings[2].proposal_id, "p-3");
        assert!(ratings[2].should_post);
    }

    /// What this catches: AI returned only 1 rating but we have 3
    /// proposals. The 2 missing positions must be filled with the
    /// configured defaults so the caller always receives proposals.len()
    /// ratings. Same fallback contract as TS.
    #[test]
    fn fills_missing_positions_with_defaults_when_ai_returned_fewer() {
        let proposals = vec![p("p-1", "alice"), p("p-2", "bob"), p("p-3", "carol")];
        let response = "\
PROPOSAL 1:
Score: 0.9
ShouldPost: yes
Reasoning: only this one
";
        let cfg = ParseConfig::default();
        let ratings = parse_ratings_from_ai_response(response, &proposals, &cfg);
        assert_eq!(ratings.len(), 3);
        assert_eq!(ratings[0].proposal_id, "p-1");
        assert!((ratings[0].score - 0.9).abs() < 1e-9);
        for i in 1..3 {
            assert_eq!(ratings[i].proposal_id, proposals[i].proposal_id);
            assert_eq!(ratings[i].score, cfg.default_score);
            assert_eq!(ratings[i].should_post, cfg.default_should_post);
            assert_eq!(ratings[i].reasoning, cfg.missing_rating_reasoning);
        }
    }

    /// What this catches: AI returned MORE sections than proposals.
    /// We must take only proposals.len() — extra sections are ignored.
    /// Same as TS `Math.min(sections.length, proposals.length)`.
    #[test]
    fn caps_at_proposals_length_when_ai_returned_more() {
        let proposals = vec![p("p-1", "alice")];
        let response = "\
PROPOSAL 1:
Score: 0.5
ShouldPost: no
Reasoning: ok

PROPOSAL 2:
Score: 0.9
ShouldPost: yes
Reasoning: should not appear
";
        let ratings = parse_ratings_from_ai_response(response, &proposals, &ParseConfig::default());
        assert_eq!(ratings.len(), 1);
        assert_eq!(ratings[0].proposal_id, "p-1");
        assert!((ratings[0].score - 0.5).abs() < 1e-9);
    }

    /// What this catches: missing Score: line falls back to
    /// default_score. Common AI failure mode — model outputs reasoning
    /// without the structured fields.
    #[test]
    fn missing_score_line_falls_back_to_default() {
        let proposals = vec![p("p-1", "alice")];
        let response = "\
PROPOSAL 1:
ShouldPost: yes
Reasoning: forgot the score line
";
        let cfg = ParseConfig::default();
        let ratings = parse_ratings_from_ai_response(response, &proposals, &cfg);
        assert_eq!(ratings[0].score, cfg.default_score);
        assert!(ratings[0].should_post);
    }

    /// What this catches: missing ShouldPost: falls back to
    /// default_should_post (conservative `false`). Drift would let
    /// half-parsed responses post by accident.
    #[test]
    fn missing_should_post_line_falls_back_to_conservative_no() {
        let proposals = vec![p("p-1", "alice")];
        let response = "\
PROPOSAL 1:
Score: 0.9
Reasoning: high score, but no post directive
";
        let ratings = parse_ratings_from_ai_response(response, &proposals, &ParseConfig::default());
        assert_eq!(ratings[0].should_post, false);
        assert!((ratings[0].score - 0.9).abs() < 1e-9);
    }

    /// What this catches: score >1.0 gets clamped down to 1.0; negative
    /// scores fall back to default because the `[0-9.]+` regex doesn't
    /// match a leading `-` (so the whole capture fails and the parser
    /// uses `default_score`, not a clamped negative). This mirrors the
    /// TS regex `/Score:\s*([0-9.]+)/` exactly — the minus sign is
    /// invisible to it. Documented so a future reader doesn't "fix" the
    /// regex to allow negatives without checking the TS contract first.
    #[test]
    fn out_of_range_scores_handled_consistently_with_ts() {
        let proposals = vec![p("p-1", "alice"), p("p-2", "bob")];
        let response = "\
PROPOSAL 1:
Score: 1.5
ShouldPost: yes
Reasoning: too high

PROPOSAL 2:
Score: -0.3
ShouldPost: no
Reasoning: leading minus prevents [0-9.]+ from matching at all
";
        let cfg = ParseConfig::default();
        let ratings = parse_ratings_from_ai_response(response, &proposals, &cfg);
        assert_eq!(ratings[0].score, 1.0, "1.5 clamps down to 1.0");
        assert_eq!(
            ratings[1].score, cfg.default_score,
            "negative score → regex fails to match → default_score (0.5), same as TS"
        );
    }

    /// What this catches: case-insensitive ShouldPost match. AI sometimes
    /// outputs "ShouldPost: YES" or "shouldpost: yes" — must accept both.
    #[test]
    fn should_post_match_is_case_insensitive() {
        let proposals = vec![p("p-1", "alice"), p("p-2", "bob")];
        let response = "\
PROPOSAL 1:
Score: 0.5
ShouldPost: YES
Reasoning: a

PROPOSAL 2:
Score: 0.5
shouldpost: NO
Reasoning: b
";
        let ratings = parse_ratings_from_ai_response(response, &proposals, &ParseConfig::default());
        assert_eq!(ratings[0].should_post, true);
        assert_eq!(ratings[1].should_post, false);
    }

    /// What this catches: case-insensitive PROPOSAL N: split. AI
    /// sometimes outputs `Proposal 1:` or `proposal 1:`.
    #[test]
    fn proposal_split_is_case_insensitive() {
        let proposals = vec![p("p-1", "alice"), p("p-2", "bob")];
        let response = "\
Proposal 1:
Score: 0.4
ShouldPost: no
Reasoning: lower-case header

proposal 2:
Score: 0.6
ShouldPost: yes
Reasoning: still parses
";
        let ratings = parse_ratings_from_ai_response(response, &proposals, &ParseConfig::default());
        assert_eq!(ratings.len(), 2);
        assert!((ratings[0].score - 0.4).abs() < 1e-9);
        assert!((ratings[1].score - 0.6).abs() < 1e-9);
    }

    /// What this catches: completely empty / unparseable AI response.
    /// All proposals get the missing-rating defaults. Same as TS path.
    #[test]
    fn empty_response_fills_all_defaults() {
        let proposals = vec![p("p-1", "alice"), p("p-2", "bob")];
        let cfg = ParseConfig::default();
        let ratings = parse_ratings_from_ai_response("", &proposals, &cfg);
        assert_eq!(ratings.len(), 2);
        for r in &ratings {
            assert_eq!(r.score, cfg.default_score);
            assert_eq!(r.should_post, cfg.default_should_post);
            assert_eq!(r.reasoning, cfg.missing_rating_reasoning);
        }
    }

    /// What this catches: zero proposals + non-empty response = empty
    /// ratings. Edge case but the loop must not panic on cap calc.
    #[test]
    fn zero_proposals_yields_zero_ratings() {
        let proposals: Vec<ResponseProposal> = vec![];
        let response = "PROPOSAL 1:\nScore: 0.5\nShouldPost: yes\nReasoning: x";
        let ratings = parse_ratings_from_ai_response(response, &proposals, &ParseConfig::default());
        assert!(ratings.is_empty());
    }

    /// What this catches: reasoning ends at the first blank line, even
    /// when followed by trailing text (like the next PROPOSAL section).
    /// Without the lazy + lookahead, the regex could capture all the way
    /// to end-of-input and concat reasonings.
    #[test]
    fn reasoning_terminates_at_blank_line_not_end_of_input() {
        let proposals = vec![p("p-1", "alice"), p("p-2", "bob")];
        let response = "\
PROPOSAL 1:
Score: 0.5
ShouldPost: yes
Reasoning: first reasoning ends here

PROPOSAL 2:
Score: 0.5
ShouldPost: yes
Reasoning: second reasoning
";
        let ratings = parse_ratings_from_ai_response(response, &proposals, &ParseConfig::default());
        assert_eq!(ratings[0].reasoning, "first reasoning ends here");
        assert_eq!(ratings[1].reasoning, "second reasoning");
    }
}
