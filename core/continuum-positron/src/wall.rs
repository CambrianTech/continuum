//! Typed wall payloads — `WallViewState`, the substrate-shaped view of a
//! room's **wall** (its pinned shared documents) that fills
//! `StateEnvelope.payload` for `kind="wall"`.
//!
//! ## The wall is the room's shared board — the sibling of chat
//!
//! [`crate::chat::ChatViewState`] projects the room's *conversation*;
//! this projects the room's *pinned documents* — the plan, the coding
//! instructions, the agenda, the principles, the recipe. In airc these
//! are `WallPostPublished` rows: event-sourced, broadcast, supersede-
//! chained, keyed by an open consumer-defined `category`. A human edits
//! them with `airc publish --room …`; a persona pins them with
//! `persona/wall/pin`; a widget renders them here. One shared data
//! layer, many faces — the persona already reads this same board for RAG
//! grounding (`persona::wall_source::WallSource`); this module is the
//! *renderer's* face of it, so the wall widget is defined once for web,
//! terminal, mobile, and AI-observer surfaces alike.
//!
//! ## These fields are a VIEW onto airc-owned state
//!
//! `room_id` is the airc `RoomId`; each post rides airc's transcript as a
//! `TranscriptKind::WallPostPublished` event. The **currently-pinned**
//! board is airc's supersede projection (`Airc::wall_posts`) — walk the
//! chain, keep the latest revision per post, drop archived (empty-body)
//! posts. This struct is the projection the renderer reads; the airc row
//! is the truth, never a second store of it. Because the supersede walk
//! is airc-owned, the continuum-side projector *re-reads* that authority
//! on each wall change rather than re-implementing the fold (compression:
//! one supersede impl, in airc).
//!
//! ## Why structs, not `serde_json::Value`
//!
//! Same rationale as `chat.rs` (`[[strong-typing-across-boundaries]]`):
//! the substrate types here ARE the schema; ts-rs mirrors them; the
//! widget side reads typed objects, not `unknown`. The schema grows by
//! extending these structs; the wire kind string stays `"wall"`.
//!
//! ## Attribution is woven in from day one
//!
//! A pinned document is an authored, accountable act — a persona pinning
//! "the coding rules" or a human pinning "the plan" must be attributable
//! on its face, per `[[positron-identity-security-first-class]]`. So each
//! post carries the SAME identity axes a chat row does
//! ([`crate::chat::SenderKind`] + [`crate::chat::Provenance`] + the
//! opaque `integrations` badge map), resolved from the room roster at the
//! substrate side exactly as `ChatMessageView` is — reused, not
//! duplicated (compression). positron stays neutral: an AI author is an
//! `Agent`; whose agent it is rides `integrations`, read at the app
//! layer.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::chat::{Provenance, SenderKind};

/// One currently-pinned wall post — the bits the wall widget needs to
/// render a board entry.
///
/// `post_id`, `room_id`, `author_id` are continuum's substrate UUIDs
/// rendered as strings on the wire (the ts-rs default for `Uuid`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WallPostView {
    /// Stable id across this post's supersede chain — the anchor a
    /// renderer keys a board row on. A revision generates a NEW
    /// `post_id` pointing at the prior; the projection surfaces only the
    /// currently-pinned head of each chain.
    #[ts(type = "string")]
    pub post_id: Uuid,
    #[ts(type = "string")]
    pub room_id: Uuid,
    /// Consumer-defined category (`"doctrine"`, `"rules"`, `"agenda"`,
    /// `"principles"`, `"recipe"`, …). Open string, never an enum — the
    /// substrate makes no inference from it; the renderer groups/filters
    /// on it the way middleware filters on a header
    /// ([[room-purpose-is-per-recipe-not-an-enum]]).
    pub category: String,
    /// Peer that pinned this version (airc's authoritative
    /// `published_by`). Identity below is resolved from this id via the
    /// roster, never carried on the post itself — same discipline as
    /// `ChatMessageView` (identity is a presence fact, not a content
    /// fact).
    #[ts(type = "string")]
    pub author_id: Uuid,
    /// Display name resolved at the substrate side. Renderers must not
    /// re-resolve from `author_id` — that would re-introduce the
    /// widget-local source-of-truth cache positron's contract exists to
    /// prevent. Provisional (a short peer-id label) until the author's
    /// presence card folds in, then upgraded in place
    /// ([[fallbacks-are-illegal-fail-loud]]).
    pub author_name: String,
    /// Neutral author kind. `Agent` covers every AI author; whose agent
    /// it is rides `integrations`, read at the app layer.
    pub author_kind: SenderKind,
    /// Opaque cross-system identity badges, transported straight from the
    /// authoritative airc `Identity.integrations`. positron does NOT
    /// interpret these — the app layer does. Empty until the author's
    /// card resolves.
    #[ts(type = "Record<string, string>")]
    pub integrations: BTreeMap<String, String>,
    /// Verifiable provenance of the author — the accountability half of
    /// identity. Makes a pinned document attributable on its face; grows
    /// to carry trust tier + verification with no wire break.
    pub provenance: Provenance,
    /// Opaque post body. Markdown by convention for human-readable
    /// categories; structured categories may ship JSON. No substrate-side
    /// schema — the renderer decides how to present a category.
    pub body: String,
    /// Unix-ms airc emission time of this pinned version. The board
    /// renders in published order; a renderer MAY show it as an edit
    /// timestamp.
    #[ts(type = "number")]
    pub timestamp: u64,
}

/// Top-level wall state for `kind="wall"` — the focused room plus its
/// currently-pinned board, in airc published-time order.
///
/// The board is a REPLACE-on-change projection: each wall change re-reads
/// airc's supersede-projected `wall_posts()` and swaps the whole `posts`
/// vector, so an unpin (archival supersede) is reflected by absence, never
/// a stale merged entry — the same "full snapshot, replace not merge"
/// discipline the chat roster uses for presence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WallViewState {
    #[ts(type = "string")]
    pub room_id: Uuid,
    /// Currently-pinned posts in airc published-time order. Empty when
    /// the room has no wall — a room with nothing pinned renders an empty
    /// board, not an error ([[substrate-is-a-good-citizen-on-the-host]]).
    pub posts: Vec<WallPostView>,
}

/// `WallViewState` is a first-class positron `ViewState` — the SAME
/// contract renderers (positron-lit's `LitHost`) and the O6 observer
/// bridge key off for chat, so the wall widget routes through the
/// identical seam, not a continuum-private shape. Sibling of
/// `ChatViewState`'s impl: a `kind="wall"` top-level payload is a
/// `ViewState` exactly as `kind="chat"` is.
///
/// The `Clone + Send + Sync + Debug + 'static` bound the trait requires
/// is already satisfied by the struct's derives + its owned-data fields
/// (`Uuid` / `String` / `Vec`), so no new bounds are introduced.
impl positron_core::ViewState for WallViewState {
    fn kind(&self) -> &'static str {
        // Single-source the wire string through `KnownKind` — the same
        // "wall" the `StateEnvelope.kind` carries — so the trait's view
        // of the kind can never drift from the envelope's. Per
        // `[[strong-typing-across-boundaries]]`: the string is encoded
        // once, at the typed kind seam.
        crate::kinds::KnownKind::Wall.wire_name()
    }

    // `revision()` is intentionally the trait default (`None`), for the
    // same reason as `ChatViewState`: the monotonic wall revision is an
    // ENVELOPE-level counter (`Revisions` keyed by `KnownKind`, framed in
    // by `StateBuilder`), NOT a payload field. Carrying a copy here would
    // be two sources of truth for one counter (`[[compression]]`).
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinds::KnownKind;

    fn sample_post() -> WallPostView {
        WallPostView {
            post_id: Uuid::from_u128(1),
            room_id: Uuid::from_u128(9),
            category: "rules".to_string(),
            author_id: Uuid::from_u128(2),
            author_name: "Asha".to_string(),
            author_kind: SenderKind::Agent,
            integrations: BTreeMap::from([(
                "continuum.persona".to_string(),
                "asha".to_string(),
            )]),
            provenance: Provenance::unresolved(),
            body: "Always fail loud; never gate around a missing precondition.".to_string(),
            timestamp: 1_720_000_000_000,
        }
    }

    #[test]
    fn wall_view_state_round_trips() {
        // what this catches: a serde-shape regression on the wire type the
        // wall widget's ts-rs binding is generated from. If a field is
        // renamed / retyped / dropped, the JSON the renderer parses drifts
        // from the generated TS — a silent UI bug this pins to a compile-
        // and-assert failure. Mirrors chat::tests::chat_view_state_round_trips.
        let state = WallViewState {
            room_id: Uuid::from_u128(9),
            posts: vec![sample_post()],
        };
        let json = serde_json::to_string(&state).expect("serialize");
        let back: WallViewState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(state, back);
    }

    #[test]
    fn empty_board_is_a_valid_view_not_an_error() {
        // what this catches: the "room with nothing pinned renders an empty
        // board" contract. A regression that made `posts` non-optional-but-
        // required-non-empty, or that serialized `None`/omitted the field,
        // would break the honest-empty projection the projector depends on.
        let state = WallViewState {
            room_id: Uuid::from_u128(9),
            posts: vec![],
        };
        let json = serde_json::to_string(&state).expect("serialize");
        let back: WallViewState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(state, back);
        assert!(back.posts.is_empty());
    }

    #[test]
    fn wall_view_state_is_a_positron_view_state() {
        // what this catches: regression where `WallViewState` stops being
        // a positron `ViewState` (the impl deleted, or `kind()` hand-rolled
        // to a literal that drifts from the wire name). Renderers
        // (positron-lit's `LitHost`) and the O6 observer bridge
        // route/subscribe off `ViewState::kind()`; if it stops equalling the
        // `StateEnvelope.kind` the substrate emits (`KnownKind::Wall.wire_name()`),
        // the wall widget silently receives state it can't match to a
        // renderer. Also pins `revision()` to the trait default (`None`):
        // the wall revision is an envelope-level counter, never a payload
        // field. Mirrors chat::tests::chat_view_state_is_a_positron_view_state.
        use positron_core::ViewState;
        let state = WallViewState {
            room_id: Uuid::from_u128(9),
            posts: vec![],
        };
        assert_eq!(state.kind(), "wall");
        assert_eq!(
            state.kind(),
            KnownKind::Wall.wire_name(),
            "ViewState::kind() must single-source the wire name, never a drifting literal"
        );
        assert_eq!(state.revision(), None);
    }
}
