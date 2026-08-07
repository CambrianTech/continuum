//! `room/members` — the citizen's PULL on "who is here".
//!
//! ## Why this exists (#358)
//!
//! The room roster was already built, correct, and **push-only**. `AircRosterReader`
//! feeds the UI (`ipc/experience_resolver`, `ipc/positron_presence`) and gets injected
//! into a persona's prompt as grounding (`persona/room_roster_source`). Every path was
//! system→citizen. There was no citizen→system path: she could be TOLD who was present,
//! and could never ASK.
//!
//! Found from Kimi's own words on `#general` (2026-08-07), unprompted:
//!
//! > "Repeatedly used `perception/look` TO CHECK FOR PARTICIPANTS but obtained no new
//! > information."
//!
//! She named the intent and reached for the only sense-shaped verb on offer.
//! `perception/look` is LIVE-CALL VIDEO — it returns camera frames from her own
//! perception buffer. Not in a call, it correctly answered "you are not in a live video
//! call" and redirected her to `perception/observe` **with a URL** — a social question
//! routed to a web fetcher. Nothing else in the 32-verb native surface answered it
//! (files, a screenshot, hardware, the board, workspace lifecycle), so she asked again,
//! and again, and read her own looping as having nothing to contribute.
//!
//! Per [[a-citizen-saying-i-have-nothing-to-contribute-is-a-substrate-gap-report]] that
//! phrase is a bug report from the witness. This is the missing organ, not a nicety: a
//! citizen confirming she is not alone is the precondition for addressing anyone.
//!
//! ## Shape
//!
//! Self-scoped, like `perception/look`: the roster is read through the CALLER's own airc
//! handle (`persona_airc`, shared with the work verbs), so it answers "who is in MY room"
//! and can never be pointed at someone else's. Reads `room_roster_cards` — the identity
//! join — never the bare `room_roster`, because a roster of `peer-xxxx` rows is the #262
//! regression this verb would otherwise reproduce on its own surface.

use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::modules::work::persona_airc;
use crate::persona::room_roster_source::{PRESENCE_WINDOW, ROSTER_SCAN};
use crate::persona::PersonaAircRuntimeRegistry;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx, DynCommand};

// ─────────────────────────── room/members ──────────────────────────

/// Who is present in the caller's own room, right now.
pub struct RoomMembers {
    pub registry: PersonaAircRuntimeRegistry,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
pub struct RoomMembersParams {}

/// One present peer, projected for a citizen to READ — identity first, ids last.
#[derive(Debug, Clone, Serialize, TS)]
pub struct RoomMemberView {
    /// Published display name, or `None` for a present-but-uncarded peer. Never a
    /// fabricated stand-in: an unnamed peer renders as unnamed (#262) so the gap is
    /// visible as a gap instead of being papered over with its own id.
    pub name: Option<String>,
    pub pronouns: Option<String>,
    pub role: Option<String>,
    pub bio: Option<String>,
    /// What kind of client this peer runs (persona runtime, CLI, UI).
    pub runtime: String,
    /// Self-reported availability from the peer's last heartbeat. `None` = the peer
    /// did not report it — unknown, NOT "unavailable".
    pub availability: Option<String>,
    /// Seconds since this peer's last heartbeat, at the moment of the read.
    pub last_seen_secs_ago: u64,
    /// True for the caller's own row — she is a member of the room she is asking about.
    pub is_you: bool,
    pub peer_id: String,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct RoomMembersResult {
    pub count: u32,
    pub members: Vec<RoomMemberView>,
    /// Plain-language reading of `count`, so the result explains ITSELF rather than
    /// leaving the citizen to interpret an empty list. An empty roster is a real,
    /// honest answer ("you appear to be alone here") and must never read as a failure.
    pub summary: String,
}

#[async_trait]
impl ActionCommand for RoomMembers {
    const NAME: &'static str = "room/members";
    // Meet the vocabulary they actually reach for (#328/#202) rather than making them
    // find ours. `who` is what a human types; the rest are the shapes seen in the wild.
    const ALIASES: &'static [&'static str] = &[
        "who",
        "who_is_here",
        "room_roster",
        "list_participants",
        "list_members",
    ];
    const NATIVE: bool = true; // the social sense — as core as reading the board
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "See who is in the room with you right now — the people and personas present, \
         with their names, roles and how recently each was heard from. Takes no \
         arguments; it always answers for YOUR current room. Use this when you want to \
         know who is around, who to address, or whether you are alone. (This is the \
         room's roster, not a video call — for a live call use perception/look.)";
    type Params = RoomMembersParams;
    type Output = RoomMembersResult;

    async fn run(
        &self,
        ctx: &Ctx,
        _p: RoomMembersParams,
    ) -> Result<RoomMembersResult, CommandError> {
        let airc = persona_airc(&self.registry, ctx, "room/members")?;
        let me = airc.peer_id();

        // The identity join, NOT the bare roster — see the module doc / #262.
        let cards = airc
            .room_roster_cards(PRESENCE_WINDOW, ROSTER_SCAN)
            .await
            .map_err(|e| CommandError::Internal(format!("roster read failed: {e}")))?;

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let members: Vec<RoomMemberView> = cards
            .into_iter()
            .map(|c| {
                let identity = c.identity;
                RoomMemberView {
                    name: identity.as_ref().map(|i| i.name.clone()).filter(|n| !n.is_empty()),
                    pronouns: identity
                        .as_ref()
                        .map(|i| i.pronouns.clone())
                        .filter(|s| !s.is_empty()),
                    role: identity
                        .as_ref()
                        .map(|i| i.role.clone())
                        .filter(|s| !s.is_empty()),
                    bio: identity
                        .as_ref()
                        .map(|i| i.bio.clone())
                        .filter(|s| !s.is_empty()),
                    runtime: c.runtime,
                    availability: c.availability.map(|a| format!("{a:?}").to_lowercase()),
                    last_seen_secs_ago: now_ms.saturating_sub(c.last_seen_ms) / 1000,
                    is_you: c.peer_id == me,
                    peer_id: c.peer_id.to_string(),
                }
            })
            .collect();

        Ok(RoomMembersResult {
            count: members.len() as u32,
            summary: summarize(&members),
            members,
        })
    }
}

/// Turn the roster into one sentence the caller can act on. Split out so the
/// "what does this count MEAN" rule lives in exactly one place and is unit-testable
/// without an airc daemon.
fn summarize(members: &[RoomMemberView]) -> String {
    let others = members.iter().filter(|m| !m.is_you).count();
    if others == 0 {
        return "No one else is present in this room right now — you appear to be alone. \
                This is an honest empty roster, not an error; anyone who joins will show up here."
            .to_string();
    }
    let named: Vec<&str> = members
        .iter()
        .filter(|m| !m.is_you)
        .filter_map(|m| m.name.as_deref())
        .collect();
    let unnamed = others - named.len();
    let who = if named.is_empty() {
        String::new()
    } else {
        format!(" ({})", named.join(", "))
    };
    let tail = if unnamed > 0 {
        format!(
            " {unnamed} of them {} not published an identity yet, so {} shown by peer id only.",
            if unnamed == 1 { "has" } else { "have" },
            if unnamed == 1 { "it is" } else { "they are" }
        )
    } else {
        String::new()
    };
    format!("{others} other participant(s) present{who}. You can address them by name.{tail}")
}

crate::register_command!(RoomMembers);

/// The room module — holds the persona airc-runtime registry so the roster read
/// resolves the CALLER's own handle and answers for HER room.
pub struct RoomModule {
    registry: PersonaAircRuntimeRegistry,
}

impl RoomModule {
    pub fn new(registry: PersonaAircRuntimeRegistry) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl ServiceModule for RoomModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "room",
            priority: ModulePriority::Normal,
            command_prefixes: &[],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        Err(format!(
            "room command '{command}' is a typed object, not prefix-routed"
        ))
    }

    fn commands(&self) -> Vec<Arc<dyn DynCommand>> {
        vec![Arc::new(RoomMembers {
            registry: self.registry.clone(),
        })]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(name: Option<&str>, is_you: bool) -> RoomMemberView {
        RoomMemberView {
            name: name.map(str::to_string),
            pronouns: None,
            role: None,
            bio: None,
            runtime: "persona".into(),
            availability: None,
            last_seen_secs_ago: 3,
            is_you,
            peer_id: "peer-test".into(),
        }
    }

    /// what this catches (#358): an empty roster reading as a FAILURE. A citizen who
    /// asks who is here and gets a bare `count: 0` has been handed the same silence
    /// `perception/look` gave her; the answer has to say what the zero means.
    #[test]
    fn an_empty_room_is_an_honest_answer_not_an_error() {
        let s = summarize(&[view(None, true)]);
        assert!(s.contains("you appear to be alone"), "{s}");
        assert!(s.contains("not an error"), "{s}");
    }

    /// what this catches: counting YOURSELF as company. The caller is always in her own
    /// roster, so a naive len() would tell a solitary citizen one participant is present
    /// and send her addressing a room containing only herself.
    #[test]
    fn the_caller_never_counts_as_her_own_company() {
        let s = summarize(&[view(Some("Asha"), true), view(Some("Anwen"), false)]);
        assert!(s.starts_with("1 other participant(s)"), "{s}");
        assert!(s.contains("Anwen"), "{s}");
        assert!(!s.contains("Asha"), "must not list the caller as a peer: {s}");
    }

    /// what this catches (#262): silently dropping present-but-uncarded peers, or
    /// inventing a name for them. Either one lies about the room — the first hides
    /// people, the second fabricates identity. They must be counted AND flagged.
    #[test]
    fn uncarded_peers_are_counted_and_named_as_unnamed() {
        let s = summarize(&[view(Some("Anwen"), false), view(None, false)]);
        assert!(s.starts_with("2 other participant(s)"), "{s}");
        assert!(s.contains("1 of them has not published an identity"), "{s}");
        assert!(s.contains("peer id only"), "{s}");
    }
}
