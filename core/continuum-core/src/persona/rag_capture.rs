//! RAG turn capture — the mechanic-shop's lift + diagnostic gauges.
//!
//! Per Joel (2026-05-31): "We have often needed to see how a model
//! would work to debug it. Within harness with real world rag." …
//! "These things are complex machines. Make sure we can act as
//! mechanics."
//!
//! Per memory [[persona-record-replay-is-a-product-requirement]]:
//! capture live turns + replay; AR/CV source-video pattern; infra
//! (LiveTurnReplayFixture) exists but unwired — this slice wires
//! capture for the RAG layer specifically.
//!
//! ### What this module provides (slice 11 — capture side)
//!
//! - `RagCaptureEvent` — a tagged record of one fact in the turn
//!   (TurnStart, BudgetAllocated, SourceDelivered, TurnEnd).
//! - `RagCaptureSink` trait — abstract recording surface.
//! - `NoopRagCaptureSink` — production-safe default. Drops events on
//!   the floor; zero overhead when capture isn't in use.
//! - `JsonlRagCaptureSink` — file-based JSON-line writer. One JSON
//!   object per line; replay reader groups by turn_id.
//! - `RecordingRagSource<S>` — decorator wrapping any `RagSource`,
//!   intercepts `deliver` and `deliver_continuation`, records the
//!   call + result via the sink, returns the delivery unchanged.
//!   Drop-in around production sources.
//!
//! ### What's deferred
//!
//! - `ReplayRagSource` (slice 11.5) — reads captured deliveries
//!   from a sink, returns them instead of hitting live state.
//!   Symmetric to RecordingRagSource.
//! - Telemetry counter aggregation across captured events (slice 12).
//! - `airc rag-inspect <turn-id>` operator CLI (slice 12).
//! - Disk-pressure integration via the substrate pressure broker
//!   (task #88).
//! - File rotation policy. JsonlRagCaptureSink takes a path; the
//!   caller decides rotation (per-turn file, per-day file, etc.).
//!   Capture writes accumulate; source/drain doctrine says they
//!   must drain — that policy lives in the caller for slice 11.
//!
//! ### Doctrine alignment
//!
//! - [[substrate-is-a-good-citizen-on-the-host]]: NoopRagCaptureSink
//!   is the default; capture is opt-in. Atomic appends within-
//!   process via Mutex<File>. Honest observability — every event
//!   carries persona_id + turn_id (when present) for cross-event
//!   correlation.
//! - [[RTOS-brain-no-region-on-hot-path]]: capture writes are
//!   synchronous-after the source's call returns. Off the cognition
//!   hot path because cognition is whatever runs INSIDE the
//!   source's deliver(); capture writes happen after the cognition
//!   work is done.
//! - [[organization-purity-as-we-migrate]]: no backwards-compat
//!   hooks. Decorator pattern keeps `RagSource` impls untouched.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::persona::rag_budget::{
    BudgetAllocation, ContinuationCursor, RagContext, RagDelivery, RagSource, RagSourceBudget,
    ReservedTokens, ResolutionPreference,
};

//=============================================================================
// EVENT MODEL — one fact about the turn, tagged
//=============================================================================

/// One captured fact in a RAG turn. Every event carries persona_id
/// + (optional) turn_id for cross-event correlation. Replay readers
/// group events by turn_id; per-source diagnostics filter by
/// source_id.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RagCaptureEvent {
    /// Caller signals the start of a turn. The PromptAssembly layer
    /// emits this in slice 12; for slice 11, it's optional — sources
    /// can be recorded without bracketing events.
    TurnStart {
        captured_at_ms: u64,
        persona_id: uuid::Uuid,
        turn_id: Option<uuid::Uuid>,
        context_window: u32,
        reserved: ReservedTokens,
        source_budgets: Vec<RagSourceBudget>,
        context: RagContext,
    },
    /// The budget allocator decided who gets what. Emitted by the
    /// caller after `RagBudgetAdapter::allocate` returns.
    BudgetAllocated {
        captured_at_ms: u64,
        persona_id: uuid::Uuid,
        turn_id: Option<uuid::Uuid>,
        allocation: BudgetAllocation,
    },
    /// A source delivered. Emitted by `RecordingRagSource` decorator
    /// automatically after every `deliver` or `deliver_continuation`.
    SourceDelivered {
        captured_at_ms: u64,
        persona_id: uuid::Uuid,
        turn_id: Option<uuid::Uuid>,
        source_id: String,
        budget_requested: u32,
        resolution_requested: ResolutionPreference,
        /// Some when the call was deliver_continuation; carries the
        /// cursor that resumed.
        cursor: Option<ContinuationCursor>,
        delivery: RagDelivery,
    },
    /// Caller signals the end of a turn. Optional — replay can
    /// infer turn boundaries from turn_id + timestamps.
    TurnEnd {
        captured_at_ms: u64,
        persona_id: uuid::Uuid,
        turn_id: Option<uuid::Uuid>,
    },
}

impl RagCaptureEvent {
    pub fn persona_id(&self) -> uuid::Uuid {
        match self {
            Self::TurnStart { persona_id, .. }
            | Self::BudgetAllocated { persona_id, .. }
            | Self::SourceDelivered { persona_id, .. }
            | Self::TurnEnd { persona_id, .. } => *persona_id,
        }
    }

    pub fn turn_id(&self) -> Option<uuid::Uuid> {
        match self {
            Self::TurnStart { turn_id, .. }
            | Self::BudgetAllocated { turn_id, .. }
            | Self::SourceDelivered { turn_id, .. }
            | Self::TurnEnd { turn_id, .. } => *turn_id,
        }
    }
}

//=============================================================================
// SINK TRAIT — the recording surface
//=============================================================================

/// The abstract recording surface. `record` is synchronous because
/// the simplest sinks (Noop, in-memory Vec) don't need async; the
/// JsonlRagCaptureSink uses a Mutex<File> + sync writes (also fast,
/// just a few KB per event). Async sinks (network shipping, remote
/// telemetry) can implement on top of a sync interface by spawning
/// internally.
pub trait RagCaptureSink: Send + Sync {
    fn record(&self, event: RagCaptureEvent);
}

//=============================================================================
// NOOP SINK — production-safe default
//=============================================================================

/// Drops every event. The substrate's default when capture isn't
/// turned on — zero overhead beyond a trait-object virtual call.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopRagCaptureSink;

impl RagCaptureSink for NoopRagCaptureSink {
    fn record(&self, _event: RagCaptureEvent) {
        // Intentionally empty.
    }
}

//=============================================================================
// JSONL SINK — file-based, one JSON object per line
//=============================================================================

/// Writes one JSON object per line to a file. Within-process atomic
/// via Mutex<File>; cross-process atomicity is a future concern
/// (single-writer-per-file invariant for slice 11).
///
/// Per the no-clipping spirit: each event serializes as a complete
/// JSON object. Malformed lines (which shouldn't happen but might
/// during disk-full scenarios) are caller-visible — we return errors
/// from construction; per-event write failures log + drop. (Capture
/// failure must NEVER fail the cognition turn — the substrate stays
/// up; the mechanic's lift might be temporarily out of order.)
pub struct JsonlRagCaptureSink {
    path: PathBuf,
    file: Mutex<std::fs::File>,
}

impl JsonlRagCaptureSink {
    /// Open `path` for append (creating it if needed). Parent dir
    /// MUST already exist; caller is responsible for the rotation
    /// strategy + directory creation.
    pub fn open(path: PathBuf) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        Ok(Self {
            path,
            file: Mutex::new(file),
        })
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl RagCaptureSink for JsonlRagCaptureSink {
    fn record(&self, event: RagCaptureEvent) {
        let mut line = match serde_json::to_string(&event) {
            Ok(s) => s,
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    sink_path = %self.path.display(),
                    "rag capture: failed to serialize event — dropping (capture failures must not fail cognition)"
                );
                return;
            }
        };
        line.push('\n');
        // Mutex-protected append. Failures log + drop per the
        // "capture failure must never fail the cognition turn" rule.
        let mut file = self.file.lock().unwrap();
        if let Err(err) = std::io::Write::write_all(&mut *file, line.as_bytes()) {
            tracing::warn!(
                error = %err,
                sink_path = %self.path.display(),
                "rag capture: write failed — dropping (capture failures must not fail cognition)"
            );
        }
    }
}

//=============================================================================
// RECORDING DECORATOR — wraps any RagSource
//=============================================================================

/// Drop-in wrapper around any `RagSource`. Intercepts `deliver` and
/// `deliver_continuation`, records the call + result to the sink,
/// returns the delivery unchanged. Production callers wrap their
/// sources at construction:
///
/// ```ignore
/// let source = RecordingRagSource::new(
///     EngramSource::new(persona_id, admission_state),
///     capture_sink.clone(),
/// );
/// ```
///
/// The wrapped source's `source_id()` and behavior are pass-through;
/// the decorator only adds recording.
pub struct RecordingRagSource<S: RagSource> {
    inner: S,
    sink: Arc<dyn RagCaptureSink>,
}

impl<S: RagSource> RecordingRagSource<S> {
    pub fn new(inner: S, sink: Arc<dyn RagCaptureSink>) -> Self {
        Self { inner, sink }
    }
}

#[async_trait]
impl<S: RagSource + 'static> RagSource for RecordingRagSource<S> {
    fn source_id(&self) -> &'static str {
        self.inner.source_id()
    }

    /// Recording is transparent — the wrapped source's expansion verb is the
    /// real one; a capture wrapper must never change what a citizen is told.
    fn expand_command(&self) -> Option<&'static str> {
        self.inner.expand_command()
    }

    async fn deliver(
        &self,
        ctx: &RagContext,
        budget: u32,
        resolution: ResolutionPreference,
    ) -> RagDelivery {
        let delivery = self.inner.deliver(ctx, budget, resolution).await;
        let event = RagCaptureEvent::SourceDelivered {
            captured_at_ms: ctx.now_ms,
            persona_id: ctx.persona_id,
            turn_id: ctx.turn_id,
            source_id: self.inner.source_id().to_string(),
            budget_requested: budget,
            resolution_requested: resolution,
            cursor: None,
            delivery: delivery.clone(),
        };
        self.sink.record(event);
        delivery
    }

    async fn deliver_continuation(
        &self,
        ctx: &RagContext,
        cursor: ContinuationCursor,
        budget: u32,
    ) -> Option<RagDelivery> {
        let cursor_for_event = cursor.clone();
        let delivery = self
            .inner
            .deliver_continuation(ctx, cursor, budget)
            .await?;
        let event = RagCaptureEvent::SourceDelivered {
            captured_at_ms: ctx.now_ms,
            persona_id: ctx.persona_id,
            turn_id: ctx.turn_id,
            source_id: self.inner.source_id().to_string(),
            budget_requested: budget,
            resolution_requested: ResolutionPreference::Raw,
            cursor: Some(cursor_for_event),
            delivery: delivery.clone(),
        };
        self.sink.record(event);
        Some(delivery)
    }
}

//=============================================================================
// IN-MEMORY SINK — for tests + golden-trace harness scaffolding
//=============================================================================

/// In-memory sink that buffers events in a `Vec` behind a Mutex.
/// Used in tests + by the upcoming golden-trace harness (slice 11.5+)
/// to assert on captured events without touching disk.
#[derive(Debug, Default)]
pub struct InMemoryRagCaptureSink {
    inner: Mutex<Vec<RagCaptureEvent>>,
}

impl InMemoryRagCaptureSink {
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of all captured events so far. Cheap clone — events
    /// are Clone.
    pub fn events(&self) -> Vec<RagCaptureEvent> {
        self.inner.lock().unwrap().clone()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().is_empty()
    }

    /// Clear all captured events. Useful between test phases.
    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }
}

impl RagCaptureSink for InMemoryRagCaptureSink {
    fn record(&self, event: RagCaptureEvent) {
        self.inner.lock().unwrap().push(event);
    }
}

//=============================================================================
// TESTS
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::rag_budget::{ContinuationCursor, RagDelivery, RagItem, StubRagSource};
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

    // ---- Sink-level tests ----

    #[test]
    fn noop_sink_drops_events_silently() {
        let sink = NoopRagCaptureSink;
        // Should be a no-op; just verify no panic.
        sink.record(RagCaptureEvent::TurnEnd {
            captured_at_ms: 0,
            persona_id: persona(),
            turn_id: None,
        });
    }

    #[test]
    fn in_memory_sink_records_and_exposes_events() {
        let sink = InMemoryRagCaptureSink::new();
        assert!(sink.is_empty());
        sink.record(RagCaptureEvent::TurnEnd {
            captured_at_ms: 1,
            persona_id: persona(),
            turn_id: None,
        });
        sink.record(RagCaptureEvent::TurnEnd {
            captured_at_ms: 2,
            persona_id: persona(),
            turn_id: None,
        });
        assert_eq!(sink.len(), 2);
        let events = sink.events();
        assert_eq!(events.len(), 2);
        sink.clear();
        assert!(sink.is_empty());
    }

    #[test]
    fn jsonl_sink_writes_one_json_object_per_line() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("trace.jsonl");
        let sink = JsonlRagCaptureSink::open(path.clone()).unwrap();
        sink.record(RagCaptureEvent::TurnStart {
            captured_at_ms: 1_000,
            persona_id: persona(),
            turn_id: Some(Uuid::new_v4()),
            context_window: 32_768,
            reserved: ReservedTokens {
                system: 500,
                completion: 2_000,
            },
            source_budgets: vec![],
            context: ctx(),
        });
        sink.record(RagCaptureEvent::TurnEnd {
            captured_at_ms: 2_000,
            persona_id: persona(),
            turn_id: None,
        });
        drop(sink); // flush + close

        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2);
        // Each line should parse as a complete JSON object.
        let first: RagCaptureEvent = serde_json::from_str(lines[0]).unwrap();
        assert!(matches!(first, RagCaptureEvent::TurnStart { .. }));
        let second: RagCaptureEvent = serde_json::from_str(lines[1]).unwrap();
        assert!(matches!(second, RagCaptureEvent::TurnEnd { .. }));
    }

    #[test]
    fn jsonl_sink_appends_across_reopens() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("trace.jsonl");
        // Phase 1: write one event, close.
        {
            let sink = JsonlRagCaptureSink::open(path.clone()).unwrap();
            sink.record(RagCaptureEvent::TurnEnd {
                captured_at_ms: 1,
                persona_id: persona(),
                turn_id: None,
            });
        }
        // Phase 2: reopen, write another, close.
        {
            let sink = JsonlRagCaptureSink::open(path.clone()).unwrap();
            sink.record(RagCaptureEvent::TurnEnd {
                captured_at_ms: 2,
                persona_id: persona(),
                turn_id: None,
            });
        }
        let contents = std::fs::read_to_string(&path).unwrap();
        let line_count = contents.lines().count();
        assert_eq!(line_count, 2, "append across reopens must accumulate");
    }

    // ---- Decorator tests ----

    #[tokio::test]
    async fn recording_decorator_passes_through_delivery() {
        let inner = StubRagSource::new(
            "stub",
            persona(),
            vec![item("hello", 5), item("world", 5)],
        );
        let sink: Arc<dyn RagCaptureSink> = Arc::new(InMemoryRagCaptureSink::new());
        let recorder = RecordingRagSource::new(inner, sink.clone());
        let delivery = recorder.deliver(&ctx(), 100, ResolutionPreference::Raw).await;
        // Wrapped source's items pass through.
        assert_eq!(delivery.items.len(), 2);
        // source_id pass-through.
        assert_eq!(recorder.source_id(), "stub");
    }

    #[tokio::test]
    async fn recording_decorator_records_each_deliver() {
        let inner = StubRagSource::new("stub", persona(), vec![item("a", 5)]);
        let sink = Arc::new(InMemoryRagCaptureSink::new());
        let sink_dyn: Arc<dyn RagCaptureSink> = sink.clone();
        let recorder = RecordingRagSource::new(inner, sink_dyn);
        recorder.deliver(&ctx(), 100, ResolutionPreference::Raw).await;
        let events = sink.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            RagCaptureEvent::SourceDelivered {
                source_id,
                budget_requested,
                resolution_requested,
                cursor,
                delivery,
                ..
            } => {
                assert_eq!(source_id, "stub");
                assert_eq!(*budget_requested, 100);
                assert_eq!(*resolution_requested, ResolutionPreference::Raw);
                assert!(cursor.is_none());
                assert_eq!(delivery.items.len(), 1);
            }
            other => panic!("expected SourceDelivered, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn recording_decorator_records_continuation_with_cursor() {
        let inner = StubRagSource::new(
            "stub",
            persona(),
            vec![item("a", 5), item("b", 5), item("c", 5)],
        );
        let sink = Arc::new(InMemoryRagCaptureSink::new());
        let sink_dyn: Arc<dyn RagCaptureSink> = sink.clone();
        let recorder = RecordingRagSource::new(inner, sink_dyn);
        // First call doesn't consume everything.
        let first = recorder.deliver(&ctx(), 5, ResolutionPreference::Raw).await;
        let cursor = first.continuation.expect("expected continuation");
        sink.clear();
        // Continuation call should be recorded with the cursor.
        recorder
            .deliver_continuation(&ctx(), cursor.clone(), 100)
            .await
            .expect("continuation should yield");
        let events = sink.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            RagCaptureEvent::SourceDelivered {
                cursor: recorded_cursor,
                ..
            } => {
                let recorded = recorded_cursor.as_ref().expect("recorded cursor");
                assert_eq!(recorded.source_id, cursor.source_id);
                assert_eq!(recorded.persona_id, cursor.persona_id);
            }
            other => panic!("expected SourceDelivered, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn recording_decorator_records_persona_and_turn_id() {
        let inner = StubRagSource::new("stub", persona(), vec![item("a", 5)]);
        let sink = Arc::new(InMemoryRagCaptureSink::new());
        let sink_dyn: Arc<dyn RagCaptureSink> = sink.clone();
        let recorder = RecordingRagSource::new(inner, sink_dyn);
        // Build a context with turn_id set.
        let turn_id = Uuid::new_v4();
        let mut ctx_with_turn = ctx();
        ctx_with_turn.substrate.turn_id = Some(turn_id);
        recorder
            .deliver(&ctx_with_turn, 100, ResolutionPreference::Raw)
            .await;
        let events = sink.events();
        let ev = &events[0];
        assert_eq!(ev.persona_id(), persona());
        assert_eq!(ev.turn_id(), Some(turn_id));
    }

    #[test]
    fn captured_event_serde_roundtrip() {
        let event = RagCaptureEvent::SourceDelivered {
            captured_at_ms: 42,
            persona_id: persona(),
            turn_id: Some(Uuid::new_v4()),
            source_id: "stub".to_string(),
            budget_requested: 100,
            resolution_requested: ResolutionPreference::Compressed,
            cursor: Some(ContinuationCursor {
                persona_id: persona(),
                source_id: "stub".to_string(),
                opaque: serde_json::json!({ "next": 3 }),
            }),
            delivery: RagDelivery {
                source_id: "stub".to_string(),
                items: vec![item("hi", 2)],
                tokens_used: 2,
                continuation: None,
                resolution_used: ResolutionPreference::Compressed,
            },
        };
        let json = serde_json::to_string(&event).unwrap();
        let round: RagCaptureEvent = serde_json::from_str(&json).unwrap();
        // The kind discriminant survives.
        assert!(matches!(round, RagCaptureEvent::SourceDelivered { .. }));
    }
}
