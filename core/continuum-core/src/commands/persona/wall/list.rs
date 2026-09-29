//! `persona/wall/list` — the READ verb for a room's wall (card 61eed2dd).
//!
//! The wall reached a citizen only as `[room-wall]` grounding, rendered without
//! dates, authors or ids, and its continuation pointed at `work/list`, which lists
//! cards: there was no verb that read the wall at all (2026-09-28, Codex's context
//! audit of Kimi's turns). This is that verb. It reads the SAME airc projection the
//! grounding reads (`wall_posts_in`, supersede chain applied), for a room the caller
//! names, as the caller's own citizen, and returns each post with its provenance, a
//! page at a time. History is untouched: superseded versions stay in the transcript.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::persona::PersonaAircRuntimeRegistry;
use crate::sdk_codegen::CommandError;

/// Posts per page when `limit` is omitted, and the most one page returns.
const DEFAULT_LIMIT: usize = 5;
const MAX_LIMIT: usize = 50;

/// Which room's wall to read, and which page.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PersonaWallListParams.ts"
)]
pub struct PersonaWallListParams {
    /// The room whose wall to read (id or name).
    pub room: String,
    /// Where to resume: pass the previous page's `next`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub after: Option<String>,
    /// Posts per page (default 5, at most 50).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub limit: Option<u32>,
}

/// One currently-pinned post, with the provenance grounding leaves out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PersonaWallPost.ts"
)]
pub struct PersonaWallPost {
    /// This version's id; pass it as `supersedes` to persona/wall/pin to edit it.
    pub post_id: String,
    /// The post's category label (plan, rules, agenda, ...).
    pub category: String,
    /// The peer that published this version.
    pub author: String,
    /// When this version was published (epoch ms).
    #[ts(type = "number")]
    pub published_at_ms: u64,
    /// The post this version replaced, if it is an edit.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub supersedes: Option<String>,
    /// The post body, verbatim.
    pub body: String,
}

/// One page of a room's wall.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PersonaWallListResult.ts"
)]
pub struct PersonaWallListResult {
    /// The room whose wall this is.
    pub room_id: String,
    /// Currently-pinned posts on the wall now, all pages. The wall can change
    /// between calls, so totals from different calls need not agree.
    #[ts(type = "number")]
    pub total: u32,
    /// This page's posts, in published-time order.
    pub posts: Vec<PersonaWallPost>,
    /// Pass as `after` for the next page; absent on the last page.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub next: Option<String>,
}

impl From<&airc_core::doctrine::WallPostPublished> for PersonaWallPost {
    fn from(post: &airc_core::doctrine::WallPostPublished) -> Self {
        Self {
            post_id: post.post_id.to_string(),
            category: post.category.clone(),
            author: post.published_by.as_uuid().to_string(),
            published_at_ms: post.published_at_ms,
            supersedes: post.supersedes.map(|u| u.to_string()),
            body: post.body.clone(),
        }
    }
}

/// Where a page ended: the last post's id (and time, for the message). A position
/// index would shift under a supersede between calls and silently skip a current post
/// (Codex on #4560); a post's own identity does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WallCursor {
    published_at_ms: u64,
    post_id: uuid::Uuid,
}

impl std::fmt::Display for WallCursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.published_at_ms, self.post_id)
    }
}

impl std::str::FromStr for WallCursor {
    type Err = CommandError;
    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let bad = || CommandError::Invalid(format!("after '{raw}' is not a wall cursor: pass a page's `next` unchanged"));
        let (ms, id) = raw.trim().split_once(':').ok_or_else(bad)?;
        Ok(Self {
            published_at_ms: ms.parse().map_err(|_| bad())?,
            post_id: uuid::Uuid::parse_str(id).map_err(|_| bad())?,
        })
    }
}

impl From<&airc_core::doctrine::WallPostPublished> for WallCursor {
    fn from(post: &airc_core::doctrine::WallPostPublished) -> Self {
        Self {
            published_at_ms: post.published_at_ms,
            post_id: post.post_id,
        }
    }
}

/// One page and where the next begins.
#[derive(Debug, PartialEq, Eq)]
struct Page {
    posts: Vec<PersonaWallPost>,
    next: Option<WallCursor>,
}

/// PURE: one page of `posts` (in published-time order) after `after`, which must name
/// a post that is still current. If it was superseded since the last page, the walk
/// is refused and restarts: posts can share a publish time, so no guess from the
/// timestamp can promise nothing current is skipped (Codex on #4560).
fn page(
    posts: &[airc_core::doctrine::WallPostPublished],
    after: Option<WallCursor>,
    limit: Option<u32>,
) -> Result<Page, CommandError> {
    let start = match after {
        None => 0,
        Some(c) => match posts.iter().position(|p| p.post_id == c.post_id) {
            Some(i) => i + 1,
            None => {
                return Err(CommandError::Invalid(format!(
                    "post {} (the cursor) is not on this wall now: it was superseded since \
                     your last page, or the cursor came from another room. Call again without \
                     `after` to read the wall from the start.",
                    c.post_id
                )))
            }
        },
    };
    let take = limit.map_or(DEFAULT_LIMIT, |l| (l as usize).clamp(1, MAX_LIMIT));
    let end = (start + take).min(posts.len());
    Ok(Page {
        posts: posts[start..end].iter().map(PersonaWallPost::from).collect(),
        next: (end < posts.len()).then(|| WallCursor::from(&posts[end - 1])),
    })
}

crate::action_command! {
    /// Read a room's wall: the currently-pinned posts, each with its post_id,
    /// category, author, date and the post it replaced, a page at a time.
    pub struct PersonaWallList {
        registry: PersonaAircRuntimeRegistry,
    }
    name: "persona/wall/list",
    access: AiSafe,
    params: PersonaWallListParams,
    output: PersonaWallListResult,
    run(this, ctx, p) => {
        if p.room.trim().is_empty() {
            return Err(CommandError::Invalid(
                "persona/wall/list: room is required: name the room whose wall to read".into(),
            ));
        }
        let airc = crate::modules::work::persona_airc(&this.registry, ctx, "wall commands")?;
        let room = crate::modules::room_resolve::resolve_room(&airc, Some(&p.room)).await?;
        let posts = airc_lib::Airc::wall_posts_in(&airc, &room, None)
            .await
            .map_err(|e| CommandError::Internal(format!("wall read failed: {e}")))?;
        let after = p.after.as_deref().map(str::parse::<WallCursor>).transpose()?;
        let page = page(&posts, after, p.limit)?;
        Ok(PersonaWallListResult {
            room_id: room.channel.as_uuid().to_string(),
            total: posts.len() as u32,
            posts: page.posts,
            next: page.next.map(|c| c.to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn post(n: u64) -> airc_core::doctrine::WallPostPublished {
        airc_core::doctrine::WallPostPublished {
            room_id: airc_core::RoomId::from_uuid(uuid::Uuid::nil()),
            post_id: uuid::Uuid::from_u128(n as u128),
            category: "plan".into(),
            body: format!("post {n}"),
            supersedes: (n > 1).then(|| uuid::Uuid::from_u128(n as u128 - 1)),
            published_by: airc_core::PeerId::from_uuid(uuid::Uuid::from_u128(7)),
            published_at_ms: 1000 + n,
        }
    }

    // what this catches (card 61eed2dd): a wall read that drops provenance, or pages
    // that repeat or never end. Every post carries id, author, date and supersede link;
    // an unchanged wall is walked exactly once and the last page has no `next`.
    #[test]
    fn an_unchanged_wall_is_walked_once_with_provenance() {
        let wall: Vec<_> = (1..=7).map(post).collect();
        let first = page(&wall, None, None).expect("first page");
        assert_eq!(first.posts.len(), DEFAULT_LIMIT);
        assert_eq!(first.posts[1].supersedes.as_deref(), Some(uuid::Uuid::from_u128(1).to_string().as_str()));
        assert_eq!(first.posts[0].published_at_ms, 1001);
        assert_eq!(first.posts[0].author, uuid::Uuid::from_u128(7).to_string());
        let cursor = first.next.expect("more posts");
        let parsed: WallCursor = cursor.to_string().parse().expect("round trip");
        assert_eq!(parsed, cursor);
        let rest = page(&wall, Some(cursor), None).expect("second page");
        assert_eq!(rest.posts.iter().map(|p| p.body.as_str()).collect::<Vec<_>>(), ["post 6", "post 7"]);
        assert_eq!(rest.next, None, "the last page ends the walk");
        assert_eq!(page(&wall, None, Some(0)).expect("page").posts.len(), 1, "limit 0 clamps to 1");
        assert!("12".parse::<WallCursor>().is_err(), "an old numeric offset is refused, not misread");
    }

    // what this catches (Codex on #4560): the cursor's post superseded between calls,
    // including when a still-current post shares its publish time (A and B at 1000, page 1
    // ends at A, A is edited). Resuming by time would skip B; the walk is refused with a
    // restart instruction instead, and nothing is silently skipped.
    #[test]
    fn a_superseded_cursor_is_refused_never_guessed_past() {
        let mut wall: Vec<_> = (1..=3).map(post).collect();
        wall[1].published_at_ms = wall[0].published_at_ms;
        let cursor = page(&wall, None, Some(1)).expect("page").next.expect("more");
        let edited: Vec<_> = wall.iter().filter(|p| p.post_id != cursor.post_id).cloned().collect();
        let refused = page(&edited, Some(cursor), Some(10)).expect_err("stale cursor");
        assert!(refused.to_string().contains("without `after`"), "{refused}");
        let restarted = page(&edited, None, Some(10)).expect("restart");
        assert_eq!(restarted.posts.first().map(|p| p.body.as_str()), Some("post 2"), "the tied post is still there");
    }

    // what this catches (Cormac on #4560): `next` and `supersedes` serialised as null
    // while the binding says optional, so a client looping on `next !== undefined` never
    // stops.
    #[test]
    fn absent_fields_are_omitted_on_the_wire() {
        let wall = vec![post(1)];
        let p = page(&wall, None, None).expect("page");
        let json = serde_json::to_value(PersonaWallListResult {
            room_id: "r".into(),
            total: 1,
            posts: p.posts,
            next: p.next.map(|c| c.to_string()),
        })
        .expect("serialises");
        assert!(json.get("next").is_none(), "{json}");
        assert!(json["posts"][0].get("supersedes").is_none(), "{json}");
    }
}
