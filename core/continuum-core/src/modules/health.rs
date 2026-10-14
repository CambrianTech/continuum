//! HealthModule — the trivial outlier that validates the ServiceModule interface.
//!
//! Handles: health-check, get-stats
//! This is Phase 1: if this module routes correctly through the registry,
//! the ServiceModule trait design is proven for the simplest case.

use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::sdk_codegen::{ActionCommand, CommandError, Ctx};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::any::Any;
use std::time::Instant;
use ts_rs::TS;

/// Params for `ping` — the canonical health/liveness command every SDK exposes.
/// An optional echo message round-trips so a caller can correlate.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/health/PingParams.ts"
)]
pub struct PingParams {
    /// Optional message echoed back (for correlation / a hello).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub message: Option<String>,
}

/// Result of `ping` — the substrate is alive.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/health/PingResult.ts"
)]
pub struct PingResult {
    /// Always true on a successful round-trip.
    pub ok: bool,
    /// Substrate-measured handling time in milliseconds.
    pub round_trip_ms: u32,
    /// Git commit this RUNNING process was compiled from (deploy provenance, #194).
    /// Self-reported by the live process image — unlike re-exec'ing the on-disk
    /// binary at the process's path, this cannot be fooled by a rebuild that
    /// swapped the file under a still-running old core. `"unknown"` only when the
    /// server was built outside a git tree.
    pub build_sha: String,
    /// Auto-incrementing build number: the repo's commit count at compile time
    /// (Joel, 2026-08-08: "versions must always increment and display along with
    /// sha … stale binaries ruin you"). Monotonic per branch, so two nodes'
    /// builds can be ORDERED at a glance — "is this node stale?" becomes
    /// arithmetic instead of SHA archaeology. 0 only outside a git tree.
    #[ts(type = "number")]
    pub build_number: u32,
    /// UTC timestamp this binary was compiled (third leg of the version trio:
    /// number orders SOURCE, sha names it, built-at dates the BINARY — catching
    /// a rebuild of old source after a fix landed, which number+sha both miss).
    pub built_at: String,
}

/// `ping` — the canonical self-routing command. As an [`ActionCommand`] it gets
/// `CommandSpec` (Bare wire), `CommandHandler`, AND `DynCommand` from the blanket
/// impls — so this ONE type + a `run` body is the whole command. It is STATELESS,
/// so `register_stateless_command!` puts it on the kernel's typed object map with
/// ZERO host-module ceremony (no `handle_command` arm, no `commands()` override).
#[derive(Default)]
pub struct PingCommand;

#[async_trait]
impl ActionCommand for PingCommand {
    const NAME: &'static str = "ping";
    const DESCRIPTION: &'static str =
        "Health check: confirm the substrate is alive and responding. Returns a pong.";
    type Params = PingParams;
    type Output = PingResult;

    async fn run(&self, _ctx: &Ctx, _p: PingParams) -> Result<PingResult, CommandError> {
        Ok(PingResult {
            ok: true,
            round_trip_ms: 0,
            build_sha: env!("CONTINUUM_BUILD_GIT_SHA").to_string(),
            build_number: env!("CONTINUUM_BUILD_NUMBER").parse().unwrap_or(0),
            built_at: env!("CONTINUUM_BUILD_AT").to_string(),
        })
    }
}
crate::register_stateless_command!(PingCommand);

pub struct HealthModule {
    started_at: Instant,
}

impl Default for HealthModule {
    fn default() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }
}

impl HealthModule {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ServiceModule for HealthModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "health",
            priority: ModulePriority::Normal,
            // `ping` is NOT here — it's a STATELESS self-routing command
            // (`register_stateless_command!`), live on the typed object map with no
            // host-module ceremony. Only the un-migrated `health-`/`get-` verbs
            // still prefix-route to `handle_command`.
            command_prefixes: &["health-", "get-"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    /// Contribute the dep-holding `health-check` verb over this module's live boot
    /// `Instant` (`get-stats` self-registers statelessly; `ping` likewise). All three
    /// liveness verbs now live on the typed object map.
    fn commands(&self) -> Vec<std::sync::Arc<dyn crate::sdk_codegen::DynCommand>> {
        crate::commands::health::command_objects(self.started_at)
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // Both verbs are migrated to the typed registry (commands/health/). They
        // route via `route_object` — `health-check` against THIS module's boot
        // instant (contributed by `commands()`), `get-stats` as a stateless command.
        // Reaching this arm means the typed path failed to register; fail loud
        // naming the migration rather than silently re-handling.
        match command {
            "health-check" | "get-stats" => Err(format!(
                "'{command}' is migrated to the typed registry (commands/health/) — \
                 it must route via route_object, not the legacy handle_command path"
            )),

            _ => Err(format!("Unknown health command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: health-check + get-stats are migrated to the typed registry
    // (#62), so the legacy handle_command arms must FAIL LOUD naming the migration —
    // never silently re-handle. A regression that re-adds an inline handler (forking
    // liveness away from the typed command) is caught here.
    #[tokio::test]
    async fn migrated_arms_fail_loud() {
        let module = HealthModule::new();
        for command in ["health-check", "get-stats"] {
            let err = module
                .handle_command(command, Value::Null)
                .await
                .expect_err("migrated arm must fail loud");
            assert!(err.contains("migrated"), "got {err}");
            assert!(err.contains(command), "got {err}");
        }
    }

    // what this catches: the module contributes the dep-holding `health-check` verb
    // (bound to its live boot instant) to the kernel object map. A regression that
    // drops the `commands()` override — leaving the persona surface without the
    // uptime probe — is caught.
    #[test]
    fn contributes_the_typed_health_check_command() {
        let module = HealthModule::new();
        let names: Vec<&str> = module.commands().iter().map(|c| c.name()).collect();
        assert!(names.contains(&"health-check"), "got {names:?}");
    }

    // what this catches: `ping` migrated to the TYPED PATH end-to-end — it is NOT
    // in the prefix table (so the legacy handle_command match can't serve it), and
    // it IS in the registry's DynCommand object map, where it invokes to the bare
    // PingResult (no envelope). This is the proof that a base-trait command
    // (ActionCommand ⟹ DynCommand) self-routes through the kernel registry with no
    // per-module match arm. Regression here = the typed path silently falling back
    // to prefix routing (or ping vanishing entirely).
    #[tokio::test]
    async fn ping_routes_through_the_typed_object_map() {
        use crate::runtime::ModuleRegistry;
        use std::sync::Arc;
        let registry = ModuleRegistry::new();
        registry.register(Arc::new(HealthModule::new()));

        // Not prefix-routed any more — only the typed object map serves it.
        assert!(
            registry.route_command("ping").is_none(),
            "ping must NOT be on the prefix table (it's a DynCommand object)"
        );
        assert!(
            registry.list_command_objects().contains(&"ping"),
            "ping is registered as a self-routing command object"
        );

        let cmd = registry
            .route_object("ping")
            .expect("ping resolves through the typed object map");
        let cr = cmd
            .invoke(serde_json::json!({}), None)
            .await
            .expect("ping invoke ok");
        match cr {
            CommandResult::Json(v) => {
                assert_eq!(v["ok"], true);
                assert!(v.get("success").is_none(), "Bare wire — no envelope");
                // what this catches (#194): ping is the deploy-provenance surface —
                // the running core self-reports its compiled-in git SHA so
                // `continuum reboot` can verify the swap actually shipped fresh code.
                // A regression that drops buildSha turns reboot receipts back into lies.
                let sha = v["buildSha"].as_str().expect("buildSha is a string");
                assert!(!sha.is_empty(), "buildSha must never be empty");
            }
            other => panic!("expected Json, got {other:?}"),
        }
    }
}
