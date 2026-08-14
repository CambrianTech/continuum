//! A room's RECIPE BINDING — which activity type this room is an instance of.
//!
//! Sits beside [`standing`](super::standing) (is this activity still live) and
//! [`membership`](super::membership) (who is party to it): the binding is the
//! answer to **what IS this room**. Recipe is the content-type, the room is the
//! content, and this is the pointer from the one to the other.
//!
//! # Why the binding lives on the WALL
//!
//! airc's own `ScopeRef` doc names the split: peer-private room state is
//! `ScopeRef::Room`, but "plan / instructions / **recipe** that every participant
//! must see" belongs on the wall. What a room IS must be shared — every client,
//! human or citizen, has to agree on it, or two surfaces render two different
//! activities over one transcript. So it is a wall post, not per-peer state, and
//! not a continuum-side table shadowing the room.
//!
//! # Why this is a TYPE and not an inline `json!`
//!
//! It was an inline `serde_json::json!` at the write site in
//! [`crate::modules::activity`], and **nothing on the planet read it back**. That
//! is worse than a missing feature: `activity/spawn` reported success, the binding
//! landed on the wall, and every renderer — web, mobile, and the citizen standing
//! in the room — still projected the room as a plain chat, because
//! `DefaultRoomPurpose` answered `"chat"` for every room in existence. A benchmark
//! room and a chat room were indistinguishable to everyone who had to work in one.
//!
//! One type, serialized by the writer and deserialized by the reader, is the
//! "agree by construction" discipline the presence and standing payloads already
//! follow. A hand-authored JSON literal on one side of a seam is a contract nobody
//! can typecheck ([[compression]]).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The wall category that carries a room's recipe binding.
pub const RECIPE_WALL_CATEGORY: &str = "recipe";

/// The room → recipe pointer, as published by `activity/spawn` and read back by
/// [`crate::ipc::recipe_room_purpose::RecipeRoomPurpose`].
// `Eq` was dropped when `params` arrived (#433): `serde_json::Value` carries
// floats, which are only `PartialEq`. Nothing keyed on the binding's Eq.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/experience/RoomRecipeBinding.ts"
)]
pub struct RoomRecipeBinding {
    /// Which recipe this room instantiates — the `purpose` key of an authored
    /// [`ExperienceRecipe`](super::recipe::ExperienceRecipe).
    ///
    /// Still the resolution key: #274 moves rooms to binding by `RecipeId`, and
    /// until that slice lands the purpose string is what both sides agree on.
    /// A binding naming a purpose no recipe declares resolves to no manifest —
    /// honestly absent, never a fabricated stand-in.
    pub recipe: String,
    /// Optional parent activity — activities spawn activities, and the graph is
    /// POINTERS, never nested blobs.
    ///
    /// A pointer to a room is a `RoomId`. It was a `String` while the doc directly
    /// above it said "POINTERS" — a pointer typed as text is not a pointer, it is a
    /// hope that whoever fills it in spells a uuid correctly, and nothing rejects
    /// `"the benchmark one"` ([[uuids-are-not-strings-and-never-hand-drawn]]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub parent: Option<airc_core::RoomId>,
    /// The RESOLVED parameters this room was spawned with (#433) — caller
    /// overrides merged over the recipe's declared defaults, validated at
    /// spawn. On the wall so the room is SELF-DESCRIBING: a citizen, a
    /// renderer, or a grader reads WHAT this room is parameterized to do from
    /// the same pipe as everything else — no side-channel run files
    /// (BENCHMARKS-ARE-ADAPTERS law). Empty for parameterless recipes, and
    /// absent on bindings published before #433 (serde default keeps them
    /// readable).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    #[ts(type = "Record<string, unknown>")]
    pub params: std::collections::BTreeMap<String, serde_json::Value>,
}

/// Why a wall could not be turned into a recipe binding.
///
/// One variant, for the same reason [`super::standing::StandingParseError`] has
/// one: a post exists under the recipe category and this build cannot read it.
#[derive(Debug, Clone, thiserror::Error)]
#[error(
    "room recipe binding is present but unreadable ({source_message}) — refusing to \
     guess, because guessing would render a purpose-built activity as a plain chat \
     room and nobody in it would be told"
)]
pub struct BindingParseError {
    /// The serde message verbatim, so the operator sees which field disagreed.
    pub source_message: String,
}

/// Project a room's already-fetched recipe-category wall posts into its binding.
///
/// `posts` must come from a wall read filtered to [`RECIPE_WALL_CATEGORY`] — the
/// wall projection has already applied the supersede chain, so the surviving post
/// is the current declaration and the last one wins (a re-bound room adopts its
/// newest recipe).
///
/// `Ok(None)` — no binding — is not an error. A room made by a bare `airc join`
/// has no recipe and IS a plain chat room; that is the ordinary case and the
/// honest default the [`RoomPurposeSource`](crate::ipc::room_purpose) contract
/// requires.
///
/// A present-but-unparseable post IS an error. Defaulting there would silently
/// downgrade a purpose-built activity — the exact failure this whole module
/// exists to end ([[fallbacks-are-illegal-fail-loud]]).
pub fn project_binding(
    posts: &[airc_core::doctrine::WallPostPublished],
) -> Result<Option<RoomRecipeBinding>, BindingParseError> {
    match posts.last() {
        Some(post) => serde_json::from_str(&post.body)
            .map(Some)
            .map_err(|source| BindingParseError {
                source_message: source.to_string(),
            }),
        None => Ok(None),
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
            category: RECIPE_WALL_CATEGORY.to_string(),
            body: body.to_string(),
            supersedes: None,
            published_by: PeerId::from_u128(1),
            published_at_ms: 0,
        }
    }

    /// what this catches: an unbound room erroring instead of reading as chat.
    /// Every room made by a bare `airc join` has no binding, so this is the
    /// common path — if it failed, the purpose index would log errors for the
    /// entire grid and resolve nothing.
    #[test]
    fn a_room_with_no_binding_is_simply_unbound() {
        assert_eq!(project_binding(&[]).expect("empty wall is not an error"), None);
    }

    /// what this catches: the supersede order inverting. Posts arrive in
    /// published order with superseded versions already dropped, so the LAST is
    /// current — reading the first would pin a re-bound room to its old activity.
    #[test]
    fn the_last_surviving_post_is_the_current_binding() {
        let posts = vec![
            post(r#"{"recipe":"chat"}"#),
            post(r#"{"recipe":"benchmark/hard-rs"}"#),
        ];
        let binding = project_binding(&posts).expect("parse").expect("bound");
        assert_eq!(binding.recipe, "benchmark/hard-rs");
    }

    /// what this catches: a `unwrap_or_default()` creeping in. A room a newer
    /// client bound to an activity this build cannot read must NOT silently read
    /// as a chat room — that is precisely the "renders as plain chat and nobody
    /// is told" failure the binding exists to end.
    #[test]
    fn an_unreadable_binding_fails_loud_instead_of_defaulting_to_chat() {
        let rendered = project_binding(&[post("{not json at all")])
            .expect_err("unparseable binding must be an error, never a default")
            .to_string();
        assert!(
            rendered.contains("unreadable"),
            "the error must say what happened: {rendered}"
        );
        assert!(
            rendered.contains("plain chat"),
            "the error must say what the guess would have COST: {rendered}"
        );
    }

    /// what this catches: a newer client's extra field hard-failing a read it
    /// could have survived. Forward compatibility is the reason the parse failure
    /// above is loud — an unknown field is not a corrupt binding.
    #[test]
    fn a_binding_carrying_a_newer_clients_extra_field_still_reads() {
        let binding = project_binding(&[post(
            r#"{"recipe":"benchmark/hard-rs","someFutureField":"whatever"}"#,
        )])
        .expect("an unknown field is not a corrupt binding")
        .expect("bound");
        assert_eq!(binding.recipe, "benchmark/hard-rs");
    }

    /// what this catches: the WRITE and the READ drifting. `activity/spawn`
    /// serializes this type and the purpose index deserializes it; if the field
    /// names stopped matching, every spawned activity would silently be a chat
    /// room again — which is exactly what an inline `json!` at the write site
    /// allowed for as long as it existed.
    #[test]
    fn the_binding_survives_the_round_trip_the_two_sides_share() {
        let written = RoomRecipeBinding {
            recipe: "benchmark/hard-rs".to_string(),
            // This fixture was ALWAYS a uuid — written as a String only because the
            // field was one. The value never changed; the type caught up to it.
            parent: Some(airc_core::RoomId::from_uuid(
                uuid::Uuid::parse_str("f1a1b2c3-0000-4000-8000-000000000000").expect("fixture id"),
            )),
            params: std::collections::BTreeMap::from([
                ("suite".to_string(), serde_json::json!("swe-lite")),
                ("instances".to_string(), serde_json::json!(2)),
            ]),
        };
        let body = serde_json::to_string(&written).expect("encode");
        let read = project_binding(&[post(&body)]).expect("decode").expect("bound");
        assert_eq!(read, written);
    }
}
