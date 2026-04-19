//! Response Orchestrator — the verb that produces `ResponderDecision[]`.
//!
//! Takes a `SharedAnalysis` + room participants + (eventually) lever
//! calls, returns one `ResponderDecision` per persona. `should_respond`
//! = false is a first-class outcome — silence-with-reason is the
//! architecture's preferred answer when a persona has nothing additive.
//!
//! Phase A: pure-function specialty matching against
//! `SharedAnalysis.suggested_angles`. No lever evaluation yet (A.5),
//! no streaming-lead election (B.3). Heuristic relevance score:
//! the persona's specialty key matches a non-empty entry in
//! `suggested_angles` → relevant. Empty entry → silent.
//!
//! Why Rust: this runs on EVERY chat message + per-persona scoring
//! parallelizes. SIMD-friendly when scores grow more sophisticated
//! (cosine match against analysis embedding, etc.). Stays in the
//! same crate as `shared_analysis` so future fusions (e.g. analysis
//! produces an embedding the orchestrator scores against, in one
//! pass) don't have to cross IPC.

use crate::cognition::types::{ResponderDecision, SharedAnalysis};
use uuid::Uuid;

/// Threshold above which a persona is selected to respond. Below =
/// silent with reason. 0.0..1.0 scale.
///
/// Default 0.30 — generous enough that any persona with a non-empty
/// matched angle responds; strict enough that empty-angle personas
/// stay silent. Tunable per-room/per-recipe later.
pub const DEFAULT_RELEVANCE_THRESHOLD: f32 = 0.30;

/// What the orchestrator needs about each persona in the room. Minimal
/// — the orchestrator doesn't need the full UserEntity, just the
/// identity + specialty + capability state.
#[derive(Debug, Clone)]
pub struct PersonaSlot {
    pub persona_id: Uuid,
    /// Stable specialty identifier — must match a key in
    /// `SharedAnalysis.suggested_angles` to be selected. Personas
    /// without a specialty (or with one that doesn't appear in the
    /// analysis's known specialties) get a generic "general" treatment.
    pub specialty: String,
    /// Optional human-readable name for the explanation string.
    /// Pure cosmetic — orchestration logic uses persona_id.
    pub display_name: String,
}

/// Orchestrate responders for a chat turn.
///
/// Phase A heuristic: for each persona, look up its specialty in
/// `analysis.suggested_angles`. Non-empty entry → respond, score = 1.0
/// for now (refine when we have embedding similarity); empty / missing
/// entry → silent with the reason recorded for trainability.
///
/// Lead election: the highest-scoring responder is marked `is_lead=true`.
/// In Phase A all selected responders run in parallel anyway; the lead
/// flag is forward-compat with Phase B's streaming model where the lead
/// goes first and others build on it.
///
/// Pure function — no IO, no state, deterministic for given inputs.
/// Test in isolation; chat-path validation covers the integration.
pub fn orchestrate(
    analysis: &SharedAnalysis,
    personas: &[PersonaSlot],
    threshold: f32,
) -> Vec<ResponderDecision> {
    let threshold = threshold.clamp(0.0, 1.0);

    // First pass: score each persona. Track best for lead election.
    let mut decisions: Vec<ResponderDecision> = personas
        .iter()
        .map(|p| score_persona(analysis, p, threshold))
        .collect();

    // Lead election: highest relevance among `should_respond=true`. Ties
    // broken by persona_id ordering for determinism (same input always
    // produces same lead).
    let lead_idx = decisions
        .iter()
        .enumerate()
        .filter(|(_, d)| d.should_respond)
        .max_by(|(_, a), (_, b)| {
            a.relevance_score
                .partial_cmp(&b.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.persona_id.cmp(&a.persona_id)) // reverse for tie-break stability
        })
        .map(|(i, _)| i);

    if let Some(idx) = lead_idx {
        decisions[idx].is_lead = Some(true);
    }
    decisions
}

/// Score a single persona against the analysis. The Phase A heuristic
/// is intentionally simple: angle present + non-empty → 1.0; angle
/// present + empty → 0.0; angle missing entirely → small generic
/// score (lets unknown-specialty personas chime in but at low priority).
///
/// Phase B can replace this with embedding-similarity scoring without
/// changing the orchestrate() signature.
fn score_persona(
    analysis: &SharedAnalysis,
    persona: &PersonaSlot,
    threshold: f32,
) -> ResponderDecision {
    let angle = analysis.suggested_angles.get(&persona.specialty);

    let (score, matched_angles, explanation) = match angle {
        // Specialty has a non-empty angle → high relevance.
        Some(a) if !a.is_empty() => (
            1.0_f32,
            vec![persona.specialty.clone()],
            format!(
                "{} ({}): specialty matched analysis angle: {}",
                persona.display_name, persona.specialty, a
            ),
        ),
        // Specialty appeared in analysis but with empty angle → silent.
        Some(_) => (
            0.0_f32,
            Vec::new(),
            format!(
                "{} ({}): analysis assigned no signal to this specialty for this message",
                persona.display_name, persona.specialty
            ),
        ),
        // Specialty wasn't in the analysis's known specialties at all.
        // Give a small generic relevance — unknown specialties may still
        // be useful occasionally; let the threshold filter them.
        None => (
            0.10_f32,
            Vec::new(),
            format!(
                "{} ({}): specialty not in analysis's known set; generic-relevance only",
                persona.display_name, persona.specialty
            ),
        ),
    };

    ResponderDecision {
        persona_id: persona.persona_id,
        should_respond: score >= threshold,
        relevance_score: score,
        matched_angles,
        explanation,
        is_lead: None, // Lead election happens in orchestrate() across all decisions.
    }
}

#[cfg(test)]
mod tests {
    //! Pure-function tests. Validate scoring logic, threshold filtering,
    //! lead election, and silence-with-reason as a first-class outcome.
    use super::*;
    use crate::cognition::types::SharedAnalysisIntent;
    use std::collections::HashMap;

    fn fake_analysis(angles: Vec<(&str, &str)>) -> SharedAnalysis {
        let suggested_angles: HashMap<String, String> = angles
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        SharedAnalysis {
            message_id: Uuid::nil(),
            room_id: Uuid::nil(),
            cache_key: "test".to_string(),
            generated_at_ms: 0,
            summary: "test".to_string(),
            key_concepts: vec![],
            intent: SharedAnalysisIntent::Question,
            emotional_tone: None,
            suggested_angles,
            relevant_context: None,
            duration_ms: 0,
            model_used: "test".to_string(),
            from_cache: false,
        }
    }

    fn slot(name: &str, specialty: &str) -> PersonaSlot {
        PersonaSlot {
            persona_id: Uuid::new_v4(),
            specialty: specialty.to_string(),
            display_name: name.to_string(),
        }
    }

    #[test]
    fn persona_with_non_empty_angle_is_selected() {
        let analysis = fake_analysis(vec![
            ("code", "Direct relevance — the question is about caching."),
            ("general", ""),
        ]);
        let personas = vec![slot("CodeReview AI", "code")];
        let decisions = orchestrate(&analysis, &personas, DEFAULT_RELEVANCE_THRESHOLD);
        assert_eq!(decisions.len(), 1);
        assert!(decisions[0].should_respond);
        assert_eq!(decisions[0].matched_angles, vec!["code".to_string()]);
        assert!(decisions[0].relevance_score >= DEFAULT_RELEVANCE_THRESHOLD);
    }

    #[test]
    fn persona_with_empty_angle_is_silent_with_reason() {
        let analysis = fake_analysis(vec![("general", "")]);
        let personas = vec![slot("Helper AI", "general")];
        let decisions = orchestrate(&analysis, &personas, DEFAULT_RELEVANCE_THRESHOLD);
        assert_eq!(decisions.len(), 1);
        assert!(!decisions[0].should_respond);
        assert!(decisions[0].matched_angles.is_empty());
        // Explanation must explain why — silence is observable.
        assert!(decisions[0].explanation.contains("no signal"));
    }

    #[test]
    fn persona_with_unknown_specialty_is_generic_low_relevance() {
        let analysis = fake_analysis(vec![("code", "x")]);
        let personas = vec![slot("Mystery AI", "esoteric-specialty")];
        let decisions = orchestrate(&analysis, &personas, DEFAULT_RELEVANCE_THRESHOLD);
        // 0.10 < 0.30 default threshold → silent.
        assert!(!decisions[0].should_respond);
        assert_eq!(decisions[0].relevance_score, 0.10);
        assert!(decisions[0].explanation.contains("not in analysis"));
    }

    #[test]
    fn lower_threshold_lets_unknown_specialty_in() {
        let analysis = fake_analysis(vec![("code", "x")]);
        let personas = vec![slot("Mystery AI", "esoteric-specialty")];
        // Threshold 0.05 lets the 0.10 generic-relevance pass.
        let decisions = orchestrate(&analysis, &personas, 0.05);
        assert!(decisions[0].should_respond);
    }

    #[test]
    fn lead_election_picks_highest_relevance() {
        let analysis = fake_analysis(vec![
            ("code", "Direct hit."),
            ("general", ""),
            ("education", "Tangential but worth noting."),
        ]);
        let personas = vec![
            slot("Helper AI", "general"),
            slot("CodeReview AI", "code"),
            slot("Teacher AI", "education"),
        ];
        let decisions = orchestrate(&analysis, &personas, DEFAULT_RELEVANCE_THRESHOLD);

        // CodeReview + Teacher both selected (non-empty angles); Helper silent.
        let leads: Vec<_> = decisions.iter().filter(|d| d.is_lead == Some(true)).collect();
        assert_eq!(leads.len(), 1, "exactly one lead");

        // Both code and education score 1.0 (non-empty angle = 1.0). The lead
        // tie-break is deterministic by persona_id but we don't care which
        // wins here — just that exactly one was elected and they were a
        // selected (should_respond=true) persona.
        assert!(leads[0].should_respond);
    }

    #[test]
    fn no_responders_no_lead() {
        let analysis = fake_analysis(vec![("code", ""), ("general", "")]);
        let personas = vec![slot("Helper AI", "general"), slot("CodeReview AI", "code")];
        let decisions = orchestrate(&analysis, &personas, DEFAULT_RELEVANCE_THRESHOLD);
        assert!(decisions.iter().all(|d| !d.should_respond));
        assert!(decisions.iter().all(|d| d.is_lead.is_none()));
    }

    #[test]
    fn deterministic_for_same_input() {
        let analysis = fake_analysis(vec![("code", "x"), ("education", "y")]);
        let personas = vec![slot("a", "code"), slot("b", "education")];
        let d1 = orchestrate(&analysis, &personas, DEFAULT_RELEVANCE_THRESHOLD);
        let d2 = orchestrate(&analysis, &personas, DEFAULT_RELEVANCE_THRESHOLD);
        assert_eq!(d1.len(), d2.len());
        for (a, b) in d1.iter().zip(d2.iter()) {
            assert_eq!(a.should_respond, b.should_respond);
            assert_eq!(a.relevance_score, b.relevance_score);
            assert_eq!(a.is_lead, b.is_lead);
        }
    }
}
