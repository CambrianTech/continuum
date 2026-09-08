//! CHILD RECORDS — a spawned activity announces itself to its PARENT room, so a node that
//! never spawned it can find it.
//!
//! Before 2026-09-08 a run room carried its recipe binding on its OWN wall and nothing
//! reached the parent: the tracker on the spawning node knew the round, the presence
//! refresher on that node adopted it, and a citizen hosted on another machine — seated
//! in the same commons, working the same board — could never be seated in the round,
//! because her node had no way to learn the room's name. "Personas that scale cross grid"
//! (Joel, the alpha) starts with this record: one durable wall post in the parent, read
//! by every node's resume pass.
//!
//! The record is small and stable: what the child IS (room, name, recipe, driver, the
//! run's suite) and when it was spawned. Standing (working / paused / done) is NOT here —
//! it lives on the child's own wall (`experience::standing`) and is read after joining,
//! so a stale parent record can never seat a citizen into a finished round.

use serde::{Deserialize, Serialize};

/// Wall category on the PARENT room. Sibling of `recipe` and `standing`.
pub const CHILD_WALL_CATEGORY: &str = "children";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomChildRecord {
    /// The child room's id, as a string uuid (the wire form the binding already uses).
    pub room_id: String,
    /// The child room's NAME — what a remote node joins by.
    pub name: String,
    pub recipe: String,
    /// The activity's driver when the recipe carries one (`citizen`, `detached_solve`…).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    /// The benchmark suite when this is a benchmark round; None for any other activity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suite: Option<String>,
    pub spawned_at_ms: u64,
}

impl RoomChildRecord {
    /// A benchmark round citizens pull from: it names a suite and its driver is citizen-
    /// class. A detached solve or a chat spawned under the commons is not one.
    pub fn is_citizen_benchmark(&self) -> bool {
        self.suite.is_some()
            && self
                .driver
                .as_deref()
                .map(|d| d.to_ascii_lowercase().contains("citizen"))
                .unwrap_or(false) // unwrap_or: no driver named = not a citizen round; a remote node never seats on a guess
    }
}

/// The children a parent's wall names — one per child room, the NEWEST record wins when a
/// room was announced twice (a re-spawn of the same name). Malformed posts are skipped:
/// a parent wall may carry older shapes and one bad row must not hide the rest.
pub fn project_children(posts: &[airc_core::doctrine::WallPostPublished]) -> Vec<RoomChildRecord> {
    let mut by_room: std::collections::BTreeMap<String, RoomChildRecord> = Default::default();
    for post in posts {
        let Ok(rec) = serde_json::from_str::<RoomChildRecord>(&post.body) else {
            continue;
        };
        match by_room.get(&rec.room_id) {
            Some(prev) if prev.spawned_at_ms > rec.spawned_at_ms => {}
            _ => {
                by_room.insert(rec.room_id.clone(), rec);
            }
        }
    }
    by_room.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use airc_core::doctrine::WallPostPublished;

    fn post(body: &str) -> WallPostPublished {
        WallPostPublished {
            room_id: airc_core::RoomId::from_uuid(uuid::Uuid::nil()),
            post_id: uuid::Uuid::nil(),
            category: CHILD_WALL_CATEGORY.to_string(),
            body: body.to_string(),
            supersedes: None,
            published_by: airc_core::PeerId::from_u128(1),
            published_at_ms: 0,
        }
    }

    // what this catches: a remote node seating its residents into the wrong child (a chat,
    // a detached solve, or a stale duplicate announcement) — only a citizen-driven
    // benchmark round with a suite qualifies, and the newest record for a room wins.
    #[test]
    fn only_citizen_benchmark_children_qualify_and_the_newest_record_wins() {
        let bench = RoomChildRecord {
            room_id: "r1".into(),
            name: "bench-swe-bench-verified-1".into(),
            recipe: "benchmark".into(),
            driver: Some("citizen".into()),
            suite: Some("swe-bench-verified".into()),
            spawned_at_ms: 10,
        };
        let detached = RoomChildRecord { driver: Some("detached_solve".into()), room_id: "r2".into(), ..bench.clone() };
        let chat = RoomChildRecord { suite: None, driver: None, room_id: "r3".into(), ..bench.clone() };
        let older = RoomChildRecord { spawned_at_ms: 5, name: "stale".into(), ..bench.clone() };
        let posts = vec![
            post(&serde_json::to_string(&older).unwrap()),   // unwrap: test literal
            post(&serde_json::to_string(&bench).unwrap()),   // unwrap: test literal
            post("not json"),
            post(&serde_json::to_string(&detached).unwrap()), // unwrap: test literal
            post(&serde_json::to_string(&chat).unwrap()),     // unwrap: test literal
        ];
        let children = project_children(&posts);
        assert_eq!(children.len(), 3, "{children:?}");
        let r1 = children.iter().find(|c| c.room_id == "r1").expect("r1 present"); // expect: test asserts presence
        assert_eq!(r1.name, "bench-swe-bench-verified-1", "the newest record wins");
        assert!(r1.is_citizen_benchmark());
        assert!(!detached.is_citizen_benchmark());
        assert!(!chat.is_citizen_benchmark());
    }
}
