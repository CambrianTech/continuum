//! Per-Persona Admission State (continuum#1121 PR-4)
//!
//! Owns the per-persona admission machinery + the in-memory side-effect
//! stores that turn the stateless runner from PR-3 into a stateful loop.
//! This is the bridge between the IPC layer (`cognition/admit-inbox-message`)
//! and the pure-Rust admission gate from PRs 1-3.
//!
//! # What ships
//!
//! - [`AdmissionState`] — bundles a `InboxAdmissionRunner<HeuristicIsMemorable>`
//!   plus in-memory `SeenContentLookup` + `SeenEventLookup` impls plus a
//!   simple `Vec<Engram>` admitted-engram store. One per persona, owned by
//!   `PersonaCognition` (see `persona::unified`).
//! - `admit(message, trace)` — runs the full pipeline AND records the
//!   side-effects (admitted engram added to store, content_hash recorded
//!   for dedup, AIRC event_id recorded for replay protection).
//! - Read-only inspection: `engram_count()`, `engram_at()`,
//!   `is_content_seen()`, `is_event_seen()` — for tests + future recall
//!   surface (PR-5+).
//!
//! # What this PR does NOT ship (deferred)
//!
//! - **ORM persistence.** Engrams stay in-memory for v1. PR-5 swaps in
//!   ORM-backed lookups + the entity registry path so admitted engrams
//!   survive restarts.
//! - **Recall surface.** Reading admitted engrams back out is just
//!   `engram_at(idx)` for v1. PR-5+ adds a typed query API.
//! - **Quarantine store.** `Quarantine` decisions don't actually quarantine
//!   anywhere; the engram is dropped on the floor for now. (Replay
//!   protection still records the event_id, which is correct.) PR-5+ adds
//!   the quarantine store.
//! - **Per-persona config customization.** All personas use the same
//!   `default_v1()` runner config in this PR. Config-per-persona ships
//!   when the IPC layer needs it.
//!
//! # Concurrency
//!
//! `AdmissionState` is `Send + Sync`. Internal mutability via `Mutex` so
//! the struct can be borrowed immutably (`&AdmissionState`) and called
//! concurrently from per-persona task tasks. Same shape as `PersonaInbox`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use super::admission::{HeuristicIsMemorable, SeenContentLookup, SeenEventLookup};
use super::engram::{AdmissionDecision, AdmissionError, Engram, EngramOrigin};
use super::inbox_admission::InboxAdmissionRunner;
use super::trace::CognitionTrace;
use super::types::InboxMessage;

//=============================================================================
// IN-MEMORY ORACLES (private, used by AdmissionState)
//=============================================================================

#[derive(Default)]
struct InMemorySeenContent(Mutex<HashMap<String, Uuid>>);

impl SeenContentLookup for InMemorySeenContent {
    fn find_by_content_hash(&self, hash: &str) -> Option<Uuid> {
        self.0.lock().unwrap().get(hash).copied()
    }
}

impl InMemorySeenContent {
    fn record(&self, hash: String, engram_id: Uuid) {
        self.0.lock().unwrap().insert(hash, engram_id);
    }
}

#[derive(Default)]
struct InMemorySeenEvents(Mutex<HashMap<String, u64>>);

impl SeenEventLookup for InMemorySeenEvents {
    fn first_seen_ms(&self, event_id: &str) -> Option<u64> {
        self.0.lock().unwrap().get(event_id).copied()
    }
}

impl InMemorySeenEvents {
    fn record(&self, event_id: String, when_ms: u64) {
        self.0.lock().unwrap().insert(event_id, when_ms);
    }
}

//=============================================================================
// ADMISSION STATE
//=============================================================================

/// Per-persona admission bundle. Holds the runner + in-memory oracles +
/// admitted-engram store. One per persona, lazy-initialized on first
/// admission attempt or eagerly in `PersonaCognition::with_budget()`.
///
/// In-memory only for v1. PR-5 will swap the oracle + engram store for
/// ORM-backed implementations without changing this struct's public API.
pub struct AdmissionState {
    runner: InboxAdmissionRunner<HeuristicIsMemorable>,
    seen_content: Arc<InMemorySeenContent>,
    seen_events: Arc<InMemorySeenEvents>,
    engrams: Mutex<Vec<Engram>>,
}

impl Default for AdmissionState {
    fn default() -> Self {
        Self::new()
    }
}

impl AdmissionState {
    /// Construct fresh admission state with the v1 default recipe + permissive
    /// trust mapping. All personas use the same shape until per-persona
    /// config customization lands (PR-5+).
    pub fn new() -> Self {
        Self {
            runner: InboxAdmissionRunner::default_v1(),
            seen_content: Arc::new(InMemorySeenContent::default()),
            seen_events: Arc::new(InMemorySeenEvents::default()),
            engrams: Mutex::new(Vec::new()),
        }
    }

    /// Run the admission pipeline on one inbox message, recording all
    /// side-effects (admitted engram → store + content_hash dedup record;
    /// any signed origin → event_id replay record).
    ///
    /// Returns the typed `AdmissionDecision` (Admit/Drop/Quarantine) or a
    /// typed `AdmissionError`. Trace gets one `SEAM_ADMISSION` entry per
    /// call (success + every error path) — same forensic invariant as
    /// `AdmissionGate::admit`.
    pub fn admit(
        &self,
        message: &InboxMessage,
        trace: &mut CognitionTrace,
    ) -> Result<AdmissionDecision, AdmissionError> {
        let decision = self.runner.admit(
            message,
            self.seen_content.as_ref(),
            self.seen_events.as_ref(),
            trace,
        )?;
        self.record_side_effects(&decision);
        Ok(decision)
    }

    /// Apply the decision's side-effects to the stores. Pulled out so the
    /// admission path stays linear and testable.
    fn record_side_effects(&self, decision: &AdmissionDecision) {
        match decision {
            AdmissionDecision::Admit { engram, .. } => {
                self.record_engram_origin(engram);
                self.engrams.lock().unwrap().push(engram.clone());
            }
            AdmissionDecision::Quarantine { engram, .. } => {
                // Quarantine drops the engram on the floor for v1 (no
                // quarantine store yet — PR-5+). Replay protection still
                // applies: record the event_id so a duplicate quarantined
                // event doesn't re-fire admission.
                self.record_engram_origin(engram);
            }
            AdmissionDecision::Drop { .. } => {
                // Pure drop. No side-effect — by design, dropped messages
                // shouldn't bias future dedup or replay decisions.
            }
        }
    }

    /// Record content_hash + (for AIRC origins) event_id from the engram's
    /// origin. Pulled out so Admit + Quarantine share the same recording
    /// shape.
    fn record_engram_origin(&self, engram: &Engram) {
        match &engram.origin {
            EngramOrigin::Chat(r) => {
                self.seen_content
                    .record(r.content_hash.clone(), engram.id);
            }
            EngramOrigin::Airc(r) => {
                self.seen_content
                    .record(r.content_hash.clone(), engram.id);
                self.seen_events
                    .record(r.message_id.clone(), engram.admitted_at_ms);
            }
            EngramOrigin::Tool(_) | EngramOrigin::SelfReflection { .. } => {
                // Tool + SelfReflection origins don't carry a content_hash
                // string on a uniform field — dedup for those paths lands
                // when the tool/reflection ingestion converters land
                // (later PR). For now the admit path doesn't synthesize
                // these origins from the inbox path.
            }
        }
    }

    //--- read-only inspection (for tests + future recall surface) -----------

    /// Number of admitted engrams currently in this persona's store.
    pub fn engram_count(&self) -> usize {
        self.engrams.lock().unwrap().len()
    }

    /// Borrow an admitted engram by index (for inspection / future recall).
    /// Returns None if index out of bounds. Clone is cheap in v1; PR-5+
    /// recall will return `&Engram` borrowed from a longer-lived store.
    pub fn engram_at(&self, idx: usize) -> Option<Engram> {
        self.engrams.lock().unwrap().get(idx).cloned()
    }

    /// True iff `content_hash` is recorded as seen in the dedup store.
    pub fn is_content_seen(&self, content_hash: &str) -> bool {
        self.seen_content.find_by_content_hash(content_hash).is_some()
    }

    /// True iff the AIRC event_id is recorded in the replay-protection store.
    pub fn is_event_seen(&self, event_id: &str) -> bool {
        self.seen_events.first_seen_ms(event_id).is_some()
    }

    /// Borrow the runner — useful for tests + introspection of per-persona
    /// config (recipe id, trust thresholds, etc.).
    pub fn runner(&self) -> &InboxAdmissionRunner<HeuristicIsMemorable> {
        &self.runner
    }
}

//=============================================================================
// TESTS
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::admission::IsMemorable as _;
    use crate::persona::engram::AdmissionDropReason;
    use crate::persona::inbox_admission::content_hash_sha256;
    use crate::persona::types::SenderType;

    fn synthetic_human_message(content: &str) -> InboxMessage {
        InboxMessage {
            id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            sender_name: "test-human".to_string(),
            sender_type: SenderType::Human,
            content: content.to_string(),
            timestamp: 1_715_625_600_000,
            priority: 0.5,
            source_modality: None,
            voice_session_id: None,
        }
    }

    /// What this catches: a clean admit records the engram in the store,
    /// records the content_hash for dedup, AND a subsequent admit of the
    /// SAME content gets dropped as Duplicate (proving the side-effect
    /// recording actually feeds back into the next call's recipe).
    #[test]
    fn admit_records_engram_and_dedup_blocks_repeat() {
        let state = AdmissionState::new();
        let mut trace = CognitionTrace::new();
        let content = "this is a non-trivial design observation worth storing";
        let msg = synthetic_human_message(content);

        let first = state.admit(&msg, &mut trace).unwrap();
        assert!(matches!(first, AdmissionDecision::Admit { .. }));
        assert_eq!(state.engram_count(), 1);
        assert!(state.is_content_seen(&content_hash_sha256(content)));

        // Second admit of identical content (different message id, same content)
        // should drop as Duplicate.
        let msg2 = synthetic_human_message(content);
        let second = state.admit(&msg2, &mut trace).unwrap();
        match second {
            AdmissionDecision::Drop {
                reason: AdmissionDropReason::Duplicate { .. },
            } => {}
            other => panic!("expected Drop Duplicate, got {other:?}"),
        }
        // No new engram was admitted.
        assert_eq!(state.engram_count(), 1);
    }

    /// What this catches: dropped messages do NOT pollute either store.
    /// A dropped message's content_hash should NOT be in seen_content
    /// (otherwise a later legit version of the same content would be
    /// blocked as duplicate against a non-existent engram).
    #[test]
    fn dropped_message_records_no_side_effect() {
        let state = AdmissionState::new();
        let mut trace = CognitionTrace::new();
        // Short content → drops with NotMemorable.
        let msg = synthetic_human_message("short");

        let decision = state.admit(&msg, &mut trace).unwrap();
        match decision {
            AdmissionDecision::Drop {
                reason: AdmissionDropReason::NotMemorable { .. },
            } => {}
            other => panic!("expected Drop NotMemorable, got {other:?}"),
        }
        assert_eq!(state.engram_count(), 0);
        assert!(!state.is_content_seen(&content_hash_sha256("short")));
    }

    /// What this catches: admitted engrams accumulate in admission order
    /// + each engram is retrievable by index. Future recall surface
    /// depends on this; missing items would silently break recall.
    #[test]
    fn admitted_engrams_accumulate_in_order_and_are_retrievable() {
        let state = AdmissionState::new();
        let mut trace = CognitionTrace::new();
        let messages = [
            "first design observation worth recording",
            "second design observation worth recording",
            "third design observation worth recording",
        ];
        for content in messages {
            let _ = state.admit(&synthetic_human_message(content), &mut trace);
        }
        assert_eq!(state.engram_count(), 3);
        assert_eq!(
            state.engram_at(0).expect("first engram present").content,
            messages[0]
        );
        assert_eq!(
            state.engram_at(2).expect("third engram present").content,
            messages[2]
        );
        assert!(state.engram_at(99).is_none(), "out-of-bounds returns None");
    }

    /// What this catches: the trace seam invariant carries through the
    /// state wrapper. Every admit() call (success + drop) appends exactly
    /// one SEAM_ADMISSION to the trace. Same forensic guarantee as the
    /// underlying runner.
    #[test]
    fn admit_emits_one_seam_per_call_through_state_wrapper() {
        let state = AdmissionState::new();
        let mut trace = CognitionTrace::new();
        // Three admits with three different outcomes:
        // (1) admit, (2) drop short, (3) drop duplicate of #1.
        let msg1 = synthetic_human_message("a long enough observation worth recording");
        let msg2 = synthetic_human_message("short");
        let msg3 = synthetic_human_message("a long enough observation worth recording");
        let _ = state.admit(&msg1, &mut trace);
        let _ = state.admit(&msg2, &mut trace);
        let _ = state.admit(&msg3, &mut trace);
        assert_eq!(trace.seam_count(), 3, "one seam per admit() call");
    }

    /// What this catches: the runner accessor returns the configured
    /// runner so callers can introspect (recipe id for trace metadata,
    /// trust thresholds for debugging). A regression in the accessor
    /// would silently hide config from observability surfaces.
    #[test]
    fn runner_accessor_exposes_default_v1_config() {
        let state = AdmissionState::new();
        assert_eq!(state.runner().recipe().id(), "heuristic.v1");
    }

    /// What this catches: AdmissionState is Send + Sync. Compile-time
    /// proof that it can live inside `PersonaCognition` (which is held in
    /// a `DashMap<Uuid, PersonaCognition>` + crossed across tokio tasks).
    /// If a future refactor drops Send/Sync, this test fails to compile.
    #[test]
    fn admission_state_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AdmissionState>();
    }
}
