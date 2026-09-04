//! `RecipeRoomPurpose` — the room→recipe binding, finally READ (#6, #274, #329).
//!
//! ## The gap this closes
//!
//! [`crate::modules::activity`]'s `activity/spawn` has always published a room's
//! recipe binding to the wall, and its own doc says why: *"Without this the room
//! forgets which recipe it is and every client falls back to projecting it as a
//! plain chat room."* That was accurate — and the binding **had no reader**. The
//! only [`RoomPurposeSource`] wired into the process was [`DefaultRoomPurpose`],
//! which answers `"chat"` for every room in existence.
//!
//! So the whole recipe layer was live and inert at once: recipes authored as data,
//! `RecipeExperienceSource` projecting them, four shipped manifests including a
//! benchmark with a scoreboard region — and every room resolving to `"chat"`
//! anyway. A benchmark run's room and a chat room were the same object to every
//! renderer, and to the citizen standing inside one. That is what "benchmarks are
//! a parallel system" looks like at the substrate: not a missing feature, a
//! **write with no reader**
//! ([[a-correct-check-that-nothing-calls-is-nastier-than-a-missing-one]]).
//!
//! ## Shape: an event-invalidated cache, not a per-read fetch
//!
//! [`RoomPurposeSource::purpose_for`] is **sync** and sits on the projection's
//! store path — a wall read there would put daemon I/O inside a render. So this is
//! the canonical shape instead: one owner task folds `wall:changed` off the bus
//! and re-reads the authoritative binding for the room that changed; readers take
//! a read-lock on a small map. Purpose changes when someone re-binds a room —
//! roughly never — so the cache is nearly always warm and always current
//! ([[rag-sources-are-event-invalidated-caches]], [[the-whole-system-is-event-based-not-polling]]).
//!
//! Re-reading (rather than trusting the `wall:changed` delta) is the same
//! discipline [`crate::ipc::positron_wall_source`] documents: the supersede chain
//! is airc-owned and cannot be reconstructed from one delta.
//!
//! ## What stays honest
//!
//! - A room with **no** binding resolves to `"chat"` — the trait requires a total
//!   function, and an unbound room genuinely IS a plain chat room.
//! - A room whose binding is present but **unreadable** also resolves to `"chat"`,
//!   and says so LOUDLY on the probe stream. The purpose seam has nowhere to put
//!   an error, so the refusal to guess lives in
//!   [`project_binding`](crate::experience::binding::project_binding) and the
//!   noise lives here — never a silent downgrade.
//! - A binding naming a purpose no recipe declares resolves to that purpose
//!   verbatim; `RecipeExperienceSource` then honestly returns no manifest rather
//!   than substituting one.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use airc_core::doctrine::WallPostPublished;

use crate::experience::binding::{project_binding, RECIPE_WALL_CATEGORY};
use crate::ipc::room_purpose::RoomPurposeSource;
use crate::runtime::MessageBus;

/// The purpose an unbound room reports — the honest default the
/// [`RoomPurposeSource`] contract names. A bare `airc join` makes a chat room.
pub const UNBOUND_PURPOSE: &str = "chat";

/// Reads a ROOM'S recipe-category wall posts — room-scoped, never
/// current-room-scoped.
///
/// A trait rather than a bare `Arc<Airc>` for the reason
/// [`crate::persona::wall_source::WallReader`] is one: the fold is then testable
/// without a daemon, and the "which room" argument is explicit at the seam. The
/// current-room read (`Airc::wall_posts`) is precisely the wrong primitive here —
/// it answers about whatever room the handle happens to point at, which is a
/// plausible answer for the wrong room, with nothing in the result saying so.
#[async_trait]
pub trait RoomRecipeReader: Send + Sync {
    /// This room's posts under [`RECIPE_WALL_CATEGORY`], supersede-projected, in
    /// published order. An error is a READ failure (daemon down, room not
    /// resolvable) — distinct from an empty vec, which means "no binding".
    async fn recipe_posts(&self, room_id: Uuid) -> Result<Vec<WallPostPublished>, String>;

    /// Every room this reader can resolve — the boot seed set.
    ///
    /// Without it the index would only learn a room's purpose when its wall NEXT
    /// changes, so an activity spawned before this core booted would render as
    /// chat until someone happened to re-pin something. A room's identity must
    /// survive a reboot; it is durable wall state, not live traffic.
    async fn known_rooms(&self) -> Result<Vec<Uuid>, String>;
}

/// `room id → purpose`, folded by the owner task and read by the projections.
///
/// Cheap to clone (`Arc` inside) so the same index backs the chat projection, the
/// experience source, and anything else that must agree on what a room IS —
/// exactly one answer per room, process-wide ([[compression]]).
#[derive(Clone, Default)]
pub struct RecipeRoomPurpose {
    /// room → (purpose, the binding's parent activity).
    index: Arc<RwLock<HashMap<Uuid, (String, Option<Uuid>)>>>,
}

impl RecipeRoomPurpose {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a room's resolved purpose. Idempotent; last write wins, which is
    /// what a re-bind means.
    fn set(&self, room_id: Uuid, purpose: String, parent: Option<Uuid>) {
        let mut index = self.index.write().unwrap_or_else(|e| e.into_inner());
        index.insert(room_id, (purpose, parent));
    }

    /// Resolve one room's binding through `reader` and fold it in. Returns the
    /// purpose now in effect for that room.
    ///
    /// Every failure path lands on [`UNBOUND_PURPOSE`] and says why on the probe
    /// stream — the seam is a total function, so the only choice is between a
    /// LOUD default and a silent one.
    pub async fn refresh(&self, room_id: Uuid, reader: &dyn RoomRecipeReader) -> String {
        let mut parent: Option<Uuid> = None;
        let purpose = match reader.recipe_posts(room_id).await {
            Err(error) => {
                crate::probe!(
                    class = "activity.purpose.read_failed",
                    room_id = %room_id,
                    error = %error,
                    "could not read a room's recipe binding — it resolves as a plain chat \
                     room until the next wall change, so a purpose-built activity may be \
                     rendering without its regions"
                );
                UNBOUND_PURPOSE.to_string()
            }
            Ok(posts) => match project_binding(&posts) {
                Ok(Some(binding)) => {
                    parent = binding.parent.map(|p| p.as_uuid());
                    binding.recipe
                }
                Ok(None) => UNBOUND_PURPOSE.to_string(),
                Err(error) => {
                    crate::probe!(
                        class = "activity.purpose.unreadable_binding",
                        room_id = %room_id,
                        error = %error,
                        "a room is BOUND to an activity this build cannot read — it will \
                         render as a plain chat room, which is a downgrade, not a default"
                    );
                    UNBOUND_PURPOSE.to_string()
                }
            },
        };
        self.set(room_id, purpose.clone(), parent);
        purpose
    }
}

impl RoomPurposeSource for RecipeRoomPurpose {
    fn purpose_for(&self, room_id: Uuid) -> String {
        self.index
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&room_id)
            .map(|(purpose, _)| purpose.clone())
            .unwrap_or_else(|| UNBOUND_PURPOSE.to_string())
    }

    fn parent_for(&self, room_id: Uuid) -> Option<Uuid> {
        self.index
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&room_id)
            .and_then(|(_, parent)| *parent)
    }
}

/// Spawn the purpose index's owner task over an already-built reader: seed every
/// known room, then fold `wall:changed` forever.
///
/// Returns the index handle SYNCHRONOUSLY — it is a shared map, so the chat
/// projection can hold it from boot while the seed reads are still in flight. A
/// room resolves as chat until its binding lands, which is what it did before
/// this module existed; no consumer has to wait on a daemon to come up.
pub fn spawn_purpose_index(
    rt: &tokio::runtime::Handle,
    bus: Arc<MessageBus>,
    reader: Arc<dyn RoomRecipeReader>,
) -> RecipeRoomPurpose {
    let purpose = RecipeRoomPurpose::new();
    let owned = purpose.clone();
    // Subscribe BEFORE spawning so no wall change can race ahead of the receiver
    // — the same ordering discipline the chat projection uses.
    let events = bus.receiver();
    rt.spawn(async move { run_purpose_loop(owned, reader, events).await });
    purpose
}

/// The node's purpose index: attach a reader of its own, then run the loop.
///
/// Its own airc identity + home, exactly like the wall and kanban node
/// projectors — a reader that must resolve ANY room's wall cannot borrow a handle
/// pinned to one room. Consolidating the node readers into a single attach is a
/// real follow-up; it is named here rather than pretended away.
pub fn spawn_node_purpose_index(
    rt: &tokio::runtime::Handle,
    bus: Arc<MessageBus>,
    daemon_socket: std::path::PathBuf,
    node_home: std::path::PathBuf,
    room_name: String,
) -> RecipeRoomPurpose {
    let purpose = RecipeRoomPurpose::new();
    let owned = purpose.clone();
    let events = bus.receiver();
    rt.spawn(async move {
        if let Err(error) = tokio::fs::create_dir_all(&node_home).await {
            tracing::error!(
                %error,
                home = %node_home.display(),
                "recipe_room_purpose: cannot create reader home — every room will render \
                 as a plain chat room"
            );
            return;
        }
        let airc = match airc_lib::Airc::attach_as(
            node_home.clone(),
            NODE_PURPOSE_READER_NAME,
            daemon_socket,
        )
        .await
        {
            Ok(airc) => airc,
            Err(error) => {
                tracing::error!(
                    %error,
                    home = %node_home.display(),
                    "recipe_room_purpose: reader attach failed — every room will render as \
                     a plain chat room, so a recipe-spawned activity loses its regions"
                );
                return;
            }
        };
        // Join by NAME (never UUID-as-string, which derives a DIFFERENT channel —
        // the recurring hazard the presence projector documents). The reader needs
        // at least one real subscription for `subscription_set` to resolve rooms.
        if let Err(error) = airc.join(&room_name).await {
            tracing::error!(
                %error,
                room = %room_name,
                "recipe_room_purpose: reader could not join — room purposes stay unresolved"
            );
            return;
        }
        tracing::info!(
            room = %room_name,
            "recipe_room_purpose: node reader attached — resolving room activities"
        );
        let reader: Arc<dyn RoomRecipeReader> = Arc::new(AircRecipeReader {
            airc: Arc::new(airc),
        });
        run_purpose_loop(owned, reader, events).await;
    });
    purpose
}

/// The airc identity the purpose reader attaches as — its own, so its reads are
/// attributable and never entangled with the presence/wall/kanban readers.
const NODE_PURPOSE_READER_NAME: &str = "node-purpose-reader";

/// Seed every known room, then fold wall changes forever. Shared by both spawn
/// entry points so the injected-reader path and the live path cannot drift.
async fn run_purpose_loop(
    purpose: RecipeRoomPurpose,
    reader: Arc<dyn RoomRecipeReader>,
    mut events: tokio::sync::broadcast::Receiver<crate::runtime::BusEvent>,
) {
    match reader.known_rooms().await {
        Ok(rooms) => {
            // Probe the SEED ITSELF, not only its interesting outcomes. A fold whose
            // success is silent is indistinguishable from a fold that never ran —
            // which is exactly the ambiguity this hit on its first live test, where
            // "no probes" could equally have meant "never spawned" or "nothing bound"
            // ([[a-correct-check-that-nothing-calls-is-nastier-than-a-missing-one]]).
            let total = rooms.len();
            let mut bound = 0usize;
            for room_id in rooms {
                let resolved = purpose.refresh(room_id, reader.as_ref()).await;
                if resolved != UNBOUND_PURPOSE {
                    bound += 1;
                    crate::probe!(
                        class = "activity.purpose.resolved",
                        room_id = %room_id,
                        purpose = %resolved,
                        "a room resolved to its authored activity — its recipe's regions and \
                         affordances now reach every renderer, human and citizen alike"
                    );
                }
            }
            crate::probe!(
                class = "activity.purpose.seeded",
                rooms = total,
                bound,
                "activity-purpose index seeded — `bound` rooms carry a recipe, the rest are \
                 plain chat rooms"
            );
        }
        Err(error) => {
            crate::probe!(
                class = "activity.purpose.seed_failed",
                error = %error,
                "could not enumerate rooms to seed activity purposes — every room reads as \
                 chat until its wall next changes"
            );
        }
    }
    loop {
        match events.recv().await {
            Ok(event) => {
                let Some(room_id) = wall_changed_room(&event.name, &event.payload) else {
                    continue;
                };
                let resolved = purpose.refresh(room_id, reader.as_ref()).await;
                // Wall changes are rare (a re-pin, a re-bind), so probing every cue
                // costs nothing and makes the invalidation path observable instead of
                // inferred ([[the-whole-system-is-event-based-not-polling]] wants the
                // EVENT visible, not just its side effect).
                crate::probe!(
                    class = "activity.purpose.refreshed",
                    room_id = %room_id,
                    purpose = %resolved,
                    "a wall change re-resolved this room's activity"
                );
            }
            // Fell behind the broadcast buffer. The index is a cache of a durable
            // wall, not guaranteed delivery — the next change re-establishes it,
            // and a stale purpose is a stale render, not a corrupt one.
            Err(RecvError::Lagged(_)) => continue,
            Err(RecvError::Closed) => break,
        }
    }
}

/// Which room a bus event says changed its wall, or `None` when the event is not
/// a wall change. Pure, so the fold's classification is testable without a bus.
fn wall_changed_room(name: &str, payload: &serde_json::Value) -> Option<Uuid> {
    if name != crate::ipc::positron_wall_source::WALL_CHANGED {
        return None;
    }
    let body = payload.get("payload").unwrap_or(payload);
    body.get("roomId")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
}

/// The [`RoomRecipeReader`] over a live airc handle: resolve the room from this
/// peer's own subscriptions, then read THAT room's wall.
///
/// Resolution goes through `subscription_set` for the same reason
/// `work/claim`'s cross-room lookup does — a `Room` carries a wire path, and
/// deriving one from a name that was never subscribed reads an empty room that
/// looks exactly like a room with no binding.
pub struct AircRecipeReader {
    pub airc: Arc<airc_lib::Airc>,
}

#[async_trait]
impl RoomRecipeReader for AircRecipeReader {
    async fn recipe_posts(&self, room_id: Uuid) -> Result<Vec<WallPostPublished>, String> {
        let set = self
            .airc
            .subscription_set()
            .await
            .map_err(|e| format!("subscription set unavailable: {e}"))?;
        let room = set
            .all()
            .map(|sub| sub.as_room())
            .find(|room| room.channel.as_uuid() == room_id)
            .ok_or_else(|| {
                format!(
                    "room {room_id} is not in this reader's subscriptions — its wall is not \
                     reachable from here"
                )
            })?;
        self.airc
            .wall_posts_in(&room, Some(RECIPE_WALL_CATEGORY))
            .await
            .map_err(|e| format!("wall read failed: {e}"))
    }

    async fn known_rooms(&self) -> Result<Vec<Uuid>, String> {
        let set = self
            .airc
            .subscription_set()
            .await
            .map_err(|e| format!("subscription set unavailable: {e}"))?;
        Ok(set
            .all()
            .map(|sub| sub.as_room().channel.as_uuid())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airc_core::{PeerId, RoomId};
    use std::sync::Mutex;

    fn post(room: Uuid, body: &str) -> WallPostPublished {
        WallPostPublished {
            room_id: RoomId::from_uuid(room),
            post_id: Uuid::nil(),
            category: RECIPE_WALL_CATEGORY.to_string(),
            body: body.to_string(),
            supersedes: None,
            published_by: PeerId::from_u128(1),
            published_at_ms: 0,
        }
    }

    /// A reader over canned per-room posts, so the fold is driven with no daemon.
    #[derive(Default)]
    struct StubReader {
        posts: Mutex<HashMap<Uuid, Vec<WallPostPublished>>>,
        fail: Mutex<bool>,
    }

    impl StubReader {
        fn bind(&self, room: Uuid, body: &str) {
            self.posts
                .lock()
                .unwrap()
                .insert(room, vec![post(room, body)]);
        }
    }

    #[async_trait]
    impl RoomRecipeReader for StubReader {
        async fn recipe_posts(&self, room_id: Uuid) -> Result<Vec<WallPostPublished>, String> {
            if *self.fail.lock().unwrap() {
                return Err("daemon down".to_string());
            }
            Ok(self
                .posts
                .lock()
                .unwrap()
                .get(&room_id)
                .cloned()
                .unwrap_or_default())
        }

        async fn known_rooms(&self) -> Result<Vec<Uuid>, String> {
            Ok(self.posts.lock().unwrap().keys().copied().collect())
        }
    }

    /// what this catches: THE defect this module exists for. `activity/spawn`
    /// wrote a benchmark binding to the wall and every reader still said "chat",
    /// so a benchmark room and a chat room were the same object to every
    /// renderer and to the citizen inside one. If this regresses, the recipe
    /// layer goes inert again with no test failing anywhere else.
    #[tokio::test]
    async fn a_bound_room_resolves_to_its_authored_activity_not_chat() {
        let room = Uuid::from_u128(0xbe);
        let reader = StubReader::default();
        reader.bind(room, r#"{"recipe":"benchmark/hard-rs"}"#);
        let purpose = RecipeRoomPurpose::new();

        assert_eq!(
            purpose.purpose_for(room),
            "chat",
            "before the fold, an unknown room is honestly chat"
        );
        purpose.refresh(room, &reader).await;
        assert_eq!(purpose.purpose_for(room), "benchmark/hard-rs");
    }

    /// what this catches: the seam losing totality. `purpose_for` is called on
    /// the projection's store path for every room the node sees; a panic or an
    /// error there would take the chat projection down over a room nobody ever
    /// bound — which is most rooms.
    #[tokio::test]
    async fn an_unbound_room_and_an_unread_room_both_resolve_to_chat() {
        let unbound = Uuid::from_u128(1);
        let unreadable = Uuid::from_u128(2);
        let broken = Uuid::from_u128(3);
        let reader = StubReader::default();
        reader.bind(unreadable, "{not json at all");
        let purpose = RecipeRoomPurpose::new();

        assert_eq!(purpose.refresh(unbound, &reader).await, "chat");
        assert_eq!(
            purpose.refresh(unreadable, &reader).await,
            "chat",
            "an unreadable binding degrades LOUDLY (probe), never fatally"
        );
        *reader.fail.lock().unwrap() = true;
        assert_eq!(
            purpose.refresh(broken, &reader).await,
            "chat",
            "a read failure resolves, it does not propagate"
        );
    }

    /// what this catches: a re-bind not taking. A room re-bound to a different
    /// activity must adopt the NEW recipe — the wall's last-wins semantics have
    /// to survive the cache, or a room would be pinned to the first activity it
    /// ever had.
    #[tokio::test]
    async fn a_rebound_room_adopts_its_new_recipe() {
        let room = Uuid::from_u128(0xa1);
        let reader = StubReader::default();
        reader.bind(room, r#"{"recipe":"chat"}"#);
        let purpose = RecipeRoomPurpose::new();
        purpose.refresh(room, &reader).await;
        assert_eq!(purpose.purpose_for(room), "chat");

        reader.bind(room, r#"{"recipe":"video-chat"}"#);
        purpose.refresh(room, &reader).await;
        assert_eq!(purpose.purpose_for(room), "video-chat");
    }

    /// what this catches: the fold's cue. The index is event-invalidated, so if
    /// `wall:changed` stopped classifying, a room's purpose would freeze at boot
    /// and a room bound after startup would never resolve — the silent-staleness
    /// failure, which looks identical to the bug this module just fixed.
    #[test]
    fn only_a_wall_change_carrying_a_room_id_cues_a_refresh() {
        let room = Uuid::from_u128(0x7);
        let payload = serde_json::json!({ "roomId": room.to_string() });
        assert_eq!(
            wall_changed_room(crate::ipc::positron_wall_source::WALL_CHANGED, &payload),
            Some(room)
        );
        // The airc bus nests event bodies under `payload` — both shapes must cue.
        assert_eq!(
            wall_changed_room(
                crate::ipc::positron_wall_source::WALL_CHANGED,
                &serde_json::json!({ "payload": payload })
            ),
            Some(room)
        );
        assert_eq!(
            wall_changed_room("chat:posted", &payload),
            None,
            "a message is not a wall change"
        );
        assert_eq!(
            wall_changed_room(
                crate::ipc::positron_wall_source::WALL_CHANGED,
                &serde_json::json!({})
            ),
            None,
            "a wall change with no room is not actionable"
        );
    }
}
