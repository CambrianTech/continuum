//! How an admitted turn leaves the engine. One owner (`Driver::end`) ends every admitted turn, so
//! each ends in exactly one terminal record: its turn row when answered, else
//! `persona.engine.refused` with why (Codex on ce8a894e93: a stop and a closed submission channel
//! ended turns with no record). A stop is acknowledged only once those records are written.

use continuum_flow::Outbox;
use tokio::sync::oneshot;

use super::super::Records;
use super::{emit, record, report, transition, Driver, Suspended, TokenEvent, Turn, TurnError};

/// A stop's suspend, awaiting its acknowledgement.
pub(super) struct Stop {
    pub(super) reply: tokio::sync::oneshot::Sender<Suspended>,
    pub(super) suspension: Suspension,
}

/// What a stop's suspend did natively: minds preserved, and every mind it could not preserve, with
/// why. Its acknowledgement adds whether the records of the turns it ended were written.
pub(super) struct Suspension {
    pub(super) preserved: usize,
    pub(super) retained: Vec<(uuid::Uuid, String)>,
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

    /// Acknowledge a stop: its native suspend result as it is, with whether the records of the turns
    /// it ended were confirmed written.
    pub(super) fn acknowledge(&mut self, stop: Stop, stages_on: &tokio::runtime::Handle) {
        let records = self.settle_records(stages_on);
        let Suspension { preserved, retained } = stop.suspension;
        report(stop.reply, Suspended { records, preserved, retained });
    }

    /// Settle what a stop must not lose before it is acknowledged, or the loop ends ([`settle`]).
    pub(super) fn settle_records(&mut self, stages_on: &tokio::runtime::Handle) -> Records {
        stages_on.block_on(settle(&mut self.emit, &mut self.record, record::Fact::Barrier))
    }
}

/// Every record sent so far is written, confirmed by the record stage answering a barrier it reaches
/// after them (it is FIFO). Emit is flushed but never awaited: a caller that stopped reading her answer
/// never holds the stop (Codex on 8e597e1bcd). A record stage that ended first reads `Unconfirmed`.
pub(super) async fn settle<E, R>(emit: &mut Outbox<E>, record: &mut Outbox<R>, barrier: impl FnOnce(oneshot::Sender<()>) -> R) -> Records {
    emit.flush();
    let (written, answered) = oneshot::channel();
    record.send(barrier(written));
    record.drain().await;
    if answered.await.is_ok() {
        Records::Written
    } else {
        continuum_base::probe!(class = "persona.engine.records_unconfirmed", "the record stage ended before confirming a stop's last records");
        Records::Unconfirmed
    }
}

#[cfg(test)]
#[path = "ending_tests.rs"]
mod tests;
