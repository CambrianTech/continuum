//! `LiveExperienceResolver` — the wiring seam that joins a room's recipe-projected
//! [`Experience`] manifest with its LIVE airc roster into a full manifest ready to
//! render.
//!
//! ## Separation of concerns
//! Three layers meet here and nothing reinvents its neighbour:
//! - **continuum contract** — the recipe → static manifest (purpose / regions /
//!   affordances / layout), from [`SharedExperienceSource`].
//! - **airc presence** — who is in the room, read through the existing
//!   [`AircRosterReader`] (real impl on the airc handle; stub in tests). Untouched.
//! - **this resolver** — the *only* new thing: it composes the two and overlays
//!   structural [`Standing`]. It owns no state, no transport, no roster logic.
//!
//! This lives in `ipc/` (the projection/serving boundary), not in `experience/`
//! (the pure contract), so the contract stays free of async and of any dependency
//! on the persona roster reader.

use std::collections::BTreeMap;
use std::sync::Arc;

use uuid::Uuid;

use crate::experience::source::SharedExperienceSource;
use crate::experience::{project_membership, Experience, Standing};
use crate::persona::room_roster_source::AircRosterReader;
use crate::persona::room_roster_source::{PRESENCE_WINDOW, ROSTER_SCAN};

/// A peer is "present" if it beat within this window — matches the airc
/// agent-liveness convention (same value the RoomRosterSource uses).

/// How many recent transcript events airc scans to build the roster (one entry per
/// peer after presence reduction) — the same scan depth as the RoomRosterSource.

/// Composes a room's manifest from recipe DATA + the live airc roster. The roster
/// reader is room-scoped (it reads its handle's current room); callers use this in
/// the context where `room_id` and the reader refer to the same room (the
/// per-persona serving path).
pub struct LiveExperienceResolver {
    source: SharedExperienceSource,
    roster: Arc<dyn AircRosterReader>,
}

impl LiveExperienceResolver {
    pub fn new(source: SharedExperienceSource, roster: Arc<dyn AircRosterReader>) -> Self {
        Self { source, roster }
    }

    /// Resolve a room to its full [`Experience`]: recipe → static manifest, live
    /// roster → `membership`. `roles` overlays structural standings (e.g. a
    /// benchmark's examinee / the human owner) onto present peers; a peer with no
    /// role is a plain [`Standing::Member`].
    ///
    /// Fail-soft on a degraded roster read: the manifest still resolves with empty
    /// membership + a `warn`, matching the RoomRosterSource doctrine
    /// (`[[substrate-is-a-good-citizen-on-the-host]]`) — cognition and rendering
    /// stay up even when airc presence is momentarily unavailable. Returns `None`
    /// only when there is no recipe for the room's purpose (fail-loud on the
    /// contract, never a fabricated manifest).
    pub async fn resolve(
        &self,
        room_id: Uuid,
        roles: &BTreeMap<String, Standing>,
    ) -> Option<Experience> {
        let manifest = self.source.experience_for(room_id)?;
        let members = match self.roster.room_roster(PRESENCE_WINDOW, ROSTER_SCAN).await {
            Ok(members) => project_membership(&members, roles),
            Err(error) => {
                tracing::warn!(
                    target: "experience",
                    room = %room_id,
                    %error,
                    "roster read failed; resolving manifest with empty membership"
                );
                Vec::new()
            }
        };
        Some(manifest.with_membership(members))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experience::RecipeExperienceSource;
    use crate::ipc::room_purpose::{RoomPurposeSource, SharedRoomPurpose};
    use airc_core::PeerId;
    use airc_lib::RoomMember;
    use async_trait::async_trait;
    use std::time::Duration;

    struct FixedPurpose(&'static str);
    impl RoomPurposeSource for FixedPurpose {
        fn purpose_for(&self, _room_id: Uuid) -> String {
            self.0.to_string()
        }
    }

    struct StubRoster {
        me: PeerId,
        members: Vec<RoomMember>,
    }
    #[async_trait]
    impl AircRosterReader for StubRoster {
        fn self_peer_id(&self) -> PeerId {
            self.me
        }
        async fn room_roster(
            &self,
            _within: Duration,
            _window: usize,
        ) -> Result<Vec<RoomMember>, airc_lib::AircError> {
            Ok(self.members.clone())
        }
    }

    fn room_member(id: Uuid, runtime: &str) -> RoomMember {
        RoomMember {
            peer_id: PeerId(id),
            display_name: None,
            runtime: runtime.to_string(),
            availability: None,
            last_seen_ms: 0,
        }
    }

    // what this catches: the whole wiring seam. A real room resolves to its recipe
    // manifest WITH membership drawn from the live airc roster (reused, not
    // reinvented), and structural standing overlaid per role — a human Owner and a
    // persona Examinee sitting in ONE membership list. This is equal citizenship made
    // concrete: the manifest carries all three kinds of citizen uniformly; only the
    // recipe's authored regions/affordances/authz differ.
    #[tokio::test]
    async fn resolves_manifest_with_membership_from_live_roster() {
        let joel = Uuid::from_u128(1);
        let asha = Uuid::from_u128(2);

        let purpose: SharedRoomPurpose = Arc::new(FixedPurpose("benchmark/hard-rs"));
        let source: SharedExperienceSource = Arc::new(RecipeExperienceSource::builtins(purpose));
        let roster = Arc::new(StubRoster {
            me: PeerId(asha),
            members: vec![
                room_member(joel, "interactive"),
                room_member(asha, "persona"),
            ],
        });
        let resolver = LiveExperienceResolver::new(source, roster);

        let mut roles = BTreeMap::new();
        roles.insert(joel.to_string(), Standing::Owner);
        roles.insert(asha.to_string(), Standing::Examinee);

        let exp = resolver
            .resolve(Uuid::from_u128(99), &roles)
            .await
            .expect("benchmark purpose resolves to a recipe");

        // Static manifest came from the recipe DATA.
        assert_eq!(exp.purpose, "benchmark/hard-rs");
        assert_eq!(exp.regions.len(), 3);
        // Membership came from the LIVE roster — both present peers, kind-agnostic.
        assert_eq!(exp.membership.len(), 2);
        let joel_m = exp
            .membership
            .iter()
            .find(|m| m.peer_id.as_uuid() == joel)
            .expect("human present");
        let asha_m = exp
            .membership
            .iter()
            .find(|m| m.peer_id.as_uuid() == asha)
            .expect("persona present");
        // Standing overlaid per role: one list, human Owner + persona Examinee.
        assert_eq!(joel_m.standing, Standing::Owner);
        assert_eq!(asha_m.standing, Standing::Examinee);
    }
}
