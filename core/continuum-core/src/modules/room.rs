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
    #[ts(type = "string")]
    pub peer_id: crate::identity::PeerId,
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
                    name: identity
                        .as_ref()
                        .map(|i| i.name.clone())
                        .filter(|n| !n.is_empty()),
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
                    peer_id: c.peer_id,
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

// ──────────────────── room/list · room/join · room/leave ────────────────────
//
// Joel, 2026-08-08: "Like you or anyone in the UI they are first class and should
// subscribe or can to many rooms."
//
// Membership was the one citizen property with no citizen verb. `room/members`
// answered "who is here"; nothing answered "where am I", "put me in there", or
// "I am done with that one" — and nothing anywhere in continuum could add a
// citizen to a second room at all (zero callers for the airc subscription API,
// task #65). A persona's rooms were whatever bootstrap seeded, forever.
//
// These are deliberately hers to call, not only an operator's to apply to her.
// First-class means she can join a room the way you do — the difference between
// a citizen and a managed resource is who is allowed to move her.

/// The rooms this citizen belongs to, and which one is her focus.
pub struct RoomList {
    pub registry: PersonaAircRuntimeRegistry,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
pub struct RoomListParams {}

#[derive(Debug, Clone, Serialize, TS)]
pub struct RoomListEntry {
    pub name: String,
    pub room_id: String,
    /// True for the room short-shape actions resolve against when no room is
    /// named. One entry at most carries this.
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct RoomListResult {
    pub rooms: Vec<RoomListEntry>,
    pub count: u32,
    pub summary: String,
}

#[async_trait]
impl ActionCommand for RoomList {
    const NAME: &'static str = "room/list";
    const ALIASES: &'static [&'static str] = &["rooms", "where_am_i"];
    const NATIVE: bool = true;
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "The rooms you belong to — you can be in several. For who is PRESENT in one, \
         use room/members.";
    type Params = RoomListParams;
    type Output = RoomListResult;

    async fn run(&self, ctx: &Ctx, _p: RoomListParams) -> Result<RoomListResult, CommandError> {
        let airc = persona_airc(&self.registry, ctx, "room/list")?;
        let set = airc
            .subscription_set()
            .await
            .map_err(|e| CommandError::Internal(format!("subscription read failed: {e}")))?;
        let default_name = set.default_subscription().map(|s| s.name.clone());
        let rooms: Vec<RoomListEntry> = set
            .all()
            .map(|sub| {
                let room = sub.as_room();
                RoomListEntry {
                    is_default: default_name.as_ref() == Some(&sub.name),
                    name: room.name,
                    room_id: room.channel.as_uuid().to_string(),
                }
            })
            .collect();
        Ok(RoomListResult {
            count: rooms.len() as u32,
            summary: summarize_rooms(&rooms),
            rooms,
        })
    }
}

/// Plain-language reading of the room list, so the result explains itself rather
/// than handing back a bare array. Belonging to exactly one room is a normal
/// state, not a deficiency, and must not read as one.
fn summarize_rooms(rooms: &[RoomListEntry]) -> String {
    if rooms.is_empty() {
        return "You are not in any room yet. Join one with room/join to start \
                hearing and being heard. This is not an error."
            .to_string();
    }
    let names: Vec<&str> = rooms.iter().map(|r| r.name.as_str()).collect();
    let focus = rooms
        .iter()
        .find(|r| r.is_default)
        .map(|r| format!(" Your default room is {}.", r.name))
        .unwrap_or_default();
    format!(
        "You belong to {} room(s): {}.{focus}",
        rooms.len(),
        names.join(", ")
    )
}

crate::register_command!(RoomList);

/// Join a room — gain membership WITHOUT being moved into it.
pub struct RoomJoin {
    pub registry: PersonaAircRuntimeRegistry,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
pub struct RoomJoinParams {
    /// Room NAME, e.g. `academy`. Not a uuid — airc derives a channel id by
    /// HASHING the name, so a uuid-shaped string re-hashes into a different
    /// channel and silently lands you somewhere nobody is (airc card c409eaf5).
    /// airc refuses uuid-shaped names at its own boundary; this field documents
    /// why rather than letting the refusal arrive unexplained.
    pub room: String,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct RoomJoinResult {
    pub name: String,
    pub room_id: String,
    /// True when this room is also now your default — which happens only when
    /// you had none. Joining never MOVES an existing focus.
    pub is_default: bool,
    pub summary: String,
}

#[async_trait]
impl ActionCommand for RoomJoin {
    const NAME: &'static str = "room/join";
    const ALIASES: &'static [&'static str] = &["join_room", "enter_room"];
    const NATIVE: bool = true;
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Join a room by NAME so you hear it and can speak in it. Joining does not \
         remove you from your other rooms.";
    type Params = RoomJoinParams;
    type Output = RoomJoinResult;

    async fn run(&self, ctx: &Ctx, p: RoomJoinParams) -> Result<RoomJoinResult, CommandError> {
        let name = p.room.trim();
        if name.is_empty() {
            return Err(CommandError::Invalid(
                "room/join needs a room NAME (e.g. `academy`). Use room/list to see \
                 the rooms you are already in."
                    .to_string(),
            ));
        }
        let airc = persona_airc(&self.registry, ctx, "room/join")?;
        // `subscribe_room`, never `join` (airc#1330): `join` would promote this
        // room to her default, so every room she is added to would silently
        // become the one her un-named reads resolve against.
        let room = airc
            .subscribe_room(name)
            .await
            .map_err(|e| CommandError::Internal(format!("join '{name}' failed: {e}")))?;
        let is_default = airc
            .subscription_set()
            .await
            .ok()
            .and_then(|s| {
                s.default_subscription()
                    .map(|d| d.name.as_str() == room.name)
            })
            .unwrap_or(false);
        let summary = if is_default {
            format!(
                "You joined {} — your first room, so it is also your default.",
                room.name
            )
        } else {
            format!(
                "You joined {}. Your default room is unchanged; you are now in both.",
                room.name
            )
        };
        Ok(RoomJoinResult {
            name: room.name,
            room_id: room.channel.as_uuid().to_string(),
            is_default,
            summary,
        })
    }
}

crate::register_command!(RoomJoin);

/// Leave a room you belong to.
pub struct RoomLeave {
    pub registry: PersonaAircRuntimeRegistry,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
pub struct RoomLeaveParams {
    /// Room NAME to leave. Omit to leave your DEFAULT room — the same
    /// "no name means the current one" shape the rest of the surface uses.
    #[ts(optional)]
    pub room: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct RoomLeaveResult {
    pub name: String,
    pub room_id: String,
    pub remaining: u32,
    pub summary: String,
}

#[async_trait]
impl ActionCommand for RoomLeave {
    const NAME: &'static str = "room/leave";
    const ALIASES: &'static [&'static str] = &["leave_room", "part_room"];
    const NATIVE: bool = true;
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Leave a room by name, or your default room if you name none. Your other \
         rooms are unaffected.";
    type Params = RoomLeaveParams;
    type Output = RoomLeaveResult;

    async fn run(&self, ctx: &Ctx, p: RoomLeaveParams) -> Result<RoomLeaveResult, CommandError> {
        let airc = persona_airc(&self.registry, ctx, "room/leave")?;
        let named = p.room.as_deref().map(str::trim).filter(|s| !s.is_empty());
        let room = airc
            .part_channel(named)
            .await
            .map_err(|e| CommandError::Internal(format!("leave failed: {e}")))?;
        // Read the set AFTER parting so `remaining` is the fact, not an
        // arithmetic guess about what the part did.
        let remaining = airc
            .subscription_set()
            .await
            .map(|s| s.all().count() as u32)
            .unwrap_or(0);
        let summary = if remaining == 0 {
            format!(
                "You left {}. You are now in no rooms — join one with room/join to be \
                 reachable again.",
                room.name
            )
        } else {
            format!(
                "You left {}. You still belong to {remaining} other room(s).",
                room.name
            )
        };
        Ok(RoomLeaveResult {
            name: room.name,
            room_id: room.channel.as_uuid().to_string(),
            remaining,
            summary,
        })
    }
}

crate::register_command!(RoomLeave);

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
        vec![
            Arc::new(RoomMembers {
                registry: self.registry.clone(),
            }),
            Arc::new(RoomList {
                registry: self.registry.clone(),
            }),
            Arc::new(RoomJoin {
                registry: self.registry.clone(),
            }),
            Arc::new(RoomLeave {
                registry: self.registry.clone(),
            }),
        ]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airc_core::PeerId;

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
            peer_id: PeerId::from_uuid(uuid::Uuid::new_v5(
                &uuid::Uuid::NAMESPACE_OID,
                b"peer-test",
            )),
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
        assert!(
            !s.contains("Asha"),
            "must not list the caller as a peer: {s}"
        );
    }

    fn room(name: &str, is_default: bool) -> RoomListEntry {
        RoomListEntry {
            name: name.into(),
            room_id: "00000000-0000-0000-0000-000000000000".into(),
            is_default,
        }
    }

    /// what this catches (#65): belonging to no room reading as a FAILURE. Same
    /// discipline as the empty-roster answer above — a citizen who asks where she is
    /// and gets a bare `count: 0` has been handed a silence she has to interpret.
    /// Absence is a real state with a next action, and the answer must say so.
    #[test]
    fn belonging_to_no_room_is_an_honest_answer_with_a_way_out() {
        let s = summarize_rooms(&[]);
        assert!(s.contains("not in any room"), "{s}");
        assert!(s.contains("room/join"), "names the verb that fixes it: {s}");
        assert!(s.contains("not an error"), "{s}");
    }

    /// what this catches: a multi-room citizen being told only how MANY rooms she is
    /// in. The whole point of Joel's ruling is that she holds several at once, so the
    /// answer has to name them AND say which one un-named actions resolve against —
    /// otherwise "you are in 3 rooms" leaves her unable to act on any of them.
    #[test]
    fn a_multi_room_citizen_is_told_which_rooms_and_which_is_default() {
        let s = summarize_rooms(&[
            room("academy", true),
            room("cambriantech", false),
            room("k3-serving", false),
        ]);
        assert!(s.starts_with("You belong to 3 room(s)"), "{s}");
        assert!(s.contains("academy") && s.contains("k3-serving"), "{s}");
        assert!(
            s.contains("default room is academy"),
            "the focus must be named, not just implied: {s}"
        );
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
