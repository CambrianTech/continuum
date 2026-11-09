//! `health-check` — substrate liveness with uptime + build version.
//!
//! Dep-holding: captures the owning [`HealthModule`](crate::modules::health)'s
//! boot [`Instant`] so it can report uptime. The module builds the runtime object
//! in its `commands()` with the live `started_at`; the descriptor self-publishes to
//! the registry.
//!
//! ## Wire contract (do NOT camelCase-rename)
//!
//! This is a PRE-EXISTING IPC contract — the TypeScript base client
//! (`bindings/modules/base.ts::healthCheck`) reads `result.healthy`, and the legacy
//! handler emitted snake_case `uptime_seconds`. The output keys are therefore
//! preserved verbatim (`healthy` / `uptime_seconds` / `version`) so the migration is
//! byte-identical on the wire. The IPC layer wraps this Bare output in the transport
//! `{success, result}` envelope uniformly, exactly as it did the legacy
//! `CommandResult::Json`.
//!
//! ## Gating
//!
//! `AiSafe` — pure liveness probe: no state mutation, no compute spend, no
//! credentials. A persona may legitimately check whether the substrate is alive.

use std::sync::Arc;
use std::time::Instant;

use crate::runtime::{ModuleReadiness, ModuleReadinessRow};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// `health-check` input — none.
#[derive(Debug, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/health/HealthCheckParams.ts"
)]
pub struct HealthCheckParams {}

/// `health-check` output — the substrate is alive, plus uptime and build version.
///
/// Field names are the legacy snake_case keys verbatim (see module docs) — NOT
/// camelCased — to keep the IPC wire contract byte-identical.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/health/HealthCheckReport.ts"
)]
pub struct HealthCheckReport {
    /// Always true on a successful response.
    pub healthy: bool,
    /// Seconds since the substrate booted.
    pub uptime_seconds: u64,
    /// The continuum-core build version (`CARGO_PKG_VERSION`).
    pub version: String,
    /// Each module's initialization state. The core answers before every module has
    /// initialized, so this is where a degraded module (initialize past its deadline, its
    /// commands refused) is read.
    pub modules: Vec<ModuleReadinessRow>,
}

crate::action_command! {
    /// Confirm the substrate is alive. Returns `healthy: true` along with how long
    /// the core has been up (seconds since boot), the build version, and each
    /// module's initialization state (initializing, ready, failed, or degraded with
    /// its reason). A pure liveness probe — spends no compute and mutates nothing.
    pub struct HealthCheck {
        started_at: Instant,
        readiness: Arc<ModuleReadiness>,
    }
    name: "health-check",
    access: AiSafe,
    params: HealthCheckParams,
    output: HealthCheckReport,
    run(this, _ctx, _p) => {
        Ok(HealthCheckReport {
            healthy: true,
            uptime_seconds: this.started_at.elapsed().as_secs(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            modules: this.readiness.report(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand, Ctx};

    // what this catches: name/access wiring — health-check is a read-only liveness
    // probe, so it lives on the AiSafe surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(HealthCheck::NAME, "health-check");
        assert!(matches!(HealthCheck::ACCESS, AccessLevel::AiSafe));
    }

    // what this catches: the migrated handler reports healthy + a non-decreasing
    // uptime + the build version, preserving the legacy JSON shape the TS base
    // client (base.ts::healthCheck reads result.healthy) depends on.
    #[tokio::test]
    async fn reports_alive_with_uptime_and_version() {
        let cmd = HealthCheck {
            started_at: Instant::now(),
            readiness: Arc::new(ModuleReadiness::new()),
        };
        let out = cmd
            .run(&Ctx::default(), HealthCheckParams {})
            .await
            .expect("health-check never errors");
        assert!(out.healthy);
        assert_eq!(out.version, env!("CARGO_PKG_VERSION"));

        // Serialized form keeps the legacy snake_case keys verbatim — a camelCase
        // drift here would break the TS IPC client's `result.healthy` read path and
        // any consumer of `uptime_seconds`.
        let json = serde_json::to_value(&out).unwrap();
        assert_eq!(json["healthy"], true);
        assert!(json["uptime_seconds"].is_number());
        assert!(json["version"].is_string());
        assert!(json["modules"].is_array());
    }

    // what this catches (M5 2026-11-07 19:11Z, card 1a566cd9): the core answering only after
    // every module initialized. With a module whose initialize never returns, `ping` must answer
    // on the socket's route BEFORE initialize settles, the settle must still come, and
    // `health-check` must name the wedged module degraded with its typed reason.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ping_and_health_answer_while_a_module_is_wedged_and_health_names_it() {
        use crate::runtime::runtime::fixtures::{InitBehavior, WedgeableInitModule};
        use crate::runtime::{InitDeadline, InitPlan, Runtime};
        let runtime = Runtime::new();
        runtime.register(Arc::new(crate::modules::health::HealthModule::new(
            runtime.registry().readiness(),
        )));
        runtime.register(Arc::new(WedgeableInitModule { name: "wedged", prefix: &["wedged/"], init: InitBehavior::Wedges }));
        let settle = runtime.begin_initialize(InitPlan::unrecorded(InitDeadline::fixture(100, "health")));

        let ping = runtime.route_command("ping", serde_json::json!({}), None).await;
        assert!(matches!(ping, Some(Ok(_))), "ping answers before initialize settles: {ping:?}");

        let settled = tokio::time::timeout(std::time::Duration::from_secs(5), settle)
            .await
            .expect("initialize settles although one module never returns");
        assert_eq!(settled.degraded, vec!["wedged"]);

        let report = match runtime.route_command("health-check", serde_json::json!({}), None).await {
            Some(Ok(result)) => result.to_json_value().expect("json"),
            other => panic!("health-check answers: {other:?}"),
        };
        let modules = report["modules"].as_array().expect("modules");
        let wedged = modules.iter().find(|m| m["module"] == "wedged").expect("wedged is reported");
        assert_eq!(wedged["state"]["state"], "degraded");
        assert_eq!(wedged["state"]["reason"]["kind"], "init_deadline");
        let health = modules.iter().find(|m| m["module"] == "health").expect("health is reported");
        assert_eq!(health["state"]["state"], "ready");
    }
}
