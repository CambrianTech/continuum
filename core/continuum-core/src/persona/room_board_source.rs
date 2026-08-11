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
    /// The work board of `room` — folded by airc into a [`BoardSnapshot`].
    ///
    /// `room` is the id this source is BOUND to (`for_room`), i.e. the same
    /// value [`crate::persona::rag_budget::room_scope_allows`] gates on. Passing
    /// it is what makes gate and read one decision instead of two that merely
    /// coincide: before 2026-08-07 the read resolved airc's `current_room()`
    /// independently ("whatever my default subscription happens to be") while the
    /// gate checked a bound id the read never consulted. They agreed only because
    /// both were seeded from the same call at bootstrap — nothing prevented a gate
    /// that passed for room A while the board came from room B, and no probe would
    /// have fired.
    ///
    /// `None` means the caller genuinely has no binding and accepts the scope's
    /// current room (the CLI's intent). A `Some(id)` that the scope is not
    /// subscribed to is an ERROR, never a silent fall back to the default: handing
    /// a citizen a different room's board is the failure this parameter exists to
    /// make impossible.
    async fn work_board(&self, room: Option<uuid::Uuid>) -> Result<BoardSnapshot, AircError>;

    /// Published display names for the given peers — the DURABLE alias store,
    /// not the room roster.
    ///
    /// Deliberately not presence-based: the card owner most worth naming is a
    /// teammate who went down still holding a claim, and a presence roster
    /// cannot name exactly that peer. Joel, 2026-08-06: *"a persona could go
    /// down too. They should be able, like you are me, to claim a card,
    /// diagnose etc."*
    ///
    /// No default impl on purpose — a default returning "no names" would let a
    /// new reader silently render every teammate as hex, which is the defect
    /// this method exists to kill. Every implementor decides explicitly; an
    /// implementor with no alias store returns an empty map and the projection
    /// falls back to short ids (addressable, unlike "someone").
    async fn peer_names(
        &self,
        peers: &[airc_core::PeerId],
    ) -> std::collections::HashMap<airc_core::PeerId, String>;
}

/// `airc_lib::Airc` satisfies the reader contract directly. Orphan rule OK —
/// the trait is ours. Reads the complete board and snapshots it into the plain
/// [`BoardSnapshot`] this source renders. Same call the desktop-app projector
/// makes; the shared truth is airc's fold, not a shared continuum trait.
#[async_trait]
impl RoomBoardReader for airc_lib::Airc {
    async fn work_board(&self, room: Option<uuid::Uuid>) -> Result<BoardSnapshot, AircError> {
        let projection = match room {
            // BOUND: read exactly the room the gate approved. A miss is loud —
            // `NotSubscribed` — never a quiet slide to the default room, because
            // a citizen handed the wrong room's board cannot tell that from a
            // board that is genuinely empty. That indistinguishability is the
            // whole bug class ([[fail-loud-never-swallow]]).
            Some(id) => {
                let channel = airc_core::RoomId::from_uuid(id);
                let Some(resolved) = airc_lib::Airc::room_by_channel(self, channel).await? else {
                    return Err(AircError::NotSubscribed(format!(
                        "work board requested for room {id}, which this scope is not \
                         subscribed to — refusing to substitute the default room's board"
                    )));
                };
                airc_lib::Airc::project_room_work_board(
                    self,
                    &resolved,
                    airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE,
                )
                .await?
            }
            // UNBOUND: the caller genuinely means "my current room" (the CLI's
            // intent). Same behaviour as before this parameter existed.
            None => {
                airc_lib::Airc::work_board_complete(self, airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE)
                    .await?
            }
        };
        Ok(projection.snapshot())
    }

    /// One scan per distinct owner, exactly as `airc work board` resolves its
    /// own board (work_commands.rs) — N is small (distinct owners on one
    /// board), and `peer_alias` is page_recent-backed. A peer with no published
    /// alias is simply absent from the map; the projection renders its short
    /// id rather than inventing a name.
    async fn peer_names(
        &self,
        peers: &[airc_core::PeerId],
    ) -> std::collections::HashMap<airc_core::PeerId, String> {
        let mut names = std::collections::HashMap::new();
        for peer in peers {
            if let Ok(Some(alias)) = airc_lib::Airc::peer_alias(self, *peer).await {
                names.insert(*peer, alias);
            }
        }
        names
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
    /// Holder resolution is NOT done here — it lives in
    /// [`crate::persona::card_holder`], the ONE place every board surface
    /// (this source, the service-loop anchor, `work/list`, the CLI) answers
    /// "who holds this, is the hold live". Rendering it locally is what let
    /// five surfaces disagree, and why a peer's card read as an 8-hex prefix
    /// no teammate could recognize or reach out to.
    fn render(
        card: &airc_work::WorkCard,
        self_id: uuid::Uuid,
        now_ms: u64,
        names: &dyn crate::persona::card_holder::PeerNames,
    ) -> String {
        let id8: String = card.card_id.as_uuid().to_string().chars().take(8).collect();
        let held = crate::persona::card_holder::holder(card, self_id, now_ms, names);
        // The tag leads with what the card IS to someone deciding whether to take
        // it, not with the column it happens to sit in — a lapsed claim is takeable
        // work whose column still reads `Claimed`. See `CardHolder::state_tag`;
        // found live by Benchy on a board of 61 cards, 0 in state Open, 59 leases
        // stale — every available card read as taken.
        let owner = held.render();
        format!(
            "card {id8} [{state}] \"{title}\" ({prio:?}, {owner})",
            state = held.state_tag(card),
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
///
/// Now a one-line delegation to [`crate::persona::card_holder::hold_of`] — the
/// predicate itself moved there so the anchor, `work/list`, and this source
/// cannot drift apart on what "held" means. Kept as a named local so the
/// call sites below read as liveness questions rather than enum matches.
fn claim_is_live(card: &airc_work::WorkCard, now_ms: u64) -> bool {
    crate::persona::card_holder::hold_of(card, now_ms) == crate::persona::card_holder::Hold::Held
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

    fn expand_command(&self) -> Option<&'static str> {
        Some("work/list")
    }

    /// MEASURED 26 tokens: the `[board] you hold N card(s); M claimable.`
    /// headline, which is deliberately the first unit precisely so a
    /// prefix-take cannot halve it. 32 gives it headroom for larger counts.
    fn floor_tokens(&self) -> u32 {
        32
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
        let board = match self.reader.work_board(self.room_id).await {
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
        //
        // But SAY SO. "The board is empty" and "she never got a board" are
        // different facts and only one is knowable from an absent block — the
        // same law `room_scope_allows` follows, which is why the room gate was
        // diagnosable in one grep and THIS exit cost a night of guessing
        // (#331). A silent early-return in a grounding source is a hole in the
        // glass box. [[observability-as-substrate]]
        if board.cards.is_empty() {
            tracing::info!(
                probe_class = "rag.board.empty",
                source = SOURCE_ID,
                persona_id = %self.persona_id,
                bound_room = ?self.room_id,
                turn_room = ?ctx.airc_room.as_ref().map(|r| r.as_uuid()),
                "room-board delivered nothing: the READ SUCCEEDED and the board has zero cards"
            );
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
        // AVAILABLE-WORK SALIENCE (#122): unclaimed cards are work waiting for
        // someone to pick up. Computed here, before anything is rendered, because the
        // HEADLINE below needs both counts — see its comment for why that matters.
        let open: Vec<&airc_work::WorkCard> = board
            .cards
            .iter()
            // Unclaimed-and-Open, OR a non-terminal card whose claim LAPSED — an
            // expired lease is genuinely available work (claim-contention allows
            // takeover), and before 2026-08-03 lapsed cards appeared in NEITHER
            // "available" nor honestly-held: invisible as work, sticky as an
            // attractor.
            //
            // The predicate itself lives in `card_holder` and is shared with
            // `work/list` — this filter used to re-derive it here and excluded only
            // Merged|Closed, so `Review` cards (which `work/claim` refuses) were
            // advertised as available: 11 of the 58 offered on the live board
            // 2026-08-07. One claimability decision, one place.
            .filter(|c| crate::persona::card_holder::claimable_now(c, now_ms))
            .collect();

        // HEADLINE — the cheapest COMPLETE statement of this board's two facts, first,
        // so a prefix-take can never deliver half of them.
        //
        // The defect this exists for, read out of Asha's own capture 2026-08-06: her
        // window fit exactly ONE of 31 board units. Divisibility (b6429f583) meant she
        // got the longest fitting PREFIX — which was "[your work] you HOLD 1 card" — and
        // then "…30 more not shown". The "[available work] 8 card(s) are claimable" lead
        // was unit two, and died. So she reported "no open tasks" and was CORRECT about
        // what she could see, in a room where 8 cards were claimable. Four citizens spent
        // the day in that loop.
        //
        // Ordering alone does not fix it — flipping the leads just severs the other half.
        // Two facts that are only true TOGETHER have to be ONE unit, and that unit has to
        // be small enough to survive any budget that delivers grounding at all (~25
        // tokens, vs ~210 for the two detailed leads). Detail degrades; meaning does not.
        if !mine.is_empty() || !open.is_empty() {
            let headline = format!(
                "[board] you hold {held} card(s); {avail} claimable.",
                held = mine.len(),
                avail = open.len(),
            );
            let headline_tokens = estimate_tokens(&headline);
            if headline_tokens <= budget {
                tokens_used += headline_tokens;
                items.push(RagItem {
                    content: headline,
                    tokens: headline_tokens,
                    metadata: json!({
                        "kind": "board-headline",
                        "held_count": mine.len(),
                        "open_count": open.len(),
                    }),
                });
            }
        }

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

        // The detailed available-work lead (#122): unclaimed cards are work waiting for
        // someone to pick up. Glass-boxed live 2026-07-10: with hands proven and an
        // open card sitting on the board, both personas drifted to identity-monologue
        // chatter instead of noticing the available work — an unclaimed card, flat in
        // the list, out-salienced by nothing. Names their count + titles + how to pick
        // one up. A true structural fact she WEIGHS — it never says she must
        // ([[no-hardcoded-heuristics-to-steer-cognition]]). Whole board still follows
        // verbatim in airc's own order — this adds salience, it does not re-rank or
        // filter the board itself.
        if !open.is_empty() {
            // NEWEST FIRST, not board order (measured 2026-08-07).
            //
            // Only five titles are ever shown. Taken in board order, a card dispatched
            // MINUTES AGO lands at the end and is structurally unshowable: the citizen reads
            // an honest "N card(s) are claimable" above five titles that are all weeks old,
            // and the work someone just handed her is not among them. Measured live — three
            // freshly dispatched bench cards were in every citizen's board cache and in none
            // of their prompt windows, while the count was correct the whole time. The count
            // was never the lie; the sample was.
            //
            // Recency is the right key because a just-created card is the one most likely to
            // be waiting on THIS citizen right now, and because it is the only ordering under
            // which "someone dispatched you work" is reliably visible within one turn. Ties
            // keep board order, so a static board renders exactly as before.
            //
            // Bounded-window eviction: a chatty writer pushes the signal out of a fixed-size
            // view, and the diagnostic goes blind precisely when the board is busiest. The
            // five-title cap is fine; taking the OLDEST five was not.
            let mut newest: Vec<&&airc_work::WorkCard> = open.iter().collect();
            newest.sort_by(|a, b| b.created_at_ms.cmp(&a.created_at_ms));
            let titles = newest
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

        // Resolve every DISTINCT non-self owner on this board to a published
        // name, in ONE pass, before rendering. Joel's rule (2026-08-06): a card
        // must say WHO holds it — "someone" (or an 8-hex prefix that reads as
        // one) leaves a citizen unable to reach out, which turns a coordination
        // problem into a dead end. Owners with nothing published stay absent
        // from the map and render as their short id, which is still addressable.
        let mut owner_peers: Vec<airc_core::PeerId> = Vec::new();
        for card in &board.cards {
            if let Some(o) = card.owner {
                if o.as_uuid() != self.persona_id && !owner_peers.contains(&o) {
                    owner_peers.push(o);
                }
            }
        }
        let names = self.reader.peer_names(&owner_peers).await;

        let mut cards_delivered = 0usize;
        for card in &board.cards {
            let content = Self::render(card, self.persona_id, now_ms, &names);
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
                    // Serde, NOT `format!("{:?}")`. Debug output is a developer convenience
                    // with NO stability guarantee — renaming a variant silently changes it
                    // and nothing fails to compile. It also gave the substrate TWO string
                    // forms of one enum: `{:?}` wrote "Claimed" here while `work/list`'s
                    // serde wrote "claimed", so a reader that guessed wrong compared against
                    // a spelling that never occurs. Consumers parse this straight back into
                    // `CardState` and match on VARIANTS, so the compiler owns the mapping.
                    // Joel 2026-08-06: "use constants or enums so you can't make
                    // capitalization type issues. Use rust as it is meant to be used, for
                    // predictable behavior."
                    "state": card.state,
                    "owner": card.owner.map(|o| o.as_uuid().to_string()),
                    // Is the hold STILL GOOD? `state` alone cannot answer it: a card sits in
                    // `Claimed` forever while its lease quietly expires, so a consumer reading
                    // only the state sees "taken" for work that is free to take. That is the
                    // exact blindness that stalled every citizen on this node 2026-08-06 —
                    // 19 cards, all Claimed, all leases stale, all held by the four residents,
                    // and each of them read the board as "nothing available" and passed.
                    //
                    // Carried as a BOOLEAN FACT from the same `claim_is_live` the card line
                    // renders from, so the anchor and the line can never disagree about whether
                    // work is takeable. A consumer that wants "available" must not re-derive it
                    // from a string.
                    "claim_live": claim_is_live(card, now_ms),
                    "priority": card.priority,
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

    /// Stamp a fixture card's creation time — the ordering key the available-work
    /// lead samples by, so a test can express "this one was dispatched just now".
    fn created_at(mut c: WorkCard, ms: u64) -> WorkCard {
        c.created_at_ms = ms;
        c
    }

    // what this catches: a card dispatched MINUTES AGO being structurally unshowable.
    // Only five titles are ever rendered; taken in board order a fresh card lands at the
    // end and never appears, so the citizen reads an honest "N claimable" above five
    // titles that are all old. Measured live 2026-08-07: three freshly dispatched bench
    // cards sat in every citizen's board cache and in NONE of their prompt windows while
    // the count was correct throughout — the count was never the lie, the sample was.
    #[tokio::test]
    async fn freshly_dispatched_work_is_shown_not_merely_counted() {
        let mut cards: Vec<WorkCard> = (0..8)
            .map(|i| {
                created_at(
                    card(&format!("old backlog card {i}"), CardState::Open, None),
                    1_000_000 + i as u64,
                )
            })
            .collect();
        // Dispatched just now, and LAST in board order — the exact live shape.
        cards.push(created_at(
            card("bench coder-write-eval: sum_evens", CardState::Open, None),
            9_000_000_000,
        ));

        let reader = Arc::new(StubReader::new(snapshot(cards)));
        let source = RoomBoardSource::new(persona(), reader);
        let delivery = source.deliver(&ctx(), 4_000, ResolutionPreference::Raw).await;
        let lead = delivery
            .items
            .iter()
            .find(|i| i.metadata["kind"] == "available-work-lead")
            .expect("available-work lead");

        assert!(
            lead.content.contains("bench coder-write-eval"),
            "the just-dispatched card must be VISIBLE, not just counted:\n{}",
            lead.content
        );
        // The count stays honest — it always was. This fix is about the sample.
        assert_eq!(lead.metadata["open_count"], 9);
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
        /// Published aliases the stub knows, standing in for airc's durable
        /// alias store. Empty by default — an owner with no published name
        /// renders as its short id, the honest degradation.
        names: std::collections::HashMap<airc_core::PeerId, String>,
    }

    impl StubReader {
        fn new(board: BoardSnapshot) -> Self {
            Self {
                board,
                fail: Mutex::new(false),
                names: std::collections::HashMap::new(),
            }
        }
        fn with_name(mut self, peer: airc_core::PeerId, name: &str) -> Self {
            self.names.insert(peer, name.to_string());
            self
        }
        fn set_fail(&self, fail: bool) {
            *self.fail.lock().unwrap() = fail;
        }
    }

    #[async_trait]
    impl RoomBoardReader for StubReader {
        async fn work_board(
            &self,
            _room: Option<uuid::Uuid>,
        ) -> Result<BoardSnapshot, AircError> {
            if *self.fail.lock().unwrap() {
                return Err(AircError::UnknownPeer(airc_core::PeerId::new()));
            }
            Ok(self.board.clone())
        }

        async fn peer_names(
            &self,
            peers: &[airc_core::PeerId],
        ) -> std::collections::HashMap<airc_core::PeerId, String> {
            peers
                .iter()
                .filter_map(|p| self.names.get(p).map(|n| (*p, n.clone())))
                .collect()
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
        // Headline first, then the available-work lead, then the card list.
        assert_eq!(delivery.items.len(), 4);
        assert_eq!(delivery.items[0].metadata["kind"], "board-headline");
        assert_eq!(delivery.items[1].metadata["kind"], "available-work-lead");
        let cards: Vec<&RagItem> = delivery
            .items
            .iter()
            .filter(|i| i.metadata.get("card_id").is_some())
            .collect();
        assert_eq!(cards.len(), 2);
        assert!(cards[0].content.contains("Wire the projector"));
        assert!(cards[0].content.contains("[InProgress]"));
        // No alias published for this holder → the short id, which is still
        // addressable (work/claim and airc DM both take it). NEVER "someone".
        let owner8: String = holder.as_uuid().to_string().chars().take(8).collect();
        assert!(cards[0].content.contains(&owner8));
        assert!(!cards[0].content.contains("someone"));
        // An unclaimed card is surfaced as such — all owners visible on the board.
        assert!(cards[1].content.contains("unclaimed"));
        // serde, not Debug — one canonical wire form for the enum (see the json! above).
        assert_eq!(cards[1].metadata["state"], "open");
        assert!(delivery.continuation.is_none());
    }

    // what this catches: the board's two facts getting SEVERED by a prefix-take.
    // Read out of Asha's own capture 2026-08-06: her window fit exactly ONE of 31
    // board units, divisibility handed her the longest fitting prefix — "[your work]
    // you HOLD 1 card" — and "[available work] 8 claimable" was unit two and died.
    // She then reported "no open tasks" and was CORRECT about what she could see,
    // in a room with 8 claimable cards. Four citizens looped on that all day.
    // The FIRST unit must therefore state BOTH counts, and be small enough to
    // survive a budget that delivers grounding at all.
    #[tokio::test]
    async fn the_first_board_unit_states_both_counts_so_a_prefix_take_cannot_halve_it() {
        let me = persona();
        let mut held = card("The card I hold", CardState::Claimed, Some(airc_core::PeerId::from_uuid(me)));
        held.claim_expires_at_ms = Some(now_unix_ms() + 60_000);
        let open_a = card("Claimable one", CardState::Open, None);
        let open_b = card("Claimable two", CardState::Open, None);

        let reader = Arc::new(StubReader::new(snapshot(vec![held, open_a, open_b])));
        let source = RoomBoardSource::new(me, reader);
        let delivery = source.deliver(&ctx(), 2_000, ResolutionPreference::Raw).await;

        let first = &delivery.items[0];
        assert_eq!(first.metadata["kind"], "board-headline", "the headline must LEAD");
        assert!(first.content.contains("hold 1"), "{}", first.content);
        assert!(first.content.contains("2 claimable"), "{}", first.content);
        // Cheap enough that any budget delivering grounding at all delivers BOTH
        // facts — the detailed leads are ~10x this and are what should degrade.
        assert!(first.tokens <= 32, "headline must stay tiny, was {}", first.tokens);
    }

    // what this catches: THE rule Joel set on 2026-08-06 — "should never say
    // taken by 'someone', tell them WHO, otherwise they can't reach out. And a
    // persona could go down too." A peer-held card must render that peer's
    // PUBLISHED NAME, and a peer whose lease lapsed must still be named so a
    // citizen can coordinate instead of silently stealing the card. Board
    // owners resolve through the durable alias store, NOT the presence roster,
    // precisely so a teammate who went down still holding work stays nameable.
    #[tokio::test]
    async fn a_peers_card_names_the_peer_live_and_lapsed_never_bare_hex() {
        let asha = airc_core::PeerId::new();
        let mut live = card("Wire the projector", CardState::InProgress, Some(asha));
        live.claim_expires_at_ms = Some(now_unix_ms() + 60_000);
        let mut lapsed = card("Windows blocker #1", CardState::Claimed, Some(asha));
        lapsed.claim_expires_at_ms = Some(1_000_000); // 1970-adjacent — long expired

        let reader =
            Arc::new(StubReader::new(snapshot(vec![live, lapsed])).with_name(asha, "Asha"));
        let source = RoomBoardSource::new(persona(), reader);
        let delivery = source.deliver(&ctx(), 2_000, ResolutionPreference::Raw).await;
        let cards: Vec<&RagItem> = delivery
            .items
            .iter()
            .filter(|i| i.metadata.get("card_id").is_some())
            .collect();
        assert_eq!(cards.len(), 2);

        let asha8: String = asha.as_uuid().to_string().chars().take(8).collect();
        // Live hold: named, and the raw hex is GONE from the line.
        assert!(cards[0].content.contains("owner Asha"), "{}", cards[0].content);
        assert!(!cards[0].content.contains(&asha8), "{}", cards[0].content);
        // Lapsed hold: still names WHO held it (so she can reach out) AND says
        // it is takeable — the two facts that were missing while six citizens
        // read stale claims as active work and announced "no open tasks".
        assert!(cards[1].content.contains("Asha"), "{}", cards[1].content);
        assert!(cards[1].content.contains("claimable"), "{}", cards[1].content);
        assert!(!cards[1].content.contains(&asha8), "{}", cards[1].content);
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
        // Found by KIND, not by index: the headline now leads, and a test that
        // pins position breaks every time the delivery grows a unit.
        let lead = d
            .items
            .iter()
            .find(|i| i.metadata["kind"] == "available-work-lead")
            .expect("open cards must produce an available-work lead");
        assert_eq!(lead.metadata["open_count"], 2);
        assert!(lead.content.contains("[available work]"));
        assert!(lead.content.contains("Compile wordstats"));
        assert!(lead.content.contains("work/claim"));

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
