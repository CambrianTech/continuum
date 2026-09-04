//! Per-citizen substrate scoping for PER-USER view kinds (nav slice: scoping A).
//!
//! The node's one shared [`Substrate`] holds per-ROOM kinds — `chat`, `wall`,
//! `kanban` — because those describe *the room*, and every session viewing that
//! room reads the same envelope. But a per-USER view (`nav`, and later settings,
//! the costume-per-activity pick) is different: two citizens' nav can't share one
//! `kind="nav"` slot without overwriting each other (the cache is keyed by kind
//! alone). So per-user kinds get a **citizen-scoped substrate**, handed out here.
//!
//! ## Citizen-agnostic BY CONSTRUCTION
//!
//! There is exactly one method, [`PerUserSubstrates::for_citizen`], and it takes a
//! citizen id — a `Uuid`. It does NOT know or care whether that citizen is a human
//! at a browser or a persona like Asha: a human session and a persona session take
//! the IDENTICAL path to their own nav substrate. That's `[[persona-is-a-client]]`
//! made literal — the scoping code has no is-human branch, so the two can never
//! drift apart. Minimizing the human/persona gap starts with refusing to encode it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use positron_core::wire::StateEnvelope;
use tokio::sync::watch;
use uuid::Uuid;

use crate::cache::StateSource;
use crate::Substrate;

/// What `run_session` needs from a substrate: the snapshot READ (via
/// [`StateSource`]) AND the live BROADCAST subscription, both routed by kind. A
/// plain [`Substrate`] implements it trivially (one store); [`CompositeCache`]
/// routes each per-USER kind to the citizen's store and each per-ROOM kind to the
/// node store — so a session's snapshot AND its live updates both come from the
/// right place, transparently. This is the one seam the session serving loop
/// generalizes over.
pub trait SessionSubstrate: StateSource {
    /// A live receiver for `kind`'s updates, from the store that owns that kind.
    fn subscribe_kind(&self, kind: &str) -> watch::Receiver<Option<Arc<StateEnvelope>>>;
}

impl StateSource for Substrate {
    fn get_state(&self, kind: &str) -> Option<Arc<StateEnvelope>> {
        self.cache().get(kind)
    }
}

impl SessionSubstrate for Substrate {
    fn subscribe_kind(&self, kind: &str) -> watch::Receiver<Option<Arc<StateEnvelope>>> {
        self.broadcast().subscribe(kind)
    }
}

/// Kinds that are PER-USER — routed to the citizen's own substrate. Everything
/// else is per-room and stays on the node substrate. OPEN by data: a new per-user
/// view (settings, the costume-per-activity pick) adds its kind string here, never
/// a branch elsewhere. `nav` is the first.
pub const PER_USER_KINDS: &[&str] = &["nav"];

/// A registry of per-citizen substrates for per-user view kinds. One substrate
/// per citizen, created on first use. Per-room kinds do NOT come here — they stay
/// on the node's shared substrate.
#[derive(Default)]
pub struct PerUserSubstrates {
    by_citizen: Mutex<HashMap<Uuid, Substrate>>,
}

impl PerUserSubstrates {
    pub fn new() -> Self {
        Self::default()
    }

    /// The citizen's own substrate, created on first use. Returns a clone —
    /// [`Substrate`] is `Arc`-shared, so the clone points at the SAME underlying
    /// cache/broadcast; the projector that writes this citizen's nav and the
    /// session that reads it get the same store. Human or persona: identical path.
    pub fn for_citizen(&self, citizen: Uuid) -> Substrate {
        self.by_citizen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(citizen)
            .or_insert_with(Substrate::new)
            .clone()
    }

    /// How many citizens have a per-user substrate. Ops/telemetry read.
    pub fn citizen_count(&self) -> usize {
        self.by_citizen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }
}

/// Kinds that are PER-ROOM — routed to that room's own substrate.
///
/// These describe A ROOM, and a node hosts many rooms at once, so they collide
/// under a single kind slot exactly the way two citizens' `nav` did: the node
/// substrate ends up holding whichever room wrote last — the FOCUSED room — and
/// every other room reads as empty.
///
/// That is not hypothetical. It is what blocks a citizen from reading her own
/// room's roster through the same projection the browser reads
/// (`persona/viewstate_rag.rs`): a persona is a first-class MULTI-room subscriber,
/// so under one shared slot most citizens would see an empty roster rather than a
/// wrong one. OPEN by data, exactly like [`PER_USER_KINDS`] — a new per-room view
/// adds its kind string here, never a branch elsewhere.
pub const PER_ROOM_KINDS: &[&str] = &["chat", "roster", "kanban", "wall"];

/// A registry of per-room substrates for per-room view kinds. One substrate per
/// room, created on first use.
///
/// Deliberately the SAME shape as [`PerUserSubstrates`] rather than a new
/// mechanism: this is the second instance of one idea ("N scopes share a kind
/// namespace"), and the second instance is where you reuse the pattern instead of
/// inventing a parallel one ([[compression]]). Room or citizen, the scoping code
/// is identical — which is also why neither can drift from the other.
#[derive(Default)]
pub struct PerRoomSubstrates {
    by_room: Mutex<HashMap<Uuid, Substrate>>,
}

impl PerRoomSubstrates {
    pub fn new() -> Self {
        Self::default()
    }

    /// The room's own substrate, created on first use. Returns a clone —
    /// [`Substrate`] is `Arc`-shared, so the projector that writes this room's
    /// roster and the consumer that reads it (a browser session OR a citizen's
    /// grounding) get the SAME store. That shared store is the whole point: one
    /// definition, no second fold to go stale.
    pub fn for_room(&self, room: Uuid) -> Substrate {
        self.by_room
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(room)
            .or_insert_with(Substrate::new)
            .clone()
    }

    /// How many rooms have a substrate. Ops/telemetry read.
    pub fn room_count(&self) -> usize {
        self.by_room
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }
}

/// A read view that UNIONS a node substrate (per-room kinds) with a citizen's
/// per-user substrate (per-user kinds), routed by kind. A session reads through
/// this and never knows there are two stores — it asks for a kind, gets the right
/// envelope. This is what lets one session see the room's `chat` AND its own `nav`,
/// each from the correct store, with no session-code change (it just needs a
/// [`StateSource`]). Holds two [`Substrate`] handles (Arc-shared — cheap clones).
pub struct CompositeCache {
    node: Substrate,
    per_user: Substrate,
    /// The store for [`PER_ROOM_KINDS`]. When a caller has not scoped a room yet
    /// this IS the node substrate, which reproduces the pre-room-scoping behavior
    /// exactly — so adding the third scope changes nothing until a caller opts in.
    per_room: Substrate,
}

impl CompositeCache {
    /// Union the shared node substrate with `citizen`'s per-user substrate,
    /// UNSCOPED for rooms (per-room kinds resolve from the node store).
    ///
    /// Preserved verbatim so every existing caller keeps today's behavior; the
    /// room scoping is opt-in via [`Self::scoped`]. A migration that silently
    /// re-pointed every reader would make "did this change anything?"
    /// unanswerable.
    pub fn new(node: Substrate, per_user: Substrate) -> Self {
        Self {
            per_room: node.clone(),
            node,
            per_user,
        }
    }

    /// Union all THREE scopes: node-global kinds (bench, serving, metrics) from
    /// `node`, per-user kinds from `per_user`, per-room kinds from `per_room`.
    ///
    /// This is what lets two citizens in DIFFERENT rooms each read THEIR room's
    /// roster in the same tick — the property the citizen-side positron repair
    /// waits on.
    pub fn scoped(node: Substrate, per_user: Substrate, per_room: Substrate) -> Self {
        Self {
            node,
            per_user,
            per_room,
        }
    }
}

impl StateSource for CompositeCache {
    fn get_state(&self, kind: &str) -> Option<Arc<StateEnvelope>> {
        // Per-user kinds come from the citizen's own store; everything else (the
        // room's chat/wall/kanban) from the shared node store. Data-routed, no
        // is-human / is-persona branch — the same union for every citizen.
        if PER_USER_KINDS.contains(&kind) {
            self.per_user.cache().get(kind)
        } else if PER_ROOM_KINDS.contains(&kind) {
            self.per_room.cache().get(kind)
        } else {
            // Node-global kinds — bench, serving, system-metrics. They describe the
            // NODE, not a room or a citizen, so they have no scope to route to.
            self.node.cache().get(kind)
        }
    }
}

impl SessionSubstrate for CompositeCache {
    fn subscribe_kind(&self, kind: &str) -> watch::Receiver<Option<Arc<StateEnvelope>>> {
        // Same routing as the read: a per-user kind's LIVE updates come from the
        // citizen's store, a per-room kind's from the node store — so the session
        // streams both correctly, not just the initial snapshot.
        if PER_USER_KINDS.contains(&kind) {
            self.per_user.subscribe_kind(kind)
        } else if PER_ROOM_KINDS.contains(&kind) {
            self.per_room.subscribe_kind(kind)
        } else {
            self.node.subscribe_kind(kind)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StateBuilder;

    // A minimal per-user view to store — we only need something with a kind to
    // prove the per-citizen isolation, so a tiny local ViewState stands in.
    #[derive(Debug, Clone, serde::Serialize)]
    struct TinyNav {
        current: String,
    }
    impl positron_core::ViewState for TinyNav {
        fn kind(&self) -> &'static str {
            "nav"
        }
    }

    // what this catches: two handles for the SAME citizen share one underlying
    // store (write via one, read via the other) — so the projector and the session
    // agree — while DIFFERENT citizens are isolated (no nav collision under one kind).
    #[test]
    fn same_citizen_shares_store_different_citizens_isolated() {
        let reg = PerUserSubstrates::new();
        let asha = Uuid::from_u128(0xa54a);
        let joel = Uuid::from_u128(0x101);

        // Two handles for Asha — the projector's and the session's.
        let asha_writer = reg.for_citizen(asha);
        let asha_reader = reg.for_citizen(asha);
        asha_writer.store(StateBuilder::standalone().session(TinyNav {
            current: "room-a".into(),
        }));
        assert!(
            asha_reader.cache().get("nav").is_some(),
            "same citizen: a write on one handle is visible on the other (shared store)"
        );

        // Joel's substrate is a different store — Asha's nav does not leak in.
        let joel_reader = reg.for_citizen(joel);
        assert!(
            joel_reader.cache().get("nav").is_none(),
            "different citizens are isolated — no nav collision under kind=nav"
        );
        assert_eq!(reg.citizen_count(), 2);
    }

    // what this catches: the API is citizen-agnostic — a persona id and a human id
    // are both just Uuids down the identical path; nothing branches on who they are.
    #[test]
    fn human_and_persona_take_the_identical_path() {
        let reg = PerUserSubstrates::new();
        let human = Uuid::from_u128(1);
        let persona = Uuid::from_u128(2);
        // Same call, same type, same behaviour — no is-human branch exists to test,
        // which is the point: both get an isolated substrate the same way.
        let _h = reg.for_citizen(human);
        let _p = reg.for_citizen(persona);
        assert_eq!(reg.citizen_count(), 2);
    }

    #[derive(Debug, Clone, serde::Serialize)]
    struct TinyRoom {
        topic: String,
    }
    impl positron_core::ViewState for TinyRoom {
        fn kind(&self) -> &'static str {
            "chat"
        }
    }

    // what this catches: the composite ROUTES by kind — per-room `chat` resolves
    // from the node store, per-user `nav` from the citizen store — and it does not
    // merge everything into one place (nav is never in node; chat never in per-user).
    // This is the union that lets one session see the room AND its own nav.
    #[test]
    fn composite_routes_per_user_to_citizen_store_room_to_node() {
        let node = Substrate::new();
        let per_user = Substrate::new();
        node.store(StateBuilder::standalone().session(TinyRoom {
            topic: "general".into(),
        }));
        per_user.store(StateBuilder::standalone().session(TinyNav {
            current: "room-a".into(),
        }));
        let composite = CompositeCache::new(node.clone(), per_user.clone());
        assert!(
            composite.get_state("chat").is_some(),
            "per-room kind resolves from the node store"
        );
        assert!(
            composite.get_state("nav").is_some(),
            "per-user kind resolves from the citizen's own store"
        );
        // The union routes, it does not merge: each store holds only its own kinds.
        assert!(node.cache().get("nav").is_none(), "nav never lands in the node store");
        assert!(
            per_user.cache().get("chat").is_none(),
            "chat never lands in the per-user store"
        );
    }

    // what this catches: THE ACCEPTANCE TEST for per-room scoping (#408). Two
    // citizens in DIFFERENT rooms must EACH read THEIR room's state in the same
    // tick. Under the old single-slot model the node substrate held whichever room
    // wrote last, so one of these two would read the other's room — or, once a
    // room-scope gate is applied, read NOTHING. A persona is a first-class
    // multi-room subscriber, so this is the difference between citizens seeing
    // their room and citizens going blind.
    #[test]
    fn two_rooms_each_keep_their_own_state_in_the_same_tick() {
        let rooms = PerRoomSubstrates::new();
        let general = Uuid::from_u128(0x9e2e);
        let academy = Uuid::from_u128(0xacad);

        rooms.for_room(general).store(StateBuilder::standalone().session(TinyRoom {
            topic: "general".into(),
        }));
        rooms.for_room(academy).store(StateBuilder::standalone().session(TinyRoom {
            topic: "academy".into(),
        }));

        // Each room reads ITS OWN topic — neither was overwritten by the other.
        let read = |room: Uuid| -> String {
            let env = rooms
                .for_room(room)
                .cache()
                .get("chat")
                .expect("each room has its own chat state");
            env.payload["topic"].as_str().unwrap().to_string()
        };
        assert_eq!(read(general), "general");
        assert_eq!(read(academy), "academy");
        assert_eq!(rooms.room_count(), 2);
    }

    // what this catches: the same-store guarantee that makes "one definition, two
    // renderers" true. The projector that WRITES a room's roster and the consumer
    // that READS it (a browser session, or a citizen's grounding via
    // ViewStateRagSource) must land on ONE store — otherwise there are two folds
    // again and one of them goes stale (#346).
    #[test]
    fn a_rooms_writer_and_reader_share_one_store() {
        let rooms = PerRoomSubstrates::new();
        let room = Uuid::from_u128(0x1);
        let writer = rooms.for_room(room);
        let reader = rooms.for_room(room);
        writer.store(StateBuilder::standalone().session(TinyRoom {
            topic: "shared".into(),
        }));
        assert!(
            reader.cache().get("chat").is_some(),
            "writer and reader of the same room must share one store — a second \
             store is a second fold, and a second fold is how the board went stale"
        );
    }

    // what this catches: the migration being genuinely additive. `new` must behave
    // EXACTLY as before (per-room kinds from the node store), so adding the third
    // scope changes nothing until a caller opts into `scoped`. If this broke, every
    // existing session would silently start reading an empty per-room store.
    #[test]
    fn the_unscoped_constructor_still_resolves_room_kinds_from_the_node_store() {
        let node = Substrate::new();
        let per_user = Substrate::new();
        node.store(StateBuilder::standalone().session(TinyRoom {
            topic: "general".into(),
        }));
        let composite = CompositeCache::new(node, per_user);
        assert!(
            composite.get_state("chat").is_some(),
            "unscoped composite must keep resolving per-room kinds from node"
        );
    }

    // what this catches: the three-way route. A scoped composite must pull each
    // kind from ITS OWN scope — per-room from the room store, per-user from the
    // citizen store, node-global (bench) from the node — and never merge them.
    #[test]
    fn scoped_composite_routes_all_three_scopes_independently() {
        let node = Substrate::new();
        let per_user = Substrate::new();
        let per_room = Substrate::new();
        per_room.store(StateBuilder::standalone().session(TinyRoom {
            topic: "academy".into(),
        }));
        per_user.store(StateBuilder::standalone().session(TinyNav {
            current: "room-a".into(),
        }));
        node.store(StateBuilder::standalone().session(TinyBench { runs: 3 }));

        let composite = CompositeCache::scoped(node.clone(), per_user.clone(), per_room.clone());
        assert!(composite.get_state("chat").is_some(), "per-room from the room store");
        assert!(composite.get_state("nav").is_some(), "per-user from the citizen store");
        assert!(composite.get_state("bench").is_some(), "node-global from the node store");

        // Routing, not merging: a room kind never lands in node, and the node-global
        // bench never lands in the room store.
        assert!(node.cache().get("chat").is_none(), "chat never lands in node");
        assert!(per_room.cache().get("bench").is_none(), "bench never lands per-room");
    }

    /// A node-global view — describes the NODE, not a room or a citizen.
    #[derive(Debug, Clone, serde::Serialize)]
    struct TinyBench {
        runs: u32,
    }
    impl positron_core::ViewState for TinyBench {
        fn kind(&self) -> &'static str {
            "bench"
        }
    }

    // what this catches: LIVE nav updates (not just the subscribe snapshot) route
    // from the citizen's store — a `nav` broadcast subscription taken through the
    // composite fires when the CITIZEN substrate stores, so the session streams
    // per-user updates from the right place.
    #[test]
    fn composite_broadcast_routes_per_user_updates_to_the_citizen_store() {
        let node = Substrate::new();
        let per_user = Substrate::new();
        let composite = CompositeCache::new(node.clone(), per_user.clone());
        let mut nav_rx = composite.subscribe_kind("nav");
        // Store nav to the citizen store — the composite's nav subscriber must see it.
        per_user.store(StateBuilder::standalone().session(TinyNav {
            current: "room-a".into(),
        }));
        assert!(
            nav_rx.borrow_and_update().is_some(),
            "a per-user nav update reaches the composite's nav subscriber"
        );
    }
}
