//! How an admitted turn leaves the engine. One owner (`Driver::end`) ends every admitted turn, so
//! each ends in exactly one terminal record: its turn row when answered, else
//! `persona.engine.refused` with why (Codex on ce8a894e93: a stop and a closed submission channel
//! ended turns with no record). A stop is acknowledged only once those records are written.

use super::{emit, record, report, transition, Driver, Suspended, TokenEvent, Turn, TurnError};

/// A stop's suspend, awaiting its acknowledgement.
pub(super) struct Stop {
    pub(super) reply: tokio::sync::oneshot::Sender<Suspended>,
    pub(super) suspended: Suspended,
}

/// How an admitted turn left the engine.
pub(super) enum Ending {
    /// She answered: EOG, a stop sequence, or max_tokens.
    Answered { tokens_generated: usize, elapsed_ms: u64 },
    /// A step was refused or failed, or her caller hung up.
    Refused(TurnError),
    /// The engine is stopping: what her blocks computed stays hers.
    Stopping,
    /// Every submission handle was dropped while her turn was still admitted.
    Closed,
}

impl Ending {
    /// Why an unanswered turn ended, as its caller and its record read it.
    fn reason(&self) -> String {
        match self {
            Self::Answered { .. } => "answered".into(),
            Self::Refused(refused) => refused.to_string(),
            Self::Stopping => "the engine is stopping: her state is preserved".into(),
            Self::Closed => "every submission handle was dropped".into(),
        }
    }
}

impl Driver<'_, '_> {
    /// End `turn`: the one place an admitted turn leaves the engine, so each ends in exactly one
    /// terminal record (its turn row when answered, else `persona.engine.refused`), its one-off
    /// mind released, and its caller told.
    pub(super) fn end(&mut self, turn: Turn, ending: Ending) {
        let event = match ending {
            Ending::Answered { tokens_generated, elapsed_ms } => {
                self.record.send(record::Fact::Turn(Self::row(&turn, tokens_generated)));
                TokenEvent::Done { tokens_generated, elapsed_ms }
            }
            unanswered => {
                let reason = unanswered.reason();
                self.transition(&turn, transition::At::Refused { since_admit_ms: record::whole_ms(turn.submitted.elapsed()), reason: reason.clone() });
                TokenEvent::Error(reason)
            }
        };
        self.finish_one_off(&turn);
        self.emit.send(emit::Out::End { answer: turn.answer, event });
    }

    /// Acknowledge a stop once every record of the turns it ended is written.
    pub(super) fn acknowledge(&mut self, stop: Stop, stages_on: &tokio::runtime::Handle) {
        self.settle_records(stages_on);
        report(stop.reply, stop.suspended);
    }

    /// Every fact sent so far is written and every answer delivered: both outboxes drained, then the
    /// record stage's barrier answered (it is FIFO). Before a stop is acknowledged, or the loop ends.
    pub(super) fn settle_records(&mut self, stages_on: &tokio::runtime::Handle) {
        self.flush();
        let (written, barrier) = tokio::sync::oneshot::channel();
        self.record.send(record::Fact::Barrier(written));
        stages_on.block_on(async {
            self.drain().await;
            if barrier.await.is_err() {
                continuum_base::probe!(class = "persona.engine.records_unsettled", "the record stage ended before writing a stop's last records");
            }
        });
    }
}
