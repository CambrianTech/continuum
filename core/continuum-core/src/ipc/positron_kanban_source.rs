//! The airc → positron **kanban projection** (task #89 — the room's work
//! board as renderer-shaped `KanbanViewState`).
//!
//! ## What this is
//!
//! The third face of a room, sibling to [`crate::ipc::positron_source`]
//! (the room's *conversation*, `kind="chat"`) and
//! [`crate::ipc::positron_wall_source`] (the room's *pinned documents*,
//! `kind="wall"`). This one projects the room's *work*: the cards agents
//! file, claim, move through states, and land as PRs — grouped into lanes
//! (`kind="kanban"`). Like its siblings it's a passive consumer of the
//! airc room stream on the `MessageBus`, off the transport hot path,
//! writing a renderer-shaped snapshot to the thin-client [`Substrate`]
//! that WS sessions read AND the O6 persona observer grounds on.
//!
//! ## Why this projector RE-READS instead of folding deltas
//!
//! The work board is event-sourced: `CardCreated`, `CardStateChanged`,
//! `WorkCardClaimed`, `LaneCreated`, … folded by airc's
//! `WorkBoardProjection` into a current board. That fold is **airc-owned**
//! (`Airc::work_board_complete` → `.snapshot()` → `BoardSnapshot` —
//! projection internals private to `airc-work`). So, exactly like the wall
//! projector's supersede walk, this projector does NOT re-implement the
//! event fold — it **re-reads** airc's authoritative board through the
//! [`WorkBoardReader`] seam on each work change. One board fold, in airc
//! ([[compression]]); the continuum side is a cache of airc's truth, never
//! a second store of it ([[airc-native-identity-rooms-security]]).
//!
//! ## The airc→positron enum mapping lives HERE, not in positron
//!
//! `continuum-positron` is a neutral, standalone contract library that
//! MUST NOT depend on `airc-work`, so its `KanbanCardState` /
//! `KanbanLaneState` / `KanbanPriority` MIRROR airc's `CardState` /
//! `LaneState` / `Priority` variant-for-variant. This projector — the ONE
//! crate that depends on both — maps airc → positron **by value** at the
//! seam ([`map_card_state`] / [`map_lane_state`] / [`map_priority`]). The
//! maps are exhaustive: airc adding a variant is a compile error here (the
//! signal to extend the mirror), never a silent `Other`
//! ([[fallbacks-are-illegal-fail-loud]]).
//!
//! ## The two bus streams it folds
//!
//! - **`kanban:changed`** — a work-domain transcript event landed for this
//!   room (emitted by
//!   [`crate::airc::inbound_attach::publish_transcript_event`] when it sees
//!   a `TranscriptKind::System` transcript carrying the
//!   `HEADER_FORGE_WORK_EVENT_KIND` header — work events are System
//!   transcripts distinguished by that header, unlike the wall's dedicated
//!   `WallPostPublished` kind). The projector RE-READS the board via
//!   `work_board_complete()` and re-projects. The signal carries only the
//!   `room_id`; the board content comes from the authoritative re-read,
//!   never from the delta (the board fold can't be reconstructed from one
//!   event).
//! - **`presence:updated`** — the room roster changed. A card carries only
//!   its `created_by` / `owner` peer ids, never an author name (identity is
//!   a presence fact, not a content fact — the exact discipline the chat
//!   and wall projections follow). So the projector holds the roster as its
//!   **identity-resolution lookup table** and re-projects when it changes,
//!   so a card filed before its author's card arrived UPGRADES from a
//!   provisional peer-id label to the real name in place. Reuses the SAME
//!   `presence:updated` stream and the SAME [`resolve_identity`] resolver
//!   the chat and wall projections use ([[compression]]).
//!
//! ## Attribution is woven in from day one
//!
//! Filing a card is an authored, accountable act; claiming one is a second
//! accountable act. Per `[[positron-identity-security-first-class]]` the
//! projected card's CREATOR carries the author's neutral kind + opaque
//! badges + accountability provenance, resolved from the roster by
//! [`resolve_identity`] exactly as a chat sender / wall author is. The
//! current ASSIGNEE (claimant) is a lighter status pointer — a resolved id
//! + display name — since it annotates *state* ("claimed by X") rather than
//! an authored document. positron stays neutral (an AI creator is an
//! `Agent`; whose agent rides `integrations`), read at the app layer.
//!
//! ## Single focused room
//!
//! Bound to ONE room at construction — the node's bootstrap room, the same
//! room its [`WorkBoardReader`] handle joined and the same room the
//! presence emitter serves. A `kanban:changed` / `presence:updated` for any
//! other room is ignored (a defensive room guard; the node observes exactly
//! this room today). Per-room instancing is the same deferred `RevisionKey`
//! note the chat/wall projections carry.

use std::sync::Arc;

use airc_work::{BoardSnapshot, CardState, LaneRecord, LaneState, Priority, WorkCard};
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use continuum_positron::{
    KanbanCardState, KanbanCardView, KanbanHold, KanbanLaneState, KanbanLaneView, KanbanPriority,
    KanbanPullRequest, KanbanViewState, RosterSlotView, StateBuilder, Substrate,
};
use serde::Deserialize;

use crate::ipc::positron_source::{resolve_identity, AircPresenceUpdate, PRESENCE_UPDATED};
use crate::runtime::{BusEvent, MessageBus};

/// Abstract reader over the airc room work board. Production rides on
/// `airc_lib::Airc::work_board_complete`; tests stub it without a daemon.
/// Mirrors the [`crate::persona::wall_source::WallReader`] rail.
///
/// Returns the WHOLE board ([`BoardSnapshot`]) — `work_board_complete`, not
/// the recent-window `work_board`, so an old active card doesn't drop off
/// the projected board just because chat/status traffic pushed its creation
/// event outside a recent transcript window (airc's own scheduling/mutation
/// paths read the complete board for the same reason).
#[async_trait::async_trait]
pub trait WorkBoardReader: Send + Sync {
    /// The current work board for this reader's room, folded by airc.
    async fn work_board(&self) -> Result<BoardSnapshot, airc_lib::AircError>;
}

/// `airc_lib::Airc` satisfies the reader contract directly. Orphan rule OK
/// — the trait is ours. Reads the complete board and snapshots it into the
/// plain [`BoardSnapshot`] the projector maps.
#[async_trait::async_trait]
impl WorkBoardReader for airc_lib::Airc {
    async fn work_board(&self) -> Result<BoardSnapshot, airc_lib::AircError> {
        let projection =
            airc_lib::Airc::work_board_complete(self, airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE)
                .await?;
        Ok(projection.snapshot())
    }
}

/// Bus event signalling that a work-domain transcript event landed for a
/// room — the projector's cue to RE-READ the authoritative board.
///
/// `pub(crate)` because the EMITTER
/// (`airc::inbound_attach::publish_transcript_event`) and this CONSUMER
/// must agree on the wire name — one string, one source of truth
/// ([[compression]]), exactly as [`super::positron_wall_source::WALL_CHANGED`]
/// is shared.
pub(crate) const KANBAN_CHANGED: &str = "kanban:changed";

/// Typed `kanban:changed` payload. Deliberately carries ONLY the `room_id`:
/// the board content is never trusted from the signal (the board fold can't
/// be reconstructed from a single delta), so the projector re-reads
/// `work_board()` for the authoritative board. camelCase matches the bus
/// JSON convention (same as [`super::positron_wall_source`]'s change event).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AircKanbanChanged {
    room_id: Uuid,
}

/// Map airc's `CardState` → positron's mirrored `KanbanCardState` by value.
/// Exhaustive: airc adding a state is a compile error here, the signal to
/// extend the mirror — never a silent default ([[fallbacks-are-illegal-fail-loud]]).
fn map_card_state(state: CardState) -> KanbanCardState {
    match state {
        CardState::Open => KanbanCardState::Open,
        CardState::Claimed => KanbanCardState::Claimed,
        CardState::InProgress => KanbanCardState::InProgress,
        CardState::Blocked => KanbanCardState::Blocked,
        CardState::Review => KanbanCardState::Review,
        CardState::Merged => KanbanCardState::Merged,
        CardState::Closed => KanbanCardState::Closed,
    }
}

/// Map airc's `LaneState` → positron's mirrored `KanbanLaneState` by value.
/// Exhaustive for the same reason as [`map_card_state`].
fn map_lane_state(state: LaneState) -> KanbanLaneState {
    match state {
        LaneState::Planned => KanbanLaneState::Planned,
        LaneState::Active => KanbanLaneState::Active,
        LaneState::Blocked => KanbanLaneState::Blocked,
        LaneState::Landing => KanbanLaneState::Landing,
        LaneState::Done => KanbanLaneState::Done,
    }
}

/// Map the SHARED holder projection's lease verdict → positron's mirrored
/// [`KanbanHold`].
///
/// The verdict itself is NOT computed here: it comes from
/// [`crate::persona::card_holder::hold_of`], the same call the persona's board
/// line and `work/list` make. That is the point — the human's card and the
/// citizen's board line cannot disagree about whether a claim is still good,
/// because there is one predicate and this is only its projection.
/// Exhaustive for the same reason as [`map_card_state`].
fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn map_hold(hold: crate::persona::card_holder::Hold) -> KanbanHold {
    use crate::persona::card_holder::Hold;
    match hold {
        Hold::Held => KanbanHold::Held,
        Hold::Lapsed => KanbanHold::Lapsed,
        Hold::Unclaimed => KanbanHold::Unclaimed,
    }
}

/// Map airc's `Priority` → positron's mirrored `KanbanPriority` by value.
/// Exhaustive for the same reason as [`map_card_state`].
fn map_priority(priority: Priority) -> KanbanPriority {
    match priority {
        Priority::P0 => KanbanPriority::P0,
        Priority::P1 => KanbanPriority::P1,
        Priority::P2 => KanbanPriority::P2,
        Priority::P3 => KanbanPriority::P3,
    }
}

/// Accumulates the room's work board into the renderer-shaped
/// [`KanbanViewState`] and writes each transition to the [`Substrate`].
///
/// Holds the last authoritative board read (airc-owned fold) plus the
/// roster it resolves creators/assignees against. Not `Clone` — one owner
/// per projection; the consume loop owns it.
struct KanbanProjection {
    substrate: Substrate,
    builder: StateBuilder,
    /// The single room this projector describes (the node's bootstrap
    /// room). Fixed at construction — the `WorkBoardReader` handle is joined
    /// to exactly this room.
    room_id: Uuid,
    /// Last authoritative cards read from the board fold — never a
    /// continuum-folded copy.
    cards: Vec<WorkCard>,
    /// Last authoritative lanes read from the board fold.
    lanes: Vec<LaneRecord>,
    /// Identity-resolution lookup table (from `presence:updated`). NOT
    /// stored on the view — the kanban view carries resolved creators /
    /// assignees, not the whole roster (a board renders cards, not a member
    /// list).
    roster: Vec<RosterSlotView>,
    reader: Arc<dyn WorkBoardReader>,
}

impl KanbanProjection {
    fn new(substrate: Substrate, room_id: Uuid, reader: Arc<dyn WorkBoardReader>) -> Self {
        Self {
            substrate,
            // Sole writer of the `kanban` kind → its own standalone
            // `Revisions` well is the authoritative monotonic source for
            // that kind (same discipline as the chat / wall projections).
            builder: StateBuilder::standalone(),
            room_id,
            cards: Vec::new(),
            lanes: Vec::new(),
            roster: Vec::new(),
            reader,
        }
    }

    /// Re-read the authoritative board via the [`WorkBoardReader`] and store
    /// the re-projected view. A read error keeps the last-good board on the
    /// widget rather than blinking it empty — the reader (`airc_lib::Airc`)
    /// owns reconnection ([[persona-airc-resilience]]); a transient failure
    /// must not fabricate an empty board ([[fallbacks-are-illegal-fail-loud]]:
    /// resilience, never a fabricated substitute — the same distinction the
    /// wall projector documents).
    async fn reload(&mut self) {
        match self.reader.work_board().await {
            Ok(board) => {
                self.cards = board.cards;
                self.lanes = board.lanes;
                self.store();
            }
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    room_id = %self.room_id,
                    "positron_kanban: work_board read failed — keeping last board (reader owns reconnection)"
                );
            }
        }
    }

    /// Replace the identity-resolution roster and re-project, so any card
    /// filed before its author's card arrived upgrades from a provisional
    /// label to the resolved name in place.
    fn apply_roster(&mut self, roster: Vec<RosterSlotView>) {
        self.roster = roster;
        self.store();
    }

    /// Project one airc `WorkCard` into a renderer-shaped [`KanbanCardView`],
    /// resolving the CREATOR richly and the ASSIGNEE (owner) as a light
    /// status pointer, both from the current roster.
    fn project_card(&self, card: &WorkCard) -> KanbanCardView {
        let creator_id = card.created_by.as_uuid();
        let creator = resolve_identity(&self.roster, creator_id);
        // Assignee is a status pointer, not an authored act — resolve only
        // its display name, not the full identity axes.
        let assignee_id = card.owner.map(|o| o.as_uuid());
        let assignee_name = assignee_id.map(|id| resolve_identity(&self.roster, id).name);
        KanbanCardView {
            card_id: card.card_id.as_uuid(),
            room_id: self.room_id,
            title: card.title.clone(),
            body: card.body.clone(),
            state: map_card_state(card.state),
            priority: map_priority(card.priority),
            lane_id: card.lane_id.map(|l| l.as_uuid()),
            creator_id,
            creator_name: creator.name,
            creator_kind: creator.kind,
            integrations: creator.integrations,
            provenance: creator.provenance,
            assignee_id,
            assignee_name,
            // Read the clock per card rather than once per board: a lease
            // boundary crossing mid-projection must not make one card claim a
            // different "now" than the predicate that judged it. Cheap, and
            // the projection is not a hot path.
            hold: map_hold(crate::persona::card_holder::hold_of(card, now_unix_ms())),
            pull_request: card.pull_request.as_ref().map(|pr| KanbanPullRequest {
                repo: pr.repo.to_string(),
                number: pr.number,
            }),
            created_at: card.created_at_ms,
            updated_at: card.updated_at_ms,
        }
    }

    /// Project one airc `LaneRecord` into a renderer-shaped [`KanbanLaneView`].
    fn project_lane(lane: &LaneRecord) -> KanbanLaneView {
        KanbanLaneView {
            lane_id: lane.lane_id.as_uuid(),
            title: lane.title.clone(),
            state: map_lane_state(lane.state),
            card_ids: lane.card_ids.iter().map(|c| c.as_uuid()).collect(),
        }
    }

    /// Frame the current board as a `kanban` `StateEnvelope` and write it to
    /// the substrate (cache + live broadcast). Persistent-tier: the work
    /// board is long-lived state (< 1 Hz), not the user-perceivable chat
    /// cadence — same tier as the wall.
    fn store(&self) {
        let cards = self.cards.iter().map(|c| self.project_card(c)).collect();
        let lanes = self.lanes.iter().map(Self::project_lane).collect();
        let view = KanbanViewState {
            room_id: self.room_id,
            lanes,
            cards,
        };
        self.substrate
            .store(self.builder.persistent(view));
    }
}

/// A bus event classified into a kanban projection input, or `None` when the
/// event is not one this projection folds. Pure — no substrate side effect,
/// no reader I/O — so it's unit-testable without a live bus or daemon.
enum KanbanInput {
    /// A work change landed for this `room_id` — cue to re-read the board.
    Changed(Uuid),
    /// The room roster changed (for this `room_id`) — new identity lookup.
    Presence(Uuid, Vec<RosterSlotView>),
}

fn classify(name: &str, payload: &serde_json::Value) -> Option<KanbanInput> {
    // The airc bus wraps event bodies under a `payload` key (see
    // `positron_source::classify`); accept a nested `payload` object, else
    // the top-level value — one unwrap convention across the projections.
    let body = payload.get("payload").unwrap_or(payload);
    match name {
        KANBAN_CHANGED => serde_json::from_value::<AircKanbanChanged>(body.clone())
            .ok()
            .map(|c| KanbanInput::Changed(c.room_id)),
        PRESENCE_UPDATED => serde_json::from_value::<AircPresenceUpdate>(body.clone())
            .ok()
            .map(|u| KanbanInput::Presence(u.room_id, u.roster)),
        _ => None,
    }
}

/// Run the kanban projection consume loop against an already-attached
/// [`WorkBoardReader`]. Subscribes to the bus, does an initial authoritative
/// read (so the board renders at boot without waiting for a change), then
/// folds `kanban:changed` / `presence:updated` for its room.
///
/// The receiver is taken BEFORE the initial read so a change racing the boot
/// read is still caught by the loop (a redundant re-read is idempotent — it
/// just re-reads the same authoritative board). Runs for the process
/// lifetime.
async fn run_kanban_loop(
    substrate: Substrate,
    room_id: Uuid,
    reader: Arc<dyn WorkBoardReader>,
    bus: Arc<MessageBus>,
) {
    let mut rx = bus.receiver();
    // Demand the current roster now (#118): `reload()` re-reads the board
    // authoritatively, but the roster it resolves card creators/assignees
    // against rides the fire-once `presence:updated` stream. Without this
    // cue a kanban projector that (re)started after the emitter's last
    // publish would label every card provisionally until presence next
    // changes. `rx` is subscribed above, so the re-publish lands in our
    // buffer.
    crate::ipc::positron_presence::request_presence_resync(&bus);
    let mut projection = KanbanProjection::new(substrate, room_id, reader);
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
/// trust-me). Pure control-flow over the projection; no bus access. Mirrors
/// the wall projector's `fold_recv`.
async fn fold_recv(
    projection: &mut KanbanProjection,
    room_id: Uuid,
    recv: Result<BusEvent, RecvError>,
) -> LoopStep {
    match recv {
        Ok(event) => {
            match classify(&event.name, &event.payload) {
                // Re-read only when the change is for OUR room (defensive
                // room guard; the node observes exactly this room today).
                Some(KanbanInput::Changed(rid)) if rid == room_id => projection.reload().await,
                Some(KanbanInput::Presence(rid, roster)) if rid == room_id => {
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

/// Fixed identity name for the node-level kanban reader. Like the presence /
/// wall readers, it attaches as a **heartbeat-less lurker** (reads the
/// daemon's authoritative transcript without ever appearing in the roster) —
/// a distinct name from the other node lurkers so the readers hold
/// independent keypairs and neither's lifecycle blinks the others.
const NODE_KANBAN_READER_NAME: &str = "continuum-kanban";

/// Attach a node-level kanban reader and run the kanban projection for one
/// room. The **consuming half** of the `kanban:changed` stream: it
/// subscribes to the bus (fed by `inbound_attach`) and re-reads the
/// airc-owned board on each change.
///
/// Mirrors [`super::positron_wall_source::spawn_node_wall_projector`]: a
/// dedicated node reader (not a persona's handle) so the board renders with
/// **zero resident personas** — a chat window with no persona present still
/// shows its work board. A failed attach/join means the projection can't
/// start; it logs the cause loudly and the task exits (the WS server is
/// optional → a disabled feature, not a substrate-wide panic;
/// [[fallbacks-are-illegal-fail-loud]]: no fabricated board is ever
/// substituted).
pub fn spawn_node_kanban_projector(
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
                "positron_kanban: cannot create node reader home — kanban projection disabled"
            );
            return;
        }
        let airc = match airc_lib::Airc::attach_as(
            node_home.clone(),
            NODE_KANBAN_READER_NAME,
            daemon_socket,
        )
        .await
        {
            Ok(airc) => airc,
            Err(err) => {
                tracing::error!(
                    error = %err,
                    home = %node_home.display(),
                    "positron_kanban: node reader attach failed — kanban projection disabled"
                );
                return;
            }
        };
        // Join by NAME (never UUID-as-string, which derives a DIFFERENT
        // channel — the recurring hazard the wall/presence readers document).
        // The reader must share the operator's channel or its board reads
        // land in an empty derived room.
        if let Err(err) = airc.join(&room_name).await {
            tracing::error!(
                error = %err,
                room = %room_name,
                "positron_kanban: node reader could not join room — kanban projection disabled"
            );
            return;
        }
        // `airc_lib::Airc` satisfies `WorkBoardReader` directly (impl above)
        // — reuse it, no adapter.
        let reader: Arc<dyn WorkBoardReader> = Arc::new(airc);
        tracing::info!(
            %room_id,
            room = %room_name,
            "positron_kanban: node reader attached — projecting kind=kanban"
        );
        run_kanban_loop(substrate, room_id, reader, bus).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use airc_core::{PeerId, RoomId};
    use airc_work::{LaneId, RepoId, WorkCardId};
    use async_trait::async_trait;
    use crate::ipc::positron_source::{test_presence_payload, test_roster_slot};
    use continuum_positron::{Provenance, SenderKind};
    use serde_json::json;
    use std::sync::Mutex;

    /// A `WorkBoardReader` stub returning a canned board — lets the
    /// projection be driven without a daemon (mirrors
    /// `positron_wall_source::tests::StubReader`).
    struct StubReader {
        board: Mutex<BoardSnapshot>,
        fail: Mutex<bool>,
    }

    impl StubReader {
        fn new(cards: Vec<WorkCard>, lanes: Vec<LaneRecord>) -> Arc<Self> {
            Arc::new(Self {
                board: Mutex::new(board(cards, lanes)),
                fail: Mutex::new(false),
            })
        }
        /// Flip the reader into failure mode — the next `work_board()`
        /// returns a terminal-shaped `AircError`.
        fn set_fail(&self, fail: bool) {
            *self.fail.lock().unwrap() = fail;
        }
    }

    #[async_trait]
    impl WorkBoardReader for StubReader {
        async fn work_board(&self) -> Result<BoardSnapshot, airc_lib::AircError> {
            if *self.fail.lock().unwrap() {
                return Err(airc_lib::AircError::UnknownPeer(PeerId::new()));
            }
            Ok(self.board.lock().unwrap().clone())
        }
    }

    /// An empty `BoardSnapshot` with the given cards/lanes — the other
    /// (workspace/PR/hygiene) collections stay empty, since the kanban view
    /// projects only cards + lanes.
    fn board(cards: Vec<WorkCard>, lanes: Vec<LaneRecord>) -> BoardSnapshot {
        BoardSnapshot {
            cards,
            lanes,
            workspaces: Vec::new(),
            repo_tracking: Vec::new(),
            pull_requests: Vec::new(),
            manager_hats: Vec::new(),
            agent_availability: Vec::new(),
            hygiene_reports: Vec::new(),
        }
    }

    fn repo() -> RepoId {
        RepoId::new("CambrianTech/continuum").expect("valid repo id")
    }

    fn card(
        room: RoomId,
        creator: PeerId,
        title: &str,
        state: CardState,
        owner: Option<PeerId>,
    ) -> WorkCard {
        let _ = room; // cards carry repo, not room_id; room comes from the projector
        WorkCard {
            card_id: WorkCardId::new(),
            repo: repo(),
            title: title.to_string(),
            body: None,
            priority: Priority::P2,
            lane_id: None,
            state,
            owner,
            claim_id: None,
            claim_expires_at_ms: None,
            last_heartbeat_at_ms: None,
            pull_request: None,
            created_by: creator,
            created_at_ms: 1_700_000_000_000,
            updated_at_ms: 1_700_000_000_000,
            reviews: None,
        }
    }

    fn lane(title: &str, state: LaneState, card_ids: Vec<WorkCardId>) -> LaneRecord {
        LaneRecord {
            lane_id: LaneId::new(),
            repo: repo(),
            title: title.to_string(),
            state,
            card_ids,
            created_by: PeerId::new(),
            created_at_ms: 1_700_000_000_000,
            updated_at_ms: 1_700_000_000_000,
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

    fn current_kanban(substrate: &Substrate) -> KanbanViewState {
        let env = substrate
            .cache()
            .get(KanbanViewState::KIND)
            .expect("a kanban envelope must be stored");
        serde_json::from_value(env.payload.clone()).expect("payload is a KanbanViewState")
    }

    #[tokio::test]
    async fn reload_projects_the_board_into_the_substrate() {
        // what this catches: regression where a board re-read does not reach
        // the substrate as a KanbanViewState — the whole point of the kanban
        // projection. With no presence yet, the creator renders provisionally
        // (short peer-id label, neutral Human) — the honest pending-truth
        // state, never a fabricated author. Mirrors
        // positron_wall_source::tests::reload_projects_the_board_into_the_substrate.
        let substrate = Substrate::new();
        let room = RoomId::new();
        let creator = PeerId::new();
        let c = card(room, creator, "Wire the kanban projector", CardState::Open, None);
        let reader = StubReader::new(vec![c], vec![]);
        let mut p = KanbanProjection::new(substrate.clone(), room.as_uuid(), reader);
        p.reload().await;

        let view = current_kanban(&substrate);
        assert_eq!(view.room_id, room.as_uuid());
        assert_eq!(view.cards.len(), 1);
        assert_eq!(view.cards[0].title, "Wire the kanban projector");
        assert_eq!(view.cards[0].state, KanbanCardState::Open);
        assert_eq!(view.cards[0].creator_id, creator.as_uuid());
        assert_eq!(view.cards[0].creator_kind, SenderKind::Human);
        assert!(
            view.cards[0].creator_name.starts_with("peer-"),
            "provisional creator label until presence, got {}",
            view.cards[0].creator_name
        );
        assert!(view.cards[0].integrations.is_empty());
        // Open, unclaimed card: no assignee, no PR.
        assert!(view.cards[0].assignee_id.is_none());
        assert!(view.cards[0].assignee_name.is_none());
        assert!(view.cards[0].pull_request.is_none());
    }

    #[tokio::test]
    async fn creator_upgrades_in_place_when_presence_folds_the_card() {
        // what this catches: the attribution-woven-in contract — a card filed
        // before its author's presence card arrives renders provisionally,
        // then UPGRADES (name + kind + badges + provenance) the instant
        // presence folds the card. A regression that dropped the re-project on
        // presence would leave the creator stuck as "peer-xxxx"/Human forever,
        // unattributable. [[positron-identity-security-first-class]]. Mirrors
        // the wall projector's author-upgrade test.
        let substrate = Substrate::new();
        let room = RoomId::new();
        let creator = PeerId::new();
        let c = card(room, creator, "File the card", CardState::Open, None);
        let reader = StubReader::new(vec![c], vec![]);
        let mut p = KanbanProjection::new(substrate.clone(), room.as_uuid(), reader);
        p.reload().await;
        assert_eq!(current_kanban(&substrate).cards[0].creator_kind, SenderKind::Human);

        // Card arrives via presence: Agent named Asha carrying a badge.
        let presence = presence_one(room.as_uuid(), creator.as_uuid(), "Asha", "agent");
        match classify(PRESENCE_UPDATED, &presence).expect("presence classifies") {
            KanbanInput::Presence(rid, roster) => {
                assert_eq!(rid, room.as_uuid());
                p.apply_roster(roster);
            }
            _ => panic!("presence:updated must classify as Presence"),
        }
        let view = current_kanban(&substrate);
        assert_eq!(view.cards[0].creator_name, "Asha");
        assert_eq!(view.cards[0].creator_kind, SenderKind::Agent);
        assert_eq!(
            view.cards[0].integrations.get("continuum.persona_id").map(String::as_str),
            Some("asha-1"),
            "opaque badge resolved from the card"
        );
        assert_eq!(
            view.cards[0].provenance.runtime, "claude",
            "accountability provenance resolved from the card"
        );
    }

    #[tokio::test]
    async fn claimed_card_resolves_assignee_name_from_roster() {
        // what this catches: the assignee (owner) status-pointer contract — a
        // claimed card carries a resolved assignee id + display name (from the
        // roster), distinct from the richer creator identity. A regression
        // that dropped assignee resolution, or filled it for an unclaimed
        // card, would mislabel who's working a card. The assignee is resolved
        // to NAME only (a status pointer), not the full creator identity axes.
        let substrate = Substrate::new();
        let room = RoomId::new();
        let creator = PeerId::new();
        let owner = PeerId::new();
        let c = card(room, creator, "Claim me", CardState::Claimed, Some(owner));
        let reader = StubReader::new(vec![c], vec![]);
        let mut p = KanbanProjection::new(substrate.clone(), room.as_uuid(), reader);
        p.reload().await;
        // Roster knows the owner as "BigMama".
        let presence = presence_one(room.as_uuid(), owner.as_uuid(), "BigMama", "agent");
        if let KanbanInput::Presence(_, roster) = classify(PRESENCE_UPDATED, &presence).unwrap() {
            p.apply_roster(roster);
        }
        let view = current_kanban(&substrate);
        assert_eq!(view.cards[0].state, KanbanCardState::Claimed);
        assert_eq!(view.cards[0].assignee_id, Some(owner.as_uuid()));
        assert_eq!(view.cards[0].assignee_name.as_deref(), Some("BigMama"));
    }

    #[tokio::test]
    async fn a_lapsed_lease_projects_as_takeable_even_though_the_column_still_says_claimed() {
        // what this catches: the human board's half of the stale-lease defect.
        // A claim is a LEASE; when it expires the holder stopped and the card
        // is takeable — but `state` still reads Claimed, so a view carrying
        // only the column shows dead work as active work. Measured 2026-08-06:
        // 17 of 19 leases expired and six citizens reported nothing to do.
        // The renderer can only grey a lapsed hold if the substrate SAYS it,
        // and this asserts the substrate says it — through the SAME predicate
        // the persona's board line uses, so the two cannot disagree.
        let substrate = Substrate::new();
        let room = RoomId::new();
        let owner = PeerId::new();

        let mut live = card(room, PeerId::new(), "Live hold", CardState::Claimed, Some(owner));
        live.claim_id = Some(airc_work::ClaimId::from_uuid(Uuid::new_v4()));
        live.claim_expires_at_ms = Some(u64::MAX);

        let mut lapsed = card(room, PeerId::new(), "Lapsed hold", CardState::Claimed, Some(owner));
        lapsed.claim_id = Some(airc_work::ClaimId::from_uuid(Uuid::new_v4()));
        lapsed.claim_expires_at_ms = Some(1_000_000); // 1970-adjacent — long expired

        let open = card(room, PeerId::new(), "Nobody's", CardState::Open, None);

        let reader = StubReader::new(vec![live, lapsed, open], vec![]);
        let mut p = KanbanProjection::new(substrate.clone(), room.as_uuid(), reader);
        p.reload().await;
        let view = current_kanban(&substrate);

        // The column says Claimed for BOTH held cards — that is exactly why the
        // column alone cannot carry the fact.
        assert_eq!(view.cards[0].state, KanbanCardState::Claimed);
        assert_eq!(view.cards[1].state, KanbanCardState::Claimed);
        // The hold tells them apart.
        assert_eq!(view.cards[0].hold, KanbanHold::Held);
        assert_eq!(view.cards[1].hold, KanbanHold::Lapsed);
        assert_eq!(view.cards[2].hold, KanbanHold::Unclaimed);
        // And the lapsed card still names who held it, so the renderer can say
        // whom to ask rather than who is busy.
        assert_eq!(view.cards[1].assignee_id, Some(owner.as_uuid()));
    }

    #[tokio::test]
    async fn lane_projects_with_state_and_card_ids() {
        // what this catches: the lane projection — a lane's id, title, mapped
        // state, and card_ids must reach the view so a renderer can group
        // cards into swimlanes. A regression in the LaneRecord→KanbanLaneView
        // map (dropped card_ids, wrong state) would break swimlane grouping.
        let substrate = Substrate::new();
        let room = RoomId::new();
        let creator = PeerId::new();
        let c = card(room, creator, "Laned card", CardState::InProgress, None);
        let card_id = c.card_id;
        let l = lane("positron define-once", LaneState::Active, vec![card_id]);
        let lane_id = l.lane_id;
        let reader = StubReader::new(vec![c], vec![l]);
        let mut p = KanbanProjection::new(substrate.clone(), room.as_uuid(), reader);
        p.reload().await;

        let view = current_kanban(&substrate);
        assert_eq!(view.lanes.len(), 1);
        assert_eq!(view.lanes[0].lane_id, lane_id.as_uuid());
        assert_eq!(view.lanes[0].title, "positron define-once");
        assert_eq!(view.lanes[0].state, KanbanLaneState::Active);
        assert_eq!(view.lanes[0].card_ids, vec![card_id.as_uuid()]);
    }

    #[tokio::test]
    async fn empty_board_is_projected_not_an_error() {
        // what this catches: a room with no work projects an empty board (not
        // a skipped store, not an error) — the honest-empty contract the
        // KanbanViewState type documents. A renderer subscribed to kind=kanban
        // must still receive a snapshot so it can clear a stale board. Mirrors
        // the wall projector's empty-board test.
        let substrate = Substrate::new();
        let room = RoomId::new();
        let reader = StubReader::new(vec![], vec![]);
        let mut p = KanbanProjection::new(substrate.clone(), room.as_uuid(), reader);
        p.reload().await;
        let view = current_kanban(&substrate);
        assert_eq!(view.room_id, room.as_uuid());
        assert!(view.cards.is_empty());
        assert!(view.lanes.is_empty());
    }

    #[test]
    fn foreign_room_and_malformed_events_are_skipped() {
        // what this catches: regression where a non-kanban event, or a
        // kanban:changed missing the room_id, gets classified into a spurious
        // re-read. Per [[fallbacks-are-illegal-fail-loud]]: an event that
        // can't deserialize into the contract is not-a-kanban-event → None.
        // (Room-mismatch filtering is enforced in the loop, tested via the
        // room_id the Changed/Presence inputs carry.)
        assert!(classify("media:frame", &json!({ "bytes": 4 })).is_none());
        assert!(classify(KANBAN_CHANGED, &json!({ "nope": true })).is_none());
        let room = Uuid::from_u128(0xa);
        match classify(KANBAN_CHANGED, &json!({ "roomId": room })).expect("classifies") {
            KanbanInput::Changed(rid) => assert_eq!(rid, room),
            _ => panic!("kanban:changed must classify as Changed"),
        }
    }

    #[tokio::test]
    async fn revision_advances_monotonically_across_stores() {
        // what this catches: regression where successive kanban projections
        // stamp a stale or non-monotonic revision — the session-protocol
        // last_seen replay routes by revision, so a flat revision would stop
        // the kanban widget from seeing updates. Mirrors the wall projector's
        // revision test.
        let substrate = Substrate::new();
        let room = RoomId::new();
        let creator = PeerId::new();
        let c = card(room, creator, "v1", CardState::Open, None);
        let reader = StubReader::new(vec![c], vec![]);
        let mut p = KanbanProjection::new(substrate.clone(), room.as_uuid(), reader);
        p.reload().await;
        let r1 = substrate.cache().get(KanbanViewState::KIND).unwrap().revision;
        // A presence fold re-projects → a second store, revision advances.
        let presence = presence_one(room.as_uuid(), creator.as_uuid(), "Asha", "agent");
        if let KanbanInput::Presence(_, roster) = classify(PRESENCE_UPDATED, &presence).unwrap() {
            p.apply_roster(roster);
        }
        let r2 = substrate.cache().get(KanbanViewState::KIND).unwrap().revision;
        assert!(r2 > r1, "revision must advance: {r1:?} -> {r2:?}");
    }

    #[tokio::test]
    async fn read_error_keeps_the_last_good_board() {
        // what this catches: the projector's DISTINCT resilience contract — a
        // `work_board()` read failure must keep the last-good board on the
        // widget, never blink it empty. A regression that cleared cards/lanes
        // on error, or stored an empty view, would flash the board blank on
        // every transient airc hiccup ([[fallbacks-are-illegal-fail-loud]]:
        // resilience, never a fabricated empty substitute). Mirrors the wall
        // projector's last-good-board test.
        let substrate = Substrate::new();
        let room = RoomId::new();
        let creator = PeerId::new();
        let c = card(room, creator, "v1", CardState::Open, None);
        let reader = StubReader::new(vec![c], vec![]);
        let mut p = KanbanProjection::new(substrate.clone(), room.as_uuid(), reader.clone());
        p.reload().await;
        assert_eq!(current_kanban(&substrate).cards.len(), 1);

        // The reader goes down mid-stream; the re-read fails.
        reader.set_fail(true);
        p.reload().await;

        let view = current_kanban(&substrate);
        assert_eq!(view.cards.len(), 1, "last-good board survives a read error");
        assert_eq!(view.cards[0].title, "v1");
    }

    #[tokio::test]
    async fn lagged_re_reads_and_closed_stops_and_foreign_room_ignored() {
        // what this catches: three otherwise trust-me loop-guard branches (the
        // loop is an infinite recv). (1) On `Lagged` (fell behind the buffer)
        // it RE-READS the authoritative board rather than continuing on a
        // stale one. (2) A `kanban:changed` for a DIFFERENT room is NOT
        // re-read — the defensive room guard drops it so a foreign room's
        // churn can't thrash this projector's board. (3) A `Closed` bus stops
        // the loop (Stop) — no spin. Drives the exact loop decisions via
        // `fold_recv` (no racing a live channel). Mirrors the wall projector's
        // fold_recv guard tests.
        let substrate = Substrate::new();
        let room = RoomId::new();
        let creator = PeerId::new();
        let reader = StubReader::new(vec![], vec![]);
        let mut p = KanbanProjection::new(substrate.clone(), room.as_uuid(), reader.clone());
        p.reload().await;
        assert!(current_kanban(&substrate).cards.is_empty());

        // Board gains a card while the loop was behind; a Lagged arrives.
        reader
            .board
            .lock()
            .unwrap()
            .cards
            .push(card(room, creator, "caught up", CardState::Open, None));
        let step = fold_recv(&mut p, room.as_uuid(), Err(RecvError::Lagged(3))).await;
        assert_eq!(step, LoopStep::Continue);
        assert_eq!(
            current_kanban(&substrate).cards.len(),
            1,
            "Lagged must trigger an authoritative re-read, not a stale continue"
        );

        // A change for some OTHER room must not re-read our board.
        let other = Uuid::from_u128(0xdead);
        let foreign = BusEvent {
            name: KANBAN_CHANGED.to_string(),
            payload: json!({ "roomId": other }),
        };
        reader.board.lock().unwrap().cards.clear();
        let step = fold_recv(&mut p, room.as_uuid(), Ok(foreign)).await;
        assert_eq!(step, LoopStep::Continue);
        assert_eq!(
            current_kanban(&substrate).cards.len(),
            1,
            "a foreign room's kanban:changed must not re-read our board"
        );

        // A closed bus stops the loop.
        let step = fold_recv(&mut p, room.as_uuid(), Err(RecvError::Closed)).await;
        assert_eq!(step, LoopStep::Stop);
    }
}
