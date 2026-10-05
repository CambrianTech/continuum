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
    #[ts(optional)]
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
/// A room's REVIEW POLICY (Joel, 2026-10-05, card fa4aaaaa): review is the room's one
/// verdict action, whoever gives it (a peer, the benchmark grader in its room, the author
/// herself where the room allows it). Like GitHub, a room can require several, by role.
/// Declared as the `review` recipe param and set per room by `activity/spawn --params`,
/// read from the binding exactly like `repo`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReviewPolicy {
    /// Passing reviews, from distinct reviewers, a card needs before it finishes.
    #[serde(default = "ReviewPolicy::one")]
    pub required: u32,
    /// Roles a reviewer must hold; empty = any member. Declared now, enforced when
    /// membership carries roles (the recipe's `citizens[].role`, a named follow-up).
    #[serde(default)]
    pub roles: Vec<String>,
    /// Whether the card's own author may review it (Kimi, on her own work, where her
    /// room allows it).
    #[serde(default, rename = "self")]
    pub self_review: bool,
}

impl ReviewPolicy {
    fn one() -> u32 {
        1
    }
}

impl Default for ReviewPolicy {
    fn default() -> Self {
        Self { required: 1, roles: Vec::new(), self_review: false }
    }
}

impl RoomRecipeBinding {
    /// The review policy this room declares (`params.review`), `Ok(None)` when it declares
    /// none: a room bound before the param existed keeps today's behaviour rather than
    /// suddenly gating every card. A value that is present but not a policy is an error,
    /// never a guess.
    pub fn declared_review(&self) -> Result<Option<ReviewPolicy>, String> {
        match self.params.get("review") {
            None | Some(serde_json::Value::Null) => Ok(None),
            Some(v) => serde_json::from_value(v.clone())
                .map(Some)
                .map_err(|e| format!("the room's review policy is not {{required, roles, self}}: {e}")),
        }
    }

    /// The repo this room declares (`params.repo`, project.json), or `None` when the
    /// recipe has no such param or it was left at the blank default. A project IS a repo;
    /// `work/create` reads this before refusing (Kimi, 2026-10-05: six refusals in a
    /// project room whose binding had carried the field, blank, since spawn).
    pub fn declared_repo(&self) -> Option<&str> {
        self.params
            .get("repo")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|r| !r.is_empty())
    }
}

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
    // what this catches: the review policy read from the room (card fa4aaaaa). A full
    // policy parses with `self` mapped; an absent one is None (today's behaviour kept);
    // defaults fill omitted fields; a malformed value is an error, never a guess.
    #[test]
    fn a_room_declares_its_review_policy() {
        let with = |v: serde_json::Value| {
            let mut b: RoomRecipeBinding = serde_json::from_value(serde_json::json!({"recipe": "project"}))
                .expect("a minimal binding");
            b.params.insert("review".into(), v);
            b
        };
        let full = with(serde_json::json!({"required": 2, "roles": ["reviewer"], "self": true}));
        assert_eq!(
            full.declared_review().unwrap(),
            Some(ReviewPolicy { required: 2, roles: vec!["reviewer".into()], self_review: true })
        );
        assert_eq!(with(serde_json::json!({"self": true})).declared_review().unwrap(),
            Some(ReviewPolicy { required: 1, roles: vec![], self_review: true }), "omitted fields default");
        let none: RoomRecipeBinding = serde_json::from_value(serde_json::json!({"recipe": "project"})).unwrap();
        assert_eq!(none.declared_review().unwrap(), None, "no policy declared = no gate");
        assert!(with(serde_json::json!("two please")).declared_review().is_err());
    }

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

    // what this catches: the room's declared repo is read from its binding; a recipe with
    // no repo param, or the blank default every project room was spawned with until
    // 2026-10-05, declares none, so work/create refuses with the words that fix it.
    #[test]
    fn a_room_declares_its_repo_only_when_the_binding_carries_one() {
        let bound = |params: serde_json::Value| RoomRecipeBinding {
            recipe: "project".into(),
            parent: None,
            params: serde_json::from_value(params).expect("a params map"),
        };
        assert_eq!(bound(serde_json::json!({"repo": "CambrianTech/career-wrangler"})).declared_repo(), Some("CambrianTech/career-wrangler"));
        assert_eq!(bound(serde_json::json!({"repo": "  "})).declared_repo(), None, "the blank default declares none");
        assert_eq!(bound(serde_json::json!({})).declared_repo(), None, "a recipe without the param declares none");
        assert_eq!(bound(serde_json::json!({"repo": 7})).declared_repo(), None, "a non-string is not a repo");
    }

}
