//! LaunchModeModule — owns the shared [`LaunchModeState`] (the message bus) and
//! contributes the `system/launch-mode/{get,set}` commands as typed, self-routing
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s via
//! [`commands`](LaunchModeModule::commands).
//!
//! The launch preference (`headless` | `ui` | `auto`) lives in ONE place:
//! `CONTINUUM_LAUNCH_MODE` in `~/.continuum/config.env` — the same key the
//! pre-core `bin/continuum` bootstrap resolver reads/writes, so boot-time and
//! runtime agree on one truth. These commands are the RUNTIME lever: any surface
//! (CLI, menu-bar tray, desktop Grid tab, mobile, and personas via the SDK) sets
//! the mode through `system/launch-mode/set`, and a `system:launch-mode:changed`
//! event lets a running UI attach or tear down its own overlay (the "UI turns
//! itself off when the last human leaves" behavior).
//!
//! Headless-native by design: continuum-core owns this with ZERO Node dependency
//! — the Node desktop is a dependent client that calls it over IPC, never the
//! reverse. That's why it lives in the Rust core and reuses [`crate::config_env`]
//! (the in-core config.env reader/writer) rather than the TypeScript
//! `SecretManager`.
//!
//! This is DISTINCT from the substrate boot `--mode`
//! (full-citizen/inference-only/fail-fast), which stays explicit and is never
//! guessed (`[[no-fallbacks-ever]]`). `auto` resolution into a concrete
//! `headless`/`ui` is the shell's boot-time job (`bin/continuum has_display`),
//! kept in one place — these commands report/persist the stored setting.
//!
//! `handle_command` survives only as a fail-loud safety net (the trait still
//! requires it) until Registry A is retired wholesale (Wave Z, #63).

use crate::runtime::{
    CommandResult, MessageBus, ModuleConfig, ModuleContext, ModulePriority, ServiceModule,
};
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;
use std::sync::OnceLock;

/// config.env key — MUST match `bin/continuum` and the TS `SecretManager`.
pub(crate) const LAUNCH_MODE_KEY: &str = "CONTINUUM_LAUNCH_MODE";
/// Bus topic a running UI subscribes to so it can attach/tear down its overlay.
pub(crate) const CHANGED_EVENT: &str = "system:launch-mode:changed";

/// Canonicalize a raw setting to one of the three valid modes. `None` for anything
/// else — the caller decides whether that's a default (`get`) or a hard error
/// (`set`). Shared by both commands so the deny-by-default rule lives in ONE place.
pub(crate) fn normalize_mode(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "headless" => Some("headless"),
        "ui" => Some("ui"),
        "auto" => Some("auto"),
        _ => None,
    }
}

/// Shared state for the launch-mode commands: the message bus, captured at
/// `initialize` so `set` can emit the change event. Commands are harvested at
/// `register` (before the bus exists), so the bus is filled later via a
/// `OnceLock` and read at run time — the deferred-bus pattern
/// [`DataState`](crate::modules::data::DataState) uses.
pub struct LaunchModeState {
    bus: OnceLock<Arc<MessageBus>>,
}

impl LaunchModeState {
    pub fn new() -> Self {
        Self {
            bus: OnceLock::new(),
        }
    }

    /// Emit `system:launch-mode:changed` so a running UI attaches/tears down its
    /// overlay. No-ops before `initialize` fills the bus (a command can't run that
    /// early anyway) — never a panic on a missing dep.
    pub fn publish_changed(&self, mode: &str, previous: &str) {
        if let Some(bus) = self.bus.get() {
            bus.publish_async_only(
                CHANGED_EVENT,
                serde_json::json!({ "mode": mode, "previousMode": previous }),
            );
        }
    }
}

impl Default for LaunchModeState {
    fn default() -> Self {
        Self::new()
    }
}

/// `ServiceModule` shell over the shared [`LaunchModeState`]. The kernel registers
/// this; the typed `system/launch-mode/*` commands capture `self.state.clone()`
/// via [`LaunchModeModule::commands`].
pub struct LaunchModeModule {
    state: Arc<LaunchModeState>,
}

impl LaunchModeModule {
    pub fn new() -> Self {
        Self {
            state: Arc::new(LaunchModeState::new()),
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
        // Fill the deferred bus so `set` can publish the changed-event at run time.
        let _ = self.state.bus.set(ctx.bus.clone());
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // Both verbs are migrated typed `ActionCommand`s that route via `route_object`
        // (`get` stateless, `set` dep-holding over this module's bus — see
        // `crate::commands::system::launch_mode`). Reaching this legacy path means a
        // descriptor failed to register — fail loud naming the command rather than
        // silently re-handling. (Retired wholesale when Registry A's trait default
        // becomes fail-loud — #63.)
        Err(format!(
            "'{command}' is a migrated, typed launch-mode command \
             (system/launch-mode/{{get,set}}) — it must route via the object registry \
             (route_object), not the legacy handle_command path. Reaching here means \
             its descriptor failed to register."
        ))
    }

    /// The migrated launch-mode commands as typed self-routing objects on the ONE
    /// registry. `set` is dep-holding (captures the bus); `get` is stateless and
    /// self-registers, so it isn't listed here. Their `CommandSpec` descriptors flow
    /// into `command_registry()` → the persona tool surface + grid ACL.
    fn commands(&self) -> Vec<Arc<dyn crate::sdk_codegen::DynCommand>> {
        crate::commands::system::launch_mode::command_objects(self.state.clone())
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
        assert_eq!(normalize_mode("headless"), Some("headless"));
        assert_eq!(normalize_mode("UI"), Some("ui"));
        assert_eq!(normalize_mode("  Auto "), Some("auto"));
        assert_eq!(normalize_mode("banana"), None);
        assert_eq!(normalize_mode(""), None);
    }

    /// What this catches: the command surface is stable — the module only claims the
    /// system/launch-mode/ prefix (so it can't shadow other system commands).
    #[test]
    fn config_claims_only_launch_mode_prefix() {
        let m = LaunchModeModule::new();
        assert_eq!(m.config().command_prefixes, &["system/launch-mode/"]);
    }

    /// What this catches: the legacy path is retired — every launch-mode verb now
    /// fails loud naming the command, never silently re-handles on the old path. A
    /// regression that re-adds an inline arm (forking a command off the typed object)
    /// is caught here.
    #[tokio::test]
    async fn legacy_arms_fail_loud() {
        let m = LaunchModeModule::new();
        for command in ["system/launch-mode/get", "system/launch-mode/set"] {
            let err = m
                .handle_command(command, Value::Null)
                .await
                .expect_err("legacy launch-mode arm must fail loud");
            assert!(err.contains("migrated"), "got {err}");
            assert!(err.contains(command), "got {err}");
        }
    }
}
