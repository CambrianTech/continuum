use super::*;

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
            now_ms: EpochMs::now().millis(),
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
        // record-side work is skipped — no `EpochMs::now().millis()`, no `serde_json::json!`
        // Map allocation, no String allocations for seam name/metadata.
        // continuum#1213 follow-up: cuts ~7 allocations per chat turn per
        // persona on the admission hot path. Trace-using callers (TS-IPC
        // `cognition/admit-inbox-message` + the unit tests + the future
        // recorder integration) keep their existing per-seam visibility
        // by passing `Some(&mut trace)`; the in-process inline gate added
        // by #1213 passes `None` because it doesn't propagate the trace
        // anywhere.
        let mut trace = trace;
        let started = EpochMs::now().millis();

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
pub(super) fn verify_envelope(origin: &EngramOrigin) -> Result<(), AdmissionError> {
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
pub(super) fn verify_airc_envelope(r: &AircMessageRef) -> Result<(), AdmissionError> {
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
pub(super) fn wire_event_id(origin: &EngramOrigin) -> Option<String> {
    match origin {
        EngramOrigin::Airc(r) => Some(r.message_id.clone()),
        _ => None,
    }
}

/// Append a `SEAM_ADMISSION` entry to the trace, when one is supplied.
///
/// When `trace` is `None` (the in-process hot-path admission gate added
/// by continuum#1213, which doesn't propagate the trace), this function
/// is a complete no-op — no `EpochMs::now().millis()` syscall, no `serde_json::json!`
/// Map allocation, no String allocations. Cuts ~7 allocations per chat
/// turn per persona on the admission hot path.
pub(super) fn record_seam(
    trace: Option<&mut CognitionTrace>,
    recipe_id: &str,
    started_ms: u64,
    structural: &str,
    decision: Option<&'static str>,
) {
    let Some(trace) = trace else {
        return;
    };
    let duration_ms = EpochMs::now().millis().saturating_sub(started_ms);
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
pub(super) fn decision_label(decision: &AdmissionDecision) -> &'static str {
    match decision {
        AdmissionDecision::Admit { .. } => "Admit",
        AdmissionDecision::Drop { .. } => "Drop",
        AdmissionDecision::Quarantine { .. } => "Quarantine",
    }
}
