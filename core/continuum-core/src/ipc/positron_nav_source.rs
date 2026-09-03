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

use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Mutex};

use continuum_positron::nav::{NavBookmark, NavTab, NavTargetKind, NavViewState};
use continuum_positron::scoping::PerUserSubstrates;
use continuum_positron::{StateBuilder, Substrate};
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::watch;
use uuid::Uuid;

use crate::ipc::positron_source::{
    AircPresenceUpdate, CHAT_FOCUSED, CHAT_POSTED, PRESENCE_UPDATED,
};
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
    /// The activity purpose the room-purpose seam resolved ("chat", "foundry",
    /// …). Empty = unresolved — honest unknown.
    pub purpose: String,
    /// The binding's parent activity (`RoomPurposeSource::parent_for`) — the generic
    /// nesting every recipe gets; the bench-specific solve→run lineage is layered on top.
    pub parent: Option<String>,
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
            {
                // The tree half of #2632: a solve room nests under its run
                // room — the round tracker already RECORDS the lineage
                // (CardActivity.solve_room ↔ round_id), so parenthood is a
                // lookup, never new state. The humanized label derives from
                // the same record (instance · assignee name resolved by the
                // renderer's roster); the raw room id keeps its identity job
                // in `id` and retreats from the reading line.
                let (lineage_parent, display_label) = activity_lineage(&a.id, &a.title);
                // The bench solve→run lineage first (it also names the tab); else the
                // binding's own parent — a run room nests under the room it was
                // dispatched from, a pipeline under its project, generically.
                let parent_ref = if lineage_parent.is_empty() {
                    a.parent.clone().unwrap_or_default()  // unwrap_or: no parent = a top-level activity
                } else {
                    lineage_parent
                };
                NavTab {
                    id: a.id,
                    title: a.title,
                    kind: a.kind,
                    unread: a.unread,
                    purpose: a.purpose,
                    parent_ref,
                    display_label,
                }
            }
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

/// The per-citizen EXPLICIT focus store — the `currentTab` nav fact the
/// `nav/select` verb writes and [`ChannelBookmarksNavReader`] surfaces as
/// `current` (the "explicit focus" its first-room stand-in anticipated).
///
/// HONEST FIRST SLICE, in-core: the canonical home for nav facts is the airc
/// generic per-`(user, scope)` scoped-state store (task #89 —
/// `docs/design/NAVIGATION-ACROSS-MODALITIES.md` §1, the same store the read
/// cursors' `uir:<peer>:<room>`-shaped rows belong to). Until that store
/// exposes a current-tab row, this process-global map is the ONE write path
/// (`nav/select`) and the ONE read path (the nav reader) — migrating is a
/// storage swap behind these two methods, never a second store to drift
/// ([[navigation-is-airc-state-one-semantics-many-idioms]]).
///
/// The target is a `String` ref, not a `Uuid`: a tab can open a content ref
/// that isn't a Uuid (see [`continuum_positron::nav::NavTab::id`]). The stored
/// row is `(target, kind)` — a tab is any activity KIND (room / content /
/// persona), and the reader needs the kind to surface the focused activity as
/// the right tab (a persona focus must NOT read as a room switch).
#[derive(Default)]
struct CitizenNav {
    /// The current tab (the last `nav/select`).
    current: Option<(String, NavTargetKind)>,
    /// The citizen's OPEN non-room activities, in open order. Selecting a
    /// persona OPENS a tab (activity == room == tab — a durable member of
    /// the set, not a transient content overlay); selecting another persona
    /// adds a SECOND tab, never replaces the first. Rooms don't live here —
    /// the room-set fold carries them. A `nav/close` verb removes entries.
    open: Vec<(String, NavTargetKind)>,
}

#[derive(Default)]
pub struct NavFocus {
    inner: Mutex<std::collections::HashMap<Uuid, CitizenNav>>,
}

impl NavFocus {
    /// The citizen's explicit current tab, if one was ever selected.
    pub fn current(&self, user: Uuid) -> Option<(String, NavTargetKind)> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&user)
            .and_then(|n| n.current.clone())
    }

    /// The citizen's open non-room activities (persona homes today), in the
    /// order they were opened. Every entry renders as its own tab.
    pub fn open_activities(&self, user: Uuid) -> Vec<(String, NavTargetKind)> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&user)
            .map(|n| n.open.clone())
            .unwrap_or_default()
    }

    /// Set the citizen's current tab (+ its activity kind), returning the
    /// PREVIOUS focus (the activity being left — when it's a room, the
    /// `markRead` sibling advances its cursor). A non-room select also OPENS
    /// the activity: it joins the citizen's tab set if not already there.
    pub fn focus(
        &self,
        user: Uuid,
        target: String,
        kind: NavTargetKind,
    ) -> Option<(String, NavTargetKind)> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let nav = inner.entry(user).or_default();
        if kind != NavTargetKind::Room && !nav.open.iter().any(|(t, _)| *t == target) {
            nav.open.push((target.clone(), kind.clone()));
        }
        nav.current.replace((target, kind))
    }

    /// Close one open activity tab (the `nav/close` verb's storage half). A
    /// close of the CURRENT tab also clears focus — the reader's first-room
    /// stand-in takes over, exactly like the pre-select state.
    pub fn close(&self, user: Uuid, target: &str) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(nav) = inner.get_mut(&user) {
            nav.open.retain(|(t, _)| t != target);
            if nav.current.as_ref().is_some_and(|(t, _)| t == target) {
                nav.current = None;
            }
        }
    }
}

/// The shared process-global [`NavFocus`] — same singleton pattern as
/// `global_channel_bookmarks()`: one focus row per citizen, read by the nav
/// reader and written by `nav/select`.
pub fn global_nav_focus() -> Arc<NavFocus> {
    use std::sync::OnceLock;
    static G: OnceLock<Arc<NavFocus>> = OnceLock::new();
    G.get_or_init(|| Arc::new(NavFocus::default())).clone()
}

/// The live room-set snapshot — every room the node has observed on the airc
/// stream, `room id → human title` (empty title until `presence:updated`
/// resolves the name). Published by [`spawn_room_set_fold`]'s single fold task
/// over a `watch` cell (the canonical snapshot shape — one owner task, N cheap
/// readers), seeded with the bootstrap room so a fresh boot has its landing
/// room before the first event.
pub type RoomSet = BTreeMap<Uuid, String>;

/// Thin `chat:posted` parse for the fold — the ONLY fact the room set needs
/// from a message event is which room it happened in. The full typed payload
/// stays [`positron_source`]'s concern.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatPostedRoom {
    room_id: Uuid,
}

/// Spawn the single room-set fold: consume `presence:updated` (room id + name)
/// and `chat:posted` (room id) off the bus and publish the accumulated
/// [`RoomSet`] over `watch`. This is a PROJECTION of the observed airc stream —
/// airc's registry stays the truth; this cell is the node's honest "rooms I
/// have seen" cache, exactly like the chat accumulator ([[compression]]: one
/// fold, every citizen's nav reader borrows the same receiver).
///
/// #241: when `registry_socket` is given, the owner task's FIRST act is to
/// ask the airc daemon for the DURABLE subscribed-room registry
/// (`DaemonClient::list_rooms`, airc #1303) and fold every membership row in
/// — so a member's rooms exist in the nav before their first event, and a
/// rebooted interface never collapses to just the rooms that happened to
/// speak (the one-visible-room symptom, live-found 2026-07-31). The fetch
/// runs inside the spawned task: boot never blocks on the daemon, and an
/// unreachable daemon degrades to the traffic-observed behavior with a loud
/// probe, never a wedge.
pub fn spawn_room_set_fold(
    rt: &tokio::runtime::Handle,
    bus: Arc<MessageBus>,
    seed: Vec<(Uuid, String)>,
    registry_socket: Option<std::path::PathBuf>,
) -> watch::Receiver<RoomSet> {
    let initial: RoomSet = seed.into_iter().collect();
    let (tx, rx) = watch::channel(initial);
    let mut events = bus.receiver();
    rt.spawn(async move {
        if let Some(socket) = registry_socket {
            match airc_ipc::DaemonClient::new(socket).list_rooms().await {
                Ok(response) => {
                    tx.send_if_modified(|set| {
                        let mut changed = false;
                        for room in response.rooms {
                            changed |=
                                fold_observed_room(set, room.room_id.as_uuid(), Some(room.name));
                        }
                        changed
                    });
                }
                Err(error) => {
                    crate::probe!(
                        class = "nav.room_registry_seed_failed",
                        error = format!("{error}"),
                        "durable room-registry seed unavailable — nav degrades to \
                         traffic-observed rooms until the daemon answers (#241)"
                    );
                }
            }
        }
        loop {
            match events.recv().await {
                Ok(event) => {
                    let observed: Option<(Uuid, Option<String>)> = if event.name == PRESENCE_UPDATED
                    {
                        AircPresenceUpdate::deserialize(&*event.payload)
                            .ok()
                            .map(|p| (p.room_id, Some(p.room_name)))
                    } else if event.name == CHAT_POSTED {
                        ChatPostedRoom::deserialize(&*event.payload)
                            .ok()
                            .map(|p| (p.room_id, None))
                    } else {
                        None
                    };
                    if let Some((room, name)) = observed {
                        tx.send_if_modified(|set| fold_observed_room(set, room, name));
                    }
                }
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => break,
            }
        }
    });
    rx
}

/// Pure fold step: register/upgrade one observed room in the set. Returns
/// whether the set changed (drives `watch::send_if_modified` — an unchanged
/// observation never wakes subscribers). A presence-resolved (non-empty) name
/// registers the room and/or upgrades its title; a nameless observation (a
/// `chat:posted`) registers an unseen room with an empty title for presence to
/// name later, and is a no-op on a known room.
fn fold_observed_room(set: &mut RoomSet, room: Uuid, name: Option<String>) -> bool {
    match name {
        Some(n) if !n.is_empty() => {
            let entry = set.entry(room).or_default();
            if *entry == n {
                false
            } else {
                *entry = n;
                true
            }
        }
        _ => {
            if set.contains_key(&room) {
                false
            } else {
                set.insert(room, String::new());
                true
            }
        }
    }
}

/// The live member-name snapshot — every citizen the node has observed in a
/// presence roster, `member id → display name`. The identity lookup a
/// persona-kind tab's TITLE resolves through (the same `display_name` the chat
/// roster carries — one presence stream, two folds, zero fabrication).
pub type MemberSet = BTreeMap<Uuid, String>;

/// Spawn the single member-set fold: consume `presence:updated` rosters off the
/// bus and publish the accumulated [`MemberSet`] over `watch` — the member
/// sibling of [`spawn_room_set_fold`] (same canonical shape: one owner task, N
/// cheap readers). A projection of the observed airc presence stream; airc's
/// identity cards stay the truth.
pub fn spawn_member_set_fold(
    rt: &tokio::runtime::Handle,
    bus: Arc<MessageBus>,
) -> watch::Receiver<MemberSet> {
    let (tx, rx) = watch::channel(MemberSet::new());
    let mut events = bus.receiver();
    rt.spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => {
                    if event.name != PRESENCE_UPDATED {
                        continue;
                    }
                    if let Ok(update) =
                        AircPresenceUpdate::deserialize(&*event.payload)
                    {
                        tx.send_if_modified(|set| {
                            let mut changed = false;
                            for slot in &update.roster {
                                let entry = set.entry(slot.member_id).or_default();
                                if *entry != slot.display_name && !slot.display_name.is_empty() {
                                    *entry = slot.display_name.clone();
                                    changed = true;
                                }
                            }
                            changed
                        });
                    }
                }
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => break,
            }
        }
    });
    rx
}

/// The LIVE [`NavReader`] — reads each room's real read cursor from the shared
/// `ChannelBookmarks` (the SAME row the persona's RAG grounding reads: the
/// first-class dual-consumer atom — a human's unread badge and a persona's
/// "what's new since I last looked" are one value) plus real unread from the
/// pre-staged digest buffer.
///
/// The room set is the live [`spawn_room_set_fold`] snapshot — every room the
/// node has observed, not a caller-frozen list; the read cursors are fully live.
pub struct ChannelBookmarksNavReader {
    /// Live room-set snapshot (room id → title), shared with the fold task.
    rooms: watch::Receiver<RoomSet>,
    /// Live member-name snapshot (member id → display name), shared with the
    /// member fold task — resolves a persona-kind tab's title from the SAME
    /// presence stream the chat roster reads (never a fabricated name).
    members: watch::Receiver<MemberSet>,
    /// The room-purpose seam — resolves each room's activity purpose for the
    /// tab's description/facet line. Same default the chat projection uses
    /// (`room_purpose::default_source()`): every room honestly "chat" until
    /// the recipe store answers richer.
    purpose: crate::ipc::room_purpose::SharedRoomPurpose,
}

impl ChannelBookmarksNavReader {
    pub fn new(rooms: watch::Receiver<RoomSet>, members: watch::Receiver<MemberSet>) -> Self {
        Self {
            rooms,
            members,
            purpose: crate::ipc::room_purpose::default_source(),
        }
    }

    /// A reader over a FIXED room set — test/fixture construction (no fold
    /// task). The live path uses [`spawn_room_set_fold`]'s receiver.
    pub fn fixed(rooms: Vec<(Uuid, String)>) -> Self {
        Self::fixed_with_members(rooms, Vec::new())
    }

    /// A fixed reader that also carries a member-name set — the persona-tab
    /// test/fixture construction.
    pub fn fixed_with_members(rooms: Vec<(Uuid, String)>, members: Vec<(Uuid, String)>) -> Self {
        let (_rtx, rrx) = watch::channel(rooms.into_iter().collect::<RoomSet>());
        let (_mtx, mrx) = watch::channel(members.into_iter().collect::<MemberSet>());
        Self {
            rooms: rrx,
            members: mrx,
            purpose: crate::ipc::room_purpose::default_source(),
        }
    }
}

impl NavReader for ChannelBookmarksNavReader {
    fn nav_snapshot(&self, user: Uuid) -> NavSnapshot {
        use crate::cognition::channel_substrate::global_channel_digest_buffer;
        use crate::runtime::ready_buffer::ReadyBuffer;
        // The cursor comes off the STAGED DIGEST, not a separate store: a digest
        // carries the exact `bookmark` it split on, so the projection and the
        // reader's window agree by construction. (The durable cursor itself is
        // airc's `runtime_cursor`, read async at build time; this projection is
        // sync, and the staged digest is the sync-side truth it already has.)
        let digests = global_channel_digest_buffer();
        let rooms = self.rooms.borrow().clone();
        let activities = rooms
            .iter()
            .map(|(room, title)| {
                // REAL cursor — the split point of the digest this citizen was
                // last built. No staged digest → 0 (nothing known yet), the same
                // honest "no info" the unread branch below reports.
                let last = digests
                    .peek(&(user, *room))
                    .map(|d| d.bookmark)
                    .unwrap_or(0);
                // REAL unread from the pre-staged digest when one is staged for
                // this (citizen, room); no staged digest → no unread info yet (an
                // honestly-absent badge, never a fabricated "all read").
                let unread = digests
                    .peek(&(user, *room))
                    .map(|d| d.unread().len() as u32)
                    .unwrap_or(0);
                // A room observed before presence named it gets an honest
                // short-id label, never an invisible empty tab.
                let title = if title.is_empty() {
                    room.to_string()[..8].to_string()
                } else {
                    title.clone()
                };
                NavActivity {
                    id: room.to_string(),
                    title,
                    kind: NavTargetKind::Room,
                    unread,
                    // The recipe-defined activity nature, resolved through the
                    // ONE purpose seam — the tab's description/facet line.
                    purpose: self.purpose.purpose_for(*room),
                    parent: self.purpose.parent_for(*room).map(|p| p.to_string()),
                    last_read: Some(last as i64),
                }
            })
            .collect();
        let mut activities: Vec<NavActivity> = activities;
        // Current = the citizen's EXPLICIT focus (the `nav/select` write —
        // surfaced verbatim: what the citizen selected is the truth, even if
        // the fold hasn't observed that room yet). Before any select, the
        // first room stands in — the honest pre-focus view for a fresh
        // citizen, unchanged from the pre-nav/select behavior.
        let focus_store = global_nav_focus();
        let focus = focus_store.current(user);
        // EVERY open non-room activity surfaces as its OWN tab (`activity ==
        // room == tab`): selecting a persona OPENED a durable tab, and opening
        // a second persona adds a SECOND tab — never a swap (glass-boxed live
        // 2026-07-30: deriving the persona tab from the single `current` focus
        // meant one shape-shifting tab). Titles resolve through the live
        // member-name fold (the same presence display_name the chat roster
        // carries); an unnamed member gets the honest short-id label, exactly
        // like an unnamed room. The persona home's room-ification (a real airc
        // room per citizen) is the follow-up; these tabs ARE the nav truth
        // today, not a parallel router.
        for (target, kind) in focus_store.open_activities(user) {
            let title = Uuid::parse_str(&target)
                .ok()
                .and_then(|id| self.members.borrow().get(&id).cloned())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| target.chars().take(8).collect());
            activities.push(NavActivity {
                id: target,
                title,
                kind,
                unread: 0,
                purpose: "persona".to_string(),
                last_read: None,
                parent: None,
            });
        }
        let current = focus
            .map(|(target, _)| target)
            .or_else(|| rooms.keys().next().map(|r| r.to_string()));
        NavSnapshot {
            current,
            activities,
            bookmarks: Vec::new(),
        }
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
                    // `chat:posted` moves unread counts and can register a new
                    // room; `presence:updated` names rooms; `nav:changed` is the
                    // explicit nav write; `chat:focused` is the select verb's
                    // realtime twin (the `chat:` prefix is never coalesced, so a
                    // rapid re-select whose `nav:changed` the bus coalesced away
                    // still re-projects). All re-read authority (cheap local
                    // reads — same discipline as the kanban projector).
                    if event.name == NAV_CHANGED
                        || event.name == PRESENCE_UPDATED
                        || event.name == CHAT_POSTED
                        || event.name == CHAT_FOCUSED
                    {
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

/// Idempotent per-citizen nav-projector spawner — the boot owns ONE of these,
/// the WS ingress calls [`ensure`](Self::ensure) whenever a citizen-scoped
/// session (`?me=<uuid>`) connects. First arrival spawns that citizen's
/// projector writing into their [`PerUserSubstrates`] cell; every later
/// connection finds it already live. One projector per citizen for the process
/// lifetime — nav is durable citizen state, not connection state.
pub struct NavProjectorRegistry {
    bus: Arc<MessageBus>,
    per_user: Arc<PerUserSubstrates>,
    reader: Arc<dyn NavReader>,
    spawned: Mutex<HashSet<Uuid>>,
}

impl NavProjectorRegistry {
    pub fn new(
        bus: Arc<MessageBus>,
        per_user: Arc<PerUserSubstrates>,
        reader: Arc<dyn NavReader>,
    ) -> Self {
        Self {
            bus,
            per_user,
            reader,
            spawned: Mutex::new(HashSet::new()),
        }
    }

    /// Ensure `citizen`'s nav projector is running. Must be called from within
    /// a tokio runtime (the WS accept path is one). Idempotent — a poisoned
    /// registry lock is unrecoverable state corruption, so it panics loud
    /// rather than double-spawning.
    pub fn ensure(&self, citizen: Uuid) {
        let mut spawned = self
            .spawned
            .lock()
            .expect("nav projector registry lock poisoned");
        if !spawned.insert(citizen) {
            return;
        }
        drop(spawned);
        spawn(
            &tokio::runtime::Handle::current(),
            Arc::clone(&self.bus),
            self.per_user.for_citizen(citizen),
            citizen,
            Arc::clone(&self.reader),
        );
    }
}


/// The activity's tree position + humanized reading-line label (#2632 slice a).
///
/// Pure lookup over records the substrate already keeps: a room that hosts a
/// tracked solve card nests under its round's run room, labeled
/// `<instance> · <assignee-short>`. Anything untracked stays top-level with
/// its own title — honest flat, never an invented hierarchy.
fn activity_lineage(room_ref: &str, title: &str) -> (String, String) {
    let Ok(room) = uuid::Uuid::parse_str(room_ref) else {
        return (String::new(), String::new());
    };
    let Some(act) = crate::cognition::bench_round::team_for_room(room) else {
        return (String::new(), String::new());
    };
    let Some(run_room) = crate::cognition::bench_round::run_room_for_solve(room) else {
        return (String::new(), String::new());
    };
    // `swe--<instance>--<card8>` → `<instance>`; an unconventional name keeps
    // its title (label stays honest, never lossy).
    let instance = title
        .strip_prefix("swe--")
        .and_then(|rest| rest.rsplit_once("--").map(|(i, _)| i))
        .unwrap_or(title);
    // Outsider-readable (Joel, 2026-08-31: "I don't really even comprehend
    // what those rooms are"): `repo__repo-1234` reads as its issue half
    // (`pylint-7114`), and the assignee is her NAME when the registry knows
    // her — a hex prefix labels nothing for a human.
    let short_instance = instance.rsplit_once("__").map(|(_, tail)| tail).unwrap_or(instance);
    let who = crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()
        .map(|reg| reg.roster_snapshot())
        .and_then(|snap| {
            snap.into_iter()
                .find(|(_, peer)| *peer == act.assignee)
                .map(|(name, _)| name)
        })
        .unwrap_or_else(|| act.assignee.to_string()[..8].to_string());
    (
        run_room.to_string(),
        format!("{short_instance} · {who}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A canned reader — returns a fixed snapshot, the nav analogue of the
    /// kanban `StubReader`.
    // what this catches: nesting is GENERIC — an activity whose binding names a parent
    // nests under it in the nav even when it is not a bench solve room. Before
    // 2026-09-03 only solve rooms nested (under their run room, via the bench tracker);
    // a dispatched run room was a flat top-level tab, or absent (Joel: "active benchmark
    // rooms don't even show up… not under the base academy room").
    #[test]
    fn a_bound_activity_nests_under_its_binding_parent() {
        let academy = Uuid::new_v4();
        let run = Uuid::new_v4();
        let mut a = room(&run.to_string(), "bench-swe-bench-verified-1", 0, 0);
        a.parent = Some(academy.to_string());
        let snap = NavSnapshot { current: None, activities: vec![a], bookmarks: vec![] };
        let view = project_nav(Uuid::new_v4(), snap);
        assert_eq!(view.open_tabs[0].parent_ref, academy.to_string());
    }

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
            purpose: "chat".into(),
            last_read: Some(last_read),
            parent: None,
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

    // what this catches: the LIVE reader surfaces the SAME split point the
    // citizen's own window was built at — the digest's `bookmark`. One value, two
    // consumers (her perception and the nav view), so they cannot disagree. The
    // durable cursor behind it is airc's `runtime_cursor`; the staged digest is
    // what this sync projection can see of it.
    #[test]
    fn live_reader_reflects_the_real_shared_bookmark() {
        use crate::cognition::channel_substrate::global_channel_digest_buffer;
        use crate::runtime::ready_buffer::ReadyBuffer;
        // Unique ids so the process-global digest buffer can't collide with
        // another test staging a different (user, room).
        let asha = Uuid::from_u128(0xa54a_u128);
        let room = Uuid::from_u128(0x9e21_u128);
        global_channel_digest_buffer().publish(
            (asha, room),
            std::sync::Arc::new(crate::cognition::channel_digest::ChannelDigest {
                room_id: room,
                persona_id: asha,
                bookmark: 42,
                elements: Vec::new(),
                unread_start: 0,
            }),
        );
        let reader = ChannelBookmarksNavReader::fixed(vec![(room, "General".into())]);
        let snap = reader.nav_snapshot(asha);
        let view = project_nav(asha, snap);
        assert_eq!(view.open_tabs.len(), 1);
        assert_eq!(view.open_tabs[0].title, "General");
        assert_eq!(view.last_read.get(&room.to_string()), Some(&42));
        assert_eq!(view.current_tab, Some(room.to_string()));
    }

    // what this catches: an empty workspace projects to an honest empty nav
    // view (no fabricated default tab), and the reader seam drives it.
    #[test]
    fn empty_snapshot_projects_honest_empty_nav() {
        let view = project_nav(
            Uuid::from_u128(9),
            StubNav(NavSnapshot::default()).nav_snapshot(Uuid::from_u128(9)),
        );
        assert!(view.current_tab.is_none());
        assert!(view.open_tabs.is_empty());
        assert!(view.last_read.is_empty());
        assert!(view.bookmarks.is_empty());
    }

    // what this catches: the room-set fold's change discipline — a chat event
    // registers an unseen room (empty title), presence names it, repeats of
    // either are NOT modifications (send_if_modified must not wake subscribers
    // on a no-op), and a rename upgrades the title.
    #[test]
    fn room_set_fold_registers_names_and_skips_noops() {
        let room = Uuid::from_u128(0xf00d);
        let mut set = RoomSet::new();
        assert!(
            fold_observed_room(&mut set, room, None),
            "first sighting registers"
        );
        assert_eq!(set.get(&room).map(String::as_str), Some(""));
        assert!(
            !fold_observed_room(&mut set, room, None),
            "repeat chat = no-op"
        );
        assert!(
            fold_observed_room(&mut set, room, Some("general".into())),
            "presence names the room"
        );
        assert_eq!(set.get(&room).map(String::as_str), Some("general"));
        assert!(
            !fold_observed_room(&mut set, room, Some("general".into())),
            "same name = no-op"
        );
        assert!(
            fold_observed_room(&mut set, room, Some("general-2".into())),
            "rename upgrades"
        );
        assert!(
            !fold_observed_room(&mut set, room, Some(String::new())),
            "empty name never erases a known room"
        );
        assert_eq!(set.get(&room).map(String::as_str), Some("general-2"));
    }

    // what this catches: the explicit-focus half of the reader — after a
    // `nav/select` write lands in the shared NavFocus store, `current` must
    // surface THAT room (not the first-room stand-in), and a citizen who never
    // selected keeps the first-room fallback. The read half of the seam the
    // nav/select command's own test drives the write half of.
    #[test]
    fn explicit_focus_beats_the_first_room_as_current() {
        // Unique user ids: the focus store is process-global, so a shared id
        // would collide with a parallel test's write.
        let room_a = Uuid::from_u128(0x50a);
        let room_b = Uuid::from_u128(0x50b);
        let rooms = vec![
            (room_a, "General".to_string()),
            (room_b, "Code".to_string()),
        ];
        let reader = ChannelBookmarksNavReader::fixed(rooms.clone());

        let fresh = Uuid::from_u128(0x50f1);
        assert_eq!(
            reader.nav_snapshot(fresh).current,
            Some(room_a.to_string()),
            "no select yet → first room stands in"
        );

        let selector = Uuid::from_u128(0x50f2);
        global_nav_focus().focus(selector, room_b.to_string(), NavTargetKind::Room);
        assert_eq!(
            reader.nav_snapshot(selector).current,
            Some(room_b.to_string()),
            "the explicit focus is surfaced as current"
        );
    }

    // what this catches: a persona-kind focus surfaces as its OWN open tab —
    // kind Persona, purpose "persona", title resolved from the live member-name
    // fold (the presence display_name, never a fabricated label) — and becomes
    // current WITHOUT displacing any room tab. The honest first slice of the
    // persona home: the tab is nav truth, the chat projection stays untouched.
    #[test]
    fn persona_focus_surfaces_a_persona_tab_with_resolved_name() {
        let room = Uuid::from_u128(0x9e50_a);
        let asha = Uuid::from_u128(0x9e50_b);
        let reader = ChannelBookmarksNavReader::fixed_with_members(
            vec![(room, "General".into())],
            vec![(asha, "Asha".into())],
        );
        let user = Uuid::from_u128(0x9e50_c);
        global_nav_focus().focus(user, asha.to_string(), NavTargetKind::Persona);
        let snap = reader.nav_snapshot(user);
        assert_eq!(snap.current, Some(asha.to_string()));
        let persona_tab = snap
            .activities
            .iter()
            .find(|a| a.kind == NavTargetKind::Persona)
            .expect("persona tab surfaced");
        assert_eq!(persona_tab.title, "Asha", "title from the member fold");
        assert_eq!(persona_tab.purpose, "persona");
        assert_eq!(persona_tab.unread, 0);
        // The room tab is still there — a persona tab ADDS, never displaces.
        assert!(snap
            .activities
            .iter()
            .any(|a| a.kind == NavTargetKind::Room));
    }

    // what this catches: a persona focus whose name the fold hasn't observed
    // yet gets the honest short-id label — same discipline as an unnamed room,
    // never an invisible or fabricated title.
    #[test]
    fn unnamed_persona_focus_gets_short_id_label() {
        let room = Uuid::from_u128(0x9e51_a);
        let stranger = Uuid::from_u128(0x9e51_b);
        let reader = ChannelBookmarksNavReader::fixed(vec![(room, "General".into())]);
        let user = Uuid::from_u128(0x9e51_c);
        global_nav_focus().focus(user, stranger.to_string(), NavTargetKind::Persona);
        let snap = reader.nav_snapshot(user);
        let tab = snap
            .activities
            .iter()
            .find(|a| a.kind == NavTargetKind::Persona)
            .expect("persona tab surfaced");
        assert_eq!(
            tab.title,
            stranger.to_string().chars().take(8).collect::<String>()
        );
    }

    // what this catches: a room the fold has seen but presence hasn't named
    // renders an honest short-id tab label, never an invisible empty title.
    #[test]
    fn unnamed_room_gets_short_id_tab_label() {
        let room = Uuid::from_u128(0xbeef);
        let reader = ChannelBookmarksNavReader::fixed(vec![(room, String::new())]);
        let snap = reader.nav_snapshot(Uuid::from_u128(11));
        assert_eq!(snap.activities.len(), 1);
        assert_eq!(snap.activities[0].title, room.to_string()[..8].to_string());
    }

    // what this catches: the registry spawns a citizen's projector exactly once
    // — the second ensure() for the same citizen must not double-spawn (nav is
    // per-citizen process-lifetime state, not per-connection state).
    #[tokio::test]
    async fn registry_ensures_a_projector_once_per_citizen() {
        let per_user = Arc::new(PerUserSubstrates::new());
        let registry = NavProjectorRegistry::new(
            Arc::new(crate::runtime::MessageBus::new()),
            Arc::clone(&per_user),
            Arc::new(ChannelBookmarksNavReader::fixed(vec![(
                Uuid::from_u128(0xcafe),
                "general".into(),
            )])),
        );
        let me = Uuid::from_u128(0xa11ce);
        registry.ensure(me);
        registry.ensure(me);
        assert_eq!(
            registry.spawned.lock().expect("registry lock").len(),
            1,
            "one citizen, one projector"
        );
        // The spawned projector's immediate reload lands the citizen's nav view
        // in THEIR substrate — poll briefly (spawn is async wrt this test).
        let substrate = per_user.for_citizen(me);
        let mut seen = false;
        for _ in 0..50 {
            if substrate.cache().get(NavViewState::KIND).is_some() {
                seen = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(
            seen,
            "the citizen's nav view materialized in their per-user substrate"
        );
    }
}
