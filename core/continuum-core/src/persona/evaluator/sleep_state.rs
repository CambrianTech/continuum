//! Voluntary sleep state for personas.
//!
//! Mirrors the TypeScript `PersonaSleepManager`. Drives Gate 4 of
//! `full_evaluate` — whether the persona is currently in a self-imposed
//! quiet mode, and whether an auto-wake threshold has passed.
//!
//! Extracted from `evaluator.rs` (continuum#1208) — independent of the
//! gate pipeline, reusable wherever a persona's attention state matters.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Voluntary sleep modes — persona controls own attention.
#[derive(
    Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/SleepMode.ts"
)]
pub enum SleepMode {
    #[default]
    Active,
    MentionedOnly,
    HumanOnly,
    Sleeping,
    UntilTopic,
}

/// Per-persona sleep state with optional auto-wake.
#[derive(Debug, Clone)]
pub struct SleepState {
    pub mode: SleepMode,
    pub reason: String,
    pub set_at_ms: u64,
    pub wake_at_ms: Option<u64>,
}

impl Default for SleepState {
    fn default() -> Self {
        Self {
            mode: SleepMode::Active,
            reason: String::new(),
            set_at_ms: 0,
            wake_at_ms: None,
        }
    }
}

impl SleepState {
    /// Check if auto-wake time has passed. Returns true if should wake.
    pub fn should_auto_wake(&self, now_ms: u64) -> bool {
        if let Some(wake_at) = self.wake_at_ms {
            now_ms >= wake_at
        } else {
            false
        }
    }

    /// Get effective mode, accounting for auto-wake.
    pub fn effective_mode(&self, now_ms: u64) -> SleepMode {
        if self.should_auto_wake(now_ms) {
            SleepMode::Active
        } else {
            self.mode
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: regression where `effective_mode` stops
    /// honoring the auto-wake threshold and keeps reporting the
    /// stored sleep mode after `wake_at_ms` has passed.
    #[test]
    fn effective_mode_returns_active_after_wake_threshold() {
        let state = SleepState {
            mode: SleepMode::Sleeping,
            reason: "test".into(),
            set_at_ms: 1000,
            wake_at_ms: Some(2000),
        };
        assert_eq!(state.effective_mode(1500), SleepMode::Sleeping);
        assert_eq!(state.effective_mode(2000), SleepMode::Active);
        assert_eq!(state.effective_mode(3000), SleepMode::Active);
    }

    /// What this catches: regression where a sleep state with no
    /// `wake_at_ms` (manual sleep, no auto-wake) accidentally reports
    /// itself as awake.
    #[test]
    fn effective_mode_with_no_wake_threshold_keeps_sleeping() {
        let state = SleepState {
            mode: SleepMode::Sleeping,
            reason: "manual".into(),
            set_at_ms: 1000,
            wake_at_ms: None,
        };
        assert_eq!(state.effective_mode(u64::MAX), SleepMode::Sleeping);
    }
}
