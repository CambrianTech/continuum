//! Admission Gate + IsMemorable Recipe (continuum#1121 PR-2)
//!
//! Layers the admission policy machinery over the storage-shape types
//! shipped in PR-1 (`persona::engram`). Splits cleanly into two responsibilities:
//!
//! - **Gate (structural)** — `AdmissionGate::admit()` runs the prereqs that
//!   are independent of any specific persona's policy: envelope structure
//!   verification, trust-tier threshold check, replay protection. Failures
//!   here return typed `AdmissionError` variants, never silent drops.
//! - **Recipe (policy)** — implementations of the `IsMemorable` trait
//!   decide whether a candidate that *passed* the structural prereqs should
//!   be admitted, dropped, or quarantined. Different personas plug in
//!   different recipes (a fuzzy/agent persona may use a permissive
//!   `HeuristicIsMemorable`; a SOC governance persona may use a strict
//!   policy-driven recipe). The trait is the seam.
//!
//! # Design choices
//!
//! - **Stateless gate, injected stores.** `AdmissionGate::admit` is a free
//!   function (no `Self`). State lives in `AdmissionContext`'s lookup
//!   trait objects (`SeenContentLookup`, `SeenEventLookup`). Keeps the
//!   gate trivially testable + composable; same shape as how `recorder`
//!   takes the trace as parameter rather than owning it.
//! - **Caller stores admitted engrams.** The gate returns the
//!   `AdmissionDecision`; the caller is responsible for inserting into
//!   whatever engram store backs the persona. This keeps gate concerns
//!   orthogonal to persistence (PR-3+ adds the ORM persistence path).
//! - **Trace seam emitted unconditionally.** Whether the call returns
//!   `Ok(decision)` or `Err(error)`, a `SEAM_ADMISSION` entry is appended
//!   to the trace. Forensics need to see the gate ran even on error,
//!   matching `recorder.rs`'s always-call-record_turn discipline.
//! - **No panic-catching around recipes.** Recipes return `Result`. If
//!   one panics, that's a bug — let it propagate so the caller sees it.
//!   Same anti-fallback discipline as the rest of the cognition path.
//! - **Envelope verification is structural in v1.** Cryptographic
//!   signature verification against the AIRC pubkey infrastructure is
//!   deferred to a follow-up PR (airc#561 is formalizing the envelope
//!   format). v1 enforces that signed origins have non-empty
//!   signature/content_hash/schema_version fields; the cryptographic
//!   verifier hook lives in [`verify_envelope`] for the real impl to
//!   replace.
//!
//! Pairs with:
//! - [`persona::engram`] — storage-shape types this module operates over.
//! - [`persona::trace`] — `SEAM_ADMISSION` constant + `CognitionTrace`.
//! - `docs/grid/COGNITIVE-IMMUNE-MODEL.md` — defense posture this gate
//!   participates in (apoptosis-cheaper-than-corruption, B-cell anergy,
//!   forensic-not-destructive).
//!
//! # Module layout (continuum#1208)
//!
//! Split out of a 1225-LOC file:
//! - this `mod.rs` — gate machinery (`AdmissionGate::admit`), candidate
//!   + context types, IsMemorable trait, structural-gate tests,
//!   helpers (build_engram_from_candidate, envelope verification,
//!   trace seam emission).
//! - [`recipes`] — concrete `IsMemorable` implementations Continuum
//!   ships (currently `HeuristicIsMemorable`); re-exported here so
//!   external callers see no API change.

pub mod recipes;

pub use recipes::HeuristicIsMemorable;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

// Re-exported pub so submodules (`recipes`) can import via `super::`
// without reaching across to `crate::persona::engram` for every type.
use super::engram::Engram;
pub use super::engram::{
    AdmissionDecision, AdmissionDropReason, AdmissionError, AircMessageRef, EngramKind,
    EngramOrigin, TrustState,
};
use super::trace::{now_ms, CognitionTrace, SEAM_ADMISSION};

//=============================================================================
// CANDIDATE: input to the admission pipeline
//=============================================================================

/// Pre-admission candidate — a unit of cognition that *might* become an
/// `Engram` if both the structural gate and the policy recipe approve.
///
/// Constructed by callers (typically by an AIRC inbox converter or by a
/// chat/tool wrapper) from the source-side data. Does NOT carry an
/// engram id — id assignment happens at admission time inside the
/// `Admit` decision.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/AdmissionCandidate.ts"
)]
pub struct AdmissionCandidate {
    /// The would-be engram content (text in v1; structured later).
    pub content: String,

    /// Engram category to assign on admission (Episodic for an AIRC
    /// observation, Procedural for an admitted skill update, etc.).
    pub kind: EngramKind,

    /// Where this candidate came from. Carries the protocol-compatible
    /// reference fields used for verification + later forensics.
    pub origin: EngramOrigin,

    /// Trust tier of the source AT CANDIDATE TIME. The gate compares
    /// against `AdmissionConfig.trust_threshold` for the structural
    /// trust check; recipes may also re-inspect for finer-grained policy.
    pub trust_state: TrustState,

    /// Free-text recall keys / tags to attach if admitted.
    pub recall_keys: Vec<String>,

    /// SHA-256 of canonical content (caller computes — usually matches
    /// `origin`'s `content_hash`). Used by recipes for content-dedup.
    /// Required because dedup is a hot path and we don't want the recipe
    /// re-hashing on every evaluate.
    pub content_hash: String,
}

//=============================================================================
// CONFIG: gate-level thresholds + policy
//=============================================================================

/// Admission gate configuration — thresholds the structural gate
/// enforces and defaults the recipe pipeline can consult.
///
/// Per-persona; multiple personas in one process each carry their own
/// `AdmissionConfig`. Defaults via `AdmissionConfig::permissive_v1()`
/// (suitable for fuzzy/agent personas just bootstrapping a memory) and
/// `AdmissionConfig::strict_v1()` (suitable for SOC governance roles).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/AdmissionConfig.ts"
)]
pub struct AdmissionConfig {
    /// Minimum trust tier required for any admission. Sources below
    /// this threshold get `AdmissionError::TrustBoundaryRejected` —
    /// the recipe is not even consulted.
    pub trust_threshold: TrustState,

    /// How long a quarantined candidate stays in the quarantine store
    /// before auto-dropping (epoch-ms span). Used by recipes when they
    /// emit `Quarantine` decisions.
    #[ts(type = "number")]
    pub quarantine_ttl_ms: u64,
}

impl AdmissionConfig {
    /// Permissive defaults — appropriate for a fuzzy or agent persona
    /// bootstrapping its memory. Accepts anything from an authenticated
    /// (signature-verified) source upward; quarantines are 24h.
    pub fn permissive_v1() -> Self {
        Self {
            trust_threshold: TrustState::Authenticated,
            quarantine_ttl_ms: 24 * 60 * 60 * 1000,
        }
    }

    /// Strict defaults — appropriate for SOC governance personas.
    /// Requires intragrid membership for any admission; quarantines
    /// are 1h (faster auto-drop because review is faster in SOC ops).
    pub fn strict_v1() -> Self {
        Self {
            trust_threshold: TrustState::IntragridMember,
            quarantine_ttl_ms: 60 * 60 * 1000,
        }
    }
}

//=============================================================================
// CONTEXT: per-call state + injected lookups
//=============================================================================

/// Lookup trait for content-hash dedup. Implementors back this with whatever
/// engram store they use (in-memory map for tests, ORM-backed for prod).
pub trait SeenContentLookup: Send + Sync {
    /// Return the existing engram id if a content hash is already in the
    /// store. None means "novel content; safe to admit on dedup grounds."
    fn find_by_content_hash(&self, hash: &str) -> Option<Uuid>;
}

/// Lookup trait for wire-event replay protection. Distinct from content
/// dedup: this catches the same envelope re-arriving (potentially attacker-
/// replayed), not the same content from a different envelope.
pub trait SeenEventLookup: Send + Sync {
    /// Return the epoch-ms timestamp of the first time this event id was
    /// processed, if any. None means "novel event id; safe on replay grounds."
    fn first_seen_ms(&self, event_id: &str) -> Option<u64>;
}

/// Per-call admission context. Borrowed for the duration of one
/// `AdmissionGate::admit()` call; not stored. The lookup trait objects
/// allow the gate to consult external state without owning it.
pub struct AdmissionContext<'a> {
    /// Gate thresholds + recipe defaults.
    pub config: &'a AdmissionConfig,
    /// Wall-clock (epoch ms) at the start of this admission attempt.
    /// Recipes use this for `admitted_at_ms` + quarantine expiry.
    pub now_ms: u64,
    /// The room/conversation this admission happens in (the contextId), so the
    /// minted engram is keyed to its room within the persona's identity. `None`
    /// for contextless admissions. See IDENTITY-SCOPE-PEER-LIVENESS-MODEL.md A.
    pub context_id: Option<Uuid>,
    /// Content-hash dedup oracle (recipe consults).
    pub seen_content: &'a dyn SeenContentLookup,
    /// Wire-event replay oracle (gate consults).
    pub seen_events: &'a dyn SeenEventLookup,
}

impl<'a> AdmissionContext<'a> {
    /// Convenience constructor; sets `now_ms` from the system clock.
    pub fn new(
        config: &'a AdmissionConfig,
        seen_content: &'a dyn SeenContentLookup,
        seen_events: &'a dyn SeenEventLookup,
    ) -> Self {
        Self {
            config,
            now_ms: now_ms(),
            context_id: None,
            seen_content,
            seen_events,
        }
    }

    /// Scope this admission to a room/conversation (the contextId), so minted
    /// engrams are keyed to their room within the persona's identity.
    pub fn with_context(mut self, context_id: Uuid) -> Self {
        self.context_id = Some(context_id);
        self
    }
}

//=============================================================================
// RECIPE: the IsMemorable trait
//=============================================================================

/// Persona-specific policy: given a candidate that has passed structural
/// prereqs (envelope verification, trust threshold, replay check), decide
/// whether to admit it, drop it, or quarantine it.
///
/// Single sync method (v1 recipes are heuristic / cheap). Async / LLM-backed
/// recipes for PR-3+ will get an `IsMemorableAsync` companion trait;
/// keeping this one sync means it's safe to call from anywhere without
/// runtime considerations.
///
/// Send + Sync because personas live across `tokio::task` boundaries and
/// the recipe is shared.
pub trait IsMemorable: Send + Sync {
    /// Stable identifier for this recipe (e.g., `"heuristic.v1"`,
    /// `"soc-strict.v1"`, `"persona-trained.v3"`). Surfaces in the
    /// `SEAM_ADMISSION` trace metadata + in `AdmissionError::RecipeFailure`
    /// attribution.
    fn id(&self) -> &'static str;

    /// Evaluate the candidate. Returns the policy decision
    /// (`Admit`/`Drop`/`Quarantine`), or `Err` if the recipe itself
    /// could not reach a decision (returns
    /// `AdmissionError::RecipeFailure` typically).
    fn evaluate(
        &self,
        candidate: &AdmissionCandidate,
        ctx: &AdmissionContext<'_>,
    ) -> Result<AdmissionDecision, AdmissionError>;
}

//=============================================================================
// GATE: orchestrator
//=============================================================================

/// Admission gate orchestrator. Stateless (zero-sized struct); namespace
/// holder for the `admit()` associated function. Use as `AdmissionGate::admit(...)`.
pub struct AdmissionGate;

impl AdmissionGate {
    /// Run the full admission pipeline on a candidate.
    ///
    /// Pipeline:
    /// 1. **Envelope structure** — for signed origins, verify the envelope
    ///    has non-empty signature/content_hash/schema_version. Returns
    ///    `EnvelopeVerificationFailed` if structural fields are missing.
    ///    (Cryptographic signature verification is deferred to a follow-up
    ///    PR — see [`verify_envelope`].)
    /// 2. **Trust threshold** — `candidate.trust_state` must be >= the
    ///    configured threshold. Returns `TrustBoundaryRejected` otherwise.
    /// 3. **Replay protection** — for origins that carry a wire event id
    ///    (Airc messages do), check the `seen_events` oracle. Returns
    ///    `ReplayDetected` if the event id was previously processed.
    /// 4. **Recipe evaluation** — call `recipe.evaluate(...)`. Recipe
    ///    decides admit / drop / quarantine; any internal failure
    ///    propagates as `RecipeFailure`.
    ///
    /// In ALL paths (success and error), a `SEAM_ADMISSION` entry is
    /// appended to the trace with the recipe id, structural outcome, and
    /// final decision label. Forensics depend on this — even rejected
    /// admissions must leave a trace entry.
    pub fn admit<R: IsMemorable + ?Sized>(
        candidate: &AdmissionCandidate,
        recipe: &R,
        ctx: &AdmissionContext<'_>,
        trace: Option<&mut CognitionTrace>,
    ) -> Result<AdmissionDecision, AdmissionError> {
        // Wrap the optional trace in a reference cell so the per-step
        // `record_seam` call sites stay uniform (one borrow API regardless
        // of whether the caller wanted a trace). When None, all
        // record-side work is skipped — no `now_ms()`, no `serde_json::json!`
        // Map allocation, no String allocations for seam name/metadata.
        // continuum#1213 follow-up: cuts ~7 allocations per chat turn per
        // persona on the admission hot path. Trace-using callers (TS-IPC
        // `cognition/admit-inbox-message` + the unit tests + the future
        // recorder integration) keep their existing per-seam visibility
        // by passing `Some(&mut trace)`; the in-process inline gate added
        // by #1213 passes `None` because it doesn't propagate the trace
        // anywhere.
        let mut trace = trace;
        let started = now_ms();

        // Step 1: Envelope structure
        if let Err(err) = verify_envelope(&candidate.origin) {
            record_seam(
                trace.as_deref_mut(),
                recipe.id(),
                started,
                "EnvelopeVerificationFailed",
                None,
            );
            return Err(err);
        }

        // Step 2: Trust threshold
        if candidate.trust_state < ctx.config.trust_threshold {
            let err = AdmissionError::TrustBoundaryRejected {
                source_trust: candidate.trust_state,
                threshold: ctx.config.trust_threshold,
            };
            record_seam(
                trace.as_deref_mut(),
                recipe.id(),
                started,
                "TrustBoundaryRejected",
                None,
            );
            return Err(err);
        }

        // Step 3: Replay protection (only for origins with a wire event id)
        if let Some(event_id) = wire_event_id(&candidate.origin) {
            if let Some(prev_ms) = ctx.seen_events.first_seen_ms(&event_id) {
                let err = AdmissionError::ReplayDetected {
                    event_id,
                    previously_seen_at_ms: prev_ms,
                };
                record_seam(
                    trace.as_deref_mut(),
                    recipe.id(),
                    started,
                    "ReplayDetected",
                    None,
                );
                return Err(err);
            }
        }

        // Step 4: Recipe evaluation
        match recipe.evaluate(candidate, ctx) {
            Ok(decision) => {
                let label = decision_label(&decision);
                // Last use of `trace` in this branch — pass by move
                // rather than `as_deref_mut()` (clippy
                // `needless_option_as_deref` would fire on a final
                // reborrow when the next line is just `Ok(...)`).
                record_seam(trace, recipe.id(), started, "accepted", Some(label));
                Ok(decision)
            }
            Err(err) => {
                // Last use of `trace` in this branch — same as above.
                record_seam(trace, recipe.id(), started, "RecipeError", None);
                Err(err)
            }
        }
    }
}

//=============================================================================
// HELPERS
//=============================================================================

/// Synthesize an `Engram` from a candidate + context. Caller (the recipe)
/// uses this when emitting `Admit` so id/timestamp/trust-snapshot wiring
/// stays consistent across recipes. Public so custom recipes can use it.
pub fn build_engram_from_candidate(
    candidate: &AdmissionCandidate,
    ctx: &AdmissionContext<'_>,
) -> Engram {
    Engram {
        id: Uuid::new_v4(),
        context_id: ctx.context_id,
        kind: candidate.kind,
        content: candidate.content.clone(),
        origin: candidate.origin.clone(),
        recall_keys: candidate.recall_keys.clone(),
        admitted_at_ms: ctx.now_ms,
        trust_state_at_admission: candidate.trust_state,
        // admission_trace_id wiring lands in PR-3 alongside the recorder
        // changes that surface a stable trace id from CognitionTrace.
        admission_trace_id: None,
    }
}

/// Verify the envelope's structural fields. v1 = sanity check on the
/// signed-origin shape (signature/content_hash/schema_version non-empty).
/// Cryptographic signature verification is deferred — see module docs.
fn verify_envelope(origin: &EngramOrigin) -> Result<(), AdmissionError> {
    match origin {
        EngramOrigin::Airc(r) => verify_airc_envelope(r),
        // Local-trust origins (chat/tool/self-reflection/agent) don't carry
        // signed envelopes; structural verification is trivially OK. An agent
        // memory is authored by the local agent's own `/remember` — trusted as
        // its own. (When cross-agent shared memories travel signed with the
        // author's peer key, this arm gains a verify like AIRC's — future.)
        EngramOrigin::Chat(_)
        | EngramOrigin::Tool(_)
        | EngramOrigin::SelfReflection { .. }
        | EngramOrigin::Agent(_) => Ok(()),
    }
}

/// AIRC-specific envelope structural check. Empty signature, content_hash,
/// or schema_version means the envelope was constructed without the
/// fields that admission relies on for verifiability.
fn verify_airc_envelope(r: &AircMessageRef) -> Result<(), AdmissionError> {
    if r.signature.is_empty() {
        return Err(AdmissionError::EnvelopeVerificationFailed {
            detail: "AIRC envelope has empty signature".to_string(),
        });
    }
    if r.content_hash.is_empty() {
        return Err(AdmissionError::EnvelopeVerificationFailed {
            detail: "AIRC envelope has empty content_hash".to_string(),
        });
    }
    if r.schema_version.is_empty() {
        return Err(AdmissionError::EnvelopeVerificationFailed {
            detail: "AIRC envelope has empty schema_version".to_string(),
        });
    }
    // v1 admission only understands schema v1 envelopes. Future schema
    // versions should be handled explicitly, not silently coerced.
    if r.schema_version != "v1" {
        return Err(AdmissionError::UnsupportedSchemaVersion {
            schema_version: r.schema_version.clone(),
        });
    }
    Ok(())
}

/// Extract the wire event id used for replay protection. Only Airc
/// origins carry a wire event id (`message_id` in the envelope); other
/// origins return None so the gate skips the replay check.
fn wire_event_id(origin: &EngramOrigin) -> Option<String> {
    match origin {
        EngramOrigin::Airc(r) => Some(r.message_id.clone()),
        _ => None,
    }
}

/// Append a `SEAM_ADMISSION` entry to the trace, when one is supplied.
///
/// When `trace` is `None` (the in-process hot-path admission gate added
/// by continuum#1213, which doesn't propagate the trace), this function
/// is a complete no-op — no `now_ms()` syscall, no `serde_json::json!`
/// Map allocation, no String allocations. Cuts ~7 allocations per chat
/// turn per persona on the admission hot path.
fn record_seam(
    trace: Option<&mut CognitionTrace>,
    recipe_id: &str,
    started_ms: u64,
    structural: &str,
    decision: Option<&'static str>,
) {
    let Some(trace) = trace else {
        return;
    };
    let duration_ms = now_ms().saturating_sub(started_ms);
    let metadata = match decision {
        Some(label) => serde_json::json!({
            "recipe": recipe_id,
            "structural": structural,
            "decision": label,
        }),
        None => serde_json::json!({
            "recipe": recipe_id,
            "structural": structural,
        }),
    };
    trace.record(SEAM_ADMISSION, started_ms, duration_ms, metadata);
}

/// Map an `AdmissionDecision` to a static label for trace metadata.
fn decision_label(decision: &AdmissionDecision) -> &'static str {
    match decision {
        AdmissionDecision::Admit { .. } => "Admit",
        AdmissionDecision::Drop { .. } => "Drop",
        AdmissionDecision::Quarantine { .. } => "Quarantine",
    }
}

//=============================================================================
// TESTS
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

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

    fn airc_ref(message_id: &str, sig: &str, hash: &str, schema: &str) -> AircMessageRef {
        AircMessageRef {
            transport: "airc".to_string(),
            room_id: "cambriantech".to_string(),
            message_id: message_id.to_string(),
            sender_id: "airc-8a5e".to_string(),
            sent_at_ms: FIXED_NOW_MS,
            received_at_ms: FIXED_NOW_MS,
            content_hash: hash.to_string(),
            signature: sig.to_string(),
            proof_refs: vec![],
            schema_version: schema.to_string(),
            client_name: Some("airc-bash".to_string()),
        }
    }

    fn candidate(content: &str, trust: TrustState, origin: EngramOrigin) -> AdmissionCandidate {
        AdmissionCandidate {
            content: content.to_string(),
            kind: EngramKind::Episodic,
            origin,
            trust_state: trust,
            recall_keys: vec!["test".to_string()],
            content_hash: format!("sha256:fake-{}", content.len()),
        }
    }

    fn airc_candidate(content: &str, trust: TrustState, message_id: &str) -> AdmissionCandidate {
        candidate(
            content,
            trust,
            EngramOrigin::Airc(airc_ref(message_id, "sig", "hash", "v1")),
        )
    }

    fn permissive_ctx<'a>(
        config: &'a AdmissionConfig,
        content: &'a InMemoryContent,
        events: &'a InMemoryEvents,
    ) -> AdmissionContext<'a> {
        AdmissionContext {
            context_id: None,
            config,
            now_ms: FIXED_NOW_MS,
            seen_content: content,
            seen_events: events,
        }
    }

    // ── context (room) carried onto the engram ──────────────────────────

    // what this catches: an admitted engram carries its ROOM (contextId) from
    // the admission context, so a persona's memory is keyed to the conversation
    // it happened in (within the persona's identity), not contextless. Regression
    // here = engrams lose their room and per-room recall can't scope. See
    // docs/architecture/IDENTITY-SCOPE-PEER-LIVENESS-MODEL.md Part A.
    #[test]
    fn admitted_engram_carries_its_room_context() {
        let cfg = AdmissionConfig::permissive_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let room = Uuid::new_v4();
        let ctx = permissive_ctx(&cfg, &content, &events).with_context(room);
        let cand = candidate(
            "remember this",
            TrustState::ApprovedPeer,
            EngramOrigin::Airc(airc_ref("msg-ctx", "sig", "hash", "v1")),
        );
        let engram = build_engram_from_candidate(&cand, &ctx);
        assert_eq!(
            engram.context_id,
            Some(room),
            "engram must carry the room it was admitted in"
        );
    }

    // ── envelope verification ───────────────────────────────────────────

    /// What this catches: empty signature on an Airc envelope is a
    /// structural failure, not a recipe-policy decision. Returns
    /// `EnvelopeVerificationFailed`, not `Drop` — the gate must fail
    /// loud rather than silently rejecting.
    #[test]
    fn empty_signature_returns_envelope_verification_failed() {
        let cfg = AdmissionConfig::permissive_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let ctx = permissive_ctx(&cfg, &content, &events);
        let mut trace = CognitionTrace::new();

        let cand = candidate(
            "interesting",
            TrustState::ApprovedPeer,
            EngramOrigin::Airc(airc_ref("msg-1", "", "hash", "v1")),
        );

        let result = AdmissionGate::admit(
            &cand,
            &HeuristicIsMemorable::default_v1(),
            &ctx,
            Some(&mut trace),
        );
        match result {
            Err(AdmissionError::EnvelopeVerificationFailed { detail }) => {
                assert!(detail.contains("signature"), "detail: {detail}");
            }
            other => panic!("expected EnvelopeVerificationFailed, got {other:?}"),
        }
        // Seam recorded even on error — forensics need it.
        assert_eq!(trace.seam_count(), 1);
        assert_eq!(trace.last_seam_name(), Some(SEAM_ADMISSION));
    }

    /// What this catches: empty content_hash on an Airc envelope is a
    /// structural failure (the gate needs the hash for tamper detection
    /// + dedup). Symmetric with the empty-signature test; same failure
    /// class returned via `EnvelopeVerificationFailed`. Asymmetric
    /// coverage between empty-signature/empty-content-hash/empty-schema
    /// would let one of the three regress silently.
    #[test]
    fn empty_content_hash_returns_envelope_verification_failed() {
        let cfg = AdmissionConfig::permissive_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let ctx = permissive_ctx(&cfg, &content, &events);
        let mut trace = CognitionTrace::new();

        let cand = candidate(
            "perfectly novel content of sufficient length",
            TrustState::ApprovedPeer,
            EngramOrigin::Airc(airc_ref("msg-x", "sig", "", "v1")),
        );

        match AdmissionGate::admit(
            &cand,
            &HeuristicIsMemorable::default_v1(),
            &ctx,
            Some(&mut trace),
        ) {
            Err(AdmissionError::EnvelopeVerificationFailed { detail }) => {
                assert!(detail.contains("content_hash"), "detail: {detail}");
            }
            other => panic!("expected EnvelopeVerificationFailed, got {other:?}"),
        }
        assert_eq!(trace.seam_count(), 1);
    }

    /// What this catches: empty schema_version is structurally invalid
    /// (admission can't reason about a schema with no name). Distinct
    /// from `UnsupportedSchemaVersion` which fires for unknown values
    /// — empty is its own class returned via `EnvelopeVerificationFailed`.
    /// Symmetric coverage with empty-signature/empty-content-hash.
    #[test]
    fn empty_schema_version_returns_envelope_verification_failed() {
        let cfg = AdmissionConfig::permissive_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let ctx = permissive_ctx(&cfg, &content, &events);
        let mut trace = CognitionTrace::new();

        let cand = candidate(
            "perfectly novel content of sufficient length",
            TrustState::ApprovedPeer,
            EngramOrigin::Airc(airc_ref("msg-x", "sig", "hash", "")),
        );

        match AdmissionGate::admit(
            &cand,
            &HeuristicIsMemorable::default_v1(),
            &ctx,
            Some(&mut trace),
        ) {
            Err(AdmissionError::EnvelopeVerificationFailed { detail }) => {
                assert!(detail.contains("schema_version"), "detail: {detail}");
            }
            other => panic!("expected EnvelopeVerificationFailed, got {other:?}"),
        }
        assert_eq!(trace.seam_count(), 1);
    }

    /// What this catches: unsupported schema_version returns
    /// `UnsupportedSchemaVersion`, not silent acceptance. Forward-
    /// compatibility hinge: if a sender claims schema v2 we want to fail
    /// loudly until the v2 admission code is shipped.
    #[test]
    fn unknown_schema_version_returns_unsupported_schema_version() {
        let cfg = AdmissionConfig::permissive_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let ctx = permissive_ctx(&cfg, &content, &events);
        let mut trace = CognitionTrace::new();

        let cand = candidate(
            "novel content of sufficient length to be memorable",
            TrustState::ApprovedPeer,
            EngramOrigin::Airc(airc_ref("msg-x", "sig", "hash", "v2")),
        );

        let result = AdmissionGate::admit(
            &cand,
            &HeuristicIsMemorable::default_v1(),
            &ctx,
            Some(&mut trace),
        );
        match result {
            Err(AdmissionError::UnsupportedSchemaVersion { schema_version }) => {
                assert_eq!(schema_version, "v2");
            }
            other => panic!("expected UnsupportedSchemaVersion, got {other:?}"),
        }
    }

    /// What this catches: local-trust origins (chat / tool / self-reflection)
    /// don't carry signed envelopes, so the structural envelope check
    /// must pass-through rather than treating "no signature" as failure.
    /// Otherwise admission of any internal-cognition engram would be
    /// impossible.
    #[test]
    fn self_reflection_origin_passes_envelope_structure() {
        let cfg = AdmissionConfig::permissive_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let ctx = AdmissionContext {
            context_id: None,
            config: &cfg,
            now_ms: FIXED_NOW_MS,
            seen_content: &content,
            seen_events: &events,
        };
        let mut trace = CognitionTrace::new();

        let parent = Uuid::new_v4();
        let cand = candidate(
            "reflection on a prior engram which is sufficiently long",
            TrustState::SelfTrust,
            EngramOrigin::SelfReflection {
                parent_engram_id: parent,
            },
        );

        let result = AdmissionGate::admit(
            &cand,
            &HeuristicIsMemorable::default_v1(),
            &ctx,
            Some(&mut trace),
        )
        .expect("self-reflection should pass structural checks");
        match result {
            AdmissionDecision::Admit { engram, .. } => {
                assert_eq!(engram.trust_state_at_admission, TrustState::SelfTrust);
                if let EngramOrigin::SelfReflection { parent_engram_id } = engram.origin {
                    assert_eq!(parent_engram_id, parent);
                } else {
                    panic!("origin should round-trip as SelfReflection");
                }
            }
            other => panic!("expected Admit, got {other:?}"),
        }
    }

    // ── trust threshold ─────────────────────────────────────────────────

    /// What this catches: trust below the configured threshold returns
    /// `TrustBoundaryRejected` BEFORE the recipe is consulted. A strict
    /// gate must not let unauthenticated traffic reach the recipe at
    /// all, even if the recipe would have rejected anyway — defense in
    /// depth.
    #[test]
    fn untrusted_source_rejected_at_trust_boundary_before_recipe() {
        let cfg = AdmissionConfig::strict_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let ctx = permissive_ctx(&cfg, &content, &events);
        let mut trace = CognitionTrace::new();

        // ApprovedPeer is below IntragridMember (strict_v1's threshold).
        let cand = airc_candidate(
            "totally legitimate content here",
            TrustState::ApprovedPeer,
            "msg-2",
        );

        let result = AdmissionGate::admit(
            &cand,
            &HeuristicIsMemorable::default_v1(),
            &ctx,
            Some(&mut trace),
        );
        match result {
            Err(AdmissionError::TrustBoundaryRejected {
                source_trust,
                threshold,
            }) => {
                assert_eq!(source_trust, TrustState::ApprovedPeer);
                assert_eq!(threshold, TrustState::IntragridMember);
            }
            other => panic!("expected TrustBoundaryRejected, got {other:?}"),
        }
    }

    /// What this catches: equal-tier source passes the threshold (>=, not >).
    /// Off-by-one on the comparison would silently reject valid traffic.
    #[test]
    fn trust_threshold_uses_inclusive_comparison() {
        let cfg = AdmissionConfig::strict_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let ctx = permissive_ctx(&cfg, &content, &events);
        let mut trace = CognitionTrace::new();

        // IntragridMember == threshold; must pass.
        let cand = airc_candidate(
            "intragrid member message of sufficient length here",
            TrustState::IntragridMember,
            "msg-3",
        );

        let result = AdmissionGate::admit(
            &cand,
            &HeuristicIsMemorable::default_v1(),
            &ctx,
            Some(&mut trace),
        )
        .expect("equal-tier source should pass threshold");
        assert!(matches!(result, AdmissionDecision::Admit { .. }));
    }

    // ── replay protection ───────────────────────────────────────────────

    /// What this catches: an event_id present in the seen-events oracle
    /// returns `ReplayDetected`. The gate must consult the oracle and
    /// reject before the recipe runs — replay protection is structural,
    /// not policy.
    #[test]
    fn replayed_event_returns_replay_detected() {
        let cfg = AdmissionConfig::permissive_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        events
            .0
            .lock()
            .unwrap()
            .insert("msg-replay".to_string(), 1_000_000);
        let ctx = permissive_ctx(&cfg, &content, &events);
        let mut trace = CognitionTrace::new();

        let cand = airc_candidate(
            "perfectly novel content here",
            TrustState::ApprovedPeer,
            "msg-replay",
        );

        let result = AdmissionGate::admit(
            &cand,
            &HeuristicIsMemorable::default_v1(),
            &ctx,
            Some(&mut trace),
        );
        match result {
            Err(AdmissionError::ReplayDetected {
                event_id,
                previously_seen_at_ms,
            }) => {
                assert_eq!(event_id, "msg-replay");
                assert_eq!(previously_seen_at_ms, 1_000_000);
            }
            other => panic!("expected ReplayDetected, got {other:?}"),
        }
    }

    /// What this catches: non-Airc origins skip replay (no wire event id
    /// to check). A SelfReflection candidate must not get
    /// `ReplayDetected` even if an unrelated event id is in the oracle.
    #[test]
    fn non_airc_origin_skips_replay_check() {
        let cfg = AdmissionConfig::permissive_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        events
            .0
            .lock()
            .unwrap()
            .insert("some-airc-id".to_string(), 1_000_000);
        let ctx = permissive_ctx(&cfg, &content, &events);
        let mut trace = CognitionTrace::new();

        let cand = candidate(
            "reflective thought of sufficient length to admit",
            TrustState::SelfTrust,
            EngramOrigin::SelfReflection {
                parent_engram_id: Uuid::new_v4(),
            },
        );

        AdmissionGate::admit(
            &cand,
            &HeuristicIsMemorable::default_v1(),
            &ctx,
            Some(&mut trace),
        )
        .expect("non-airc origin should bypass replay check");
    }

    // (HeuristicIsMemorable policy tests moved to admission/recipes.rs
    // per continuum#1208 — keep mod.rs focused on gate-level tests.)

    // ── trace seam emission ─────────────────────────────────────────────

    /// What this catches: every admission attempt — success OR error —
    /// emits exactly one `SEAM_ADMISSION` entry. Forensics and replay
    /// tooling depend on this invariant; missing seams break the
    /// "every gate decision is auditable" promise.
    #[test]
    fn every_admission_path_emits_exactly_one_seam() {
        let cfg = AdmissionConfig::permissive_v1();
        let mut trace = CognitionTrace::new();

        // Path 1: structural failure
        {
            let content = InMemoryContent::default();
            let events = InMemoryEvents::default();
            let ctx = permissive_ctx(&cfg, &content, &events);
            let cand = candidate(
                "x",
                TrustState::ApprovedPeer,
                EngramOrigin::Airc(airc_ref("e1", "", "h", "v1")),
            );
            let _ = AdmissionGate::admit(
                &cand,
                &HeuristicIsMemorable::default_v1(),
                &ctx,
                Some(&mut trace),
            );
        }
        assert_eq!(trace.seam_count(), 1);

        // Path 2: successful admit
        {
            let content = InMemoryContent::default();
            let events = InMemoryEvents::default();
            let ctx = permissive_ctx(&cfg, &content, &events);
            let cand = airc_candidate(
                "well-formed candidate of sufficient length to admit",
                TrustState::ApprovedPeer,
                "e2",
            );
            let _ = AdmissionGate::admit(
                &cand,
                &HeuristicIsMemorable::default_v1(),
                &ctx,
                Some(&mut trace),
            );
        }
        assert_eq!(trace.seam_count(), 2);

        // Path 3: drop (length)
        {
            let content = InMemoryContent::default();
            let events = InMemoryEvents::default();
            let ctx = permissive_ctx(&cfg, &content, &events);
            let cand = airc_candidate("short", TrustState::ApprovedPeer, "e3");
            let _ = AdmissionGate::admit(
                &cand,
                &HeuristicIsMemorable::default_v1(),
                &ctx,
                Some(&mut trace),
            );
        }
        assert_eq!(trace.seam_count(), 3);

        // Each seam should be SEAM_ADMISSION.
        for seam in &trace.seams {
            assert_eq!(seam.name, SEAM_ADMISSION);
        }
    }

    /// What this catches: trace metadata on a successful admit includes
    /// the recipe id + decision label. Operators reading the seam log
    /// need to see WHICH recipe ran and WHAT it decided, without parsing
    /// neighbouring data.
    #[test]
    fn admit_seam_metadata_carries_recipe_id_and_decision() {
        let cfg = AdmissionConfig::permissive_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let ctx = permissive_ctx(&cfg, &content, &events);
        let mut trace = CognitionTrace::new();

        let cand = airc_candidate(
            "this is a meaningful design observation worth recalling",
            TrustState::ApprovedPeer,
            "msg-trace-1",
        );

        AdmissionGate::admit(
            &cand,
            &HeuristicIsMemorable::default_v1(),
            &ctx,
            Some(&mut trace),
        )
        .unwrap();
        let seam = &trace.seams[0];
        assert_eq!(seam.metadata["recipe"], serde_json::json!("heuristic.v1"));
        assert_eq!(seam.metadata["structural"], serde_json::json!("accepted"));
        assert_eq!(seam.metadata["decision"], serde_json::json!("Admit"));
    }

    // ── recipe error path ───────────────────────────────────────────────

    /// What this catches: a recipe that returns `Err(AdmissionError::RecipeFailure)`
    /// has its error propagated unchanged. Critical that the gate doesn't
    /// silently coerce recipe errors into Drop (would hide bugs in the
    /// recipe and turn loud failures into quiet drops).
    #[test]
    fn recipe_failure_propagates_as_recipe_failure() {
        struct FailingRecipe;
        impl IsMemorable for FailingRecipe {
            fn id(&self) -> &'static str {
                "test.failing"
            }
            fn evaluate(
                &self,
                _candidate: &AdmissionCandidate,
                _ctx: &AdmissionContext<'_>,
            ) -> Result<AdmissionDecision, AdmissionError> {
                Err(AdmissionError::RecipeFailure {
                    recipe_id: "test.failing".to_string(),
                    detail: "intentional test failure".to_string(),
                })
            }
        }

        let cfg = AdmissionConfig::permissive_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let ctx = permissive_ctx(&cfg, &content, &events);
        let mut trace = CognitionTrace::new();

        let cand = airc_candidate(
            "passes structural checks, recipe will explode",
            TrustState::ApprovedPeer,
            "msg-fail",
        );

        let result = AdmissionGate::admit(&cand, &FailingRecipe, &ctx, Some(&mut trace));
        match result {
            Err(AdmissionError::RecipeFailure { recipe_id, detail }) => {
                assert_eq!(recipe_id, "test.failing");
                assert!(detail.contains("intentional"), "detail: {detail}");
            }
            other => panic!("expected RecipeFailure, got {other:?}"),
        }
    }

    /// What this catches: a recipe that emits `Quarantine` has the
    /// decision propagated unchanged (the gate doesn't override the
    /// recipe's quarantine choice). PR-3+ recipes will use this for
    /// borderline-similarity content.
    #[test]
    fn recipe_quarantine_decision_propagates() {
        struct QuarantineRecipe;
        impl IsMemorable for QuarantineRecipe {
            fn id(&self) -> &'static str {
                "test.quarantine"
            }
            fn evaluate(
                &self,
                candidate: &AdmissionCandidate,
                ctx: &AdmissionContext<'_>,
            ) -> Result<AdmissionDecision, AdmissionError> {
                Ok(AdmissionDecision::Quarantine {
                    engram: build_engram_from_candidate(candidate, ctx),
                    reason: "borderline similarity to existing engram".to_string(),
                    expiry_ms: ctx.now_ms + ctx.config.quarantine_ttl_ms,
                })
            }
        }

        let cfg = AdmissionConfig::permissive_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let ctx = permissive_ctx(&cfg, &content, &events);
        let mut trace = CognitionTrace::new();

        let cand = airc_candidate(
            "borderline content that the recipe wants to quarantine",
            TrustState::ApprovedPeer,
            "msg-quar",
        );

        match AdmissionGate::admit(&cand, &QuarantineRecipe, &ctx, Some(&mut trace)).unwrap() {
            AdmissionDecision::Quarantine {
                engram, expiry_ms, ..
            } => {
                assert_eq!(engram.trust_state_at_admission, TrustState::ApprovedPeer);
                assert_eq!(expiry_ms, FIXED_NOW_MS + cfg.quarantine_ttl_ms);
            }
            other => panic!("expected Quarantine, got {other:?}"),
        }
        // Trace metadata should carry the Quarantine decision label.
        assert_eq!(
            trace.seams[0].metadata["decision"],
            serde_json::json!("Quarantine")
        );
    }

    // ── AdmissionConfig presets ─────────────────────────────────────────

    /// What this catches: the two preset configs have the trust ordering
    /// the docs claim (permissive accepts Authenticated; strict requires
    /// IntragridMember). A regression in the preset values would silently
    /// change the security posture of every persona using the defaults.
    #[test]
    fn admission_config_presets_have_documented_thresholds() {
        let permissive = AdmissionConfig::permissive_v1();
        let strict = AdmissionConfig::strict_v1();
        assert_eq!(permissive.trust_threshold, TrustState::Authenticated);
        assert_eq!(strict.trust_threshold, TrustState::IntragridMember);
        assert!(strict.trust_threshold > permissive.trust_threshold);
        // strict is shorter quarantine (faster auto-drop in SOC ops)
        assert!(strict.quarantine_ttl_ms < permissive.quarantine_ttl_ms);
    }

    // ── ts-rs binding tests ─────────────────────────────────────────────

    #[test]
    fn export_bindings_admission_candidate() {
        let cfg = ts_rs::Config::default();
        AdmissionCandidate::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_admission_config() {
        let cfg = ts_rs::Config::default();
        AdmissionConfig::export_all(&cfg).unwrap();
    }
}
