//! A DROPPED GENERATION IS A COUNTED OUTCOME, NEVER A SILENT ONE (card ebce2ba0).
//!
//! Joel, 2026-09-20: "we need to better know this too when you find it, need safeguards,
//! I'm tired of limping around."
//!
//! ## What was silent
//!
//! A generation the awaiting side abandons leaves ONE trace today: a `cancelled` row in
//! the persona's prompt capture whose error reads `"request future dropped before a
//! terminal response"` — written by `prompt_capture::CaptureLease`'s `Drop`, which is the
//! only thing left running when a future is torn down. Nothing counts it, nothing says
//! WHICH bound reaped it or what the bound was derived from, and nothing says how much
//! output existed at the moment it was thrown away. Fifty submitted generations on the M5
//! the night this landed: 13 completed, 18 cancelled, 7 failed — a 26% completion rate
//! that read on the hour line as citizens who would not work.
//!
//! ## The two classes, told apart
//!
//! - **Never dispatched** — the mind was abandoned at an admission gate, before the model
//!   ever saw the request. Already counted as `lane_starved`; this module only gives its
//!   capture row a NAMED reason ([`crate::cognition::prompt_capture::CaptureLease::abandon`])
//!   so it stops masquerading as a dropped generation.
//! - **Dropped in flight** — the model WAS generating and the awaiting side went away.
//!   That is what [`InFlight`] witnesses and what the hour line counts as `dropped`.
//!
//! ## The salvage seam, and its honest limit
//!
//! [`InFlight`] holds the streaming receiver the non-observing generation path used to
//! throw away. When the future is torn down the adapter's own future (holding the sender)
//! drops first; this witness drops next, drains what the model had already produced, and
//! says it: answer chars, reasoning chars, prefill progress, and the tool calls ALREADY
//! PARSEABLE in the partial text ([`crate::ai::json_in_prompt_tools::parse_tool_calls`]).
//!
//! It REPORTS the loss; it does not re-enter the turn with it. `Drop` is synchronous and
//! the settle loop that would execute those calls is itself being torn down at that
//! instant — there is no seam to hand them back to. Naming them is the difference between
//! "a turn produced nothing" and "a turn produced a `write_file` call we threw away."

use crate::ai::adapter::GenerationChunk;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedReceiver;

/// What existed of a generation at the moment it was dropped.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct PartialOutput {
    /// Chars of user-facing answer the model had already decoded.
    pub answer_chars: usize,
    /// Chars of private reasoning (`<think>`) it had already decoded.
    pub reasoning_chars: usize,
    /// The last prefill progress the slot reported: tokens ingested of the prompt total.
    /// `total == 0` = the stream never reported prefill (a non-incremental adapter).
    pub prefill_processed: u64,
    pub prefill_total: u64,
    /// Tool calls already COMPLETE in the partial answer — the hands this drop threw
    /// away, by name.
    pub tool_calls: Vec<String>,
}

/// PURE: fold the chunks a dropped stream had already delivered into the salvage report.
/// Separated from [`InFlight`] so the fold is testable without a runtime or an adapter.
pub(crate) fn partial_from_chunks(
    chunks: impl IntoIterator<Item = GenerationChunk>,
) -> PartialOutput {
    let mut answer = String::new();
    let mut out = PartialOutput::default();
    for chunk in chunks {
        match chunk {
            GenerationChunk::Token(text) => answer.push_str(&text),
            GenerationChunk::Reasoning(text) => out.reasoning_chars += text.chars().count(),
            GenerationChunk::Prefill {
                processed, total, ..
            } => {
                out.prefill_processed = processed;
                out.prefill_total = total;
            }
        }
    }
    out.answer_chars = answer.chars().count();
    // Only COMPLETE calls parse — a half-emitted envelope yields nothing, which is the
    // honest answer ("she was still writing it"), not a fabricated partial call.
    out.tool_calls = crate::ai::json_in_prompt_tools::parse_tool_calls(&answer)
        .into_iter()
        .map(|call| call.name)
        .collect();
    out
}

/// An in-flight generation that REPORTS ITSELF if the awaiting side goes away.
///
/// Armed around the model call; [`InFlight::disarm`] on every terminal answer (a response
/// OR a typed adapter error — both reached the model). Anything else is a drop, and `Drop`
/// is the only code that still runs when a future is torn down, so `Drop` is where the
/// receipt is written.
pub(crate) struct InFlight {
    persona: String,
    /// The bound this turn STATED for itself on the wire — her measured expectation with
    /// headroom ([`crate::inference::turn_bound::from_expectation`]). A generation dropped
    /// while still inside it was healthy work, and the bound that reaped it (an outer act
    /// deadline, a caller that walked away) was the undersized one.
    turn_bound: Option<Duration>,
    started: Instant,
    /// The stream, when this path owned one. `None` = the caller took the chunks (a live
    /// Speak), so this witness cannot see the output and says so rather than reporting 0.
    chunks: Option<UnboundedReceiver<GenerationChunk>>,
    armed: bool,
}

impl InFlight {
    /// Arm around a model call. `chunks` is the receiving half of the sink handed to the
    /// adapter, when this caller owns it.
    pub fn arm(
        persona: &str,
        turn_bound: Option<Duration>,
        chunks: Option<UnboundedReceiver<GenerationChunk>>,
    ) -> Self {
        Self {
            persona: persona.to_string(),
            turn_bound,
            started: Instant::now(),
            chunks,
            armed: true,
        }
    }

    /// The generation reached a terminal answer — a response, or a typed adapter refusal.
    /// Both are outcomes the substrate can read; neither is a drop.
    pub fn disarm(&mut self) {
        if std::mem::take(&mut self.armed) {
            crate::modules::citizen_health::note_generation_outcome(false);
        }
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        if !std::mem::take(&mut self.armed) {
            return;
        }
        crate::modules::citizen_health::note_generation_outcome(true);
        let observed = self.chunks.is_some();
        let partial = match self.chunks.take() {
            Some(mut rx) => {
                let mut chunks = Vec::new();
                // The adapter's future (and its sender) dropped before this witness, so
                // the channel is closed and `try_recv` drains it without blocking.
                while let Ok(chunk) = rx.try_recv() {
                    chunks.push(chunk);
                }
                partial_from_chunks(chunks)
            }
            None => PartialOutput::default(),
        };
        let (bound, source) = crate::inference::turn_bound::effective_bound_with_source(
            Duration::ZERO,
            self.turn_bound,
        );
        let lost_names = partial.tool_calls.join(",");
        crate::probe!(
            class = "persona.generation.dropped",
            persona = %self.persona,
            at = "generation",
            elapsed_secs = self.started.elapsed().as_secs(),
            // The turn's OWN bound and where it came from. `floor` here means the turn
            // stated no bound at all (her first turn, or an unmeasured box) — then nothing
            // above her sized its wait from her work, which is its own defect.
            bound_secs = bound.as_secs(),
            bound_source = source.as_str(),
            expected_secs = self
                .turn_bound
                .map(|b| crate::inference::turn_bound::expectation_behind(b).as_secs())
                .unwrap_or(0), // unwrap_or: 0 = no measured expectation existed to derive from
            // What was thrown away. `observed = false` = this path handed the stream to
            // the caller, so the counts are unknown, NOT zero.
            observed = observed,
            answer_chars = partial.answer_chars as u64,
            reasoning_chars = partial.reasoning_chars as u64,
            prefill_processed = partial.prefill_processed,
            prefill_total = partial.prefill_total,
            tool_calls_lost = partial.tool_calls.len() as u64,
            tool_calls = %lost_names,
            "a generation was DROPPED in flight — the awaiting side went away while the model was still producing; if elapsed is inside the turn's own bound, the bound that reaped it is undersized"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the salvage report is the difference between "she produced
    // nothing" and "she produced a tool call we threw away". A drop that reports 0 chars
    // for a stream that had delivered thousands sends the hunt at the persona instead of
    // the pipe — which is exactly how a 36% loss rate read as lazy citizens.
    #[test]
    fn a_partial_generation_reports_its_answer_reasoning_prefill_and_the_tool_calls_lost() {
        let partial = partial_from_chunks([
            GenerationChunk::Prefill {
                processed: 27_547,
                total: 30_000,
                cached: 0,
            },
            GenerationChunk::Reasoning("I should read the file first.".to_string()),
            GenerationChunk::Token("{\"tool_call\":{\"name\":\"read_file\",".to_string()),
            GenerationChunk::Token("\"arguments\":{\"path\":\"a.rs\"}}}".to_string()),
        ]);
        assert_eq!(partial.prefill_processed, 27_547);
        assert_eq!(partial.prefill_total, 30_000);
        assert_eq!(partial.reasoning_chars, "I should read the file first.".chars().count());
        assert_eq!(partial.answer_chars, 62);
        assert_eq!(
            partial.tool_calls,
            vec!["read_file".to_string()],
            "a COMPLETE call in the partial text is named, so the drop says what it cost"
        );
    }

    // what this catches: a half-written envelope must not be reported as a lost call —
    // the report is evidence, and an invented call would send the reader hunting an
    // action that was never finished.
    #[test]
    fn a_half_emitted_envelope_reports_chars_but_no_tool_call() {
        let partial = partial_from_chunks([GenerationChunk::Token(
            "{\"tool_call\":{\"name\":\"write_fi".to_string(),
        )]);
        assert!(partial.answer_chars > 0);
        assert!(partial.tool_calls.is_empty());
    }

    // what this catches: an EMPTY stream is still a drop with a legible report — zero
    // chars is a fact (the lane never produced a token), not a missing measurement.
    #[test]
    fn an_empty_stream_folds_to_an_empty_report() {
        assert_eq!(partial_from_chunks(Vec::<GenerationChunk>::new()), PartialOutput::default());
    }
}
