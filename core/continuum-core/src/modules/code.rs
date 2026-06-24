//! CodeModule — wraps file operations, git operations, and shell sessions.
//!
//! Handles: code/create-workspace, code/read, code/write, code/edit, code/delete,
//!          code/diff, code/undo, code/history, code/search, code/tree,
//!          code/git-status, code/git-diff, code/git-log, code/git-add, code/git-commit, code/git-push,
//!          code/shell-create, code/shell-execute, code/shell-poll, code/shell-kill,
//!          code/shell-cd, code/shell-status, code/shell-watch, code/shell-sentinel, code/shell-destroy
//!
//! Priority: Normal — code operations are important but not time-critical.

use crate::code::{self, FileEngine, PathSecurity, ShellSession};
use crate::log_info;
use crate::logging::TimingGuard;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::utils::params::Params;
use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;
use uuid::Uuid;

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
    /// Message bus for publishing shell events (set during initialize)
    bus: std::sync::OnceLock<Arc<crate::runtime::MessageBus>>,
}

impl CodeModule {
    pub fn new(state: Arc<CodeState>) -> Self {
        Self {
            state,
            bus: std::sync::OnceLock::new(),
        }
    }

    /// Publish a shell event to the message bus.
    /// Events: shell:{persona_id}:output, shell:{persona_id}:error, shell:{persona_id}:complete
    fn publish_shell_event(&self, persona_id: &str, event_type: &str, payload: serde_json::Value) {
        if let Some(bus) = self.bus.get() {
            let event_name = format!("shell:{}:{}", persona_id, event_type);
            bus.publish_async_only(&event_name, payload);
        }
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

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        let _ = self.bus.set(ctx.bus.clone());
        log_info!("module", "code", "CodeModule initialized with event bus");
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        let p = Params::new(&params);

        match command {
            // ================================================================
            // File Operations
            // ================================================================
            "code/create-workspace" => {
                let _timer = TimingGuard::new("module", "code_create_workspace");
                let persona_id = p.str("persona_id")?;
                let workspace_root = p.str("workspace_root")?;
                let read_roots: Vec<String> = p.json_or("read_roots");

                let root = std::path::Path::new(workspace_root);
                let mut security =
                    PathSecurity::new(root).map_err(|e| format!("Invalid workspace: {}", e))?;

                for rr in &read_roots {
                    security
                        .add_read_root(std::path::Path::new(rr))
                        .map_err(|e| format!("Invalid read root '{}': {}", rr, e))?;
                }

                let engine = FileEngine::new(persona_id, security);
                self.state
                    .file_engines
                    .insert(persona_id.to_string(), engine);

                log_info!(
                    "module",
                    "code",
                    "Created workspace for {} at {} with {} read roots: {:?}",
                    persona_id,
                    workspace_root,
                    read_roots.len(),
                    read_roots
                );
                Ok(CommandResult::Json(serde_json::json!({ "created": true })))
            }

            // code/read, code/write, code/edit are migrated to typed ActionCommands
            // (modules/code_commands.rs) and route via the object map — caller-scoped
            // identity, real param schema, one registry. No legacy arm here.
            "code/delete" => {
                let _timer = TimingGuard::new("module", "code_delete");
                let persona_id = p.str("persona_id")?;
                let file_path = p.str("file_path")?;
                let description = p.str_opt("description");

                let engine = self
                    .state
                    .file_engines
                    .get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                let result = engine
                    .delete(file_path, description)
                    .map_err(|e| e.to_string())?;
                log_info!("module", "code", "Delete {} by {}", file_path, persona_id);
                Ok(CommandResult::Json(
                    serde_json::to_value(&result).unwrap_or_default(),
                ))
            }

            "code/diff" => {
                let _timer = TimingGuard::new("module", "code_diff");
                let persona_id = p.str("persona_id")?;
                let file_path = p.str("file_path")?;
                let edit: crate::code::EditMode = p.json("edit_mode")?;

                let engine = self
                    .state
                    .file_engines
                    .get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                let result = engine
                    .preview_diff(file_path, &edit)
                    .map_err(|e| e.to_string())?;
                Ok(CommandResult::Json(
                    serde_json::to_value(&result).unwrap_or_default(),
                ))
            }

            "code/undo" => {
                let _timer = TimingGuard::new("module", "code_undo");
                let persona_id = p.str("persona_id")?;
                let change_id = p.str_opt("change_id");
                let count = p.u64_opt("count").map(|n| n as usize);

                let engine = self
                    .state
                    .file_engines
                    .get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                if let Some(id_str) = change_id {
                    let change_uuid =
                        Uuid::parse_str(id_str).map_err(|e| format!("Invalid change_id: {}", e))?;
                    let result = engine.undo(&change_uuid).map_err(|e| e.to_string())?;
                    log_info!("module", "code", "Undo {} by {}", id_str, persona_id);
                    Ok(CommandResult::Json(serde_json::json!({
                        "success": true,
                        "changes_undone": [serde_json::to_value(&result).unwrap_or_default()],
                        "error": null
                    })))
                } else {
                    let n = count.unwrap_or(1);
                    let result = engine.undo_last(n).map_err(|e| e.to_string())?;
                    log_info!(
                        "module",
                        "code",
                        "Undo {} changes by {}",
                        result.changes_undone.len(),
                        persona_id
                    );
                    Ok(CommandResult::Json(
                        serde_json::to_value(&result).unwrap_or_default(),
                    ))
                }
            }

            "code/history" => {
                let _timer = TimingGuard::new("module", "code_history");
                let persona_id = p.str("persona_id")?;
                let file_path = p.str_opt("file_path");
                let limit = p.u64_or("limit", 50) as usize;

                let engine = self
                    .state
                    .file_engines
                    .get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                let result = if let Some(fp) = file_path {
                    engine.file_history(fp, limit)
                } else {
                    engine.workspace_history(limit)
                };
                Ok(CommandResult::Json(
                    serde_json::to_value(&result).unwrap_or_default(),
                ))
            }

            // code/search, code/tree, code/exists, code/list, code/glob are migrated
            // to typed ActionCommands (modules/code_commands.rs) and route via the
            // object map — caller-scoped identity, real param schema, one registry.

            // Git Operations migrated to typed ActionCommands — see
            // `crate::commands::code::git` (the `code/git/<verb>` family is now
            // descriptor-advertised + routed on the O(1) typed path, one command per
            // file with the wire name mirroring the path). The legacy string arms are
            // deleted; identity is the authenticated caller, never a spoofable
            // `persona_id` param.

            // ================================================================
            // Shell Sessions
            // ================================================================
            "code/shell-create" => {
                let _timer = TimingGuard::new("module", "code_shell_create");
                let persona_id = p.str("persona_id")?;
                let workspace_root = p.str("workspace_root")?;

                let session_id = Uuid::new_v4().to_string();
                let shell = ShellSession::new(
                    &session_id,
                    persona_id,
                    std::path::Path::new(workspace_root),
                )
                .map_err(|e| format!("Failed to create shell: {}", e))?;

                let shell_id = shell.id().to_string();
                self.state
                    .shell_sessions
                    .insert(persona_id.to_string(), shell);

                log_info!(
                    "module",
                    "code",
                    "Created shell {} for {} at {}",
                    &shell_id[..8],
                    persona_id,
                    workspace_root
                );
                Ok(CommandResult::Json(serde_json::json!({
                    "created": true,
                    "session_id": shell_id,
                })))
            }

            "code/shell-execute" => {
                let _timer = TimingGuard::new("module", "code_shell_execute");
                let persona_id = p.str("persona_id")?;
                let cmd = p.str("cmd")?;
                let timeout_ms = p.u64_opt("timeout_ms");
                let wait = p.bool_or("wait", false);

                let (execution_id, state_arc) = {
                    let mut shell = self
                        .state
                        .shell_sessions
                        .get_mut(persona_id)
                        .ok_or_else(|| format!("No shell session for {}", persona_id))?;

                    let exec_id = shell
                        .execute(cmd, timeout_ms, &self.state.rt_handle)
                        .map_err(|e| e.to_string())?;
                    let state = shell
                        .get_execution_state(&exec_id)
                        .ok_or_else(|| "Execution vanished".to_string())?;
                    (exec_id, state)
                };

                if wait {
                    let result = loop {
                        let (is_done, response, notify) = {
                            let s = state_arc
                                .lock()
                                .map_err(|e| format!("Lock poisoned: {e}"))?;
                            if s.status != crate::code::shell_types::ShellExecutionStatus::Running {
                                let resp = crate::code::shell_types::ShellExecuteResponse {
                                    execution_id: s.id.clone(),
                                    status: s.status.clone(),
                                    stdout: Some(s.stdout_lines.join("\n")),
                                    stderr: Some(s.stderr_lines.join("\n")),
                                    exit_code: s.exit_code,
                                };
                                (true, Some(resp), None)
                            } else {
                                (false, None, Some(s.output_notify.clone()))
                            }
                        };

                        if let (true, Some(resp)) = (is_done, response) {
                            break resp;
                        }
                        if let Some(n) = notify {
                            n.notified().await;
                        }
                    };

                    let exit_code = result.exit_code.unwrap_or(-1);
                    let has_error = exit_code != 0;
                    self.publish_shell_event(persona_id, "complete", serde_json::json!({
                        "execution_id": result.execution_id,
                        "command": cmd,
                        "exit_code": exit_code,
                        "success": !has_error,
                        "stdout_lines": result.stdout.as_ref().map(|s| s.lines().count()).unwrap_or(0),
                        "stderr_lines": result.stderr.as_ref().map(|s| s.lines().count()).unwrap_or(0),
                        "has_error": has_error,
                    }));

                    if has_error {
                        if let Some(stderr) = &result.stderr {
                            let error_preview: String =
                                stderr.lines().take(5).collect::<Vec<_>>().join("\n");
                            self.publish_shell_event(
                                persona_id,
                                "error",
                                serde_json::json!({
                                    "execution_id": result.execution_id,
                                    "command": cmd,
                                    "exit_code": exit_code,
                                    "error_preview": error_preview,
                                }),
                            );
                        }
                    }

                    Ok(CommandResult::Json(
                        serde_json::to_value(&result).unwrap_or_default(),
                    ))
                } else {
                    self.publish_shell_event(
                        persona_id,
                        "started",
                        serde_json::json!({
                            "execution_id": execution_id,
                            "command": cmd,
                        }),
                    );
                    Ok(CommandResult::Json(serde_json::json!({
                        "execution_id": execution_id,
                        "started": true,
                    })))
                }
            }

            "code/shell-poll" => {
                let _timer = TimingGuard::new("module", "code_shell_poll");
                let persona_id = p.str("persona_id")?;
                let execution_id = p.str("execution_id")?;

                let shell = self
                    .state
                    .shell_sessions
                    .get(persona_id)
                    .ok_or_else(|| format!("No shell session for {}", persona_id))?;

                let result = shell.poll(execution_id).map_err(|e| e.to_string())?;
                Ok(CommandResult::Json(
                    serde_json::to_value(&result).unwrap_or_default(),
                ))
            }

            "code/shell-kill" => {
                let _timer = TimingGuard::new("module", "code_shell_kill");
                let persona_id = p.str("persona_id")?;
                let execution_id = p.str("execution_id")?;

                let shell = self
                    .state
                    .shell_sessions
                    .get(persona_id)
                    .ok_or_else(|| format!("No shell session for {}", persona_id))?;

                shell.kill(execution_id).map_err(|e| e.to_string())?;
                Ok(CommandResult::Json(serde_json::json!({ "killed": true })))
            }

            "code/shell-cd" => {
                let _timer = TimingGuard::new("module", "code_shell_cd");
                let persona_id = p.str("persona_id")?;
                let path = p.str("path")?;

                let mut shell = self
                    .state
                    .shell_sessions
                    .get_mut(persona_id)
                    .ok_or_else(|| format!("No shell session for {}", persona_id))?;

                let new_cwd = shell.cd(path).map_err(|e| e.to_string())?;
                Ok(CommandResult::Json(
                    serde_json::json!({ "changed": true, "cwd": new_cwd }),
                ))
            }

            "code/shell-status" => {
                let _timer = TimingGuard::new("module", "code_shell_status");
                let persona_id = p.str("persona_id")?;

                let shell = self
                    .state
                    .shell_sessions
                    .get(persona_id)
                    .ok_or_else(|| format!("No shell session for {}", persona_id))?;

                let info = shell.info();
                Ok(CommandResult::Json(
                    serde_json::to_value(&info).unwrap_or_default(),
                ))
            }

            "code/shell-watch" => {
                let _timer = TimingGuard::new("module", "code_shell_watch");
                let persona_id = p.str("persona_id")?;
                let execution_id = p.str("execution_id")?;

                let (exec_state, notify) = {
                    let shell = self
                        .state
                        .shell_sessions
                        .get(persona_id)
                        .ok_or_else(|| format!("No shell session for {}", persona_id))?;
                    shell
                        .get_watch_handles(execution_id)
                        .map_err(|e| e.to_string())?
                };

                let exec_id = execution_id.to_string();
                let result = self
                    .state
                    .rt_handle
                    .block_on(async { code::watch_execution(&exec_id, exec_state, notify).await })
                    .map_err(|e| e.to_string())?;

                Ok(CommandResult::Json(
                    serde_json::to_value(&result).unwrap_or_default(),
                ))
            }

            "code/shell-sentinel" => {
                let _timer = TimingGuard::new("module", "code_shell_sentinel");
                let persona_id = p.str("persona_id")?;
                let execution_id = p.str("execution_id")?;
                let rules: Vec<code::shell_types::SentinelRule> = p.json_or("rules");

                let shell = self
                    .state
                    .shell_sessions
                    .get(persona_id)
                    .ok_or_else(|| format!("No shell session for {}", persona_id))?;

                let count = shell
                    .set_sentinel(execution_id, &rules)
                    .map_err(|e| e.to_string())?;
                Ok(CommandResult::Json(
                    serde_json::json!({ "rules_applied": count }),
                ))
            }

            "code/shell-destroy" => {
                let _timer = TimingGuard::new("module", "code_shell_destroy");
                let persona_id = p.str("persona_id")?;

                let removed = self.state.shell_sessions.remove(persona_id).is_some();
                log_info!("module", "code", "Destroyed shell for {}", persona_id);
                Ok(CommandResult::Json(
                    serde_json::json!({ "destroyed": removed }),
                ))
            }

            _ => Err(format!("Unknown code command: {command}")),
        }
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
