//! WallSource — reads the airc room WALL (pinned shared documents) and
//! packages it as a `[room-board]` grounding block.
//!
//! ### Why this source exists (the shared-layer grounding)
//!
//! The roster (the `RosterViewState` source) grounds a
//! persona in WHO is present; the doctrine
//! ([`RoomDoctrineSource`](super::room_doctrine_source)) in WHAT KIND of
//! room it is (the participation contract). The WALL grounds it in the
//! room's living shared documents — the plan, the coding instructions,
//! the agenda, the principles, the recipe. These are the SAME airc rows a
//! human edits on the room wall (`airc publish --room …`) and a widget
//! renders: one shared data layer, two faces. The persona reads exactly
//! what a teammate pinned; no continuum-side copy, no drift.
//!
//! airc already owns this: `Airc::wall_posts` walks the recent transcript,
//! applies the supersede chain, and returns the currently-pinned
//! [`WallPostPublished`] posts in published-time order. This source is the
//! injection of those posts into the persona's grounding, routed through
//! the same RAG-grounding + capture/replay path as the roster and
//! doctrine. Thin continuum: we read airc's wall, we don't invent a
//! pinned-docs concept. See
//! [[docs/grid/AIRC-NATIVE-IDENTITY-ROOMS-SECURITY.md]] §5 and
//! [[airc-generic-per-user-room-state]].
//!
//! ### Wall vs doctrine — distinct airc streams, no dedup
//!
//! `room_doctrine` (the operating contract) and `wall_posts` (the pinned
//! board) are SEPARATE airc event streams. A room MAY pin a `"doctrine"`-
//! category wall post AND publish a `RoomDoctrinePublished`; surfacing both
//! is correct — the substrate stays dumb and never second-guesses what the
//! room operator chose to pin where. A continuum-side dedup heuristic
//! between the two would be exactly the kind of output-puppeteering the
//! doctrine forbids ([[no-hardcoded-heuristics-to-steer-cognition]]).
//!
//! ### Doctrine alignment
//!
//! - [[substrate-is-a-good-citizen-on-the-host]]: a failed/absent wall
//!   read returns an empty delivery — cognition stays up; a room with no
//!   pinned posts simply renders no block.
//! - Persona-scoped at construction (defense in depth, same as the roster
//!   + doctrine + engram sources).
//! - Enriching framing, NOT a participation gate: the wall shapes HOW a
//!   persona works in the room (the plan, the instructions), it does not
//!   decide WHETHER it speaks (that is doctrine). Bound as a
//!   defer-tolerant grounding faculty, like the active-work + workspace-map
//!   sources.
//! - Atomic unit = ONE pinned post. Unlike the doctrine (a single current
//!   contract) the wall is a BOARD of many posts, so this source delivers
//!   every pinned post as its own keyed item (key = post id) with a summary
//!   and a pointer; the prompt view's one allocation picks each level.

use std::sync::Arc;

use airc_core::doctrine::WallPostPublished;
use airc_lib::AircError;
use async_trait::async_trait;

use crate::cognition::rag_budget::{RagContext, RagDelivery, RagItem, RagSource};

use crate::cognition::faculty_ids::WALL;

/// Abstract reader over the airc room wall. Production rides on
/// `airc_lib::Airc::wall_posts`; tests stub it without a daemon. Mirrors
/// the `AircDoctrineReader` / `AircRosterReader` rails.
#[async_trait]
pub trait WallReader: Send + Sync {
    /// The currently-pinned wall posts for this persona's current room, in
    /// published-time order (empty if the room has no wall). Slice 1
    /// surfaces ALL categories; a category-filtered variant can come later
    /// without changing this seam.
    ///
    /// `room` is the id this source is BOUND to (`for_room`) — the same value
    /// [`crate::cognition::rag_budget::room_scope_allows`] gates on, exactly as
    /// [`crate::persona::room_board_source::RoomBoardReader::work_board`]
    /// takes it. Passing it makes gate and read ONE decision instead of two
    /// that merely tend to agree. `None` = unscoped legacy construction.
    async fn wall_posts(&self, room: Option<uuid::Uuid>)
        -> Result<Vec<WallPostPublished>, AircError>;

    /// Each card on the bound room's work board, as (title, wire state), so a card-ledger
    /// post can name the card it is about (card 4cfd4530). `None` when the board could not be
    /// read (or this reader has none): ledgers then render verbatim. An EMPTY map is a board
    /// that was read and holds no cards, and only that may say a card is off the board
    /// (BigMama on #4959: one empty map for both told her every card was gone).
    ///
    /// REQUIRED, no default (the 5090, 2026-10-12, build 6153): with a `None` default, the two
    /// PRODUCTION readers (`PersonaAircRuntime`, `AircHandleAdapter`) compiled without it and
    /// silently answered "board unread", so #4959's labels and #4962's concluded-ledger drop
    /// never ran on a persona's turn: Kimi's wall still carried a merged card's ledger,
    /// unlabelled, while every test passed against a stub that supplied labels directly. A
    /// reader now states what it knows about the board, so the next one cannot opt out by
    /// omission.
    async fn card_labels(&self, room: Option<uuid::Uuid>) -> Option<CardLabels>;
}

/// The work board's cards by id: (title, state).
pub type CardLabels = std::collections::HashMap<uuid::Uuid, (String, airc_work::CardState)>;

/// PURE: the card ledgers a persona's context carries, from the board as read. A ledger
/// whose card is CONCLUDED on this room's board (merged or closed, `work::is_concluded`)
/// leaves the context; every other ledger leads with one line naming the card it is
/// about (short id, title, board state), or that it is not on this room's board. Other
/// categories, and an unreadable ledger, pass through verbatim. Stored posts are never
/// touched: this shapes the copy the context renders, and a concluded card's ledger stays
/// readable on the wall and by id.
///
/// Why the concluded ones leave (Kimi's call from inside, card 7c8c44a1, 2026-10-11): 10 of
/// the 14 ledgers on her wall (14,386 of 25,802 chars) were for merged or closed cards,
/// re-prefilled every turn while her history was cut to fit. Their live value was "don't
/// re-take a finished lane" (the board says that) and provenance (git and the submission
/// record, reachable by id). A ledger for a card still open, claimed, blocked or in review
/// is her resumption state and stays. This is her decision about her own context, carried
/// out by the substrate (PERSONA-COGNITION-PIPELINE.md §5), not a relevance heuristic.
///
/// Why each kept ledger is labelled (card 4cfd4530): her failed work/submit on 27f133e4
/// passed a staged revision lifted from another card's ledger (1f58fc16) that sat beside
/// her active card's, labelled only `[card-ledger]` with the card's id buried in JSON.
///
/// Board unread (`None`): every post renders verbatim. Nothing is dropped and nothing is
/// labelled, since a board that could not be read cannot say a card is over or gone.
fn ledgers_for_context(posts: Vec<WallPostPublished>, labels: Option<&CardLabels>) -> Vec<WallPostPublished> {
    let Some(labels) = labels else {
        return posts;
    };
    posts
        .into_iter()
        .filter_map(|mut post| {
            if post.category != crate::experience::ledger::LEDGER_WALL_CATEGORY {
                return Some(post);
            }
            let Ok(ledger) = serde_json::from_str::<crate::experience::ledger::CardLedger>(&post.body) else {
                return Some(post); // an unreadable ledger renders as it is, unlabelled
            };
            let short = airc_core::shown_id::shown_form(ledger.card_id);
            let head = match labels.get(&ledger.card_id) {
                Some((_, state)) if crate::cognition::card_facts::is_concluded(state) => return None,
                Some((title, state)) => {
                    format!("ledger of card {short} \"{title}\" ({})\n", crate::cognition::card_facts::state_str(state))
                }
                None => format!("ledger of card {short} (not on this room's board)\n"),
            };
            post.body = head + &post.body;
            Some(post)
        })
        .collect()
}

/// `airc_lib::Airc` satisfies the reader contract directly. Orphan rule OK
/// — the trait is ours. `None` category filter = the whole board.
#[async_trait]
impl WallReader for airc_lib::Airc {
    async fn wall_posts(
        &self,
        room: Option<uuid::Uuid>,
    ) -> Result<Vec<WallPostPublished>, AircError> {
        match room {
            // BOUND: read exactly the room the gate approved, via the same
            // room_by_channel -> *_in pair the work board uses. This deliberately
            // does NOT go through `Airc::wall_posts`, whose first act is
            // `current_room().await` — resolving "whatever room I happen to point
            // at" when the caller already KNOWS the room, and returning a plausible
            // wall for the wrong one when they differ ([[fail-loud-never-swallow]]).
            //
            // Measured 2026-09-06 (canary 45a852a9b) and the reason this is not
            // merely tidier: the wall read took 143,128ms and 147,971ms, or never
            // returned (wall.read.phase enter=4 exit=2), CONSTANT across a
            // 3,985-event room and a room with ZERO events — so it was never a
            // volume problem. The work board, through the SAME daemon in the SAME
            // process, returned in ~0ms 127 times out of 127. The phase marks below
            // are what will say whether the remaining stall is room resolution or
            // the projection itself; the board's identical marks already exonerate
            // its half.
            Some(id) => {
                let channel = airc_core::RoomId::from_uuid(id);
                continuum_base::probe!(
                    class = "wall.read.phase",
                    room = %id,
                    phase = "resolve_room",
                    "entering room_by_channel — no `project_wall` mark for this room means \
                     room resolution is where the wall read hung"
                );
                let resolve_started = std::time::Instant::now();
                let Some(resolved) = airc_lib::Airc::room_by_channel(self, channel).await? else {
                    return Err(AircError::NotSubscribed(format!(
                        "wall requested for room {id}, which this scope is not subscribed \
                         to — refusing to substitute the default room's wall"
                    )));
                };
                let resolve_ms = resolve_started.elapsed().as_millis() as u64;
                continuum_base::probe!(
                    class = "wall.read.phase",
                    room = %id,
                    phase = "project_wall",
                    resolve_ms,
                    "room resolved; entering wall_posts_in — a missing `complete` mark for \
                     this room means the projection hung"
                );
                let project_started = std::time::Instant::now();
                let posts = airc_lib::Airc::wall_posts_in(self, &resolved, None).await?;
                continuum_base::probe!(
                    class = "wall.read.phase",
                    room = %id,
                    phase = "complete",
                    resolve_ms,
                    project_ms = project_started.elapsed().as_millis() as u64,
                    "both halves finished — the pair of durations is the distribution a \
                     stalled wall read is measured against"
                );
                Ok(posts)
            }
            // Unscoped legacy/test construction: pre-binding behaviour.
            None => airc_lib::Airc::wall_posts(self, None).await,
        }
    }

    /// The bound room's work board through the same room_by_channel -> *_in pair the wall
    /// read uses (the board returns in ~0 ms where the wall can stall). Any failure is `None`:
    /// ledgers then read as before, never as cards that are gone.
    async fn card_labels(&self, room: Option<uuid::Uuid>) -> Option<CardLabels> {
        let id = room?;
        let resolved = airc_lib::Airc::room_by_channel(self, airc_core::RoomId::from_uuid(id)).await.ok()??;
        let board = airc_lib::Airc::work_board_in(self, &resolved).await.ok()?;
        Some(board
            .snapshot()
            .cards
            .iter()
            .map(|card| (card.card_id.as_uuid(), (card.title.clone(), card.state)))
            .collect())
    }
}

/// WallSource — persona-bound, reads the room wall from any [`WallReader`].
pub struct WallSource {
    persona_id: uuid::Uuid,
    /// The room whose wall this source grounds — see the room gate in `deliver`
    /// and [`for_room`](Self::for_room). `None` = unscoped (legacy/test
    /// construction): pre-gate behavior.
    room_id: Option<uuid::Uuid>,
    reader: Arc<dyn WallReader>,
}

impl WallSource {
    pub fn new(persona_id: uuid::Uuid, reader: Arc<dyn WallReader>) -> Self {
        Self {
            persona_id,
            room_id: None,
            reader,
        }
    }

    /// Bind this source to the room its reader answers for, so a context-stamped
    /// turn in ANY other context (another room, the eval fork's nil room) gets an
    /// empty delivery instead of this room's wall (the exam-bleed fix).
    /// [[identity-context-session-three-axes]]
    pub fn for_room(mut self, room_id: uuid::Uuid) -> Self {
        self.room_id = Some(room_id);
        self
    }

    /// The per-post category label prefix. The block header is already
    /// `[room-board]`; inside it, each post is labelled by its airc category
    /// (`plan`, `rules`, `agenda`, …) so the persona can tell a plan from a
    /// rule.
    fn label(post: &WallPostPublished) -> String {
        format!("[{}]\n", post.category)
    }

    /// Render one pinned post as a grounding unit: `[category]\n{body}`. The
    /// body is rendered verbatim (markdown or JSON — the source never parses
    /// it).
    fn render(post: &WallPostPublished) -> String {
        format!("{}{}", Self::label(post), post.body)
    }

    /// One pinned post as an item: whole `[category]\n{body}`, keyed by its post id.
    /// Summary = `[category] <first line>` (a line boundary, never a mid-text cut), only
    /// when the post has more than that line; pointer = the post id and the verb that
    /// lists the wall again.
    fn item(post: &WallPostPublished) -> RagItem {
        let content = Self::render(post);
        let first_line = post.body.lines().map(str::trim).find(|l| !l.is_empty());
        let summary = first_line
            .map(|line| format!("[{}] {line}", post.category))
            .filter(|summary| summary.len() < content.len());
        let mut item = RagItem::whole(
            post.post_id.to_string(),
            content,
            serde_json::json!({
                "post_id": post.post_id.to_string(),
                "category": post.category,
                "supersedes": post.supersedes.map(|u| u.to_string()),
                "published_by": post.published_by.as_uuid().to_string(),
                "published_at_ms": post.published_at_ms,
            }),
        )
        .with_pointer(format!("[wall post {} ({}): persona/wall/list]", post.post_id, post.category));
        if let Some(summary) = summary {
            item = item.with_summary(summary);
        }
        item
    }
}

#[async_trait]
impl RagSource for WallSource {
    fn source_id(&self) -> &'static str {
        WALL
    }

    fn expand_command(&self) -> Option<&'static str> {
        // The verb that pages THIS wall: a room's pinned posts (`persona/wall/list`).
        // It named `work/list`, the work board, so a cut wall sent her to the wrong
        // surface (Codex root, review of #5016).
        Some("persona/wall/list")
    }

    async fn deliver(&self, ctx: &RagContext) -> RagDelivery {
        let empty = || RagDelivery::empty(WALL);

        // Persona-scoped (defense in depth, same shape as the roster/doctrine).
        if ctx.persona_id != self.persona_id {
            return empty();
        }
        // ROOM-SCOPED, RESOLVED FROM THE TURN — not from the room this source was
        // bound to at construction. `supervisor.rs` builds it with
        // `.for_room(identity.default_room)`, so gating on `self.room_id` meant the
        // wall abstained on EVERY turn the citizen took anywhere else: the same
        // defect as the roster's (#3862), in a second source, which #3879 fixed only
        // for the roster. Measured live after that fix landed — with the roster
        // silent, this one was the sole surviving `rag.room_gate.abstain`,
        // bound=#general turn=#continuum, exactly as predicted (#3883).
        //
        // The shape is copied from `room_board_source` rather than reinvented, nil
        // arm included: a synthetic nil room (the eval fork) gets NOTHING and must
        // not fall back to the bound room, or an exam sees another room's wall.
        let effective_room = match ctx.airc_room.as_ref().map(|r| r.as_uuid()) {
            Some(turn) if turn.is_nil() => {
                continuum_base::probe!(
                    class = "rag.room_gate.abstain",
                    source = WALL,
                    bound_room = ?self.room_id,
                    turn_room = %turn,
                    persona_id = %ctx.persona_id,
                    "synthetic nil-room context — no wall, and no fallback to the bound room"
                );
                return empty();
            }
            Some(turn) => Some(turn),
            // An UNSTAMPED context (background consolidation, an idle tick) keeps the
            // bound room: it is the only room this source can honestly speak for, and
            // it is what the pre-#3883 behaviour was for every turn.
            None => self.room_id,
        };

        // BREADCRUMB BEFORE THE READ, for the same reason as RoomBoardSource: this
        // runs under workspace.rs's 30s PERCEPTION_DEADLINE, and the ticks worth
        // diagnosing are the CANCELLED ones, which never reach a probe placed after
        // the await. Measured 2026-09-05: `room-board` (this source's id then) is
        // the slowest faculty, pinned at 30.00s — on 45a852a9b, 8 of 8 ticks.
        //
        // CORRECTED 2026-09-06, because the first explanation here was wrong and the
        // way it was wrong is the useful part. This comment used to say the cost was
        // `room_transcripts_since` paging the whole transcript from a zero cursor with
        // no cache. That is TRUE of the code and NOT the cause: the stall is CONSTANT
        // at ~30s across a 3,985-event room AND a room with ZERO events, and no
        // quantity explains a constant. The real magnitude was hidden by this very
        // deadline — the two reads that completed took 143,128ms and 147,971ms, so
        // every 30s figure ever quoted for this faculty is floor(true_cost, 30s).
        //
        // The control that localises it: the work board rides the SAME daemon in the
        // SAME process and returns in ~0ms, 127 times out of 127. So it is not the
        // daemon, not the IPC, not SQLite (the (room_id, lamport, event_id) index
        // exists and EXPLAIN shows SEARCH, not SCAN). It is a hang in the wall path
        // specifically, which is why this source now takes the bound-room branch
        // above rather than `Airc::wall_posts` and its `current_room()` resolution.
        continuum_base::probe!(
            class = "wall.read.phase",
            persona = %self.persona_id,
            phase = "enter",
            "entering wall_posts — no `exit` mark for this persona means the wall read              is where the tick was cancelled"
        );
        let wall_started = std::time::Instant::now();
        let posts = match self.reader.wall_posts(effective_room).await {
            Ok(posts) => posts,
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    persona_id = %self.persona_id,
                    "wall_posts: read failed — empty delivery, cognition stays up"
                );
                return empty();
            }
        };
        continuum_base::probe!(
            class = "wall.read.phase",
            persona = %self.persona_id,
            phase = "exit",
            wall_ms = wall_started.elapsed().as_millis() as u64,
            posts = posts.len(),
            "wall read returned — this duration is what the 30s deadline is spent on              when it is spent here"
        );
        // No pinned posts → no block (normal: most rooms have an empty wall).
        if posts.is_empty() {
            return empty();
        }

        // Ledgers, standings, recipes, child announcements and activity bundles are
        // append-only records whose owners say newest wins: only the records still in
        // force spend the context budget (one rule, `experience::wall_records`).
        let posts = ledgers_for_context(
            crate::experience::wall_records::current_wall_records(posts),
            self.reader.card_labels(effective_room).await.as_ref(),
        );
        // Posts a concluded-ledger rule dropped can leave the board empty.
        if posts.is_empty() {
            return empty();
        }
        let items: Vec<RagItem> = posts.iter().map(Self::item).collect();

        tracing::debug!(
            persona_id = %self.persona_id,
            posts = posts.len(),
            "wall_posts: deliver"
        );

        RagDelivery::of(WALL, items, false)
    }

}

#[cfg(test)]
mod tests;
