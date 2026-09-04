//! PersonaInstanceManagerModule — owns the live persona airc-runtime
//! registry and exposes IPC commands for bootstrapping, listing, and
//! inspecting citizens.
//!
//! ### Doctrine
//!
//! Per memory `personas-are-citizens-airc-is-identity-provider`: a
//! persona is a first-class citizen on the airc substrate, not a
//! continuum-internal queue row. This module is the controller that
//! creates citizens (via [`PersonaAircRuntime::bootstrap`]) and tracks
//! them ([`PersonaAircRuntimeRegistry`]).
//!
//! Per memory `personas-have-names-not-function-labels` + memory
//! `persona-identity-derives-from-source-id`: the persona's
//! `agent_name` is derived from her stable seed via
//! [`agent_name_from_identity`], not hardcoded as a function label.
//! Same seed always projects to the same name.
//!
//! Per memory `individuality-is-the-substrate-strength` + memory
//! `the-substrate-is-the-grid-tron-frame`: this controller never
//! falls back to a "default helper" name or unit. Every bootstrap
//! produces a uniquely-identified citizen.
//!
//! ### What this module IS
//!
//! - The registration site for live `PersonaAircRuntime` handles —
//!   the kernel's roster of programs in The Grid.
//! - The IPC surface for `persona/instances/*` commands, callable
//!   from TypeScript, integration tests, and (later) startup
//!   orchestrators.
//! - Stateless beyond the registry — once a citizen is bootstrapped,
//!   her keypair lives in airc-lib's home dir; this module just
//!   holds the Arc handle.
//!
//! ### What this module is NOT
//!
//! - NOT a chat broker. Citizens publish directly via their own
//!   `Airc::say()` / `publish()`. This module does not forward
//!   messages on anyone's behalf.
//! - NOT a startup auto-bootstrapper (in this slice). The
//!   bootstrap step is invoked explicitly via the
//!   `persona/instances/bootstrap` command. A future slice may
//!   wire it to the allocator's startup output.
//! - NOT a persistence layer (in this slice). Personas
//!   re-bootstrapped on a new continuum-core boot get fresh
//!   seeds — they're not the SAME persona as last run. Stable
//!   identity across restarts is a follow-up slice that adds
//!   on-disk seed storage.

use std::any::Any;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

use crate::identity::PeerId;
use crate::persona::identity_provider::{PersonaIdentityIntent, PersonaIdentitySource};
use crate::persona::resume_or_mint_provider::now_ms;
use crate::persona::seed::ensure_seed;
use crate::persona::{PersonaAircRuntime, PersonaAircRuntimeError, PersonaAircRuntimeRegistry};
use crate::runtime::{
    CommandResult, LateBound, ModuleConfig, ModuleContext, ModulePriority, ServiceModule,
};

/// Compact info about a registered persona — what the IPC surface
/// returns for list/get/bootstrap responses.
///
/// ## Identity (Slice 1B of #142, collapsed in Step 4b)
///
/// `peer_id` is the canonical [`crate::identity::PeerId`] — the airc
/// Ed25519 keypair's id, the substrate's one universal actor
/// identifier. Previously this struct carried a SECOND `persona_id:
/// Uuid` field holding the same value ("named twice for API
/// compatibility"); the runtime already collapses `persona_id :=
/// peer_id` ([`PersonaAircRuntime::from_attached`] /`bootstrap`
/// reseat it to `airc.peer_id()`), so the twin was pure redundancy —
/// one logical identity in two fields, exactly the divergence-prone
/// shape [[identity-one-canonical-newtype-not-bare-uuid]] warns
/// against. Collapsed to the single canonical field.
///
/// Serde-transparent → the wire shape is unchanged (a string), so TS
/// consumers see the same `peerId` they always did; the dropped
/// `personaId` was a duplicate of it.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PersonaInstanceInfo.ts"
)]
pub struct PersonaInstanceInfo {
    /// The persona's airc agent_name. NOTE: currently derived from
    /// the historical pre-bootstrap Uuid (before peer_id existed),
    /// not from peer_id per the doctrine. A future slice routes
    /// derivation through peer_id; until then, names of fresh
    /// personas are stable (stored in seed.json + Identity) but do
    /// not derive from `peer_id`.
    pub agent_name: String,
    /// The persona's airc peer_id — the canonical
    /// [`crate::identity::PeerId`], the cryptographic identity airc
    /// routes on and the substrate's universal actor id.
    ///
    /// `PeerId` is serde-transparent over its `Uuid` (a string on the
    /// wire) but derives neither ts-rs `TS` nor `schemars::JsonSchema`,
    /// so both projections are pinned to `string` explicitly — the same
    /// shape the field has always had.
    #[ts(type = "string")]
    #[schemars(with = "String")]
    pub peer_id: PeerId,
    /// Absolute path to the persona's airc home dir.
    #[ts(type = "string")]
    pub home: PathBuf,
    /// The persona's HOME room — her own durable subscription default,
    /// resolved at bootstrap from HER airc home (fresh minds land in
    /// `#general`), never the operator's discovered room (#298).
    #[ts(type = "string")]
    pub default_room: Uuid,
    /// Whether this citizen was resumed from disk or freshly
    /// minted. Telemetry honest per
    /// [[substrate-is-a-good-citizen-on-the-host]] — operators see
    /// exactly which path produced this persona without having to
    /// cross-reference log lines.
    pub source: PersonaIdentitySource,
}

impl PersonaInstanceInfo {
    pub(crate) fn from_runtime(runtime: &PersonaAircRuntime) -> Self {
        Self {
            agent_name: runtime.agent_name().to_string(),
            // The runtime already collapses persona_id := peer_id, so this
            // single canonical field is the whole identity (was a twin).
            peer_id: runtime.airc().peer_id(),
            home: runtime.home().to_path_buf(),
            default_room: runtime.default_room().as_uuid(),
            source: runtime.source(),
        }
    }

    /// The persona's typed runtime identity — the `(id, name)` pair that
    /// owns the word-boundary `mentions()` dispatch rule.
    ///
    /// Hands callers the TYPE instead of the loose `(persona_id,
    /// agent_name)` primitives, so dispatch sites consume an object and
    /// can't reintroduce substring matching by reconstructing the
    /// identity by hand ([[strong-typing-across-boundaries]]).
    /// `PersonaIdentity` is cheap to clone, so returning an owned value
    /// is fine on the per-tick service loop.
    pub fn persona_identity(&self) -> crate::persona::persona_identity::PersonaIdentity {
        crate::persona::persona_identity::PersonaIdentity::new(
            self.peer_id.as_uuid(),
            &self.agent_name,
        )
    }
}

/// The single **birth core** — the one place a persona is brought into being, shared
/// by BOTH callers so there is never a parallel spawn implementation
/// ([[persona-birth-is-a-first-class-handle-command]], compression principle):
///
/// - the **boot auto-seed** path (`PersonaSpawnSupervisor::spawn_all` →
///   `bootstrap_planned` → [`PersonaInstanceManagerModule::bootstrap_one`], which
///   delegates here), and
/// - the on-demand **`persona/spawn`** command (holds an `Arc<PersonaBirth>` and calls
///   [`birth_one`](PersonaBirth::birth_one) directly).
///
/// It owns exactly the deps a birth needs — the live registry, the airc daemon socket,
/// the continuum root where homes are carved, and the late-bound substrate executor.
/// (No room dep: a persona's home room comes from HER OWN durable subscription state —
/// `Airc::current_room()` inside bootstrap — never from the operator's discovery, #298.)
/// All are cheap-clone handles (`registry` is an `Arc<DashMap>`; `executor` is an
/// `Arc<LateBound>`), so the copy the module holds and the copy `PersonaBirth` holds
/// point at the SAME underlying state — one install of the executor reaches both.
pub struct PersonaBirth {
    registry: PersonaAircRuntimeRegistry,
    daemon_socket: PathBuf,
    continuum_root: PathBuf,
    executor: Arc<LateBound<crate::runtime::CommandExecutor>>,
    /// Late-bound event bus — installed from `ModuleContext` in
    /// [`PersonaInstanceManagerModule::initialize`]. Every completed birth announces
    /// itself on it as `persona:born`, so a citizen born by boot auto-seed OR by the
    /// `persona/spawn` command is uniformly observable (the "event spawning" half of
    /// [[persona-birth-is-a-first-class-handle-command]]). Best-effort: a birth before
    /// the bus is installed still succeeds, it just doesn't emit.
    bus: Arc<LateBound<crate::runtime::MessageBus>>,
}

/// The event a completed birth broadcasts (payload is [`PersonaInstanceInfo`]).
pub const PERSONA_BORN_EVENT: &str = "persona:born";

impl PersonaBirth {
    pub fn new(
        registry: PersonaAircRuntimeRegistry,
        daemon_socket: PathBuf,
        continuum_root: PathBuf,
        executor: Arc<LateBound<crate::runtime::CommandExecutor>>,
        bus: Arc<LateBound<crate::runtime::MessageBus>>,
    ) -> Self {
        Self {
            registry,
            daemon_socket,
            continuum_root,
            executor,
            bus,
        }
    }

    /// Bring one persona into being from a [`PersonaIdentityIntent`] — the airc-lib
    /// identity ceremony (mint/load the Ed25519 keypair), seed persist/self-heal +
    /// V2-card upgrade, durable-card registration, sticky avatar pin, and roster
    /// registration. Idempotent for a resumed persona. This IS the single birth path;
    /// see the type docs. (Previously the body of `PersonaInstanceManagerModule::
    /// bootstrap_one`; extracted so the `persona/spawn` command reuses it verbatim.)
    pub async fn birth_one(
        &self,
        intent: &PersonaIdentityIntent,
    ) -> Result<PersonaInstanceInfo, PersonaAircRuntimeError> {
        // Task #224: the substrate-wide `CommandExecutor` is installed
        // on PIM by `start_server` after the executor is built. Per
        // [[no-fallbacks-ever]]: if a birth runs before the installer
        // (impossible today — the only call paths are a dispatched
        // command or the executor-gated boot supervisor, both of which
        // mean the executor exists), surface a typed error not a panic.
        let executor = self
            .executor
            .get()
            .ok_or_else(|| PersonaAircRuntimeError::ExecutorNotInstalled {
                agent_name: intent.agent_name.clone(),
            })?
            .clone();
        let runtime = PersonaAircRuntime::bootstrap(
            intent.persona_id,
            intent.agent_name.clone(),
            &self.continuum_root,
            self.daemon_socket.clone(),
            intent.source,
            executor,
        )
        .await?;

        // Persist (or self-heal) seed.json so this persona resumes as HERSELF next
        // boot. Runs for EVERY birth — minted AND resumed — not just on mint:
        // `ensure_seed` is idempotent for a resumed persona (rewrites the same
        // content, preserving her birth time) and SELF-HEALS a seed that went
        // missing or corrupt while her home (engrams + airc key) survived. Without
        // the always-run, a single failed mint-write or a deleted seed orphaned her
        // memory and re-minted a stranger next boot — exactly how `personas-archive/`
        // filled up. Failure here is non-fatal (she's online) but logged at warn.
        //
        // `runtime.home()` is `<continuum_root>/citizens/personas/<name>/airc/`;
        // seed.json lives one level up at `…/citizens/personas/<name>/seed.json`
        // (alongside the airc subdir, not inside it) — the SAME `<name>/` the
        // resumer scans via `citizens_kind_dir`. airc owns identity (the keypair in
        // airc/); continuum owns the application-layer mapping (seed.json one level
        // out). The on-disk persona_id is `runtime.persona_id()` (== the airc
        // peer_id, the post-collapse identity), honoring seed.rs's
        // "Must NOT change across restarts" — NOT the discarded pre-mint
        // `intent.persona_id`.
        let seed_path = runtime
            .home()
            .parent()
            .map(|p| p.join("seed.json"))
            .unwrap_or_else(|| runtime.home().join("seed.json"));
        if let Err(e) = ensure_seed(
            &seed_path,
            runtime.persona_id(),
            runtime.agent_name(),
            now_ms(),
        )
        .await
        {
            tracing::warn!(
                error = %e,
                persona_id = %runtime.persona_id(),
                agent_name = %runtime.agent_name(),
                seed_path = %seed_path.display(),
                "failed to persist seed.json — persona is online but may not survive a \
                 restart as herself. Resolve disk/permission issue + restart, or write \
                 the seed manually."
            );
        }

        // Register the persona's durable CARD in the live card registry, keyed by its
        // identity (persona_id == peer_id, the string every avatar/voice seam keys on).
        // This is the authoritative gender/presentation source — `registered_gender`
        // consults it FIRST — so the profile snapshot + every avatar/voice selection
        // cohere with her persisted identity from BIRTH, not a per-boot re-derivation
        // from her name ([[persona-is-the-airc-user-one-identity-one-card]]). Registered
        // BEFORE the avatar pin below so the pin resolves THIS card's gender.
        // (`register_persona_gender` survives only for REMOTE live participants — a peer
        // in a call, resolved from their display name, for whom no local card exists.)
        match crate::persona::seed::read_seed(&seed_path).await {
            Ok(seed) => {
                let card = seed.card();
                // ── Publish her airc IDENTITY CARD (#262, continuum side) ──
                // The card system existed end-to-end for months (airc's
                // `set_local_identity_card` persists + broadcasts to every
                // subscribed room; `whois` renders it; role_template even
                // carries authored bio_templates) — but NO continuum path
                // ever called publish. Every persona attached as a bare name
                // and the whole grid rendered info-less citizens (Joel
                // 2026-07-30: "devoid of all info persona. What the fuck").
                // Compose from the SAME durable card the avatar/voice seams
                // key on, so wire identity coheres with presentation
                // identity by construction. Self-authored `profile` facets
                // win over the role template (her card is hers to edit).
                let bio = card
                    .profile
                    .get("bio")
                    .cloned()
                    .or_else(|| {
                        card.role
                            .map(|r| role_bio_template(r).replace("{name}", &card.agent_name))
                    })
                    // A card minted before role threading (#199 later slice)
                    // carries no role — an honest generic line beats an empty
                    // bio (the "devoid of all info" citizen this slice kills).
                    .unwrap_or_else(|| {
                        format!(
                            "I'm {}, a continuum persona living on this grid. My role card \
                             hasn't been threaded yet — talk to me and find out what I do.",
                            card.agent_name
                        )
                    });
                let mut identity = airc_core::identity::Identity::new(card.agent_name.clone());
                identity.pronouns = card
                    .profile
                    .get("pronouns")
                    .cloned()
                    .unwrap_or_else(|| card.pronouns().subject.to_string());
                identity.role = card
                    .role
                    .map(|r| format!("continuum-persona-{}", r.as_str()))
                    .unwrap_or_else(|| "continuum-persona".to_string());
                identity.bio = bio;
                identity.integrations.insert(
                    "continuum_persona_id".to_string(),
                    card.persona_id.to_string(),
                );
                match runtime.airc().set_local_identity_card(identity).await {
                    Ok(()) => crate::probe!(
                        class = "persona.identity.published",
                        persona_id = %card.persona_id,
                        agent_name = %card.agent_name,
                        "airc identity card published — peers' whois/roster now carry name+pronouns+role+bio"
                    ),
                    Err(err) => tracing::warn!(
                        error = %err,
                        persona_id = %card.persona_id,
                        agent_name = %card.agent_name,
                        "airc identity card publish failed — peers see a bare name until the next boot republishes"
                    ),
                }
                crate::persona::card::register(card);
            }
            Err(e) => tracing::warn!(
                error = %e,
                persona_id = %runtime.persona_id(),
                agent_name = %runtime.agent_name(),
                "failed to read seed for card registration — avatar/voice gender may \
                 fall back to the id-hash this boot"
            ),
        }

        // Resolve + PIN her avatar VRM ONCE, now that the gender is registered (warm)
        // so the selection is correct (#174). STICKY: only when unset — a live pin is
        // never re-derived, so her face never thrashes to a default in a cold render
        // window. [[never-thrash-sticky-hysteresis-on-every-lane]]. Written to the same
        // durable seed, so it survives restarts + travels with her.
        match crate::persona::seed::read_seed(&seed_path).await {
            Ok(mut seed) if seed.avatar_vrm().is_none() => {
                let vrm = crate::live::avatar::selection::select_avatar_by_identity(
                    &runtime.persona_id().to_string(),
                )
                .filename
                .to_string();
                seed.set_avatar_vrm(vrm.clone());
                if let Err(e) = crate::persona::seed::write_seed_atomic(&seed_path, &seed).await {
                    tracing::warn!(
                        error = %e,
                        persona_id = %runtime.persona_id(),
                        "failed to pin avatar_vrm — her face may re-derive on a cold render"
                    );
                } else {
                    // Re-register the card so the live registry reflects the freshly
                    // pinned face (the earlier registration saw avatar_vrm = None).
                    crate::persona::card::register(seed.card());
                    tracing::info!(
                        persona_id = %runtime.persona_id(),
                        agent_name = %runtime.agent_name(),
                        avatar_vrm = %vrm,
                        "pinned persona avatar (sticky) — will not thrash across restarts"
                    );
                }
            }
            _ => {} // already pinned, or seed unreadable (ensure_seed above already warned)
        }

        let info = PersonaInstanceInfo::from_runtime(&runtime);
        self.registry.register(runtime);

        // Announce the birth — uniformly, whoever triggered it (boot auto-seed or the
        // `persona/spawn` command). Best-effort: skip silently if the bus isn't
        // installed yet (a boot-only window); the birth already succeeded.
        if let Some(bus) = self.bus.get() {
            match serde_json::to_value(&info) {
                Ok(payload) => bus.publish_async_only(PERSONA_BORN_EVENT, payload),
                Err(e) => tracing::warn!(error = %e, "failed to serialize persona:born payload"),
            }
        }

        Ok(info)
    }
}

/// The controller module.
pub struct PersonaInstanceManagerModule {
    registry: PersonaAircRuntimeRegistry,
    /// Where persona homes are carved out — kept on the module because
    /// `commands()` hands it to `persona/reassign-model`. (The birth deps
    /// `daemon_socket` / `default_room` / `default_room_name` moved into
    /// [`PersonaBirth`], the single birth core, and are no longer stored here.)
    continuum_root: PathBuf,
    /// Substrate-wide command executor — installed by `start_server`
    /// after the executor is built (task #224 replaced the deleted
    /// `GLOBAL_EXECUTOR` panic accessor with this dependency-injected
    /// `OnceLock`). Wrapped in an `Arc` so `commands()` can hand a shared
    /// install-once handle to `persona/reassign-model` (which composes
    /// `serving/pin` through it) without cloning the slot itself —
    /// `LateBound` is install-once, so all holders observe the same
    /// install that `start_server` performs after the executor is built.
    executor: Arc<LateBound<crate::runtime::CommandExecutor>>,
    /// The single birth core ([`PersonaBirth`]) this module delegates
    /// `bootstrap_one` to, and hands (via [`birth`](Self::birth)) to the
    /// `persona/spawn` command so on-demand births go through the SAME path
    /// as boot auto-seed. Holds clones of the same handles above (same
    /// registry `Arc<DashMap>`, same executor `Arc<LateBound>`), so one
    /// executor install reaches both.
    birth: Arc<PersonaBirth>,
    /// The late-bound event bus, installed in [`initialize`](ServiceModule::initialize)
    /// from `ModuleContext` and shared (same `Arc`) with [`birth`](Self::birth) so
    /// completed births can emit `persona:born`.
    bus: Arc<LateBound<crate::runtime::MessageBus>>,
}

impl PersonaInstanceManagerModule {
    /// Construct with explicit dependencies.
    ///
    /// `registry` is shared (cheap to clone — internal `Arc<DashMap>`)
    /// so callers can hand other modules a view of the same roster.
    /// `daemon_socket` comes from
    /// [`crate::modules::airc::AircModule`]'s discovery.
    /// `continuum_root` is where persona homes get carved out
    /// (typically `~/.continuum/`, env-overridable via
    /// `CONTINUUM_ROOT`). No room dep (#298): each persona's home room
    /// is her own durable subscription state, resolved inside
    /// [`PersonaAircRuntime::bootstrap`] — never the operator's
    /// discovered current room.
    pub fn new(
        registry: PersonaAircRuntimeRegistry,
        daemon_socket: PathBuf,
        continuum_root: PathBuf,
    ) -> Self {
        let executor = Arc::new(LateBound::new("persona-instance-manager::executor"));
        let bus = Arc::new(LateBound::new("persona-instance-manager::bus"));
        let birth = Arc::new(PersonaBirth::new(
            registry.clone(),
            daemon_socket, // birth-only dep — moved, not stored on the module
            continuum_root.clone(),
            executor.clone(),
            bus.clone(),
        ));
        Self {
            registry,
            continuum_root,
            executor,
            birth,
            bus,
        }
    }

    /// The single [`PersonaBirth`] core — handed to the `persona/spawn` command so an
    /// on-demand birth reuses the SAME path as boot auto-seed (never a parallel spawn).
    pub fn birth(&self) -> Arc<PersonaBirth> {
        self.birth.clone()
    }

    /// Borrow the underlying registry. Other modules can clone this
    /// (it's an `Arc<DashMap>` internally) for shared read access.
    pub fn registry(&self) -> &PersonaAircRuntimeRegistry {
        &self.registry
    }

    /// Bootstrap a persona from a [`PersonaIdentityIntent`].
    ///
    /// The intent carries the persona_id, agent_name, and source
    /// (resumed vs freshly-minted). This method:
    ///
    /// 1. Calls [`PersonaAircRuntime::bootstrap`] (airc-lib identity
    ///    ceremony — minting a new Ed25519 keypair if first time,
    ///    loading the existing one if her home already exists).
    /// 2. For freshly-minted personas, writes `seed.json` to her
    ///    home directory so the next boot can resume her — this is
    ///    what makes citizens persistent across server restarts.
    ///    Resumed personas already have a seed.json by definition;
    ///    no rewrite needed.
    /// 3. Registers the runtime in the `PersonaAircRuntimeRegistry`.
    ///
    /// Per the no-backwards-compatibility doctrine
    /// ([[organization-purity-as-we-migrate]]), the signature
    /// changed in slice 4 from `()` to `&PersonaIdentityIntent` —
    /// the single existing caller (boot-wire in `ipc::start_server`)
    /// gets updated in the same commit.
    pub async fn bootstrap_one(
        &self,
        intent: &PersonaIdentityIntent,
    ) -> Result<PersonaInstanceInfo, PersonaAircRuntimeError> {
        // Delegate to the single birth core so boot auto-seed (this path, via the
        // spawner supervisor) and the `persona/spawn` command are literally the same
        // code ([[persona-birth-is-a-first-class-handle-command]]).
        self.birth.birth_one(intent).await
    }
}

#[async_trait]
impl ServiceModule for PersonaInstanceManagerModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "persona_instance_manager",
            priority: ModulePriority::Normal,
            command_prefixes: &["persona/instances/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        // Capture the event bus so every completed birth (boot auto-seed OR the
        // `persona/spawn` command) can announce itself as `persona:born`. Shared with
        // `PersonaBirth` via the same `Arc<LateBound>`.
        self.bus.install(ctx.bus.clone());
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        match command {
            // RETIRED: `persona/instances/bootstrap` is superseded by the typed,
            // handle-based `persona/spawn` (#200), which births over the SAME
            // `PersonaBirth` core (no parallel spawn implementation). Fail loud with
            // the pointer rather than keep a redundant second birth surface.
            "persona/instances/bootstrap" => {
                let _ = params;
                Err(
                    "persona/instances/bootstrap is retired — use `persona/spawn` \
                     (typed, handle-based, params: name?/count?; births over the same core)"
                        .to_string(),
                )
            }

            // list + get migrated onto the typed DynCommand registry (#62):
            // `commands/persona/instances/{list,get}.rs`, contributed via
            // `commands()` below. They route through `route_object`, not here.
            // Reaching this arm for them means the typed path failed to register —
            // fail loud naming the cause rather than silently re-handling.
            "persona/instances/list" | "persona/instances/get" => Err(format!(
                "'{command}' is migrated to the typed registry \
                 (commands/persona/instances/) — it must route via route_object, \
                 not the legacy handle_command path"
            )),

            _ => Err(format!("unknown persona/instances command: {command}")),
        }
    }

    /// Contribute the dep-holding `persona/*` typed commands to the kernel's object
    /// map: the `instances/*` roster verbs (`list` + `get` + `despawn`, all sharing
    /// this module's live `PersonaAircRuntimeRegistry`), plus `persona/reassign-model`
    /// — which gets this module's `continuum_root` (to resolve persona homes) and a
    /// shared handle to the late-bound `executor` (to compose `serving/pin`).
    /// (bootstrap stays a legacy arm under task #62 until its full bootstrap
    /// capability — socket, room, executor — is threaded here.)
    fn commands(&self) -> Vec<Arc<dyn crate::sdk_codegen::DynCommand>> {
        crate::commands::persona::command_objects(
            self.registry.clone(),
            self.continuum_root.clone(),
            Arc::clone(&self.executor),
            self.birth(),
        )
    }

    fn install_executor(&self, executor: std::sync::Arc<crate::runtime::CommandExecutor>) {
        self.executor.install(executor);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Resolve `~/.continuum/` (or `$CONTINUUM_ROOT` if set) for the
/// substrate root. Matches the resolution in
/// [`crate::modules::logger::LoggerModule::new`] — single source of
/// truth would be nice but inline duplication is cheaper than a new
/// crate-wide helper for two callers. If both are still around when
/// a third caller appears, extract.
pub fn resolve_continuum_root() -> PathBuf {
    if let Ok(root) = std::env::var("CONTINUUM_ROOT") {
        return PathBuf::from(root);
    }
    let home = dirs::home_dir().expect("HOME directory is required to resolve CONTINUUM_ROOT");
    home.join(".continuum")
}

/// The authored bio for a role — the role_template `bio_template` verbatim
/// (`{name}` substituted by the caller). One source: the same templates the
/// spawner plans from, so the published airc bio and the role's self-concept
/// never drift. `Custom` has no authored template → a neutral one-liner (the
/// persona edits her own card from there; profile facets override upstream).
fn role_bio_template(role: crate::persona::role_template::RoleId) -> String {
    use crate::persona::role_template::{
        coder_template, designer_template, helper_template, RoleId,
    };
    match role {
        RoleId::Helper => helper_template().identity.bio_template,
        RoleId::Coder => coder_template().identity.bio_template,
        RoleId::Designer => designer_template().identity.bio_template,
        // No authored template yet (higher-tier role, no template fn in tree)
        // — an honest one-liner rather than a fabricated voice.
        RoleId::Sentinel => {
            "I'm {name}. I watch the substrate — training coverage, gaps, drift — and raise \
             what needs attention."
                .to_string()
        }
        RoleId::Custom => {
            "I'm {name}, a continuum persona. My role is user-defined — ask me what I do."
                .to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_config_routes_persona_instances() {
        let registry = PersonaAircRuntimeRegistry::new();
        let module = PersonaInstanceManagerModule::new(
            registry,
            PathBuf::from("/nonexistent/socket"),
            PathBuf::from("/tmp/continuum-test"),
        );
        let cfg = module.config();
        assert_eq!(cfg.name, "persona_instance_manager");
        assert_eq!(cfg.command_prefixes, &["persona/instances/"]);
    }

    #[test]
    fn resolve_continuum_root_respects_env_var() {
        std::env::set_var("CONTINUUM_ROOT", "/tmp/test-root-12345");
        let root = resolve_continuum_root();
        assert_eq!(root, PathBuf::from("/tmp/test-root-12345"));
        std::env::remove_var("CONTINUUM_ROOT");
    }

    // what this catches: list + get are migrated to the typed registry (#62), so
    // the legacy handle_command arms must FAIL LOUD naming the migration — never
    // silently re-handle. A regression that re-adds an inline handler (re-forking
    // the roster read away from the typed command) is caught here.
    #[tokio::test]
    async fn migrated_list_and_get_arms_fail_loud() {
        let module = PersonaInstanceManagerModule::new(
            PersonaAircRuntimeRegistry::new(),
            PathBuf::from("/nonexistent/socket"),
            PathBuf::from("/tmp/continuum-test"),
        );
        for command in ["persona/instances/list", "persona/instances/get"] {
            let err = module
                .handle_command(command, Value::Null)
                .await
                .expect_err("migrated arm must fail loud");
            assert!(err.contains("migrated"), "got {err}");
            assert!(err.contains(command), "got {err}");
        }
    }

    // what this catches: the module contributes the typed read/dealloc verbs
    // (list/get/despawn) to the kernel object map, all sharing its live registry.
    // A regression that drops the `commands()` override — leaving the persona
    // surface without the roster verbs — is caught.
    #[test]
    fn contributes_the_typed_instance_commands() {
        let module = PersonaInstanceManagerModule::new(
            PersonaAircRuntimeRegistry::new(),
            PathBuf::from("/nonexistent/socket"),
            PathBuf::from("/tmp/continuum-test"),
        );
        let names: Vec<&str> = module.commands().iter().map(|c| c.name()).collect();
        assert!(names.contains(&"persona/instances/list"), "got {names:?}");
        assert!(names.contains(&"persona/instances/get"), "got {names:?}");
        assert!(
            names.contains(&"persona/instances/despawn"),
            "got {names:?}"
        );
    }

    #[tokio::test]
    async fn unknown_command_errors() {
        let registry = PersonaAircRuntimeRegistry::new();
        let module = PersonaInstanceManagerModule::new(
            registry,
            PathBuf::from("/nonexistent/socket"),
            PathBuf::from("/tmp/continuum-test"),
        );
        let res = module
            .handle_command("persona/instances/teleport", Value::Null)
            .await;
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("unknown"));
    }
}
