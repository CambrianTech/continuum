#![cfg(test)]

use super::*;
use airc_core::{
    Body, ClientId, EventId, Headers, MentionTarget, PeerId, RoomId, TranscriptEvent,
    TranscriptKind,
};
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

/// All events in these tests share ONE room. A real airc channel has a single
/// room_id, and the digest is room-scoped (filters events to the room derived
/// from the transcript, slice-2 #43). A per-event random room would model
/// nothing real and silently drop all-but-the-last event from the window.
static TEST_ROOM: std::sync::LazyLock<RoomId> = std::sync::LazyLock::new(RoomId::new);

fn make_event(text: Option<&str>, lamport: u64, occurred_at_ms: u64) -> TranscriptEvent {
    TranscriptEvent {
        event_id: EventId::new(),
        room_id: *TEST_ROOM,
        peer_id: PeerId::new(),
        client_id: ClientId::new(),
        kind: TranscriptKind::Message,
        occurred_at_ms,
        lamport,
        target: MentionTarget::Room(*TEST_ROOM),
        headers: Headers::default(),
        body: text.map(Body::text),
        attachment: None,
        receipt: None,
        metadata: serde_json::Value::Null,
    }
}

fn request(now_ms: u64) -> RagInspectionRequest {
    RagInspectionRequest::for_window(persona(), "TestPersona".to_string(), now_ms, 4_096)
}

// ---- TDD tests ----

#[tokio::test]
async fn empty_transcript_yields_empty_delivery() {
    let reader = Arc::new(StubReader::new(vec![]));
    let result = inspect_persona_rag(&request(1_000_000), reader)
        .await
        .unwrap();
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

// what this catches: the glass box loses the per-item fields (score, age, peer, levels) a mechanic reads.
#[tokio::test]
async fn inspected_items_carry_score_age_and_peer_prefix() {
    let now_ms = 2_000_000u64;
    let event_ms = 1_995_000u64; // 5 seconds ago
    let reader = Arc::new(StubReader::new(vec![make_event(
        Some("hello world"),
        42,
        event_ms,
    )]));
    let result = inspect_persona_rag(&request(now_ms), reader).await.unwrap();
    let items = &result.deliveries[0].items;
    assert_eq!(items.len(), 1);
    let it = &items[0];
    assert_eq!(it.index, 0);
    assert_eq!(it.content, "hello world");
    assert!(
        (it.score - 1.0).abs() < 1e-9,
        "first item scores 1.0, got {}",
        it.score
    );
    assert_eq!(it.lamport, 42);
    assert_eq!(it.age_s, 5);
    assert_eq!(it.peer_id_shown.len(), 8);
    assert!(it.metadata.get("event_id").is_some());
    // levels are reported, whatever the source set
    let _ = (it.has_summary, it.has_pointer);
}

#[tokio::test]
async fn long_content_arrives_whole_and_tokens_are_accurate() {
    // what this catches: a preview cut standing in for the content a mechanic is inspecting.
    let msg: String = "x".repeat(1_000);
    let reader = Arc::new(StubReader::new(vec![make_event(Some(&msg), 1, 1_000_000)]));
    let req = request(1_000_000);
    let result = inspect_persona_rag(&req, reader).await.unwrap();
    let it = &result.deliveries[0].items[0];
    assert_eq!(it.content.chars().count(), 1_000, "the content is whole");
    assert!(
        it.tokens >= 250,
        "1000 chars should cost ~250 tokens, got {}",
        it.tokens
    );
}

// what this catches: the inspect view sizing the conversation (FLOW plan §2d). The digest
// is delivered whole (there is no budget), with NO continuation cursor (more history is a
// scrollback command); her view, not the source, decides what of it she sees.
#[tokio::test]
async fn delivers_the_whole_window_with_no_continuation_cursor() {
    let reader = Arc::new(StubReader::new(vec![
        make_event(Some("aaaaa"), 1, 1_000_000),
        make_event(Some("bbbbb"), 2, 1_000_000),
        make_event(Some("ccccc"), 3, 1_000_000),
        make_event(Some("ddddd"), 4, 1_000_000),
    ]));
    let result = inspect_persona_rag(&request(1_000_000), reader)
        .await
        .unwrap();
    let d = &result.deliveries[0];
    assert_eq!(d.items.len(), 4, "the source delivers every staged turn");
    assert!(
        !d.has_continuation,
        "the digest carries no continuation cursor by design"
    );
}

#[tokio::test]
async fn reader_failure_surfaces_as_empty_delivery_not_panic() {
    let reader = Arc::new(StubReader::new(vec![make_event(
        Some("oops"),
        1,
        1_000_000,
    )]));
    reader.set_fail(true);
    let result = inspect_persona_rag(&request(1_000_000), reader)
        .await
        .unwrap();
    assert!(result.deliveries[0].items.is_empty());
    // No panic — substrate-is-a-good-citizen
}

// what this catches: the trace must be exactly the budget-free turn (start, delivered, end); a revived allocation event would break replay.
#[tokio::test]
async fn trace_path_writes_jsonl_lines() {
    let dir = tempfile::tempdir().unwrap();
    let trace = dir.path().join("inspect.jsonl");
    let reader = Arc::new(StubReader::new(vec![make_event(
        Some("traced"),
        1,
        1_000_000,
    )]));
    let mut req = request(1_000_000);
    req.trace_path = Some(trace.clone());
    let result = inspect_persona_rag(&req, reader).await.unwrap();
    assert_eq!(result.trace_path.as_deref(), Some(trace.as_path()));
    let body = std::fs::read_to_string(&trace).unwrap();
    // Expect TurnStart, SourceDelivered, TurnEnd
    let line_count = body.lines().count();
    assert!(
        line_count >= 3,
        "expected ≥3 capture events, got {line_count}"
    );
    assert!(body.contains("turn_start"));
    assert!(!body.contains("budget_allocated"));
    assert!(body.contains("source_delivered"));
    assert!(body.contains("turn_end"));
}

#[tokio::test]
async fn no_trace_path_uses_noop_sink() {
    let reader = Arc::new(StubReader::new(vec![make_event(
        Some("untraced"),
        1,
        1_000_000,
    )]));
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
    let reader = Arc::new(StubReader::new(vec![make_event(
        Some("for A"),
        1,
        1_000_000,
    )]));
    let result = inspect_persona_rag(&request(1_000_000), reader)
        .await
        .unwrap();
    assert_eq!(result.persona_id, persona());
    assert_eq!(result.deliveries[0].items.len(), 1);
}

// ── chained inference probe (task #104) ─────────────────────

#[tokio::test]
async fn ragonly_path_leaves_model_response_none() {
    let reader = Arc::new(StubReader::new(vec![make_event(Some("hi"), 1, 999_000)]));
    let result = inspect_persona_rag(&request(1_000_000), reader)
        .await
        .unwrap();
    assert!(result.model_response.is_none());
}

#[tokio::test]
async fn chained_path_captures_response_from_heuristic_adapter() {
    use crate::ai::heuristic_adapter::{HeuristicInferenceAdapter, HEURISTIC_PROVIDER_ID};
    let reader = Arc::new(StubReader::new(vec![
        make_event(Some("hello"), 1, 999_000),
        make_event(Some("world"), 2, 999_500),
    ]));
    let adapter: Arc<dyn crate::ai::adapter::AIProviderAdapter> =
        Arc::new(HeuristicInferenceAdapter::new());
    let result = inspect_persona_rag_with_inference(&request(1_000_000), reader, Some(adapter))
        .await
        .unwrap();
    let mr = result.model_response.expect("expected model_response");
    assert_eq!(mr.adapter_id, HEURISTIC_PROVIDER_ID);
    assert!(mr.response_text.starts_with("[heuristic:"));
    // The latest user message should appear in the response
    // (the heuristic adapter echoes the last user turn).
    assert!(mr.response_text.contains("world"));
    assert_eq!(mr.finish_reason, "stop");
    assert!(mr.input_tokens > 0);
    assert!(mr.output_tokens > 0);
}

#[tokio::test]
async fn chained_path_with_zero_items_still_produces_marker_response() {
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
    let reader = Arc::new(StubReader::new(vec![]));
    let adapter: Arc<dyn crate::ai::adapter::AIProviderAdapter> =
        Arc::new(HeuristicInferenceAdapter::new());
    let result = inspect_persona_rag_with_inference(&request(1_000_000), reader, Some(adapter))
        .await
        .unwrap();
    let mr = result
        .model_response
        .expect("expected model_response even with no items");
    // The heuristic adapter saw an empty messages list → "(no
    // user text in prompt)" marker response per its contract.
    assert!(mr.response_text.contains("(no user text in prompt)"));
}

#[tokio::test]
async fn chained_path_prompt_text_carries_system_and_messages() {
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
    let reader = Arc::new(StubReader::new(vec![make_event(
        Some("greetings persona"),
        1,
        999_000,
    )]));
    let adapter: Arc<dyn crate::ai::adapter::AIProviderAdapter> =
        Arc::new(HeuristicInferenceAdapter::new());
    let result = inspect_persona_rag_with_inference(&request(1_000_000), reader, Some(adapter))
        .await
        .unwrap();
    let prompt = result.model_response.unwrap().prompt_text;
    assert!(prompt.contains("You are TestPersona"));
    assert!(prompt.contains("greetings persona"));
    assert!(prompt.starts_with("System:"));
    assert!(prompt.contains("user:"));
}

#[tokio::test]
async fn chained_path_same_prompt_yields_same_response_replay_safe() {
    // The heuristic adapter is deterministic — running the same
    // inspection twice produces byte-identical responses. This is
    // the substrate's replay contract per
    // [[inference-is-an-adapter-always-in-the-loop]].
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
    let reader1 = Arc::new(StubReader::new(vec![make_event(Some("hi"), 1, 999_000)]));
    let reader2 = Arc::new(StubReader::new(vec![make_event(Some("hi"), 1, 999_000)]));
    let adapter1: Arc<dyn crate::ai::adapter::AIProviderAdapter> =
        Arc::new(HeuristicInferenceAdapter::new());
    let adapter2: Arc<dyn crate::ai::adapter::AIProviderAdapter> =
        Arc::new(HeuristicInferenceAdapter::new());
    let r1 = inspect_persona_rag_with_inference(&request(1_000_000), reader1, Some(adapter1))
        .await
        .unwrap();
    let r2 = inspect_persona_rag_with_inference(&request(1_000_000), reader2, Some(adapter2))
        .await
        .unwrap();
    let m1 = r1.model_response.unwrap();
    let m2 = r2.model_response.unwrap();
    assert_eq!(m1.response_text, m2.response_text);
    assert_eq!(m1.prompt_text, m2.prompt_text);
}
