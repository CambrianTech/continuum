//! A turn's transitions inside the engine, each recorded once: admitted to the driver's queue, and
//! its first frame. Keyed by the submitted request's id, so a stall reads as the last transition it
//! reached (the M5, 2026-11-21: Aris entered `generate_text` at 00:48:32 and dropped 1,499 s later
//! with no record of whether the engine ever held her turn).

use super::RequestId;

/// Where a turn has reached.
pub(super) enum At {
    /// The driver took her turn into its queue.
    Admitted {
        /// Turns in the queue, hers included.
        queue_depth: usize,
        prompt_tokens: usize,
        /// Since her turn was submitted to the scheduler.
        since_submit_ms: u64,
    },
    /// Her turn advanced for the first time.
    FirstFrame {
        /// Since the driver admitted her turn.
        waited_ms: u64,
        /// Prompt tokens her blocks already held: known once she is bound.
        reused_tokens: usize,
        prompt_tokens: usize,
        /// Whether reaching her took a switch from another mind.
        switched: bool,
    },
}

/// One transition of one turn.
pub(super) struct Transition {
    pub request: Option<RequestId>,
    pub persona: uuid::Uuid,
    pub one_off: bool,
    pub at: At,
}

impl Transition {
    pub(super) fn write(self) {
        match self.at {
            At::Admitted { queue_depth, prompt_tokens, since_submit_ms } => continuum_base::probe!(
                class = "persona.engine.admitted",
                request = ?self.request,
                persona = %self.persona,
                one_off = self.one_off,
                queue_depth = queue_depth,
                prompt_tokens = prompt_tokens,
                since_submit_ms = since_submit_ms,
                "the driver took a turn into its queue"
            ),
            At::FirstFrame { waited_ms, reused_tokens, prompt_tokens, switched } => continuum_base::probe!(
                class = "persona.engine.first_frame",
                request = ?self.request,
                persona = %self.persona,
                one_off = self.one_off,
                waited_ms = waited_ms,
                reused_tokens = reused_tokens,
                prompt_tokens = prompt_tokens,
                switched = switched,
                "a turn advanced for the first time"
            ),
        }
    }
}
