use super::*;
use continuum_flow::{spawn_stage, Capacity, Qos, Stage};
use std::num::NonZeroUsize;

/// What the test stages take: an item, or a record stage's barrier.
enum Fact {
    Item,
    Barrier(oneshot::Sender<()>),
}

/// A record stage: answers each barrier once everything before it is handled.
struct Answers;

impl Stage for Answers {
    type Input = Fact;
    const QOS: Qos = Qos::Utility;
    async fn handle(&mut self, fact: Fact) {
        if let Fact::Barrier(written) = fact {
            assert!(written.send(()).is_ok(), "the settle awaits its barrier");
        }
    }
}

/// An Emit sink whose caller stopped reading: it never finishes handling what it took.
struct Stalls;

impl Stage for Stalls {
    type Input = Fact;
    const QOS: Qos = Qos::Utility;
    async fn handle(&mut self, _fact: Fact) {
        std::future::pending::<()>().await;
    }
}

fn one() -> Capacity {
    Capacity::granted(NonZeroUsize::MIN)
}

// what this catches: a stop acknowledged as fully settled when the record stage ended before
// confirming its last records (Codex on 8e597e1bcd), and a healthy stop misread as unconfirmed.
#[tokio::test]
async fn records_read_written_only_when_the_record_stage_confirms_them() {
    let (emit_input, _emit) = spawn_stage(Answers, one());
    let mut emit = Outbox::new(emit_input);

    let (record_input, _record) = spawn_stage(Answers, one());
    let mut record = Outbox::new(record_input);
    record.send(Fact::Item);
    assert_eq!(settle(&mut emit, &mut record, Fact::Barrier).await, Records::Written);

    let (stopped_input, stopped) = spawn_stage(Answers, one());
    stopped.stop();
    let mut stopped_record = Outbox::new(stopped_input);
    assert_eq!(settle(&mut emit, &mut stopped_record, Fact::Barrier).await, Records::Unconfirmed, "a record stage that ended cannot confirm");
}

// what this catches: a stop held by a caller that stopped reading her answer (Codex on 8e597e1bcd:
// the settle joined Emit's drain), with output still queued behind the stalled sink.
#[tokio::test]
async fn a_stalled_emit_sink_never_holds_the_stop() {
    let (emit_input, _emit) = spawn_stage(Stalls, one());
    let mut emit = Outbox::new(emit_input);
    for _ in 0..4 {
        emit.send(Fact::Item);
    }
    assert!(emit.waiting() > 0, "output is queued behind the stalled sink");

    let (record_input, _record) = spawn_stage(Answers, one());
    let mut record = Outbox::new(record_input);
    assert_eq!(settle(&mut emit, &mut record, Fact::Barrier).await, Records::Written);
    assert!(emit.waiting() > 0, "the stalled sink's output is left for its caller, not awaited");
}
