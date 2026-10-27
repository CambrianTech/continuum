//! ONE registry of every activity room this node minted through `activity/spawn`,
//! read by the presence emitter's refresher so a spawned room is bridged (its
//! transcript reaches the chat store and the rail) the same minute it exists.
//!
//! Measured 2026-09-05 (card 3d4b3d9c): the refresher adopted rooms from the
//! BENCHMARK tracker's list plus the daemon's room set — so a bench run room was
//! ingested and listed within a minute, while the `continuum` project room spawned
//! by the same `spawn_activity_room` was subscribed, joined, streamed (attach
//! frames in the log) and STILL had `chat/history` = 0 rows and no rail entry.
//! Benchmarks are adapters, not a parallel runner: the room registry the
//! refresher reads must be the activity substrate's, not the bench tracker's.
//! The tracker's list stays for solve rooms it mints itself; this one is generic.
//!
//! Process-lifetime only, on purpose: the durable truth is the airc subscription
//! set of the runtime that spawned the room (the refresher folds that too), so a
//! reboot re-adopts from subscriptions and this map only covers the window
//! between a spawn and the next refresh tick.

use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};

use uuid::Uuid;

static SPAWNED: LazyLock<Mutex<BTreeMap<Uuid, String>>> = LazyLock::new(|| Mutex::new(BTreeMap::new()));

/// Record a room minted by `activity/spawn` (idempotent; the newest name wins).
pub fn record(room: Uuid, name: &str) {
    if name.trim().is_empty() {
        return;
    }
    SPAWNED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(room, name.to_string());
}

/// Every room recorded since boot, `(room_id, name)`.
pub fn spawned_rooms() -> Vec<(Uuid, String)> {
    SPAWNED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .map(|(id, name)| (*id, name.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a spawned room falling out of the adoption set — the
    // exact absence that left the project room unread and unlisted (3d4b3d9c).
    #[test]
    fn a_spawned_room_is_listed_until_the_process_ends_and_nameless_rooms_are_refused() {
        let id = Uuid::new_v4();
        record(id, "widgets");
        assert!(spawned_rooms().iter().any(|(r, n)| *r == id && n == "widgets"));
        let anon = Uuid::new_v4();
        record(anon, "   ");
        assert!(!spawned_rooms().iter().any(|(r, _)| *r == anon), "a nameless room cannot be adopted by name");
    }
}
