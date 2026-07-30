//! The airc → positron **presence emitter** (task #29 / #38 identity seam).
//!
//! ## What this is
//!
//! The producing half of the `presence:updated` stream that
//! [`crate::ipc::positron_source`] consumes. That consumer projects a
//! room roster into the renderer-shaped `ChatViewState`; until this
//! emitter exists, nothing publishes `presence:updated`, so every message
//! renders with only a provisional peer-id label
//! (`inbound_attach::chat_posted_from_message` emits identity-free
//! message facts and explicitly skips presence transitions). This module
//! reads airc's owned roster, projects each member into an identity card,
//! and publishes it on the bus — the identity lookup table the consumer
//! folds in to resolve every sender's name / kind / **provenance**.
//!
//! ## Why the provenance slot rides from day one — weave, not wall
//!
//! Per `[[positron-identity-security-first-class]]`: identity is a
//! property that must hold at every seam, woven in as each seam is built,
//! never bolted on after. So every projected slot carries a
//! [`Provenance`](continuum_positron::Provenance) — the member's verifiable origin. Today that is airc's
//! self-reported `runtime` class, carried **verbatim** (the one
//! accountability axis airc surfaces now); trust tier + cryptographic
//! verification join the same struct later with no wire break (growable
//! struct-carrier, `[[adapter-capability-surface-growable-struct-carrier]]`).
//! The slot costs nothing now and is ruinous to retrofit — the whole
//! point of weaving it in at Commit 2.
//!
//! ## The one wire contract, defined once
//!
//! Per the compression principle: the emitter builds and serializes the
//! SAME [`AircPresenceUpdate`] struct the consumer deserializes, and its
//! roster rows ARE the neutral `RosterSlotView` (no hand-copied twin — the
//! shared [`roster_slot_from_member`] projection produces the identical slot
//! both the widget and the persona grounding read). Both sides agree by
//! construction, not by a hand-copied JSON literal — a round-trip test in
//! `positron_source` pins the emitter's output against the consumer's
//! `classify` path.
//!
//! ## kind is derived coarsely; runtime is the truth
//!
//! `RoomMember` carries no author `kind` — the shared projection derives one
//! from `runtime` via [`SenderKind::from_runtime`](continuum_positron::SenderKind::from_runtime), a *coarse styling hint*
//! (`"interactive"` → Human, else → Agent). The authoritative,
//! unabridged origin rides `provenance.runtime`. This is deliberate: a
//! richer `runtime → kind` table would be the string-matching smell
//! (task #70); the coarse projection stays honest by keeping the full
//! string in provenance for anyone who needs to discriminate.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use continuum_positron::chat::RosterSlotView;
use std::sync::Arc;
use std::time::Duration;

use airc_lib::RoomMember;
use uuid::Uuid;

use crate::ipc::positron_source::{roster_slot_from_member, AircPresenceUpdate, PRESENCE_UPDATED};
use crate::persona::room_roster_source::AircRosterReader;
use crate::runtime::MessageBus;
use tokio::sync::broadcast::error::RecvError;

/// How often the emitter re-reads the roster. Presence is Session-tier
/// (a human-perceivable roster delta, not a sub-second signal), and the
/// change-dedup below means an unchanged roster publishes nothing — so a
/// modest poll is cheap. This is a first-cut poll; a future airc
/// presence-delta subscription would replace it with an event-driven read
/// (design-for, not built here).
const EMIT_INTERVAL: Duration = Duration::from_secs(2);

/// Presence recency window + transcript scan depth — mirrors
/// [`crate::persona::room_roster_source`] so the widget roster and the
/// persona's grounding roster describe the same population.
const PRESENCE_WINDOW: Duration = Duration::from_secs(120);
const ROSTER_SCAN: usize = 200;

/// Membership is DURABLE; presence is live (#258/#262 — "grey, not gone",
/// Joel 2026-07-30: "Why won't bigmama's persona ever show up?"). A grid
/// citizen who has ever been seen in this room stays in the roster forever —
/// rendered `active: false` while her home node is unreachable — instead of
/// vanishing 120s after her last heartbeat. These bounds shape the one-shot
/// BOOT deep-scan that seeds the on-disk directory from the daemon's
/// persistent transcript; the 2s poll keeps using the shallow live window,
/// so steady-state daemon load is unchanged.
const MEMBERSHIP_WINDOW: Duration = Duration::from_secs(14 * 24 * 3600);
const MEMBERSHIP_SCAN: usize = 4000;

/// On-disk room directory: every `RosterSlotView` this node has ever
/// projected for the room, keyed by member id. The sibling of the airc
/// attach-cursor file — node-local durable state under `~/.continuum/state/`.
fn directory_path(room_id: &Uuid) -> Option<PathBuf> {
    dirs::home_dir().map(|h| {
        h.join(".continuum")
            .join("state")
            .join(format!("room-directory-{room_id}.json"))
    })
}

fn load_directory(room_id: &Uuid) -> HashMap<Uuid, RosterSlotView> {
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

fn persist_directory(room_id: &Uuid, dir: &HashMap<Uuid, RosterSlotView>) {
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

/// Fold freshly-projected live slots into the directory. Newest sighting
/// wins wholesale — EXCEPT a real display name never regresses to the
/// provisional `peer-xxxx` label (identity is adopted once; a card that
/// crossed the mesh yesterday must survive today's card-less sighting).
/// Returns whether anything changed (persist gate).
fn fold_into_directory(
    dir: &mut HashMap<Uuid, RosterSlotView>,
    live: &[RosterSlotView],
) -> bool {
    let mut changed = false;
    for slot in live {
        match dir.get_mut(&slot.member_id) {
            Some(existing) => {
                let mut incoming = slot.clone();
                let provisional =
                    crate::ipc::positron_source::provisional_sender_name(slot.member_id);
                if incoming.display_name == provisional && existing.display_name != provisional {
                    incoming.display_name = existing.display_name.clone();
                }
                if *existing != incoming {
                    *existing = incoming;
                    changed = true;
                }
            }
            None => {
                dir.insert(slot.member_id, slot.clone());
                changed = true;
            }
        }
    }
    changed
}

/// Live roster ∪ remembered members: everyone in the directory who is NOT
/// in the live read joins the roster as an `active: false` ghost — grey,
/// not gone. Stale liveness signals (availability, vitals) are cleared on
/// ghosts: an unreachable member with yesterday's "ready" badge or energy
/// bars would be the interface lying about liveness (#260). Ordering:
/// active first, then recency, then id — stable for the change-dedup.
fn union_with_directory(
    mut live: Vec<RosterSlotView>,
    dir: &HashMap<Uuid, RosterSlotView>,
) -> Vec<RosterSlotView> {
    let live_ids: std::collections::HashSet<Uuid> =
        live.iter().map(|s| s.member_id).collect();
    for (id, stored) in dir {
        if !live_ids.contains(id) {
            let mut ghost = stored.clone();
            ghost.active = false;
            ghost.availability = None;
            ghost.vitals = BTreeMap::new();
            live.push(ghost);
        }
    }
    live.sort_by(|a, b| {
        b.active
            .cmp(&a.active)
            .then(b.last_seen_ms.cmp(&a.last_seen_ms))
            .then(a.member_id.cmp(&b.member_id))
    });
    live
}

/// Bus event a presence CONSUMER publishes to demand a fresh
/// `presence:updated`, bypassing the emitter's change-dedup. See
/// [`request_presence_resync`]. `presence:`-prefixed → the bus treats it as
/// realtime (never coalesced), so a cue is never dropped in favour of a
/// stale one.
const PRESENCE_RESYNC: &str = "presence:resync";

/// A booting or reconnecting presence CONSUMER (the chat / wall / kanban
/// projectors) calls this on boot to demand the emitter re-publish its
/// current roster, bypassing the emitter's change-dedup.
///
/// ## Why the cue exists (#118)
///
/// The emitter fires `presence:updated` only when the roster *changes*
/// (change-dedup — an idle re-render is a wasted revision bump). On a
/// stable roster it fires once, then goes silent. A projector that
/// (re)starts *after* that single fire never receives presence — it holds
/// a roster-empty view until the roster next happens to change. The cue
/// closes that gap: the late consumer asks, the emitter re-asserts.
///
/// ## Why it is room-agnostic (and the invariant that makes it safe)
///
/// The cue carries no room_id. The chat projection is room-following (it
/// does not know its room at boot — it switches rooms as events arrive),
/// so it could not name a room even if the wire had a slot. Each per-room
/// emitter re-asserts only its OWN roster on any cue.
///
/// This is correct **only while at most one room has a live emitter** —
/// the single-room reality today (one `spawn_node_presence_emitter` call
/// against the bootstrap room). It is NOT safe by "consumers filter by
/// room": the chat projector does the opposite — `apply_presence` calls
/// `switch_room(update.room_id)`, *adopting* the update's room and wiping
/// the previously-focused room's messages + roster. Under a single emitter
/// that is harmless (every `presence:updated` names the same room). But the
/// moment a second room's emitter exists, a broadcast cue makes every
/// emitter re-publish, and room B's `presence:updated` would yank the chat
/// projector off room A — wiping the user's view. The room-following
/// property is exactly what makes the room-agnostic cue unsafe once
/// multiple emitters coexist.
///
/// So before multi-room emitters land, either put a room_id on the cue so
/// a consumer requests only its focused room, OR guard `apply_presence` to
/// drop an update whose room ≠ the focused room (follow only on an explicit
/// `switch_room` message, not on presence). The second half now exists for
/// the explicit case: once a citizen runs `nav/select`, the chat projection
/// holds an `explicit_focus` pin and drops other rooms' updates (see
/// `ChatProjection::pinned_away_from`); pre-select it still follows events.
/// Until multi-room emitters land the broadcast cue is correct and its only
/// cost is one idle roster re-read per emitter.
pub(crate) fn request_presence_resync(bus: &MessageBus) {
    // A payload-free cue: the event name IS the whole signal. `Null` is the
    // honest "no data" — a resync *requests* a roster, it does not carry
    // one ([[fallbacks-are-illegal-fail-loud]]: no fabricated payload).
    bus.publish_async_only(PRESENCE_RESYNC, serde_json::Value::Null);
}

/// Project airc's owned roster into a `presence:updated` payload for one
/// room. Pure — no bus, no clock — so the mapping is unit-testable and
/// the round-trip against the consumer's `classify` is a plain function
/// call.
///
/// Unlike [`crate::persona::room_roster_source`] (which drops `self`
/// because it grounds a persona in "who is NOT me"), the widget roster
/// shows **every** present member including self — a chat roster you are
/// absent from would be wrong. Every member airc returns becomes a slot.
/// The node's avatar-image store: `~/.continuum/avatars/<peer-id>.png`. The
/// presence emitter is the ONE place this disk fact meets the wire — the
/// shared [`roster_slot_from_member`] projection stays pure (no I/O), and the
/// persona-grounding rail (which has no use for pixels) never pays for it.
fn avatar_store_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".continuum").join("avatars"))
}

/// Scan the avatar store once: every `<uuid>.png` becomes `peer id →
/// "/avatars/<uuid>.png"` (the URL path the client's static tier serves).
/// Non-uuid names (named art, emote sets, subdirs) are not member avatars and
/// are skipped. A missing/unreadable store is the honest empty map — members
/// simply carry no `avatar_url`, never a fabricated one.
fn scan_avatar_store(dir: &Path) -> HashMap<Uuid, String> {
    let mut map = HashMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return map;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(stem) = name.strip_suffix(".png") {
            if let Ok(id) = Uuid::parse_str(stem) {
                map.insert(id, format!("/avatars/{name}"));
            }
        }
    }
    map
}

pub(crate) fn project_presence(
    members: Vec<airc_lib::RoomMemberCard>,
    room_id: Uuid,
    room_name: String,
    avatars: &HashMap<Uuid, String>,
) -> AircPresenceUpdate {
    // ONE projection for both rails — the WS widget roster and the persona's
    // grounding roster are the same neutral `RosterSlotView`, built here by
    // the shared [`roster_slot_from_member`] (the convergence #8/#13). No
    // local twin: this emitter is now purely "read airc → project → publish".
    //
    // Unlike the persona-grounding source (which drops `self` because it
    // grounds a persona in "who is NOT me"), the widget roster shows EVERY
    // present member including self — a chat roster you are absent from would
    // be wrong. Self-exclusion is the persona source's own post-projection
    // policy, never baked into the shared projection.
    let roster = members
        .iter()
        .map(|m| {
            // #262: the CARDS projection — pronouns/role/bio/integrations
            // from each peer's published identity card ride into every slot.
            let mut slot = crate::ipc::positron_source::roster_slot_from_card(m);
            // Enrich with the node's stored avatar image, when one exists for
            // this peer — the URL only; absent stays honestly absent.
            slot.avatar_url = avatars.get(&slot.member_id).cloned();
            slot
        })
        .collect();
    AircPresenceUpdate {
        room_id,
        room_name,
        roster,
    }
}

/// Subscribe an airc roster reader to a room and emit `presence:updated`
/// whenever the roster changes. Spawns a tick loop on `rt`; each tick
/// reads the roster, projects it, and publishes only when the projection
/// differs from the last published one (change-dedup — an idle-tick
/// re-render is wasted work and a wasted revision bump).
///
/// A roster read error skips the tick and keeps the last published roster
/// on the widget (the reader — `airc_lib::Airc` — owns reconnection per
/// `[[persona-airc-resilience]]`; a transient poll failure must not blink
/// the roster empty). This is resilience, not a fallback: no fabricated
/// data is ever substituted; the emitter simply waits for the next good
/// read.
pub fn spawn_presence_emitter(
    rt: &tokio::runtime::Handle,
    reader: Arc<dyn AircRosterReader>,
    room_id: Uuid,
    room_name: String,
    bus: Arc<MessageBus>,
) {
    rt.spawn(run_presence_loop(reader, room_id, room_name, bus));
}

/// The tick loop, shared by every spawn entry point. Reads the roster,
/// projects it, and publishes only on change. Kept as a standalone
/// future so a caller that already holds a reader
/// ([`spawn_presence_emitter`]) and one that must first *attach* a reader
/// ([`spawn_node_presence_emitter`]) run the identical loop — one wire
/// contract, one dedup discipline, defined once.
async fn run_presence_loop(
    reader: Arc<dyn AircRosterReader>,
    room_id: Uuid,
    room_name: String,
    bus: Arc<MessageBus>,
) {
    let mut ticker = tokio::time::interval(EMIT_INTERVAL);
    let mut rx = bus.receiver();
    let mut last_published: Option<AircPresenceUpdate> = None;
    // Durable room directory (#258/#262): load what this node already knows,
    // then seed it with ONE deep transcript scan so members whose last event
    // predates the live window exist from the first publish. Disk I/O off
    // the async tick (spawn_blocking) per CONCURRENCY-STYLE-GUIDE.
    let mut directory: HashMap<Uuid, RosterSlotView> = tokio::task::spawn_blocking(move || {
        load_directory(&room_id)
    })
    .await
    .unwrap_or_default();
    match reader.room_roster_cards(MEMBERSHIP_WINDOW, MEMBERSHIP_SCAN).await {
        Ok(members) => {
            let seeded = project_presence(members, room_id, room_name.clone(), &HashMap::new());
            if fold_into_directory(&mut directory, &seeded.roster) {
                let snapshot = directory.clone();
                let _ = tokio::task::spawn_blocking(move || persist_directory(&room_id, &snapshot))
                    .await;
            }
            tracing::info!(
                %room_id,
                remembered = directory.len(),
                probe_class = "presence.directory.seeded",
                "room directory seeded from deep transcript scan"
            );
        }
        Err(err) => tracing::warn!(
            %err,
            %room_id,
            "membership deep scan failed — directory holds prior knowledge only (live ticks still fold)"
        ),
    }
    // The avatar store map, refreshed off-task (spawn_blocking — disk I/O never
    // rides the async tick, CONCURRENCY-STYLE-GUIDE) on a slow cadence: presence
    // is 2s, avatar files change on human timescales. Refreshed every
    // AVATAR_RESCAN_TICKS ticks and on a resync cue.
    const AVATAR_RESCAN_TICKS: u32 = 15;
    let mut avatars: HashMap<Uuid, String> = HashMap::new();
    let mut ticks_until_rescan: u32 = 0;
    // Once the bus closes (all senders dropped — process teardown) no
    // consumer is left to cue us; disable the resync arm so the select does
    // not busy-loop on `Closed`, and keep ticking (the emitter still owns
    // the roster read regardless of any consumer).
    let mut bus_open = true;
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if ticks_until_rescan == 0 {
                    ticks_until_rescan = AVATAR_RESCAN_TICKS;
                    avatars = rescan_avatars().await;
                } else {
                    ticks_until_rescan -= 1;
                }
                // Idle re-read: publish only on a real change (dedup).
                if emit_once(&reader, room_id, &room_name, &bus, &mut last_published, &avatars, &mut directory, false).await {
                    let snapshot = directory.clone();
                    tokio::task::spawn_blocking(move || persist_directory(&room_id, &snapshot));
                }
            }
            recv = rx.recv(), if bus_open => match recv {
                // A booting/reconnecting presence CONSUMER demands the
                // current roster. Force a re-publish even if unchanged —
                // the whole point of the cue (#118): a projector that
                // (re)started after our last publish would otherwise hold a
                // roster-empty view until the roster next changed.
                Ok(event) if event.name == PRESENCE_RESYNC => {
                    avatars = rescan_avatars().await;
                    ticks_until_rescan = AVATAR_RESCAN_TICKS;
                    if emit_once(&reader, room_id, &room_name, &bus, &mut last_published, &avatars, &mut directory, true).await {
                        let snapshot = directory.clone();
                        tokio::task::spawn_blocking(move || persist_directory(&room_id, &snapshot));
                    }
                }
                // Any other bus traffic is not ours — ignore.
                Ok(_) => {}
                // Fell behind the broadcast buffer: some cues were dropped,
                // but the next tick re-reads the roster anyway, so a missed
                // resync only delays a late consumer by one EMIT_INTERVAL.
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => bus_open = false,
            }
        }
    }
}

/// Read the roster once, project it, and publish `presence:updated`.
///
/// `force = false` (the idle tick path) publishes only when the projection
/// differs from `last_published` — change-dedup, so an idle re-render is
/// not a wasted revision bump. `force = true` (a resync cue) bypasses the
/// dedup and always publishes: a late consumer needs the current roster
/// re-asserted even when it is unchanged.
///
/// A roster read error skips this emit and keeps the last published roster
/// on the widget (the reader — `airc_lib::Airc` — owns reconnection per
/// `[[persona-airc-resilience]]`; a transient poll failure must not blink
/// the roster empty). This is resilience, not a fallback: no fabricated
/// data is ever substituted; the emitter simply waits for the next good
/// read.
/// Refresh the avatar-store map off the async tick (`spawn_blocking` — a
/// read_dir is disk I/O, CONCURRENCY-STYLE-GUIDE). A join failure (worker
/// panic/cancel) keeps the empty map — honest absence, logged, never a crash
/// of the presence loop.
async fn rescan_avatars() -> HashMap<Uuid, String> {
    let Some(dir) = avatar_store_dir() else {
        return HashMap::new();
    };
    match tokio::task::spawn_blocking(move || scan_avatar_store(&dir)).await {
        Ok(map) => map,
        Err(err) => {
            tracing::warn!(error = %err, "positron_presence: avatar store scan failed — no avatar urls this round");
            HashMap::new()
        }
    }
}

async fn emit_once(
    reader: &Arc<dyn AircRosterReader>,
    room_id: Uuid,
    room_name: &str,
    bus: &MessageBus,
    last_published: &mut Option<AircPresenceUpdate>,
    avatars: &HashMap<Uuid, String>,
    directory: &mut HashMap<Uuid, RosterSlotView>,
    force: bool,
) -> bool {
    let members = match reader.room_roster_cards(PRESENCE_WINDOW, ROSTER_SCAN).await {
        Ok(m) => m,
        Err(err) => {
            tracing::warn!(
                error = %err,
                %room_id,
                "positron_presence: room_roster failed — skip emit, keep last roster (reader owns reconnection)"
            );
            return false;
        }
    };
    let mut update = project_presence(members, room_id, room_name.to_string(), avatars);
    // Durable membership ∪ live presence (#258/#262): fold this sighting into
    // the directory, then publish live members PLUS remembered members as
    // `active: false` ghosts — a citizen whose home node is unreachable is
    // grey, never gone. Persistence is the CALLER's concern (the loop
    // persists on `true`; tests stay disk-free — the #7 isolation lesson).
    let directory_changed = fold_into_directory(directory, &update.roster);
    update.roster = union_with_directory(update.roster, directory);
    if !force && last_published.as_ref() == Some(&update) {
        return directory_changed;
    }
    // Substrate-owned type: a serialize failure is a bug, not a
    // runtime condition (same discipline as
    // `continuum_positron::StateBuilder::build`).
    let payload = serde_json::to_value(&update)
        .expect("AircPresenceUpdate must serialize — bug, not a runtime error");
    bus.publish_async_only(PRESENCE_UPDATED, payload);
    *last_published = Some(update);
    directory_changed
}

/// Fixed identity name for the node-level presence reader. It attaches
/// as a **heartbeat-less lurker** (`attach_as` opens + wires a daemon
/// client but never spawns a `HeartbeatTask`), so it reads the daemon's
/// authoritative roster without ever appearing in it — an invisible
/// observer, never a phantom occupant. The name is stable so the reader
/// resumes the same keypair across reboots.
const NODE_PRESENCE_READER_NAME: &str = "continuum-node";

/// Attach a node-level roster reader and run the presence emitter for one
/// room. This is the **producing half** of the `presence:updated` stream:
/// without it, nothing publishes presence and every rendered message
/// carries only a provisional peer-id label.
///
/// ## Why a dedicated node reader, not a persona's handle
///
/// The widget roster must survive **zero resident personas** — a chat
/// window with no persona present still shows its human occupants. So the
/// reader is node-scoped, not borrowed from a persona lifecycle. It reuses
/// the exact non-persona attach shape [`crate::context::agent`] uses
/// (`Airc::attach_as` → `AircHandleAdapter`), attaching a heartbeat-less
/// lurker at a stable node home. `room_roster` routes through the daemon
/// (`page_recent` is daemon-aware), so the lurker reads every *other*
/// peer's presence from the shared transcript while contributing none of
/// its own.
///
/// A failed initial attach/join means the projection can't start; it logs
/// the cause loudly and the task exits (the WS server is optional, so this
/// is a disabled feature, not a substrate-wide panic —
/// [[fallbacks-are-illegal-fail-loud]]: no fabricated roster is ever
/// substituted).
pub fn spawn_node_presence_emitter(
    rt: &tokio::runtime::Handle,
    daemon_socket: std::path::PathBuf,
    node_home: std::path::PathBuf,
    room_id: Uuid,
    room_name: String,
    bus: Arc<MessageBus>,
) {
    rt.spawn(async move {
        if let Err(err) = tokio::fs::create_dir_all(&node_home).await {
            tracing::error!(
                error = %err,
                home = %node_home.display(),
                "positron_presence: cannot create node reader home — presence projection disabled"
            );
            return;
        }
        let airc = match airc_lib::Airc::attach_as(
            node_home.clone(),
            NODE_PRESENCE_READER_NAME,
            daemon_socket,
        )
        .await
        {
            Ok(airc) => airc,
            Err(err) => {
                tracing::error!(
                    error = %err,
                    home = %node_home.display(),
                    "positron_presence: node reader attach failed — presence projection disabled"
                );
                return;
            }
        };
        // Join by NAME (never UUID-as-string, which derives a *different*
        // channel — the recurring hazard documented in
        // `context::agent` + `PersonaAircRuntime`). The reader must share
        // the operator's channel or its roster reads land in an empty
        // derived room.
        if let Err(err) = airc.join(&room_name).await {
            tracing::error!(
                error = %err,
                room = %room_name,
                "positron_presence: node reader could not join room — presence projection disabled"
            );
            return;
        }
        let reader: Arc<dyn AircRosterReader> =
            Arc::new(crate::context::airc_adapter::AircHandleAdapter::new(Arc::new(airc)));
        tracing::info!(
            %room_id,
            room = %room_name,
            "positron_presence: node reader attached — emitting presence:updated"
        );
        run_presence_loop(reader, room_id, room_name, bus).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use airc_core::PeerId;
    use airc_lib::AircError;
    use async_trait::async_trait;
    // The module proper no longer names `SenderKind` (the coarse kind is
    // derived inside the shared `roster_slot_from_member` projection now);
    // the tests still assert on the projected slot's `kind`.
    use continuum_positron::SenderKind;

    /// A roster reader that returns a fixed member list — enough to drive
    /// [`emit_once`]'s read → project → publish path without a daemon.
    struct StubRosterReader {
        members: Vec<airc_lib::RoomMemberCard>,
    }

    #[async_trait]
    impl AircRosterReader for StubRosterReader {
        fn self_peer_id(&self) -> PeerId {
            PeerId::new()
        }

        async fn room_roster(
            &self,
            _within: Duration,
            _window: usize,
        ) -> Result<Vec<RoomMember>, AircError> {
            Ok(self
                .members
                .iter()
                .map(|c| RoomMember {
                    peer_id: c.peer_id,
                    display_name: c.identity.as_ref().map(|i| i.name.clone()),
                    runtime: c.runtime.clone(),
                    availability: c.availability,
                    last_seen_ms: c.last_seen_ms,
                })
                .collect())
        }

        async fn room_roster_cards(
            &self,
            _within: Duration,
            _window: usize,
        ) -> Result<Vec<airc_lib::RoomMemberCard>, AircError> {
            Ok(self.members.clone())
        }
    }

    /// Build a `RoomMember` — `name: Some` mirrors a peer that published
    /// an identity card; `None` mirrors present-but-unnamed. Mirrors the
    /// `room_roster_source` test builder so both sides describe the same
    /// airc shape.
    fn member(peer: PeerId, runtime: &str, name: Option<&str>) -> airc_lib::RoomMemberCard {
        airc_lib::RoomMemberCard {
            peer_id: peer,
            runtime: runtime.to_string(),
            availability: None,
            last_seen_ms: 1_000_000,
            identity: name.map(|s| airc_core::identity::Identity::new(s.to_string())),
        }
    }

    // what this catches: the identity card the emitter produces must
    // carry the member's verifiable origin (provenance.runtime, verbatim)
    // AND a coarse neutral kind — the accountability guarantee Commit 2
    // exists to weave in. A regression that dropped provenance, or that
    // string-matched runtime into kind instead of carrying it whole,
    // would leave every rendered row unattributable.
    #[test]
    fn project_carries_origin_verbatim_and_coarse_kind() {
        let named = PeerId::new();
        let unnamed = PeerId::new();
        let human = PeerId::new();
        let update = project_presence(
            vec![
                member(named, "claude", Some("win-claude")),
                member(unnamed, "codex", None),
                member(human, "interactive", Some("Joel")),
            ],
            Uuid::from_u128(0xa),
            "general".into(),
            &HashMap::new(),
        );
        assert_eq!(update.roster.len(), 3, "every present member is a slot");

        let card = &update.roster[0];
        assert_eq!(card.member_id, named.as_uuid());
        assert_eq!(card.display_name, "win-claude");
        assert_eq!(card.kind, SenderKind::Agent);
        assert_eq!(
            card.provenance.runtime, "claude",
            "the full runtime origin is carried verbatim, not lost to the coarse kind"
        );
        assert!(
            card.integrations.is_empty(),
            "no airc badge map yet — empty is honest, not fabricated"
        );
        assert!(card.active, "a present member is active");

        // Unnamed peer → the SAME provisional short-peer label the
        // consumer uses; still an Agent by its codex runtime.
        let anon = &update.roster[1];
        assert!(
            anon.display_name.starts_with("peer-"),
            "unnamed citizen is labelled, never invisible"
        );
        assert_eq!(anon.kind, SenderKind::Agent);
        assert_eq!(anon.provenance.runtime, "codex");

        // The one interactive runtime → Human (the coarse hint), origin
        // still whole in provenance.
        let carbon = &update.roster[2];
        assert_eq!(carbon.kind, SenderKind::Human);
        assert_eq!(carbon.provenance.runtime, "interactive");
    }

    // what this catches: #258/#262 grey-not-gone (Joel 2026-07-30: "Why won't
    // bigmama's persona ever show up?"). (1) A remembered member absent from
    // the live read joins the published roster as an `active: false` ghost
    // with stale liveness signals (availability/vitals) cleared — never
    // vanished, never lying about liveness. (2) A real display name adopted
    // once never regresses to the provisional peer label on a card-less
    // sighting. (3) A member reappearing live replaces its ghost. Before
    // this, anyone silent >120s ceased to exist in the roster — BigMama's
    // citizens were perpetually unborn on this node.
    #[test]
    fn directory_keeps_offline_members_grey_and_names_never_regress() {
        let kimi = PeerId::new();
        let local = PeerId::new();
        // Sighting 1: Kimi live, named (her card crossed once).
        let named = crate::ipc::positron_source::roster_slot_from_card(&member(kimi, "persona", Some("Kimi")));
        let mut dir = HashMap::new();
        assert!(fold_into_directory(&mut dir, std::slice::from_ref(&named)));

        // Her node drops off: live read has only the local member.
        let live = vec![crate::ipc::positron_source::roster_slot_from_card(&member(local, "interactive", Some("Joel")))];
        let published = union_with_directory(live.clone(), &dir);
        let ghost = published
            .iter()
            .find(|s| s.member_id == kimi.as_uuid())
            .expect("remembered member must stay in the roster");
        assert!(!ghost.active, "unreachable member renders grey, not gone");
        assert_eq!(ghost.display_name, "Kimi", "adopted identity survives the outage");
        assert!(ghost.availability.is_none() && ghost.vitals.is_empty(),
            "stale liveness signals cleared — the roster never lies about liveness");
        assert!(published[0].active, "active members sort before ghosts");

        // Later sighting WITHOUT a card (provisional label) must not erase her name.
        let unnamed = crate::ipc::positron_source::roster_slot_from_card(&member(kimi, "persona", None));
        fold_into_directory(&mut dir, std::slice::from_ref(&unnamed));
        assert_eq!(dir[&kimi.as_uuid()].display_name, "Kimi",
            "a card-less sighting never regresses an adopted name");

        // She reappears live: the ghost is replaced by the live slot.
        let back = vec![crate::ipc::positron_source::roster_slot_from_card(&member(kimi, "persona", Some("Kimi")))];
        let published = union_with_directory(back, &dir);
        assert_eq!(published.iter().filter(|s| s.member_id == kimi.as_uuid()).count(), 1);
        assert!(published.iter().find(|s| s.member_id == kimi.as_uuid()).unwrap().active);
    }

    // what this catches: the avatar-image enrichment — a peer with a stored
    // `<uuid>.png` carries its URL on the slot, everyone else stays honestly
    // absent (glyph fallback, never a broken image). And the store scan maps
    // ONLY uuid-named .png files — named art / emote subdirs are not member
    // avatars and must not leak onto arbitrary slots.
    #[test]
    fn avatar_url_enriches_only_members_with_a_stored_image() {
        let pictured = PeerId::new();
        let plain = PeerId::new();
        let mut avatars = HashMap::new();
        avatars.insert(
            pictured.as_uuid(),
            format!("/avatars/{}.png", pictured.as_uuid()),
        );
        let update = project_presence(
            vec![
                member(pictured, "claude", Some("win-claude")),
                member(plain, "codex", None),
            ],
            Uuid::from_u128(0xd),
            "general".into(),
            &avatars,
        );
        assert_eq!(
            update.roster[0].avatar_url.as_deref(),
            Some(format!("/avatars/{}.png", pictured.as_uuid()).as_str()),
            "a stored avatar rides the slot as its URL"
        );
        assert_eq!(
            update.roster[1].avatar_url, None,
            "no stored image → honest absent, never a fabricated face"
        );

        // The store scan: uuid-named .png only.
        let dir = std::env::temp_dir().join(format!("avatar-scan-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("mk tmp store");
        let id = Uuid::new_v4();
        std::fs::write(dir.join(format!("{id}.png")), b"png").expect("write");
        std::fs::write(dir.join("asha.png"), b"png").expect("write");
        std::fs::write(dir.join(format!("{id}-happy.png")), b"png").expect("write");
        std::fs::create_dir_all(dir.join("emote")).expect("subdir");
        let map = scan_avatar_store(&dir);
        assert_eq!(map.len(), 1, "only exact <uuid>.png names are member avatars");
        assert_eq!(map.get(&id).map(String::as_str), Some(format!("/avatars/{id}.png").as_str()));
        std::fs::remove_dir_all(&dir).ok();
    }

    // what this catches: the widget roster must include self — unlike the
    // persona-grounding roster (which drops self). A regression that
    // copied the self-exclusion here would erase the local user from
    // their own room roster.
    #[test]
    fn self_is_included_in_the_widget_roster() {
        let me = PeerId::new();
        let update = project_presence(
            vec![member(me, "interactive", Some("Joel"))],
            Uuid::from_u128(0xb),
            "general".into(),
            &HashMap::new(),
        );
        assert_eq!(update.roster.len(), 1);
        assert_eq!(update.roster[0].member_id, me.as_uuid());
    }

    // what this catches: the emitter's serialized output must survive a
    // round-trip through the exact wire structs the consumer
    // deserializes — the "both sides agree by construction" contract. A
    // divergence (e.g. a rename_all mismatch or a non-serde field) would
    // silently drop the roster on the consumer side.
    #[test]
    fn serialized_update_round_trips_the_wire_shape() {
        let update = project_presence(
            vec![member(PeerId::new(), "claude", Some("win-claude"))],
            Uuid::from_u128(0xc),
            "general".into(),
            &HashMap::new(),
        );
        let json = serde_json::to_value(&update).expect("serializes");
        let back: AircPresenceUpdate = serde_json::from_value(json).expect("round-trips");
        assert_eq!(update, back);
    }

    // what this catches: the late-subscriber gap (#118). The idle tick
    // dedups an unchanged roster (publishes nothing on the second call);
    // a resync cue must set `force` and re-publish the SAME roster anyway,
    // so a projector that booted after the emitter's one-and-only publish
    // still receives presence. A regression that let the dedup swallow a
    // forced emit would leave that projector roster-empty forever.
    #[tokio::test]
    async fn resync_forces_republish_of_unchanged_roster() {
        let reader: Arc<dyn AircRosterReader> = Arc::new(StubRosterReader {
            members: vec![member(PeerId::new(), "claude", Some("win-claude"))],
        });
        let bus = MessageBus::new();
        let mut rx = bus.receiver();
        let room = Uuid::from_u128(0x1);
        let mut last = None;
        let mut dir = HashMap::new();

        // First idle emit publishes (roster changed None → Some).
        emit_once(&reader, room, "general", &bus, &mut last, &HashMap::new(), &mut dir, false).await;
        let first = rx.try_recv().expect("first emit publishes presence:updated");
        assert_eq!(first.name, PRESENCE_UPDATED);

        // Second idle emit dedups — nothing published.
        emit_once(&reader, room, "general", &bus, &mut last, &HashMap::new(), &mut dir, false).await;
        assert!(
            rx.try_recv().is_err(),
            "an unchanged roster dedups on the idle path"
        );

        // A resync cue forces a re-publish of the unchanged roster.
        emit_once(&reader, room, "general", &bus, &mut last, &HashMap::new(), &mut dir, true).await;
        let forced = rx
            .try_recv()
            .expect("a resync forces a re-publish even when the roster is unchanged");
        assert_eq!(forced.name, PRESENCE_UPDATED);
    }

    // what this catches: the consumer-side half of the cue — a booting
    // projector must actually emit `presence:resync` on the bus so the
    // emitter's select arm fires. A regression that renamed the event or
    // dropped the publish would silently reopen the late-subscriber gap.
    #[tokio::test]
    async fn request_presence_resync_publishes_the_cue() {
        let bus = MessageBus::new();
        let mut rx = bus.receiver();
        request_presence_resync(&bus);
        let cue = rx
            .try_recv()
            .expect("request_presence_resync publishes a cue event");
        assert_eq!(cue.name, PRESENCE_RESYNC);
    }
}
