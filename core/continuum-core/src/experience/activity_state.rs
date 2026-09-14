//! THE ACTIVITY STATE — an activity's saved instance state, the Android way.
//!
//! Every activity is a recipe that became a room. Its RUNTIME state (the cards it
//! imported, what it bound while its pipeline ran, the stage it reached) used to live
//! in whichever process ran it — a tracker map, a per-round file under `state/` —
//! and a seam (reboot, hand-off, another node) rebuilt it by guesswork or lost it
//! (2026-09-14: the round tracker's second copy of card state, `bench-rounds/*.json`,
//! shadowing the board; card c9ddb911). Android settled this shape a decade ago:
//! the framework calls `onSaveInstanceState` at the boundaries and hands the bundle
//! back on `onCreate`; the activity never persists itself.
//!
//! Here the bundle is a wall record on the activity's OWN room (category
//! [`ACTIVITY_STATE_WALL_CATEGORY`]), one per `kind` (the recipe purpose), newest
//! wins — the same shape as the room's binding, standing, children and card
//! ledgers: durable, airc-replicated, readable by any node the room reaches, visible
//! to the human on the room. The SHARED half. (The citizen half — what one mind
//! remembers about her seat in the activity — is hers, in her store; slice 2.)
//!
//! Who writes it: the recipe executor, at step boundaries the recipe marks with
//! `saves: ["<binding>", ...]` (see [`crate::recipe::types::RecipeStep::saves`]) —
//! the framework saves, the activity declares what. Who reads it: whoever resumes
//! the activity — the reconciler on boot, a peer node seating its residents.
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const ACTIVITY_STATE_WALL_CATEGORY: &str = "activity-state";

/// One saved bundle. `kind` is the recipe purpose (`benchmark/round`, `project`,
/// …) so a room hosting a nested or migrated activity keeps the bundles apart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/experience/ActivityStateRecord.ts")]
pub struct ActivityStateRecord {
    pub kind: String,
    #[ts(type = "number")]
    pub saved_at_ms: u64,
    /// The bundle: top-level keys are the recipe's binding names (`cards`,
    /// `imported`, …); a save PATCHES keys, it never drops the ones it did not name.
    #[ts(type = "Record<string, unknown>")]
    pub state: serde_json::Map<String, serde_json::Value>,
}

/// The newest bundle of `kind` among a room's activity-state posts (posts arrive in
/// publish order; a record another kind wrote is not this activity's).
pub fn project_activity_state(
    posts: &[airc_core::doctrine::WallPostPublished],
    kind: &str,
) -> Option<ActivityStateRecord> {
    posts
        .iter()
        .rev()
        .filter_map(|p| serde_json::from_str::<ActivityStateRecord>(&p.body).ok()) // ORM boundary: a record another writer could not encode is not this activity's bundle
        .find(|r| r.kind == kind)
}

/// `base` patched by `patch`: named keys replaced, the rest kept.
pub fn merged(
    base: Option<&ActivityStateRecord>,
    kind: &str,
    patch: serde_json::Map<String, serde_json::Value>,
    now_ms: u64,
) -> ActivityStateRecord {
    let mut state = base.map(|b| b.state.clone()).unwrap_or_default(); // JUSTIFIED unwrap_or_default: no bundle yet IS the empty bundle
    for (k, v) in patch {
        state.insert(k, v);
    }
    ActivityStateRecord { kind: kind.to_string(), saved_at_ms: now_ms, state }
}

/// The store the framework saves through. One implementation is the wall; a test
/// can hold one in memory.
#[async_trait::async_trait]
pub trait ActivityStateStore: Send + Sync {
    async fn load(&self, room: &airc_lib::Room, kind: &str) -> Result<Option<ActivityStateRecord>, String>;
    /// Patch the newest bundle of `kind` and publish the result; returns what was written.
    async fn save(
        &self,
        room: &airc_lib::Room,
        kind: &str,
        patch: serde_json::Map<String, serde_json::Value>,
    ) -> Result<ActivityStateRecord, String>;
}

pub struct WallActivityStateStore {
    airc: std::sync::Arc<airc_lib::Airc>,
}

impl WallActivityStateStore {
    pub fn new(airc: std::sync::Arc<airc_lib::Airc>) -> Self {
        Self { airc }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0) // JUSTIFIED unwrap_or: a pre-epoch clock stamps 0 — the bundle still carries its state
}

#[async_trait::async_trait]
impl ActivityStateStore for WallActivityStateStore {
    async fn load(&self, room: &airc_lib::Room, kind: &str) -> Result<Option<ActivityStateRecord>, String> {
        let posts = self
            .airc
            .wall_posts_in(room, Some(ACTIVITY_STATE_WALL_CATEGORY))
            .await
            .map_err(|e| format!("activity-state wall read failed: {e}"))?;
        Ok(project_activity_state(&posts, kind))
    }

    async fn save(
        &self,
        room: &airc_lib::Room,
        kind: &str,
        patch: serde_json::Map<String, serde_json::Value>,
    ) -> Result<ActivityStateRecord, String> {
        let keys: Vec<String> = patch.keys().cloned().collect();
        let base = self.load(room, kind).await?;
        let record = merged(base.as_ref(), kind, patch, now_ms());
        let body = serde_json::to_string(&record).map_err(|e| e.to_string())?; // ORM boundary: a wall record is airc's wire + store, read back by whoever resumes the activity
        self.airc
            .publish_wall_post_in(room, ACTIVITY_STATE_WALL_CATEGORY.to_string(), body, None)
            .await
            .map_err(|e| format!("activity-state publish failed: {e}"))?;
        crate::probe!(
            class = "activity.state.saved",
            room = %room.name,
            kind = %kind,
            keys = %keys.join(","),
            total_keys = record.state.len() as u64,
            "activity bundle saved on the room"
        );
        Ok(record)
    }
}

/// A sink already bound to ONE activity (its room and kind) — what the recipe
/// executor holds, so the executor stays ignorant of rooms and wire.
#[async_trait::async_trait]
pub trait StepStateSink: Send + Sync {
    async fn save(&self, patch: serde_json::Map<String, serde_json::Value>) -> Result<(), String>;
}

pub struct BoundStateSink {
    store: std::sync::Arc<dyn ActivityStateStore>,
    room: airc_lib::Room,
    kind: String,
}

impl BoundStateSink {
    pub fn new(store: std::sync::Arc<dyn ActivityStateStore>, room: airc_lib::Room, kind: impl Into<String>) -> Self {
        Self { store, room, kind: kind.into() }
    }
}

#[async_trait::async_trait]
impl StepStateSink for BoundStateSink {
    async fn save(&self, patch: serde_json::Map<String, serde_json::Value>) -> Result<(), String> {
        self.store.save(&self.room, &self.kind, patch).await.map(|_| ())
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
            category: ACTIVITY_STATE_WALL_CATEGORY.to_string(),
            body: body.to_string(),
            supersedes: None,
            published_by: PeerId::from_u128(1),
            published_at_ms: 0,
        }
    }

    /// what this catches: a resume reading an OLDER bundle, or another activity's
    /// bundle on the same room, as its own state.
    #[test]
    fn the_newest_bundle_of_this_kind_is_the_state_and_other_kinds_are_not() {
        let posts = vec![
            post(r#"{"kind":"benchmark/round","savedAtMs":1,"state":{"stage":"working"}}"#),
            post(r#"{"kind":"project","savedAtMs":2,"state":{"stage":"other"}}"#),
            post(r#"{"kind":"benchmark/round","savedAtMs":3,"state":{"stage":"settled"}}"#),
            post("not a bundle at all"),
        ];
        let r = project_activity_state(&posts, "benchmark/round").expect("a bundle"); // JUSTIFIED: the test built the post
        assert_eq!(r.saved_at_ms, 3);
        assert_eq!(r.state["stage"], "settled");
        assert!(project_activity_state(&posts, "chat").is_none());
    }

    /// what this catches: a save that names two keys wiping the ten it did not
    /// name — a step boundary saves ITS bindings, the bundle keeps the rest.
    #[test]
    fn a_save_patches_the_bundle_and_keeps_what_it_did_not_name() {
        let base = ActivityStateRecord {
            kind: "benchmark/round".into(),
            saved_at_ms: 1,
            state: serde_json::from_value(serde_json::json!({"imported": {"n": 8}, "stage": "working"})).unwrap(), // JUSTIFIED: a literal
        };
        let patch: serde_json::Map<String, serde_json::Value> =
            serde_json::from_value(serde_json::json!({"cards": ["a", "b"], "stage": "settled"})).unwrap(); // JUSTIFIED: a literal
        let r = merged(Some(&base), "benchmark/round", patch, 9);
        assert_eq!(r.saved_at_ms, 9);
        assert_eq!(r.state["imported"]["n"], 8, "unnamed key kept");
        assert_eq!(r.state["stage"], "settled", "named key replaced");
        assert_eq!(r.state["cards"].as_array().map(|a| a.len()), Some(2));
        let again: ActivityStateRecord = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap(); // JUSTIFIED: round-trip of a value this test built
        assert_eq!(again, r, "the wire shape round-trips");
    }
}
