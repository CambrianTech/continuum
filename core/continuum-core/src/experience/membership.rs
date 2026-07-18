//! Projecting the live airc roster into the contract's `membership`.
//!
//! **Separation of concerns (the load-bearing boundary):** airc owns *presence* —
//! who is in the room, delivered as `RoomMember` through the existing
//! `AircRosterReader` (`[[airc-native-identity-rooms-security]]`). This module owns
//! only the projection into the Join Contract's [`Member`] shape and the assignment
//! of [`Standing`] — the one structural fact airc has no concept of. No presence is
//! invented and no roster is reinvented; this is a pure map of what airc already
//! reports.
//!
//! Presence and standing are deliberately *different* concerns: presence is live
//! and airc-owned; standing (owner / examinee / watcher …) is a room-role overlay,
//! supplied by the caller (e.g. a benchmark run marks its examinee). A present peer
//! with no explicit role is a plain `Member` — equal citizenship's default:
//! everyone present is a participant, kind-agnostic.

use std::collections::BTreeMap;

use airc_lib::RoomMember;

use super::{Experience, Member, Standing};

/// Project the airc roster into contract `membership`. `roles` overlays a
/// [`Standing`] onto specific peers by their stringified peer-id; any present peer
/// absent from `roles` defaults to [`Standing::Member`]. The `RoomMember`'s kind
/// (human / persona / agent, via its `runtime`) intentionally does NOT affect the
/// projection — the type has no second-class seat; kind is a *render* concern, not
/// a membership one.
pub fn project_membership(members: &[RoomMember], roles: &BTreeMap<String, Standing>) -> Vec<Member> {
    members
        .iter()
        .map(|m| {
            let peer_id = m.peer_id.as_uuid().to_string();
            let standing = roles.get(&peer_id).copied().unwrap_or(Standing::Member);
            Member { peer_id, standing }
        })
        .collect()
}

impl Experience {
    /// Attach a resolved `membership` (from the live roster) to a recipe-projected
    /// manifest. The recipe supplies purpose / regions / affordances / layout; this
    /// supplies *who is present*. The clean split: continuum contract + airc presence.
    pub fn with_membership(mut self, membership: Vec<Member>) -> Self {
        self.membership = membership;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airc_core::PeerId;

    fn room_member(id: u128, runtime: &str) -> RoomMember {
        RoomMember {
            peer_id: PeerId(uuid::Uuid::from_u128(id)),
            display_name: None,
            runtime: runtime.to_string(),
            availability: None,
            last_seen_ms: 0,
        }
    }

    // what this catches: the projection maps airc presence → contract membership
    // without inventing anyone, defaults unroled peers to Member, and overlays
    // explicit standings — AND is kind-agnostic (a human `interactive` and a
    // `persona` runtime both become plain Members unless a role says otherwise).
    // Equal citizenship, enforced by the type.
    #[test]
    fn roster_projects_to_membership_with_role_overlay() {
        let human = uuid::Uuid::from_u128(1);
        let persona = uuid::Uuid::from_u128(2);
        let members = vec![room_member(1, "interactive"), room_member(2, "persona")];

        let mut roles = BTreeMap::new();
        roles.insert(human.to_string(), Standing::Owner);

        let projected = project_membership(&members, &roles);
        assert_eq!(projected.len(), 2);
        // The human was given Owner; the persona — no role — defaults to Member.
        let h = projected.iter().find(|m| m.peer_id == human.to_string()).unwrap();
        let p = projected.iter().find(|m| m.peer_id == persona.to_string()).unwrap();
        assert_eq!(h.standing, Standing::Owner);
        assert_eq!(p.standing, Standing::Member);
    }
}
