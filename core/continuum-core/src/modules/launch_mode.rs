//! LaunchModeModule — `system/launch-mode/{get,set}` IPC commands.
//!
//! The launch preference (`headless` | `ui` | `auto`) lives in ONE place:
//! `CONTINUUM_LAUNCH_MODE` in `~/.continuum/config.env` — the same key the
//! pre-core `bin/continuum` bootstrap resolver reads/writes, so boot-time and
//! runtime agree on one truth. This module is the RUNTIME lever: any surface
//! (CLI, menu-bar tray, desktop Grid tab, mobile, and personas via the SDK)
//! sets the mode through this command, and a `system:launch-mode:changed` event
//! lets a running UI attach or tear down its own overlay (the "UI turns itself
//! off when the last human leaves" behavior).
//!
//! Headless-native by design: continuum-core owns this with ZERO Node
//! dependency — the Node desktop is a dependent client that calls it over IPC,
//! never the reverse. That's why it lives in the Rust core and reuses
//! [`crate::config_env`] (the in-core config.env reader/writer) rather than the
//! TypeScript `SecretManager`.
//!
//! This is DISTINCT from the substrate boot `--mode`
//! (full-citizen/inference-only/fail-fast), which stays explicit and is never
//! guessed (`[[no-fallbacks-ever]]`). `auto` resolution into a concrete
//! `headless`/`ui` is the shell's boot-time job (`bin/continuum has_display`),
//! kept in one place — this command reports/persists the stored setting.

use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::any::Any;
use std::sync::Arc;
use std::sync::OnceLock;

/// config.env key — MUST match `bin/continuum` and the TS `SecretManager`.
const LAUNCH_MODE_KEY: &str = "CONTINUUM_LAUNCH_MODE";
/// Bus topic a running UI subscribes to so it can attach/tear down its overlay.
const CHANGED_EVENT: &str = "system:launch-mode:changed";

pub struct LaunchModeModule {
    /// Captured during `initialize` so `handle_command` (which has no ctx) can
    /// publish the changed-event. Same pattern as `CodeModule`.
    bus: OnceLock<Arc<crate::runtime::MessageBus>>,
}

impl LaunchModeModule {
    pub fn new() -> Self {
        Self {
            bus: OnceLock::new(),
        }
    }

    /// Canonicalize a raw setting to one of the three valid modes. `None` for
    /// anything else — the caller decides whether that's a default (`get`) or a
    /// hard error (`set`).
    fn normalize_mode(raw: &str) -> Option<&'static str> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "headless" => Some("headless"),
            "ui" => Some("ui"),
            "auto" => Some("auto"),
            _ => None,
        }
    }
}

impl Default for LaunchModeModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for LaunchModeModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "launch-mode",
            priority: ModulePriority::Normal,
            command_prefixes: &["system/launch-mode/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        let _ = self.bus.set(ctx.bus.clone());
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        match command {
            // Report the stored launch mode. Unset (or unrecognized) → "auto".
            "system/launch-mode/get" => {
                let (mode, source) = match crate::config_env::read(LAUNCH_MODE_KEY) {
                    Some(raw) => match Self::normalize_mode(&raw) {
                        Some(m) => (m.to_string(), "config"),
                        None => ("auto".to_string(), "default"),
                    },
                    None => ("auto".to_string(), "default"),
                };
                Ok(CommandResult::Json(json!({
                    "success": true,
                    "mode": mode,
                    "source": source,
                })))
            }

            // Persist a new launch mode + emit the changed event.
            "system/launch-mode/set" => {
                let raw = params
                    .get("mode")
                    .and_then(|v| v.as_str())
                    .ok_or("system/launch-mode/set requires a 'mode' string param (headless|ui|auto)")?;
                let mode = Self::normalize_mode(raw).ok_or_else(|| {
                    format!("invalid mode '{raw}'. Expected one of: headless, ui, auto")
                })?;

                let previous = crate::config_env::read(LAUNCH_MODE_KEY)
                    .and_then(|raw| Self::normalize_mode(&raw).map(str::to_string))
                    .unwrap_or_default();

                crate::config_env::upsert(LAUNCH_MODE_KEY, mode)?;

                // Fire-and-forget: a running UI tears down/attaches its overlay.
                if let Some(bus) = self.bus.get() {
                    bus.publish_async_only(
                        CHANGED_EVENT,
                        json!({ "mode": mode, "previousMode": previous }),
                    );
                }

                Ok(CommandResult::Json(json!({
                    "success": true,
                    "mode": mode,
                    "previousMode": previous,
                    "applied": true,
                })))
            }

            other => Err(format!("LaunchModeModule: unknown command '{other}'")),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: the three valid modes canonicalize (case-insensitively)
    /// and everything else is rejected — the deny-by-default validation the `set`
    /// path leans on so a bad value never reaches config.env.
    #[test]
    fn normalize_accepts_only_the_three_modes() {
        assert_eq!(LaunchModeModule::normalize_mode("headless"), Some("headless"));
        assert_eq!(LaunchModeModule::normalize_mode("UI"), Some("ui"));
        assert_eq!(LaunchModeModule::normalize_mode("  Auto "), Some("auto"));
        assert_eq!(LaunchModeModule::normalize_mode("banana"), None);
        assert_eq!(LaunchModeModule::normalize_mode(""), None);
    }

    /// What this catches: the command surface is stable — the module only claims
    /// the system/launch-mode/ prefix (so it can't shadow other system commands).
    #[test]
    fn config_claims_only_launch_mode_prefix() {
        let m = LaunchModeModule::new();
        assert_eq!(m.config().command_prefixes, &["system/launch-mode/"]);
    }

    /// What this catches: an unknown subcommand fails loudly rather than silently
    /// succeeding (every-error-is-an-opportunity-to-battle-harden).
    #[tokio::test]
    async fn unknown_command_errors() {
        let m = LaunchModeModule::new();
        let err = m
            .handle_command("system/launch-mode/frobnicate", json!({}))
            .await
            .expect_err("unknown subcommand must error");
        assert!(err.contains("unknown command"), "got: {err}");
    }

    /// What this catches: `set` with no/blank mode is rejected with a message
    /// naming the valid values — not a silent no-op write.
    #[tokio::test]
    async fn set_without_mode_is_rejected() {
        let m = LaunchModeModule::new();
        let err = m
            .handle_command("system/launch-mode/set", json!({}))
            .await
            .expect_err("missing mode must error");
        assert!(err.contains("mode"), "got: {err}");

        let err2 = m
            .handle_command("system/launch-mode/set", json!({ "mode": "sideways" }))
            .await
            .expect_err("invalid mode must error");
        assert!(err2.contains("headless"), "error should name valid modes: {err2}");
    }
}
