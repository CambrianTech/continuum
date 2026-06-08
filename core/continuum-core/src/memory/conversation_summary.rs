//! Conversation summary — the consolidated event arc that personas
//! actually use, instead of full verbatim history per turn.
//!
//! Per §15 of docs/architecture/PERSONA-CONTEXT-PAGING.md:
//!
//! AIs don't need to re-read every prior word. They need:
//!   - The gist of the conversation arc (consolidated, ~200-500 tokens)
//!   - The specific recent exchange the new message responds to (verbatim window)
//!   - The new message itself
//!
//! Current default is verbatim-unless-tight (consolidation only fires when
//! token budget is pressured). This module is the substrate for flipping
//! that: consolidated-by-default, with verbatim opt-in via RecallMode.
//!
//! This file is the DATA layer (RecallMode enum, ConversationSummary
//! struct, helpers). Background-incremental update task and the actual
//! summarizer LLM call are separate (Phase 3.x of the implementation
//! roadmap; the substrate ships now so the rest can plug in).

use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use uuid::Uuid;

/// How a persona should consume conversation history for a given task.
/// Recipe-driven: the recipe author / persona / task-class declares
/// which mode is appropriate; the prompt assembler reads it and
/// builds the right kind of context block.
///
/// Defaults per §15.3 of the design doc:
///   Chat / VoiceChat / VideoChat / GameNpc → ConsolidatedSummary
///   CodingSmall → Hybrid { verbatim_window: 5 }
///   CodingLarge → Hybrid { verbatim_window: 10 }
///   AcademyStudent → Hybrid { verbatim_window: 5 }
///   SentinelHard → Hybrid { verbatim_window: 3 }
///   CodeReview / Translation / FreshDebug → Verbatim
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecallMode {
    /// Default for chat / NPC. Consolidated arc summary + last 1-2
    /// messages verbatim + current message. ~10x less context than
    /// verbatim, same downstream outcome for casual conversation.
    ConsolidatedSummary,
    /// Coding / academy / sentinel research. Consolidated arc + last N
    /// messages verbatim. The verbatim window covers the immediate
    /// reasoning context where exact wording matters.
    Hybrid {
        /// How many of the most-recent messages to include verbatim.
        /// 3 = sentinel research, 5 = academy / coding-small, 10 = coding-large.
        verbatim_window: u32,
    },
    /// Code review / translation / when the user explicitly asks
    /// "what did you say earlier about X". Full verbatim history
    /// within token budget. No consolidation — the model sees every
    /// word.
    Verbatim,
}

impl Default for RecallMode {
    fn default() -> Self {
        RecallMode::ConsolidatedSummary
    }
}

impl RecallMode {
    /// True if the mode involves any consolidated summary at all.
    /// Verbatim mode = full message history, no summary involved.
    pub fn uses_summary(self) -> bool {
        !matches!(self, RecallMode::Verbatim)
    }

    /// How many most-recent messages this mode wants verbatim.
    /// ConsolidatedSummary keeps the immediately-replied-to message;
    /// Hybrid declares the window; Verbatim wants all of them
    /// (returns u32::MAX as "no limit").
    pub fn verbatim_window_size(self) -> u32 {
        match self {
            RecallMode::ConsolidatedSummary => 2,
            RecallMode::Hybrid { verbatim_window } => verbatim_window,
            RecallMode::Verbatim => u32::MAX,
        }
    }
}

/// The persistent room-state object that holds the consolidated
/// conversation arc. One per room, shared across all personas in
/// that room (no per-persona re-summarization cost).
///
/// Background task incrementally extends this as new messages arrive
/// (rather than re-summarizing from scratch each turn). When a persona
/// turn fires, the summary is already current — no inline summarization
/// latency on the response path.
///
/// The fields here are the SHAPE; the actual summarizer LLM call and
/// the background-update task are separate (Phase 3.x). This struct
/// ships now so callers can construct + read summaries via the standard
/// data primitives.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConversationSummary {
    /// Which room this summary belongs to.
    pub room_id: Uuid,

    /// How many turns of the conversation have been folded into
    /// `arc_summary`. New messages beyond this index are NOT yet in
    /// the summary — they live verbatim in the (separate) recent-
    /// messages buffer until the next consolidation pass.
    pub turns_summarized: u32,

    /// Dense narrative summary of the conversation so far. ~200-500
    /// tokens for a typical chat. Updated incrementally — each new
    /// summarization pass appends/refines, doesn't rewrite from scratch.
    pub arc_summary: String,

    /// Currently-active topic tags (e.g. "rust-migration", "scheduler-
    /// debugging", "qwen3.5-eog-bug"). Useful for recipe routing and
    /// for the persona's own meta-cognitive forecast (§20 — "incoming
    /// message touches a topic I have deep context on").
    pub topic_tags: Vec<String>,

    /// Open questions the user has asked that haven't been resolved.
    /// Helps personas prioritize: an unanswered "should we use
    /// option A or B?" stays salient until someone addresses it.
    pub open_questions: Vec<String>,

    /// When this summary was last touched (extension or refinement).
    /// Stale summaries (>5 min in active conversation) need a refresh
    /// before being considered current.
    pub last_summarized_at: Option<SystemTime>,
}

impl ConversationSummary {
    /// Construct a fresh empty summary for a room. Filled in by the
    /// summarizer (background task) as messages flow.
    pub fn new(room_id: Uuid) -> Self {
        Self {
            room_id,
            turns_summarized: 0,
            arc_summary: String::new(),
            topic_tags: Vec::new(),
            open_questions: Vec::new(),
            last_summarized_at: None,
        }
    }

    /// True if this summary is empty (no consolidation has happened
    /// yet). New rooms / very-recent rooms hit this.
    pub fn is_empty(&self) -> bool {
        self.turns_summarized == 0 && self.arc_summary.is_empty()
    }

    /// Estimate the token cost of this summary in the model's context.
    /// Rough — ~4 chars/token. Enough for the budget arithmetic in
    /// the prompt assembler (§14 task seeds vs actual summary size).
    pub fn estimated_tokens(&self) -> u32 {
        let arc_chars = self.arc_summary.len();
        let tag_chars: usize = self.topic_tags.iter().map(|t| t.len() + 2).sum();
        let q_chars: usize = self.open_questions.iter().map(|q| q.len() + 2).sum();
        ((arc_chars + tag_chars + q_chars) / 4) as u32
    }

    /// True if the summary has fallen behind the current turn count by
    /// more than `max_lag` turns — the background updater should run.
    pub fn is_stale(&self, current_turns: u32, max_lag: u32) -> bool {
        current_turns.saturating_sub(self.turns_summarized) > max_lag
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: regression in the default mode (someone
    /// changes Default to Verbatim "just in case" and silently
    /// regresses every chat task to consume 10x more context).
    /// ConsolidatedSummary is the right default per §15.2 — verbatim
    /// is opt-in for tasks that genuinely need it.
    ///
    /// Validated 2026-04-21: changed Default impl to Verbatim, test
    /// fails clearly; reverted, passes.
    #[test]
    fn default_recall_mode_is_consolidated_summary() {
        assert_eq!(RecallMode::default(), RecallMode::ConsolidatedSummary);
    }

    /// What this catches: uses_summary returning the wrong boolean
    /// for any variant — would cause the prompt assembler to skip
    /// summary construction or waste effort building one that's not
    /// going to be used.
    ///
    /// Validated 2026-04-21: inverted the Verbatim case to true,
    /// test fails on Verbatim assertion; reverted.
    #[test]
    fn uses_summary_true_for_consolidated_and_hybrid_only() {
        assert!(RecallMode::ConsolidatedSummary.uses_summary());
        assert!(RecallMode::Hybrid { verbatim_window: 5 }.uses_summary());
        assert!(!RecallMode::Verbatim.uses_summary());
    }

    /// What this catches: verbatim_window_size returning the wrong
    /// number per mode. ConsolidatedSummary keeps the last 2 messages
    /// verbatim (current + last reply); Hybrid honors its declared
    /// window; Verbatim wants everything (u32::MAX).
    ///
    /// Validated 2026-04-21: changed ConsolidatedSummary to return 0
    /// (would suppress the most-recent message), test fails clearly;
    /// reverted.
    #[test]
    fn verbatim_window_size_matches_mode_semantics() {
        assert_eq!(RecallMode::ConsolidatedSummary.verbatim_window_size(), 2);
        assert_eq!(
            RecallMode::Hybrid { verbatim_window: 5 }.verbatim_window_size(),
            5
        );
        assert_eq!(
            RecallMode::Hybrid {
                verbatim_window: 10
            }
            .verbatim_window_size(),
            10
        );
        assert_eq!(RecallMode::Verbatim.verbatim_window_size(), u32::MAX);
    }

    /// What this catches: ConversationSummary::new not initializing
    /// fields properly — would lead to "looks-empty-but-isn't" bugs
    /// where is_empty returns wrong answer.
    ///
    /// Validated 2026-04-21: forced turns_summarized=99 in new(),
    /// test fails on is_empty=false; reverted.
    #[test]
    fn new_conversation_summary_is_empty_and_zero_turns() {
        let room = Uuid::new_v4();
        let s = ConversationSummary::new(room);
        assert_eq!(s.room_id, room);
        assert_eq!(s.turns_summarized, 0);
        assert!(s.arc_summary.is_empty());
        assert!(s.topic_tags.is_empty());
        assert!(s.open_questions.is_empty());
        assert!(s.is_empty());
    }

    /// What this catches: estimated_tokens off-by-byte (using bytes
    /// instead of chars / wrong divisor). Prompt assembler uses this
    /// to decide if the summary fits the persona's task budget; wrong
    /// estimate = wrong budgeting.
    ///
    /// Validated 2026-04-21: used arc_chars * 4 instead of / 4, test
    /// fails because estimate is 16x reality; reverted.
    #[test]
    fn estimated_tokens_approximates_at_4_chars_per_token() {
        let mut s = ConversationSummary::new(Uuid::nil());
        s.arc_summary = "x".repeat(400); // 400 chars / 4 = 100 tokens
        assert_eq!(s.estimated_tokens(), 100);

        s.topic_tags = vec!["rust".to_string(), "scheduler".to_string()];
        // arc=400 + tags=("rust"+2 + "scheduler"+2 = 17) = 417 / 4 = 104
        assert_eq!(s.estimated_tokens(), 104);
    }

    /// What this catches: is_stale boundary errors. The background
    /// updater triggers based on this; wrong threshold = either
    /// constant retraining (too eager) or stale summaries (too lazy).
    ///
    /// Validated 2026-04-21: changed > to >=, test fails on the
    /// equal-to-max case; reverted.
    #[test]
    fn is_stale_triggers_only_when_lag_exceeds_max() {
        let s = ConversationSummary {
            turns_summarized: 10,
            ..ConversationSummary::new(Uuid::nil())
        };
        // current=12, lag=2, max=2 — at the threshold, NOT stale
        assert!(!s.is_stale(12, 2));
        // current=13, lag=3, max=2 — over threshold, IS stale
        assert!(s.is_stale(13, 2));
        // current=10, no lag, NOT stale
        assert!(!s.is_stale(10, 2));
        // current<turns_summarized (impossible in practice but defensive):
        // saturating_sub returns 0, never stale
        assert!(!s.is_stale(5, 2));
    }
}
