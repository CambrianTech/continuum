//! `StateBuilder` — frames typed payloads into
//! `positron_core::wire::StateEnvelope`s with the right kind tag,
//! monotonic revision, and layer.
//!
//! ## Why a builder
//!
//! Three things have to line up on every `StateEnvelope` continuum
//! ships, and getting any one wrong silently breaks the renderer:
//!
//! 1. `kind` must match the renderer's route — a typo and the widget
//!    never updates.
//! 2. `revision` must be monotonic from the substrate's
//!    [`Revisions`](crate::Revisions) source so the session
//!    protocol's `last_seen` replay works.
//! 3. `layer` must reflect the actual cadence class of THIS update —
//!    Session by default (the user-perceivable tier); Ephemeral for
//!    sub-second changes only.
//!
//! The builder centralizes all three so substrate code calling
//! `builder.session(ChatViewState { ... })` can't accidentally
//! re-stringify a kind or forget a revision bump.
//!
//! ## The kind comes from the payload, not a separate argument
//!
//! The builder takes `P: ViewState + Serialize` and reads the kind
//! string from `payload.kind()` — the view's own `KIND` const (open
//! self-registration; there is no central kind enum). This makes a
//! kind/payload mismatch structurally impossible: you cannot frame a
//! `ChatViewState` under the "wall" kind because you never name the
//! kind at the call site — the payload names itself. A new view kind
//! is a new `ViewState` impl in consumer code; the builder needs no
//! edit to frame it.
//!
//! ## Doctrine
//!
//! Per `[[strong-typing-across-boundaries]]`: the typed `Payload`
//! generic is `ViewState + Serialize` AND ts-rs-exported. The widget
//! side reads the same struct shape; mis-typing on either side is a
//! compile error.
//!
//! Per `[[no-fallbacks-ever]]`: if `serde_json::to_value` fails on a
//! payload, that's a substrate bug at compile-test time, not a
//! runtime fallback. The builder unwraps — substrate types MUST
//! serialize. Test coverage in the consumer module pins this.

use std::sync::Arc;

use positron_core::wire::{StateEnvelope, StateLayer};
use positron_core::ViewState;
use serde::Serialize;

use crate::revisions::Revisions;

/// Frames typed substrate state into wire-shaped `StateEnvelope`s.
///
/// Cheap to share via `Arc` — internally just an `Arc<Revisions>`.
/// All builder methods are immutable on `self`; concurrent callers
/// see the same monotonic revision source.
#[derive(Debug, Clone)]
pub struct StateBuilder {
    revisions: Arc<Revisions>,
}

impl StateBuilder {
    /// Construct from an existing revision source. Typical: the
    /// substrate boots one `Revisions` and shares it across every
    /// `StateBuilder` so all kinds draw monotonic revisions from one
    /// well.
    pub fn new(revisions: Arc<Revisions>) -> Self {
        Self { revisions }
    }

    /// Stand-alone builder for tests / examples. Owns its own
    /// `Revisions`.
    pub fn standalone() -> Self {
        Self::new(Arc::new(Revisions::new()))
    }

    /// Frame `payload` as a Session-tier update — the user-
    /// perceivable cadence (1–10 Hz). Default for state changes a
    /// human reads: new chat message, roster delta, room switched.
    ///
    /// Caller's `Payload` type is consumer-defined (e.g.
    /// [`crate::chat::ChatViewState`]) and names its own kind via
    /// `ViewState::kind()`; only the JSON wire shape crosses positron.
    pub fn session<P: ViewState + Serialize>(&self, payload: P) -> StateEnvelope {
        self.build(StateLayer::Session, payload)
    }

    /// Frame `payload` as a Persistent-tier update — long-lived
    /// state (< 1 Hz). Profile edits, theme changes.
    pub fn persistent<P: ViewState + Serialize>(&self, payload: P) -> StateEnvelope {
        self.build(StateLayer::Persistent, payload)
    }

    /// Frame `payload` as an Ephemeral-tier update — sub-second
    /// cadence (≤ 60 Hz). Typing indicators, hover state. AI
    /// observers should NOT subscribe to this layer; the substrate
    /// quantizes aggressively under load.
    pub fn ephemeral<P: ViewState + Serialize>(&self, payload: P) -> StateEnvelope {
        self.build(StateLayer::Ephemeral, payload)
    }

    /// Frame `payload` as a Semantic-tier update — pull-oriented,
    /// AI-tier meaning extraction. "The conversation shifted topic."
    /// Cognition produces these; renderers don't subscribe.
    pub fn semantic<P: ViewState + Serialize>(&self, payload: P) -> StateEnvelope {
        self.build(StateLayer::Semantic, payload)
    }

    /// Lower-level escape hatch when the call site already has a
    /// `StateLayer` value (e.g. forwarding from another layer-aware
    /// source). Prefer the named methods above for clarity at the
    /// call site.
    ///
    /// The kind is read from `payload.kind()` — the view's own `KIND`
    /// const — so it can never disagree with the payload type.
    pub fn build<P: ViewState + Serialize>(&self, layer: StateLayer, payload: P) -> StateEnvelope {
        // Read the kind from the payload itself (its `KIND` const)
        // before we consume `payload` into JSON. `kind()` is a
        // `&'static str`, which is exactly the key `Revisions` wants —
        // zero allocation on the hot path, and no way to stamp a kind
        // that disagrees with the payload's type.
        let kind = payload.kind();
        // Per `[[no-fallbacks-ever]]`: a substrate-owned `Payload`
        // type that fails to serialize is a programming bug, not a
        // runtime condition. Test coverage in the consumer module
        // (`chat::tests::chat_view_state_round_trips`) pins each
        // payload's serde shape.
        let payload_json = serde_json::to_value(payload)
            .expect("substrate payload must serialize — this is a bug, not a runtime error");
        let revision = self.revisions.next(kind);
        StateEnvelope {
            kind: kind.to_string(),
            revision: Some(revision),
            layer,
            payload: payload_json,
        }
    }

    /// Frame an already-serialized `payload` under an explicit `kind` — the escape
    /// hatch for a renderer-agnostic CONTRACT type that deliberately does NOT
    /// implement [`ViewState`] (e.g. the Join Contract's `Experience`, which must not
    /// depend on positron-core to stay renderer-agnostic). The caller passes its own
    /// `KIND` const and the serialized value; the revision is drawn from the same
    /// monotonic [`Revisions`] well keyed by that kind, exactly like [`Self::build`].
    /// Session-tier.
    ///
    /// Prefer [`Self::session`] whenever the payload IS a `ViewState` — there the kind
    /// can't drift from the type. This variant trusts the caller to pass the type's
    /// own `KIND` const, so keep the two paired at the call site.
    pub fn session_raw(&self, kind: &'static str, payload: serde_json::Value) -> StateEnvelope {
        let revision = self.revisions.next(kind);
        StateEnvelope {
            kind: kind.to_string(),
            revision: Some(revision),
            layer: StateLayer::Session,
            payload,
        }
    }

    /// Read the current revision for `kind` without advancing.
    /// Used by the wire-session layer: a resubscribe with
    /// `last_seen.revision < revisions.current(kind)` triggers
    /// replay. Takes a `&str` because the cursor arrives from the wire
    /// as a plain kind string, not a `'static` const.
    pub fn current_revision(&self, kind: &str) -> Option<u64> {
        self.revisions.current(kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::{ChatViewState, Provenance, RosterSlotView, SenderKind};
    use std::collections::BTreeMap;
    use uuid::Uuid;

    // what this catches: session_raw is the renderer-agnostic escape hatch — a
    // non-ViewState payload (like the Join Contract's Experience) still gets a
    // correctly-tagged, monotonically-revisioned Session envelope from the same
    // per-kind Revisions well. If it stopped drawing revisions per kind, wire replay
    // (last_seen) would silently break for that kind.
    #[test]
    fn session_raw_frames_kind_revision_and_payload() {
        let b = StateBuilder::standalone();
        let e1 = b.session_raw("experience", serde_json::json!({ "purpose": "chat" }));
        let e2 = b.session_raw("experience", serde_json::json!({ "purpose": "chat" }));
        assert_eq!(e1.kind, "experience");
        assert!(matches!(e1.layer, StateLayer::Session));
        assert_eq!(e1.payload, serde_json::json!({ "purpose": "chat" }));
        assert!(
            e2.revision.unwrap() > e1.revision.unwrap(),
            "revisions monotonic per kind"
        );
    }

    fn empty_chat(room_id: Uuid) -> ChatViewState {
        ChatViewState {
            room_id,
            room_name: "general".into(),
            purpose: "chat".into(),
            messages: Vec::new(),
            roster: vec![RosterSlotView {
            pronouns: None,
            role_label: None,
            bio: None,
                member_id: Uuid::from_u128(1),
                display_name: "Helper".into(),
                kind: SenderKind::Agent,
                integrations: BTreeMap::new(),
                provenance: Provenance {
                    runtime: "claude".into(),
                },
                active: true,
                availability: Some("ready".into()),
                last_seen_ms: 1_700_000_000_000,
                vitals: BTreeMap::new(),
                loadout: None,
                avatar_url: None,
            genes: Vec::new(),
            }],
        }
    }

    #[test]
    fn session_default_envelope_wires_kind_and_monotonic_revision() {
        // what this catches: regression where the builder forgets to
        // bump the revision or stamps the wrong kind tag. Renderer
        // side routes by kind; session-protocol replay routes by
        // revision. Both load-bearing.
        let b = StateBuilder::standalone();
        let room = Uuid::from_u128(7);
        let env1 = b.session(empty_chat(room));
        let env2 = b.session(empty_chat(room));
        assert_eq!(env1.kind, "chat");
        assert_eq!(env1.layer, StateLayer::Session);
        assert_eq!(env1.revision, Some(1));
        assert_eq!(env2.revision, Some(2));
    }

    #[test]
    fn layers_share_one_per_kind_revision_counter() {
        // what this catches: regression where layers accidentally
        // partition the revision counter. Per Fable's session-
        // protocol design, revision is per-KIND not per-(kind,
        // layer); Session + Ephemeral deliveries OF chat advance the
        // same chat-revision counter.
        let b = StateBuilder::standalone();
        let room = Uuid::from_u128(7);
        let s = b.session(empty_chat(room));
        let e = b.ephemeral(empty_chat(room));
        let s2 = b.session(empty_chat(room));
        assert_eq!(s.revision, Some(1));
        assert_eq!(e.revision, Some(2), "ephemeral shares the chat counter");
        assert_eq!(s2.revision, Some(3));
    }

    #[test]
    fn current_revision_matches_last_built() {
        // what this catches: regression where `current_revision`
        // off-by-ones the wire-session layer's resync logic. A
        // subscriber's `last_seen` is compared against `current` —
        // if `current` is N+1 of what was actually stamped on the
        // envelope, the wire layer would replay forever.
        let b = StateBuilder::standalone();
        let room = Uuid::from_u128(7);
        assert_eq!(b.current_revision(ChatViewState::KIND), None);
        let env = b.session(empty_chat(room));
        assert_eq!(b.current_revision(ChatViewState::KIND), env.revision);
    }
}
