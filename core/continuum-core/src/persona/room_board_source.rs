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

use continuum_base::time::EpochMs;
use std::sync::Arc;

use airc_lib::AircError;
use airc_work::BoardSnapshot;
use async_trait::async_trait;
use serde_json::json;

use crate::persona::rag_budget::{ContinuationCursor, RagContext, RagDelivery, RagItem, RagSource, ResolutionPreference};

/// Source identifier — the deliberation faculty renders this delivery under a
/// `[room-kanban]` header (generic `[<source_id>]` projection). Distinct from
/// `active-work` (own claims) and `room-wall` (the wall, renamed from the
/// misleading `room-board` in #3874).
pub(crate) const SOURCE_ID: &str = "room-kanban";

/// Most cards this source will render in full, per turn.
///
/// NOT a token budget — a RELEVANCE bound. The per-card loop below used to be limited only
/// by whatever budget remained, so the board grew to fill the prompt: measured 2026-08-17,
/// 38 cards / 9,284 chars / 49.7% of an 18,697-char system prompt on a routine ambient turn.
/// That makes every citizen think slower as the backlog grows, which is exactly backwards
/// for a work board.
///
/// Sized from what it MUST cover, not picked: the `[your work]` and `[available work]` leads
/// each name up to 5 cards, so the detailed render must be able to carry both (10) or the
/// summaries would promise cards the detail then omits — a citizen seeing "you hold 5" above
/// a list of 3 would reasonably read the substrate as inconsistent. Two spare keeps a
/// just-created card visible in the same turn it arrives. Ten is the floor this cannot go
/// below without re-deriving the lead caps with it.
const MAX_RENDERED_CARDS: usize = 12;

/// Token estimate — the ONE canonical chars/4 estimator
/// (`cognition::token_budget`), shared by every RAG source so the replay
/// ledger's numbers match.
use continuum_base::token_estimate::estimate_prompt_tokens as estimate_tokens;

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
/// A board read slower than this is always marked (the pair of durations is the
/// distribution a stalled tick is measured against); faster reads mark once per room
/// per minute, enough to notice a hang without flooding the ledger.
const SLOW_BOARD_READ_MS: u64 = 100;

fn board_read_mark_due(room: uuid::Uuid) -> bool {
    static LAST: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<uuid::Uuid, u64>>> =
        std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    let now = EpochMs::now().millis();
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner()); // poisoned lock = read the last state, same policy as every lock in this crate
    let due = last.get(&room).is_none_or(|t| now.saturating_sub(*t) >= 60_000);
    if due {
        last.insert(room, now);
    }
    due
}

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
                // A BREADCRUMB BEFORE EACH PHASE, not one duration after both. This
                // read runs inside workspace.rs's 30s PERCEPTION_DEADLINE, so the ticks
                // worth diagnosing are exactly the ones whose future is CANCELLED — and
                // a cancelled future never reaches a probe placed after its await.
                // What these marks actually established, 2026-09-05: they EXONERATED
                // this read. Chasing a faculty rendered as `[room-board]` that pins at
                // the 30s deadline, I assumed it was this one. It is not — that
                // SOURCE_ID belongs to `wall_source.rs`. The proof is an ABSENCE: every
                // read that emitted `resolve_room` here also reached `complete`
                // (1.4-9.9s), while the 30s deadline events carry no mark at all, not
                // even the first. A faculty cancelled before the first line of this
                // branch is not stalling in this branch. Keep the marks: they are cheap,
                // and "which projection did the tick actually spend itself in" is a
                // question two sources with confusable names will raise again.
                // ONE MARK PER ROOM PER MINUTE unless the read is slow: the enter/complete
                // pair is a hang detector, and at three board reads a second (2026-09-13
                // 15:0xZ: 1,700 reads in ten minutes across five citizens) an unconditional
                // mark drowned the ledger in nothing.
                let mark_due = board_read_mark_due(id);
                if mark_due {
                    continuum_base::probe!(
                        class = "board.read.phase",
                        room = %id,
                        phase = "resolve_room",
                        "entering room_by_channel — if no project_board mark follows for this                      room, room resolution is where the read hung"
                    );
                }
                let resolve_started = std::time::Instant::now();
                let Some(resolved) = airc_lib::Airc::room_by_channel(self, channel).await? else {
                    return Err(AircError::NotSubscribed(format!(
                        "work board requested for room {id}, which this scope is not                          subscribed to — refusing to substitute the default room's board"
                    )));
                };
                let resolve_ms = resolve_started.elapsed().as_millis() as u64;
                if mark_due || resolve_ms > SLOW_BOARD_READ_MS {
                    continuum_base::probe!(
                        class = "board.read.phase",
                        room = %id,
                        phase = "project_board",
                        resolve_ms,
                        "room resolved; entering project_room_work_board — a missing complete                      mark for this room means the projection hung"
                    );
                }
                let project_started = std::time::Instant::now();
                let projection = airc_lib::Airc::project_room_work_board(
                    self,
                    &resolved,
                    airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE,
                )
                .await?;
                let project_ms = project_started.elapsed().as_millis() as u64;
                if mark_due || resolve_ms + project_ms > SLOW_BOARD_READ_MS {
                continuum_base::probe!(
                    class = "board.read.phase",
                    room = %id,
                    phase = "complete",
                    resolve_ms,
                    project_ms,
                    "both halves finished — the pair of durations is the distribution a                      stalled tick is measured against"
                );
                }
                projection
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
    /// FALLBACK room for UNSTAMPED contexts only (#443). A context-stamped turn
    /// reads the board of ITS OWN room (turn-parametric — see the room
    /// resolution in `deliver`); this bound id answers only when the context
    /// carries no room at all (background consolidation, legacy construction).
    /// Bound at assembly from the room the persona joined at bootstrap
    /// (`identity.default_room`). `None` = unscoped: an unstamped delivery
    /// reads the scope's current room, exactly the pre-gate behavior.
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
        board: &BoardSnapshot,
        self_id: uuid::Uuid,
        now_ms: u64,
        names: &dyn crate::persona::card_holder::PeerNames,
        include_submission: bool,
    ) -> String {
        let id8: String = card.card_id.as_uuid().to_string().chars().take(8).collect();
        let held = crate::persona::card_holder::holder(card, self_id, now_ms, names);
        // The tag leads with what the card IS to someone deciding whether to take
        // it, not with the column it happens to sit in — a lapsed claim is takeable
        // work whose column still reads `Claimed`. See `CardHolder::state_tag`;
        // found live by Benchy on a board of 61 cards, 0 in state Open, 59 leases
        // stale — every available card read as taken.
        let owner = held.render();
        let mut content = format!(
            "card {id8} [{state}] \"{title}\" ({prio:?}, {owner})",
            state = held.state_tag(),
            title = card.title,
            prio = card.priority,
        );
        if include_submission {
            if let Some(guidance) = Self::submission_guidance(card, board) {
                content.push('\n');
                content.push_str(&guidance);
            }
        }
        content
    }

    /// Current signed candidate state is independent of a potentially long title.
    /// Reuse the submission owner's next-action semantics and evidence address.
    fn submission_guidance(card: &airc_work::WorkCard, board: &BoardSnapshot) -> Option<String> {
        let candidate = crate::modules::work::submission::latest_submission(card)?;
        let reviews: Vec<_> = board.submission_reviews.iter().filter(|review| {
            review.card_id == card.card_id && review.submission_id == candidate.submission_id
        }).collect();
        let review = reviews.first()?;
        let outcomes: Vec<_> = reviews.iter().map(|review| review.outcome).collect();
        let next = crate::modules::work::submission::next_after_submission(
            Some(candidate.submission_id.as_uuid()), &outcomes,
            crate::recipe::benchmark_catalog::graded_at_submit(&card.title),
            Some(review.review_card_id),
        )?;
        Some(format!("{next} Read work/submission card_id={} submission_id={} for all {} signed verdicts, reviewers, and evidence.", card.card_id, candidate.submission_id, reviews.len()))
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

/// One held card in the `[your work]` lead: its id, title and state, and the absolute path
/// its work lives at on this node when there is one ([`crate::persona::card_checkout::checkout_path_for`],
/// the one answer to "where does this card's work live").
///
/// WHY (card 31a9fe2b; the M5, 2026-10-08/09): the card and the round's kickoff name only
/// `swe/<instance>/`, relative to a workspace she cannot see from every turn. So citizens
/// reconstructed the location from memory, 131 times walking `find /` and `ls ~` through
/// Joel's home dir, and one listed a `/Users/joel/continuum/swe` that never existed ~20
/// times. The path is now in front of her each turn she holds the card. With no checkout on
/// this node the line says nothing about one: an absence is not reported as a location.
fn held_card_line(card: &airc_work::WorkCard, checkout: Option<&std::path::Path>) -> String {
    let id8: String = card.card_id.as_uuid().to_string().chars().take(8).collect();
    let line = format!("  {id8}: \"{}\" [{:?}]", card.title, card.state);
    match checkout {
        Some(path) => format!("{line}\n    its work is at {}", path.display()),
        None => line,
    }
}

#[async_trait]
impl RagSource for RoomBoardSource {
    fn room_scope(&self) -> super::rag_budget::RagRoomScope {
        super::rag_budget::RagRoomScope::Activity
    }

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
        let working = crate::cognition::persona_workspace::acting_root_of(self.persona_id).is_some();
        self.deliver_with_workspace(ctx, budget, resolution, working)
            .await
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

impl RoomBoardSource {
    // The caller observes workspace custody; eligibility and rendering use the
    // same board snapshot regardless of whether open-deck detail is suppressed.
    async fn deliver_with_workspace(
        &self,
        ctx: &RagContext,
        budget: u32,
        resolution: ResolutionPreference,
        working: bool,
    ) -> RagDelivery {
        // Persona-scoped: a cross-persona ctx gets nothing (defense in depth,
        // same shape as the wall / active-work / roster sources).
        if ctx.persona_id != self.persona_id {
            return Self::empty();
        }
        // Room resolution — TURN-PARAMETRIC (#443, measured live 2026-08-15).
        // This source used to be BOUND to the persona's bootstrap room and the
        // gate abstained on any other stamped room — so a turn in a per-run
        // bench room took a WORK turn with NO board surface: the kickoff said
        // "card X is claimed for you" while the kanban block stayed silent
        // (every turn of the round fired `rag.room_gate.abstain` with
        // bound=academy, turn=bench-room). The board she needs is the board OF
        // THE ROOM SHE IS STANDING IN, so the stamped turn room WINS; the
        // bound room is only the fallback for UNSTAMPED contexts (background
        // consolidation, legacy construction — pre-gate behavior, unchanged).
        //
        // The invariants the old gate protected, restated stronger:
        //  - Cross-room leakage is impossible BY CONSTRUCTION: the reader
        //    refuses (`NotSubscribed`, fail-loud) any room this persona's OWN
        //    airc scope has not joined — enforced by the store, not by whoever
        //    remembered to bind correctly.
        //  - The eval fork's SYNTHETIC nil room still gets NOTHING, ever — an
        //    exam context must not see any board (the exam-bleed pin), and it
        //    must not fall back to the bound room either.
        let turn_room = ctx.airc_room.as_ref().map(|r| r.as_uuid());
        let effective_room = match turn_room {
            Some(t) if t.is_nil() => {
                continuum_base::probe!(
                    class = "rag.room_gate.abstain",
                    source = SOURCE_ID,
                    bound_room = ?self.room_id,
                    turn_room = %t,
                    persona_id = %ctx.persona_id,
                    "synthetic nil-room context — no board, and no fallback to the bound room"
                );
                return Self::empty();
            }
            Some(t) => Some(t),
            None => self.room_id,
        };

        // One airc call (the effective room's complete board). Failure is
        // non-fatal — empty delivery, cognition stays up (good-citizen doctrine).
        let board = match self.reader.work_board(effective_room).await {
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
                "room-kanban delivered nothing: the READ SUCCEEDED and the board has zero cards"
            );
            return Self::empty();
        }

        let mut items: Vec<RagItem> = Vec::new();
        let mut tokens_used: u32 = 0;
        let mut dropped: usize = 0;
        // One clock reading per delivery — every lease judgment in this block
        // (held / lapsed / available) agrees on the same instant.
        let now_ms = continuum_base::time::EpochMs::now().millis();

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
        // A rooted workspace suppresses the detailed open-deck lead, not the
        // cards' eligibility. Explicit claims remain available while working.
        let open: Vec<&airc_work::WorkCard> = board
            .cards
            .iter()
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
            // "on THIS room's board", not "you hold" bare (glass-boxed 2026-08-11,
            // Atlas's own capture): boards are per-room, but the bare phrasing
            // presents a room-scoped count as a fact about HER. She held three
            // live-leased cards on the academy board while a general-room turn
            // told her "you hold 0 card(s)" — so on her own time she reasonably
            // concluded she had no work and spoke yet another pass into the room
            // (the pass-spam Joel called out was this line lying, not her mind).
            // Three words scope the claim to what was actually read. The real fix
            // — her holds as an IDENTITY-level fact folded across her subscribed
            // rooms ([[personas-are-first-class-multi-room-subscribers]]) — needs
            // a cross-room reader verb and is a separate slice; this stops the
            // lie today without new reads.
            let headline = format!(
                "[board] this room only: you hold {held} card(s) here; {avail} claimable here.",
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
                        "open_lead_suppressed": working,
                    }),
                });
            }
        }

        // A title-only lead used to consume the budget before the separate card
        // containing its signed verdict could fit. Keep the focused candidate's
        // next action/evidence handle ahead of incidental board detail, within the
        // same budget. This is one atomic, truthful unit, not unbounded review prose.
        let mut focused_submission = None;
        if let Some(card) = crate::persona::work_focus::focus_workspace_card(mine.iter().copied()) {
            if let Some(content) = Self::submission_guidance(card, &board) {
                let tokens = estimate_tokens(&content);
                if tokens_used.saturating_add(tokens) <= budget {
                    tokens_used += tokens;
                    focused_submission = Some(card.card_id);
                    items.push(RagItem {
                        content,
                        tokens,
                        metadata: json!({ "kind": "focused-submission", "card_id": card.card_id.as_uuid().to_string() }),
                    });
                }
            }
        }

        if !mine.is_empty() {
            let titles = mine
                .iter()
                .take(5)
                .map(|c| {
                    let checkout = crate::persona::card_checkout::checkout_path_for(&self.persona_id, c);
                    held_card_line(c, checkout.as_deref())
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
            if tokens_used.saturating_add(lead_tokens) <= budget {
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
        if !working && !open.is_empty() {
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
                    let id8: String = c.card_id.as_uuid().to_string().chars().take(8).collect();
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
            if tokens_used.saturating_add(lead_tokens) <= budget {
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

        // RELEVANCE ORDER + A HARD CAP, not "however many the budget happens to fit".
        //
        // Measured 2026-08-17 on a live ambient turn (capture 762a806b): this loop rendered
        // 38 distinct cards into 9,284 chars — 49.7% of an 18,697-char system prompt, ~3,100
        // of the turn's ~12,286 prefill tokens. Every citizen paid it on every self-tick.
        //
        // Two things make that wrong rather than merely expensive:
        //
        // 1. IT IS REDUNDANT. The `[board]` headline and the `[your work]` / `[available
        //    work]` leads above already carry the counts and the top titles — and they were
        //    ADDED because this dump is unreliable under budget (see the Asha 2026-08-06
        //    note above). Rendering the summary AND the exhaustive list means the same facts
        //    are paid for twice, and the expensive copy is the one that gets truncated.
        // 2. IT SCALES THE WRONG WAY. Bounded only by remaining budget, the board grows to
        //    fill the prompt, so every citizen thinks SLOWER as the backlog grows. A team
        //    that accumulates work gets progressively less able to do it — the opposite of
        //    what a work board is for.
        //
        // So: order by what she can act on (her own holds first, then newest claimable —
        // the same recency argument the available-work lead already makes), take at most
        // `MAX_RENDERED_CARDS`, and let the leads plus `work/list` carry the tail. Counts in
        // the headline stay the TRUE totals; nothing here makes the board smaller than it is,
        // only shorter than it was.
        let mut ordered: Vec<&airc_work::WorkCard> = board.cards.iter().collect();
        ordered.sort_by_key(|c| {
            let mine_first = !(c.owner.map(|o| o.as_uuid()) == Some(self.persona_id));
            // Negate for newest-first without needing a reversed comparator.
            (mine_first, std::cmp::Reverse(c.created_at_ms))
        });
        let render_budget_cards = MAX_RENDERED_CARDS.min(ordered.len());

        let mut cards_delivered = 0usize;
        for card in ordered.into_iter().take(render_budget_cards) {
            let content = Self::render(card, &board, self.persona_id, now_ms, &names, focused_submission != Some(card.card_id));
            let tokens = estimate_tokens(&content);
            if tokens_used.saturating_add(tokens) > budget {
                // Budget exhausted — a truncated board is still truthful for the
                // cards it names. Count the drops so truncation is visible, not
                // silent. Atomic unit = one card; no continuation (see module doc).
                // Counted over CARDS, excluding the available-work lead item.
                dropped = board.cards.len() - cards_delivered;
                break;
                // (unchanged: `dropped` is measured against the TRUE board size, so a
                // budget-truncated render and a cap-truncated one report the same honest
                // "how many did she not see".)
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

        // Cap-truncation counts as dropped just like budget-truncation. Without this the
        // debug line would report dropped=0 on a capped board and the shortening would be
        // invisible in the one place an operator looks for it.
        if dropped == 0 && cards_delivered < board.cards.len() {
            dropped = board.cards.len() - cards_delivered;
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
}

#[cfg(test)]
mod tests;
