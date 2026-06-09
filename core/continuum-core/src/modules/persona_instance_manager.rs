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
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::persona::identity_provider::{PersonaIdentityIntent, PersonaIdentitySource};
use crate::persona::resume_or_mint_provider::now_ms;
use crate::persona::seed::{write_seed_atomic, PersonaSeedFile};
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
/// ## INVARIANT (Slice 1B of #142)
///
/// `persona_id == peer_id`. Both fields hold the airc Ed25519
/// keypair's Uuid; the runtime constructor collapses them per
/// [[persona-identity-derives-from-source-id]] (the cryptographic
/// keypair IS the substrate identity). The two fields exist
/// side-by-side for API back-compat; a future cleanup may collapse
/// to a single `peer_id` field once external consumers no longer
/// reference `persona_id`.
///
/// Test fixtures that bypass `from_runtime` (e.g.
/// `supervisor::tests::fake_instance`, `service_loop` test fixture)
/// honor this invariant by convention: `persona_id` and `peer_id`
/// are set to the same Uuid even when the keypair is stubbed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaInstanceInfo {
    /// The persona's airc peer_id (Ed25519 keypair Uuid) — the
    /// substrate's universal actor identifier per Slice 1B of #142.
    /// Equals `peer_id` field by invariant.
    pub persona_id: Uuid,
    /// The persona's airc agent_name. NOTE: currently derived from
    /// the historical pre-bootstrap Uuid (before peer_id existed),
    /// not from peer_id per the doctrine. A future slice routes
    /// derivation through peer_id; until then, names of fresh
    /// personas are stable (stored in seed.json + Identity) but do
    /// not derive from `peer_id`.
    pub agent_name: String,
    /// The persona's airc peer_id. Equals `persona_id` post-
    /// Slice-1B (same Uuid, named twice for API compatibility).
    /// The cryptographic identity airc routes on.
    pub peer_id: Uuid,
    /// Absolute path to the persona's airc home dir.
    pub home: PathBuf,
    /// The room the persona joined at bootstrap (currently always
    /// the continuum-core's discovered default_room).
    pub default_room: Uuid,
    /// Whether this citizen was resumed from disk or freshly
    /// minted. Telemetry honest per
    /// [[substrate-is-a-good-citizen-on-the-host]] — operators see
    /// exactly which path produced this persona without having to
    /// cross-reference log lines.
    pub source: PersonaIdentitySource,
}

impl PersonaInstanceInfo {
    fn from_runtime(runtime: &PersonaAircRuntime) -> Self {
        Self {
            persona_id: runtime.persona_id(),
            agent_name: runtime.agent_name().to_string(),
            peer_id: runtime.airc().peer_id().as_uuid(),
            home: runtime.home().to_path_buf(),
            default_room: runtime.default_room().as_uuid(),
            source: runtime.source(),
        }
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
    /// `OnceLock`).
    executor: LateBound<crate::runtime::CommandExecutor>,
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
            executor: LateBound::new("persona-instance-manager::executor"),
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

        // For freshly-minted personas, write seed.json so next boot
        // can resume them. Failure here is non-fatal — the persona
        // bootstrapped fine, she just won't survive a restart.
        // Logged at warn so operators see and can act.
        if intent.source == PersonaIdentitySource::FreshlyMinted {
            // runtime.home() is `<continuum_root>/personas/<name>/airc/`.
            // seed.json lives one level up at
            // `<continuum_root>/personas/<name>/seed.json` — alongside
            // the airc subdirectory, not inside it. This matches the
            // doctrine that airc owns identity (the keypair inside
            // airc/) and continuum owns the application-layer mapping
            // (seed.json one level out).
            let seed_path = runtime
                .home()
                .parent()
                .map(|p| p.join("seed.json"))
                .unwrap_or_else(|| runtime.home().join("seed.json"));
            // Slice 1B of #142: write the POST-COLLAPSE persona_id —
            // i.e. `runtime.persona_id()` (which equals
            // `runtime.airc().peer_id().as_uuid()`) — to seed.json,
            // NOT the discarded `intent.persona_id` from the
            // pre-bootstrap mint. The seed.rs contract says
            // "Must NOT change across restarts"; honoring it means
            // the on-disk Uuid IS the substrate identity (peer_id),
            // not the historical-artifact seed Uuid.
            let seed = PersonaSeedFile::V1 {
                persona_id: runtime.persona_id(),
                agent_name: runtime.agent_name().to_string(),
                created_at_ms: now_ms(),
            };
            if let Err(e) = write_seed_atomic(&seed_path, &seed).await {
                tracing::warn!(
                    error = %e,
                    persona_id = %runtime.persona_id(),
                    agent_name = %runtime.agent_name(),
                    seed_path = %seed_path.display(),
                    "failed to write seed.json — persona is online but won't survive restart. \
                     Resolve disk/permission issue + restart to re-mint, or write the seed \
                     manually."
                );
            }
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

            "persona/instances/list" => {
                let infos: Vec<PersonaInstanceInfo> = self
                    .registry
                    .iter()
                    .map(|rt| PersonaInstanceInfo::from_runtime(&rt))
                    .collect();
                let json = serde_json::to_value(&infos)
                    .map_err(|e| format!("serialize Vec<PersonaInstanceInfo>: {e}"))?;
                Ok(CommandResult::Json(json))
            }

            "persona/instances/get" => {
                let persona_id_str = params
                    .get("personaId")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "persona/instances/get requires personaId".to_string())?;
                let persona_id = Uuid::parse_str(persona_id_str)
                    .map_err(|e| format!("invalid personaId UUID: {e}"))?;
                match self.registry.get(persona_id) {
                    Some(rt) => {
                        let info = PersonaInstanceInfo::from_runtime(&rt);
                        let json = serde_json::to_value(&info)
                            .map_err(|e| format!("serialize PersonaInstanceInfo: {e}"))?;
                        Ok(CommandResult::Json(json))
                    }
                    None => Err(format!("no persona registered with id {persona_id}")),
                }
            }

            _ => Err(format!("unknown persona/instances command: {command}")),
        }
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

    #[tokio::test]
    async fn get_returns_error_for_unknown_persona_id() {
        let registry = PersonaAircRuntimeRegistry::new();
        let module = PersonaInstanceManagerModule::new(
            registry,
            PathBuf::from("/nonexistent/socket"),
            RoomId::from_uuid(Uuid::nil()),
            None,
            PathBuf::from("/tmp/continuum-test"),
        );
        let params = serde_json::json!({"personaId": Uuid::new_v4().to_string()});
        let res = module.handle_command("persona/instances/get", params).await;
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("no persona registered"));
    }

    #[tokio::test]
    async fn list_returns_empty_array_when_no_instances() {
        let registry = PersonaAircRuntimeRegistry::new();
        let module = PersonaInstanceManagerModule::new(
            registry,
            PathBuf::from("/nonexistent/socket"),
            RoomId::from_uuid(Uuid::nil()),
            None,
            PathBuf::from("/tmp/continuum-test"),
        );
        let res = module
            .handle_command("persona/instances/list", Value::Null)
            .await;
        match res {
            Ok(CommandResult::Json(v)) => {
                let arr = v.as_array().expect("list returns array");
                assert!(arr.is_empty());
            }
            other => panic!("expected Ok(Json), got {other:?}"),
        }
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
