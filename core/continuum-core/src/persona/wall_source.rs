//! WallSource — reads the airc room WALL (pinned shared documents) and
//! packages it as a `[room-board]` grounding block.
//!
//! ### Why this source exists (the shared-layer grounding)
//!
//! The roster ([`RoomRosterSource`](super::room_roster_source)) grounds a
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
//!   contract) the wall is a BOARD of many posts, so this source packs
//!   whole posts greedily within budget and hands back a continuation
//!   cursor for the overflow.

use std::sync::Arc;

use airc_core::doctrine::WallPostPublished;
use airc_lib::AircError;
use async_trait::async_trait;

use crate::persona::rag_budget::{
    ContinuationCursor, RagContext, RagDelivery, RagItem, RagSource, ResolutionPreference,
};

/// Source identifier — the deliberation faculty renders this delivery
/// under a `[room-board]` header (generic `[<source_id>]` projection).
const SOURCE_ID: &str = "room-board";

/// Token estimate — the ONE canonical chars/4 estimator
/// (`cognition::token_budget`), shared by every RAG source so the replay
/// ledger's numbers match.
use crate::cognition::token_budget::estimate_prompt_tokens as estimate_tokens;

/// Abstract reader over the airc room wall. Production rides on
/// `airc_lib::Airc::wall_posts`; tests stub it without a daemon. Mirrors
/// the `AircDoctrineReader` / `AircRosterReader` rails.
#[async_trait]
pub trait WallReader: Send + Sync {
    /// The currently-pinned wall posts for this persona's current room, in
    /// published-time order (empty if the room has no wall). Slice 1
    /// surfaces ALL categories; a category-filtered variant can come later
    /// without changing this seam.
    async fn wall_posts(&self) -> Result<Vec<WallPostPublished>, AircError>;
}

/// `airc_lib::Airc` satisfies the reader contract directly. Orphan rule OK
/// — the trait is ours. `None` category filter = the whole board.
#[async_trait]
impl WallReader for airc_lib::Airc {
    async fn wall_posts(&self) -> Result<Vec<WallPostPublished>, AircError> {
        airc_lib::Airc::wall_posts(self, None).await
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

    /// Fit a single post to `budget` tokens, truncating the BODY (never the
    /// label) with a marker — used only when the FIRST post of a
    /// (sub)delivery doesn't fit whole, so partial guidance still grounds the
    /// persona rather than dropping the post entirely. Same
    /// `estimate_tokens(s) = s.chars()/4 + 1` arithmetic the doctrine source
    /// fits under: a string of `4*budget-4` chars estimates to exactly
    /// `budget`. Returns `None` when the budget can't carry the label +
    /// marker PLUS at least one char of real BODY — a block that's only the
    /// label and a truncation marker spends tokens to say nothing, strictly
    /// worse than no block (the bug an over-eager char-truncation hits when
    /// the category prefix alone fills a tiny budget).
    fn fit_post(post: &WallPostPublished, budget: u32) -> Option<String> {
        if budget == 0 {
            return None;
        }
        let rendered = Self::render(post);
        if estimate_tokens(&rendered) <= budget {
            return Some(rendered);
        }
        const MARKER: &str = "\n…[post truncated]";
        let label = Self::label(post);
        let reserved = label.chars().count() + MARKER.chars().count();
        let max_chars = (budget as usize).saturating_mul(4).saturating_sub(4);
        if max_chars <= reserved {
            // No room for the label + marker plus any real body content.
            return None;
        }
        let body_chars = max_chars - reserved;
        let body_prefix: String = post.body.chars().take(body_chars).collect();
        if body_prefix.is_empty() {
            return None;
        }
        Some(format!("{label}{body_prefix}{MARKER}"))
    }

    /// Build a [`RagItem`] for one pinned post from already-fitted content.
    fn item(post: &WallPostPublished, content: String) -> RagItem {
        let tokens = estimate_tokens(&content);
        RagItem {
            content,
            tokens,
            metadata: serde_json::json!({
                "post_id": post.post_id.to_string(),
                "category": post.category,
                "supersedes": post.supersedes.map(|u| u.to_string()),
                "published_by": post.published_by.as_uuid().to_string(),
                "published_at_ms": post.published_at_ms,
            }),
        }
    }

    /// Pack whole pinned posts from `start` greedily within `budget`.
    /// Returns the packed items, the tokens they consumed, and the index of
    /// the first UNPACKED post (`Some(i)` → more remain, hand back a cursor;
    /// `None` → the board is fully delivered).
    ///
    /// If the very first candidate doesn't fit whole, it is truncated with a
    /// marker so partial guidance still lands (mirrors the doctrine source);
    /// a budget too small to carry even a truncated post yields no block and
    /// no cursor — the budget regime simply can't carry this board.
    fn pack(
        posts: &[WallPostPublished],
        start: usize,
        budget: u32,
    ) -> (Vec<RagItem>, u32, Option<usize>) {
        let mut items: Vec<RagItem> = Vec::new();
        let mut used: u32 = 0;
        let mut i = start;
        while i < posts.len() {
            let rendered = Self::render(&posts[i]);
            let toks = estimate_tokens(&rendered);
            if used.saturating_add(toks) <= budget {
                // Whole post fits.
                items.push(Self::item(&posts[i], rendered));
                used += toks;
                i += 1;
            } else if items.is_empty() {
                // Nothing packed yet and this post overflows alone →
                // truncate its body to fill the budget, then stop. A
                // truncated post fills what remains; the rest is the
                // continuation's.
                if let Some(truncated) = Self::fit_post(&posts[i], budget) {
                    let item = Self::item(&posts[i], truncated);
                    used += item.tokens;
                    items.push(item);
                }
                i += 1;
                break;
            } else {
                // Already packed whole posts; leave the rest for the cursor.
                break;
            }
        }
        let next = if i < posts.len() { Some(i) } else { None };
        (items, used, next)
    }

    /// Wrap a packed slice into a delivery, minting a continuation cursor
    /// when `next` posts remain. The cursor's opaque carries the resume
    /// index; persona_id + source_id are the substrate identity guards.
    fn delivery(
        &self,
        items: Vec<RagItem>,
        used: u32,
        next: Option<usize>,
        resolution: ResolutionPreference,
    ) -> RagDelivery {
        let continuation = next.map(|idx| ContinuationCursor {
            persona_id: self.persona_id,
            source_id: SOURCE_ID.to_string(),
            opaque: serde_json::json!({ "next_index": idx }),
        });
        RagDelivery {
            source_id: SOURCE_ID.to_string(),
            items,
            tokens_used: used,
            continuation,
            resolution_used: resolution,
        }
    }
}

#[async_trait]
impl RagSource for WallSource {
    fn source_id(&self) -> &'static str {
        SOURCE_ID
    }

    fn expand_command(&self) -> Option<&'static str> {
        Some("work/list")
    }

    /// One wall post's title line. A pinned document's NAME is a complete
    /// statement — she knows the plan exists and can fetch it.
    fn floor_tokens(&self) -> u32 {
        32
    }

    async fn deliver(
        &self,
        ctx: &RagContext,
        budget: u32,
        resolution: ResolutionPreference,
    ) -> RagDelivery {
        let empty = |res| RagDelivery {
            source_id: SOURCE_ID.to_string(),
            items: Vec::new(),
            tokens_used: 0,
            continuation: None,
            resolution_used: res,
        };

        // Persona-scoped (defense in depth, same shape as the roster/doctrine).
        if ctx.persona_id != self.persona_id {
            return empty(ResolutionPreference::Placeholder);
        }
        // Room-scoped: the ONE shared gate (`room_scope_allows`) — probes every
        // abstain with both rooms named (see RoomBoardSource for the rationale).
        if !crate::persona::rag_budget::room_scope_allows(self.room_id, ctx, SOURCE_ID) {
            return empty(ResolutionPreference::Placeholder);
        }

        let posts = match self.reader.wall_posts().await {
            Ok(posts) => posts,
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    persona_id = %self.persona_id,
                    "wall_posts: read failed — empty delivery, cognition stays up"
                );
                return empty(ResolutionPreference::Placeholder);
            }
        };
        // No pinned posts → no block (normal: most rooms have an empty wall).
        if posts.is_empty() {
            return empty(resolution);
        }

        let (items, used, next) = Self::pack(&posts, 0, budget);
        if items.is_empty() {
            // Budget too small to carry even a truncated first post.
            return empty(resolution);
        }

        tracing::debug!(
            persona_id = %self.persona_id,
            budget,
            posts = posts.len(),
            delivered = items.len(),
            tokens = used,
            "wall_posts: deliver"
        );

        self.delivery(items, used, next, resolution)
    }

    async fn deliver_continuation(
        &self,
        ctx: &RagContext,
        cursor: ContinuationCursor,
        budget: u32,
    ) -> Option<RagDelivery> {
        // Substrate identity guards — refuse a cursor from another persona
        // or another source (per the ContinuationCursor contract).
        if cursor.persona_id != self.persona_id || cursor.source_id != SOURCE_ID {
            return None;
        }
        if ctx.persona_id != self.persona_id {
            return None;
        }
        let start = cursor.opaque.get("next_index").and_then(|v| v.as_u64())? as usize;

        // Re-read the wall: the projection is cheap (a transcript window) and
        // re-reading keeps the cursor honest against a wall edited between
        // turns, rather than caching a snapshot that can drift.
        let posts = self.reader.wall_posts().await.ok()?;
        if start >= posts.len() {
            return None;
        }

        let (items, used, next) = Self::pack(&posts, start, budget);
        if items.is_empty() {
            return None;
        }
        Some(self.delivery(items, used, next, ResolutionPreference::Raw))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airc_core::PeerId;
    use std::sync::Mutex;
    use uuid::Uuid;

    fn persona() -> Uuid {
        Uuid::parse_str("00000000-0000-0000-0000-000000000aaa").unwrap()
    }

    fn ctx() -> RagContext {
        RagContext::for_persona(persona(), 1_000_000)
    }

    fn post(category: &str, body: &str) -> WallPostPublished {
        WallPostPublished {
            room_id: airc_core::RoomId::new(),
            post_id: Uuid::new_v4(),
            category: category.to_string(),
            body: body.to_string(),
            supersedes: None,
            published_by: PeerId::new(),
            published_at_ms: 1_000_000,
        }
    }

    struct StubReader {
        posts: Vec<WallPostPublished>,
        fail: Mutex<bool>,
    }

    impl StubReader {
        fn new(posts: Vec<WallPostPublished>) -> Self {
            Self {
                posts,
                fail: Mutex::new(false),
            }
        }
        fn set_fail(&self, fail: bool) {
            *self.fail.lock().unwrap() = fail;
        }
    }

    #[async_trait]
    impl WallReader for StubReader {
        async fn wall_posts(&self) -> Result<Vec<WallPostPublished>, AircError> {
            if *self.fail.lock().unwrap() {
                return Err(AircError::UnknownPeer(PeerId::new()));
            }
            Ok(self.posts.clone())
        }
    }

    // what this catches: pinned wall posts surface as a delivery the brain
    // renders into the [room-board] grounding block — the shared-layer
    // grounding that lets a persona read the room's plan/instructions.
    #[tokio::test]
    async fn pinned_posts_surface() {
        let reader = Arc::new(StubReader::new(vec![
            post("plan", "Ship the wall grounding slice."),
            post("rules", "Be concise; cite the post you follow."),
        ]));
        let source = WallSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx(), 1_000, ResolutionPreference::Raw)
            .await;
        assert_eq!(delivery.items.len(), 2);
        assert!(delivery.items[0]
            .content
            .contains("Ship the wall grounding"));
        assert!(delivery.items[0].content.contains("[plan]"));
        assert_eq!(delivery.items[1].metadata["category"], "rules");
        assert!(delivery.continuation.is_none());
    }

    // what this catches: a room with NO pinned posts renders no block
    // (backwards-compatible; most rooms start with an empty wall).
    #[tokio::test]
    async fn empty_wall_delivers_nothing() {
        let reader = Arc::new(StubReader::new(vec![]));
        let source = WallSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx(), 1_000, ResolutionPreference::Raw)
            .await;
        assert!(delivery.items.is_empty());
        assert_eq!(delivery.tokens_used, 0);
        assert!(delivery.continuation.is_none());
    }

    // what this catches: a read failure degrades to empty, never panics —
    // cognition stays up if the wall subsystem is degraded.
    #[tokio::test]
    async fn read_error_returns_empty_no_panic() {
        let reader = Arc::new(StubReader::new(vec![post("plan", "body")]));
        reader.set_fail(true);
        let source = WallSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx(), 1_000, ResolutionPreference::Raw)
            .await;
        assert!(delivery.items.is_empty());
        assert_eq!(delivery.resolution_used, ResolutionPreference::Placeholder);
    }

    // what this catches: cross-persona ctx gets nothing (defense in depth).
    #[tokio::test]
    async fn cross_persona_ctx_returns_empty() {
        let reader = Arc::new(StubReader::new(vec![post("plan", "body")]));
        let source = WallSource::new(persona(), reader);
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

    // what this catches: a board too large for one budget packs whole posts,
    // hands back a continuation cursor, and the continuation resumes from
    // exactly where the first delivery stopped — no post lost, none doubled.
    #[tokio::test]
    async fn overflow_paginates_via_continuation() {
        // Each rendered post ("[c]\n" + 200 chars) ~= 51 tokens; budget 60
        // admits exactly one whole post per call.
        let body = "y".repeat(200);
        let reader = Arc::new(StubReader::new(vec![
            post("a", &body),
            post("b", &body),
            post("c", &body),
        ]));
        let source = WallSource::new(persona(), reader);

        let first = source.deliver(&ctx(), 60, ResolutionPreference::Raw).await;
        assert_eq!(first.items.len(), 1, "one whole post fits in budget 60");
        assert_eq!(first.items[0].metadata["category"], "a");
        let cursor = first.continuation.expect("more posts remain → cursor");
        assert_eq!(cursor.opaque["next_index"], 1);

        let second = source
            .deliver_continuation(&ctx(), cursor, 60)
            .await
            .expect("continuation resumes");
        assert_eq!(second.items.len(), 1);
        assert_eq!(second.items[0].metadata["category"], "b");
        assert_eq!(
            second.continuation.expect("still one more").opaque["next_index"],
            2
        );
    }

    // what this catches: an oversized FIRST post is truncated and NEVER
    // overspends the budget, across the whole budget regime — the same
    // marker-only-overspend bug the doctrine source's adversarial review
    // caught (PR #1651). Invariant: for ANY budget, either no block, or the
    // delivered tokens fit the budget AND carry real content.
    #[tokio::test]
    async fn oversized_first_post_never_overspends_across_budget_regime() {
        let big = "z".repeat(10_000);
        for budget in [1u32, 2, 3, 5, 6, 7, 8, 20, 100, 500] {
            let reader = Arc::new(StubReader::new(vec![post("plan", &big)]));
            let source = WallSource::new(persona(), reader);
            let delivery = source
                .deliver(&ctx(), budget, ResolutionPreference::Raw)
                .await;
            assert!(
                delivery.tokens_used <= budget,
                "budget {budget}: overspent ({} > {budget})",
                delivery.tokens_used
            );
            if let Some(item) = delivery.items.first() {
                let only_marker =
                    item.content.trim_start().starts_with('…') || !item.content.contains('z');
                assert!(
                    !only_marker,
                    "budget {budget}: delivered a content-free block: {:?}",
                    item.content
                );
            }
        }
    }

    // what this catches: a continuation cursor minted by another source (or
    // another persona) is refused — the substrate identity guard on
    // ContinuationCursor, so a stale/foreign cursor can't resume our wall.
    #[tokio::test]
    async fn continuation_refuses_foreign_cursor() {
        let reader = Arc::new(StubReader::new(vec![post("a", "x"), post("b", "y")]));
        let source = WallSource::new(persona(), reader);

        let wrong_source = ContinuationCursor {
            persona_id: persona(),
            source_id: "some-other-source".to_string(),
            opaque: serde_json::json!({ "next_index": 0 }),
        };
        assert!(source
            .deliver_continuation(&ctx(), wrong_source, 1_000)
            .await
            .is_none());

        let wrong_persona = ContinuationCursor {
            persona_id: Uuid::parse_str("00000000-0000-0000-0000-000000000bbb").unwrap(),
            source_id: SOURCE_ID.to_string(),
            opaque: serde_json::json!({ "next_index": 0 }),
        };
        assert!(source
            .deliver_continuation(&ctx(), wrong_persona, 1_000)
            .await
            .is_none());
    }
}
