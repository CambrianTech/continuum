//! CodeModule — owns the shared [`CodeState`] (per-caller file engines + shell
//! sessions) and contributes every `code/*` command as a typed, self-routing
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand) via [`commands`](CodeModule::commands).
//!
//! There is **no legacy `code/*` arm left**: file ops, the shell session family
//! (`code/shell`, `code/shell-poll`, `code/shell-kill`), `code/create-workspace`,
//! and the `git`/`cargo` families all route on the ONE registry through
//! `route_object`, keyed on the authenticated caller (never a spoofable
//! `persona_id` param). `handle_command` survives only as a fail-loud safety net
//! (the trait still requires it) until Registry A is retired wholesale (Wave Z).
//!
//! Priority: Normal — code operations are important but not time-critical.

use crate::code::{FileEngine, ShellSession};
use crate::log_info;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;

/// Shared state for code module.
pub struct CodeState {
    /// Per-persona file engines — workspace-scoped file operations with change tracking.
    pub file_engines: Arc<DashMap<String, FileEngine>>,
    /// Per-persona shell sessions — persistent bash per workspace with handle+poll.
    pub shell_sessions: Arc<DashMap<String, ShellSession>>,
    /// Tokio runtime handle for spawning async shell execution tasks.
    pub rt_handle: tokio::runtime::Handle,
}

impl CodeState {
    pub fn new(
        file_engines: Arc<DashMap<String, FileEngine>>,
        shell_sessions: Arc<DashMap<String, ShellSession>>,
        rt_handle: tokio::runtime::Handle,
    ) -> Self {
        Self {
            file_engines,
            shell_sessions,
            rt_handle,
        }
    }
}

pub struct CodeModule {
    state: Arc<CodeState>,
}

impl CodeModule {
    pub fn new(state: Arc<CodeState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl ServiceModule for CodeModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "code",
            priority: ModulePriority::Normal,
            command_prefixes: &["code/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        log_info!("module", "code", "CodeModule initialized");
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // Every `code/*` command is now a typed `ActionCommand` that routes via
        // `route_object` (file ops + `create-workspace` in `code_commands.rs`, the
        // `code/shell*` session family there, and the `git`/`cargo` families under
        // `crate::commands::code`). They are keyed on the authenticated caller, never
        // a spoofable `persona_id` param. Reaching this legacy path at all means a
        // descriptor failed to register — fail loud naming the command rather than
        // silently re-handling it on a non-caller-scoped path. (This whole impl is
        // retired wholesale when Registry A's trait default becomes fail-loud — #63.)
        Err(format!(
            "'{command}' is a migrated, typed code command — it must route via the              object registry (route_object), not the legacy handle_command path.              Reaching here means its descriptor failed to register."
        ))
    }

    /// The migrated file-operation commands as typed self-routing objects on the
    /// ONE registry. The executor routes these names directly here (winning over
    /// the legacy prefix arm), and their `CommandSpec` descriptors flow into
    /// `command_registry()` → the persona tool surface + grid ACL. See
    /// [`crate::modules::code_commands`].
    fn commands(&self) -> Vec<Arc<dyn crate::sdk_codegen::DynCommand>> {
        let mut objs = crate::modules::code_commands::command_objects(self.state.clone());
        // The git family (`code/git/<verb>`), one command per file under
        // `crate::commands::code::git`.
        objs.extend(crate::commands::code::git::command_objects(
            self.state.clone(),
        ));
        // The GitHub-collaboration family (`code/github/<verb>`) — PRs, issues, comments:
        // the executor→teammate layer, wrapping `gh`.
        objs.extend(crate::commands::code::github::command_objects(
            self.state.clone(),
        ));
        // The cargo family (`code/cargo/<verb>`) — the persona's Rust hands.
        objs.extend(crate::commands::code::cargo::command_objects(
            self.state.clone(),
        ));
        objs
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::ServiceModule;

    fn module() -> CodeModule {
        let state = Arc::new(CodeState::new(
            Arc::new(DashMap::new()),
            Arc::new(DashMap::new()),
            tokio::runtime::Handle::current(),
        ));
        CodeModule::new(state)
    }

    // what this catches: EVERY code/* command (file ops, create-workspace, the
    // shell session family) is now a typed ActionCommand that routes via route_object
    // with caller-scoped identity. The legacy handle_command path no longer handles
    // anything — it must FAIL LOUD naming the command, never silently re-handle on the
    // old spoofable persona_id path. A regression that re-adds an inline arm (forking a
    // command away from the typed, identity-safe object) is caught here, across the
    // file, shell, and workspace surfaces that previously had live arms.
    #[tokio::test]
    async fn every_legacy_arm_fails_loud() {
        let module = module();
        for command in [
            // formerly-migrated file ops (kept as a regression anchor)
            "code/delete",
            "code/diff",
            "code/undo",
            "code/history",
            // this wave: shell session family + workspace, previously live arms
            "code/shell-execute",
            "code/shell-create",
            "code/shell-cd",
            "code/shell-status",
            "code/shell-watch",
            "code/shell-sentinel",
            "code/shell-destroy",
            "code/create-workspace",
        ] {
            let err = module
                .handle_command(command, Value::Null)
                .await
                .expect_err("legacy code arm must fail loud");
            assert!(err.contains("migrated"), "got {err}");
            assert!(err.contains(command), "got {err}");
        }
    }
}
