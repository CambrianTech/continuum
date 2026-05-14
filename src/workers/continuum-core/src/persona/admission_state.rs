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
    ///
    /// **Quarantine subtlety (claude-tab-2 review nit on #1155):** v1 has
    /// no quarantine store, so a Quarantined engram gets dropped on the
    /// floor. Recording its `content_hash` in `seen_content` would leave
    /// a dangling pointer — future dedup hits would return an
    /// `existing_engram_id` that can't be looked up. So Quarantine ONLY
    /// records the `event_id` (replay protection — the load-bearing
    /// behaviour for `AdmissionError::ReplayDetected`). Once PR-5+ adds
    /// a real quarantine store, the engram lands somewhere lookup-able
    /// and content_hash recording can come back.
    fn record_side_effects(&self, decision: &AdmissionDecision) {
        match decision {
            AdmissionDecision::Admit { engram, .. } => {
                self.record_admitted(engram);
                self.engrams.lock().unwrap().push(engram.clone());
            }
            AdmissionDecision::Quarantine { engram, .. } => {
                // Replay-only recording — see method-doc Quarantine note.
                self.record_replay_only(engram);
            }
            AdmissionDecision::Drop { .. } => {
                // Pure drop. No side-effect — by design, dropped messages
                // shouldn't bias future dedup or replay decisions.
            }
        }
    }

    /// Full recording for an admitted engram: content_hash → engram_id
    /// (dedup) PLUS, for AIRC origins, event_id → timestamp (replay).
    /// Use only when the engram is actually being stored, otherwise the
    /// dedup pointer dangles.
    fn record_admitted(&self, engram: &Engram) {
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

    /// Replay-only recording for a Quarantined engram: event_id → timestamp
    /// for AIRC origins (so a duplicate quarantined event doesn't re-fire
    /// admission). Skips content_hash because v1 doesn't actually store
    /// quarantined engrams; recording dedup pointers to dropped engrams
    /// would leave dangling `existing_engram_id` references in
    /// `AdmissionDropReason::Duplicate` results.
    fn record_replay_only(&self, engram: &Engram) {
        if let EngramOrigin::Airc(r) = &engram.origin {
            self.seen_events
                .record(r.message_id.clone(), engram.admitted_at_ms);
        }
        // Chat / Tool / SelfReflection origins have no replay surface
        // distinct from content dedup, so quarantine of those origins
        // records nothing here. PR-5's quarantine store will revisit.
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
    use crate::persona::engram::{
        AdmissionDropReason, AircMessageRef, ChatMessageRef, EngramKind, TrustState,
    };
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

    // ── Quarantine side-effect rule (claude-tab-2 review nit on #1155) ──
    //
    // v1 has no quarantine store, so a Quarantined engram is dropped on
    // the floor. Recording its content_hash → engram_id in the dedup
    // store would leave a dangling pointer (future Duplicate drops would
    // surface an existing_engram_id that can't be looked up). The right
    // behaviour: ONLY record event_id (replay protection still applies),
    // never record content_hash on Quarantine.
    //
    // These tests construct synthetic AdmissionDecision values + call
    // `record_side_effects` directly so they don't need a custom recipe
    // — the heuristic recipe shipped here doesn't naturally emit
    // Quarantine, but the rule is about the side-effect helper itself.

    fn synthetic_engram_with_chat_origin(content: &str) -> Engram {
        Engram {
            id: Uuid::new_v4(),
            kind: EngramKind::Episodic,
            content: content.to_string(),
            origin: EngramOrigin::Chat(ChatMessageRef {
                message_id: Uuid::new_v4(),
                room_id: Uuid::new_v4(),
                sender_id: Uuid::new_v4(),
                posted_at_ms: 1_000_000,
                content_hash: content_hash_sha256(content),
            }),
            recall_keys: vec!["test".to_string()],
            admitted_at_ms: 1_000_000,
            trust_state_at_admission: TrustState::ApprovedPeer,
            admission_trace_id: None,
        }
    }

    fn synthetic_engram_with_airc_origin(content: &str, message_id: &str) -> Engram {
        Engram {
            id: Uuid::new_v4(),
            kind: EngramKind::Episodic,
            content: content.to_string(),
            origin: EngramOrigin::Airc(AircMessageRef {
                transport: "airc".to_string(),
                room_id: "cambriantech".to_string(),
                message_id: message_id.to_string(),
                sender_id: "airc-8a5e".to_string(),
                sent_at_ms: 1_000_000,
                received_at_ms: 1_000_000,
                content_hash: content_hash_sha256(content),
                signature: "sig".to_string(),
                proof_refs: vec![],
                schema_version: "v1".to_string(),
                client_name: None,
            }),
            recall_keys: vec!["test".to_string()],
            admitted_at_ms: 1_000_000,
            trust_state_at_admission: TrustState::ApprovedPeer,
            admission_trace_id: None,
        }
    }

    /// What this catches: Quarantine of a Chat-origin engram records
    /// NEITHER content_hash NOR event_id. Chat origins have no replay
    /// surface distinct from content dedup, so quarantine on chat is a
    /// pure no-op as far as the side-effect stores are concerned.
    /// Original PR-4 code recorded content_hash here, leaving a dangling
    /// pointer.
    #[test]
    fn quarantine_chat_origin_records_no_side_effects() {
        let state = AdmissionState::new();
        let engram = synthetic_engram_with_chat_origin("borderline observation");
        let content_hash = match &engram.origin {
            EngramOrigin::Chat(r) => r.content_hash.clone(),
            _ => unreachable!(),
        };
        let decision = AdmissionDecision::Quarantine {
            engram,
            reason: "test borderline".to_string(),
            expiry_ms: 2_000_000,
        };

        state.record_side_effects(&decision);

        assert!(
            !state.is_content_seen(&content_hash),
            "chat-origin quarantine MUST NOT record content_hash (would dangle)"
        );
        assert_eq!(state.engram_count(), 0, "quarantine MUST NOT add to engram store");
    }

    /// What this catches: Quarantine of an AIRC-origin engram records
    /// the event_id (replay protection — the load-bearing behaviour) but
    /// MUST NOT record the content_hash (which would dangle since v1
    /// doesn't store quarantined engrams).
    #[test]
    fn quarantine_airc_origin_records_event_id_only_not_content_hash() {
        let state = AdmissionState::new();
        let event_id = "airc-msg-quarantine-1";
        let engram = synthetic_engram_with_airc_origin(
            "borderline observation worth holding",
            event_id,
        );
        let content_hash = match &engram.origin {
            EngramOrigin::Airc(r) => r.content_hash.clone(),
            _ => unreachable!(),
        };
        let decision = AdmissionDecision::Quarantine {
            engram,
            reason: "test borderline".to_string(),
            expiry_ms: 2_000_000,
        };

        state.record_side_effects(&decision);

        assert!(
            state.is_event_seen(event_id),
            "airc-origin quarantine MUST record event_id (replay protection)"
        );
        assert!(
            !state.is_content_seen(&content_hash),
            "airc-origin quarantine MUST NOT record content_hash (would dangle)"
        );
        assert_eq!(state.engram_count(), 0, "quarantine MUST NOT add to engram store");
    }

    /// What this catches: Admit (NOT Quarantine) records BOTH content_hash
    /// AND event_id for AIRC origins. This is the regression-anchor for
    /// the refactor that split `record_engram_origin` → `record_admitted`
    /// + `record_replay_only`. If the refactor accidentally narrowed the
    /// Admit path's recording, dedup would silently break.
    #[test]
    fn admit_airc_origin_still_records_both_content_hash_and_event_id() {
        let state = AdmissionState::new();
        let event_id = "airc-msg-admit-1";
        let engram = synthetic_engram_with_airc_origin(
            "valuable observation worth recalling",
            event_id,
        );
        let content_hash = match &engram.origin {
            EngramOrigin::Airc(r) => r.content_hash.clone(),
            _ => unreachable!(),
        };
        let decision = AdmissionDecision::Admit {
            engram,
            why: "test admit".to_string(),
        };

        state.record_side_effects(&decision);

        assert!(
            state.is_event_seen(event_id),
            "airc-origin admit MUST record event_id"
        );
        assert!(
            state.is_content_seen(&content_hash),
            "airc-origin admit MUST record content_hash"
        );
        assert_eq!(state.engram_count(), 1, "admit MUST add to engram store");
    }
}
