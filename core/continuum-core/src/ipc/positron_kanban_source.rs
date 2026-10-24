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

use airc_work::{BoardSnapshot, CardState, LaneRecord, LaneState, Priority, WorkCard, WorkReviewOutcome};
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use continuum_positron::{
    KanbanCardState, KanbanCardView, KanbanHold, KanbanLaneState, KanbanLaneView, KanbanPriority,
    KanbanPullRequest, KanbanReviewOutcome, KanbanReviewView, KanbanViewState, RosterSlotView, StateBuilder, Substrate,
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

/// Map airc's `WorkReviewOutcome` → positron's mirrored `KanbanReviewOutcome` by value.
/// Exhaustive for the same reason as [`map_card_state`].
fn map_review_outcome(outcome: WorkReviewOutcome) -> KanbanReviewOutcome {
    match outcome {
        WorkReviewOutcome::Passed => KanbanReviewOutcome::Passed,
        WorkReviewOutcome::Failed => KanbanReviewOutcome::Failed,
        WorkReviewOutcome::Unknown => KanbanReviewOutcome::Unknown,
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

fn map_hold(hold: crate::cognition::card_holder::Hold) -> KanbanHold {
    use crate::cognition::card_holder::Hold;
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
    /// Last authoritative accepted reviews, from the same board fold: each card shows the
    /// latest review of its latest submission, finding included.
    reviews: Vec<airc_work::WorkSubmissionReview>,
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
            reviews: Vec::new(),
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
                self.reviews = board.submission_reviews;
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
            hold: map_hold(crate::cognition::card_holder::hold_of(card, continuum_base::time::EpochMs::now().millis())),
            pull_request: card.pull_request.as_ref().map(|pr| KanbanPullRequest {
                repo: pr.repo.to_string(),
                number: pr.number,
            }),
            review: self.latest_review(card),
            created_at: card.created_at_ms,
            updated_at: card.updated_at_ms,
        }
    }

    /// The latest review of `card`'s latest submission, its reviewer resolved like a creator
    /// and its finding as the reviewer wrote it.
    fn latest_review(&self, card: &WorkCard) -> Option<KanbanReviewView> {
        let submission = crate::cognition::card_holder::latest_submission(card)?.submission_id;
        let review = self
            .reviews
            .iter()
            .filter(|r| r.submission_id == submission)
            .max_by_key(|r| r.reviewed_at_ms)?;
        let reviewer_id = review.reviewer.as_uuid();
        Some(KanbanReviewView {
            submission_id: submission.as_uuid(),
            reviewer_id,
            reviewer_name: resolve_identity(&self.roster, reviewer_id).name,
            outcome: map_review_outcome(review.outcome),
            finding: review.evidence_text.clone(),
            reviewed_at: review.reviewed_at_ms,
        })
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
        self.substrate.store(self.builder.persistent(view));
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
mod tests;
