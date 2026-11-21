//! A turn's transitions inside the engine, each recorded once: admitted to the driver's queue, its
//! first frame started, and its first successful native progress. Keyed by the submitted request's
//! id and the engine's incarnation, so a stall reads as the last transition it reached, on which
//! engine (the M5, 2026-11-21: Aris entered `generate_text` at 00:48:32 and dropped 1,499 s later
//! with no record of whether the engine ever held her turn).

use super::RequestId;

/// Where a turn has reached.
pub(super) enum At {
    /// The driver took her turn into its queue.
    Admitted {
        /// Turns in the queue, hers included.
        queue_depth: usize,
        prompt_tokens: usize,
        /// Since her turn was handed to the scheduler.
        since_submit_ms: u64,
    },
    /// Her first frame began: she is bound, and her prefill's start is known. It may still block or
    /// fail; [`At::FirstProgress`] is the proof it advanced.
    FrameStarted {
        /// Since the driver admitted her turn.
        since_admit_ms: u64,
        /// Prompt tokens her blocks already held.
        reused_tokens: usize,
        prompt_tokens: usize,
        /// Whether reaching her took a switch from another mind.
        switched: bool,
    },
    /// Her turn ended without an answer: refused, failed, or her caller hung up. Every admitted turn
    /// ends in exactly one of this or `persona.engine.turn`.
    Refused {
        /// Since the driver admitted her turn.
        since_admit_ms: u64,
        /// Why, as the driver's typed error states it.
        reason: String,
    },
    /// Her first step that succeeded natively: a prefill chunk landed, or a token decoded.
    FirstProgress {
        /// Since the driver admitted her turn.
        since_admit_ms: u64,
        /// How long that step took.
        step_ms: u64,
        /// Whether that step was a prefill chunk (otherwise a decoded token).
        prefill: bool,
    },
}

/// How far a turn's once-only transitions have been reported: progress implies a start.
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum Milestones {
    #[default]
    Admitted,
    Started,
    Progressed,
}

impl Milestones {
    /// Her first frame begins: true the first time only.
    pub(super) fn start(&mut self) -> bool {
        let first = *self == Self::Admitted;
        if first {
            *self = Self::Started;
        }
        first
    }

    /// A step ended, `succeeded` or not: true only for the first that succeeded.
    pub(super) fn step(&mut self, succeeded: bool) -> bool {
        let first = succeeded && *self != Self::Progressed;
        if first {
            *self = Self::Progressed;
        }
        first
    }
}

/// One transition of one turn.
pub(super) struct Transition {
    pub request: Option<RequestId>,
    pub persona: uuid::Uuid,
    pub one_off: bool,
    pub at: At,
}

impl Transition {
    /// Write it, tagged with the incarnation of the engine that served it.
    pub(super) fn write(self, incarnation: u64) {
        // The same shape the adapter logs (`Some("…")`), so the two ends join exactly.
        let request = self.request.as_ref().map(RequestId::as_str);
        match self.at {
            At::Admitted { queue_depth, prompt_tokens, since_submit_ms } => continuum_base::probe!(
                class = "persona.engine.admitted",
                request = ?request,
                incarnation = incarnation,
                persona = %self.persona,
                one_off = self.one_off,
                queue_depth = queue_depth,
                prompt_tokens = prompt_tokens,
                since_submit_ms = since_submit_ms,
                "the driver took a turn into its queue"
            ),
            At::FrameStarted { since_admit_ms, reused_tokens, prompt_tokens, switched } => continuum_base::probe!(
                class = "persona.engine.frame_started",
                request = ?request,
                incarnation = incarnation,
                persona = %self.persona,
                one_off = self.one_off,
                since_admit_ms = since_admit_ms,
                reused_tokens = reused_tokens,
                prompt_tokens = prompt_tokens,
                switched = switched,
                "a turn's first frame began"
            ),
            At::Refused { since_admit_ms, reason } => continuum_base::probe!(
                class = "persona.engine.refused",
                request = ?request,
                incarnation = incarnation,
                persona = %self.persona,
                one_off = self.one_off,
                since_admit_ms = since_admit_ms,
                reason = %reason,
                "a turn ended without an answer"
            ),
            At::FirstProgress { since_admit_ms, step_ms, prefill } => continuum_base::probe!(
                class = "persona.engine.first_progress",
                request = ?request,
                incarnation = incarnation,
                persona = %self.persona,
                one_off = self.one_off,
                since_admit_ms = since_admit_ms,
                step_ms = step_ms,
                prefill = prefill,
                "a turn's first step succeeded natively"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (Codex, review of d3f24526ab): a first frame that blocks or fails reported as
    // progress, and progress reported more than once on a turn of many chunks.
    #[test]
    fn a_failed_first_step_starts_without_progress_and_progress_is_reported_once() {
        let mut failed = Milestones::default();
        assert!(failed.start(), "her first frame begins");
        assert!(!failed.step(false), "a failed first chunk is not progress");
        assert!(!failed.start(), "the start is reported once");

        let mut turn = Milestones::default();
        assert!(turn.start());
        let reported: Vec<bool> = [true, true, true].into_iter().map(|ok| turn.step(ok)).collect();
        assert_eq!(reported, [true, false, false], "a multi-chunk turn reports its first progress once");
    }
}
