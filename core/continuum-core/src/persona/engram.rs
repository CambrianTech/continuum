//! Persona Engram + Admission Membrane Types
//!
//! Pure value types for the AIRC-inbox → cognition-admission → engram-storage
//! membrane (continuum#1121, queue card #1125).
//!
//! This module ships the storage-shape types ONLY — no Recipe impl, no
//! admission-gate logic, no PersonaInbox wiring, no ORM persistence path.
//! Subsequent PRs layer those over these types.
//!
//! Design principles (per AIRC discussion 2026-05-13):
//!
//! - **Cognition decides storage.** Raw AIRC messages never become engrams
//!   automatically; the persona's admission Recipe (PR-2+) decides what
//!   becomes memorable, with typed failure modes that keep the decision
//!   itself auditable.
//! - **Provenance is load-bearing.** Every admitted Engram carries
//!   structured origin (source kind + protocol-compatible reference fields)
//!   so later introspection can answer "where did this belief come from?"
//!   This is the forensic surface against poisoning attacks (see
//!   `docs/grid/COGNITIVE-IMMUNE-MODEL.md`).
//! - **Protocol over client.** AIRC origin is a protocol-compatible reference
//!   (`AircMessageRef`), not a binding to any specific client implementation.
//!   `transport = "airc"` names the protocol; `client_name` is informational
//!   only. Admission must judge valid envelope+signature data, not which
//!   binary emitted it (per Joel 2026-05-13 + Codex relay).
//! - **TrustState models policy, not implementation.** Trust variants
//!   describe the source's policy/trust tier — not which client produced
//!   the data.
//! - **Typed failure modes only.** `AdmissionError` enumerates the explicit
//!   reasons a candidate may not be engrammed; no silent drops, no
//!   un-catchable refusals. Same shape as `NoLocalModelLoadable` (#1089)
//!   and `NoMultimodalBase` (#1074).
//!
//! Pairs with:
//! - [`docs/grid/FORGE-ALLOY-PROOF-CONTRACTS.md`] — artifact-verification
//!   trust layer that this module is the runtime-cognition complement of.
//! - [`docs/grid/COGNITIVE-IMMUNE-MODEL.md`] — defense posture this
//!   substrate enables (detection, forensics, quarantine, recovery).
//!
//! Convention notes (matching existing `persona/*.rs` modules):
//! - `Uuid` fields use `#[ts(type = "string")]` for the TS export.
//! - Timestamps are `u64` epoch milliseconds with `#[ts(type = "number")]`,
//!   matching `PersonaInboxFrame.oldest_timestamp` etc. Not
//!   `chrono::DateTime<Utc>`, because the workspace's chrono crate doesn't
//!   enable the `serde` feature and the existing persona modules use the
//!   u64-epoch shape consistently.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::orm::Entity;

//=============================================================================
// CORE: ENGRAM
//=============================================================================

/// A single memorable cognition unit, durably storable + recall-addressable.
///
/// Engrams are the unit of long-term cognitive memory. They survive persona
/// session boundaries, get indexed for recall, and carry full provenance so
/// any persona (including future-self) can audit "where did this belief
/// come from + why was it admitted." The biological metaphor (memory trace)
/// is structural, not decorative — engrams accumulate, decay, get yanked,
/// and contribute to recall via the same mechanisms a biological memory
/// store does.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Entity)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/persona/Engram.ts")]
#[entity(collection = "engrams")]
pub struct Engram {
    /// Stable engram id. Used for recall keys, deduplication, and as the
    /// referent target for `EngramOrigin::SelfReflection { parent_engram_id }`.
    /// Marked `primary_key` so the derive pulls in BaseEntity columns
    /// (id + createdAt + updatedAt + version) and skips emitting this
    /// field separately — `id` is the BaseEntity column.
    #[ts(type = "string")]
    #[entity(primary_key)]
    pub id: Uuid,

    /// The room/conversation this memory belongs to — the contextId, the
    /// third ID tier (see docs/architecture/IDENTITY-SCOPE-PEER-LIVENESS-MODEL.md
    /// Part A). A persona's engram store IS its identity's memory; within that one
    /// identity, memory is sub-keyed by context (room) so recall can scope to a
    /// conversation. Indexed: per-room recall is a common filter. `None` for
    /// engrams with no room (self-reflection, contextless admissions). NEVER a
    /// session id — context is durable, session is ephemeral.
    #[ts(optional, type = "string")]
    #[entity(indexed)]
    pub context_id: Option<Uuid>,

    /// Engram category — episodic vs semantic vs procedural vs meta.
    /// Indexed: recall by kind ("show me all Episodic engrams") is a
    /// common filter.
    #[entity(indexed)]
    pub kind: EngramKind,

    /// The memorable content itself. v1 is plain text; later PRs may
    /// structure this further (e.g., `content: EngramContent` enum with
    /// variants for text / embedding / structured fact / etc.).
    pub content: String,

    /// What kind of source this engram came from + the protocol-compatible
    /// reference fields needed to verify or re-locate it.
    /// `EngramOrigin` is a tagged-union enum; persisted as a JSON column
    /// so the variant rides intact.
    #[entity(json)]
    pub origin: EngramOrigin,

    /// Free-text recall keys / tags. v1 is unstructured strings; recall
    /// (later PR) may add embeddings or structured indexes alongside.
    #[entity(json)]
    pub recall_keys: Vec<String>,

    /// When this engram was admitted (epoch milliseconds UTC). Indexed:
    /// admission-order is the primary sort for recall_recent + the
    /// recency tiebreak for Algorithm 4 scoring.
    #[ts(type = "number")]
    #[entity(indexed)]
    pub admitted_at_ms: u64,

    /// The trust tier of the source AT ADMISSION TIME. Snapshot, not live —
    /// later trust changes don't retroactively rewrite this engram's
    /// recorded trust. A trust degradation across the polity creates new
    /// signal in introspection ("engrams admitted from peer X while their
    /// trust was high but is now low — re-evaluate"). Indexed: forensic
    /// queries filter by trust tier.
    #[entity(indexed)]
    pub trust_state_at_admission: TrustState,

    /// Optional pointer to the `CognitionTrace` SEAM record that explains
    /// WHY this engram was admitted. v1 carries an optional trace id
    /// string (the trace itself lives in the recorder); PR-2's IsMemorable
    /// Recipe will populate this. None = trace not recorded (acceptable
    /// for v1 manual admissions; should be Some for Recipe-driven
    /// admissions in PR-2+).
    pub admission_trace_id: Option<String>,
}

// ORM schema is now derived by `#[derive(Entity)]` on the struct
// above. The 100-line hand-written `impl OrmEntity for Engram` block
// previously lived here — deleted in slice #168 once the derive
// macro + relational features were proven in #166 / #167. Schema
// IS the struct; drift is structurally impossible.

//=============================================================================
// CATEGORY: ENGRAM KIND
//=============================================================================

/// Engram categories (biological-memory analogs).
///
/// `Episodic` — something happened (an interaction, an event, an observation).
/// `Semantic` — a fact learned (a piece of knowledge separable from when/how
/// it was learned).
/// `Procedural` — a way to do things (a skill, a pattern, a heuristic).
/// `SelfReflection` — meta-cognition: an engram ABOUT engrams or about the
/// persona's own past decisions. The recursion that makes self-introspection
/// possible (see `COGNITIVE-IMMUNE-MODEL.md` §3.9).
///
/// Single-Engram-with-discriminator (vs separate-types-per-kind) is
/// intentional: composes better, lets recall + admission share machinery
/// across kinds, and the discriminator is cheap. Per the airc design
/// discussion 2026-05-13.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/EngramKind.ts"
)]
pub enum EngramKind {
    Episodic,
    Semantic,
    Procedural,
    SelfReflection,
}

//=============================================================================
// PROVENANCE: ENGRAM ORIGIN
//=============================================================================

/// Where this engram came from.
///
/// Variant-typed (vs generic `Provenance` interface) so each origin kind
/// has its identity primitive present in the type. A consumer can
/// pattern-match and KNOW that `EngramOrigin::Airc(reference)` carries
/// the protocol-compatible reference fields — the type system enforces
/// structure rather than relying on documentation.
///
/// `SelfReflection` is the only origin without an external reference;
/// it carries the parent engram id whose introspection produced this
/// meta-engram.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/EngramOrigin.ts"
)]
#[serde(tag = "kind", content = "ref")]
pub enum EngramOrigin {
    /// Came from a protocol-compatible AIRC envelope. Reference fields
    /// are sufficient to verify the envelope's signature and re-locate
    /// the original on the AIRC substrate. NOT a binding to any specific
    /// client implementation — see `AircMessageRef` doc.
    Airc(AircMessageRef),

    /// Came from a Continuum chat message (ChatMessageEntity).
    Chat(ChatMessageRef),

    /// Came from a tool invocation (the persona ran a tool and the
    /// result was admitted as an engram).
    Tool(ToolInvocationRef),

    /// Meta: this engram was produced by introspection over an existing
    /// engram. `parent_engram_id` is the engram the reflection was about.
    SelfReflection {
        #[ts(type = "string")]
        parent_engram_id: Uuid,
    },

    /// Authored by an external agent (Claude Code, Codex, a peer node's model)
    /// via the agent-memory bridge — the fix for agents re-forgetting because
    /// their memory lived in flat `.md` files reloaded wholesale each session.
    /// An agent is just a persona whose engrams now live on THIS substrate;
    /// `AgentRef` carries the load-bearing provenance (which agent learned it).
    /// See `docs/cognition/AGENT-MEMORY-BRIDGE.md`.
    Agent(AgentRef),
}

/// Protocol-compatible reference to an AIRC-substrate event/message.
///
/// Per Joel 2026-05-13 (relayed by Codex): Continuum accepts AIRC data
/// by **proof/contract**, not by client identity. Any producer that
/// emits a valid envelope with these fields populated is acceptable;
/// the official `airc` CLI is not privileged. `transport = "airc"` names
/// the PROTOCOL; `client_name` is informational only (e.g., "airc-bash",
/// "airc-py", "third-party-emitter"). Admission Recipes in PR-2+ judge
/// the envelope's signature + provenance + trust metadata, not which
/// binary produced the bytes.
///
/// Suggested field shape comes from Codex 2026-05-13 broadcast — see
/// AIRC log for full design discussion.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/AircMessageRef.ts"
)]
pub struct AircMessageRef {
    /// Protocol identifier. Always `"airc"` for this variant; field exists
    /// to support future cross-protocol references where the variant might
    /// represent multiple wire protocols.
    pub transport: String,

    /// AIRC room (channel) the message was posted to.
    pub room_id: String,

    /// Stable AIRC message/event id within the room.
    pub message_id: String,

    /// Sender pubkey or peer identity (the AIRC-whois identity, NOT a gh
    /// login — per the gh-account-not-equal-identity rule from
    /// `.airc/SAFETY.md` §Identity).
    pub sender_id: String,

    /// When the sender claims they sent it (epoch ms UTC, signed by sender).
    #[ts(type = "number")]
    pub sent_at_ms: u64,

    /// When the receiving persona observed it (epoch ms UTC, local clock).
    #[ts(type = "number")]
    pub received_at_ms: u64,

    /// SHA-256 of the canonical content. Used for tamper detection +
    /// cross-grid forensic re-verification.
    pub content_hash: String,

    /// Detached signature over the canonical envelope. Verifiable against
    /// `sender_id`'s public key. Required for the engram to admit via
    /// non-trivial trust modes; PR-2+ Recipes will enforce.
    pub signature: String,

    /// Pointers to additional proof material (e.g., forge-alloy contract
    /// settlement signatures, room-rotation event signatures, attestation
    /// chain references). Empty for plain messages.
    pub proof_refs: Vec<String>,

    /// Schema version of the envelope this reference describes. v1 starts
    /// at `"v1"`. Forward-compatibility hinge.
    pub schema_version: String,

    /// Informational client identity (e.g., "airc-bash", "airc-py",
    /// "third-party-emitter"). Optional, NOT load-bearing for trust
    /// decisions. Present so the polity can observe client-population
    /// telemetry without admission ever depending on it.
    pub client_name: Option<String>,
}

/// Protocol-compatible reference to a Continuum chat message.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/ChatMessageRef.ts"
)]
pub struct ChatMessageRef {
    /// Continuum chat message id.
    #[ts(type = "string")]
    pub message_id: Uuid,
    /// Continuum room id.
    #[ts(type = "string")]
    pub room_id: Uuid,
    /// Sender (Continuum user id).
    #[ts(type = "string")]
    pub sender_id: Uuid,
    /// When the message was posted (epoch ms UTC).
    #[ts(type = "number")]
    pub posted_at_ms: u64,
    /// SHA-256 of canonical content for tamper detection.
    pub content_hash: String,
}

/// Reference to a tool invocation that produced this engram.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/ToolInvocationRef.ts"
)]
pub struct ToolInvocationRef {
    /// Stable invocation id.
    #[ts(type = "string")]
    pub invocation_id: Uuid,
    /// Tool name (e.g., "search", "calculator").
    pub tool_name: String,
    /// When the tool was invoked (epoch ms UTC).
    #[ts(type = "number")]
    pub invoked_at_ms: u64,
    /// SHA-256 of canonical input parameters.
    pub input_hash: String,
    /// SHA-256 of canonical output. Reproducibility check anchor.
    pub output_hash: String,
}

/// Provenance reference for an engram authored by an external agent (the
/// agent-memory bridge). Mirrors the other origin refs: a typed reference
/// whose load-bearing field is WHO authored the lesson, because in a shared
/// multi-agent memory (BigMama + M5 + Codex all writing engrams) provenance-
/// by-author is what lets recall weigh, trust, and attribute a lesson.
///
/// Minimal + honest by design — grows fields later without breaking the
/// variant, same discipline as `Provenance`. See
/// `docs/cognition/AGENT-MEMORY-BRIDGE.md`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/persona/AgentRef.ts")]
pub struct AgentRef {
    /// The authoring agent's airc peer id. REQUIRED — and the SAME seed the
    /// agent's `persona_id` is derived from (`PeerId::as_uuid`, the canonical
    /// derivation the live spawner uses), so origin and identity tie together
    /// and an agent's engrams can never collide with a real persona's in the
    /// shared corpus (distinct airc peers ⇒ distinct ids). NOT a locally-
    /// invented hash — reuse airc's canonical id, never mint a rogue one.
    #[ts(type = "string")]
    pub agent_peer_id: Uuid,

    /// The session/conversation that produced the lesson. Traceability only;
    /// `None` for a migrated `.md` engram (there was no live session).
    #[ts(optional)]
    pub session: Option<String>,

    /// Free-form provenance breadcrumb: the source `.md` path for a migrated
    /// engram, a tool name, or `None`. Never load-bearing.
    #[ts(optional)]
    pub origin_hint: Option<String>,
}

//=============================================================================
// ADMISSION OUTCOME
//=============================================================================

/// Outcome of running the admission gate over a candidate engram.
///
/// Three terminal states:
/// - `Admit` — engram becomes part of the store. Includes the why-string
///   for forensic auditability.
/// - `Drop` — candidate is rejected; no engram created. Reason is typed.
/// - `Quarantine` — candidate is held in a separate quarantine store,
///   pending peer review or auto-expiry. Used when the gate is uncertain
///   but doesn't want to silently drop.
///
/// Per `COGNITIVE-IMMUNE-MODEL.md` §3.8: forensic-not-destructive applies
/// to admission too. `Quarantine` preserves the candidate for later
/// review without admitting it to the live recall surface.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/AdmissionDecision.ts"
)]
#[serde(tag = "decision", content = "data")]
pub enum AdmissionDecision {
    Admit {
        engram: Engram,
        why: String,
    },
    Drop {
        reason: AdmissionDropReason,
    },
    Quarantine {
        engram: Engram,
        reason: String,
        /// Quarantine expiry (epoch ms UTC). After this time the
        /// quarantined candidate auto-drops if not promoted.
        #[ts(type = "number")]
        expiry_ms: u64,
    },
}

impl AdmissionDecision {
    /// The engram this decision actually formed, if any.
    ///
    /// `Some` only for [`Admit`](Self::Admit). A `Drop` (dedup / policy) and a
    /// `Quarantine` did not put anything in the store the rest of the system may
    /// point at — so callers wiring causal edges get `None` and record no edge,
    /// rather than linking to something that was never admitted.
    pub fn admitted_engram_id(&self) -> Option<Uuid> {
        match self {
            Self::Admit { engram, .. } => Some(engram.id),
            Self::Drop { .. } | Self::Quarantine { .. } => None,
        }
    }

    /// Short funnel label for log lines + metrics. Lives next to the
    /// enum so adding a new variant is a compile-fail at this match
    /// rather than a silent fall-through (per claude-tab-2 review nit
    /// on PR #1213 — keeping the label in lockstep with new variants).
    ///
    /// Returns one of `"admit" | "drop" | "quarantine"` — stable
    /// string slugs suitable for grep on log lines and Prometheus
    /// counter labels.
    pub fn label(&self) -> &'static str {
        match self {
            AdmissionDecision::Admit { .. } => "admit",
            AdmissionDecision::Drop { .. } => "drop",
            AdmissionDecision::Quarantine { .. } => "quarantine",
        }
    }
}

/// Categorized reason for dropping a candidate without admitting.
///
/// Distinct from `AdmissionError` (which is for failures of the admission
/// machinery itself). `Drop` is the gate's intentional decision; `Error`
/// is the gate failing to even reach a decision.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/AdmissionDropReason.ts"
)]
#[serde(tag = "reason", content = "detail")]
pub enum AdmissionDropReason {
    /// Candidate had no signal worth remembering (e.g., a routine
    /// heartbeat ack, a duplicate of existing content, etc.).
    NotMemorable { explanation: String },
    /// Candidate matched the source-trust filter but the gate explicitly
    /// chose not to admit (e.g., low-trust source + high-bar topic).
    PolicyDeniedAdmission {
        policy_id: String,
        explanation: String,
    },
    /// Candidate was already engrammed (deduplication hit).
    Duplicate {
        #[ts(type = "string")]
        existing_engram_id: Uuid,
    },
}

//=============================================================================
// ADMISSION ERROR (typed failure modes — fail loud, no silent drops)
//=============================================================================

/// Typed failure modes for the admission machinery itself.
///
/// Per Joel's no-fallback rule + the `try/catch in execute() is
/// forbidden` discipline: these errors are returned, not swallowed.
/// Callers handle them explicitly. Admission failure is never
/// indistinguishable from "no engram created" — the error variant
/// names the cause.
///
/// Same shape as `NoLocalModelLoadable` (#1089) and `NoMultimodalBase`
/// (#1074).
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/AdmissionError.ts"
)]
#[serde(tag = "error", content = "detail")]
pub enum AdmissionError {
    /// The candidate envelope failed signature/proof verification. Cannot
    /// proceed — no admission decision can be made on unverifiable data.
    #[error("envelope verification failed: {detail}")]
    EnvelopeVerificationFailed { detail: String },

    /// The source's trust tier is below the configured threshold for any
    /// admission. Not a `Drop` (which is a policy decision); this is a
    /// hard structural reject before policy runs.
    #[error(
        "trust boundary rejected: source trust {source_trust:?} below threshold {threshold:?}"
    )]
    TrustBoundaryRejected {
        source_trust: TrustState,
        threshold: TrustState,
    },

    /// Replay protection: this nonce/message_id was already processed.
    /// Distinct from `AdmissionDropReason::Duplicate` — that's content
    /// duplication; this is wire-event replay.
    #[error("replay detected: event {event_id} already processed at {previously_seen_at_ms}ms")]
    ReplayDetected {
        event_id: String,
        #[ts(type = "number")]
        previously_seen_at_ms: u64,
    },

    /// The admission Recipe itself failed (panicked, errored internally).
    /// Caller should NOT retry blindly; investigate.
    #[error("admission recipe failed: {recipe_id}: {detail}")]
    RecipeFailure { recipe_id: String, detail: String },

    /// The schema_version on the candidate envelope is one this admission
    /// machinery doesn't understand. Caller should upgrade or reject.
    #[error("unsupported schema version: {schema_version}")]
    UnsupportedSchemaVersion { schema_version: String },
}

//=============================================================================
// TRUST STATE (policy/trust of source, NOT implementation brand)
//=============================================================================

/// Trust tier of an engram's source at admission time.
///
/// Models the SOURCE'S POLICY/TRUST POSITION, not which client implementation
/// produced the data (per Joel 2026-05-13 + Codex relay). A high-quality
/// third-party client signing valid envelopes from an approved peer
/// produces `ApprovedPeer` trust; the official airc CLI from an
/// unauthenticated stranger produces `Untrusted`. Trust is about the
/// source's standing in the polity, not the bytes that carried the data.
///
/// Ordered roughly from least to most trusted; `PartialOrd` derives so
/// admission gates can compare `source_trust >= threshold` directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/TrustState.ts"
)]
pub enum TrustState {
    /// Anonymous / unauthenticated — signature missing or fails.
    Untrusted,
    /// Signature verifies but the sender is not approved to any room
    /// the persona is in.
    Authenticated,
    /// Sender has knocked (via airc#560) but has not yet been approved.
    Knocker,
    /// Approved peer — passed `airc approve` flow (airc#561), is a valid
    /// member of at least one room the persona is in.
    ApprovedPeer,
    /// Member of the persona's intragrid (trusted Tailnet polity).
    IntragridMember,
    /// Member of a SOC governance room (security/audit role with
    /// elevated review authority).
    SocMember,
    /// This persona itself (engrams produced by own cognition).
    SelfTrust,
}

//=============================================================================
// TESTS — serde roundtrip + ts-rs export verification
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    const FIXED_TIME_MS: u64 = 1_715_625_600_000;

    fn sample_airc_ref() -> AircMessageRef {
        AircMessageRef {
            transport: "airc".to_string(),
            room_id: "cambriantech".to_string(),
            message_id: "msg-abc-123".to_string(),
            sender_id: "airc-8a5e".to_string(),
            sent_at_ms: FIXED_TIME_MS,
            received_at_ms: FIXED_TIME_MS,
            content_hash: "sha256:abc".to_string(),
            signature: "sig-base64".to_string(),
            proof_refs: vec![],
            schema_version: "v1".to_string(),
            client_name: Some("airc-bash".to_string()),
        }
    }

    fn sample_engram() -> Engram {
        Engram {
            context_id: None,
            id: Uuid::nil(),
            kind: EngramKind::Episodic,
            content: "Test content".to_string(),
            origin: EngramOrigin::Airc(sample_airc_ref()),
            recall_keys: vec!["test".to_string(), "engram".to_string()],
            admitted_at_ms: FIXED_TIME_MS,
            trust_state_at_admission: TrustState::ApprovedPeer,
            admission_trace_id: Some("trace-xyz".to_string()),
        }
    }

    #[test]
    fn engram_serde_roundtrip() {
        let original = sample_engram();
        let json = serde_json::to_string(&original).expect("serialize");
        let back: Engram = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original.id, back.id);
        assert_eq!(original.content, back.content);
        assert_eq!(original.recall_keys, back.recall_keys);
    }

    #[test]
    fn engram_kind_serde_all_variants() {
        for kind in [
            EngramKind::Episodic,
            EngramKind::Semantic,
            EngramKind::Procedural,
            EngramKind::SelfReflection,
        ] {
            let json = serde_json::to_string(&kind).expect("serialize");
            let back: EngramKind = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(kind, back);
        }
    }

    #[test]
    fn engram_origin_airc_variant_roundtrip() {
        let origin = EngramOrigin::Airc(sample_airc_ref());
        let json = serde_json::to_string(&origin).expect("serialize");
        // Discriminator-tagged: must contain "kind":"Airc"
        assert!(json.contains("\"kind\":\"Airc\""), "tagged json: {}", json);
        let back: EngramOrigin = serde_json::from_str(&json).expect("deserialize");
        match back {
            EngramOrigin::Airc(r) => {
                assert_eq!(r.transport, "airc");
                assert_eq!(r.room_id, "cambriantech");
            }
            _ => panic!("expected Airc variant"),
        }
    }

    #[test]
    fn engram_origin_self_reflection_carries_parent() {
        let parent = Uuid::new_v4();
        let origin = EngramOrigin::SelfReflection {
            parent_engram_id: parent,
        };
        let json = serde_json::to_string(&origin).expect("serialize");
        let back: EngramOrigin = serde_json::from_str(&json).expect("deserialize");
        match back {
            EngramOrigin::SelfReflection { parent_engram_id } => {
                assert_eq!(parent_engram_id, parent);
            }
            _ => panic!("expected SelfReflection variant"),
        }
    }

    #[test]
    fn admission_decision_admit_carries_engram() {
        let decision = AdmissionDecision::Admit {
            engram: sample_engram(),
            why: "high relevance".to_string(),
        };
        let json = serde_json::to_string(&decision).expect("serialize");
        let back: AdmissionDecision = serde_json::from_str(&json).expect("deserialize");
        match back {
            AdmissionDecision::Admit { why, .. } => assert_eq!(why, "high relevance"),
            _ => panic!("expected Admit variant"),
        }
    }

    #[test]
    fn admission_decision_drop_typed_reason() {
        let decision = AdmissionDecision::Drop {
            reason: AdmissionDropReason::Duplicate {
                existing_engram_id: Uuid::nil(),
            },
        };
        let json = serde_json::to_string(&decision).expect("serialize");
        let back: AdmissionDecision = serde_json::from_str(&json).expect("deserialize");
        match back {
            AdmissionDecision::Drop {
                reason: AdmissionDropReason::Duplicate { existing_engram_id },
            } => {
                assert_eq!(existing_engram_id, Uuid::nil());
            }
            _ => panic!("expected Drop with Duplicate reason"),
        }
    }

    #[test]
    fn admission_error_serializes_via_thiserror() {
        let err = AdmissionError::TrustBoundaryRejected {
            source_trust: TrustState::Untrusted,
            threshold: TrustState::ApprovedPeer,
        };
        // thiserror Display path
        let display = format!("{}", err);
        assert!(display.contains("trust boundary rejected"));
        assert!(display.contains("Untrusted"));
        assert!(display.contains("ApprovedPeer"));
        // serde JSON path
        let json = serde_json::to_string(&err).expect("serialize");
        let back: AdmissionError = serde_json::from_str(&json).expect("deserialize");
        match back {
            AdmissionError::TrustBoundaryRejected {
                source_trust,
                threshold,
            } => {
                assert_eq!(source_trust, TrustState::Untrusted);
                assert_eq!(threshold, TrustState::ApprovedPeer);
            }
            _ => panic!("expected TrustBoundaryRejected"),
        }
    }

    #[test]
    fn trust_state_ordering_supports_threshold_comparison() {
        // The whole point of PartialOrd on TrustState: admission gates can
        // compare `source_trust >= threshold` directly.
        assert!(TrustState::ApprovedPeer >= TrustState::Knocker);
        assert!(TrustState::IntragridMember >= TrustState::ApprovedPeer);
        assert!(TrustState::SelfTrust >= TrustState::SocMember);
        assert!(TrustState::Untrusted < TrustState::Authenticated);
    }

    #[test]
    fn airc_message_ref_client_name_is_optional() {
        // Joel's protocol-not-client rule: client_name is optional and
        // informational only. A producer with NO client_name field must
        // still be acceptable.
        let mut r = sample_airc_ref();
        r.client_name = None;
        let json = serde_json::to_string(&r).expect("serialize");
        let back: AircMessageRef = serde_json::from_str(&json).expect("deserialize");
        assert!(back.client_name.is_none());
    }

    #[test]
    fn airc_message_ref_third_party_client_name_accepted() {
        // The protocol-not-client rule means non-airc-CLI client names
        // must be accepted at the type level (admission policy may still
        // care, but the type does not gate).
        let mut r = sample_airc_ref();
        r.client_name = Some("third-party-emitter".to_string());
        let json = serde_json::to_string(&r).expect("serialize");
        let back: AircMessageRef = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.client_name.as_deref(), Some("third-party-emitter"));
    }

    // ── ts-rs binding tests ─────────────────────────────────────────────
    // Mirror the pattern from gpu/memory_manager.rs: each type with
    // #[ts(export, ...)] needs an explicit export_all invocation under a
    // test so cargo test triggers .ts file generation. Without these,
    // the protocol/typescript/persona/*.ts files don't materialize.

    #[test]
    fn export_bindings_engram() {
        let cfg = ts_rs::Config::default();
        Engram::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_engram_kind() {
        let cfg = ts_rs::Config::default();
        EngramKind::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_engram_origin() {
        let cfg = ts_rs::Config::default();
        EngramOrigin::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_airc_message_ref() {
        let cfg = ts_rs::Config::default();
        AircMessageRef::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_chat_message_ref() {
        let cfg = ts_rs::Config::default();
        ChatMessageRef::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_tool_invocation_ref() {
        let cfg = ts_rs::Config::default();
        ToolInvocationRef::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_admission_decision() {
        let cfg = ts_rs::Config::default();
        AdmissionDecision::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_admission_drop_reason() {
        let cfg = ts_rs::Config::default();
        AdmissionDropReason::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_admission_error() {
        let cfg = ts_rs::Config::default();
        AdmissionError::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_trust_state() {
        let cfg = ts_rs::Config::default();
        TrustState::export_all(&cfg).unwrap();
    }

    // ── ORM entity schema tests (#101 slice A) ──────────────────

    /// What this catches: Engram's OrmEntity schema carries the
    /// BaseEntity columns + every Engram domain field. If a future
    /// refactor adds a field to Engram without extending the schema,
    /// the entity↔record round-trip (when the wire-up lands) silently
    /// loses that field. This test makes that drift visible.
    #[test]
    fn engram_orm_schema_has_base_columns_and_domain_fields() {
        use crate::orm::OrmEntity;
        let schema = Engram::collection_schema();
        assert_eq!(schema.collection, "engrams");

        let field_names: std::collections::BTreeSet<&str> =
            schema.fields.iter().map(|f| f.name.as_str()).collect();

        // BaseEntity columns must be present.
        for required in ["id", "createdAt", "updatedAt", "version"] {
            assert!(
                field_names.contains(required),
                "engrams schema missing BaseEntity column {required:?}; have {field_names:?}"
            );
        }

        // Domain columns must be present.
        for required in [
            "kind",
            "content",
            "origin",
            "recallKeys",
            "admittedAtMs",
            "trustStateAtAdmission",
            "admissionTraceId",
        ] {
            assert!(
                field_names.contains(required),
                "engrams schema missing domain column {required:?}; have {field_names:?}"
            );
        }
    }

    /// What this catches: the OrmEntity registry accepts an Engram
    /// schema registration without conflict, and the resolved schema
    /// matches what `collection_schema()` returns. Smoke test for
    /// boot-path registration; same shape RoleTemplate slice 1 used.
    #[test]
    fn engram_registers_and_resolves_through_orm_registry() {
        use crate::orm::entity::OrmEntityRegistry;
        use crate::orm::OrmEntity;
        let registry = OrmEntityRegistry::new();
        registry
            .register::<Engram>()
            .expect("Engram must register cleanly");
        let resolved = registry
            .resolve("engrams")
            .expect("engrams collection must resolve");
        assert_eq!(resolved.collection, "engrams");
        // Same field count as the freshly-built schema — registry
        // didn't drop or duplicate anything.
        assert_eq!(
            resolved.fields.len(),
            Engram::collection_schema().fields.len()
        );
    }

    /// What this catches: end-to-end save / find_by_id / find_all
    /// round-trip on a real Engram through OrmStore<Engram> over real
    /// SQLite. The proof point that the derive migration didn't break
    /// production persistence — Engram is the substrate's most
    /// load-bearing entity, and this test exercises the full
    /// derive → OrmStore → adapter → SQLite → adapter → derive chain.
    #[tokio::test]
    async fn engram_round_trips_through_orm_store_with_derived_schema() {
        use crate::orm::adapter::{AdapterConfig, StorageAdapter};
        use crate::orm::sqlite::SqliteAdapter;
        use crate::orm::OrmStore;
        use std::sync::Arc;

        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("engrams.sqlite");
        let mut adapter = SqliteAdapter::new();
        let mut config = AdapterConfig::default();
        config.connection_string = path.to_string_lossy().into_owned();
        adapter.initialize(config).await.expect("adapter init");
        let adapter: Arc<dyn StorageAdapter> = Arc::new(adapter);

        let store = OrmStore::<Engram>::new(adapter).await.expect("store");

        let engram = sample_engram();
        let original_id = engram.id;
        store.save(engram.id, &engram).await.expect("save engram");

        let loaded = store
            .find_by_id(engram.id)
            .await
            .expect("find_by_id")
            .expect("engram should be present");

        assert_eq!(loaded.id, original_id);
        assert_eq!(loaded.content, "Test content");
        assert_eq!(loaded.kind, EngramKind::Episodic);
        assert_eq!(loaded.admitted_at_ms, FIXED_TIME_MS);
        assert_eq!(loaded.trust_state_at_admission, TrustState::ApprovedPeer);
        assert_eq!(loaded.admission_trace_id.as_deref(), Some("trace-xyz"));
        assert_eq!(loaded.recall_keys, vec!["test", "engram"]);
        match loaded.origin {
            EngramOrigin::Airc(r) => {
                assert_eq!(r.message_id, "msg-abc-123");
                assert_eq!(r.transport, "airc");
            }
            other => panic!("expected Airc origin, got {other:?}"),
        }

        let all = store.find_all().await.expect("find_all");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].0, original_id);
        // tmp drops at function end, removing the tempdir cleanly.
    }
}
