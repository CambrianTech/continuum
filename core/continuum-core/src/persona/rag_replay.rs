//! ReplayRagSource — the replay side of the mechanic-shop primitives.
//!
//! Closes the capture→replay round-trip from slice 11
//! (`rag_capture.rs`). Reads captured `RagCaptureEvent`s and serves
//! them back through the `RagSource` trait. Drop-in replacement for
//! a live source when:
//!
//! - Replaying a captured production turn against an alternative
//!   model / scorer / budget preset for debugging
//! - Golden-trace regression tests — replay a corpus, assert the
//!   substrate's downstream behavior (prompt assembly, model
//!   response shape) hasn't changed
//! - Deterministic test fixtures — canned engram source for prompt-
//!   assembly tests (slice 12+)
//!
//! ### Doctrine alignment
//!
//! - [[persona-record-replay-is-a-product-requirement]] — long-
//!   standing requirement, now closed for the RAG layer
//! - [[substrate-is-a-good-citizen-on-the-host]] — exhausted
//!   replay returns `None` honestly rather than fabricating
//!   responses
//! - Persona-scoped: cross-persona calls return empty (defense
//!   in depth, same shape as `EngramSource` + `StubRagSource`)
//!
//! ### Limitations
//!
//! - Sequential replay only: returns deliveries in the order they
//!   were captured. If the live source served multiple `deliver`
//!   calls in a turn, the replay returns them in the same order.
//!   Random-access by some semantic key (e.g., "give me the
//!   delivery that matches THIS ctx") is slice 12+ territory.
//! - Continuation matching is by FIFO order, not by cursor
//!   equality. The replay assumes the caller exercises the source
//!   in the same shape that produced the capture. Good for golden-
//!   trace replay; not yet ideal for free-form interactive replay.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;

use crate::persona::rag_budget::{
    ContinuationCursor, RagContext, RagDelivery, RagSource, ResolutionPreference,
};
use crate::persona::rag_capture::RagCaptureEvent;

/// A read-only source that returns previously-captured deliveries
/// instead of computing fresh ones. Persona-bound at construction;
/// source_id pass-through.
pub struct ReplayRagSource {
    source_id: &'static str,
    persona_id: uuid::Uuid,
    /// Deliveries from `deliver()` calls — popped FIFO on each
    /// `deliver()` request.
    initial: Mutex<VecDeque<RagDelivery>>,
    /// Deliveries from `deliver_continuation()` calls — popped FIFO.
    continuations: Mutex<VecDeque<RagDelivery>>,
}

impl ReplayRagSource {
    /// Construct from a set of pre-built deliveries. `initial` are
    /// the ones returned from `deliver()`; `continuations` from
    /// `deliver_continuation()`. Useful for tests that don't want
    /// to round-trip through serde.
    pub fn from_deliveries(
        source_id: &'static str,
        persona_id: uuid::Uuid,
        initial: Vec<RagDelivery>,
        continuations: Vec<RagDelivery>,
    ) -> Self {
        Self {
            source_id,
            persona_id,
            initial: Mutex::new(initial.into()),
            continuations: Mutex::new(continuations.into()),
        }
    }

    /// Construct from a captured event stream. Filters by
    /// `source_id` and `persona_id`; events from other sources or
    /// other personas are dropped on the floor. Events with a
    /// `cursor` field set go into the continuation queue; events
    /// without go into the initial queue.
    pub fn from_captures(
        source_id: &'static str,
        persona_id: uuid::Uuid,
        events: impl IntoIterator<Item = RagCaptureEvent>,
    ) -> Self {
        let mut initial: Vec<RagDelivery> = Vec::new();
        let mut continuations: Vec<RagDelivery> = Vec::new();
        for event in events {
            if let RagCaptureEvent::SourceDelivered {
                source_id: captured_source_id,
                persona_id: captured_persona_id,
                cursor,
                delivery,
                ..
            } = event
            {
                if captured_source_id != source_id || captured_persona_id != persona_id {
                    continue;
                }
                if cursor.is_some() {
                    continuations.push(delivery);
                } else {
                    initial.push(delivery);
                }
            }
        }
        Self::from_deliveries(source_id, persona_id, initial, continuations)
    }

    /// How many deliveries remain in the initial queue. Useful for
    /// tests + harness assertions ("did we exhaust the trace?").
    pub fn remaining_initial(&self) -> usize {
        self.initial.lock().unwrap().len()
    }

    /// How many deliveries remain in the continuation queue.
    pub fn remaining_continuations(&self) -> usize {
        self.continuations.lock().unwrap().len()
    }
}

#[async_trait]
impl RagSource for ReplayRagSource {
    fn source_id(&self) -> &'static str {
        self.source_id
    }

    fn expand_command(&self) -> Option<&'static str> {
        // a replay reproduces a recorded delivery verbatim; there is no live 'more'.
        None
    }

    async fn deliver(
        &self,
        ctx: &RagContext,
        _budget: u32,
        _resolution: ResolutionPreference,
    ) -> RagDelivery {
        if ctx.persona_id != self.persona_id {
            return RagDelivery {
                source_id: self.source_id.to_string(),
                items: Vec::new(),
                tokens_used: 0,
                continuation: None,
                resolution_used: ResolutionPreference::Placeholder,
            };
        }
        match self.initial.lock().unwrap().pop_front() {
            Some(delivery) => delivery,
            None => RagDelivery {
                source_id: self.source_id.to_string(),
                items: Vec::new(),
                tokens_used: 0,
                continuation: None,
                resolution_used: ResolutionPreference::Placeholder,
            },
        }
    }

    async fn deliver_continuation(
        &self,
        ctx: &RagContext,
        cursor: ContinuationCursor,
        _budget: u32,
    ) -> Option<RagDelivery> {
        if ctx.persona_id != self.persona_id {
            return None;
        }
        if cursor.persona_id != self.persona_id {
            return None;
        }
        if cursor.source_id != self.source_id {
            return None;
        }
        self.continuations.lock().unwrap().pop_front()
    }
}

//=============================================================================
// JSONL READER — load captured events back from a file
//=============================================================================

/// Load captured events from a JSONL file. Returns the parsed events
/// in the order they appear in the file. Lines that fail to parse are
/// silently skipped + logged via tracing::warn — a corrupted line
/// shouldn't poison the rest of the trace (mechanic shop has to be
/// robust to torn writes, partial files, etc.).
///
/// Returns an empty Vec if the file is missing OR empty — caller
/// decides whether absence is an error (typically: missing trace =
/// "no replay available" = fall through to live source).
pub fn read_jsonl_captures(path: &Path) -> std::io::Result<Vec<RagCaptureEvent>> {
    let contents = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err),
    };
    let mut events = Vec::new();
    for (line_num, line) in contents.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<RagCaptureEvent>(line) {
            Ok(ev) => events.push(ev),
            Err(err) => {
                tracing::warn!(
                    line_num = line_num + 1,
                    error = %err,
                    path = %path.display(),
                    "rag replay: line failed to parse, skipping (torn write? partial file?)"
                );
            }
        }
    }
    Ok(events)
}

//=============================================================================
// TESTS
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::rag_budget::{RagDelivery, RagItem};
    use crate::persona::rag_capture::{
        InMemoryRagCaptureSink, JsonlRagCaptureSink, RagCaptureSink, RecordingRagSource,
    };
    use std::sync::Arc;
    use tempfile::TempDir;
    use uuid::Uuid;

    fn persona() -> Uuid {
        Uuid::parse_str("00000000-0000-0000-0000-000000000aaa").unwrap()
    }

    fn ctx() -> RagContext {
        RagContext::for_persona(persona(), 1_000_000)
    }

    fn item(text: &str, tokens: u32) -> RagItem {
        RagItem {
            content: text.to_string(),
            tokens,
            metadata: serde_json::json!({}),
        }
    }

    fn delivery(source_id: &str, items: Vec<RagItem>) -> RagDelivery {
        let tokens_used = items.iter().map(|i| i.tokens).sum();
        RagDelivery {
            source_id: source_id.to_string(),
            items,
            tokens_used,
            continuation: None,
            resolution_used: ResolutionPreference::Raw,
        }
    }

    // ---- ReplayRagSource direct construction ----

    #[tokio::test]
    async fn replay_returns_canned_delivery_on_deliver() {
        let canned = delivery("stub", vec![item("hello", 5)]);
        let source = ReplayRagSource::from_deliveries(
            "stub",
            persona(),
            vec![canned.clone()],
            Vec::new(),
        );
        let result = source.deliver(&ctx(), 100, ResolutionPreference::Raw).await;
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].content, "hello");
        // Queue is now exhausted.
        assert_eq!(source.remaining_initial(), 0);
    }

    #[tokio::test]
    async fn replay_exhausted_returns_empty_not_panic() {
        let source = ReplayRagSource::from_deliveries(
            "stub",
            persona(),
            Vec::new(),
            Vec::new(),
        );
        let result = source.deliver(&ctx(), 100, ResolutionPreference::Raw).await;
        assert_eq!(result.items.len(), 0);
        assert_eq!(result.resolution_used, ResolutionPreference::Placeholder);
    }

    #[tokio::test]
    async fn replay_cross_persona_ctx_returns_empty() {
        let canned = delivery("stub", vec![item("a", 5)]);
        let source = ReplayRagSource::from_deliveries(
            "stub",
            persona(),
            vec![canned],
            Vec::new(),
        );
        let other = Uuid::parse_str("00000000-0000-0000-0000-000000000bbb").unwrap();
        let result = source
            .deliver(
                &RagContext::for_persona(other, 1_000_000),
                100,
                ResolutionPreference::Raw,
            )
            .await;
        assert_eq!(result.items.len(), 0);
    }

    #[tokio::test]
    async fn replay_serves_deliveries_in_capture_order() {
        let d1 = delivery("stub", vec![item("first", 5)]);
        let d2 = delivery("stub", vec![item("second", 5)]);
        let source = ReplayRagSource::from_deliveries(
            "stub",
            persona(),
            vec![d1, d2],
            Vec::new(),
        );
        let r1 = source.deliver(&ctx(), 100, ResolutionPreference::Raw).await;
        let r2 = source.deliver(&ctx(), 100, ResolutionPreference::Raw).await;
        assert_eq!(r1.items[0].content, "first");
        assert_eq!(r2.items[0].content, "second");
    }

    #[tokio::test]
    async fn replay_continuation_pops_from_continuation_queue() {
        let canned_continuation = delivery("stub", vec![item("paged", 5)]);
        let source = ReplayRagSource::from_deliveries(
            "stub",
            persona(),
            Vec::new(),
            vec![canned_continuation],
        );
        let cursor = ContinuationCursor {
            persona_id: persona(),
            source_id: "stub".to_string(),
            opaque: serde_json::json!({ "next": 1 }),
        };
        let result = source
            .deliver_continuation(&ctx(), cursor, 100)
            .await
            .expect("continuation queue had one entry");
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].content, "paged");
        // Exhausted now.
        assert_eq!(source.remaining_continuations(), 0);
    }

    #[tokio::test]
    async fn replay_continuation_refuses_wrong_persona_cursor() {
        let canned = delivery("stub", vec![item("a", 5)]);
        let source = ReplayRagSource::from_deliveries(
            "stub",
            persona(),
            Vec::new(),
            vec![canned],
        );
        let other = Uuid::parse_str("00000000-0000-0000-0000-000000000bbb").unwrap();
        let alien_cursor = ContinuationCursor {
            persona_id: other,
            source_id: "stub".to_string(),
            opaque: serde_json::json!({}),
        };
        let result = source.deliver_continuation(&ctx(), alien_cursor, 100).await;
        assert!(result.is_none());
        // Queue NOT consumed.
        assert_eq!(source.remaining_continuations(), 1);
    }

    #[tokio::test]
    async fn replay_continuation_refuses_wrong_source_id_cursor() {
        let canned = delivery("stub", vec![item("a", 5)]);
        let source = ReplayRagSource::from_deliveries(
            "stub",
            persona(),
            Vec::new(),
            vec![canned],
        );
        let alien_cursor = ContinuationCursor {
            persona_id: persona(),
            source_id: "memories".to_string(),
            opaque: serde_json::json!({}),
        };
        let result = source.deliver_continuation(&ctx(), alien_cursor, 100).await;
        assert!(result.is_none());
        assert_eq!(source.remaining_continuations(), 1);
    }

    // ---- Capture → Replay roundtrip via InMemoryRagCaptureSink ----

    #[tokio::test]
    async fn capture_then_replay_via_in_memory_sink() {
        // Live source produces 2 items.
        let live = crate::persona::rag_budget::StubRagSource::new(
            "stub",
            persona(),
            vec![item("alpha", 5), item("beta", 5)],
        );
        let sink = Arc::new(InMemoryRagCaptureSink::new());
        let sink_dyn: Arc<dyn RagCaptureSink> = sink.clone();
        let recorder = RecordingRagSource::new(live, sink_dyn);

        // Two deliver calls — captures should accumulate.
        recorder.deliver(&ctx(), 8, ResolutionPreference::Raw).await; // packs 1 item
        recorder.deliver(&ctx(), 100, ResolutionPreference::Raw).await; // packs the rest

        // Now replay the captured events through ReplayRagSource.
        let captured = sink.events();
        let replay = ReplayRagSource::from_captures("stub", persona(), captured.into_iter());

        let first = replay.deliver(&ctx(), 999, ResolutionPreference::Raw).await;
        assert_eq!(first.items.len(), 1);
        assert_eq!(first.items[0].content, "alpha");

        let second = replay.deliver(&ctx(), 999, ResolutionPreference::Raw).await;
        assert_eq!(second.items.len(), 1);
        assert_eq!(second.items[0].content, "beta");

        // Trace exhausted now.
        let third = replay.deliver(&ctx(), 999, ResolutionPreference::Raw).await;
        assert_eq!(third.items.len(), 0);
    }

    // ---- JSONL reader ----

    #[test]
    fn read_jsonl_returns_events_in_file_order() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("trace.jsonl");
        let sink = JsonlRagCaptureSink::open(path.clone()).unwrap();
        // Write 3 distinct events.
        for i in 0..3 {
            sink.record(RagCaptureEvent::TurnEnd {
                captured_at_ms: i as u64,
                persona_id: persona(),
                turn_id: None,
            });
        }
        drop(sink);

        let events = read_jsonl_captures(&path).unwrap();
        assert_eq!(events.len(), 3);
        // Order preserved (sorted by captured_at_ms).
        let stamps: Vec<u64> = events
            .iter()
            .map(|e| match e {
                RagCaptureEvent::TurnEnd { captured_at_ms, .. } => *captured_at_ms,
                _ => 0,
            })
            .collect();
        assert_eq!(stamps, vec![0, 1, 2]);
    }

    #[test]
    fn read_jsonl_missing_file_is_empty_not_error() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("nonexistent.jsonl");
        let events = read_jsonl_captures(&path).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn read_jsonl_skips_malformed_lines() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("partial.jsonl");
        // Mix of valid + invalid lines (torn write simulation).
        let valid = serde_json::to_string(&RagCaptureEvent::TurnEnd {
            captured_at_ms: 42,
            persona_id: persona(),
            turn_id: None,
        })
        .unwrap();
        let mixed = format!("{valid}\nnot json at all\n{valid}\n");
        std::fs::write(&path, mixed).unwrap();
        let events = read_jsonl_captures(&path).unwrap();
        // 2 valid events survive; the garbage line is logged + skipped.
        assert_eq!(events.len(), 2);
    }

    // ---- Full JSONL roundtrip: record → JSONL → read → replay ----

    #[tokio::test]
    async fn full_jsonl_roundtrip_capture_then_replay() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("trace.jsonl");

        // Phase 1: capture
        {
            let live = crate::persona::rag_budget::StubRagSource::new(
                "stub",
                persona(),
                vec![item("hello", 5), item("world", 5)],
            );
            let sink: Arc<dyn RagCaptureSink> =
                Arc::new(JsonlRagCaptureSink::open(path.clone()).unwrap());
            let recorder = RecordingRagSource::new(live, sink);
            recorder.deliver(&ctx(), 100, ResolutionPreference::Raw).await;
        }

        // Phase 2: load + replay
        let events = read_jsonl_captures(&path).unwrap();
        assert_eq!(events.len(), 1);
        let replay = ReplayRagSource::from_captures("stub", persona(), events);
        let result = replay.deliver(&ctx(), 999, ResolutionPreference::Raw).await;
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.items[0].content, "hello");
        assert_eq!(result.items[1].content, "world");
    }
}
