//! RoomBoardSource — grounds a persona in the CURRENT ROOM's whole work board
//! (the kanban): every card, every owner, every column — read live from airc.
//!
//! ### Why this source exists (the Observer perceiving the board)
//!
//! [`ActiveWorkSource`](super::active_work_source) grounds a persona in ITS OWN
//! claimed cards, across ALL rooms — "what am I holding." This source is the
//! complementary perception: the WHOLE board of the room this turn is in — "what
//! is on the board here, who holds what, what column is it in." A persona that
//! can only see its own claims is blind to the shared plan; a persona that sees
//! the board can coordinate against it. The two are distinct concerns
//! (self-scoped-cross-room vs whole-board-this-room) and neither subsumes the
//! other, so this is a separate source, not an extension.
//!
//! ### Reads airc DIRECTLY — never the desktop-app projection
//!
//! The continuum desktop app projects this same airc board into a
//! `KanbanViewState` for its renderers
//! ([`crate::ipc::positron_kanban_source`]). This source does NOT read that
//! projection — persona cognition grounds on the airc substrate ITSELF, exactly
//! as [`WallSource`](super::wall_source) reads airc `wall_posts` directly and
//! [`ActiveWorkSource`] reads airc `active_claims` directly. Two faces of one
//! airc board (a renderer's ViewState and a persona's grounding), never reading
//! through each other — cognition stays decoupled from the rendering layer. The
//! authoritative board fold lives ONCE in airc (`work_board_complete`); this is
//! just its injection into grounding.
//!
//! The airc access lives behind [`RoomBoardReader`] (a one-method seam, same
//! shape as [`WallReader`](super::wall_source::WallReader) /
//! [`AircWorkReader`](super::active_work_source::AircWorkReader)): production
//! rides `airc_lib::Airc::work_board_complete`; tests stub it without a daemon.
//! A distinct trait name (not the projector's `WorkBoardReader`) keeps this
//! decoupled from `ipc/` and avoids any trait-coherence clash — each consumer
//! owns its thin reader, the real logic stays single-sourced in airc.
//!
//! ### Doctrine alignment
//!
//! - [[substrate-is-a-good-citizen-on-the-host]]: a failed/absent board read
//!   returns an empty delivery — cognition stays up; an empty board renders no
//!   block.
//! - Persona-scoped at construction (defense in depth, same as the roster +
//!   wall + active-work sources).
//! - Enriching framing, NOT a participation gate: the board shapes WHAT a
//!   persona knows about the room's work, it does not decide WHETHER it speaks
//!   (that is doctrine). Whole board = all owners, so the source imposes no
//!   continuum-side ordering or filtering heuristic that would steer cognition
//!   ([[no-hardcoded-heuristics-to-steer-cognition]]) — it delivers airc's own
//!   card order and, under budget pressure, truncates with a logged count
//!   rather than silently dropping or re-ranking.
//! - Atomic unit = ONE card. A truncated board is still truthful for the cards
//!   it names, so (like the active-work source) this source truncates rather
//!   than paginates; pagination can be added later without changing the seam if
//!   boards grow past a single budget.

use std::sync::Arc;

use airc_lib::AircError;
use airc_work::BoardSnapshot;
use async_trait::async_trait;
use serde_json::json;

use crate::persona::rag_budget::{
    ContinuationCursor, RagContext, RagDelivery, RagItem, RagSource, ResolutionPreference,
};

/// Source identifier — the deliberation faculty renders this delivery under a
/// `[room-kanban]` header (generic `[<source_id>]` projection). Distinct from
/// `active-work` (own claims) and `room-board` (the wall).
const SOURCE_ID: &str = "room-kanban";

/// Token estimate — the ONE canonical chars/4 estimator
/// (`cognition::token_budget`), shared by every RAG source so the replay
/// ledger's numbers match.
use crate::cognition::token_budget::estimate_prompt_tokens as estimate_tokens;

/// Reads the CURRENT ROOM's complete work board from airc. Production rides
/// `airc_lib::Airc::work_board_complete` (the WHOLE board, not the recent
/// window — an old active card must not drop off just because chat traffic
/// pushed its creation event out of a transcript window); tests stub it.
#[async_trait]
pub trait RoomBoardReader: Send + Sync {
    /// The current room's work board, folded by airc into a [`BoardSnapshot`].
    async fn work_board(&self) -> Result<BoardSnapshot, AircError>;
}

/// `airc_lib::Airc` satisfies the reader contract directly. Orphan rule OK —
/// the trait is ours. Reads the complete board and snapshots it into the plain
/// [`BoardSnapshot`] this source renders. Same call the desktop-app projector
/// makes; the shared truth is airc's fold, not a shared continuum trait.
#[async_trait]
impl RoomBoardReader for airc_lib::Airc {
    async fn work_board(&self) -> Result<BoardSnapshot, AircError> {
        let projection =
            airc_lib::Airc::work_board_complete(self, airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE)
                .await?;
        Ok(projection.snapshot())
    }
}

/// Persona-bound source reading the current room's whole work board.
pub struct RoomBoardSource {
    persona_id: uuid::Uuid,
    reader: Arc<dyn RoomBoardReader>,
}

impl RoomBoardSource {
    pub fn new(persona_id: uuid::Uuid, reader: Arc<dyn RoomBoardReader>) -> Self {
        Self { persona_id, reader }
    }

    fn empty() -> RagDelivery {
        RagDelivery {
            source_id: SOURCE_ID.to_string(),
            items: Vec::new(),
            tokens_used: 0,
            continuation: None,
            resolution_used: ResolutionPreference::Placeholder,
        }
    }

    /// Render one card as a grounding line: the state (column), title, priority,
    /// and owner (or `unclaimed`) — the shape a teammate reads off the board.
    /// The whole board carries all owners, so owner is always surfaced (unlike
    /// the active-work source, which is implicitly self-owned).
    fn render(card: &airc_work::WorkCard) -> String {
        let id8: String = card.card_id.as_uuid().to_string().chars().take(8).collect();
        let owner = match card.owner {
            Some(o) => {
                let o8: String = o.as_uuid().to_string().chars().take(8).collect();
                format!("owner {o8}")
            }
            None => "unclaimed".to_string(),
        };
        format!(
            "card {id8} [{state:?}] \"{title}\" ({prio:?}, {owner})",
            state = card.state,
            title = card.title,
            prio = card.priority,
        )
    }
}

#[async_trait]
impl RagSource for RoomBoardSource {
    fn source_id(&self) -> &'static str {
        SOURCE_ID
    }

    async fn deliver(
        &self,
        ctx: &RagContext,
        budget: u32,
        resolution: ResolutionPreference,
    ) -> RagDelivery {
        // Persona-scoped: a cross-persona ctx gets nothing (defense in depth,
        // same shape as the wall / active-work / roster sources).
        if ctx.persona_id != self.persona_id {
            return Self::empty();
        }

        // One airc call (the current room's complete board). Failure is
        // non-fatal — empty delivery, cognition stays up (good-citizen doctrine).
        let board = match self.reader.work_board().await {
            Ok(b) => b,
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    persona_id = %self.persona_id,
                    "room_board: work_board failed — empty delivery, cognition stays up"
                );
                return Self::empty();
            }
        };
        // Empty board → no block (normal: a room may have no cards yet).
        if board.cards.is_empty() {
            return Self::empty();
        }

        let mut items: Vec<RagItem> = Vec::new();
        let mut tokens_used: u32 = 0;
        let mut dropped: usize = 0;
        for card in &board.cards {
            let content = Self::render(card);
            let tokens = estimate_tokens(&content);
            if tokens_used.saturating_add(tokens) > budget {
                // Budget exhausted — a truncated board is still truthful for the
                // cards it names. Count the drops so truncation is visible, not
                // silent. Atomic unit = one card; no continuation (see module doc).
                dropped = board.cards.len() - items.len();
                break;
            }
            tokens_used += tokens;
            items.push(RagItem {
                content,
                tokens,
                metadata: json!({
                    "card_id": card.card_id.as_uuid().to_string(),
                    "state": format!("{:?}", card.state),
                    "owner": card.owner.map(|o| o.as_uuid().to_string()),
                    "priority": format!("{:?}", card.priority),
                    "lane_id": card.lane_id.map(|l| l.as_uuid().to_string()),
                }),
            });
        }

        // Budget too small to carry even one card → no block.
        if items.is_empty() {
            return Self::empty();
        }

        tracing::debug!(
            persona_id = %self.persona_id,
            budget,
            cards = board.cards.len(),
            delivered = items.len(),
            dropped,
            tokens_used,
            "room_board: deliver"
        );

        RagDelivery {
            source_id: SOURCE_ID.to_string(),
            items,
            tokens_used,
            continuation: None,
            resolution_used: resolution,
        }
    }

    async fn deliver_continuation(
        &self,
        _ctx: &RagContext,
        _cursor: ContinuationCursor,
        _budget: u32,
    ) -> Option<RagDelivery> {
        // Atomic units (one card each), truncate-not-paginate — same as the
        // active-work source. A board that overflows one budget truncates with
        // a logged count; there is no continuation cursor to resume.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airc_work::{CardState, Priority, RepoId, WorkCard, WorkCardId};
    use std::sync::Mutex;
    use uuid::Uuid;

    fn persona() -> Uuid {
        Uuid::parse_str("00000000-0000-0000-0000-000000000aaa").unwrap()
    }

    fn ctx() -> RagContext {
        RagContext::for_persona(persona(), 1_000_000)
    }

    fn card(title: &str, state: CardState, owner: Option<airc_core::PeerId>) -> WorkCard {
        WorkCard {
            card_id: WorkCardId::new(),
            repo: RepoId::new("acme/continuum").expect("valid repo id in fixture"),
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
            created_by: airc_core::PeerId::new(),
            created_at_ms: 1_000_000,
            updated_at_ms: 1_000_000,
            reviews: None,
        }
    }

    fn snapshot(cards: Vec<WorkCard>) -> BoardSnapshot {
        BoardSnapshot {
            cards,
            lanes: Vec::new(),
            workspaces: Vec::new(),
            repo_tracking: Vec::new(),
            pull_requests: Vec::new(),
            manager_hats: Vec::new(),
            agent_availability: Vec::new(),
            hygiene_reports: Vec::new(),
        }
    }

    struct StubReader {
        board: BoardSnapshot,
        fail: Mutex<bool>,
    }

    impl StubReader {
        fn new(board: BoardSnapshot) -> Self {
            Self {
                board,
                fail: Mutex::new(false),
            }
        }
        fn set_fail(&self, fail: bool) {
            *self.fail.lock().unwrap() = fail;
        }
    }

    #[async_trait]
    impl RoomBoardReader for StubReader {
        async fn work_board(&self) -> Result<BoardSnapshot, AircError> {
            if *self.fail.lock().unwrap() {
                return Err(AircError::UnknownPeer(airc_core::PeerId::new()));
            }
            Ok(self.board.clone())
        }
    }

    // what this catches: the whole room board surfaces as a delivery the brain
    // renders into the [room-kanban] grounding block — every card with its
    // column, title, priority, and owner. This is the Observer perceiving the
    // board (task #117 O6), distinct from active-work's own-claims-only view.
    #[tokio::test]
    async fn whole_board_surfaces_with_owner_and_state() {
        let holder = airc_core::PeerId::new();
        let reader = Arc::new(StubReader::new(snapshot(vec![
            card("Wire the projector", CardState::InProgress, Some(holder)),
            card("Review the PR", CardState::Open, None),
        ])));
        let source = RoomBoardSource::new(persona(), reader);
        let delivery = source.deliver(&ctx(), 1_000, ResolutionPreference::Raw).await;
        assert_eq!(delivery.items.len(), 2);
        assert!(delivery.items[0].content.contains("Wire the projector"));
        assert!(delivery.items[0].content.contains("[InProgress]"));
        let owner8: String = holder.as_uuid().to_string().chars().take(8).collect();
        assert!(delivery.items[0].content.contains(&owner8));
        // An unclaimed card is surfaced as such — all owners visible on the board.
        assert!(delivery.items[1].content.contains("unclaimed"));
        assert_eq!(delivery.items[1].metadata["state"], "Open");
        assert!(delivery.continuation.is_none());
    }

    // what this catches: a room with NO cards renders no block (backwards-
    // compatible; a fresh room's board is empty).
    #[tokio::test]
    async fn empty_board_delivers_nothing() {
        let reader = Arc::new(StubReader::new(snapshot(vec![])));
        let source = RoomBoardSource::new(persona(), reader);
        let delivery = source.deliver(&ctx(), 1_000, ResolutionPreference::Raw).await;
        assert!(delivery.items.is_empty());
        assert_eq!(delivery.tokens_used, 0);
        assert!(delivery.continuation.is_none());
    }

    // what this catches: a read failure degrades to empty, never panics —
    // cognition stays up if the work subsystem is degraded.
    #[tokio::test]
    async fn read_error_returns_empty_no_panic() {
        let reader = Arc::new(StubReader::new(snapshot(vec![card(
        "x",
            CardState::Open,
            None,
        )])));
        reader.set_fail(true);
        let source = RoomBoardSource::new(persona(), reader);
        let delivery = source.deliver(&ctx(), 1_000, ResolutionPreference::Raw).await;
        assert!(delivery.items.is_empty());
        assert_eq!(delivery.resolution_used, ResolutionPreference::Placeholder);
    }

    // what this catches: cross-persona ctx gets nothing (defense in depth).
    #[tokio::test]
    async fn cross_persona_ctx_returns_empty() {
        let reader = Arc::new(StubReader::new(snapshot(vec![card(
            "x",
            CardState::Open,
            None,
        )])));
        let source = RoomBoardSource::new(persona(), reader);
        let alien = Uuid::parse_str("00000000-0000-0000-0000-000000000bbb").unwrap();
        let delivery = source
            .deliver(
                &RagContext::for_persona(alien, 1_000_000),
                1_000,
                ResolutionPreference::Raw,
            )
            .await;
        assert!(delivery.items.is_empty());
        assert_eq!(delivery.resolution_used, ResolutionPreference::Placeholder);
    }

    // what this catches: a board too large for the budget truncates to the
    // cards that fit and NEVER overspends — a truncated board is truthful for
    // what it names, and truncation is bounded (no partial card, no overspend).
    #[tokio::test]
    async fn oversized_board_truncates_within_budget() {
        // Each rendered card ~= tens of tokens; a tiny budget admits only a few.
        let cards: Vec<WorkCard> = (0..50)
            .map(|i| card(&format!("card number {i}"), CardState::Open, None))
            .collect();
        let reader = Arc::new(StubReader::new(snapshot(cards)));
        let source = RoomBoardSource::new(persona(), reader);
        let delivery = source.deliver(&ctx(), 40, ResolutionPreference::Raw).await;
        assert!(!delivery.items.is_empty(), "at least one card fits budget 40");
        assert!(
            delivery.tokens_used <= 40,
            "overspent: {} > 40",
            delivery.tokens_used
        );
        assert!(delivery.items.len() < 50, "truncated below the full board");
        assert!(delivery.continuation.is_none());
    }
}
