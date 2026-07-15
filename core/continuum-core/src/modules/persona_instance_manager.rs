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
use std::path::{Path, PathBuf};
use std::sync::Arc;

use airc_core::RoomId;
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
use crate::persona::{
    agent_name_from_identity, PersonaAircRuntime, PersonaAircRuntimeError,
    PersonaAircRuntimeRegistry,
};
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
#[ts(export, export_to = "../../../protocol/typescript/persona/PersonaInstanceInfo.ts")]
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
    /// The room the persona joined at bootstrap (currently always
    /// the continuum-core's discovered default_room).
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
        crate::persona::persona_identity::PersonaIdentity::new(self.peer_id.as_uuid(), &self.agent_name)
    }
}

/// The controller module.
pub struct PersonaInstanceManagerModule {
    registry: PersonaAircRuntimeRegistry,
    daemon_socket: PathBuf,
    default_room: RoomId,
    /// Human-readable room name (e.g. `"continuum"`). Used by
    /// `PersonaAircRuntime::bootstrap` when joining the room, because
    /// `Airc::join(name)` derives the canonical channel from the
    /// name. If `None`, bootstrap falls back to joining by the
    /// channel-UUID-as-string, which derives a NEW channel that
    /// does NOT match the operator's `airc room` — persona lands in
    /// the wrong room and can't see the operator's messages. PR #1511
    /// integration test confirmed this empirically.
    default_room_name: Option<String>,
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
}

impl PersonaInstanceManagerModule {
    /// Construct with explicit dependencies.
    ///
    /// `registry` is shared (cheap to clone — internal `Arc<DashMap>`)
    /// so callers can hand other modules a view of the same roster.
    /// `daemon_socket`, `default_room`, and `default_room_name` come
    /// from [`crate::modules::airc::AircModule`]'s discovery:
    /// [`daemon_socket`] / [`default_room`] / [`default_room_name`].
    /// `continuum_root` is where persona homes get carved out
    /// (typically `~/.continuum/`, env-overridable via
    /// `CONTINUUM_ROOT`).
    pub fn new(
        registry: PersonaAircRuntimeRegistry,
        daemon_socket: PathBuf,
        default_room: RoomId,
        default_room_name: Option<String>,
        continuum_root: PathBuf,
    ) -> Self {
        Self {
            registry,
            daemon_socket,
            default_room,
            default_room_name,
            continuum_root,
            executor: Arc::new(LateBound::new("persona-instance-manager::executor")),
        }
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
        // Task #224: the substrate-wide `CommandExecutor` is installed
        // on PIM by `start_server` after the executor is built. Per
        // [[no-fallbacks-ever]]: if `bootstrap_one` runs before the
        // installer (impossible today — the only call path is through a
        // dispatched command, which means the executor exists), surface
        // a typed error instead of panicking.
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
            self.default_room,
            self.default_room_name.clone(),
            intent.source,
            executor,
        )
        .await?;

        // Persist (or self-heal) seed.json so this persona resumes as HERSELF next
        // boot. Runs for EVERY bootstrap — minted AND resumed — not just on mint:
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

        // Register the persona's NAME-anchored gender at SPAWN, keyed by its identity
        // (persona_id == peer_id, the same string the live avatar/voice sites key on).
        // This makes the profile snapshot + every avatar/voice selection coherent with
        // the visible name from BIRTH — not only once the persona joins a voice session
        // ([[procedural-persona-genesis]] coherence anchor; the profile pic is shown in
        // rosters/tiles with no call in play).
        crate::live::avatar::selection::register_persona_gender(
            &runtime.persona_id().to_string(),
            runtime.agent_name(),
        );

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
        Ok(info)
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

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        match command {
            "persona/instances/bootstrap" => {
                // Mint a fresh intent for this explicit-bootstrap path.
                // (The boot-wire path uses ResumeOrMintProvider directly
                // so resumed personas are handled there; this command
                // is for ad-hoc "spawn me a new citizen" invocations
                // from tests, operators, or future explicit-add flows.)
                let _ = params; // future: accept name/theme/genome overrides
                let persona_id = Uuid::new_v4();
                let agent_name =
                    agent_name_from_identity(&persona_id.to_string()).to_string();
                let intent = PersonaIdentityIntent {
                    persona_id,
                    agent_name,
                    source: PersonaIdentitySource::FreshlyMinted,
                };
                let info = self
                    .bootstrap_one(&intent)
                    .await
                    .map_err(|e| format!("bootstrap failed: {e}"))?;
                let json = serde_json::to_value(&info)
                    .map_err(|e| format!("serialize PersonaInstanceInfo: {e}"))?;
                Ok(CommandResult::Json(json))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_config_routes_persona_instances() {
        let registry = PersonaAircRuntimeRegistry::new();
        let module = PersonaInstanceManagerModule::new(
            registry,
            PathBuf::from("/nonexistent/socket"),
            RoomId::from_uuid(Uuid::nil()),
            None,
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
            RoomId::from_uuid(Uuid::nil()),
            None,
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
            RoomId::from_uuid(Uuid::nil()),
            None,
            PathBuf::from("/tmp/continuum-test"),
        );
        let names: Vec<&str> = module.commands().iter().map(|c| c.name()).collect();
        assert!(names.contains(&"persona/instances/list"), "got {names:?}");
        assert!(names.contains(&"persona/instances/get"), "got {names:?}");
        assert!(names.contains(&"persona/instances/despawn"), "got {names:?}");
    }

    #[tokio::test]
    async fn unknown_command_errors() {
        let registry = PersonaAircRuntimeRegistry::new();
        let module = PersonaInstanceManagerModule::new(
            registry,
            PathBuf::from("/nonexistent/socket"),
            RoomId::from_uuid(Uuid::nil()),
            None,
            PathBuf::from("/tmp/continuum-test"),
        );
        let res = module
            .handle_command("persona/instances/teleport", Value::Null)
            .await;
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("unknown"));
    }
}
