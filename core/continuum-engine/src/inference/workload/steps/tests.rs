#![cfg(test)]
//! The step-stream reader against a fake engine publishing `/engine/steps`.
use std::num::NonZeroUsize;

use continuum_base::capture::{Missing, Ring};

use super::*;

const ENGINE: EngineIncarnation = EngineIncarnation { pid: 7, started_s: 1_762_300_000, port: 8_085 };
const HER: &str = "00000000-0000-0000-0000-000000000001";

/// A fake engine serving `frames` as one SSE stream, then closing it.
async fn engine(frames: Vec<String>) -> (String, tokio::task::JoinHandle<std::io::Result<()>>) {
    let body = frames.into_iter().map(|frame| format!("data: {frame}\n\n")).collect::<String>();
    let app = axum::Router::new().route(
        "/engine/steps",
        axum::routing::get(move || {
            let body = body.clone();
            async move { ([("content-type", "text/event-stream")], format!(":\n\n{body}")) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind"); // test: loopback
    let root = format!("http://{}", listener.local_addr().expect("test: addr")); // test: a bound socket has one
    (root, tokio::spawn(async move { axum::serve(listener, app).await }))
}

fn source(root: &str) -> StepSource<'_> {
    StepSource { root, engine: ENGINE, capacity: NonZeroUsize::new(64).expect("test: a literal above zero") } // test: literal
}

fn ring() -> Ring<WorkloadEvent> {
    Ring::new(NonZeroUsize::new(64).expect("test: a literal above zero")) // test: literal
}

// what this catches: the engine's records reaching the capture out of their order or spacing, or as
// anything but the typed events the replay reads; a keepalive is not a record.
#[tokio::test]
async fn a_step_stream_lands_as_typed_events_in_the_order_and_spacing_they_happened() {
    let frames = vec![
        r#"{"kind":"opened","origin_us":1000}"#.to_string(),
        format!(r#"{{"kind":"sequence","seq":0,"at_us":1000,"task":5,"mind":"{HER}","change":{{"arrived":{{"resident":0,"fresh":12}}}}}}"#),
        format!(r#"{{"kind":"sequence","seq":1,"at_us":1000,"task":5,"mind":"{HER}","change":"ready"}}"#),
        r#"{"kind":"began","seq":2,"at_us":3000,"step":1,"chosen":[5]}"#.to_string(),
        r#"{"kind":"ended","seq":3,"at_us":8000,"step":1,"work":[{"task":5,"depth":12,"prefilled":12,"decoded":1,"accepted_draft":0}]}"#.to_string(),
    ];
    let (root, server) = engine(frames).await;
    let sink = ring();
    let end = record_steps(&reqwest::Client::new(), source(&root), &sink).await;
    assert!(matches!(end, StepStreamEnd::Closed), "{end:?}");
    let interval = sink.drain();
    assert_eq!(interval.missing, Missing::default());
    let kinds: Vec<_> = interval.events.iter().map(|s| std::mem::discriminant(&s.event)).collect();
    assert_eq!(kinds.len(), 4);
    assert!(matches!(interval.events[2].event, WorkloadEvent::StepBegan { ref chosen, .. } if chosen == &vec![SequenceId(5)]));
    let gap = interval.events[3].at.since_origin() - interval.events[2].at.since_origin();
    assert_eq!(gap, Duration::from_millis(5), "the engine's 5 ms between begin and end survives the mapping");
    server.abort();
}

// what this catches: loss on the engine's side read as a whole capture. A `missed` record and a
// skipped sequence number both reach the sink as loss, so that interval cannot gate.
#[tokio::test]
async fn engine_side_loss_and_a_sequence_gap_void_the_interval() {
    let frames = vec![
        r#"{"kind":"opened","origin_us":0}"#.to_string(),
        r#"{"kind":"began","seq":0,"at_us":10,"step":1,"chosen":[]}"#.to_string(),
        r#"{"kind":"missed","seq":1,"count":3}"#.to_string(),
        r#"{"kind":"ended","seq":4,"at_us":20,"step":1,"work":[]}"#.to_string(),
    ];
    let (root, server) = engine(frames).await;
    let sink = ring();
    record_steps(&reqwest::Client::new(), source(&root), &sink).await;
    let interval = sink.drain();
    assert_eq!(interval.missing.dropped, 3 + 2, "three the engine named, two skipped sequence numbers");
    assert!(interval.complete().is_err());
    server.abort();
}

// what this catches: a stream that never states its clock origin mapped against a guessed one.
#[tokio::test]
async fn a_stream_without_its_origin_ends_unread() {
    let (root, server) = engine(vec![r#"{"kind":"began","seq":0,"at_us":10,"step":1,"chosen":[]}"#.to_string()]).await;
    let end = record_steps(&reqwest::Client::new(), source(&root), &ring()).await;
    assert!(matches!(end, StepStreamEnd::NoOrigin), "{end:?}");
    server.abort();
}
