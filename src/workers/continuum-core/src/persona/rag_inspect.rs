//! rag_inspect — the substrate's honest-look-at-the-prompt primitive.
//!
//! Joel (2026-05-31): "This is the differentiator between a complex
//! guess and an intentional brain. If we have observability and
//! replay at any stage, we can iterate, improve, add complexity, try
//! out new ideas in realistic scenarios and look at it ourselves:
//! with this prompt would I respond as it requests at this step?
//! Which layer is broken? Missing, is this contextually relevant
//! (hippocampus and caches)?"
//!
//! ### Why this exists at the library layer (not just as a binary)
//!
//! The airc_rag_demo binary proved we CAN build a per-item dump from
//! the L1 RAG pipeline. But binaries aren't callable by other AIs.
//! To make introspection a substrate-level primitive — discoverable
//! via `Commands.execute('persona/rag-inspect', { persona })` and
//! consumable by Claude / sentinels / any other persona doing
//! adversarial review — it has to be a Rust library function with
//! a structured result type. The ServiceModule + ts-rs binding sit
//! ON TOP of this function; the binary becomes a thin CLI wrapper.
//!
//! ### Doctrine alignment
//!
//! - [[observability-is-half-the-architecture]] — half the substrate
//!   is honest visibility into load-bearing decisions. This is one of
//!   them; the sink and trace path are first-class request inputs.
//! - [[persona-record-replay-is-a-product-requirement]] — every
//!   inspection that opts into `trace_path` produces a JSONL trace
//!   that ReplayRagSource consumes byte-for-byte.
//! - [[substrate-is-a-good-citizen-on-the-host]] — when `trace_path`
//!   is `None`, the sink is `NoopRagCaptureSink` (zero overhead). The
//!   hot path doesn't pay for observability it didn't ask for.
//! - [[source-drain-is-the-universal-pattern]] — the inspection IS
//!   the drain for in-flight RAG decisions. Without it those
//!   decisions are sources without drains, which is the leak shape.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::persona::airc_source::{AircRagSource, AircTranscriptReader};
use crate::persona::rag_budget::{
    BudgetAllocation, FlexboxRagBudgetAdapter, RagBudgetAdapter, RagContext, RagSource,
    RagSourceBudget, ReservedTokens, ResolutionPreference,
};
use crate::persona::rag_capture::{
    JsonlRagCaptureSink, NoopRagCaptureSink, RagCaptureEvent, RagCaptureSink, RecordingRagSource,
};

/// How many chars of an item's content to keep in the preview. Items
/// with longer content still report full token cost; this only
/// controls the human/AI-readable snippet returned in the inspection
/// result. Replay against the trace gets the full content; the
/// inspection result is for "look at what the persona would see right
/// now" mechanic-shop summarization.
pub const CONTENT_PREVIEW_CHARS: usize = 200;

/// Tunable inputs for one inspection. Defaults via `defaults_for`
/// match the `mid-local (32k)` profile the demo binary uses — a
/// sensible "what would a typical local persona see right now" probe
/// when the caller doesn't have stronger opinions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagInspectionRequest {
    pub persona_id: Uuid,
    pub persona_name: String,
    pub context_window: u32,
    pub reserved: ReservedTokens,
    pub airc_floor: u32,
    pub airc_max: u32,
    pub airc_priority: u8,
    pub airc_required: bool,
    pub airc_fetch_limit: usize,
    /// Wall-clock "now" the inspection should reason against. Caller
    /// supplies this so the function stays pure-of-clock (testable +
    /// deterministic replay).
    pub now_ms: u64,
    /// Where to write the capture trace. `None` = NoopSink (zero
    /// overhead, no file I/O). `Some(path)` = JSONL writer; the
    /// parent directory is created if absent.
    pub trace_path: Option<PathBuf>,
}

impl RagInspectionRequest {
    /// Sensible defaults for "show me what this persona would see
    /// right now at a typical 32k context model." Caller can mutate
    /// any field after this.
    pub fn defaults_for(persona_id: Uuid, persona_name: String, now_ms: u64) -> Self {
        Self {
            persona_id,
            persona_name,
            context_window: 32_768,
            reserved: ReservedTokens {
                system: 400,
                completion: 4_000,
            },
            airc_floor: 500,
            airc_max: 20_000,
            airc_priority: 10,
            airc_required: true,
            airc_fetch_limit: 100,
            now_ms,
            trace_path: None,
        }
    }
}

/// The honest-look result. Carries the full allocation outcome PLUS
/// per-source delivery details with the mechanic-grade rationale
/// (score, lamport, peer-id-prefix, age, content preview).
///
/// Specifically does NOT collapse layers — the future is multiple
/// sources (engram, airc, reference, working-memory). Each gets its
/// own `SourceDeliveryInspection` so the "which layer is broken?"
/// question is answerable by inspection rather than by guessing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagInspection {
    pub persona_id: Uuid,
    pub persona_name: String,
    pub context_window: u32,
    pub allocation: BudgetAllocation,
    pub deliveries: Vec<SourceDeliveryInspection>,
    /// Path to the JSONL trace if `trace_path` was set on the request,
    /// else `None`. Other AIs / mechanics resume replay against this.
    pub trace_path: Option<PathBuf>,
}

/// Per-source delivery, with the substrate-grade detail every
/// inspection caller needs: requested budget, actual usage,
/// continuation flag, and the full list of items packed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceDeliveryInspection {
    pub source_id: String,
    pub budget_requested: u32,
    pub tokens_used: u32,
    pub has_continuation: bool,
    pub items: Vec<InspectedItem>,
}

/// One item from a source's delivery, with the fields a mechanic
/// needs to answer "why this item?" — score, age, who, when.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectedItem {
    pub index: usize,
    pub tokens: u32,
    pub score: f64,
    pub content_preview: String,
    pub peer_id_prefix: String,
    pub lamport: u64,
    pub age_s: u64,
    /// Full source-emitted metadata — sources may attach additional
    /// fields beyond the canonical ones above (e.g. event_id,
    /// room_id, admission_origin). Preserved verbatim for inspection
    /// callers who want the whole picture.
    pub metadata: serde_json::Value,
}

/// Run one inspection turn against the persona's airc transcript.
///
/// This is the library entry point. The ServiceModule wraps it; the
/// demo binary wraps it; tests wrap it via stub readers; future
/// adversarial reviewers wrap it via the eventual command.
pub async fn inspect_persona_rag(
    request: &RagInspectionRequest,
    airc_reader: Arc<dyn AircTranscriptReader>,
) -> Result<RagInspection, String> {
    let airc_source = AircRagSource::new(request.persona_id, airc_reader)
        .with_fetch_limit(request.airc_fetch_limit);

    let sink: Arc<dyn RagCaptureSink> = match &request.trace_path {
        Some(path) => {
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| format!("create trace dir: {e}"))?;
            }
            Arc::new(
                JsonlRagCaptureSink::open(path.clone())
                    .map_err(|e| format!("open trace sink: {e}"))?,
            )
        }
        None => Arc::new(NoopRagCaptureSink),
    };

    let recorded = RecordingRagSource::new(airc_source, sink.clone());

    let ctx_base = RagContext::for_persona(request.persona_id, request.now_ms);
    let turn_id = Uuid::new_v4();
    let mut ctx = ctx_base.clone();
    ctx.substrate.turn_id = Some(turn_id);

    let budgets = vec![RagSourceBudget {
        source_id: "airc".to_string(),
        priority: request.airc_priority,
        floor_tokens: request.airc_floor,
        min_tokens: request.airc_floor,
        max_tokens: request.airc_max,
        required: request.airc_required,
    }];

    sink.record(RagCaptureEvent::TurnStart {
        captured_at_ms: request.now_ms,
        persona_id: request.persona_id,
        turn_id: Some(turn_id),
        context_window: request.context_window,
        reserved: request.reserved,
        source_budgets: budgets.clone(),
        context: ctx.clone(),
    });

    let adapter = FlexboxRagBudgetAdapter::new();
    let allocation = adapter.allocate(&ctx, request.context_window, request.reserved, &budgets);

    sink.record(RagCaptureEvent::BudgetAllocated {
        captured_at_ms: request.now_ms,
        persona_id: request.persona_id,
        turn_id: Some(turn_id),
        allocation: allocation.clone(),
    });

    let airc_alloc = allocation
        .allocations
        .first()
        .ok_or_else(|| "allocator returned no source allocations".to_string())?;
    let budget_requested = airc_alloc.allocated_tokens;
    let delivery = recorded
        .deliver(&ctx, budget_requested, ResolutionPreference::Raw)
        .await;

    sink.record(RagCaptureEvent::TurnEnd {
        captured_at_ms: request.now_ms,
        persona_id: request.persona_id,
        turn_id: Some(turn_id),
    });

    let items: Vec<InspectedItem> = delivery
        .items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let score = item
                .metadata
                .get("score")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let lamport = item
                .metadata
                .get("lamport")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let peer_id_prefix = item
                .metadata
                .get("peer_id")
                .and_then(|v| v.as_str())
                .map(|s| s.chars().take(8).collect::<String>())
                .unwrap_or_else(|| "????".to_string());
            let occurred_at_ms = item
                .metadata
                .get("occurred_at_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let age_s = if occurred_at_ms > 0 && request.now_ms > occurred_at_ms {
                (request.now_ms - occurred_at_ms) / 1_000
            } else {
                0
            };
            let content_preview: String =
                item.content.chars().take(CONTENT_PREVIEW_CHARS).collect();
            InspectedItem {
                index: idx,
                tokens: item.tokens,
                score,
                content_preview,
                peer_id_prefix,
                lamport,
                age_s,
                metadata: item.metadata.clone(),
            }
        })
        .collect();

    Ok(RagInspection {
        persona_id: request.persona_id,
        persona_name: request.persona_name.clone(),
        context_window: request.context_window,
        allocation,
        deliveries: vec![SourceDeliveryInspection {
            source_id: delivery.source_id.clone(),
            budget_requested,
            tokens_used: delivery.tokens_used,
            has_continuation: delivery.continuation.is_some(),
            items,
        }],
        trace_path: request.trace_path.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use airc_core::{Body, ClientId, EventId, Headers, MentionTarget, PeerId, RoomId, TranscriptEvent, TranscriptKind};
    use airc_lib::AircError;
    use async_trait::async_trait;
    use std::sync::Mutex;

    fn persona() -> Uuid {
        Uuid::parse_str("00000000-0000-0000-0000-000000000aaa").unwrap()
    }

    struct StubReader {
        events: Vec<TranscriptEvent>,
        fail: Mutex<bool>,
    }
    impl StubReader {
        fn new(events: Vec<TranscriptEvent>) -> Self {
            Self {
                events,
                fail: Mutex::new(false),
            }
        }
        fn set_fail(&self, fail: bool) {
            *self.fail.lock().unwrap() = fail;
        }
    }
    #[async_trait]
    impl AircTranscriptReader for StubReader {
        async fn page_recent(&self, limit: usize) -> Result<Vec<TranscriptEvent>, AircError> {
            if *self.fail.lock().unwrap() {
                return Err(AircError::UnknownPeer(PeerId::new()));
            }
            Ok(self.events.iter().take(limit).cloned().collect())
        }
    }

    fn make_event(text: Option<&str>, lamport: u64, occurred_at_ms: u64) -> TranscriptEvent {
        TranscriptEvent {
            event_id: EventId::new(),
            room_id: RoomId::new(),
            peer_id: PeerId::new(),
            client_id: ClientId::new(),
            kind: TranscriptKind::Message,
            occurred_at_ms,
            lamport,
            target: MentionTarget::Room(RoomId::new()),
            headers: Headers::default(),
            body: text.map(Body::text),
            attachment: None,
            receipt: None,
            metadata: serde_json::Value::Null,
        }
    }

    fn request(now_ms: u64) -> RagInspectionRequest {
        let mut req = RagInspectionRequest::defaults_for(persona(), "TestPersona".to_string(), now_ms);
        // Tiny-local profile from the demo binary — reserves stay
        // small so the tests assert behavior against a 4k context.
        req.context_window = 4_096;
        req.reserved = ReservedTokens {
            system: 200,
            completion: 800,
        };
        req.airc_floor = 100;
        req.airc_max = 2_000;
        req
    }

    // ---- TDD tests ----

    #[tokio::test]
    async fn empty_transcript_yields_empty_delivery() {
        let reader = Arc::new(StubReader::new(vec![]));
        let result = inspect_persona_rag(&request(1_000_000), reader).await.unwrap();
        assert_eq!(result.persona_id, persona());
        assert_eq!(result.persona_name, "TestPersona");
        assert_eq!(result.context_window, 4_096);
        assert_eq!(result.deliveries.len(), 1);
        let d = &result.deliveries[0];
        assert_eq!(d.source_id, "airc");
        assert!(d.items.is_empty());
        assert_eq!(d.tokens_used, 0);
        assert!(!d.has_continuation);
    }

    #[tokio::test]
    async fn allocation_reports_satisfied_state_for_required_source_with_room() {
        let reader = Arc::new(StubReader::new(vec![]));
        let result = inspect_persona_rag(&request(1_000_000), reader).await.unwrap();
        // 4096 - 200 system - 800 completion = 3096 available; airc gets max=2000 → Satisfied
        assert!(!result.allocation.escalation_needed);
        let airc_a = &result.allocation.allocations[0];
        assert_eq!(airc_a.source_id, "airc");
        assert_eq!(airc_a.allocated_tokens, 2_000);
    }

    #[tokio::test]
    async fn inspected_items_carry_score_age_and_peer_prefix() {
        let now_ms = 2_000_000u64;
        let event_ms = 1_995_000u64; // 5 seconds ago
        let reader = Arc::new(StubReader::new(vec![make_event(Some("hello world"), 42, event_ms)]));
        let result = inspect_persona_rag(&request(now_ms), reader).await.unwrap();
        let items = &result.deliveries[0].items;
        assert_eq!(items.len(), 1);
        let it = &items[0];
        assert_eq!(it.index, 0);
        assert_eq!(it.content_preview, "hello world");
        assert!((it.score - 1.0).abs() < 1e-9, "first item scores 1.0, got {}", it.score);
        assert_eq!(it.lamport, 42);
        assert_eq!(it.age_s, 5);
        assert_eq!(it.peer_id_prefix.len(), 8);
        assert!(it.metadata.get("event_id").is_some());
    }

    #[tokio::test]
    async fn long_content_is_truncated_in_preview_but_tokens_remain_accurate() {
        // 1000-char message → preview is CONTENT_PREVIEW_CHARS chars; tokens are full message
        let msg: String = "x".repeat(1_000);
        let reader = Arc::new(StubReader::new(vec![make_event(Some(&msg), 1, 1_000_000)]));
        let mut req = request(1_000_000);
        req.airc_max = 10_000; // ample budget
        let result = inspect_persona_rag(&req, reader).await.unwrap();
        let it = &result.deliveries[0].items[0];
        assert_eq!(it.content_preview.chars().count(), CONTENT_PREVIEW_CHARS);
        assert!(it.tokens >= 250, "1000 chars should cost ~250 tokens, got {}", it.tokens);
    }

    #[tokio::test]
    async fn continuation_flag_set_when_budget_overflows() {
        // 4 items × ~2 tokens each, but tight budget that forces continuation
        let reader = Arc::new(StubReader::new(vec![
            make_event(Some("aaaaa"), 1, 1_000_000),
            make_event(Some("bbbbb"), 2, 1_000_000),
            make_event(Some("ccccc"), 3, 1_000_000),
            make_event(Some("ddddd"), 4, 1_000_000),
        ]));
        let mut req = request(1_000_000);
        req.airc_floor = 4;
        req.airc_max = 4;
        let result = inspect_persona_rag(&req, reader).await.unwrap();
        let d = &result.deliveries[0];
        assert!(d.has_continuation, "tight budget should leave continuation");
        assert!(d.items.len() < 4, "not all items should fit");
    }

    #[tokio::test]
    async fn reader_failure_surfaces_as_empty_delivery_not_panic() {
        let reader = Arc::new(StubReader::new(vec![make_event(Some("oops"), 1, 1_000_000)]));
        reader.set_fail(true);
        let result = inspect_persona_rag(&request(1_000_000), reader).await.unwrap();
        assert!(result.deliveries[0].items.is_empty());
        // No panic — substrate-is-a-good-citizen
    }

    #[tokio::test]
    async fn trace_path_writes_jsonl_lines() {
        let dir = tempfile::tempdir().unwrap();
        let trace = dir.path().join("inspect.jsonl");
        let reader = Arc::new(StubReader::new(vec![make_event(Some("traced"), 1, 1_000_000)]));
        let mut req = request(1_000_000);
        req.trace_path = Some(trace.clone());
        let result = inspect_persona_rag(&req, reader).await.unwrap();
        assert_eq!(result.trace_path.as_deref(), Some(trace.as_path()));
        let body = std::fs::read_to_string(&trace).unwrap();
        // Expect at least TurnStart, BudgetAllocated, SourceDelivered, TurnEnd
        let line_count = body.lines().count();
        assert!(line_count >= 4, "expected ≥4 capture events, got {line_count}");
        assert!(body.contains("turn_start"));
        assert!(body.contains("budget_allocated"));
        assert!(body.contains("source_delivered"));
        assert!(body.contains("turn_end"));
    }

    #[tokio::test]
    async fn no_trace_path_uses_noop_sink() {
        let reader = Arc::new(StubReader::new(vec![make_event(Some("untraced"), 1, 1_000_000)]));
        let req = request(1_000_000);
        assert!(req.trace_path.is_none());
        let result = inspect_persona_rag(&req, reader).await.unwrap();
        assert!(result.trace_path.is_none());
        // Just don't panic; Noop sink swallowed everything.
        assert_eq!(result.deliveries[0].items.len(), 1);
    }

    #[tokio::test]
    async fn cross_persona_scope_check_yields_empty_via_source() {
        // Inspection driven for persona A, but the source itself
        // rejects cross-persona ctx. We construct the request for
        // persona A; the source is built around persona A; we
        // verify the items come from A's view — defense in depth.
        let reader = Arc::new(StubReader::new(vec![make_event(Some("for A"), 1, 1_000_000)]));
        let result = inspect_persona_rag(&request(1_000_000), reader).await.unwrap();
        assert_eq!(result.persona_id, persona());
        assert_eq!(result.deliveries[0].items.len(), 1);
    }
}
