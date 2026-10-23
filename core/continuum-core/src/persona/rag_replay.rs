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
//!   Legacy events without turn IDs remain sequential: interleaved anonymous
//!   turns cannot be distinguished when associating recorded budget claims.

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;

use crate::persona::rag_budget::{
     RagContext, RagDelivery, RagSource, RagSourceBudget, ResolutionPreference,
};
use crate::persona::rag_capture::RagCaptureEvent;

/// A read-only source that returns previously-captured deliveries
/// instead of computing fresh ones. Persona-bound at construction;
/// source_id pass-through.
pub struct ReplayRagSource {
    source_id: &'static str,
    persona_id: uuid::Uuid,
    /// Each delivery retains its captured claim; observation does not advance
    /// either, and delivery consumes both atomically in the existing FIFO.
    initial: Mutex<VecDeque<(RagDelivery, Option<RagSourceBudget>)>>,
}

impl ReplayRagSource {
    /// Construct from a set of pre-built deliveries returned by
    /// `deliver()`, replayed FIFO in capture order. Useful for tests that
    /// don't want to round-trip through serde.
    pub fn from_deliveries(
        source_id: &'static str,
        persona_id: uuid::Uuid,
        initial: Vec<RagDelivery>,
    ) -> Self {
        Self {
            source_id,
            persona_id,
            initial: Mutex::new(
                initial
                    .into_iter()
                    .map(|delivery| (delivery, None))
                    .collect(),
            ),
        }
    }

    /// Construct from a captured event stream. Filters by
    /// `source_id` and `persona_id`; events from other sources or
    /// other personas are dropped on the floor. Every SourceDelivered
    /// event goes into the initial FIFO in capture order (ruling-a: no
    /// resumable handle). Matching TurnStart claims travel with
    /// that delivery, even when different turns are interleaved in the trace.
    /// Source-only/older traces without a claim use the ordinary default claim;
    /// their identity never selects an invented historical policy.
    pub fn from_captures(
        source_id: &'static str,
        persona_id: uuid::Uuid,
        events: impl IntoIterator<Item = RagCaptureEvent>,
    ) -> Self {
        let mut initial = VecDeque::new();
        let mut claims = HashMap::new();
        for event in events {
            match event {
                RagCaptureEvent::TurnStart {
                    persona_id: captured_persona_id,
                    turn_id,
                    source_budgets,
                    ..
                } if captured_persona_id == persona_id => {
                    claims.insert(
                        turn_id,
                        source_budgets
                            .into_iter()
                            .find(|claim| claim.source_id == source_id),
                    );
                }
                RagCaptureEvent::SourceDelivered {
                    source_id: captured_source_id,
                    persona_id: captured_persona_id,
                    turn_id,
                    delivery,
                    ..
                } if captured_source_id == source_id && captured_persona_id == persona_id => {
                    initial.push_back((delivery, claims.get(&turn_id).cloned().flatten()));
                }
                RagCaptureEvent::TurnEnd {
                    persona_id: captured_persona_id,
                    turn_id,
                    ..
                } if captured_persona_id == persona_id => {
                    claims.remove(&turn_id);
                }
                _ => {}
            }
        }
        Self {
            source_id,
            persona_id,
            initial: Mutex::new(initial),
        }
    }

    /// How many deliveries remain in the initial queue. Useful for
    /// tests + harness assertions ("did we exhaust the trace?").
    pub fn remaining_initial(&self) -> usize {
        self.initial.lock().unwrap().len()
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

    /// A replay reproduces a recorded delivery; it reserves nothing.
    fn floor_tokens(&self) -> u32 {
        0
    }

    /// Reproduce the next captured resolved claim, rather than reinterpreting a
    /// source's name under today's policy. Like delivery replay this deliberately
    /// preserves recorded values; a diagnostic caller can explicitly override
    /// allocator inputs to compare a different window or policy.
    fn budget_claim(&self, window: u32, headroom: u32) -> RagSourceBudget {
        self.initial
            .lock()
            .unwrap()
            .front()
            .and_then(|(_, claim)| claim.clone())
            .unwrap_or_else(|| RagSourceBudget::retrieval(self.source_id, 0, window, headroom))
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
                truncated: false,
                resolution_used: ResolutionPreference::Placeholder,
            };
        }
        match self.initial.lock().unwrap().pop_front() {
            Some((delivery, _)) => delivery,
            None => RagDelivery {
                source_id: self.source_id.to_string(),
                items: Vec::new(),
                tokens_used: 0,
                truncated: false,
                resolution_used: ResolutionPreference::Placeholder,
            },
        }
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
            truncated: false,
            resolution_used: ResolutionPreference::Raw,
        }
    }

    // ---- ReplayRagSource direct construction ----

    #[tokio::test]
    async fn replay_returns_canned_delivery_on_deliver() {
        let canned = delivery("stub", vec![item("hello", 5)]);
        let source =
            ReplayRagSource::from_deliveries("stub", persona(), vec![canned.clone()]);
        assert_eq!(
            source.budget_claim(2048, 512),
            RagSourceBudget::retrieval("stub", 0, 2048, 512),
            "a source-only fixture declares no captured policy"
        );
        let result = source.deliver(&ctx(), 100, ResolutionPreference::Raw).await;
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].content, "hello");
        // Queue is now exhausted.
        assert_eq!(source.remaining_initial(), 0);
    }

    #[tokio::test]
    async fn replay_exhausted_returns_empty_not_panic() {
        let source = ReplayRagSource::from_deliveries("stub", persona(), Vec::new());
        let result = source.deliver(&ctx(), 100, ResolutionPreference::Raw).await;
        assert_eq!(result.items.len(), 0);
        assert_eq!(result.resolution_used, ResolutionPreference::Placeholder);
    }

    #[tokio::test]
    async fn replay_cross_persona_ctx_returns_empty() {
        let canned = delivery("stub", vec![item("a", 5)]);
        let source = ReplayRagSource::from_deliveries("stub", persona(), vec![canned]);
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
        let source = ReplayRagSource::from_deliveries("stub", persona(), vec![d1, d2]);
        let r1 = source.deliver(&ctx(), 100, ResolutionPreference::Raw).await;
        let r2 = source.deliver(&ctx(), 100, ResolutionPreference::Raw).await;
        assert_eq!(r1.items[0].content, "first");
        assert_eq!(r2.items[0].content, "second");
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

        // Regression for 3ea02ba7: interleaved turns keep their own resolved
        // source claims; neither the next turn nor another persona replaces it.
        let mut first_ctx = ctx();
        first_ctx.substrate.turn_id = Some(Uuid::new_v4());
        let mut second_ctx = ctx();
        second_ctx.substrate.turn_id = Some(Uuid::new_v4());
        let first_claim = RagSourceBudget::floorless_share(
            "stub",
            2048,
            512,
            std::num::NonZeroU32::new(64).unwrap(),
        );
        let second_claim = RagSourceBudget::retrieval("stub", 5, 2048, 512);
        for (context, claim) in [
            (first_ctx.clone(), first_claim.clone()),
            (second_ctx.clone(), second_claim.clone()),
        ] {
            sink.record(RagCaptureEvent::TurnStart {
                captured_at_ms: context.now_ms,
                persona_id: persona(),
                turn_id: context.substrate.turn_id,
                context_window: 2048,
                reserved: crate::persona::rag_budget::ReservedTokens {
                    system: 0,
                    completion: 0,
                },
                source_budgets: vec![claim],
                context,
            });
        }
        sink.record(RagCaptureEvent::TurnStart {
            captured_at_ms: 0,
            persona_id: Uuid::new_v4(),
            turn_id: first_ctx.substrate.turn_id,
            context_window: 2048,
            reserved: crate::persona::rag_budget::ReservedTokens {
                system: 0,
                completion: 0,
            },
            source_budgets: vec![second_claim.clone()],
            context: first_ctx.clone(),
        });

        // Two deliver calls — captures should accumulate.
        recorder
            .deliver(&first_ctx, 8, ResolutionPreference::Raw)
            .await; // packs 1 item
        recorder
            .deliver(&second_ctx, 100, ResolutionPreference::Raw)
            .await; // re-derives from scratch — full budget fits both items

        // Now replay the captured events through ReplayRagSource.
        let captured = sink.events();
        let replay = ReplayRagSource::from_captures("stub", persona(), captured.into_iter());

        assert_eq!(
            replay.budget_claim(1, 1),
            first_claim,
            "recorded policy is not recomputed"
        );
        assert!(replay
            .deliver(
                &RagContext::for_persona(Uuid::new_v4(), 0),
                999,
                ResolutionPreference::Raw
            )
            .await
            .items
            .is_empty());
        assert_eq!(
            replay.budget_claim(1, 1),
            first_claim,
            "foreign delivery cannot advance the claim"
        );

        let first = replay.deliver(&ctx(), 999, ResolutionPreference::Raw).await;
        assert_eq!(first.items.len(), 1);
        assert_eq!(first.items[0].content, "alpha");
        assert_eq!(replay.budget_claim(1, 1), second_claim);

        let second = replay.deliver(&ctx(), 999, ResolutionPreference::Raw).await;
        // Ruling a: the live stub re-derives from scratch on each call (no
        // positional cursor), so the captured second delivery holds both items —
        // and the replay returns it verbatim.
        assert_eq!(second.items.len(), 2);
        assert_eq!(second.items[0].content, "alpha");
        assert_eq!(second.items[1].content, "beta");

        // Trace exhausted now.
        let third = replay.deliver(&ctx(), 999, ResolutionPreference::Raw).await;
        assert_eq!(third.items.len(), 0);
        assert_eq!(
            replay.budget_claim(2048, 512),
            RagSourceBudget::retrieval("stub", 0, 2048, 512)
        );
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
            recorder
                .deliver(&ctx(), 100, ResolutionPreference::Raw)
                .await;
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
    // ---- Wire forms of a delivered pack: current `truncated`, legacy (pre-ruling-a)
    //      `continuation` null/object, and lines carrying both or neither ----

    /// Build one SourceDelivered capture line with an arbitrary combination of the
    /// two truncation keys. `None` removes the key entirely; a value inserts it — so
    /// old JSONL shapes (no `truncated`, legacy `continuation`) and precedence lines
    /// (both present) are constructed exactly as they would appear on disk.
    fn source_delivered_wire(
        items: Vec<crate::persona::rag_budget::RagItem>,
        truncated: Option<bool>,
        continuation: Option<serde_json::Value>,
    ) -> String {
        let mut v = serde_json::to_value(RagCaptureEvent::SourceDelivered {
            captured_at_ms: 1_000,
            persona_id: persona(),
            turn_id: None,
            source_id: "stub".to_string(),
            budget_requested: 100,
            resolution_requested: ResolutionPreference::Raw,
            delivery: crate::persona::rag_budget::RagDelivery {
                source_id: "stub".to_string(),
                items,
                tokens_used: 5,
                truncated: truncated.unwrap_or(false),
                resolution_used: ResolutionPreference::Raw,
            },
        })
        .unwrap();
        let d = v
            .get_mut("delivery")
            .expect("event carries a delivery object")
            .as_object_mut()
            .expect("delivery is a JSON object");
        match truncated {
            Some(t) => d.insert("truncated".to_string(), serde_json::Value::Bool(t)),
            None => d.remove("truncated"),
        };
        match continuation {
            Some(c) => d.insert("continuation".to_string(), c),
            None => d.remove("continuation"),
        };
        serde_json::to_string(&v).unwrap()
    }

    /// Decode capture lines through the real JSONL reader (not direct serde), so
    /// assertions ride the path replay actually uses.
    fn decode_capture_lines(lines: &[String]) -> Vec<RagCaptureEvent> {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("captures.jsonl");
        std::fs::write(&path, lines.join("\n")).unwrap();
        read_jsonl_captures(&path).expect("JSONL reader must not fail on these lines")
    }

    #[test]
    fn legacy_capture_lines_decode_without_dropping_or_losing_truncation() {
        // Old-schema shapes: complete (continuation null), nonempty partial
        // (object), empty partial (object, nothing fit); plus missing-key and
        // explicit-bool precedence lines. None may be skipped as a parse
        // failure; the truncation truth must survive decode.
        let object = serde_json::json!({ "opaque": {} });
        let lines = [
            source_delivered_wire(vec![item("alpha", 5)], None, Some(serde_json::Value::Null)),
            source_delivered_wire(vec![item("beta", 5)], None, Some(object.clone())),
            source_delivered_wire(Vec::new(), None, Some(object.clone())),
            // No truncation record at all: neither key present -> complete.
            source_delivered_wire(vec![item("gamma", 5)], None, None),
            // An explicit bool wins over the legacy continuation either way.
            source_delivered_wire(
                vec![item("delta", 5)],
                Some(true),
                Some(serde_json::Value::Null),
            ),
            source_delivered_wire(Vec::new(), Some(false), Some(object.clone())),
        ];
        let events = decode_capture_lines(&lines);
        assert_eq!(events.len(), 6, "no line may be dropped as a parse failure");

        let flags: Vec<bool> = events
            .iter()
            .map(|e| match e {
                RagCaptureEvent::SourceDelivered { delivery, .. } => delivery.truncated,
                _ => panic!("expected SourceDelivered events"),
            })
            .collect();
        assert_eq!(flags, vec![false, true, true, false, true, false]);
    }

    #[tokio::test]
    async fn legacy_replay_reaches_the_faculty_notice_with_source_cut() {
        use crate::cognition::rag_source_faculty::{RagSourceFaculty, SaliencePolicy};
        use crate::cognition::workspace::{Faculty as _, Workspace};

        let ws = Workspace::new("legacy-notice");

        // One pass per old-schema shape: (expected truncation, item content).
        for (want_truncated, content) in [
            (false, Some("roster")),
            (true, Some("board")),
            (true, None), // empty partial: nothing fit, but the source has more
        ] {
            let items = match content {
                Some(c) => vec![item(c, 4)],
                None => Vec::new(),
            };
            let line = source_delivered_wire(
                items,
                None,
                Some(if want_truncated {
                    serde_json::json!({ "opaque": {} })
                } else {
                    serde_json::Value::Null
                }),
            );
            // Ride the real JSONL reader path, not a direct serde parse: the
            // cut-truth assertion must hold through what replay actually loads.
            let ev = decode_capture_lines(&[line])
                .into_iter()
                .next()
                .expect("the reader returns the line it was given");
            let replay = ReplayRagSource::from_captures("stub", persona(), vec![ev]);
            let faculty =
                RagSourceFaculty::new(persona(), Arc::new(replay), SaliencePolicy::Retrieved);
            let c = faculty
                .contribute(&ws)
                .await
                .expect("an empty PARTIAL delivery still contributes its notice (card e049aa82)");
            assert_eq!(
                c.source_truncated, want_truncated,
                "notice must carry the source-cut truth through JSONL -> replay -> faculty"
            );
        }
    }
}
