//! Typed navigation payloads — `NavViewState`, the substrate-shaped view of a
//! citizen's **navigation state**: which tabs/rooms are open, which one is
//! current, what's been read per room, and what's bookmarked. Fills
//! `StateEnvelope.payload` for `kind="nav"`.
//!
//! ## Navigation is the fourth face — but a PER-CITIZEN one
//!
//! [`crate::chat::ChatViewState`] projects a room's *conversation*,
//! [`crate::wall::WallViewState`] its *pinned documents*,
//! [`crate::kanban::KanbanViewState`] its *work board* — all three are facets
//! of a *room*. This one is different: it's the facet of a *citizen*. Where am
//! I, what do I have open, what have I read, what have I pinned — the answer is
//! the same whether you're a human at a browser, a phone, a terminal, or a
//! persona reading its RAG menu. One `NavViewState`, rendered as panels on a
//! desktop, tabs-plus-push on a phone, a keyboard list in a terminal, and a
//! MENU for the persona (see `docs/design/NAVIGATION-ACROSS-MODALITIES.md`).
//!
//! ## These fields are a VIEW onto airc-owned state
//!
//! Nav state lives in the airc generic per-`(user, scope)` scoped-state store
//! (task #89) — the same store the kanban/wall projections read their room
//! facets from. The continuum-side projector re-reads that authority and shapes
//! it here; this struct is what the renderer reads, the airc rows are the
//! truth, never a second store of them ([[airc-native-identity-rooms-security]],
//! [[navigation-is-airc-state-one-semantics-many-idioms]]).
//!
//! ## `last_read` is the keystone dual-consumer row
//!
//! `last_read[room]` is read TWICE off the same value: the human UI draws an
//! unread badge from it, and the persona's RAG grounding uses it as the "what's
//! new since I last looked" cursor (the bookmark the `ChannelDigest` already
//! keeps — [[consolidate-before-concern-shared-elements-via-cache]]). One write
//! path (`markRead`), two readers. Encoding it here, on the neutral view, is
//! what lets every surface AND the persona agree on "read" without a second
//! store to drift ([[compression]]).
//!
//! ## Why structs, not `serde_json::Value`
//!
//! Same rationale as `chat.rs`/`kanban.rs` ([[strong-typing-across-boundaries]]):
//! the substrate types here ARE the schema; ts-rs mirrors them; the widget side
//! reads typed objects, not `unknown`. The schema grows by extending these
//! structs; the wire kind string stays `"nav"`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// What an open tab / bookmark points AT. A tab can be a room's conversation, a
/// piece of content, or a persona's profile — the neutral kinds the renderer
/// switches its icon/route on. OPEN vocabulary in spirit, but these are the
/// three the nav surfaces route today; a new target kind adds a variant here
/// (a compile-error at every `match`, never a silent `Other` —
/// [[fallbacks-are-illegal-fail-loud]]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/positron/NavTargetKind.ts")]
#[serde(rename_all = "lowercase")]
pub enum NavTargetKind {
    /// A room's conversation (the common case — a chat tab).
    Room,
    /// A piece of content (a document, a wall post, a file view).
    Content,
    /// A persona's profile / brain / activity (the glass-box detail).
    Persona,
}

/// One open tab — an activity the citizen currently has open. `id` is the
/// airc ref (room id / content id / persona id) the nav idiom routes on;
/// `unread` is derived from `last_read` at projection time so the renderer
/// draws a badge without recomputing it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/positron/NavTab.ts")]
pub struct NavTab {
    /// The airc ref this tab opens (room/content/persona id, as a string so a
    /// content ref that isn't a Uuid still fits).
    pub id: String,
    /// Human-facing title (room name, document title, persona display name).
    pub title: String,
    /// What kind of thing this tab points at — drives icon + route.
    pub kind: NavTargetKind,
    /// Unread count since `last_read` for this tab's room (0 for non-room
    /// tabs, or a fully-read room). Derived at projection, not stored twice.
    pub unread: u32,
    /// The activity **purpose** of what this tab opens (`"chat"`, `"foundry"`,
    /// …) — the recipe-defined nature resolved through the room-purpose seam
    /// ([[room-purpose-is-per-recipe-not-an-enum]]). A renderer draws it as the
    /// tab/room's description line and MAY facet on it. Empty = unresolved —
    /// an honest unknown, never a fabricated purpose. `#[serde(default)]` so a
    /// tab serialized before this field folds as empty, never dropped.
    #[serde(default)]
    pub purpose: String,
}

/// A pinned quick-nav target — the citizen's bookmarks (rooms, content,
/// personas). The persona reads these as its menu's pinned items; a human as
/// favourites. Same rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/positron/NavBookmark.ts")]
pub struct NavBookmark {
    /// The airc ref this bookmark points at.
    pub target: String,
    /// Short label shown in the menu / favourites strip.
    pub label: String,
    /// What kind of thing it points at.
    pub kind: NavTargetKind,
}

/// The citizen's navigation state — open tabs, the current one, per-room read
/// cursors, and bookmarks. The single define-once nav model every surface
/// (desktop panels / phone tabs / terminal keys / persona menu) renders in its
/// own idiom, and the web additionally projects onto the URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/positron/NavViewState.ts")]
pub struct NavViewState {
    /// Whose nav state this is (the scope's citizen). Per-user, unlike the
    /// per-room chat/wall/kanban views.
    #[ts(type = "string")]
    pub user_id: Uuid,
    /// The active tab/room ref, or `None` when nothing is open (a fresh
    /// citizen with an empty workspace — a valid view, not an error).
    #[ts(optional)]
    pub current_tab: Option<String>,
    /// Open tabs, in the citizen's arrangement order.
    pub open_tabs: Vec<NavTab>,
    /// Per-room last-read cursor (room ref → last-read timestamp, ms). THE
    /// keystone dual-consumer row: human unread badge + persona RAG cursor.
    #[ts(type = "Record<string, number>")]
    pub last_read: BTreeMap<String, i64>,
    /// Pinned quick-nav targets.
    pub bookmarks: Vec<NavBookmark>,
}

impl NavViewState {
    /// The on-wire `kind` string this view is published under. Owned by the
    /// view (open self-registration), NOT a central enum — a new view adds a
    /// file, never edits a shared catalog ([[fallbacks-are-illegal-fail-loud]]).
    pub const KIND: &'static str = "nav";

    /// An empty nav state for `user` — nothing open, nothing read, nothing
    /// pinned. The honest starting view (not a fabricated default tab).
    pub fn empty(user: Uuid) -> Self {
        Self {
            user_id: user,
            current_tab: None,
            open_tabs: Vec::new(),
            last_read: BTreeMap::new(),
            bookmarks: Vec::new(),
        }
    }
}

impl positron_core::ViewState for NavViewState {
    fn kind(&self) -> &'static str {
        // Single-source the wire string through the view's own `KIND` const —
        // the same "nav" `StateEnvelope.kind` carries — so the trait's view of
        // the kind can never drift from the envelope's.
        Self::KIND
    }
    // `revision()` is the trait default (`None`): the monotonic nav revision is
    // an ENVELOPE-level counter (`Revisions` keyed by the kind string), framed
    // in by `StateBuilder`, not a payload field — same as wall/kanban.
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the "nav" kind string never silently drifts from the
    // trait's view of it, and an empty nav state is the honest fresh view.
    #[test]
    fn empty_nav_state_is_honest_and_kind_is_stable() {
        use positron_core::ViewState;
        let user = Uuid::from_u128(1);
        let nav = NavViewState::empty(user);
        assert_eq!(nav.kind(), "nav");
        assert_eq!(nav.kind(), NavViewState::KIND);
        assert_eq!(nav.user_id, user);
        assert!(nav.current_tab.is_none());
        assert!(nav.open_tabs.is_empty());
        assert!(nav.last_read.is_empty());
        assert!(nav.bookmarks.is_empty());
    }

    // what this catches: unread is a plain derived count (u32→number in ts-rs,
    // no bigint drift) and last_read is a per-room map keyed by ref string.
    #[test]
    fn tab_carries_derived_unread_and_read_cursor_is_per_room() {
        let mut nav = NavViewState::empty(Uuid::from_u128(2));
        nav.open_tabs.push(NavTab {
            id: "room-a".into(),
            title: "General".into(),
            kind: NavTargetKind::Room,
            unread: 3,
            purpose: "chat".into(),
        });
        nav.last_read.insert("room-a".into(), 1_700_000_000_000);
        nav.current_tab = Some("room-a".into());
        assert_eq!(nav.open_tabs[0].unread, 3);
        assert_eq!(nav.last_read.get("room-a"), Some(&1_700_000_000_000));
        assert_eq!(nav.current_tab.as_deref(), Some("room-a"));
    }
}
