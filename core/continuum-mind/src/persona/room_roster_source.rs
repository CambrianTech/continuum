//! The airc room-roster reader: who is present in a room, from airc presence.
//!
//! [`AircRosterReader`] abstracts airc room membership so the roster consumers
//! (the positron presence projection, the experience resolver, the room
//! directory) and their tests do not need a live airc daemon. The real impl
//! rides on `airc_lib::Airc`; stubs return canned `RoomMember`s.
//!
//! The prompt's roster is NOT read here: a citizen's "who is here" is the SAME
//! `RosterViewState` the web roster renders, delivered by
//! `persona::viewstate_rag` (one definition, rendered two ways). The legacy
//! `RoomRosterSource` that read this trait directly into RAG items had no
//! production constructor and is gone.

use std::time::Duration;

use airc_core::PeerId;
use airc_lib::{AircError, RoomMember};
use async_trait::async_trait;

/// How far back a heartbeat counts as "present". Matches the airc
/// agent-liveness convention of a short recency window — a peer that
/// hasn't beaten within this window is treated as gone.
pub const PRESENCE_WINDOW: Duration = Duration::from_secs(120);

/// How many recent transcript events airc scans to build the roster.
/// The presence reduction keeps one entry per peer, so this only needs
/// to span the heartbeat cadence across all present peers — not the
/// full scrollback. Passed straight to `Airc::room_roster`.
pub const ROSTER_SCAN: usize = 200;

/// Abstract reader over airc room membership. Production impl rides on
/// `airc_lib::Airc::room_roster`; tests use a stub that returns canned
/// `RoomMember`s without needing a daemon.
///
/// Slice "consume room_roster" (airc#1232): this used to be a 2-method
/// `active_agents` + `peer_alias_map` reader, with continuum re-parsing
/// `IdentityPublished` events to join names onto presence — a wire-format
/// coupling the adversarial review of continuum#1650 flagged. airc now
/// owns that join: `room_roster` returns presence + names in ONE batched
/// scan. So this is a single-method reader and the continuum-side
/// parsing is gone — thin continuum, airc owns presence+identity.
#[async_trait]
pub trait AircRosterReader: Send + Sync {
    /// This persona's own airc peer id — used to exclude self from the
    /// roster so the grounding block reads "who is NOT me". (`room_roster`
    /// includes self by design; the caller drops its own peer_id.)
    fn self_peer_id(&self) -> PeerId;

    /// Everyone present in this persona's room — presence joined with
    /// published display names in ONE airc-side batched scan, within
    /// `within`, over the most recent `window` transcript events.
    /// Newest-wins per peer; peers that signalled `Leaving` are excluded
    /// by airc; `display_name` is `None` for a present-but-unnamed peer.
    /// ROOM-PARAMETRIC (#443, measured live 2026-08-17). This took no `room`, so
    /// the source could only be BOUND at bootstrap and had to abstain on any
    /// other turn room — measured 42 abstains in 90 minutes with bound=academy,
    /// turn=bench-room. A citizen answering a turn in a per-run bench room saw
    /// NO ONE: no teammates, no peers to address, in the room she was actually
    /// standing in. `airc_lib::room_roster_in` already existed; it was never
    /// wired. `None` keeps the pre-#443 behaviour (the scope's current room).
    async fn room_roster(
        &self,
        within: Duration,
        window: usize,
        room: Option<uuid::Uuid>,
    ) -> Result<Vec<RoomMember>, AircError>;

    /// The CARDS-flavored roster (#262): presence + each peer's FULL
    /// published identity card (name/pronouns/role/bio/integrations).
    /// Default adapts the thin [`room_roster`](Self::room_roster) with
    /// `identity: None` — a reader that can't reach the identity store
    /// still yields an honest presence-only roster; `airc_lib::Airc`
    /// overrides with the real card join.
    async fn room_roster_cards(
        &self,
        within: Duration,
        window: usize,
        room: Option<uuid::Uuid>,
    ) -> Result<Vec<airc_lib::RoomMemberCard>, AircError> {
        Ok(self
            .room_roster(within, window, room)
            .await?
            .into_iter()
            .map(|m| airc_lib::RoomMemberCard {
                peer_id: m.peer_id,
                runtime: m.runtime,
                availability: m.availability,
                last_seen_ms: m.last_seen_ms,
                identity: None,
            })
            .collect())
    }
}

/// `airc_lib::Airc` satisfies the reader contract directly. Orphan rule
/// OK — the trait is ours (defined in this crate).
#[async_trait]
impl AircRosterReader for airc_lib::Airc {
    fn self_peer_id(&self) -> PeerId {
        airc_lib::Airc::peer_id(self)
    }

    // Both reads below are ONE liveness query plus cached identity cards. airc-lib's
    // `room_roster_in` / `room_roster_cards_in` round-trip the daemon per live member
    // on every call; rendered every 2 s for every room, that was ~250 requests a
    // second with citizens seated (2026-09-12) and it filled both descriptor tables.
    async fn room_roster(
        &self,
        within: Duration,
        window: usize,
        room: Option<uuid::Uuid>,
    ) -> Result<Vec<RoomMember>, AircError> {
        let live = airc_lib::Airc::active_agents_in(self, room.map(airc_core::RoomId::from_uuid), within, window).await?;
        let mut members = Vec::with_capacity(live.len());
        for liveness in live {
            let identity = crate::persona::identity_card_cache::identity_for(self, liveness.peer).await?;
            // an empty published name is an honest "unknown", not an empty display string
            let display_name = identity.map(|i| i.name).filter(|n| !n.trim().is_empty());
            members.push(RoomMember {
                peer_id: liveness.peer,
                display_name,
                runtime: liveness.runtime,
                availability: liveness.coordination.availability,
                last_seen_ms: liveness.last_seen_ms,
            });
        }
        Ok(members)
    }

    async fn room_roster_cards(
        &self,
        within: Duration,
        window: usize,
        room: Option<uuid::Uuid>,
    ) -> Result<Vec<airc_lib::RoomMemberCard>, AircError> {
        let live = airc_lib::Airc::active_agents_in(self, room.map(airc_core::RoomId::from_uuid), within, window).await?;
        let mut members = Vec::with_capacity(live.len());
        for liveness in live {
            let identity = crate::persona::identity_card_cache::identity_for(self, liveness.peer).await?;
            members.push(airc_lib::RoomMemberCard {
                peer_id: liveness.peer,
                runtime: liveness.runtime,
                availability: liveness.coordination.availability,
                last_seen_ms: liveness.last_seen_ms,
                identity,
            });
        }
        Ok(members)
    }
}
