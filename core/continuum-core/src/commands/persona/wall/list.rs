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
    /// Posts to skip: pass the previous page's `next`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub after: Option<u32>,
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
    /// Currently-pinned posts on the wall, all pages.
    #[ts(type = "number")]
    pub total: u32,
    /// This page's posts, in published-time order.
    pub posts: Vec<PersonaWallPost>,
    /// Pass as `after` for the next page; absent on the last page.
    #[ts(optional, type = "number")]
    pub next: Option<u32>,
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

/// PURE: one page of `posts` (already in published-time order) from `after`.
fn page(
    posts: &[airc_core::doctrine::WallPostPublished],
    after: Option<u32>,
    limit: Option<u32>,
) -> (Vec<PersonaWallPost>, Option<u32>) {
    let start = (after.unwrap_or(0) as usize).min(posts.len()); // unwrap_or: no cursor = the first page
    let take = limit.map_or(DEFAULT_LIMIT, |l| (l as usize).clamp(1, MAX_LIMIT));
    let end = (start + take).min(posts.len());
    let next = (end < posts.len()).then_some(end as u32);
    (posts[start..end].iter().map(PersonaWallPost::from).collect(), next)
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
        let (page_posts, next) = page(&posts, p.after, p.limit);
        Ok(PersonaWallListResult {
            room_id: room.channel.as_uuid().to_string(),
            total: posts.len() as u32,
            posts: page_posts,
            next,
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

    // what this catches (card 61eed2dd): a wall read that drops provenance (the gap in
    // grounding), or pages that skip, repeat, or never end. Every post carries its id,
    // author, date and supersede link; pages cover the wall exactly once and the last
    // page has no `next`; limit is clamped, never zero.
    #[test]
    fn pages_cover_the_wall_once_with_provenance() {
        let wall: Vec<_> = (1..=7).map(post).collect();
        let (first, next) = page(&wall, None, None);
        assert_eq!(first.len(), DEFAULT_LIMIT);
        assert_eq!(next, Some(5));
        assert_eq!(first[1].supersedes.as_deref(), Some(uuid::Uuid::from_u128(1).to_string().as_str()));
        assert_eq!(first[0].published_at_ms, 1001);
        assert_eq!(first[0].author, uuid::Uuid::from_u128(7).to_string());
        let (rest, end) = page(&wall, next, None);
        assert_eq!(rest.iter().map(|p| p.body.as_str()).collect::<Vec<_>>(), ["post 6", "post 7"]);
        assert_eq!(end, None, "the last page ends the walk");
        assert_eq!(page(&wall, Some(99), None), (Vec::new(), None), "a stale cursor is empty, not a panic");
        assert_eq!(page(&wall, None, Some(0)).0.len(), 1, "limit 0 clamps to 1");
    }
}
