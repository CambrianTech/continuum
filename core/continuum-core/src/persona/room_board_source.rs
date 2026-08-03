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
    /// The room whose board this source grounds. The reader answers for the
    /// persona's airc connection's room; this names it explicitly so delivery
    /// can be scoped to the TURN'S context (the room gate in `deliver`). Bound
    /// at assembly from the room the persona joined at bootstrap
    /// (`identity.default_room`). `None` = unscoped (legacy/test construction):
    /// deliver regardless of turn context, exactly the pre-gate behavior.
    room_id: Option<uuid::Uuid>,
    reader: Arc<dyn RoomBoardReader>,
}

impl RoomBoardSource {
    pub fn new(persona_id: uuid::Uuid, reader: Arc<dyn RoomBoardReader>) -> Self {
        Self {
            persona_id,
            room_id: None,
            reader,
        }
    }

    /// Bind this source to the room its reader answers for. A context-stamped
    /// turn (`ctx.airc_room`) in ANY other context — another room, or a synthetic
    /// context like the eval fork's nil room — then gets an empty delivery
    /// instead of this room's board (the exam-bleed fix: stale board imperatives
    /// injected into a coding exam derailed agentically-trained models).
    /// [[identity-context-session-three-axes]]
    pub fn for_room(mut self, room_id: uuid::Uuid) -> Self {
        self.room_id = Some(room_id);
        self
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
    fn render(card: &airc_work::WorkCard, self_id: uuid::Uuid, now_ms: u64) -> String {
        let id8: String = card.card_id.as_uuid().to_string().chars().take(8).collect();
        let owner = match card.owner {
            // Her OWN claim must read as HERS — glass-boxed 2026-07-11: cards she
            // held rendered as `owner 90e758b2`, a hex prefix she cannot recognize
            // as herself, so claimed work carried zero self-relevance and the room
            // drifted to chatter over held cards. Identity is a structural fact
            // the projection must carry (same law as (to you) addressing).
            // A LAPSED lease is equally structural (2026-08-03): rendering an
            // expired claim as plain ownership contradicts the #156 lost-claim
            // fact and re-anchors work the holder no longer has.
            Some(o) if claim_is_live(card, now_ms) && o.as_uuid() == self_id => {
                "owner YOU".to_string()
            }
            Some(o) if claim_is_live(card, now_ms) => {
                let o8: String = o.as_uuid().to_string().chars().take(8).collect();
                format!("owner {o8}")
            }
            Some(o) if o.as_uuid() == self_id => "claim lapsed (was YOURS) — claimable".to_string(),
            Some(o) => {
                let o8: String = o.as_uuid().to_string().chars().take(8).collect();
                format!("claim lapsed (was {o8}) — claimable")
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

/// Mirror of airc-lib's `is_active_claim` (work_roster.rs) — the lease truth this
/// projection must agree with. airc's roster already drops an expired lease from
/// `active_claims` (that transition is what fires the #156 lost-claim fact), but
/// this board view kept rendering the same card as HELD — so one perception said
/// "your claim lapsed" while the next said "you HOLD it", and the residents
/// re-oriented on dead work every wake, forever (glass-boxed 2026-08-03: all 17
/// board claims stale, the conway/wordstats loop attractor). Duplicated rather
/// than imported because continuum deps airc-protocol/-core, not airc-lib; the
/// doc-link is the drift guard.
fn claim_is_live(card: &airc_work::WorkCard, now_ms: u64) -> bool {
    card.owner.is_some()
        && card.claim_id.is_some()
        && card.claim_expires_at_ms.is_some_and(|e| e > now_ms)
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
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
        // Room-scoped: a context-stamped turn in a DIFFERENT context than the
        // room this board belongs to gets nothing — room A's kanban must not
        // ground a turn in room B, nor a synthetic context (the eval fork's nil
        // room). The ONE shared gate (`room_scope_allows`) probes every abstain
        // with both rooms named, so a mis-binding shows in the log instead of a
        // silent blank grounding block. [[identity-context-session-three-axes]]
        if !crate::persona::rag_budget::room_scope_allows(self.room_id, ctx, SOURCE_ID) {
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
        // One clock reading per delivery — every lease judgment in this block
        // (held / lapsed / available) agrees on the same instant.
        let now_ms = now_unix_ms();

        // YOUR-WORK SALIENCE (2026-07-11, "did they wander off and forget they
        // can work on things?"): all three personas held claimed cards, zero
        // completions — a claim was one event, then the card's self-relevance
        // vanished from perception while fresh chatter arrived every tick. Lead
        // with the cards SHE holds as a distinct perceived fact (hers, by title,
        // with state), so her own unfinished work is standing perception, not a
        // fading memory. A fact she weighs, never an instruction
        // ([[no-hardcoded-heuristics-to-steer-cognition]]).
        let mine: Vec<&airc_work::WorkCard> = board
            .cards
            .iter()
            .filter(|c| {
                c.owner.map(|o| o.as_uuid() == self.persona_id).unwrap_or(false)
                    // Only a LIVE lease is "held" (see `claim_is_live`): a lapsed
                    // claim rendered as held contradicts the #156 lost-claim fact
                    // and is the stale-lease loop attractor (2026-08-03).
                    && claim_is_live(c, now_ms)
                    && !matches!(
                        c.state,
                        airc_work::CardState::Merged | airc_work::CardState::Closed
                    )
            })
            .collect();
        if !mine.is_empty() {
            let titles = mine
                .iter()
                .take(5)
                .map(|c| {
                    let id8: String =
                        c.card_id.as_uuid().to_string().chars().take(8).collect();
                    format!("  {id8}: \"{}\" [{:?}]", c.title, c.state)
                })
                .collect::<Vec<_>>()
                .join("\n");
            let lead = format!(
                "[your work] you HOLD {n} card(s) on this board — claimed by you, not \
                 yet done:\n{titles}\nThey remain yours until finished (work/state) or \
                 released (work/release); your tools do the work.",
                n = mine.len(),
            );
            let lead_tokens = estimate_tokens(&lead);
            if lead_tokens <= budget {
                tokens_used += lead_tokens;
                items.push(RagItem {
                    content: lead,
                    tokens: lead_tokens,
                    metadata: serde_json::json!({ "kind": "your-work-lead" }),
                });
            }
        }

        // AVAILABLE-WORK SALIENCE (#122): unclaimed cards are work waiting for
        // someone to pick up. Glass-boxed live 2026-07-10: with hands proven and an
        // open card sitting on the board, both personas drifted to identity-monologue
        // chatter instead of noticing the available work — an unclaimed card, flat in
        // the list, out-salienced by nothing. Lead the delivery with the open cards
        // as a distinct perceived FACT (their count + titles + how to pick one up), so
        // available work is the first thing seen, not buried. A true structural fact
        // she WEIGHS — it names what's available and how claiming works; it never says
        // she must ([[no-hardcoded-heuristics-to-steer-cognition]]). Whole board still
        // follows verbatim in airc's own order — this adds a salience lead, it does not
        // re-rank or filter the board itself.
        let open: Vec<&airc_work::WorkCard> = board
            .cards
            .iter()
            .filter(|c| {
                // Unclaimed-and-Open, OR a non-terminal card whose claim LAPSED —
                // an expired lease is genuinely available work (claim-contention
                // allows takeover), and before 2026-08-03 lapsed cards appeared in
                // NEITHER "available" nor honestly-held: invisible as work, sticky
                // as an attractor.
                let terminal = matches!(
                    c.state,
                    airc_work::CardState::Merged | airc_work::CardState::Closed
                );
                if terminal {
                    return false;
                }
                match c.owner {
                    None => matches!(c.state, airc_work::CardState::Open),
                    Some(_) => !claim_is_live(c, now_ms),
                }
            })
            .collect();
        if !open.is_empty() {
            let titles = open
                .iter()
                .take(5)
                .map(|c| {
                    let id8: String =
                        c.card_id.as_uuid().to_string().chars().take(8).collect();
                    format!("  {id8}: \"{}\" ({:?})", c.title, c.priority)
                })
                .collect::<Vec<_>>()
                .join("\n");
            let more = open.len().saturating_sub(5);
            let tail = if more > 0 {
                format!("\n  …and {more} more unclaimed")
            } else {
                String::new()
            };
            let lead = format!(
                "[available work] {n} card(s) on this board are claimable — unclaimed, \
                 or their holder's claim lease lapsed:\n{titles}{tail}\nA card is \
                 picked up with `work/claim` (its id); once claimed it is yours to \
                 work with your tools. Claimable work waits until someone chooses it.",
                n = open.len(),
            );
            let lead_tokens = estimate_tokens(&lead);
            if lead_tokens <= budget {
                tokens_used += lead_tokens;
                items.push(RagItem {
                    content: lead,
                    tokens: lead_tokens,
                    metadata: json!({ "kind": "available-work-lead", "open_count": open.len() }),
                });
            }
        }

        let mut cards_delivered = 0usize;
        for card in &board.cards {
            let content = Self::render(card, self.persona_id, now_ms);
            let tokens = estimate_tokens(&content);
            if tokens_used.saturating_add(tokens) > budget {
                // Budget exhausted — a truncated board is still truthful for the
                // cards it names. Count the drops so truncation is visible, not
                // silent. Atomic unit = one card; no continuation (see module doc).
                // Counted over CARDS, excluding the available-work lead item.
                dropped = board.cards.len() - cards_delivered;
                break;
            }
            tokens_used += tokens;
            cards_delivered += 1;
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
        // An OWNED fixture card carries a LIVE lease by default — matching how a
        // real claim exists on the board (claim_id + unexpired claim_expires_at_ms).
        // An owner with NO live lease is the LAPSED shape, built via
        // `lapse(card(...))` below.
        let claimed = owner.is_some();
        WorkCard {
            card_id: WorkCardId::new(),
            repo: RepoId::new("acme/continuum").expect("valid repo id in fixture"),
            title: title.to_string(),
            body: None,
            priority: Priority::P2,
            lane_id: None,
            state,
            owner,
            claim_id: claimed.then(|| airc_work::ClaimId::from_uuid(uuid::Uuid::new_v4())),
            claim_expires_at_ms: claimed.then(|| now_unix_ms() + 60_000),
            last_heartbeat_at_ms: None,
            pull_request: None,
            created_by: airc_core::PeerId::new(),
            created_at_ms: 1_000_000,
            updated_at_ms: 1_000_000,
            reviews: None,
        }
    }

    /// Turn an owned fixture card into the stale-lease shape: claim still on the
    /// card, lease long expired — what the live board showed for all 17 claims
    /// on 2026-08-03.
    fn lapse(mut c: WorkCard) -> WorkCard {
        c.claim_expires_at_ms = Some(1_000_000); // 1970-adjacent — long expired
        c
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
        // With one Open card, an available-work lead precedes the card list.
        assert_eq!(delivery.items.len(), 3);
        assert_eq!(delivery.items[0].metadata["kind"], "available-work-lead");
        let cards: Vec<&RagItem> = delivery
            .items
            .iter()
            .filter(|i| i.metadata.get("card_id").is_some())
            .collect();
        assert_eq!(cards.len(), 2);
        assert!(cards[0].content.contains("Wire the projector"));
        assert!(cards[0].content.contains("[InProgress]"));
        let owner8: String = holder.as_uuid().to_string().chars().take(8).collect();
        assert!(cards[0].content.contains(&owner8));
        // An unclaimed card is surfaced as such — all owners visible on the board.
        assert!(cards[1].content.contains("unclaimed"));
        assert_eq!(cards[1].metadata["state"], "Open");
        assert!(delivery.continuation.is_none());
    }

    // what this catches: the available-work salience lead (#122). Unclaimed cards
    // must be surfaced PROMINENTLY (first item, naming them + how to claim) so a
    // persona perceives available work instead of drifting to idle chatter with an
    // open card sitting on the board. A board with NO open cards adds no lead.
    #[tokio::test]
    async fn open_cards_get_an_available_work_lead() {
        let holder = airc_core::PeerId::new();
        let reader = Arc::new(StubReader::new(snapshot(vec![
            card("Compile wordstats", CardState::Open, None),
            card("sha256 exercise", CardState::Open, None),
            card("Already mine", CardState::InProgress, Some(holder)),
        ])));
        let source = RoomBoardSource::new(persona(), reader);
        let d = source.deliver(&ctx(), 2_000, ResolutionPreference::Raw).await;
        assert_eq!(d.items[0].metadata["kind"], "available-work-lead");
        assert_eq!(d.items[0].metadata["open_count"], 2);
        assert!(d.items[0].content.contains("[available work]"));
        assert!(d.items[0].content.contains("Compile wordstats"));
        assert!(d.items[0].content.contains("work/claim"));

        // A board with only claimed cards → no lead, just the card list.
        let claimed_only = Arc::new(StubReader::new(snapshot(vec![card(
            "Held",
            CardState::InProgress,
            Some(holder),
        )])));
        let d2 = RoomBoardSource::new(persona(), claimed_only)
            .deliver(&ctx(), 2_000, ResolutionPreference::Raw)
            .await;
        assert!(d2.items.iter().all(|i| i.metadata.get("kind").is_none()));
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

    // what this catches: the your-work salience (2026-07-11, "did they wander
    // off and forget they can work on things?") — a card the perceiving persona
    // HOLDS must lead the delivery as "[your work] you HOLD…" and render as
    // "owner YOU", never as her own unrecognizable hex prefix. Another peer's
    // claim stays hex-attributed and produces no your-work lead.
    #[tokio::test]
    async fn held_cards_lead_as_your_work_and_render_owner_you() {
        let me = persona();
        let mine = card(
            "reverse a string",
            CardState::Claimed,
            Some(airc_core::PeerId::from_uuid(me)),
        );
        let theirs = card(
            "sha256 exercise",
            CardState::Claimed,
            Some(airc_core::PeerId::new()),
        );
        let reader = Arc::new(StubReader::new(snapshot(vec![mine, theirs])));
        let source = RoomBoardSource::new(me, reader);
        let delivery = source.deliver(&ctx(), 2_000, ResolutionPreference::Raw).await;
        let all: String = delivery
            .items
            .iter()
            .map(|i| i.content.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            all.contains("[your work] you HOLD 1 card(s)"),
            "her held card leads as a your-work fact: {all}"
        );
        assert!(
            all.contains("reverse a string"),
            "the lead names HER card: {all}"
        );
        assert!(all.contains("owner YOU"), "her claim renders as YOU: {all}");
        // The peer's card keeps hex attribution and never enters her lead.
        assert!(
            !all.contains("you HOLD 2"),
            "another peer's claim is not hers: {all}"
        );
    }

    // what this catches: the stale-lease loop attractor (glass-boxed 2026-08-03:
    // all 17 board claims stale, residents re-orienting on "held" conway/wordstats
    // cards every wake forever). A card whose claim lease EXPIRED must (a) never
    // count in "[your work] you HOLD", (b) surface as claimable in the
    // available-work lead, and (c) render its lapsed state — agreeing with the
    // #156 lost-claim fact instead of contradicting it every turn.
    #[tokio::test]
    async fn lapsed_lease_reads_claimable_not_held() {
        let me = persona();
        let stale_mine = lapse(card(
            "conway game of life",
            CardState::Claimed,
            Some(airc_core::PeerId::from_uuid(me)),
        ));
        let live_mine = card(
            "wordstats tests",
            CardState::Claimed,
            Some(airc_core::PeerId::from_uuid(me)),
        );
        let reader = Arc::new(StubReader::new(snapshot(vec![stale_mine, live_mine])));
        let source = RoomBoardSource::new(me, reader);
        let delivery = source.deliver(&ctx(), 2_000, ResolutionPreference::Raw).await;
        let all: String = delivery
            .items
            .iter()
            .map(|i| i.content.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            all.contains("[your work] you HOLD 1 card(s)"),
            "only the LIVE lease is held: {all}"
        );
        assert!(
            all.contains("wordstats tests") && !all.contains("you HOLD 2"),
            "the lapsed card must not count as held: {all}"
        );
        assert!(
            all.contains("[available work] 1 card(s)"),
            "the lapsed card is claimable work: {all}"
        );
        assert!(
            all.contains("claim lapsed (was YOURS) — claimable"),
            "the lapsed card renders its lapse honestly: {all}"
        );
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

    // what this catches: the exam-bleed regression (glass-boxed live 2026-07-10,
    // Hermes-8B OURS 38% < RAW 52%) — a room-BOUND board delivering into a turn
    // in a DIFFERENT context. A turn stamped with another room (or the eval
    // fork's nil room) must get an empty delivery, never this room's cards +
    // work/claim invitations; the SAME room still delivers; an UNSTAMPED ctx
    // (None — background/legacy) keeps pre-gate behavior.
    #[tokio::test]
    async fn room_bound_board_abstains_outside_its_room() {
        let home = uuid::Uuid::new_v4();
        let p = persona();
        let reader = Arc::new(StubReader::new(snapshot(vec![card(
            "write wordstats",
            CardState::Open,
            None,
        )])));
        let source = RoomBoardSource::new(p, reader).for_room(home);

        // Turn stamped with the SAME room → delivers.
        let same = RagContext::for_persona_in_room(p, 1_000, home);
        assert!(
            !source.deliver(&same, 500, ResolutionPreference::Raw).await.items.is_empty(),
            "same-room turn must still receive the board"
        );
        // Turn stamped with a DIFFERENT room → abstains.
        let other = RagContext::for_persona_in_room(p, 1_000, uuid::Uuid::new_v4());
        assert!(
            source.deliver(&other, 500, ResolutionPreference::Raw).await.items.is_empty(),
            "another room's turn must NOT receive this room's board"
        );
        // The eval fork's synthetic nil context → abstains (the exam-bleed fix).
        let exam = RagContext::for_persona_in_room(p, 1_000, uuid::Uuid::nil());
        assert!(
            source.deliver(&exam, 500, ResolutionPreference::Raw).await.items.is_empty(),
            "a synthetic exam context must NOT receive the room board"
        );
        // Unstamped ctx (None) → pre-gate behavior (delivers).
        let unstamped = RagContext::for_persona(p, 1_000);
        assert!(
            !source.deliver(&unstamped, 500, ResolutionPreference::Raw).await.items.is_empty(),
            "an unstamped ctx keeps legacy behavior"
        );
    }
}
