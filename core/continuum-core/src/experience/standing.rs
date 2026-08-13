//! A room's STANDING — is this activity still live, and may it be deleted.
//!
//! Sits beside [`recipe`](super::recipe) (what type of activity this is) and
//! [`membership`](super::membership) (who is party to it): standing is the
//! instance's lifecycle state. Recipe is the content-type, the room is the
//! content, and this is that content item's status.
//!
//! # Archived is a declaration, never a deletion
//!
//! Joel, 2026-08-07, after I designed an auto-retire and had to be corrected:
//!
//! > "Think about how tragic it would be when a room goes away on its own or a
//! > benchmark can't be read because you killed the room."
//!
//! A room is its activity's durable record — a benchmark run's room IS the
//! evidence of that run. There is no garbage collector for rooms. `archived`
//! changes nothing about what can be READ; it says the activity is concluded so
//! it stops **recruiting attention**. Joel again, same conversation:
//!
//! > "a concluded activity must READ as concluded and stop pulling people in,
//! > while staying completely readable forever. Fix the pull, never the record."
//!
//! # Why this lives here and not in the command module that writes it
//!
//! The projection below is the ONE rule for turning a room's wall into a
//! standing. It was born private inside `modules::activity`, reachable only
//! through an `async fn` over `&Airc` — which is exactly why nothing but the
//! command that wrote it could ever read it back, and why `archived` shipped as
//! a fact no code consumed. A pure function over already-fetched posts can be
//! called from a command handler, from a persona's wake gate, or from a test,
//! and there is still only one place that decides what a standing means.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The wall category carrying a room's standing.
///
/// Standing is a fact every participant must agree on, so it rides the shared
/// wall with last-wins `supersedes`, not per-peer scoped state.
pub const STANDING_WALL_CATEGORY: &str = "standing";

/// A room's declared standing. Absent entirely on a room nobody has ever marked —
/// which is the ordinary case and means "live, unprotected".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/experience/RoomStanding.ts")]
pub struct RoomStanding {
    /// Concluded: still fully readable, but it should stop recruiting attention —
    /// no longer offered as somewhere to pick up work, and it no longer wakes a
    /// citizen for ambient traffic. NOT deleted, NOT hidden.
    #[serde(default)]
    pub archived: bool,
    /// Refuses deletion. Benchmark rooms and anything else whose record is
    /// load-bearing get this, so a stray delete cannot take the evidence with it.
    #[serde(default)]
    pub protected: bool,
    /// Why — free text from whoever set it. A standing change with no reason is a
    /// mystery to the next reader.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note: Option<String>,
}

/// Why a wall could not be turned into a standing.
///
/// One variant, because there is exactly one way this fails: a post exists under
/// the standing category and this build cannot read it.
#[derive(Debug, Clone, thiserror::Error)]
#[error(
    "room standing is present but unreadable ({source_message}) — refusing to guess, \
     because guessing would silently drop whatever a newer client recorded"
)]
pub struct StandingParseError {
    /// The underlying serde message, kept verbatim so the operator sees which
    /// field disagreed rather than just "unreadable".
    pub source_message: String,
}

/// Project a room's already-fetched standing-category wall posts into its
/// current standing.
///
/// `posts` must come from a wall read filtered to [`STANDING_WALL_CATEGORY`] —
/// the wall projection has already applied the supersede chain, so the surviving
/// post is the current declaration and the last one wins.
///
/// An empty slice is not an error: a room nobody has ever marked is live and
/// unprotected, and that is the overwhelmingly common case.
///
/// A present-but-unparseable post IS an error, deliberately. Defaulting there
/// would read a room declared archived by a newer client as live, and the caller
/// would then act — or, worse, overwrite the declaration with a lossy version of
/// itself. Failing loud costs one turn; guessing loses a declaration.
pub fn project_standing(
    posts: &[airc_core::doctrine::WallPostPublished],
) -> Result<RoomStanding, StandingParseError> {
    match posts.last() {
        Some(post) => {
            serde_json::from_str(&post.body).map_err(|source| StandingParseError {
                source_message: source.to_string(),
            })
        }
        None => Ok(RoomStanding::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airc_core::doctrine::WallPostPublished;
    use airc_core::{PeerId, RoomId};

    fn post(body: &str) -> WallPostPublished {
        WallPostPublished {
            room_id: RoomId::from_uuid(uuid::Uuid::nil()),
            post_id: uuid::Uuid::nil(),
            category: STANDING_WALL_CATEGORY.to_string(),
            body: body.to_string(),
            supersedes: None,
            published_by: PeerId::from_u128(1),
            published_at_ms: 0,
        }
    }

    /// what this catches: the default flipping. If `archived` ever defaulted
    /// true, every room in the grid would go silent at once — and the rooms
    /// nobody has ever marked are the ordinary case, not the exception.
    #[test]
    fn a_room_nobody_marked_is_live_and_unprotected() {
        let standing = project_standing(&[]).expect("empty wall is not an error");
        assert!(!standing.archived, "an unmarked room is LIVE");
        assert!(!standing.protected, "an unmarked room is UNPROTECTED");
        assert_eq!(standing.note, None);
    }

    /// what this catches: the supersede order inverting. The wall projection
    /// hands us posts in published order with superseded versions already
    /// dropped, so the LAST one is current — reading the first would resurrect
    /// a reopened room's archived declaration.
    #[test]
    fn the_last_surviving_post_is_the_current_standing() {
        let posts = vec![
            post(r#"{"archived":true,"protected":false}"#),
            post(r#"{"archived":false,"protected":false}"#),
        ];
        let standing = project_standing(&posts).expect("parse");
        assert!(
            !standing.archived,
            "a later reopen must win over an earlier archive"
        );
    }

    /// what this catches: a well-meant `unwrap_or_default()` creeping in. A room
    /// a newer client declared archived, in a shape this build cannot read, must
    /// NOT read as live — that would have us treat a concluded activity as open
    /// and, on the write path, overwrite the newer declaration with a lossy one.
    #[test]
    fn a_present_but_unreadable_standing_fails_loud_instead_of_defaulting() {
        let err = project_standing(&[post("{not json at all")])
            .expect_err("unparseable standing must be an error, never a default");
        let rendered = err.to_string();
        assert!(
            rendered.contains("unreadable"),
            "the error must say what happened: {rendered}"
        );
        assert!(
            rendered.contains("newer client"),
            "the error must say WHY it refuses to guess: {rendered}"
        );
    }

    /// what this catches: an unknown field from a newer client hard-failing a
    /// read it could have survived. Forward compatibility is the whole reason
    /// the parse failure above is loud — a field we do not know about yet is
    /// not a corrupt declaration, and must not be treated as one.
    #[test]
    fn a_standing_carrying_a_newer_clients_extra_field_still_reads() {
        let standing = project_standing(&[post(
            r#"{"archived":true,"protected":true,"someFutureField":"whatever"}"#,
        )])
        .expect("an unknown field is not a corrupt standing");
        assert!(standing.archived);
        assert!(standing.protected);
    }

    /// what this catches: the note being dropped on the floor. A standing change
    /// with no reason is a mystery to the next reader, so the field has to
    /// survive the round trip that the command module writes and reads.
    #[test]
    fn the_reason_survives_the_round_trip() {
        let written = RoomStanding {
            archived: true,
            protected: true,
            note: Some("K3 bring-up concluded 2026-08-07".to_string()),
        };
        let body = serde_json::to_string(&written).expect("encode");
        let read = project_standing(&[post(&body)]).expect("decode");
        assert_eq!(read, written, "standing must survive encode → project");
    }
}
