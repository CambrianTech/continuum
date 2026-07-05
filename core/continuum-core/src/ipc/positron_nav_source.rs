//! The **navigation projection** (nav slice 2 — a citizen's open tabs, current
//! tab, per-room read cursors, and bookmarks as renderer-shaped
//! [`NavViewState`]).
//!
//! ## The fourth face — but per-CITIZEN, not per-room
//!
//! Sibling to [`crate::ipc::positron_source`] (a room's *conversation*),
//! [`crate::ipc::positron_wall_source`] (its *pinned docs*), and
//! [`crate::ipc::positron_kanban_source`] (its *work board*) — but where those
//! three describe a *room*, this describes a *citizen*: where am I, what's open,
//! what have I read, what's pinned. One [`NavViewState`], rendered as panels on
//! a desktop, tabs+push on a phone, keys in a terminal, and a MENU for the
//! persona (`docs/design/NAVIGATION-ACROSS-MODALITIES.md`).
//!
//! ## It RE-READS a [`NavReader`] seam, never folds its own store
//!
//! Exactly like the kanban projector re-reads airc's authoritative board
//! through [`WorkBoardReader`](crate::ipc::positron_kanban_source), this
//! projector re-reads the citizen's nav facts through the [`NavReader`] seam on
//! each nav change and re-projects. The read cursor half of that seam is
//! already live: `ChannelBookmarks::last_read(user, room)` /
//! `advance(...)` is `NavViewState.last_read` + its `markRead` write path
//! ([[consolidate-before-concern-shared-elements-via-cache]]). The nav state is
//! a cache of that truth, never a second store of it
//! ([[navigation-is-airc-state-one-semantics-many-idioms]]).
//!
//! ## General over activity KIND from day one
//!
//! A tab is any activity — a room, a piece of content, a persona profile, and
//! later a LiveKit call, a game, a sim ([[activity-room-content-tab-pattern-primitives]]).
//! The reader yields a neutral [`NavActivity`] carrying a [`NavTargetKind`], so
//! new activity kinds slot in as data, never as a new nav mechanism.

use std::sync::Arc;

use continuum_positron::nav::{NavBookmark, NavTab, NavTargetKind, NavViewState};
use continuum_positron::{StateBuilder, Substrate};
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use crate::runtime::MessageBus;

/// The bus signal that a citizen's nav state changed (a tab opened/closed, a
/// room read, a bookmark toggled). Carries the affected `user_id`; the content
/// comes from the authoritative [`NavReader`] re-read, never from the delta —
/// same discipline as `kanban:changed`.
pub const NAV_CHANGED: &str = "nav:changed";

/// One open activity as the reader sees it — the raw per-tab facts the
/// projection shapes into a [`NavTab`]. `unread` is the count the reader
/// derives from the read cursor; the projection carries it through (derived
/// once, at the source, not stored twice).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavActivity {
    /// The airc ref this tab opens (room/content/persona id).
    pub id: String,
    /// Human-facing title.
    pub title: String,
    /// What kind of activity — room / content / persona (extends to
    /// livekit / game / sim as new [`NavTargetKind`] variants).
    pub kind: NavTargetKind,
    /// Unread count since the read cursor (0 for non-room / fully-read tabs).
    pub unread: u32,
    /// The last-read cursor for this activity's room (ms/lamport), when it is
    /// a room. `None` for activities with no read cursor.
    pub last_read: Option<i64>,
}

/// The citizen's raw nav facts, re-read from authority on each change. The
/// input to the pure projection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NavSnapshot {
    /// The active tab's ref, or `None` for an empty workspace.
    pub current: Option<String>,
    /// Open activities, in the citizen's arrangement order.
    pub activities: Vec<NavActivity>,
    /// Pinned quick-nav targets (ref, label, kind).
    pub bookmarks: Vec<NavBookmark>,
}

/// The seam this projector re-reads. The live impl reads `ChannelBookmarks`
/// (read cursors) + the citizen's open-room set; a stub drives the tests. Sync
/// because every source is local state (no airc round-trip, unlike the kanban
/// board read).
pub trait NavReader: Send + Sync {
    /// The citizen's current nav facts. Infallible — reads local state; an
    /// empty snapshot is the honest "nothing open" view, never an error.
    fn nav_snapshot(&self, user: Uuid) -> NavSnapshot;
}

/// Pure projection: raw [`NavSnapshot`] → renderer-shaped [`NavViewState`]. No
/// I/O, no substrate — trivially testable, the shape every surface reads.
pub fn project_nav(user: Uuid, snap: NavSnapshot) -> NavViewState {
    let mut last_read = std::collections::BTreeMap::new();
    let open_tabs = snap
        .activities
        .into_iter()
        .map(|a| {
            if let Some(ts) = a.last_read {
                last_read.insert(a.id.clone(), ts);
            }
            NavTab { id: a.id, title: a.title, kind: a.kind, unread: a.unread }
        })
        .collect();
    NavViewState {
        user_id: user,
        current_tab: snap.current,
        open_tabs,
        last_read,
        bookmarks: snap.bookmarks,
    }
}

/// Accumulates a citizen's nav state into [`NavViewState`] and writes each
/// transition to the [`Substrate`]. Not `Clone` — one owner per projection; the
/// consume loop owns it.
struct NavProjection {
    substrate: Substrate,
    builder: StateBuilder,
    /// The citizen this projector describes. Fixed at construction — nav is
    /// per-user, unlike the per-room chat/wall/kanban projectors.
    user_id: Uuid,
    reader: Arc<dyn NavReader>,
}

impl NavProjection {
    fn new(substrate: Substrate, user_id: Uuid, reader: Arc<dyn NavReader>) -> Self {
        Self {
            substrate,
            // Sole writer of the `nav` kind → its own standalone `Revisions`
            // well is the authoritative monotonic source for that kind (same
            // discipline as chat / wall / kanban).
            builder: StateBuilder::standalone(),
            user_id,
            reader,
        }
    }

    /// Re-read the citizen's nav facts and store the re-projected view. Nav is
    /// durable citizen state (open tabs survive a reconnect), so it writes the
    /// PERSISTENT tier — same as the kanban board, unlike the session-tier chat.
    fn reload(&self) {
        let snap = self.reader.nav_snapshot(self.user_id);
        let view = project_nav(self.user_id, snap);
        self.substrate.store(self.builder.persistent(view));
    }
}

/// Spawn the nav projector for `user`: project once now, then re-project on
/// every `nav:changed` / `presence:updated` the bus carries. Mirrors the chat
/// projector's spawn — a passive bus consumer off the transport hot path.
pub fn spawn(
    rt: &tokio::runtime::Handle,
    bus: Arc<MessageBus>,
    substrate: Substrate,
    user: Uuid,
    reader: Arc<dyn NavReader>,
) {
    let mut rx = bus.receiver();
    rt.spawn(async move {
        let projection = NavProjection::new(substrate, user, reader);
        // Project the current nav state immediately so a fresh session sees the
        // workspace without waiting for the next change.
        projection.reload();
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if event.name == NAV_CHANGED || event.name == "presence:updated" {
                        projection.reload();
                    }
                }
                // Fell behind the broadcast buffer — the projection is a
                // last-good cache, not guaranteed delivery. Skip the gap and
                // re-read on the next event.
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => break,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A canned reader — returns a fixed snapshot, the nav analogue of the
    /// kanban `StubReader`.
    struct StubNav(NavSnapshot);
    impl NavReader for StubNav {
        fn nav_snapshot(&self, _user: Uuid) -> NavSnapshot {
            self.0.clone()
        }
    }

    fn room(id: &str, title: &str, unread: u32, last_read: i64) -> NavActivity {
        NavActivity {
            id: id.into(),
            title: title.into(),
            kind: NavTargetKind::Room,
            unread,
            last_read: Some(last_read),
        }
    }

    // what this catches: the projection carries current_tab, derives the
    // per-room last_read map from the activities (one source, not stored twice),
    // and preserves each tab's unread count + order.
    #[test]
    fn projects_snapshot_into_navviewstate_with_read_map() {
        let snap = NavSnapshot {
            current: Some("room-a".into()),
            activities: vec![
                room("room-a", "General", 0, 1_700_000_000_000),
                room("room-b", "Code", 4, 1_699_000_000_000),
            ],
            bookmarks: vec![NavBookmark {
                target: "room-a".into(),
                label: "General".into(),
                kind: NavTargetKind::Room,
            }],
        };
        let view = project_nav(Uuid::from_u128(7), snap);
        assert_eq!(view.user_id, Uuid::from_u128(7));
        assert_eq!(view.current_tab.as_deref(), Some("room-a"));
        assert_eq!(view.open_tabs.len(), 2);
        assert_eq!(view.open_tabs[1].id, "room-b");
        assert_eq!(view.open_tabs[1].unread, 4);
        assert_eq!(view.last_read.get("room-a"), Some(&1_700_000_000_000));
        assert_eq!(view.last_read.get("room-b"), Some(&1_699_000_000_000));
        assert_eq!(view.bookmarks.len(), 1);
    }

    // what this catches: an empty workspace projects to an honest empty nav
    // view (no fabricated default tab), and the reader seam drives it.
    #[test]
    fn empty_snapshot_projects_honest_empty_nav() {
        let view = project_nav(Uuid::from_u128(9), StubNav(NavSnapshot::default()).nav_snapshot(Uuid::from_u128(9)));
        assert!(view.current_tab.is_none());
        assert!(view.open_tabs.is_empty());
        assert!(view.last_read.is_empty());
        assert!(view.bookmarks.is_empty());
    }
}
