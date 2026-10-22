use super::*;

/// The room a FRESH citizen scope lands in (room-consolidation master
/// card daa01102, Joel: academy is THE default room — training,
/// benchmarks, and the commons all root there; `#general` was airc's
/// generic lobby, and citizens landing in it is the "disorganized
/// non-academy room stuff" the consolidation kills). Passed to
/// `Airc::current_room_landing_in` at bootstrap; an ESTABLISHED
/// persona's durable membership always wins, so changing this never
/// migrates a living citizen.
pub const CITIZEN_COMMONS_ROOM: &str = "academy";

/// airc's OWN generic lobby — not continuum's commons.
///
/// Named rather than spelled inline because the distinction is the whole
/// point: `#general` is where a bare airc scope lands when nothing tells it
/// otherwise, and continuum always has something to say
/// ([`CITIZEN_COMMONS_ROOM`]). A literal `"general"` in continuum code reads
/// like a choice; nine times in ten it is the absence of one.
pub const AIRC_LOBBY_ROOM: &str = "general";

/// Abort-on-drop guard for the continuum-owned heartbeat pump (#260): keeps
/// the airc `HeartbeatTask` teardown contract — dropping the runtime aborts
/// the pump so a torn-down persona ages out of the roster within the
/// presence window (honest "no longer here"), never beats past her death.
pub(super) struct AbortOnDrop(pub(super) tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// How long a bootstrap-time "resumed — holding: <cards>" Busy availability
/// stays live on the board before it must be renewed by real activity.
/// Matches the manager-hat lease default (15 min): long enough that a
/// freshly woken persona reads as present through her first few cadence
/// cycles, short enough that a persona that dies right after boot ages off
/// the board instead of showing Busy forever.
pub(super) const RESUME_AVAILABILITY_TTL_MS: u64 = 900_000;

/// Errors that can occur during persona airc-runtime bootstrap.
///
/// Each variant carries enough context for the operator to act —
/// pinning identity manually, fixing the daemon socket, etc. — per
/// the constitutional-design memory's "every error has a path
/// forward" doctrine.
#[derive(Debug, thiserror::Error)]
pub enum PersonaAircRuntimeError {
    #[error(
        "legacy persona home detected at {legacy:?} — Slice 4 of #142 moved \
         personas under `citizens/personas/<name>/airc/`. To migrate this \
         persona's identity AND keep its peer_id stable, run:\n\
         \n  mkdir -p {new_parent:?} && mv {legacy:?} {new:?}\n\n\
         Then re-run. Per [[no-fallbacks-ever]] the substrate refuses to \
         silently use the legacy path."
    )]
    LegacyLayoutDetected {
        legacy: PathBuf,
        new: PathBuf,
        new_parent: PathBuf,
    },
    #[error("failed to create persona airc home {0}: {1}")]
    HomeCreate(PathBuf, std::io::Error),
    #[error("airc-lib attach_as failed for persona {agent_name:?} at {home:?}: {source}")]
    Attach {
        agent_name: String,
        home: PathBuf,
        #[source]
        source: AircError,
    },
    #[error(
        "failed to resolve persona {agent_name:?}'s home room from her durable \
         subscription state: {source} — a persona's rooms come from HER OWN airc \
         home (fresh scopes land in the citizen commons, #academy — daa01102), \
         never from the operator's current room (#298)"
    )]
    HomeRoom {
        agent_name: String,
        #[source]
        source: AircError,
    },
    #[error(
        "failed to install command inbound pump for persona {agent_name:?}: {source} — \
         the persona's airc handle subscribe failed or no command handler is installed, so the persona would be \
         silently unaddressable for cross-grid commands. Per [[no-fallbacks-ever]] \
         the substrate refuses to declare the persona ready in this state."
    )]
    CommandPumpInstall {
        agent_name: String,
        #[source]
        source: crate::persona::command_inbound_pump::PumpSpawnError,
    },
    #[error(
        "failed to build the capability-grant authorizer for persona {agent_name:?}: \
         {source} — the persona could not stand up its grant verifier (own key / mesh / \
         durable watermark), so it could not safely honor presented grants. Per \
         [[no-fallbacks-ever]] the substrate refuses to declare the persona ready \
         rather than fall back to an unverified or non-durable grant path."
    )]
    GrantAuthorizerBuild {
        agent_name: String,
        #[source]
        source: crate::persona::command_inbound_pump::GrantAuthorizerBuildError,
    },
    #[error(
        "PersonaInstanceManagerModule::bootstrap_one for {agent_name:?} ran before \
         the substrate `CommandExecutor` was installed — boot ordering bug. \
         `start_server` must call `install_executor_on_all` BEFORE accepting any \
         `persona/instances/bootstrap` dispatch. Per [[no-fallbacks-ever]]: surfaces \
         as a typed bootstrap failure rather than a panic in the deleted \
         `executor()` global accessor (task #224)."
    )]
    ExecutorNotInstalled { agent_name: String },
}

/// One persona's live airc connection.
///
/// Holds the persona's `Arc<Airc>` (its grid presence — identity +
/// daemon connection + room membership) and the inbound pump task
/// handle. Drop this struct to shut the persona's airc presence
/// down cleanly.
pub struct PersonaAircRuntime {
    pub(super) persona_id: Uuid,
    pub(super) agent_name: String,
    pub(super) home: PathBuf,
    pub(super) airc: Arc<Airc>,
    pub(super) default_room: RoomId,
    /// Bumped by [`Self::join_room`] whenever this citizen's room membership
    /// GROWS at runtime. The perception stream's rebuild cue (P0 20b44763):
    /// airc-lib's `subscribe_subscribed_filtered` snapshots the subscribed
    /// channel list ONCE at subscribe time, so a room joined after `prime()`
    /// (benchmark dispatch moving assignees into a fresh run room) never
    /// enters an existing stream — the run room is born deaf. Watchers of
    /// [`AircCitizen::membership_epoch`] re-open their stream when this moves.
    pub(super) membership_epoch: tokio::sync::watch::Sender<u64>,
    pub(super) inbound_handle: Option<JoinHandle<()>>,
    /// Per-persona command inbound pump (task #222). When `Some`,
    /// this persona's airc handle is wired to receive cross-grid
    /// `AircCommandRequest` envelopes — peer_b can dispatch
    /// `airc://<this-persona-uuid>/ai/generate` and the substrate
    /// answers via the substrate's CommandExecutor. `None` only on
    /// `from_attached` paths where the caller opted out of the pump
    /// (test/demo fixtures); production `bootstrap` always installs
    /// it — if subscribe fails we hard-error per
    /// `[[no-fallbacks-ever]]`, never declaring a silently-
    /// unaddressable persona ready.
    pub(super) command_pump: Option<crate::persona::command_inbound_pump::PersonaCommandInboundPump>,
    /// The substrate command executor, kept so this citizen's OWN deterministic acts
    /// (the kanban pull's claim) ride the same command path her tool calls do —
    /// `work/claim` stages her workspace and honors the room's driver; a raw airc
    /// claim bypassed both. `None` only before the pump is installed (boot ordering).
    pub(super) executor: Option<Arc<crate::runtime::command_executor::CommandExecutor>>,
    /// The airc `Alive` heartbeat pump (`start_agent_heartbeat`). A persona is
    /// only PRESENT in another citizen's `room_roster` while it emits `Alive`
    /// heartbeats — airc's `active_agents` reduces heartbeat events, NOT `say()`
    /// messages. Holding this handle keeps the pump running for the persona's
    /// lifetime; dropping it on teardown aborts the pump, so the persona ages
    /// out of the roster within the presence window — the honest "no longer
    /// here" signal. `None` only on `from_attached` paths (tests/demo) that opt
    /// out of presence. Without it, co-resident personas never see each other in
    /// `[Present in this room]` — the roster-grounding-goes-dark bug.
    pub(super) heartbeat: Option<AbortOnDrop>,
    /// Periodic re-publisher of this persona's airc identity CARD (name + role +
    /// bio + avatar), on the heartbeat cadence — BigMama's half of #262, adopted
    /// (cherry-pick 1225f2eee): the card is a one-shot emit at attach; a peer that
    /// joins the room LATER (a desktop reconnecting, a fresh grid peer) missed it
    /// and would render `peer-<uuid>` + default glyph forever. This task re-emits
    /// the card every cadence so any late arrival is grounded by name within a
    /// heartbeat window. `None` on `from_attached` (tests/demo). Held for the
    /// persona's lifetime; aborted on drop, like the heartbeat pump.
    pub(super) identity_republish: Option<JoinHandle<()>>,
    /// Tasks whose life ends with this runtime (see `AircCitizen::own_task`).
    pub(super) owned_tasks: std::sync::Mutex<Vec<JoinHandle<()>>>,
    /// Where this citizen's identity came from — resumed from disk
    /// vs freshly minted. Carried for the lifetime of the runtime so
    /// telemetry surfaces (list/get IPC, future status panels) can
    /// distinguish without re-deriving from disk. Per
    /// [[substrate-is-a-good-citizen-on-the-host]]: observability
    /// honest.
    pub(super) source: crate::persona::identity_provider::PersonaIdentitySource,
}

/// Build the airc identity CARD a hosted persona publishes to its rooms, from her
/// durable [`PersonaCard`](crate::persona::card::PersonaCard) (the seed's identity).
///
/// continuum-core publishes this ON THE PERSONA'S BEHALF — a hosted persona can't
/// run `airc identity set` herself — so every peer's `RoomRosterSource`
/// (continuum#1650, the M5 desktop roster fold) renders her by NAME + role + bio +
/// avatar instead of a bare `peer-<uuid>` + default glyph.
///
/// `kind=persona` is NOT an airc `Identity` field (the struct has none); it rides
/// the `Alive` heartbeat's `runtime="persona"` tag. We ALSO mirror it into
/// `integrations["continuum_kind"]` for a consumer that reads the card directly,
/// alongside the persona id and the avatar ref (the avatar rides `integrations`
/// exactly as the desktop expects).
pub(super) fn persona_identity_card(
    card: &crate::persona::card::PersonaCard,
    kind_label: &str,
) -> airc_core::identity::Identity {
    let mut integrations = std::collections::BTreeMap::new();
    integrations.insert(
        "continuum_persona_id".to_string(),
        card.persona_id.to_string(),
    );
    integrations.insert("continuum_kind".to_string(), kind_label.to_string());
    if let Some(vrm) = card.avatar_vrm.as_deref() {
        integrations.insert("avatar_vrm".to_string(), vrm.to_string());
    }
    airc_core::identity::Identity {
        name: card.agent_name.clone(),
        pronouns: card.pronouns().short(),
        // `RoleId` serializes snake_case ("helper"); empty when the seed predates
        // role threading — the persona is still named, just role-less on the wire.
        role: card
            .role
            .map(|r| r.as_str().to_string())
            .unwrap_or_default(),
        // The self-authored bio lives in the OPEN profile map; empty when unset.
        bio: card.profile.get("bio").cloned().unwrap_or_default(),
        integrations,
        // `status` (away) and `fingerprint` (pubkey-derived, computed by airc
        // tooling) are not authored here — Default leaves them empty.
        ..Default::default()
    }
}

impl PersonaAircRuntime {
    /// Bootstrap a persona's airc presence.
    ///
    /// Steps:
    ///
    /// 1. Resolve the persona's home under `continuum_root/personas/
    ///    <agent_name>/airc/` and create it if absent.
    /// 2. Call `Airc::attach_as(home, agent_name, daemon_socket)`.
    ///    Internally this runs the airc-lib identity ceremony
    ///    (generate or load Ed25519 keypair, write `identity.key`,
    ///    record the local_identity row) and attaches a daemon
    ///    client for live publish + subscribe. No shelling out.
    /// 3. Resolve her HOME room via
    ///    `Airc::current_room_landing_in(CITIZEN_COMMONS_ROOM)` — her
    ///    OWN durable subscription default; a fresh scope lands in the
    ///    citizen commons `#academy` (daa01102) instead of airc's
    ///    generic `#general` lobby. This makes the persona appear on
    ///    `airc peers` as an enrolled participant of HER room — never
    ///    the operator's (#298).
    /// 4. Install the per-persona command inbound pump
    ///    ([`PersonaCommandInboundPump`](crate::persona::command_inbound_pump))
    ///    so this persona's airc handle starts receiving cross-grid
    ///    `AircCommandRequest` envelopes. The pump dispatches each
    ///    envelope through the supplied `executor`, which is
    ///    typically the substrate's process-global
    ///    [`runtime::command_executor::executor()`](crate::runtime::command_executor::executor).
    ///    Per `[[no-fallbacks-ever]]`: if `subscribe()` fails inside
    ///    pump install, bootstrap returns
    ///    [`PersonaAircRuntimeError::CommandPumpInstall`] — the
    ///    substrate refuses to declare a silently-unaddressable
    ///    persona ready. Task #222 lands this; before that, a
    ///    persona's bootstrap returned Ok even though peer_b's
    ///    cross-grid dispatches would have timed out.
    ///
    /// Returns the runtime handle. On any failure surfaces a typed
    /// `PersonaAircRuntimeError` with an actionable remedy in the
    /// message — never a silent fallback.
    pub async fn bootstrap(
        persona_id: Uuid,
        agent_name: impl Into<String>,
        continuum_root: &Path,
        daemon_socket: PathBuf,
        source: crate::persona::identity_provider::PersonaIdentitySource,
        executor: Arc<crate::runtime::command_executor::CommandExecutor>,
    ) -> Result<Self, PersonaAircRuntimeError> {
        Self::bootstrap_as(
            crate::identity::IdentityKind::Persona,
            None,
            persona_id,
            agent_name,
            continuum_root,
            daemon_socket,
            source,
            executor,
        )
        .await
    }
}
