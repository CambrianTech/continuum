//! THE CURRENT WALL — which of a room's wall records are still in force.
//!
//! Wall storage is append-only: every save of a card ledger, a room's standing, its
//! recipe binding, a child announcement or an activity bundle is a NEW post, and airc
//! keeps every post that does not name a `supersedes`. Each record type's owner already
//! defines "newest wins" for its projection (`project_ledger`, `project_standing`,
//! `project_binding`, `project_children`, `project_activity_state`), but a context view
//! that renders the raw wall presents every revision as simultaneous current state.
//!
//! Measured on the 5090, 2026-10-11 (card c6640f7b): Kimi's `[room-wall]` was 25,802
//! chars with 30 entries, 22 of them distinct. The main room carried 12 standing posts by
//! four authors across five weeks (none naming `supersedes`), six of them byte-identical,
//! and only the last is the room's standing by `project_standing`'s own rule. All 12 were
//! re-prefilled on every turn while her history was being cut to fit the window.
//!
//! This is the ONE rule for that view: a record whose owner keys it newest-wins keeps
//! only its newest post per key. It reads the owners' types for the key and never
//! decides what a record MEANS, so the substrate still renders whatever a room pins. An
//! unreadable record stays visible and never replaces a valid one, since a record a
//! newer client wrote must not erase state this build can read. Stored history is never
//! touched.
use airc_core::doctrine::WallPostPublished;

use super::activity_state::{ActivityStateRecord, ACTIVITY_STATE_WALL_CATEGORY};
use super::binding::RECIPE_WALL_CATEGORY;
use super::children::{RoomChildRecord, CHILD_WALL_CATEGORY};
use super::ledger::{CardLedger, LEDGER_WALL_CATEGORY};
use super::standing::STANDING_WALL_CATEGORY;

/// The record a post is the newest revision of, by its owner's newest-wins key: the
/// room, the category, and the part of the body the owner keys on (none for a record
/// a room holds one of). `None` for a category no owner keys this way, and for an
/// unreadable body of a category that keys on its content.
fn record_key(post: &WallPostPublished) -> Option<(uuid::Uuid, &'static str, String)> {
    let room = post.room_id.as_uuid();
    let keyed = |category: &'static str, part: String| Some((room, category, part));
    match post.category.as_str() {
        // one per room: `project_standing` and `project_binding` read the last post
        STANDING_WALL_CATEGORY => keyed(STANDING_WALL_CATEGORY, String::new()),
        RECIPE_WALL_CATEGORY => keyed(RECIPE_WALL_CATEGORY, String::new()),
        // one per card: `project_ledger`
        LEDGER_WALL_CATEGORY => match serde_json::from_str::<CardLedger>(&post.body) {
            Ok(ledger) => keyed(LEDGER_WALL_CATEGORY, ledger.card_id.to_string()),
            Err(error) => {
                continuum_base::probe!(
                    class = "card.ledger.unreadable",
                    post = %post.post_id,
                    error = %error,
                    "unreadable wall ledger retained without replacing valid state"
                );
                None
            }
        },
        // one per child room: `project_children`
        CHILD_WALL_CATEGORY => serde_json::from_str::<RoomChildRecord>(&post.body)
            .ok() // ORM boundary: a child record this build cannot read stays visible, unkeyed
            .and_then(|child| keyed(CHILD_WALL_CATEGORY, child.room_id.to_string())),
        // one per kind: `project_activity_state` (a save patches the newest bundle)
        ACTIVITY_STATE_WALL_CATEGORY => serde_json::from_str::<ActivityStateRecord>(&post.body)
            .ok() // ORM boundary: a bundle this build cannot read stays visible, unkeyed
            .and_then(|bundle| keyed(ACTIVITY_STATE_WALL_CATEGORY, bundle.kind)),
        _ => None,
    }
}

/// The wall records still in force, in publish order: each keyed record's newest post,
/// every unkeyed post. Storage is append-only and this never mutates it.
pub(crate) fn current_wall_records(posts: Vec<WallPostPublished>) -> Vec<WallPostPublished> {
    let mut seen = std::collections::HashSet::new();
    let mut current = Vec::with_capacity(posts.len());
    for post in posts.into_iter().rev() {
        if let Some(key) = record_key(&post) {
            if !seen.insert(key) {
                continue;
            }
        }
        current.push(post);
    }
    current.reverse();
    current
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experience::ledger::project_ledger;
    use crate::experience::standing::project_standing;
    use airc_core::{PeerId, RoomId};

    fn post(category: &str, body: &str) -> WallPostPublished {
        WallPostPublished {
            room_id: RoomId::from_uuid(uuid::Uuid::nil()),
            post_id: uuid::Uuid::new_v4(),
            category: category.to_string(),
            body: body.to_string(),
            supersedes: None,
            published_by: PeerId::from_u128(1),
            published_at_ms: 0,
        }
    }

    fn ids(posts: &[WallPostPublished]) -> Vec<uuid::Uuid> {
        posts.iter().map(|p| p.post_id).collect()
    }

    // what this catches: old revisions competing with current guidance, or a
    // malformed update erasing valid state; room/card boundaries and documents survive.
    #[test]
    fn current_wall_keeps_latest_per_card_without_rewriting_history_or_documents() {
        let card = uuid::Uuid::from_u128(7);
        let make = |card_id, next_test: &str| {
            post(
                LEDGER_WALL_CATEGORY,
                &serde_json::to_string(&CardLedger {
                    card_id,
                    next_test: next_test.into(),
                    ..Default::default()
                })
                .unwrap(),
            )
        };
        let old = make(card, "old next step");
        let document = post("plan", "a shared plan, not ledger JSON");
        let other = make(uuid::Uuid::from_u128(8), "other activity");
        let latest = make(card, "current next step");
        let malformed = post(LEDGER_WALL_CATEGORY, "broken record");
        let mut other_room = make(card, "same card reference in another room");
        other_room.room_id = RoomId::from_uuid(uuid::Uuid::from_u128(9));
        let history = vec![
            old,
            document.clone(),
            other.clone(),
            latest.clone(),
            malformed.clone(),
            other_room.clone(),
        ];
        let current = current_wall_records(history.clone());
        assert_eq!(history.len(), 6);
        assert_eq!(
            ids(&current),
            vec![document.post_id, other.post_id, latest.post_id, malformed.post_id, other_room.post_id]
        );
        let same_room: Vec<_> = current
            .into_iter()
            .filter(|p| p.room_id == latest.room_id && p.category == LEDGER_WALL_CATEGORY)
            .collect();
        assert_eq!(project_ledger(&same_room, card).unwrap().next_test, "current next step");
    }

    // what this catches (the 5090 2026-10-11, card c6640f7b): a room's standing re-posted
    // without `supersedes` rendering every declaration as current, 12 on Kimi's wall,
    // six of them byte-identical. Only the last one is the room's standing, and the view
    // must agree with `project_standing` on which one that is.
    #[test]
    fn a_room_holds_one_standing_and_it_is_the_one_project_standing_reads() {
        let archived = r#"{"archived":true,"protected":true,"note":"load-bearing"}"#;
        let mut history: Vec<_> = (0..6).map(|_| post(STANDING_WALL_CATEGORY, archived)).collect();
        let reopened = post(STANDING_WALL_CATEGORY, r#"{"archived":false,"protected":true,"note":"reopened"}"#);
        history.push(reopened.clone());
        let mut elsewhere = post(STANDING_WALL_CATEGORY, archived);
        elsewhere.room_id = RoomId::from_uuid(uuid::Uuid::from_u128(9));
        history.push(elsewhere.clone());

        let current = current_wall_records(history.clone());
        assert_eq!(ids(&current), vec![reopened.post_id, elsewhere.post_id], "one per room, the newest");
        let this_room: Vec<_> = current.iter().filter(|p| p.room_id == reopened.room_id).cloned().collect();
        let all_this_room: Vec<_> = history.iter().filter(|p| p.room_id == reopened.room_id).cloned().collect();
        assert_eq!(
            project_standing(&this_room).unwrap(),
            project_standing(&all_this_room).unwrap(),
            "the view and the projection name the same standing"
        );
    }

    // what this catches: the other owner-keyed records folding by THEIR key, not one
    // per category. Two activity kinds and two child rooms in one room each keep their
    // own newest; the recipe keeps its last; an unreadable bundle stays and replaces nothing.
    #[test]
    fn each_record_keeps_its_newest_by_its_owners_key() {
        let bundle = |kind: &str, at: u64| {
            post(
                ACTIVITY_STATE_WALL_CATEGORY,
                &serde_json::json!({"kind": kind, "savedAtMs": at, "state": {}}).to_string(),
            )
        };
        let child = |room: u128, name: &str| {
            post(
                CHILD_WALL_CATEGORY,
                &serde_json::json!({"room_id": uuid::Uuid::from_u128(room), "name": name, "recipe": "r", "spawned_at_ms": 0})
                    .to_string(),
            )
        };
        let round_old = bundle("benchmark/round", 1);
        let project = bundle("project", 2);
        let round_new = bundle("benchmark/round", 3);
        let unreadable_bundle = post(ACTIVITY_STATE_WALL_CATEGORY, "not json");
        let child_a_old = child(1, "a");
        let child_b = child(2, "b");
        let child_a_new = child(1, "a again");
        let recipe_old = post(RECIPE_WALL_CATEGORY, r#"{"recipe":"chat"}"#);
        let recipe_new = post(RECIPE_WALL_CATEGORY, r#"{"recipe":"project"}"#);
        let history = vec![
            round_old,
            project.clone(),
            child_a_old,
            recipe_old,
            child_b.clone(),
            round_new.clone(),
            unreadable_bundle.clone(),
            child_a_new.clone(),
            recipe_new.clone(),
        ];
        assert_eq!(
            ids(&current_wall_records(history)),
            vec![
                project.post_id,
                child_b.post_id,
                round_new.post_id,
                unreadable_bundle.post_id,
                child_a_new.post_id,
                recipe_new.post_id
            ]
        );
    }
}
