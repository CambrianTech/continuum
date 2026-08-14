//! The airc → positron **wall projection** (task #89 — the room's pinned
//! shared documents as renderer-shaped `WallViewState`).
//!
//! ## What this is
//!
//! The sibling of [`crate::ipc::positron_source`]: that consumer projects
//! the room's *conversation* (`kind="chat"`); this one projects the room's
//! *pinned board* — the plan, the coding instructions, the agenda, the
//! principles, the recipe (`kind="wall"`). Both are passive consumers of
//! the airc room stream on the `MessageBus`, off the transport hot path,
//! writing a renderer-shaped snapshot to the thin-client [`Substrate`]
//! that WS sessions read.
//!
//! ## Why this projector RE-READS instead of folding deltas
//!
//! The wall is event-sourced with a **supersede chain**: a revision
//! publishes a new `WallPostPublished` pointing at the prior, an unpin
//! archives with an empty body, and the *currently-pinned* board is the
//! projection of that whole chain. That supersede walk is **airc-owned**
//! (`Airc::wall_posts` — private projection internals in airc-lib). So,
//! unlike the chat projection (which folds each `chat:posted` into a ring
//! it owns), this projector does NOT re-implement the supersede fold — it
//! **re-reads** airc's authoritative `wall_posts()` through the existing
//! [`WallReader`] seam on each wall change. One supersede impl, in airc
//! ([[compression]]); the continuum side is a cache of airc's truth, never
//! a second store of it ([[airc-native-identity-rooms-security]]).
//!
//! ## The two bus streams it folds
//!
//! - **`wall:changed`** — a `WallPostPublished` transcript event landed
//!   for this room (emitted by
//!   [`crate::airc::inbound_attach::publish_transcript_event`] when it
//!   sees `TranscriptKind::WallPostPublished`). The projector RE-READS the
//!   board via `wall_posts()` and re-projects. The signal carries only the
//!   `room_id`; the post content comes from the authoritative re-read,
//!   never from the signal (the supersede projection can't be reconstructed
//!   from one delta).
//! - **`presence:updated`** — the room roster changed. The wall carries
//!   only a `published_by` peer id, never an author name (identity is a
//!   presence fact, not a content fact — the exact discipline
//!   `AircChatPosted` follows). So the projector holds the roster as its
//!   **author-resolution lookup table** and re-projects when it changes, so
//!   a post pinned before its author's card arrived UPGRADES from a
//!   provisional peer-id label to the real name in place. This reuses the
//!   SAME `presence:updated` stream and the SAME identity resolver the chat
//!   projection uses ([[compression]]).
//!
//! ## Attribution is woven in from day one
//!
//! A pinned document is an authored, accountable act — per
//! `[[positron-identity-security-first-class]]` each projected post carries
//! the author's neutral kind + opaque badges + accountability provenance,
//! resolved from the roster by [`resolve_identity`] exactly as a chat
//! sender is. positron stays neutral (an AI author is an `Agent`; whose
//! agent rides `integrations`), read at the app layer.
//!
//! ## Single focused room
//!
//! The projector is bound to ONE room at construction — the node's
//! bootstrap room, the same room its [`WallReader`] handle joined and the
//! same room the presence emitter serves. A `wall:changed` /
//! `presence:updated` for any other room is ignored (a defensive room
//! guard; today the node observes exactly this room). Per-room instancing
//! is the same deferred note the chat projection carries: the revision key
//! would extend from the bare kind string to a `(room_id, kind)` tuple
//! (see `continuum-positron/src/revisions.rs`).

use std::sync::Arc;

use airc_core::doctrine::WallPostPublished;
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use continuum_positron::{RosterSlotView, StateBuilder, Substrate, WallPostView, WallViewState};
use serde::Deserialize;

use crate::ipc::positron_source::{resolve_identity, AircPresenceUpdate, PRESENCE_UPDATED};
use crate::persona::wall_source::WallReader;
use crate::runtime::{BusEvent, MessageBus};

/// Bus event signalling that a `WallPostPublished` transcript event landed
/// for a room — the projector's cue to RE-READ the authoritative board.
///
/// `pub(crate)` because the EMITTER
/// (`airc::inbound_attach::publish_transcript_event`) and this CONSUMER
/// must agree on the wire name — one string, one source of truth
/// ([[compression]]), exactly as
/// [`crate::ipc::positron_source::CHAT_POSTED`] is shared.
pub(crate) const WALL_CHANGED: &str = "wall:changed";

/// Typed `wall:changed` payload. Deliberately carries ONLY the `room_id`:
/// the post content is never trusted from the signal (the supersede
/// projection can't be reconstructed from a single delta), so the
/// projector re-reads `wall_posts()` for the authoritative board. camelCase
/// matches the bus JSON convention.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AircWallChanged {
    room_id: Uuid,
}

/// Accumulates the room's pinned board into the renderer-shaped
/// [`WallViewState`] and writes each transition to the [`Substrate`].
///
/// Holds the last authoritative `posts` read (airc-owned supersede
/// projection) plus the roster it resolves authors against. Not `Clone` —
/// one owner per projection; the consume loop owns it.
struct WallProjection {
    substrate: Substrate,
    builder: StateBuilder,
    /// The single room this projector describes (the node's bootstrap
    /// room). Fixed at construction — the `WallReader` handle is joined to
    /// exactly this room.
    room_id: Uuid,
    /// Last authoritative board read from `wall_posts()` — the airc-owned
    /// supersede projection, never a continuum-folded copy.
    posts: Vec<WallPostPublished>,
    /// Author-resolution lookup table (from `presence:updated`). NOT stored
    /// on the view — the wall view carries resolved authors, not the whole
    /// roster (a wall widget renders documents, not a member list).
    roster: Vec<RosterSlotView>,
    reader: Arc<dyn WallReader>,
}

impl WallProjection {
    fn new(substrate: Substrate, room_id: Uuid, reader: Arc<dyn WallReader>) -> Self {
        Self {
            substrate,
            // Sole writer of the `wall` kind → its own standalone
            // `Revisions` well is the authoritative monotonic source for
            // that kind (same discipline as the chat projection).
            builder: StateBuilder::standalone(),
            room_id,
            posts: Vec::new(),
            roster: Vec::new(),
            reader,
        }
    }

    /// Re-read the authoritative board via the [`WallReader`] and store the
    /// re-projected view. A read error keeps the last-good board on the
    /// widget rather than blinking it empty — the reader (`airc_lib::Airc`)
    /// owns reconnection ([[persona-airc-resilience]]); a transient failure
    /// must not fabricate an empty board ([[fallbacks-are-illegal-fail-loud]]:
    /// resilience, never a fabricated substitute).
    async fn reload(&mut self) {
        match self.reader.wall_posts().await {
            Ok(posts) => {
                self.posts = posts;
                self.store();
            }
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    room_id = %self.room_id,
                    "positron_wall: wall_posts read failed — keeping last board (reader owns reconnection)"
                );
            }
        }
    }

    /// Replace the author-resolution roster and re-project, so any post
    /// pinned before its author's card arrived upgrades from a provisional
    /// label to the resolved name in place.
    fn apply_roster(&mut self, roster: Vec<RosterSlotView>) {
        self.roster = roster;
        self.store();
    }

    /// Project one airc `WallPostPublished` into a renderer-shaped
    /// [`WallPostView`], resolving the author from the current roster.
    fn project_post(&self, post: &WallPostPublished) -> WallPostView {
        let author_id = post.published_by.as_uuid();
        let resolved = resolve_identity(&self.roster, author_id);
        WallPostView {
            post_id: post.post_id,
            room_id: post.room_id.as_uuid(),
            category: post.category.clone(),
            author_id,
            author_name: resolved.name,
            author_kind: resolved.kind,
            integrations: resolved.integrations,
            provenance: resolved.provenance,
            body: post.body.clone(),
            timestamp: post.published_at_ms,
        }
    }

    /// Frame the current board as a `wall` `StateEnvelope` and write it to
    /// the substrate (cache + live broadcast). Persistent-tier: the wall is
    /// long-lived pinned state (< 1 Hz), not the user-perceivable chat
    /// cadence.
    fn store(&self) {
        let posts = self.posts.iter().map(|p| self.project_post(p)).collect();
        let view = WallViewState {
            room_id: self.room_id,
            posts,
        };
        self.substrate.store(self.builder.persistent(view));
    }
}

/// A bus event classified into a wall projection input, or `None` when the
/// event is not one this projection folds. Pure — no substrate side effect,
/// no reader I/O — so it's unit-testable without a live bus or daemon.
enum WallInput {
    /// A wall change landed for this `room_id` — cue to re-read the board.
    Changed(Uuid),
    /// The room roster changed (for this `room_id`) — new author lookup.
    Presence(Uuid, Vec<RosterSlotView>),
}

fn classify(name: &str, payload: &serde_json::Value) -> Option<WallInput> {
    // The airc bus wraps event bodies under a `payload` key (see
    // `positron_source::classify`); accept a nested `payload` object, else
    // the top-level value — one unwrap convention across the projections.
    let body = payload.get("payload").unwrap_or(payload);
    match name {
        WALL_CHANGED => serde_json::from_value::<AircWallChanged>(body.clone())
            .ok()
            .map(|c| WallInput::Changed(c.room_id)),
        PRESENCE_UPDATED => serde_json::from_value::<AircPresenceUpdate>(body.clone())
            .ok()
            .map(|u| WallInput::Presence(u.room_id, u.roster)),
        _ => None,
    }
}

/// Run the wall projection consume loop against an already-attached
/// [`WallReader`]. Subscribes to the bus, does an initial authoritative
/// read (so the board renders at boot without waiting for a change), then
/// folds `wall:changed` / `presence:updated` for its room.
///
/// The receiver is taken BEFORE the initial read so a change racing the
/// boot read is still caught by the loop (a redundant re-read is idempotent
/// — it just re-reads the same authoritative board). Runs for the process
/// lifetime.
async fn run_wall_loop(
    substrate: Substrate,
    room_id: Uuid,
    reader: Arc<dyn WallReader>,
    bus: Arc<MessageBus>,
) {
    let mut rx = bus.receiver();
    // Demand the current roster now (#118): `reload()` re-reads the board
    // authoritatively, but the roster it renders authors against rides the
    // fire-once `presence:updated` stream. Without this cue a wall projector
    // that (re)started after the emitter's last publish would label every
    // post provisionally until presence next changes. `rx` is subscribed
    // above, so the emitter's re-publish lands in our buffer.
    crate::ipc::positron_presence::request_presence_resync(&bus);
    let mut projection = WallProjection::new(substrate, room_id, reader);
    // Initial authoritative read — render the current board immediately.
    projection.reload().await;
    while let LoopStep::Continue = fold_recv(&mut projection, room_id, rx.recv().await).await {}
}

/// Whether the consume loop keeps folding after one bus receive.
#[derive(Debug, PartialEq, Eq)]
enum LoopStep {
    Continue,
    Stop,
}

/// Fold ONE bus receive into the projection — the loop's entire per-event
/// decision, factored out so the room-guard / `Lagged` / `Closed` branches
/// are unit-testable without racing a live broadcast channel (the loop
/// itself is an infinite `recv().await`, so the branches would otherwise be
/// trust-me). Pure control-flow over the projection; no bus access.
async fn fold_recv(
    projection: &mut WallProjection,
    room_id: Uuid,
    recv: Result<BusEvent, RecvError>,
) -> LoopStep {
    match recv {
        Ok(event) => {
            match classify(&event.name, &event.payload) {
                // Re-read only when the change is for OUR room (defensive
                // room guard; the node observes exactly this room today).
                Some(WallInput::Changed(rid)) if rid == room_id => projection.reload().await,
                Some(WallInput::Presence(rid, roster)) if rid == room_id => {
                    projection.apply_roster(roster)
                }
                _ => {}
            }
            LoopStep::Continue
        }
        // Fell behind the broadcast buffer: the projection is a last-good
        // cache of a live stream, not guaranteed delivery. Re-read to
        // re-establish a coherent board, then keep folding.
        Err(RecvError::Lagged(_)) => {
            projection.reload().await;
            LoopStep::Continue
        }
        Err(RecvError::Closed) => LoopStep::Stop,
    }
}

/// Fixed identity name for the node-level wall reader. Like the presence
/// reader, it attaches as a **heartbeat-less lurker** (reads the daemon's
/// authoritative transcript without ever appearing in the roster) — a
/// distinct name from the presence reader so the two node lurkers hold
/// independent keypairs and neither's lifecycle blinks the other.
const NODE_WALL_READER_NAME: &str = "continuum-wall";

/// Attach a node-level wall reader and run the wall projection for one
/// room. The **consuming half** of the `wall:changed` stream: it subscribes
/// to the bus (fed by `inbound_attach`) and re-reads the airc-owned board
/// on each change.
///
/// Mirrors [`crate::ipc::positron_presence::spawn_node_presence_emitter`]:
/// a dedicated node reader (not a persona's handle) so the wall renders
/// with **zero resident personas** — a chat window with no persona present
/// still shows its pinned board. A failed attach/join means the projection
/// can't start; it logs the cause loudly and the task exits (the WS server
/// is optional → a disabled feature, not a substrate-wide panic;
/// [[fallbacks-are-illegal-fail-loud]]: no fabricated board is ever
/// substituted).
pub fn spawn_node_wall_projector(
    rt: &tokio::runtime::Handle,
    daemon_socket: std::path::PathBuf,
    node_home: std::path::PathBuf,
    room_id: Uuid,
    room_name: String,
    substrate: Substrate,
    bus: Arc<MessageBus>,
) {
    rt.spawn(async move {
        if let Err(err) = tokio::fs::create_dir_all(&node_home).await {
            tracing::error!(
                error = %err,
                home = %node_home.display(),
                "positron_wall: cannot create node reader home — wall projection disabled"
            );
            return;
        }
        let airc = match airc_lib::Airc::attach_as(
            node_home.clone(),
            NODE_WALL_READER_NAME,
            daemon_socket,
        )
        .await
        {
            Ok(airc) => airc,
            Err(err) => {
                tracing::error!(
                    error = %err,
                    home = %node_home.display(),
                    "positron_wall: node reader attach failed — wall projection disabled"
                );
                return;
            }
        };
        // Join by NAME (never UUID-as-string, which derives a DIFFERENT
        // channel — the recurring hazard `positron_presence` documents).
        // The reader must share the operator's channel or its wall reads
        // land in an empty derived room.
        if let Err(err) = airc.join(&room_name).await {
            tracing::error!(
                error = %err,
                room = %room_name,
                "positron_wall: node reader could not join room — wall projection disabled"
            );
            return;
        }
        // `airc_lib::Airc` satisfies `WallReader` directly (impl in
        // `persona::wall_source`) — reuse it, no adapter.
        let reader: Arc<dyn WallReader> = Arc::new(airc);
        tracing::info!(
            %room_id,
            room = %room_name,
            "positron_wall: node reader attached — projecting kind=wall"
        );
        run_wall_loop(substrate, room_id, reader, bus).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::positron_source::{test_presence_payload, test_roster_slot};
    use airc_core::{PeerId, RoomId};
    use async_trait::async_trait;
    use continuum_positron::{Provenance, SenderKind};
    use serde_json::json;
    use std::sync::Mutex;

    /// A `WallReader` stub returning canned posts — lets the projection be
    /// driven without a daemon (mirrors `wall_source::tests::StubReader`).
    struct StubReader {
        posts: Mutex<Vec<WallPostPublished>>,
        fail: Mutex<bool>,
    }

    impl StubReader {
        fn new(posts: Vec<WallPostPublished>) -> Arc<Self> {
            Arc::new(Self {
                posts: Mutex::new(posts),
                fail: Mutex::new(false),
            })
        }
        /// Flip the reader into failure mode — the next `wall_posts()`
        /// returns a terminal-shaped `AircError` (mirrors
        /// `wall_source::tests::StubReader::set_fail`).
        fn set_fail(&self, fail: bool) {
            *self.fail.lock().unwrap() = fail;
        }
    }

    #[async_trait]
    impl WallReader for StubReader {
        async fn wall_posts(&self) -> Result<Vec<WallPostPublished>, airc_lib::AircError> {
            if *self.fail.lock().unwrap() {
                return Err(airc_lib::AircError::UnknownPeer(PeerId::new()));
            }
            Ok(self.posts.lock().unwrap().clone())
        }
    }

    fn post(room: RoomId, author: PeerId, category: &str, body: &str) -> WallPostPublished {
        WallPostPublished {
            room_id: room,
            post_id: Uuid::new_v4(),
            category: category.to_string(),
            body: body.to_string(),
            supersedes: None,
            published_by: author,
            published_at_ms: 1_700_000_000_000,
        }
    }

    fn presence_one(room: Uuid, member: Uuid, name: &str, kind: &str) -> serde_json::Value {
        // Serialize the REAL typed slot via the shared helper — never a
        // hand-authored JSON literal, so the test wire can't drift from
        // `RosterSlotView`'s field names.
        let kind: SenderKind = serde_json::from_value(json!({ "kind": kind })).unwrap();
        let mut integrations = std::collections::BTreeMap::new();
        integrations.insert("continuum.persona_id".to_string(), "asha-1".to_string());
        test_presence_payload(
            room,
            vec![RosterSlotView {
                integrations,
                provenance: Provenance {
                    runtime: "claude".to_string(),
                },
                ..test_roster_slot(member, name, kind)
            }],
        )
    }

    fn current_wall(substrate: &Substrate) -> WallViewState {
        let env = substrate
            .cache()
            .get(WallViewState::KIND)
            .expect("a wall envelope must be stored");
        serde_json::from_value(env.payload.clone()).expect("payload is a WallViewState")
    }

    #[tokio::test]
    async fn reload_projects_the_board_into_the_substrate() {
        // what this catches: regression where a wall re-read does not reach
        // the substrate as a WallViewState — the whole point of the wall
        // projection. With no presence yet, the author renders provisionally
        // (short peer-id label, neutral Human) — the honest pending-truth
        // state, never a fabricated author.
        let substrate = Substrate::new();
        let room = RoomId::new();
        let author = PeerId::new();
        let reader = StubReader::new(vec![post(room, author, "plan", "Ship the wall slice.")]);
        let mut p = WallProjection::new(substrate.clone(), room.as_uuid(), reader);
        p.reload().await;

        let view = current_wall(&substrate);
        assert_eq!(view.room_id, room.as_uuid());
        assert_eq!(view.posts.len(), 1);
        assert_eq!(view.posts[0].category, "plan");
        assert_eq!(view.posts[0].body, "Ship the wall slice.");
        assert_eq!(view.posts[0].author_id, author.as_uuid());
        assert_eq!(view.posts[0].author_kind, SenderKind::Human);
        assert!(
            view.posts[0].author_name.starts_with("peer-"),
            "provisional author label until presence, got {}",
            view.posts[0].author_name
        );
        assert!(view.posts[0].integrations.is_empty());
    }

    #[tokio::test]
    async fn author_upgrades_in_place_when_presence_folds_the_card() {
        // what this catches: the attribution-woven-in contract — a post
        // pinned before its author's card arrives renders provisionally,
        // then UPGRADES (name + kind + badges + provenance) the instant
        // presence folds the card. A regression that dropped the re-project
        // on presence would leave the author stuck as "peer-xxxx"/Human
        // forever, unattributable. [[positron-identity-security-first-class]].
        let substrate = Substrate::new();
        let room = RoomId::new();
        let author = PeerId::new();
        let reader = StubReader::new(vec![post(room, author, "rules", "Fail loud.")]);
        let mut p = WallProjection::new(substrate.clone(), room.as_uuid(), reader);
        p.reload().await;
        assert_eq!(
            current_wall(&substrate).posts[0].author_kind,
            SenderKind::Human
        );

        // Card arrives via presence: Agent named Asha carrying a badge.
        let presence = presence_one(room.as_uuid(), author.as_uuid(), "Asha", "agent");
        match classify(PRESENCE_UPDATED, &presence).expect("presence classifies") {
            WallInput::Presence(rid, roster) => {
                assert_eq!(rid, room.as_uuid());
                p.apply_roster(roster);
            }
            _ => panic!("presence:updated must classify as Presence"),
        }
        let view = current_wall(&substrate);
        assert_eq!(view.posts[0].author_name, "Asha");
        assert_eq!(view.posts[0].author_kind, SenderKind::Agent);
        assert_eq!(
            view.posts[0]
                .integrations
                .get("continuum.persona_id")
                .map(String::as_str),
            Some("asha-1"),
            "opaque badge resolved from the card"
        );
        assert_eq!(
            view.posts[0].provenance.runtime, "claude",
            "accountability provenance resolved from the card"
        );
    }

    #[tokio::test]
    async fn empty_board_is_projected_not_an_error() {
        // what this catches: a room with nothing pinned projects an empty
        // board (not a skipped store, not an error) — the honest-empty
        // contract the WallViewState type documents. A renderer subscribed
        // to kind=wall must still receive a snapshot so it can clear a
        // stale board.
        let substrate = Substrate::new();
        let room = RoomId::new();
        let reader = StubReader::new(vec![]);
        let mut p = WallProjection::new(substrate.clone(), room.as_uuid(), reader);
        p.reload().await;
        let view = current_wall(&substrate);
        assert_eq!(view.room_id, room.as_uuid());
        assert!(view.posts.is_empty());
    }

    #[test]
    fn foreign_room_and_malformed_events_are_skipped() {
        // what this catches: regression where a non-wall event, or a
        // wall:changed missing the room_id, gets classified into a spurious
        // re-read. Per [[fallbacks-are-illegal-fail-loud]]: an event that
        // can't deserialize into the contract is not-a-wall-event → None.
        // (Room-mismatch filtering is enforced in the loop, tested via the
        // room_id the Changed/Presence inputs carry.)
        assert!(classify("media:frame", &json!({ "bytes": 4 })).is_none());
        assert!(classify(WALL_CHANGED, &json!({ "nope": true })).is_none());
        let room = Uuid::from_u128(0xa);
        match classify(WALL_CHANGED, &json!({ "roomId": room })).expect("classifies") {
            WallInput::Changed(rid) => assert_eq!(rid, room),
            _ => panic!("wall:changed must classify as Changed"),
        }
    }

    #[tokio::test]
    async fn revision_advances_monotonically_across_stores() {
        // what this catches: regression where successive wall projections
        // stamp a stale or non-monotonic revision — the session-protocol
        // last_seen replay routes by revision, so a flat revision would stop
        // the wall widget from seeing updates.
        let substrate = Substrate::new();
        let room = RoomId::new();
        let author = PeerId::new();
        let reader = StubReader::new(vec![post(room, author, "plan", "v1")]);
        let mut p = WallProjection::new(substrate.clone(), room.as_uuid(), reader);
        p.reload().await;
        let r1 = substrate.cache().get(WallViewState::KIND).unwrap().revision;
        // A presence fold re-projects → a second store, revision advances.
        let presence = presence_one(room.as_uuid(), author.as_uuid(), "Asha", "agent");
        if let WallInput::Presence(_, roster) = classify(PRESENCE_UPDATED, &presence).unwrap() {
            p.apply_roster(roster);
        }
        let r2 = substrate.cache().get(WallViewState::KIND).unwrap().revision;
        assert!(r2 > r1, "revision must advance: {r1:?} -> {r2:?}");
    }

    #[tokio::test]
    async fn read_error_keeps_the_last_good_board() {
        // what this catches: the projector's DISTINCT resilience contract —
        // a `wall_posts()` read failure must keep the last-good board on the
        // widget, never blink it empty. This is the difference from the
        // RagSource path (which reads fresh each turn); here a stale board is
        // the honest render while the reader reconnects. A regression that
        // cleared `posts` on error, or stored an empty view, would flash the
        // board blank on every transient airc hiccup
        // ([[fallbacks-are-illegal-fail-loud]]: resilience, never a
        // fabricated empty substitute).
        let substrate = Substrate::new();
        let room = RoomId::new();
        let author = PeerId::new();
        let reader = StubReader::new(vec![post(room, author, "plan", "v1")]);
        let mut p = WallProjection::new(substrate.clone(), room.as_uuid(), reader.clone());
        p.reload().await;
        assert_eq!(current_wall(&substrate).posts.len(), 1);

        // The reader goes down mid-stream; the re-read fails.
        reader.set_fail(true);
        p.reload().await;

        let view = current_wall(&substrate);
        assert_eq!(view.posts.len(), 1, "last-good board survives a read error");
        assert_eq!(view.posts[0].body, "v1");
    }

    #[tokio::test]
    async fn lagged_re_reads_to_re_establish_coherence() {
        // what this catches: the projector's one behavioral improvement over
        // the chat projection — on a broadcast `Lagged` (fell behind the
        // buffer) it RE-READS the authoritative board rather than `continue`-
        // ing on a possibly-stale one. A refactor that flipped this back to a
        // silent continue would leave the board stale after any burst. Drives
        // the exact loop decision via `fold_recv` (no racing a live channel).
        let substrate = Substrate::new();
        let room = RoomId::new();
        let author = PeerId::new();
        let reader = StubReader::new(vec![]);
        let mut p = WallProjection::new(substrate.clone(), room.as_uuid(), reader.clone());
        p.reload().await;
        assert!(current_wall(&substrate).posts.is_empty());

        // Board gains a post while the loop was behind; a Lagged arrives.
        reader
            .posts
            .lock()
            .unwrap()
            .push(post(room, author, "plan", "caught up"));
        let step = fold_recv(&mut p, room.as_uuid(), Err(RecvError::Lagged(3))).await;
        assert_eq!(step, LoopStep::Continue);
        assert_eq!(
            current_wall(&substrate).posts.len(),
            1,
            "Lagged must trigger an authoritative re-read, not a stale continue"
        );
    }

    #[tokio::test]
    async fn closed_channel_stops_the_loop_and_foreign_room_is_not_re_read() {
        // what this catches: two loop-guard branches that are otherwise
        // trust-me (the loop is an infinite recv). (1) A `Closed` bus stops
        // the loop (Stop) — no spin. (2) A `wall:changed` for a DIFFERENT
        // room is NOT re-read: the defensive room guard drops it, so a foreign
        // room's churn can't thrash this projector's board.
        let substrate = Substrate::new();
        let room = RoomId::new();
        let author = PeerId::new();
        let reader = StubReader::new(vec![post(room, author, "plan", "mine")]);
        let mut p = WallProjection::new(substrate.clone(), room.as_uuid(), reader.clone());
        p.reload().await;

        // A change for some OTHER room: the board must not re-read/change.
        let other = Uuid::from_u128(0xdead);
        let foreign = BusEvent {
            name: WALL_CHANGED.to_string(),
            payload: json!({ "roomId": other }),
        };
        // Even if the board content changed underneath, a foreign event must
        // not fold it in.
        reader.posts.lock().unwrap().clear();
        let step = fold_recv(&mut p, room.as_uuid(), Ok(foreign)).await;
        assert_eq!(step, LoopStep::Continue);
        assert_eq!(
            current_wall(&substrate).posts.len(),
            1,
            "a foreign room's wall:changed must not re-read our board"
        );

        // A closed bus stops the loop.
        let step = fold_recv(&mut p, room.as_uuid(), Err(RecvError::Closed)).await;
        assert_eq!(step, LoopStep::Stop);
    }
}
