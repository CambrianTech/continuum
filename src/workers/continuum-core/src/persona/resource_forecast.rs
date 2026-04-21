//! Meta-cognitive resource forecast — the persona's own prediction
//! of what this next turn will cost.
//!
//! Per §20 of docs/architecture/PERSONA-CONTEXT-PAGING.md: when the
//! paging levers exist, the persona becomes a CONSUMER of them — it
//! introspects its own state + the incoming message and produces a
//! forecast that the policy reads as an advisory hint.
//!
//! Same primitive as the existing PersonaState (energy / attention /
//! mood / cadence) for temporal resources; extended to spatial
//! resources (context, reasoning depth). Personas that are tired
//! naturally request less; engaged personas request more.
//!
//! This module is the FORECAST half of the trait. The request-grant
//! and report-actual-usage halves land with the paging policy
//! (Phase 3.x) — they need infrastructure that doesn't exist yet.
//! Forecast is pure data + read of PersonaState, so it ships now.

use crate::persona::types::PersonaState;
use serde::{Deserialize, Serialize};

/// Hints about the incoming message the persona is about to handle.
/// The orchestrator extracts these cheaply (length, modality flags,
/// urgency from sender priority) before the persona's turn fires.
/// Forecast reads these to decide what kind of turn this will be.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessagePreview {
    /// Estimated token count of the incoming message text. Cheap to
    /// compute (~chars/4); doesn't require tokenization.
    pub estimated_input_tokens: u32,
    /// Sender attached an image / vision artifact.
    pub has_image: bool,
    /// Sender attached audio (live voice frame, recorded clip).
    pub has_audio: bool,
    /// Sender flagged urgency (e.g. user is mid-conversation, not background).
    pub is_urgent: bool,
    /// Sender directly mentioned this persona (e.g. "@helper").
    pub is_directed_mention: bool,
    /// Heuristic 0.0..1.0: how concept-dense / open-ended the prompt looks.
    /// 0.0 = casual greeting, 0.5 = typical question, 1.0 = open-ended
    /// research / multi-perspective ask. Computed cheaply by the orchestrator
    /// (e.g. count of question marks, presence of "explain"/"why"/"compare",
    /// length normalized to typical chat range).
    pub concept_density: f32,
}

/// What the persona thinks it will need for the upcoming turn.
/// The policy reads this as an advisory hint when sizing the slot's
/// allocation — it's not a hard demand (policy can deny if pressure
/// is high) but it's a strongly-weighted input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceForecast {
    /// Tokens of context the persona expects to use (input + reasoning + output).
    pub estimated_context_tokens: u32,
    /// 0.0..1.0 — how deeply the persona expects to reason. 0.0 = trivial
    /// reply, 1.0 = max introspection (long `<think>` block, multi-step
    /// analysis). Drives the reasoning-budget portion of the forecast.
    pub estimated_reasoning_depth: f32,
    /// Special modality tokens the turn will use beyond text.
    pub modality_demand: ModalityDemand,
    /// 0.0..1.0 — how confident the persona is in this forecast. Low
    /// confidence = "I'm tired and my last turns were nothing like this,
    /// could be wrong"; the policy weights uncertain forecasts less.
    pub confidence: f32,
    /// 0.0..1.0 — how time-pressured the response is. Drives the policy's
    /// choice of residency tier (urgent + cold = bad UX, must promote first).
    pub urgency: f32,
}

/// Per-modality additional resource demand.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ModalityDemand {
    /// Approximate vision tokens (image patches) the turn will consume.
    /// 0 = no image. Typical image ≈ 1500-2500 tokens depending on resolution.
    pub vision_tokens: u32,
    /// Approximate audio tokens (audio chunks) the turn will consume.
    pub audio_tokens: u32,
}

/// Compute the persona's resource forecast for an incoming turn.
///
/// Pure function — reads PersonaState (energy / attention / mood /
/// inbox_load), the message preview, and the recipe-declared default
/// seed budget for the persona's task class. Produces a forecast the
/// policy uses as a sizing hint.
///
/// Heuristic now (Phase 1.4); learned later from the
/// `report_actual_usage` telemetry feedback (Phase 4.0). Same
/// architectural pattern as the rest of the policy: rules first,
/// telemetry feeds the eventual learned replacement.
pub fn forecast_from_state(
    state: &PersonaState,
    msg: &MessagePreview,
    recipe_default_seed: u32,
) -> ResourceForecast {
    // ── Reasoning depth ──
    // Driven by message complexity AND persona state. A casual greeting
    // gets shallow regardless of state; a complex question gets deep
    // ONLY if the persona has the energy/attention to actually go deep.
    let energy_factor = state.energy.clamp(0.0, 1.0);
    let attention_factor = state.attention.clamp(0.0, 1.0);
    let state_capability = (energy_factor + attention_factor) / 2.0;
    let reasoning_depth = (msg.concept_density * state_capability).clamp(0.0, 1.0);

    // ── Context tokens ──
    // Start from the recipe seed (the steady-state allocation), then
    // add: input message + expected reasoning output (proportional to
    // depth) + a small buffer.
    let input_tokens = msg.estimated_input_tokens;
    // Reasoning output rough estimate: depth=1.0 → ~3000 tokens of
    // <think> + visible answer; depth=0.0 → ~50 tokens.
    let reasoning_output_tokens = (3000.0 * reasoning_depth + 50.0) as u32;
    let estimated_context = recipe_default_seed
        .saturating_add(input_tokens)
        .saturating_add(reasoning_output_tokens);

    // ── Modality demand ──
    // Vision/audio add transient tokens for this turn only.
    let modality_demand = ModalityDemand {
        vision_tokens: if msg.has_image { 2000 } else { 0 },
        audio_tokens: if msg.has_audio { 500 } else { 0 },
    };

    // ── Confidence ──
    // Higher when energy is up (rested persona's predictions are usually
    // accurate) and inbox is light (not racing through cases). Drops
    // when fatigued — a tired persona's "I think this will be small"
    // is less reliable.
    let inbox_pressure = (state.inbox_load as f32 / 10.0).clamp(0.0, 1.0);
    let confidence = ((energy_factor + (1.0 - inbox_pressure)) / 2.0).clamp(0.1, 1.0);

    // ── Urgency ──
    // Direct mentions and explicit urgent flags push hard; concept-
    // dense long-form questions are less time-sensitive. Always at
    // least slight urgency in chat (humans waiting).
    let urgency_base = if msg.is_directed_mention { 0.7 } else { 0.3 };
    let urgency_boost = if msg.is_urgent { 0.3 } else { 0.0 };
    // Open-ended research questions are LESS urgent — user expects to wait.
    let urgency_dampener = msg.concept_density * 0.2;
    let urgency = (urgency_base + urgency_boost - urgency_dampener).clamp(0.0, 1.0);

    ResourceForecast {
        estimated_context_tokens: estimated_context,
        estimated_reasoning_depth: reasoning_depth,
        modality_demand,
        confidence,
        urgency,
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::types::PersonaState;

    fn fresh_state() -> PersonaState {
        PersonaState::default()
    }

    fn tired_state() -> PersonaState {
        let mut s = PersonaState::default();
        s.energy = 0.2;
        s.attention = 0.3;
        s.inbox_load = 8;
        s
    }

    /// What this catches: forecast missing the input tokens entirely,
    /// or forgetting to add the recipe seed. The estimated_context
    /// MUST grow with input length AND start from the seed.
    ///
    /// Validated 2026-04-21: removed input_tokens from the addition,
    /// test fails because two different message lengths produce same
    /// estimate; reverted.
    #[test]
    fn estimated_context_grows_with_input_length_above_seed() {
        let state = fresh_state();
        let small_msg = MessagePreview {
            estimated_input_tokens: 20,
            ..Default::default()
        };
        let big_msg = MessagePreview {
            estimated_input_tokens: 500,
            ..Default::default()
        };
        let small = forecast_from_state(&state, &small_msg, 8 * 1024);
        let big = forecast_from_state(&state, &big_msg, 8 * 1024);
        assert!(small.estimated_context_tokens >= 8 * 1024);
        assert!(big.estimated_context_tokens > small.estimated_context_tokens);
        assert_eq!(
            big.estimated_context_tokens - small.estimated_context_tokens,
            500 - 20,
            "context delta should equal input token delta"
        );
    }

    /// What this catches: reasoning_depth ignoring persona state. A
    /// tired persona facing a complex question should NOT forecast
    /// the same deep reasoning as a fresh persona — capability gates
    /// what depth is realistic.
    ///
    /// Validated 2026-04-21: changed state_capability multiplier to
    /// always 1.0, test fails because both forecasts produce identical
    /// depth; reverted.
    #[test]
    fn reasoning_depth_scales_down_when_persona_is_tired() {
        let complex_msg = MessagePreview {
            estimated_input_tokens: 200,
            concept_density: 0.9,
            ..Default::default()
        };
        let fresh = forecast_from_state(&fresh_state(), &complex_msg, 8 * 1024);
        let tired = forecast_from_state(&tired_state(), &complex_msg, 8 * 1024);
        assert!(
            fresh.estimated_reasoning_depth > tired.estimated_reasoning_depth,
            "fresh depth {} should exceed tired depth {}",
            fresh.estimated_reasoning_depth,
            tired.estimated_reasoning_depth,
        );
    }

    /// What this catches: casual greetings forecasting deep reasoning,
    /// which would over-allocate context for trivial turns. concept_density
    /// 0.0 should produce near-zero reasoning depth regardless of state.
    ///
    /// Validated 2026-04-21: hardcoded reasoning_depth to 0.5, test fails
    /// because casual greeting still forecasts 0.5; reverted.
    #[test]
    fn casual_greeting_forecasts_shallow_reasoning() {
        let casual = MessagePreview {
            estimated_input_tokens: 5,
            concept_density: 0.0, // "hi"
            ..Default::default()
        };
        let f = forecast_from_state(&fresh_state(), &casual, 8 * 1024);
        assert!(
            f.estimated_reasoning_depth < 0.1,
            "casual greeting depth should be near-zero, got {}",
            f.estimated_reasoning_depth
        );
    }

    /// What this catches: vision/audio modality demand getting silently
    /// dropped (forecast says "no extra modality" when an image is
    /// attached). Policy needs to know transient KV burst is coming.
    ///
    /// Validated 2026-04-21: hardcoded vision_tokens=0, test fails
    /// because has_image=true forecast still reports 0; reverted.
    #[test]
    fn modality_demand_surfaces_when_image_or_audio_attached() {
        let with_image = MessagePreview {
            estimated_input_tokens: 50,
            has_image: true,
            ..Default::default()
        };
        let with_audio = MessagePreview {
            estimated_input_tokens: 50,
            has_audio: true,
            ..Default::default()
        };
        let with_both = MessagePreview {
            estimated_input_tokens: 50,
            has_image: true,
            has_audio: true,
            ..Default::default()
        };
        let text_only = MessagePreview {
            estimated_input_tokens: 50,
            ..Default::default()
        };
        let state = fresh_state();
        assert!(forecast_from_state(&state, &with_image, 8192).modality_demand.vision_tokens > 0);
        assert!(forecast_from_state(&state, &with_audio, 8192).modality_demand.audio_tokens > 0);
        assert!(forecast_from_state(&state, &with_both, 8192).modality_demand.vision_tokens > 0);
        assert!(forecast_from_state(&state, &with_both, 8192).modality_demand.audio_tokens > 0);
        assert_eq!(forecast_from_state(&state, &text_only, 8192).modality_demand.vision_tokens, 0);
        assert_eq!(forecast_from_state(&state, &text_only, 8192).modality_demand.audio_tokens, 0);
    }

    /// What this catches: confidence not reflecting state. Policy uses
    /// confidence as a weight — low confidence = "trust this less."
    /// A tired-with-overflowing-inbox persona's predictions are flakier;
    /// confidence must drop accordingly.
    ///
    /// Validated 2026-04-21: hardcoded confidence=1.0, test fails
    /// because tired confidence stays at 1.0; reverted.
    #[test]
    fn confidence_is_lower_when_persona_is_tired_and_overloaded() {
        let msg = MessagePreview {
            estimated_input_tokens: 100,
            concept_density: 0.5,
            ..Default::default()
        };
        let fresh = forecast_from_state(&fresh_state(), &msg, 8192);
        let tired = forecast_from_state(&tired_state(), &msg, 8192);
        assert!(fresh.confidence > tired.confidence);
        assert!(fresh.confidence > 0.5, "fresh persona should be reasonably confident");
    }

    /// What this catches: urgency not reflecting message signals.
    /// Direct mentions ("@helper, look at this NOW") should bump
    /// urgency; open-ended research questions should be less urgent.
    ///
    /// Validated 2026-04-21: hardcoded urgency_base ignoring
    /// is_directed_mention, test fails because mention vs no-mention
    /// produce same urgency; reverted.
    #[test]
    fn urgency_responds_to_mention_and_concept_density() {
        let state = fresh_state();
        let casual_no_mention = MessagePreview {
            estimated_input_tokens: 30,
            concept_density: 0.1,
            ..Default::default()
        };
        let mentioned = MessagePreview {
            estimated_input_tokens: 30,
            concept_density: 0.1,
            is_directed_mention: true,
            ..Default::default()
        };
        let research_question = MessagePreview {
            estimated_input_tokens: 200,
            concept_density: 0.95,
            ..Default::default()
        };
        let casual_u = forecast_from_state(&state, &casual_no_mention, 8192).urgency;
        let mention_u = forecast_from_state(&state, &mentioned, 8192).urgency;
        let research_u = forecast_from_state(&state, &research_question, 8192).urgency;
        assert!(
            mention_u > casual_u,
            "mentioned ({mention_u}) should be more urgent than casual ({casual_u})"
        );
        assert!(
            research_u < casual_u + 0.5,
            "research question ({research_u}) should not be runaway-urgent vs casual ({casual_u})"
        );
    }

    /// What this catches: forecast values escaping their declared
    /// 0.0..1.0 ranges (depth, confidence, urgency). All three are
    /// supposed to be normalized; out-of-range values would break
    /// downstream policy math.
    ///
    /// Validated 2026-04-21: removed clamp on reasoning_depth, made
    /// concept_density 2.0 (caller-pathological), test fails because
    /// depth = 2.0; reverted (clamp restored).
    #[test]
    fn normalized_fields_stay_within_zero_to_one() {
        let state = fresh_state();
        // Pathological caller-supplied values that would overflow without clamps
        let extreme_msg = MessagePreview {
            estimated_input_tokens: 100,
            concept_density: 5.0, // out-of-range input
            is_directed_mention: true,
            is_urgent: true,
            ..Default::default()
        };
        let f = forecast_from_state(&state, &extreme_msg, 8192);
        assert!(
            (0.0..=1.0).contains(&f.estimated_reasoning_depth),
            "depth must be 0..1, got {}",
            f.estimated_reasoning_depth
        );
        assert!(
            (0.0..=1.0).contains(&f.confidence),
            "confidence must be 0..1, got {}",
            f.confidence
        );
        assert!(
            (0.0..=1.0).contains(&f.urgency),
            "urgency must be 0..1, got {}",
            f.urgency
        );
    }
}
