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
pub(crate) fn project_presence(
    members: Vec<RoomMember>,
    room_id: Uuid,
    room_name: String,
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
    let roster = members.iter().map(roster_slot_from_member).collect();
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
    // Once the bus closes (all senders dropped — process teardown) no
    // consumer is left to cue us; disable the resync arm so the select does
    // not busy-loop on `Closed`, and keep ticking (the emitter still owns
    // the roster read regardless of any consumer).
    let mut bus_open = true;
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                // Idle re-read: publish only on a real change (dedup).
                emit_once(&reader, room_id, &room_name, &bus, &mut last_published, false).await;
            }
            recv = rx.recv(), if bus_open => match recv {
                // A booting/reconnecting presence CONSUMER demands the
                // current roster. Force a re-publish even if unchanged —
                // the whole point of the cue (#118): a projector that
                // (re)started after our last publish would otherwise hold a
                // roster-empty view until the roster next changed.
                Ok(event) if event.name == PRESENCE_RESYNC => {
                    emit_once(&reader, room_id, &room_name, &bus, &mut last_published, true).await;
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
async fn emit_once(
    reader: &Arc<dyn AircRosterReader>,
    room_id: Uuid,
    room_name: &str,
    bus: &MessageBus,
    last_published: &mut Option<AircPresenceUpdate>,
    force: bool,
) {
    let members = match reader.room_roster(PRESENCE_WINDOW, ROSTER_SCAN).await {
        Ok(m) => m,
        Err(err) => {
            tracing::warn!(
                error = %err,
                %room_id,
                "positron_presence: room_roster failed — skip emit, keep last roster (reader owns reconnection)"
            );
            return;
        }
    };
    let update = project_presence(members, room_id, room_name.to_string());
    if !force && last_published.as_ref() == Some(&update) {
        return;
    }
    // Substrate-owned type: a serialize failure is a bug, not a
    // runtime condition (same discipline as
    // `continuum_positron::StateBuilder::build`).
    let payload = serde_json::to_value(&update)
        .expect("AircPresenceUpdate must serialize — bug, not a runtime error");
    bus.publish_async_only(PRESENCE_UPDATED, payload);
    *last_published = Some(update);
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
        members: Vec<RoomMember>,
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
            Ok(self.members.clone())
        }
    }

    /// Build a `RoomMember` — `name: Some` mirrors a peer that published
    /// an identity card; `None` mirrors present-but-unnamed. Mirrors the
    /// `room_roster_source` test builder so both sides describe the same
    /// airc shape.
    fn member(peer: PeerId, runtime: &str, name: Option<&str>) -> RoomMember {
        RoomMember {
            peer_id: peer,
            display_name: name.map(|s| s.to_string()),
            runtime: runtime.to_string(),
            availability: None,
            last_seen_ms: 1_000_000,
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

        // First idle emit publishes (roster changed None → Some).
        emit_once(&reader, room, "general", &bus, &mut last, false).await;
        let first = rx.try_recv().expect("first emit publishes presence:updated");
        assert_eq!(first.name, PRESENCE_UPDATED);

        // Second idle emit dedups — nothing published.
        emit_once(&reader, room, "general", &bus, &mut last, false).await;
        assert!(
            rx.try_recv().is_err(),
            "an unchanged roster dedups on the idle path"
        );

        // A resync cue forces a re-publish of the unchanged roster.
        emit_once(&reader, room, "general", &bus, &mut last, true).await;
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
