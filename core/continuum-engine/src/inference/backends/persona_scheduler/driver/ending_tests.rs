use super::*;
use continuum_sage::{spawn_stage, Capacity, Qos, Stage};
use std::num::NonZeroUsize;

/// What the test stages take: an item, or a record stage's barrier.
enum Fact {
    Item,
    /// An answer's output, numbered in the order it was sent.
    Numbered(u32),
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

// what this catches: a stop acknowledged as settled when the record stage ended before
// confirming its last records (Codex on 8e597e1bcd), and a healthy stop misread as unconfirmed.
#[tokio::test]
async fn records_read_processed_only_when_the_record_stage_confirms_them() {
    let (emit_input, _emit) = spawn_stage(Answers, one());
    let mut emit = Outbox::new(emit_input);

    let (record_input, _record) = spawn_stage(Answers, one());
    let mut record = Outbox::new(record_input);
    record.send(Fact::Item);
    assert!(matches!(settle(&mut emit, &mut record, Fact::Barrier, &mut Reading::now()).await, Records::Processed { .. }));

    let (stopped_input, stopped) = spawn_stage(Answers, one());
    stopped.stop();
    let mut stopped_record = Outbox::new(stopped_input);
    assert_eq!(settle(&mut emit, &mut stopped_record, Fact::Barrier, &mut Reading::now()).await, Records::Unconfirmed, "a record stage that ended cannot confirm");
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
    assert!(matches!(settle(&mut emit, &mut record, Fact::Barrier, &mut Reading::now()).await, Records::Processed { .. }));
    assert!(emit.waiting() > 0, "the stalled sink's output is left for its caller, not awaited");
}

// what this catches (Codex on #6208 at 162fbe5fa7): a settle that reads its records written while the
// probe log failed to write them, and a failure counted again at the next settle.
#[tokio::test]
async fn a_settle_reports_the_log_writes_that_failed_in_its_window() {
    let (emit_input, _emit) = spawn_stage(Answers, one());
    let mut emit = Outbox::new(emit_input);
    let (record_input, _record) = spawn_stage(Answers, one());
    let mut record = Outbox::new(record_input);
    let mut seen = Reading::now();

    continuum_base::probe_writes::count_failure();
    let failed = settle(&mut emit, &mut record, Fact::Barrier, &mut seen).await;
    assert!(matches!(failed, Records::Processed { write_failures } if write_failures >= 1), "{failed:?}");
    assert_eq!(seen, Reading::now(), "the window moves to this settle, so the failure is not reported twice");
}

/// An Emit stage that hands each numbered output to its caller.
struct Delivers(tokio::sync::mpsc::UnboundedSender<u32>);

impl Stage for Delivers {
    type Input = Fact;
    const QOS: Qos = Qos::Utility;
    async fn handle(&mut self, fact: Fact) {
        if let Fact::Numbered(n) = fact {
            assert!(self.0.send(n).is_ok(), "the test reads every output");
        }
    }
}

// what this catches (Codex on #6208 at 162fbe5fa7): every submission handle dropped while Emit was
// full, and the answer ends still queued for it dropped with the driver, so a caller still reading
// never heard her turn end.
#[tokio::test]
async fn a_closed_engine_hands_a_full_emit_to_a_drain_owner_in_order() {
    let (delivered, mut caller) = tokio::sync::mpsc::unbounded_channel();
    let (emit_input, _emit) = spawn_stage(Delivers(delivered), one());
    let mut emit = Outbox::new(emit_input);
    for n in 0..8 {
        emit.send(Fact::Numbered(n));
    }
    assert!(emit.waiting() > 0, "Emit is full: outputs wait behind it");

    assert!(hand_off(emit, &tokio::runtime::Handle::current()).await.is_ok(), "the drain owner finishes");
    let mut heard = Vec::new();
    while heard.len() < 8 {
        let Some(output) = caller.recv().await else { break }; // a lost output ends the stream short, and fails below
        heard.push(output);
    }
    assert_eq!(heard, (0..8).collect::<Vec<_>>(), "in the order they were sent");
}

// what this catches (Codex on 8f386fb4e1): the driver thread closing Emit's intake right after a
// closing driver handed its full outbox to the drain owner, so ends the drain had not yet pushed
// came back refused and were dropped. Shutdown here is the thread's own: hand off, then `finish`.
#[tokio::test]
async fn shutdown_with_a_full_emit_delivers_every_end_before_the_stages_finish() {
    let (delivered, mut caller) = tokio::sync::mpsc::unbounded_channel();
    let (emit_input, emit_worker) = spawn_stage(Delivers(delivered), one());
    let (record_input, record_worker) = spawn_stage(Answers, one());
    let mut emit = Outbox::new(emit_input);
    for n in 0..8 {
        emit.send(Fact::Numbered(n));
    }
    assert!(emit.waiting() > 0, "Emit is full: ends wait behind it");
    drop(record_input); // the closed driver's record outbox, already settled

    let _drain = hand_off(emit, &tokio::runtime::Handle::current());
    let (emit_ended, record_ended) = finish(emit_worker, record_worker).await;
    assert!(emit_ended.is_ok() && record_ended.is_ok(), "both stages end on their own");
    let mut heard = Vec::new();
    while let Ok(output) = caller.try_recv() {
        heard.push(output);
    }
    assert_eq!(heard, (0..8).collect::<Vec<_>>(), "every end reached its caller, in order, before the stages finished");
}
