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

use crate::persona::{
    agent_name_from_identity, PersonaAircRuntime, PersonaAircRuntimeError,
    PersonaAircRuntimeRegistry,
};
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};

/// Compact info about a registered persona — what the IPC surface
/// returns for list/get/bootstrap responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaInstanceInfo {
    /// Continuum-side stable identifier (the seed).
    pub persona_id: Uuid,
    /// The airc agent_name derived from the seed.
    pub agent_name: String,
    /// The airc peer_id minted by `airc-lib` when the runtime
    /// bootstrapped. Independent of `persona_id` — this is the
    /// cryptographic identity airc routes on.
    pub peer_id: Uuid,
    /// Absolute path to the persona's airc home dir.
    pub home: PathBuf,
    /// The room the persona joined at bootstrap (currently always
    /// the continuum-core's discovered default_room).
    pub default_room: Uuid,
}

impl PersonaInstanceInfo {
    fn from_runtime(runtime: &PersonaAircRuntime) -> Self {
        Self {
            persona_id: runtime.persona_id(),
            agent_name: runtime.agent_name().to_string(),
            peer_id: runtime.airc().peer_id(),
            home: runtime.home().to_path_buf(),
            default_room: runtime.default_room().as_uuid(),
        }
    }
}

/// The controller module.
pub struct PersonaInstanceManagerModule {
    registry: PersonaAircRuntimeRegistry,
    daemon_socket: PathBuf,
    default_room: RoomId,
    continuum_root: PathBuf,
}

impl PersonaInstanceManagerModule {
    /// Construct with explicit dependencies.
    ///
    /// `registry` is shared (cheap to clone — internal `Arc<DashMap>`)
    /// so callers can hand other modules a view of the same roster.
    /// `daemon_socket` and `default_room` come from
    /// [`crate::modules::airc::AircModule::daemon_socket`] /
    /// [`default_room`] — discovered at server boot.
    /// `continuum_root` is where persona homes get carved out
    /// (typically `~/.continuum/`, env-overridable via
    /// `CONTINUUM_ROOT`).
    pub fn new(
        registry: PersonaAircRuntimeRegistry,
        daemon_socket: PathBuf,
        default_room: RoomId,
        continuum_root: PathBuf,
    ) -> Self {
        Self {
            registry,
            daemon_socket,
            default_room,
            continuum_root,
        }
    }

    /// Borrow the underlying registry. Other modules can clone this
    /// (it's an `Arc<DashMap>` internally) for shared read access.
    pub fn registry(&self) -> &PersonaAircRuntimeRegistry {
        &self.registry
    }

    /// Bootstrap a fresh persona. Generates a UUIDv4 seed, derives
    /// the agent_name from the seed via [`agent_name_from_identity`],
    /// calls [`PersonaAircRuntime::bootstrap`] (which performs the
    /// airc-lib identity ceremony — minting a new Ed25519 keypair
    /// for this persona), and registers the runtime.
    ///
    /// In this slice the seed is fresh per call (not persisted).
    /// Stable-across-restarts identity is a follow-up.
    async fn bootstrap_one(&self) -> Result<PersonaInstanceInfo, PersonaAircRuntimeError> {
        let persona_id = Uuid::new_v4();
        let agent_name = agent_name_from_identity(&persona_id.to_string());

        let runtime = PersonaAircRuntime::bootstrap(
            persona_id,
            agent_name,
            &self.continuum_root,
            self.daemon_socket.clone(),
            self.default_room,
        )
        .await?;

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
                // params currently unused — future: take a PersonaAllocation
                // and derive seed/genome from it. For now: fresh random
                // citizen each call.
                let _ = params;
                let info = self
                    .bootstrap_one()
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
            PathBuf::from("/tmp/continuum-test"),
        );
        let res = module
            .handle_command("persona/instances/teleport", Value::Null)
            .await;
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("unknown"));
    }
}
