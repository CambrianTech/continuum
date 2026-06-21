//! Inbox → Admission Bridge (continuum#1121 PR-3)
//!
//! Closes the e2e admission loop on top of the storage types (PR-1, #1129)
//! and the gate machinery (PR-2, #1134) by giving callers ONE pure-Rust
//! object — `InboxAdmissionRunner` — that wraps:
//!
//! - The configured `IsMemorable` recipe for this persona
//! - The `AdmissionConfig` thresholds
//! - The injected `SeenContentLookup` + `SeenEventLookup` oracles
//! - The persona-specific `TrustMapping` (SenderType → TrustState)
//!
//! and exposes a single method `runner.admit(&inbox_msg, &mut trace)` that
//! returns the typed `AdmissionDecision`. This is the seam the PersonaInbox
//! processing path (call-site integration in PR-4) calls per drained
//! message.
//!
//! # What this PR ships
//!
//! - `InboxAdmissionRunner` — the per-persona runner.
//! - `TrustMapping` — configurable map from `SenderType` to `TrustState`,
//!   with `default_v1()` (permissive — Human=IntragridMember, Persona=
//!   ApprovedPeer, Agent=ApprovedPeer, System=SelfTrust) and
//!   `strict_v1()` (Persona/Agent demoted to Authenticated).
//! - `inbox_message_to_candidate(msg, mapping) -> AdmissionCandidate` —
//!   pure conversion. Synthesizes a `ChatMessageRef` origin (the existing
//!   inbox path is internal Continuum chat; AIRC-origin admission lands in
//!   PR-5 alongside the AIRC event converter).
//! - `content_hash_sha256(s) -> String` — canonical content hash format
//!   (`"sha256:<hex>"`) used by the converter so dedup is consistent
//!   across all admission paths.
//! - 16 unit tests covering conversion + every admission outcome through
//!   the runner.
//!
//! # What this PR does NOT ship
//!
//! - **Call-site integration** with `PersonaInbox::drain_frame()`. PR-4
//!   adds the actual call from the cognition path. This module ships the
//!   bridge that PR-4 will plug in.
//! - **Engram persistence**. Admitted engrams come back from the runner;
//!   the caller stores them. PR-5+ adds the ORM persistence path.
//! - **AIRC envelope origin**. Internal chat → `EngramOrigin::Chat`. The
//!   AIRC envelope path lives in `engram::AircMessageRef` already (from
//!   PR-1) but the inbox->AIRC converter is a separate slice (PR-5+)
//!   because AIRC events carry signature/proof material the chat inbox
//!   does not.
//!
//! # Design choices
//!
//! - **Runner owns Recipe + Config + TrustMapping; oracles injected per
//!   call.** Same shape as the gate from PR-2: state that lives across
//!   calls (recipe configuration) is owned; state that varies per call
//!   (engram store, seen-events store) is injected. Keeps the runner
//!   trivially testable and persona-shareable.
//! - **Pure conversion functions are public.** `inbox_message_to_candidate`,
//!   `content_hash_sha256`, and `inbox_message_to_origin` are exposed so
//!   PR-4's call-site integration plus future tests can reuse them without
//!   constructing a runner.
//! - **No `AircMessageRef` synthesis here.** Chat-origin only. AIRC origin
//!   needs envelope material this module's input doesn't carry; that
//!   conversion is a separate function in a separate slice (PR-5+).

use std::fmt::Write as _;

use sha2::{Digest, Sha256};

use super::admission::{
    AdmissionCandidate, AdmissionConfig, AdmissionContext, AdmissionGate, IsMemorable,
    SeenContentLookup, SeenEventLookup,
};
use super::engram::{
    AdmissionDecision, AdmissionError, ChatMessageRef, EngramKind, EngramOrigin, TrustState,
};
use super::trace::CognitionTrace;
use super::types::{InboxMessage, SenderType};

//=============================================================================
// TRUST MAPPING
//=============================================================================

/// Per-persona mapping from inbox `SenderType` to admission `TrustState`.
///
/// Different personas may apply different trust to the same sender class —
/// a SOC governance persona will treat external Agents as `Authenticated`
/// (verify-then-decide), while a fuzzy collab persona treats them as
/// `ApprovedPeer` (already-in-the-room). The mapping is data, not logic;
/// callers can override per persona.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrustMapping {
    pub human: TrustState,
    pub persona: TrustState,
    pub agent: TrustState,
    pub system: TrustState,
}

impl TrustMapping {
    /// Permissive default — internal Continuum chat is the trusted polity:
    /// human peers are intragrid members, AI personas are approved peers,
    /// system-emitted messages are self-trust. Suitable for the v1 chat
    /// path where everyone in the room has already passed the door.
    pub fn default_v1() -> Self {
        Self {
            human: TrustState::IntragridMember,
            persona: TrustState::ApprovedPeer,
            agent: TrustState::ApprovedPeer,
            system: TrustState::SelfTrust,
        }
    }

    /// Strict variant — demotes Persona + Agent to `Authenticated`,
    /// requiring downstream policy to do per-message judgment rather
    /// than blanket-trusting the room. Pairs with `AdmissionConfig::strict_v1`
    /// in SOC governance contexts.
    pub fn strict_v1() -> Self {
        Self {
            human: TrustState::IntragridMember,
            persona: TrustState::Authenticated,
            agent: TrustState::Authenticated,
            system: TrustState::SelfTrust,
        }
    }

    /// Resolve a `SenderType` to its configured `TrustState`.
    pub fn resolve(&self, sender: SenderType) -> TrustState {
        match sender {
            SenderType::Human => self.human,
            SenderType::Persona => self.persona,
            SenderType::Agent => self.agent,
            SenderType::System => self.system,
        }
    }
}

//=============================================================================
// PURE CONVERSION
//=============================================================================

/// Canonical content hash format used by all admission paths. Returns
/// `"sha256:<lowercase-hex>"` so dedup keys are stable across origin
/// kinds (chat / AIRC / tool) and machine boundaries.
///
/// Hot path: called once per inbox message at admission time. Hex
/// encoding writes directly into the preallocated string buffer
/// (single allocation total) rather than `format!()` per byte (which
/// allocated 32 small `String`s per hash). See claude-tab-2's review
/// nit on continuum#1143.
pub fn content_hash_sha256(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(7 + digest.len() * 2);
    hex.push_str("sha256:");
    for byte in digest {
        // `write!` into a `String` cannot fail — the `Write` impl for
        // `String` returns `Ok` unconditionally — so the unwrap is the
        // standard idiom for this pattern.
        write!(&mut hex, "{:02x}", byte).expect("write to String never fails");
    }
    hex
}

/// Build the `ChatMessageRef` for an inbox-sourced engram. Uses the
/// canonical sha256 of `content` (matching whatever `content_hash` the
/// candidate carries) so engram-side forensic re-verification works.
pub fn inbox_message_to_origin(msg: &InboxMessage) -> EngramOrigin {
    EngramOrigin::Chat(ChatMessageRef {
        message_id: msg.id,
        room_id: msg.room_id,
        sender_id: msg.sender_id,
        posted_at_ms: msg.timestamp,
        content_hash: content_hash_sha256(&msg.content),
    })
}

/// Convert a drained `InboxMessage` into an `AdmissionCandidate` ready
/// for `AdmissionGate::admit`. Pure function; no I/O, no allocation
/// beyond the candidate fields themselves.
///
/// `kind` is `EngramKind::Episodic` — chat messages are observations of
/// what happened in the room. Recipes that admit to other kinds (e.g.,
/// a persona digesting an episodic engram into a semantic fact) belong
/// in PR-5+ when the digest pipeline lands.
pub fn inbox_message_to_candidate(
    msg: &InboxMessage,
    mapping: &TrustMapping,
) -> AdmissionCandidate {
    AdmissionCandidate {
        content: msg.content.clone(),
        kind: EngramKind::Episodic,
        origin: inbox_message_to_origin(msg),
        trust_state: mapping.resolve(msg.sender_type),
        recall_keys: vec![msg.sender_name.clone()],
        content_hash: content_hash_sha256(&msg.content),
    }
}

//=============================================================================
// RUNNER
//=============================================================================

/// Per-persona admission runner. Owns the recipe + config + trust map;
/// oracles get injected per `admit()` call so the runner stays sharable
/// (e.g., across tokio tasks for the same persona). Same compositional
/// shape as the underlying `AdmissionGate` from PR-2.
///
/// Generic over the recipe type so call sites can plug in custom
/// `IsMemorable` impls without dynamic dispatch overhead in the v1 sync
/// hot path. Use `InboxAdmissionRunner<HeuristicIsMemorable>::default_v1()`
/// for the simple case.
pub struct InboxAdmissionRunner<R: IsMemorable> {
    recipe: R,
    config: AdmissionConfig,
    trust_mapping: TrustMapping,
}

impl<R: IsMemorable> InboxAdmissionRunner<R> {
    /// Construct a runner with explicit recipe + config + trust mapping.
    /// Use this for custom IsMemorable impls or for SOC-strict configs.
    pub fn new(recipe: R, config: AdmissionConfig, trust_mapping: TrustMapping) -> Self {
        Self {
            recipe,
            config,
            trust_mapping,
        }
    }

    /// Borrow the recipe (for trace metadata, custom inspection).
    pub fn recipe(&self) -> &R {
        &self.recipe
    }

    /// Borrow the config (so callers can read thresholds without owning).
    pub fn config(&self) -> &AdmissionConfig {
        &self.config
    }

    /// Borrow the trust mapping.
    pub fn trust_mapping(&self) -> &TrustMapping {
        &self.trust_mapping
    }

    /// Run the admission pipeline on one inbox message. Returns the typed
    /// decision (Admit/Drop/Quarantine) or a typed error. A `SEAM_ADMISSION`
    /// entry is appended to `trace` on every path (success + error)
    /// — same forensic invariant as `AdmissionGate::admit`.
    ///
    /// Caller responsibilities:
    /// - Provide `seen_content` + `seen_events` lookup oracles backed by
    ///   whatever engram store / replay log this persona uses.
    /// - On `Admit`: persist `engram` to the engram store + record the
    ///   `content_hash` in the seen-content store.
    /// - On `Quarantine`: hold `engram` in the quarantine store until
    ///   `expiry_ms`.
    /// - On `Drop`: log the reason for funnel observability + discard.
    pub fn admit<'a>(
        &self,
        msg: &InboxMessage,
        seen_content: &'a dyn SeenContentLookup,
        seen_events: &'a dyn SeenEventLookup,
        trace: Option<&mut CognitionTrace>,
    ) -> Result<AdmissionDecision, AdmissionError> {
        let candidate = inbox_message_to_candidate(msg, &self.trust_mapping);
        // Scope the admission to the message's room (the contextId), so the
        // minted engram is keyed to its conversation within the persona's
        // identity (per IDENTITY-SCOPE-PEER-LIVENESS-MODEL.md Part A).
        let ctx = AdmissionContext::new(&self.config, seen_content, seen_events)
            .with_context(msg.room_id);
        AdmissionGate::admit(&candidate, &self.recipe, &ctx, trace)
    }
}

//=============================================================================
// CONVENIENCE CONSTRUCTORS for the v1 default recipe
//=============================================================================

use super::admission::HeuristicIsMemorable;

impl InboxAdmissionRunner<HeuristicIsMemorable> {
    /// Permissive v1 defaults — pairs `HeuristicIsMemorable::default_v1()`
    /// with `AdmissionConfig::permissive_v1()` + `TrustMapping::default_v1()`.
    /// Suitable as a starting point for any chat-driven persona.
    pub fn default_v1() -> Self {
        Self {
            recipe: HeuristicIsMemorable::default_v1(),
            config: AdmissionConfig::permissive_v1(),
            trust_mapping: TrustMapping::default_v1(),
        }
    }

    /// SOC-strict v1 — pairs the same heuristic recipe with the strict
    /// admission config + strict trust mapping. Same recipe, tighter
    /// gate.
    pub fn strict_v1() -> Self {
        Self {
            recipe: HeuristicIsMemorable::default_v1(),
            config: AdmissionConfig::strict_v1(),
            trust_mapping: TrustMapping::strict_v1(),
        }
    }
}

//=============================================================================
// TESTS
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::engram::AdmissionDropReason;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use uuid::Uuid;

    const FIXED_NOW_MS: u64 = 1_715_625_600_000;

    // ── test doubles for the lookup oracles ─────────────────────────────

    #[derive(Default)]
    struct InMemoryContent(Mutex<HashMap<String, Uuid>>);

    impl SeenContentLookup for InMemoryContent {
        fn find_by_content_hash(&self, hash: &str) -> Option<Uuid> {
            self.0.lock().unwrap().get(hash).copied()
        }
    }

    #[derive(Default)]
    struct InMemoryEvents(Mutex<HashMap<String, u64>>);

    impl SeenEventLookup for InMemoryEvents {
        fn first_seen_ms(&self, event_id: &str) -> Option<u64> {
            self.0.lock().unwrap().get(event_id).copied()
        }
    }

    fn synthetic_message(content: &str, sender_type: SenderType) -> InboxMessage {
        InboxMessage {
            id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            sender_name: "test-sender".to_string(),
            sender_type,
            content: content.to_string(),
            timestamp: FIXED_NOW_MS,
            priority: 0.5,
            source_modality: None,
            voice_session_id: None,
        }
    }

    // ── content_hash_sha256 ─────────────────────────────────────────────

    /// What this catches: the canonical hash format is `"sha256:<hex>"`
    /// with lowercase hex + 64 hex chars (32 bytes). Any drift in the
    /// format breaks dedup keys across machines + breaks consumers that
    /// pattern-match on the prefix.
    #[test]
    fn content_hash_format_is_canonical() {
        let hash = content_hash_sha256("hello, world");
        assert!(hash.starts_with("sha256:"), "got: {hash}");
        let hex = &hash["sha256:".len()..];
        assert_eq!(
            hex.len(),
            64,
            "hex must be 64 chars (32-byte SHA-256): {hex}"
        );
        assert!(
            hex.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "hex must be lowercase: {hex}"
        );
    }

    /// What this catches: the same input always produces the same hash.
    /// If sha2 is swapped for a non-deterministic hash (or salting is
    /// accidentally introduced), dedup breaks silently.
    #[test]
    fn content_hash_is_deterministic() {
        assert_eq!(
            content_hash_sha256("identical content"),
            content_hash_sha256("identical content")
        );
    }

    /// What this catches: different inputs produce different hashes.
    /// Trivial property but the foundation of dedup correctness.
    #[test]
    fn content_hash_distinguishes_different_inputs() {
        assert_ne!(
            content_hash_sha256("content one"),
            content_hash_sha256("content two")
        );
    }

    // ── TrustMapping ────────────────────────────────────────────────────

    /// What this catches: the documented v1 mapping (Human=IntragridMember,
    /// Persona/Agent=ApprovedPeer, System=SelfTrust). A regression here
    /// silently changes the trust posture of every chat-driven persona.
    #[test]
    fn trust_mapping_default_v1_documented_values() {
        let m = TrustMapping::default_v1();
        assert_eq!(m.resolve(SenderType::Human), TrustState::IntragridMember);
        assert_eq!(m.resolve(SenderType::Persona), TrustState::ApprovedPeer);
        assert_eq!(m.resolve(SenderType::Agent), TrustState::ApprovedPeer);
        assert_eq!(m.resolve(SenderType::System), TrustState::SelfTrust);
    }

    /// What this catches: strict mapping demotes Persona + Agent to
    /// Authenticated (forces per-message policy judgment) while keeping
    /// Human + System at their intragrid/self trust. SOC governance
    /// personas depend on this distinction.
    #[test]
    fn trust_mapping_strict_v1_demotes_persona_and_agent() {
        let m = TrustMapping::strict_v1();
        assert_eq!(m.resolve(SenderType::Human), TrustState::IntragridMember);
        assert_eq!(m.resolve(SenderType::Persona), TrustState::Authenticated);
        assert_eq!(m.resolve(SenderType::Agent), TrustState::Authenticated);
        assert_eq!(m.resolve(SenderType::System), TrustState::SelfTrust);
    }

    // ── inbox_message_to_origin ─────────────────────────────────────────

    /// What this catches: inbox messages always become `EngramOrigin::Chat`,
    /// never `EngramOrigin::Airc`. AIRC envelope material isn't carried by
    /// `InboxMessage`; admitting an inbox-sourced engram as Airc would
    /// fabricate signature/proof fields the source never produced.
    #[test]
    fn inbox_origin_is_always_chat() {
        let msg = synthetic_message("hi", SenderType::Human);
        match inbox_message_to_origin(&msg) {
            EngramOrigin::Chat(r) => {
                assert_eq!(r.message_id, msg.id);
                assert_eq!(r.room_id, msg.room_id);
                assert_eq!(r.sender_id, msg.sender_id);
                assert_eq!(r.posted_at_ms, msg.timestamp);
                assert_eq!(r.content_hash, content_hash_sha256("hi"));
            }
            other => panic!("expected Chat origin, got {other:?}"),
        }
    }

    // ── inbox_message_to_candidate ──────────────────────────────────────

    /// What this catches: the converter populates the candidate fields
    /// from the message + applies the trust mapping correctly. The
    /// content_hash on the candidate must match the one on the synthesized
    /// origin's ChatMessageRef so dedup is consistent.
    #[test]
    fn candidate_carries_full_provenance_from_message() {
        let msg = synthetic_message("a non-trivial design observation", SenderType::Human);
        let cand = inbox_message_to_candidate(&msg, &TrustMapping::default_v1());
        assert_eq!(cand.content, "a non-trivial design observation");
        assert_eq!(cand.kind, EngramKind::Episodic);
        assert_eq!(cand.trust_state, TrustState::IntragridMember);
        assert_eq!(cand.recall_keys, vec!["test-sender".to_string()]);
        // Content hash on candidate matches the origin's
        if let EngramOrigin::Chat(ref r) = cand.origin {
            assert_eq!(
                r.content_hash, cand.content_hash,
                "candidate.content_hash must equal origin.content_hash"
            );
        } else {
            panic!("expected Chat origin");
        }
    }

    /// What this catches: candidate inherits trust from the trust mapping,
    /// not from any default. Different SenderTypes produce different
    /// trust_states. A regression here would silently homogenize trust.
    #[test]
    fn candidate_trust_varies_by_sender_type() {
        let mapping = TrustMapping::default_v1();
        let h = inbox_message_to_candidate(&synthetic_message("x", SenderType::Human), &mapping);
        let p = inbox_message_to_candidate(&synthetic_message("x", SenderType::Persona), &mapping);
        let a = inbox_message_to_candidate(&synthetic_message("x", SenderType::Agent), &mapping);
        let s = inbox_message_to_candidate(&synthetic_message("x", SenderType::System), &mapping);
        assert_eq!(h.trust_state, TrustState::IntragridMember);
        assert_eq!(p.trust_state, TrustState::ApprovedPeer);
        assert_eq!(a.trust_state, TrustState::ApprovedPeer);
        assert_eq!(s.trust_state, TrustState::SelfTrust);
    }

    // ── runner: end-to-end admission paths ──────────────────────────────

    /// What this catches: a non-trivial human message from an internal
    /// chat passes the runner cleanly + emerges as an Admit decision
    /// carrying a Chat-origin engram. The headline e2e success case.
    #[test]
    fn runner_admits_well_formed_human_message() {
        let runner = InboxAdmissionRunner::default_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let mut trace = CognitionTrace::new();
        let msg = synthetic_message(
            "the admission gate ratchet test fired correctly today",
            SenderType::Human,
        );

        let decision = runner
            .admit(&msg, &content, &events, Some(&mut trace))
            .expect("well-formed message should admit cleanly");
        match decision {
            AdmissionDecision::Admit { engram, .. } => {
                assert_eq!(engram.kind, EngramKind::Episodic);
                assert_eq!(engram.trust_state_at_admission, TrustState::IntragridMember);
                if let EngramOrigin::Chat(ref r) = engram.origin {
                    assert_eq!(r.message_id, msg.id);
                } else {
                    panic!("engram origin should be Chat");
                }
            }
            other => panic!("expected Admit, got {other:?}"),
        }
        // SEAM_ADMISSION emitted exactly once.
        assert_eq!(trace.seam_count(), 1);
    }

    /// What this catches: short content hits the heuristic length check
    /// → `Drop::NotMemorable`. Demonstrates the recipe is actually
    /// consulted via the runner (not bypassed).
    #[test]
    fn runner_drops_short_content_via_heuristic() {
        let runner = InboxAdmissionRunner::default_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let mut trace = CognitionTrace::new();
        let msg = synthetic_message("short", SenderType::Human);

        match runner
            .admit(&msg, &content, &events, Some(&mut trace))
            .unwrap()
        {
            AdmissionDecision::Drop {
                reason: AdmissionDropReason::NotMemorable { .. },
            } => {}
            other => panic!("expected Drop NotMemorable, got {other:?}"),
        }
    }

    /// What this catches: a duplicate content_hash already in the
    /// `seen_content` oracle → `Drop::Duplicate` carrying the existing
    /// engram id. End-to-end dedup proof through the runner.
    #[test]
    fn runner_drops_duplicate_content_with_existing_id() {
        let runner = InboxAdmissionRunner::default_v1();
        let existing = Uuid::new_v4();
        let content_text = "well-formed observation worth storing";
        let pre_hash = content_hash_sha256(content_text);
        let content = InMemoryContent::default();
        content.0.lock().unwrap().insert(pre_hash, existing);
        let events = InMemoryEvents::default();
        let mut trace = CognitionTrace::new();

        let msg = synthetic_message(content_text, SenderType::Human);
        match runner
            .admit(&msg, &content, &events, Some(&mut trace))
            .unwrap()
        {
            AdmissionDecision::Drop {
                reason: AdmissionDropReason::Duplicate { existing_engram_id },
            } => {
                assert_eq!(existing_engram_id, existing);
            }
            other => panic!("expected Drop Duplicate, got {other:?}"),
        }
    }

    /// What this catches: System-emitted messages get SelfTrust → admit
    /// even with strict config (which would reject Authenticated). Proves
    /// the trust mapping reaches the gate's threshold check correctly.
    #[test]
    fn runner_strict_admits_system_messages_via_self_trust() {
        let runner = InboxAdmissionRunner::strict_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let mut trace = CognitionTrace::new();
        let msg = synthetic_message(
            "system-generated event observation worth memorising",
            SenderType::System,
        );

        let decision = runner
            .admit(&msg, &content, &events, Some(&mut trace))
            .expect("system messages reach SelfTrust which clears any threshold");
        assert!(matches!(decision, AdmissionDecision::Admit { .. }));
    }

    /// What this catches: under strict config, Persona-emitted messages
    /// hit the `Authenticated < IntragridMember` threshold and get
    /// `TrustBoundaryRejected` BEFORE the recipe runs. Demonstrates that
    /// strict mode actually tightens admission, not just decoration.
    #[test]
    fn runner_strict_rejects_persona_messages_at_trust_boundary() {
        let runner = InboxAdmissionRunner::strict_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let mut trace = CognitionTrace::new();
        let msg = synthetic_message(
            "persona-emitted observation that would otherwise admit",
            SenderType::Persona,
        );

        match runner.admit(&msg, &content, &events, Some(&mut trace)) {
            Err(AdmissionError::TrustBoundaryRejected {
                source_trust,
                threshold,
            }) => {
                assert_eq!(source_trust, TrustState::Authenticated);
                assert_eq!(threshold, TrustState::IntragridMember);
            }
            other => panic!("expected TrustBoundaryRejected, got {other:?}"),
        }
    }

    /// What this catches: the runner's accessors (`recipe()`, `config()`,
    /// `trust_mapping()`) actually return the configured values. Useful
    /// for callers introspecting persona admission state without
    /// reconstructing the runner.
    #[test]
    fn runner_accessors_expose_configured_state() {
        let runner = InboxAdmissionRunner::default_v1();
        assert_eq!(runner.recipe().id(), "heuristic.v1");
        assert_eq!(runner.config().trust_threshold, TrustState::Authenticated);
        assert_eq!(runner.trust_mapping().human, TrustState::IntragridMember);
    }

    /// What this catches: a custom recipe (impl IsMemorable) plugs into
    /// the generic runner without modification. Validates the trait-
    /// bound generic shape.
    #[test]
    fn runner_accepts_custom_recipe_via_generic() {
        struct AlwaysAdmit;
        impl IsMemorable for AlwaysAdmit {
            fn id(&self) -> &'static str {
                "test.always-admit"
            }
            fn evaluate(
                &self,
                candidate: &AdmissionCandidate,
                ctx: &AdmissionContext<'_>,
            ) -> Result<AdmissionDecision, AdmissionError> {
                Ok(AdmissionDecision::Admit {
                    engram: super::super::admission::build_engram_from_candidate(candidate, ctx),
                    why: format!("{} — unconditional admit for test", self.id()),
                })
            }
        }

        let runner = InboxAdmissionRunner::new(
            AlwaysAdmit,
            AdmissionConfig::permissive_v1(),
            TrustMapping::default_v1(),
        );
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let mut trace = CognitionTrace::new();
        // Even short content (which the heuristic recipe would drop) admits
        // via the custom recipe — proves the custom recipe is the one being
        // consulted.
        let msg = synthetic_message("short", SenderType::Human);
        let decision = runner
            .admit(&msg, &content, &events, Some(&mut trace))
            .unwrap();
        assert!(matches!(decision, AdmissionDecision::Admit { .. }));
    }

    /// What this catches: the trace seam invariant carries through the
    /// runner — every admit() call appends exactly one SEAM_ADMISSION
    /// to the trace whether the outcome is Admit, Drop, or Err. The
    /// runner is a thin wrapper around `AdmissionGate::admit` and must
    /// preserve its forensic guarantee.
    #[test]
    fn runner_emits_one_seam_per_call_across_outcomes() {
        let runner = InboxAdmissionRunner::default_v1();
        let mut trace = CognitionTrace::new();

        // Admit
        {
            let content = InMemoryContent::default();
            let events = InMemoryEvents::default();
            let _ = runner.admit(
                &synthetic_message(
                    "well-formed human observation worth recalling",
                    SenderType::Human,
                ),
                &content,
                &events,
                Some(&mut trace),
            );
        }
        assert_eq!(trace.seam_count(), 1);

        // Drop (short content)
        {
            let content = InMemoryContent::default();
            let events = InMemoryEvents::default();
            let _ = runner.admit(
                &synthetic_message("short", SenderType::Human),
                &content,
                &events,
                Some(&mut trace),
            );
        }
        assert_eq!(trace.seam_count(), 2);
    }
}
