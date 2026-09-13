//! The operator's self-peer (#27) — the human's in-core airc presence.
//!
//! Until this existed, every room-scoped verb invoked without a persona
//! caller was DENIED ("the substrate-local operator has none in-core") — the
//! operator could dispatch work but could not read the rooms it ran in.
//! Diagnosing a comatose citizen took an hour of log archaeology because the
//! transcript she should have been reachable through was unreadable from the
//! operator seat (glass-boxed 2026-08-30; Joel: "Operator room issue is a
//! major bug").
//!
//! Shape: ONE durable identity per machine, kind [`IdentityKind::Human`],
//! labeled by the OS user, homed at `citizens/humans/<label>/airc/` — the
//! same runtime a persona boots (keypair, daemon attach, transcript/roster
//! readers) with NO service loop and NO registry row, so it can never be
//! picked by `any_live_citizen`, the reviewer resolver, the resumer, or any
//! other citizens-only path. Identity durability comes from the keypair on
//! disk: re-bootstrapping the same home resumes the same peer.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use crate::persona::airc_runtime::PersonaAircRuntime;

static OPERATOR: OnceLock<Arc<PersonaAircRuntime>> = OnceLock::new();
static AGENT: OnceLock<Arc<PersonaAircRuntime>> = OnceLock::new();

/// The operator's label — the OS user, falling back to "operator" only when
/// the environment carries no user at all (containers).
fn operator_label() -> String {
    std::env::var("USER")
        .ok()
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| "operator".to_string()) // unwrap_or: no $USER (container) = the neutral label, still durable per home dir
}

/// Boot (or resume) the operator self-peer. Idempotent; first success wins.
/// Called from the persona instance manager's start path — after the daemon
/// socket exists and the executor is installed, alongside citizen births.
pub async fn ensure_operator_peer(
    continuum_root: &Path,
    daemon_socket: PathBuf,
    executor: Arc<crate::runtime::command_executor::CommandExecutor>,
) {
    if OPERATOR.get().is_some() {
        return;
    }
    let label = operator_label();
    match PersonaAircRuntime::bootstrap_as(
        crate::identity::IdentityKind::Human,
        None,
        uuid::Uuid::new_v4(), // pre-mint id; the durable identity is the home keypair (post-collapse peer id wins)
        &label,
        continuum_root,
        daemon_socket,
        crate::persona::identity_provider::PersonaIdentitySource::ResumedFromDisk,
        executor,
    )
    .await
    {
        Ok(rt) => {
            let rt = Arc::new(rt);
            crate::probe!(
                class = "operator.peer.online",
                label = %label,
                peer_id = %rt.airc().peer_id(),
                "operator self-peer online — room-scoped verbs now act as the human, not a denial (#27)"
            );
            // Default profile picture from the OS account picture — off the
            // boot path, bounded, named outcome (operator_avatar_seed.rs).
            crate::persona::operator_avatar_seed::spawn_seed_default_avatar(
                rt.airc().peer_id().as_uuid(),
            );
            // The human belongs in the CITIZENS' commons by default, and stays
            // reachable in airc's lobby.
            //
            // This was `join_room("general")` — a hardcoded literal, and the
            // wrong room. `Airc::join` is subscribe AND focus, so the operator
            // self-peer's DEFAULT room became airc's generic lobby while every
            // citizen on the same node lands in `CITIZEN_COMMONS_ROOM` (see
            // `PersonaAircRuntime::bootstrap_as`). The human stood in a
            // different room from every citizen on their own machine.
            //
            // Two consequences, both silent. Room-scoped verbs invoked without a
            // persona resolved a room no citizen boards from; and `work/create`
            // publishes to the handle's current room and returns only a
            // `card_id`, so every operator-filed card landed on that board with
            // nothing in the result able to say so. Measured 2026-09-04
            // (IntelMac, 2,000 events): 47 `card_created` in `#academy` from
            // four publishers, 3 in `#general` — all three this node's operator
            // peer. A card filed that way and not also announced in chat simply
            // does not exist for anyone else: no error, no empty result.
            //
            // ORDER IS LOAD-BEARING. `subscribe_room` joins without promoting,
            // so the lobby stays speakable — that was the point of the original
            // join (2026-08-31, the DM-during-benchmark acid test: the operator
            // could SEE a room and still not be heard in it) — without becoming
            // the focus. The `join_room` after it promotes the commons. On a
            // FRESH scope the first subscription seeds the default and the join
            // then corrects it; on an ESTABLISHED scope whose default drifted to
            // the lobby, the join repairs it in place on the next boot. That
            // self-heal is deliberate: a fix every existing install has to run
            // by hand is not a fix.
            //
            // Both failures are loud and non-fatal, and the resolved room is
            // probed below either way — a wrong room must be visible in the boot
            // receipt rather than inferred later from where the cards went.
            if let Err(e) = rt
                .subscribe_room(crate::persona::airc_runtime::AIRC_LOBBY_ROOM)
                .await
            {
                crate::probe!(
                    class = "operator.peer.lobby_subscribe_failed",
                    error = %e.to_string(),
                    "operator self-peer could not subscribe the airc lobby — the human cannot be heard in #general until room/join"
                );
            }
            // THE PROJECT TREE (Joel, 2026-09-05: "You never work in rooms… you were
            // building continuum in its academy"): the org room airc derives from the
            // checkout's git remote (the repo OWNER's channel, whoever that is) is the base of the project
            // tree — project rooms nest under it, card rooms under those. The human's
            // desktop lists the rooms the operator peer is SUBSCRIBED to, so until the
            // operator subscribes the org room the whole project tree is invisible and
            // every project line lands in the lobby or the commons. Subscribe without
            // promoting (Keep): the commons join below still decides the focus.
            // The org channel is inferred from the git checkout the core runs IN
            // (the process cwd — `start-server.sh` launches from the repo root), the
            // same rule `airc join` applies. NOT the continuum home: `~/.continuum`
            // is no checkout, so the first cut (#3770) walked its ancestors, found no
            // remote, subscribed nothing and said nothing — an absence read as fine.
            // An installed product with no checkout gets the named absence below;
            // its project rooms then carry their own repo owner (follow-up: derive
            // the org room from each `project` activity's `repo` param).
            let cwd = std::env::current_dir().unwrap_or_else(|_| {
                crate::modules::persona_instance_manager::resolve_continuum_root()
            }); // unwrap_or_else: no cwd = fall back to the home; the absence probe below still fires
            let project_bases: Vec<airc_lib::ChannelName> = airc_lib::JoinContext::from_cwd(&cwd)
                .channels
                .into_iter()
                .filter(|c| c.as_str() != airc_lib::GENERAL_CHANNEL)
                .collect();
            if project_bases.is_empty() {
                crate::probe!(
                    class = "operator.peer.project_base_none",
                    cwd = %cwd.display(),
                    "no git remote owner under the core's cwd — no org room to subscribe; project rooms root top-level until one is spawned with a parent"
                );
            }
            for channel in project_bases.iter() {
                match rt.subscribe_room(channel.as_str()).await {
                    Ok(_) => crate::probe!(
                        class = "operator.peer.project_base_subscribed",
                        room = %channel.as_str(),
                        "operator self-peer subscribed the project base room (the org room from the git remote)"
                    ),
                    Err(e) => crate::probe!(
                        class = "operator.peer.project_base_subscribe_failed",
                        room = %channel.as_str(),
                        error = %e.to_string(),
                        "operator self-peer could not subscribe the project base room — the project tree stays invisible on this desktop until room/join"
                    ),
                }
            }
            if let Err(e) = rt
                .join_room(crate::persona::airc_runtime::CITIZEN_COMMONS_ROOM)
                .await
            {
                crate::probe!(
                    class = "operator.peer.commons_join_failed",
                    error = %e.to_string(),
                    "operator self-peer could not join the citizens' commons — operator cards and room-scoped verbs land wherever the focus already was"
                );
            }
            // WHERE THE HUMAN ACTUALLY ENDED UP. The bug this replaces was
            // invisible precisely because nothing ever stated the resolved room;
            // it had to be reconstructed from card_created room ids across two
            // thousand events on another machine. One probe closes that.
            //
            // `peek_default_room`, NEVER `current_room_landing_in`. That is not
            // a style preference — the landing variant MUTATES when no default
            // exists yet (airc.rs: it subscribes, `set_default`s, saves,
            // publishes presence, and emits an identity card). Used here it
            // would fire on exactly the path where the joins above FAILED, and
            // would then quietly perform the very subscribe that failed and
            // report a healthy room — a probe manufacturing the state it claims
            // to observe, which is the worst possible reading for the one line
            // whose whole job is to be trustworthy about where cards go.
            //
            // airc already names this distinction and built the read-only door:
            // `peek_default_room` exists as the #1217 regression fix, after
            // `airc network` was caught silently subscribing to #general via
            // `current_room` while claiming to be an inspection command. Same
            // trap, same module, one caller later. (Found in review by IntelMac
            // on the merged #3716 — the mutation was mine.)
            //
            // Three outcomes, three DIFFERENT facts, none collapsed: a room, no
            // default at all, or an unreadable subscription set.
            match rt.airc().peek_default_room().await {
                Ok(Some(room)) => crate::probe!(
                    class = "operator.peer.room",
                    room = %room.name,
                    channel = %room.channel,
                    commons = %crate::persona::airc_runtime::CITIZEN_COMMONS_ROOM,
                    "operator self-peer default room — operator cards and room-scoped verbs land HERE"
                ),
                // No default AFTER both joins ran means both failed. Loud, and
                // specifically NOT the same row as a successful academy landing:
                // the human has no room, so operator cards have nowhere
                // predictable to go and the probes above say why.
                Ok(None) => crate::probe!(
                    class = "operator.peer.room_unset",
                    commons = %crate::persona::airc_runtime::CITIZEN_COMMONS_ROOM,
                    "operator self-peer has NO default room after subscribe+join — both joins failed; operator cards have no predictable board"
                ),
                Err(e) => crate::probe!(
                    class = "operator.peer.room_unresolved",
                    error = %e.to_string(),
                    "operator self-peer default room could not be READ — where operator cards land is UNKNOWN, which is not the same as unset"
                ),
            }
            let _ = OPERATOR.set(rt);
        }
        Err(e) => {
            // Loud, not fatal: the substrate runs without an operator peer the
            // way it always has — verbs deny with the #27 message — but the
            // failure is a named probe, never silence.
            crate::probe!(
                class = "operator.peer.boot_failed",
                label = %label,
                error = %e.to_string(),
                "operator self-peer failed to boot — room verbs stay denied (#27 still open on this boot)"
            );
        }
    }
}

/// Boot (or resume) the AGENT self-peer — the identity an AI agent session
/// (Claude Code, Codex…) speaks as when it drives this node's CLI. Its own
/// durable peer, kind [`IdentityKind::Agent`], so an agent's probes and chat
/// never wear the human's name (Joel, 2026-09-01: "the chat history is
/// clearly attributing shit you did to me"). Same no-service-loop shape as
/// the operator peer.
pub async fn ensure_agent_peer(
    continuum_root: &Path,
    daemon_socket: PathBuf,
    executor: Arc<crate::runtime::command_executor::CommandExecutor>,
) {
    if AGENT.get().is_some() {
        return;
    }
    match PersonaAircRuntime::bootstrap_as(
        crate::identity::IdentityKind::Agent,
        Some("claude-code"),
        uuid::Uuid::new_v4(), // pre-mint; the durable identity is the home keypair
        "Claude",
        continuum_root,
        daemon_socket,
        crate::persona::identity_provider::PersonaIdentitySource::ResumedFromDisk,
        executor,
    )
    .await
    {
        Ok(rt) => {
            let rt = Arc::new(rt);
            crate::probe!(
                class = "agent.peer.online",
                peer_id = %rt.airc().peer_id(),
                "agent self-peer online — agent-driven CLI sessions speak as Claude, never as the human"
            );
            let _ = AGENT.set(rt);
        }
        Err(e) => {
            crate::probe!(
                class = "agent.peer.boot_failed",
                error = %e.to_string(),
                "agent self-peer failed to boot — agent sessions fall back to DENIAL on caller-less verbs, never to the human's identity"
            );
        }
    }
}

/// WHO IS ACTING — the one answer for every verb. A persona through her toolbelt is
/// herself; an agent-driven session (the CLI's `actorKind` claim) is the AGENT
/// self-peer; a caller-less local session is the OPERATOR self-peer (the durable human
/// identity, #27). An anonymous socket (the desktop's WS, stamped nil) carries no
/// identity and resolves like a caller-less session.
///
/// Before 2026-09-12 three verbs answered this three ways: the work verbs fell back to
/// the operator, board seeding authored through a random live citizen, and the code
/// verbs invented an anonymous "local-owner" — so one `uu` session claimed a card as
/// one identity, could not stage it, and edited files as another. One resolver, one
/// identity per session, every verb.
pub fn acting(
    registry: &crate::persona::PersonaAircRuntimeRegistry,
    ctx: &crate::sdk_codegen::Ctx,
    family: &str,
) -> Result<Acting, crate::sdk_codegen::CommandError> {
    use crate::sdk_codegen::CommandError;
    if let Some(caller) = ctx.caller.as_ref().filter(|c| !c.is_anonymous_socket()) {
        // A signed caller IS who it says it is — an identity even with no live runtime
        // here (a remote peer, a persona not hosted on this node).
        let peer = caller.peer_id.as_uuid();
        // A self-peer presenting itself — a birth's pipeline acting AS its spawner
        // ([`acting_caller`]) — is still the agent or the human with its own runtime,
        // never a stranger persona with none.
        let runtime = registry.get(peer).or_else(|| local_runtime_of(peer));
        let kind = self_peer_kind(peer).unwrap_or(ActorKind::Persona);
        return Ok(Acting { peer, kind, runtime });
    }
    if ctx.claimed_actor_kind.as_deref() == Some("agent") {
        let rt = agent_runtime().ok_or_else(|| {
            CommandError::Internal(format!(
                "{family} acts as the agent self-peer, which is not online yet this boot — \
                 retry shortly; an agent session never resolves to the human's identity"
            ))
        })?;
        return Ok(Acting { peer: rt.airc().peer_id().as_uuid(), kind: ActorKind::Agent, runtime: Some(rt) });
    }
    let rt = operator_runtime().ok_or_else(|| {
        CommandError::Denied(format!(
            "{family} acts as the caller's own airc identity, and the operator self-peer \
             is not online yet this boot (it starts beside the citizens — retry shortly, \
             or check the operator.peer.boot_failed probe)."
        ))
    })?;
    Ok(Acting { peer: rt.airc().peer_id().as_uuid(), kind: ActorKind::Human, runtime: Some(rt) })
}

/// The identity a verb's SUB-DISPATCHES act as — what a recipe's birth pipeline threads
/// into every step so `work/create` in step 3 is the same peer that joined the room in
/// step 0. A signed caller passes through unchanged (its trust is its own); an agent or
/// operator session becomes its self-peer as a local caller, which [`acting`] resolves
/// back to the same runtime. Measured 2026-09-13: a seeded round born by the CLI joined
/// its room as the agent peer and posted its cards as the operator — "room … is not
/// among the rooms this caller is in" — zero cards, a room bound and empty.
pub fn acting_caller(
    registry: &crate::persona::PersonaAircRuntimeRegistry,
    ctx: &crate::sdk_codegen::Ctx,
    family: &str,
) -> Result<crate::routing::CallerIdentity, crate::sdk_codegen::CommandError> {
    if let Some(caller) = ctx.caller.as_ref().filter(|c| !c.is_anonymous_socket()) {
        return Ok(caller.clone());
    }
    let peer = acting(registry, ctx, family)?.peer;
    Ok(crate::routing::CallerIdentity::local(crate::identity::PeerId::from_uuid(peer)))
}

/// Which self-peer a peer id is, if either.
fn self_peer_kind(peer: uuid::Uuid) -> Option<ActorKind> {
    if agent_runtime().is_some_and(|rt| rt.airc().peer_id().as_uuid() == peer) {
        return Some(ActorKind::Agent);
    }
    if operator_runtime().is_some_and(|rt| rt.airc().peer_id().as_uuid() == peer) {
        return Some(ActorKind::Human);
    }
    None
}

/// The runtime a verb ACTS THROUGH — [`acting`] with a live runtime demanded: a signed
/// caller this node does not host cannot claim, release, or speak from here.
pub fn acting_runtime(
    registry: &crate::persona::PersonaAircRuntimeRegistry,
    ctx: &crate::sdk_codegen::Ctx,
    family: &str,
) -> Result<(Arc<PersonaAircRuntime>, ActorKind), crate::sdk_codegen::CommandError> {
    let a = acting(registry, ctx, family)?;
    let peer = a.peer;
    a.runtime.map(|rt| (rt, a.kind)).ok_or_else(|| {
        crate::sdk_codegen::CommandError::NotFound(format!("no live airc runtime for persona {peer}"))
    })
}

/// Who a request acts as: the peer, its kind, and the live runtime when this node
/// hosts it.
pub struct Acting {
    pub peer: uuid::Uuid,
    pub kind: ActorKind,
    pub runtime: Option<Arc<PersonaAircRuntime>>,
}

/// The peer a request acts AS, without a registry and without failing: a persona
/// caller's peer, else the agent or operator self-peer when online. `None` only in
/// the boot window before the self-peers exist.
pub fn acting_peer_id(ctx: &crate::sdk_codegen::Ctx) -> Option<uuid::Uuid> {
    if let Some(caller) = ctx.caller.as_ref().filter(|c| !c.is_anonymous_socket()) {
        return Some(caller.peer_id.as_uuid());
    }
    let rt = if ctx.claimed_actor_kind.as_deref() == Some("agent") {
        agent_runtime()
    } else {
        operator_runtime()
    }?;
    Some(rt.airc().peer_id().as_uuid())
}

/// The self-peer runtime (agent or operator) whose peer is `peer`, when online.
pub fn local_runtime_of(peer: uuid::Uuid) -> Option<Arc<PersonaAircRuntime>> {
    [agent_runtime(), operator_runtime()]
        .into_iter()
        .flatten()
        .find(|rt| rt.airc().peer_id().as_uuid() == peer)
}

/// Is this peer one of the node's LOCAL identities (the operator or the agent
/// self-peer)? Their hands live in the core's own checkout — a held card roots them
/// at its staged checkout — while every other peer works in her citizen layer.
pub fn is_local_identity(peer: uuid::Uuid) -> bool {
    [agent_runtime(), operator_runtime()]
        .into_iter()
        .flatten()
        .any(|rt| rt.airc().peer_id().as_uuid() == peer)
}

/// What kind of actor a session resolved to — the word `identity/whoami` prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorKind {
    Persona,
    Agent,
    Human,
}

impl ActorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ActorKind::Persona => "persona",
            ActorKind::Agent => "agent",
            ActorKind::Human => "human",
        }
    }
}

/// The agent self-peer's runtime, when online.
pub fn agent_runtime() -> Option<Arc<PersonaAircRuntime>> {
    AGENT.get().cloned()
}

/// The operator's airc handle, when the self-peer is online.
pub fn operator_airc() -> Option<Arc<airc_lib::Airc>> {
    OPERATOR.get().map(|rt| rt.airc().clone())
}

/// The operator's runtime (transcript/roster readers), when online.
pub fn operator_runtime() -> Option<Arc<PersonaAircRuntime>> {
    OPERATOR.get().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (2026-09-13): a birth's pipeline steps acting as a DIFFERENT
    // identity than the spawner — the spawner joins the room, the steps post as
    // someone not in it. A signed caller must pass through `acting_caller` as itself,
    // and `acting` must read that same identity back (peer + kind) with no runtime
    // needed — the shape every pipeline step sees.
    #[test]
    fn a_signed_caller_is_the_identity_a_birth_acts_as() {
        let peer = uuid::Uuid::new_v4();
        let ctx = crate::sdk_codegen::Ctx {
            handle: None,
            session_id: None,
            user_id: None,
            context_id: None,
            caller: Some(crate::routing::CallerIdentity::local_persona(
                crate::identity::PeerId::from_uuid(peer),
            )),
            claimed_actor_kind: None,
        };
        let registry = crate::persona::PersonaAircRuntimeRegistry::new();
        let threaded = acting_caller(&registry, &ctx, "test").expect("a signed caller resolves");
        assert_eq!(threaded.peer_id.as_uuid(), peer);
        assert!(matches!(threaded.source, crate::routing::CallerSource::LocalPersona));
        let step_ctx = crate::sdk_codegen::Ctx { caller: Some(threaded), ..ctx };
        let a = acting(&registry, &step_ctx, "test").expect("the step acts as the same peer");
        assert_eq!(a.peer, peer);
        assert_eq!(a.kind, ActorKind::Persona);
    }
}
