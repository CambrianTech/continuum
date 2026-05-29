//! `PersonaServiceModule` — singleton Rust `ServiceModule` for persona
//! work. **L0-1 minimum unit** of [GRID-MIGRATION-ROADMAP].
//!
//! ## Scope discipline
//!
//! L0-1 ships only what L0-1 needs: a registered module that responds
//! to `persona/status`. Enrollment, cognition dispatch, channel
//! ownership, and the circuit breaker all live with the layers that
//! wire them to real work (L0-2..L0-4), shipped alongside deletion of
//! their TS counterparts in the same PRs.
//!
//! No fallbacks here. Calling `persona/enroll` returns a loud error
//! until L0-2 wires cognition dispatch.

use std::any::Any;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::runtime::service_module::{CommandResult, ModuleConfig, ModulePriority, ServiceModule};
use crate::runtime::ModuleContext;

/// Singleton owning persona work in-process. Replaces the TS
/// `PersonaAutonomousLoop`; the deletion of `PersonaAutonomousLoop.ts`
/// lands with L0-2 once cognition dispatch is wired here.
pub struct PersonaServiceModule;

impl PersonaServiceModule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PersonaServiceModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for PersonaServiceModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "persona",
            priority: ModulePriority::High,
            command_prefixes: &["persona/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 1,
            tick_interval: Some(Duration::from_millis(250)),
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(
        &self,
        command: &str,
        _params: Value,
    ) -> Result<CommandResult, String> {
        match command {
            "persona/status" => Ok(CommandResult::Json(json!({
                "module": "persona",
                "enrolled": 0,
                "scope": "L0-1: status-only; enroll wired in L0-2",
            }))),
            "persona/enroll" => Err(
                "persona/enroll requires cognition dispatch (L0-2 — card 7a45a15f); \
                 not yet wired"
                    .to_string(),
            ),
            other => Err(format!("unknown persona command: {other}")),
        }
    }

    async fn tick(&self) -> Result<(), String> {
        // L0-1: no personas to service. L0-2 wires the per-persona
        // `channel_registry::service_cycle()` dispatch here.
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_declares_persona_prefix_and_high_priority() {
        let m = PersonaServiceModule::new();
        let cfg = m.config();
        assert_eq!(cfg.name, "persona");
        assert_eq!(cfg.priority, ModulePriority::High);
        assert_eq!(cfg.command_prefixes, &["persona/"]);
        assert_eq!(cfg.tick_interval, Some(Duration::from_millis(250)));
    }

    #[tokio::test]
    async fn status_command_succeeds_and_reports_l0_1_scope() {
        let m = PersonaServiceModule::new();
        let result = m
            .handle_command("persona/status", Value::Null)
            .await
            .expect("status succeeds");
        let CommandResult::Json(v) = result else {
            panic!("expected Json result")
        };
        assert_eq!(v["module"], "persona");
        assert_eq!(v["enrolled"], 0);
        assert!(v["scope"].as_str().unwrap().contains("L0-1"));
    }

    #[tokio::test]
    async fn enroll_command_fails_loud_until_l0_2_card_7a45a15f() {
        let m = PersonaServiceModule::new();
        let err = m
            .handle_command("persona/enroll", json!({"persona_id": "x"}))
            .await
            .expect_err("enroll must fail loud — no fallback semantics");
        assert!(
            err.contains("L0-2"),
            "error must name the gating layer; got: {err}"
        );
        assert!(
            err.contains("7a45a15f"),
            "error must name the gating card so it's grep-able; got: {err}"
        );
    }

    #[tokio::test]
    async fn unknown_command_returns_clear_error() {
        let m = PersonaServiceModule::new();
        let err = m
            .handle_command("persona/teleport", Value::Null)
            .await
            .expect_err("unknown commands must error, not fall back");
        assert!(err.contains("persona/teleport"), "error names the command");
    }

    #[tokio::test]
    async fn tick_succeeds_quietly_with_no_enrolled_personas() {
        let m = PersonaServiceModule::new();
        m.tick().await.expect("empty tick succeeds");
    }
}
