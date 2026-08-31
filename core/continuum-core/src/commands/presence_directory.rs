//! `presence/directory` — WHO EXISTS in this continuum, and who is online — the
//! SCOPE-GLOBAL read the left rail seeds from.
//!
//! Why this verb exists (Joel, 2026-08-31): the USERS & AGENTS panel is GLOBAL
//! scope — "who all exists in this continuum and who is online" — but its only
//! global source was `persona/roster` (residents-only, 4 rows), so every other
//! peer's liveness came from the FOCUSED ROOM's presence and flipped when you
//! switched tabs ("online means online-for-the-tab"). The daemon has held the
//! whole answer all along: `room_roster_cards_in(room: None)` is the scope
//! roster — every peer seen in the membership window, with heartbeat recency,
//! availability, runtime, and the published identity card. This verb is a thin
//! read of that one truth.
//!
//! Liveness here is HEARTBEAT-derived (recency within the presence window) with
//! one override: a persona RESIDENT on this node is online, full stop — the
//! room-presence pipe greys a citizen precisely while she's hardest at work
//! (lane-warming reads as away), which is the exact defect the who-panel union
//! already documents.

use std::any::Any;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::persona::PersonaAircRuntimeRegistry;
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

/// Who counts as "seen" at all: the same 14-day membership window the room
/// directory scan uses (`positron_presence::MEMBERSHIP_WINDOW`) — a peer quiet
/// longer than this has left the continuum's living memory, not just the room.
const DIRECTORY_WINDOW: std::time::Duration = std::time::Duration::from_secs(14 * 24 * 3600);
const DIRECTORY_SCAN: usize = 4000;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PresenceDirectoryParams.ts"
)]
pub struct PresenceDirectoryParams {}

/// One peer in the continuum's directory.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PresenceDirectoryEntry.ts"
)]
pub struct PresenceDirectoryEntry {
    /// Display name from the published identity card; a provisional
    /// `peer-xxxxxxxx` for a present-but-uncarded peer (unmistakable on
    /// purpose — never silently wrong).
    pub name: String,
    #[ts(type = "string")]
    pub peer_id: crate::identity::PeerId,
    /// Self-reported runtime origin (`"interactive"`, `"persona"`, `"claude"`, …).
    pub runtime: String,
    /// Coarse kind from the identity card's `continuum_kind` integration
    /// (`"human"` | `"persona"`), else derived: `interactive` runtime → human,
    /// anything else → `"agent"`.
    pub kind: String,
    /// ONLINE = heartbeat within the live presence window, OR resident on this
    /// node (residency wins — see module doc).
    pub online: bool,
    /// Resident on THIS node (live service loop) — the `persona/roster` signal,
    /// folded in so one read answers the panel.
    pub resident: bool,
    /// Last heartbeat, unix ms — the REAL recency stamp (the "9d ago" rows were
    /// identity-creation dates being passed off as activity).
    #[ts(type = "number")]
    pub last_seen_ms: u64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PresenceDirectoryResult.ts"
)]
pub struct PresenceDirectoryResult {
    #[ts(type = "number")]
    pub count: u32,
    #[ts(type = "number")]
    pub online_count: u32,
    /// Online first, then by recency, then name.
    pub peers: Vec<PresenceDirectoryEntry>,
}

/// The directory read verb. Holds the registry for the residency fold and for
/// reaching an airc handle (any resident's, else the operator self-peer's).
pub struct PresenceDirectory {
    pub registry: PersonaAircRuntimeRegistry,
}

#[async_trait::async_trait]
impl ActionCommand for PresenceDirectory {
    const NAME: &'static str = "presence/directory";
    const ALIASES: &'static [&'static str] = &["who", "directory"];
    /// AiSafe read: `room/members` already serves peer_ids at ai-safe, and the
    /// who-panel seeding from a Privileged verb is exactly how the operator's
    /// own UI once showed every citizen offline.
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "The continuum's directory: every peer seen in the last 14 days across ALL rooms \
         (humans, personas, external agents) with name, runtime, kind, residency, real \
         last-seen, and whether they are online NOW. Scope-global — the same answer in \
         every room; the left rail's source of truth.";
    type Params = PresenceDirectoryParams;
    type Output = PresenceDirectoryResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        _p: PresenceDirectoryParams,
    ) -> Result<PresenceDirectoryResult, CommandError> {
        // Any attached handle sees the same daemon store; prefer a resident's,
        // fall back to the operator self-peer. Neither online = boot window.
        let airc = self
            .registry
            .any_live_citizen()
            .map(|rt| rt.airc().clone())
            .or_else(crate::persona::operator_peer::operator_airc)
            .ok_or_else(|| {
                CommandError::Internal(
                    "no airc handle is attached yet this boot — retry once citizens or the \
                     operator self-peer come online"
                        .into(),
                )
            })?;
        let cards = airc_lib::Airc::room_roster_cards_in(
            &airc,
            None, // the SCOPE roster — every room, one answer
            DIRECTORY_WINDOW,
            DIRECTORY_SCAN,
        )
        .await
        .map_err(|e| CommandError::Internal(format!("scope roster read failed: {e}")))?;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| CommandError::Internal(e.to_string()))?
            .as_millis() as u64;
        let live_window_ms =
            crate::persona::room_roster_source::PRESENCE_WINDOW.as_millis() as u64;
        let mut peers = Vec::with_capacity(cards.len());
        for card in cards {
            let peer_uuid = card.peer_id.as_uuid();
            let resident = self.registry.get(peer_uuid).is_some();
            let heartbeat_live = now_ms.saturating_sub(card.last_seen_ms) <= live_window_ms;
            let kind = card
                .identity
                .as_ref()
                .and_then(|i| i.integrations.get("continuum_kind").cloned())
                .unwrap_or_else(|| {
                    if card.runtime == "interactive" {
                        "human".to_string()
                    } else {
                        "agent".to_string()
                    }
                });
            let name = card
                .identity
                .as_ref()
                .map(|i| i.name.clone())
                .unwrap_or_else(|| {
                    crate::ipc::positron_source::provisional_sender_name(peer_uuid)
                });
            peers.push(PresenceDirectoryEntry {
                name,
                peer_id: crate::identity::PeerId::from_uuid(peer_uuid),
                runtime: card.runtime,
                kind,
                online: resident || heartbeat_live,
                resident,
                last_seen_ms: card.last_seen_ms,
            });
        }
        peers.sort_by(|a, b| {
            bool::cmp(&b.online, &a.online)
                .then(b.last_seen_ms.cmp(&a.last_seen_ms))
                .then(a.name.cmp(&b.name))
        });
        let online_count = peers.iter().filter(|p| p.online).count() as u32;
        Ok(PresenceDirectoryResult {
            count: peers.len() as u32,
            online_count,
            peers,
        })
    }
}

crate::register_command!(PresenceDirectory);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the wire contract of the panel's ONE global source —
    // renaming orphans the left rail's seed; narrowing access re-creates the
    // every-citizen-offline class the roster verb's own doc records.
    #[test]
    fn directory_is_aisafe_under_its_wire_name() {
        assert_eq!(PresenceDirectory::NAME, "presence/directory");
        assert_eq!(PresenceDirectory::ACCESS, AccessLevel::AiSafe);
    }
}
