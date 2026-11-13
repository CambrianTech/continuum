//! A node's durable ROOM DIRECTORY: every member this node has ever seen in each room, one
//! JSON file per room under `~/.continuum/state/`. Membership is durable, presence is live
//! (#258/#262): the presence emitter (`ipc::positron_presence`) folds live sightings in and
//! writes it back; a mind reads it to ask who exists and who is human. The store lives with
//! the mind that reads it, so the mind never reaches into the socket layer for a fact.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use continuum_positron::chat::RosterSlotView;
use uuid::Uuid;

/// On-disk room directory: every `RosterSlotView` this node has ever
/// projected for the room, keyed by member id. The sibling of the airc
/// attach-cursor file — node-local durable state under `~/.continuum/state/`.
fn directory_path(room_id: &Uuid) -> Option<PathBuf> {
    // The one continuum home (it honours $CONTINUUM_HOME and a test's own home), never a
    // hand-built ~/.continuum: none means no directory to read or write, said by the caller.
    continuum_kernel::paths::continuum_home()
        .ok()
        .map(|home| home.join("state").join(format!("room-directory-{room_id}.json")))
}

/// Every slot in EVERY room directory on this node — the scope-wide "who
/// exists" read `presence/directory` unions (duplicates across rooms
/// included; the caller merges by member id, newest sighting winning).
/// Files, not daemon paging: existence must survive restarts and benchmark
/// bursts, and the recent-events page provably doesn't (2026-08-31: a
/// 4000-event page during a solve burst hid every quiet peer).
pub fn scope_directory_slots() -> Vec<RosterSlotView> {
    let Some(state) = continuum_kernel::paths::continuum_home().ok().map(|home| home.join("state")) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&state) else {
        return Vec::new();
    };
    let mut slots = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(room) = name
            .strip_prefix("room-directory-")
            .and_then(|r| r.strip_suffix(".json"))
        else {
            continue;
        };
        let Ok(room_id) = Uuid::parse_str(room) else {
            continue;
        };
        slots.extend(load_directory(&room_id).into_values());
    }
    slots
}

/// Is this peer a HUMAN on this node — a directory slot whose runtime is the
/// interactive client (`SenderKind::from_runtime`)? The ONE directory
/// (`scope_directory_slots`), memoised for a minute: the persona loop head
/// asks it per drained line, and a work-holding citizen wakes for a human
/// line or an @mention, never for agent status traffic (2026-09-04: every
/// agent line in #academy woke all twelve holders — 8 work turns an hour).
pub fn is_human_peer(peer: Uuid) -> bool {
    static HUMANS: std::sync::LazyLock<std::sync::Mutex<(Option<std::time::Instant>, std::collections::HashSet<Uuid>)>> =
        std::sync::LazyLock::new(|| std::sync::Mutex::new((None, std::collections::HashSet::new())));
    let mut memo = HUMANS.lock().unwrap_or_else(|e| e.into_inner()); // poisoned lock = read the last state, same policy as every lock in this crate
    let stale = memo.0.is_none_or(|at| at.elapsed() > Duration::from_secs(60));
    if stale {
        memo.1 = scope_directory_slots()
            .into_iter()
            .filter(|s| matches!(s.kind, continuum_positron::SenderKind::Human))
            .map(|s| s.member_id)
            .collect();
        memo.0 = Some(std::time::Instant::now());
    }
    memo.1.contains(&peer)
}

pub fn load_directory(room_id: &Uuid) -> HashMap<Uuid, RosterSlotView> {
    let Some(path) = directory_path(room_id) else {
        return HashMap::new();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return HashMap::new(); // first boot — honest empty, filled by the deep scan
    };
    match serde_json::from_slice::<Vec<RosterSlotView>>(&bytes) {
        Ok(slots) => slots.into_iter().map(|s| (s.member_id, s)).collect(),
        Err(err) => {
            tracing::warn!(%err, ?path, "room directory unreadable — starting fresh (will re-seed from the deep scan)");
            HashMap::new()
        }
    }
}

pub fn persist_directory(room_id: &Uuid, dir: &HashMap<Uuid, RosterSlotView>) {
    let Some(path) = directory_path(room_id) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut slots: Vec<&RosterSlotView> = dir.values().collect();
    slots.sort_by_key(|s| s.member_id); // deterministic file → clean diffs
    match serde_json::to_vec_pretty(&slots) {
        Ok(bytes) => {
            if let Err(err) = std::fs::write(&path, bytes) {
                tracing::warn!(%err, ?path, "room directory persist failed — membership survives in memory only this session");
            }
        }
        Err(err) => tracing::warn!(%err, "room directory serialize failed"),
    }
}

