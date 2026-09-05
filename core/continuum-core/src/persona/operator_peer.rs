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
                    Ok(()) => crate::probe!(
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
