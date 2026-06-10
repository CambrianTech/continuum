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
//! ## Doctrine
//!
//! Per `[[strong-typing-across-boundaries]]`: the typed `Payload`
//! generic is `Serialize` AND ts-rs-exported. The widget side reads
//! the same struct shape; mis-typing on either side is a compile
//! error.
//!
//! Per `[[no-fallbacks-ever]]`: if `serde_json::to_value` fails on a
//! payload, that's a substrate bug at compile-test time, not a
//! runtime fallback. The builder unwraps — substrate types MUST
//! serialize. Test coverage in the consumer module pins this.

use std::sync::Arc;

use positron_core::wire::{StateEnvelope, StateLayer};
use serde::Serialize;

use crate::kinds::KnownKind;
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
    /// [`crate::chat::ChatViewState`]); only the JSON wire shape
    /// crosses positron.
    pub fn session<P: Serialize>(&self, kind: KnownKind, payload: P) -> StateEnvelope {
        self.build(kind, StateLayer::Session, payload)
    }

    /// Frame `payload` as a Persistent-tier update — long-lived
    /// state (< 1 Hz). Profile edits, theme changes.
    pub fn persistent<P: Serialize>(&self, kind: KnownKind, payload: P) -> StateEnvelope {
        self.build(kind, StateLayer::Persistent, payload)
    }

    /// Frame `payload` as an Ephemeral-tier update — sub-second
    /// cadence (≤ 60 Hz). Typing indicators, hover state. AI
    /// observers should NOT subscribe to this layer; the substrate
    /// quantizes aggressively under load.
    pub fn ephemeral<P: Serialize>(&self, kind: KnownKind, payload: P) -> StateEnvelope {
        self.build(kind, StateLayer::Ephemeral, payload)
    }

    /// Frame `payload` as a Semantic-tier update — pull-oriented,
    /// AI-tier meaning extraction. "The conversation shifted topic."
    /// Cognition produces these; renderers don't subscribe.
    pub fn semantic<P: Serialize>(&self, kind: KnownKind, payload: P) -> StateEnvelope {
        self.build(kind, StateLayer::Semantic, payload)
    }

    /// Lower-level escape hatch when the call site already has a
    /// `StateLayer` value (e.g. forwarding from another layer-aware
    /// source). Prefer the named methods above for clarity at the
    /// call site.
    pub fn build<P: Serialize>(
        &self,
        kind: KnownKind,
        layer: StateLayer,
        payload: P,
    ) -> StateEnvelope {
        // Per `[[no-fallbacks-ever]]`: a substrate-owned `Payload`
        // type that fails to serialize is a programming bug, not a
        // runtime condition. Test coverage in the consumer module
        // (`chat::tests::chat_view_state_round_trips`) pins each
        // payload's serde shape.
        let payload = serde_json::to_value(payload)
            .expect("substrate payload must serialize — this is a bug, not a runtime error");
        let revision = self.revisions.next(kind);
        StateEnvelope {
            kind: kind.wire_name().to_string(),
            revision: Some(revision),
            layer,
            payload,
        }
    }

    /// Read the current revision for `kind` without advancing.
    /// Used by the wire-session layer: a resubscribe with
    /// `last_seen.revision < revisions.current(kind)` triggers
    /// replay.
    pub fn current_revision(&self, kind: KnownKind) -> Option<u64> {
        self.revisions.current(kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::{ChatViewState, PersonaSlotView};
    use uuid::Uuid;

    fn empty_chat(room_id: Uuid) -> ChatViewState {
        ChatViewState {
            room_id,
            room_name: "general".into(),
            messages: Vec::new(),
            roster: vec![PersonaSlotView {
                persona_id: Uuid::from_u128(1),
                display_name: "Helper".into(),
                active: true,
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
        let env1 = b.session(KnownKind::Chat, empty_chat(room));
        let env2 = b.session(KnownKind::Chat, empty_chat(room));
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
        let s = b.session(KnownKind::Chat, empty_chat(room));
        let e = b.ephemeral(KnownKind::Chat, empty_chat(room));
        let s2 = b.session(KnownKind::Chat, empty_chat(room));
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
        assert_eq!(b.current_revision(KnownKind::Chat), None);
        let env = b.session(KnownKind::Chat, empty_chat(room));
        assert_eq!(b.current_revision(KnownKind::Chat), env.revision);
    }
}
