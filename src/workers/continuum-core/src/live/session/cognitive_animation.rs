//! Cognitive state → avatar gesture mapping.
//!
//! Maps AI cognitive phases (evaluating, generating) to weighted gesture selections
//! for Bevy avatar animation. Each state has a table of possible gestures with
//! weights and duration ranges. Gestures re-roll on a configurable interval,
//! creating natural variety while a cognitive state persists.
//!
//! `CognitiveAnimationConfig` is ts-rs exported for future LoRA adapter control
//! of per-persona body language.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::live::video::bevy_renderer::Gesture;

/// Cognitive state of an AI persona — drives avatar gesture selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/voice/CognitiveState.ts")]
#[serde(rename_all = "camelCase")]
pub enum CognitiveState {
    /// Persona is evaluating whether to respond (reading, thinking)
    Evaluating,
    /// Persona is generating a response (writing, composing)
    Generating,
    /// Persona is idle (no active cognitive work)
    Idle,
}

/// A gesture with its selection weight and duration range.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/voice/WeightedGesture.ts")]
#[serde(rename_all = "camelCase")]
pub struct WeightedGesture {
    /// Gesture name matching the Gesture enum variant.
    pub gesture: String,
    /// Selection weight (relative, not normalized — weights are summed for probability).
    pub weight: f32,
    /// Minimum duration in milliseconds.
    pub duration_min_ms: u32,
    /// Maximum duration in milliseconds.
    pub duration_max_ms: u32,
}

/// Per-state gesture configuration. Future: LoRA adapters output personalized configs.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/voice/CognitiveAnimationConfig.ts")]
#[serde(rename_all = "camelCase")]
pub struct CognitiveAnimationConfig {
    /// Gestures available during the Evaluating state.
    pub evaluating: Vec<WeightedGesture>,
    /// Gestures available during the Generating state.
    pub generating: Vec<WeightedGesture>,
    /// Seconds between gesture re-rolls while state persists.
    pub reroll_interval_secs: f32,
}

/// Seconds between gesture re-rolls (default).
pub const DEFAULT_REROLL_INTERVAL_SECS: f32 = 6.0;

impl Default for CognitiveAnimationConfig {
    fn default() -> Self {
        Self {
            evaluating: vec![
                WeightedGesture {
                    gesture: "Think".into(),
                    weight: 0.6,
                    duration_min_ms: 4000,
                    duration_max_ms: 7000,
                },
                WeightedGesture {
                    gesture: "Nod".into(),
                    weight: 0.2,
                    duration_min_ms: 2000,
                    duration_max_ms: 3000,
                },
                WeightedGesture {
                    gesture: "None".into(),
                    weight: 0.2,
                    duration_min_ms: 3000,
                    duration_max_ms: 5000,
                },
            ],
            generating: vec![
                WeightedGesture {
                    gesture: "OpenHands".into(),
                    weight: 0.4,
                    duration_min_ms: 3000,
                    duration_max_ms: 6000,
                },
                WeightedGesture {
                    gesture: "Nod".into(),
                    weight: 0.3,
                    duration_min_ms: 2000,
                    duration_max_ms: 4000,
                },
                WeightedGesture {
                    gesture: "Think".into(),
                    weight: 0.15,
                    duration_min_ms: 3000,
                    duration_max_ms: 5000,
                },
                WeightedGesture {
                    gesture: "Point".into(),
                    weight: 0.15,
                    duration_min_ms: 2000,
                    duration_max_ms: 4000,
                },
            ],
            reroll_interval_secs: DEFAULT_REROLL_INTERVAL_SECS,
        }
    }
}

/// Select a gesture from the weighted table using deterministic pseudo-random.
///
/// Uses elapsed time and slot index to create variety without the `rand` crate.
/// Returns `(Gesture, duration_ms)` or `None` if the table is empty.
pub fn select_weighted_gesture(
    table: &[WeightedGesture],
    elapsed_secs: f32,
    slot: u8,
) -> Option<(Gesture, u32)> {
    if table.is_empty() {
        return None;
    }

    let total_weight: f32 = table.iter().map(|g| g.weight).sum();
    if total_weight <= 0.0 {
        return None;
    }

    // Deterministic pseudo-random value in [0, 1) from elapsed time + slot
    let hash = (elapsed_secs * 1000.0 + slot as f32 * 137.0) % 10000.0;
    let rand_val = (hash.sin().abs()) % 1.0;

    // Weighted selection
    let threshold = rand_val * total_weight;
    let mut cumulative = 0.0;
    for entry in table {
        cumulative += entry.weight;
        if cumulative >= threshold {
            let gesture = gesture_from_name(&entry.gesture);
            // Duration pseudo-random within [min, max]
            let duration_hash = ((elapsed_secs * 1337.0 + slot as f32 * 42.0) % 10000.0).sin().abs();
            let range = entry.duration_max_ms.saturating_sub(entry.duration_min_ms);
            let duration_ms = entry.duration_min_ms + (duration_hash * range as f32) as u32;
            return Some((gesture, duration_ms));
        }
    }

    // Fallback to last entry (floating point edge case)
    let last = &table[table.len() - 1];
    Some((gesture_from_name(&last.gesture), last.duration_min_ms))
}

/// Map gesture name string to Gesture enum.
fn gesture_from_name(name: &str) -> Gesture {
    match name {
        "Wave" => Gesture::Wave,
        "Think" => Gesture::Think,
        "Nod" => Gesture::Nod,
        "Shrug" => Gesture::Shrug,
        "Point" => Gesture::Point,
        "OpenHands" => Gesture::OpenHands,
        _ => Gesture::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_entries() {
        let config = CognitiveAnimationConfig::default();
        assert!(!config.evaluating.is_empty());
        assert!(!config.generating.is_empty());
        assert!(config.reroll_interval_secs > 0.0);
    }

    #[test]
    fn select_returns_valid_gesture() {
        let config = CognitiveAnimationConfig::default();
        for slot in 0..16u8 {
            for t in [1.0, 5.0, 10.0, 25.0, 100.0] {
                let result = select_weighted_gesture(&config.evaluating, t, slot);
                assert!(result.is_some(), "slot={slot} t={t} returned None");
                let (gesture, duration) = result.unwrap();
                // Should be one of: Think, Nod, None
                assert!(
                    matches!(gesture, Gesture::Think | Gesture::Nod | Gesture::None),
                    "Unexpected evaluating gesture: {:?}",
                    gesture
                );
                assert!(duration >= 2000 && duration <= 7000, "Duration out of range: {duration}");
            }
        }
    }

    #[test]
    fn select_generating_returns_valid_gesture() {
        let config = CognitiveAnimationConfig::default();
        for slot in 0..16u8 {
            let result = select_weighted_gesture(&config.generating, 15.0, slot);
            assert!(result.is_some());
            let (gesture, duration) = result.unwrap();
            assert!(
                matches!(gesture, Gesture::OpenHands | Gesture::Nod | Gesture::Think | Gesture::Point),
                "Unexpected generating gesture: {:?}",
                gesture
            );
            assert!(duration >= 2000 && duration <= 6000, "Duration out of range: {duration}");
        }
    }

    #[test]
    fn empty_table_returns_none() {
        assert!(select_weighted_gesture(&[], 1.0, 0).is_none());
    }

    #[test]
    fn variety_across_time() {
        let config = CognitiveAnimationConfig::default();
        let mut gestures = std::collections::HashSet::new();
        // Sample across many time values — should get variety
        for i in 0..100 {
            if let Some((gesture, _)) = select_weighted_gesture(&config.evaluating, i as f32 * 0.5, 0) {
                gestures.insert(format!("{:?}", gesture));
            }
        }
        // Should see at least 2 different gestures across 100 samples
        assert!(gestures.len() >= 2, "Expected variety, got: {:?}", gestures);
    }
}
