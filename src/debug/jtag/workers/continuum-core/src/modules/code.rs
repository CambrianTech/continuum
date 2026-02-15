//! CodeModule — wraps file operations, git operations, and shell sessions.
//!
//! Handles: code/create-workspace, code/read, code/write, code/edit, code/delete,
//!          code/diff, code/undo, code/history, code/search, code/tree,
//!          code/git-status, code/git-diff, code/git-log, code/git-add, code/git-commit, code/git-push,
//!          code/shell-create, code/shell-execute, code/shell-poll, code/shell-kill,
//!          code/shell-cd, code/shell-status, code/shell-watch, code/shell-sentinel, code/shell-destroy
//!
//! Priority: Normal — code operations are important but not time-critical.

use crate::runtime::{ServiceModule, ModuleConfig, ModulePriority, CommandResult, ModuleContext};
use crate::code::{self, FileEngine, PathSecurity, ShellSession};
use crate::code::{git_bridge, search, tree};
use crate::logging::TimingGuard;
use crate::utils::params::Params;
use crate::log_info;
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
        Self { file_engines, shell_sessions, rt_handle }
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
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        let _ = self.bus.set(ctx.bus.clone());
        log_info!("module", "code", "CodeModule initialized with event bus");
        Ok(())
    }

    async fn handle_command(
        &self,
        command: &str,
        params: Value,
    ) -> Result<CommandResult, String> {
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
                let mut security = PathSecurity::new(root)
                    .map_err(|e| format!("Invalid workspace: {}", e))?;

                for rr in &read_roots {
                    security.add_read_root(std::path::Path::new(rr))
                        .map_err(|e| format!("Invalid read root '{}': {}", rr, e))?;
                }

                let engine = FileEngine::new(persona_id, security);
                self.state.file_engines.insert(persona_id.to_string(), engine);

                log_info!("module", "code", "Created workspace for {} at {} with {} read roots: {:?}",
                    persona_id, workspace_root, read_roots.len(), read_roots);
                Ok(CommandResult::Json(serde_json::json!({ "created": true })))
            }

            "code/read" => {
                let _timer = TimingGuard::new("module", "code_read");
                let persona_id = p.str("persona_id")?;
                let file_path = p.str("file_path")?;
                let start_line = p.u32_opt("start_line");
                let end_line = p.u32_opt("end_line");

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                let result = engine.read(file_path, start_line, end_line)
                    .map_err(|e| format!("{}", e))?;
                Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or_default()))
            }

            "code/write" => {
                let _timer = TimingGuard::new("module", "code_write");
                let persona_id = p.str("persona_id")?;
                let file_path = p.str("file_path")?;
                let content = p.str("content")?;
                let description = p.str_opt("description");

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                let result = engine.write(file_path, content, description)
                    .map_err(|e| format!("{}", e))?;
                log_info!("module", "code", "Write {} ({} bytes) by {}", file_path, result.bytes_written, persona_id);
                Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or_default()))
            }

            "code/edit" => {
                let _timer = TimingGuard::new("module", "code_edit");
                let persona_id = p.str("persona_id")?;
                let file_path = p.str("file_path")?;
                let edit: crate::code::EditMode = p.json("edit_mode")?;
                let description = p.str_opt("description");

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                let result = engine.edit(file_path, &edit, description)
                    .map_err(|e| format!("{}", e))?;
                log_info!("module", "code", "Edit {} by {}", file_path, persona_id);
                Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or_default()))
            }

            "code/delete" => {
                let _timer = TimingGuard::new("module", "code_delete");
                let persona_id = p.str("persona_id")?;
                let file_path = p.str("file_path")?;
                let description = p.str_opt("description");

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                let result = engine.delete(file_path, description)
                    .map_err(|e| format!("{}", e))?;
                log_info!("module", "code", "Delete {} by {}", file_path, persona_id);
                Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or_default()))
            }

            "code/diff" => {
                let _timer = TimingGuard::new("module", "code_diff");
                let persona_id = p.str("persona_id")?;
                let file_path = p.str("file_path")?;
                let edit: crate::code::EditMode = p.json("edit_mode")?;

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                let result = engine.preview_diff(file_path, &edit)
                    .map_err(|e| format!("{}", e))?;
                Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or_default()))
            }

            "code/undo" => {
                let _timer = TimingGuard::new("module", "code_undo");
                let persona_id = p.str("persona_id")?;
                let change_id = p.str_opt("change_id");
                let count = p.u64_opt("count").map(|n| n as usize);

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                if let Some(id_str) = change_id {
                    let change_uuid = Uuid::parse_str(id_str)
                        .map_err(|e| format!("Invalid change_id: {}", e))?;
                    let result = engine.undo(&change_uuid)
                        .map_err(|e| format!("{}", e))?;
                    log_info!("module", "code", "Undo {} by {}", id_str, persona_id);
                    Ok(CommandResult::Json(serde_json::json!({
                        "success": true,
                        "changes_undone": [serde_json::to_value(&result).unwrap_or_default()],
                        "error": null
                    })))
                } else {
                    let n = count.unwrap_or(1);
                    let result = engine.undo_last(n)
                        .map_err(|e| format!("{}", e))?;
                    log_info!("module", "code", "Undo {} changes by {}", result.changes_undone.len(), persona_id);
                    Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or_default()))
                }
            }

            "code/history" => {
                let _timer = TimingGuard::new("module", "code_history");
                let persona_id = p.str("persona_id")?;
                let file_path = p.str_opt("file_path");
                let limit = p.u64_or("limit", 50) as usize;

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                let result = if let Some(fp) = file_path {
                    engine.file_history(fp, limit)
                } else {
                    engine.workspace_history(limit)
                };
                Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or_default()))
            }

            "code/search" => {
                let _timer = TimingGuard::new("module", "code_search");
                let persona_id = p.str("persona_id")?;
                let pattern = p.str("pattern")?;
                let file_glob = p.str_opt("file_glob");
                let max_results = p.u64_or("max_results", 100) as u32;

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                let result = search::search_files(
                    &engine.workspace_root(),
                    pattern,
                    file_glob,
                    max_results,
                );
                Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or_default()))
            }

            "code/tree" => {
                let _timer = TimingGuard::new("module", "code_tree");
                let persona_id = p.str("persona_id")?;
                let path = p.str_opt("path");
                let max_depth = p.u64_or("max_depth", 10) as u32;
                let include_hidden = p.bool_or("include_hidden", false);

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                let root = engine.workspace_root();
                let target = path.map(|p| root.join(p)).unwrap_or_else(|| root.to_path_buf());
                let result = tree::generate_tree(&target, max_depth, include_hidden);
                Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or_default()))
            }

            // ================================================================
            // Git Operations
            // ================================================================

            "code/git-status" => {
                let _timer = TimingGuard::new("module", "code_git_status");
                let persona_id = p.str("persona_id")?;

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                let result = git_bridge::git_status(&engine.workspace_root());
                Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or_default()))
            }

            "code/git-diff" => {
                let _timer = TimingGuard::new("module", "code_git_diff");
                let persona_id = p.str("persona_id")?;
                let staged = p.bool_or("staged", false);

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                let diff = git_bridge::git_diff(&engine.workspace_root(), staged)?;
                Ok(CommandResult::Json(serde_json::json!({ "diff": diff })))
            }

            "code/git-log" => {
                let _timer = TimingGuard::new("module", "code_git_log");
                let persona_id = p.str("persona_id")?;
                let count = p.u64_or("limit", 10) as u32;

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                let log = git_bridge::git_log(&engine.workspace_root(), count)?;
                Ok(CommandResult::Json(serde_json::json!({ "log": log })))
            }

            "code/git-add" => {
                let _timer = TimingGuard::new("module", "code_git_add");
                let persona_id = p.str("persona_id")?;
                let paths: Vec<String> = p.json_or("paths");

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                let path_refs: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
                let output = git_bridge::git_add(&engine.workspace_root(), &path_refs)?;
                Ok(CommandResult::Json(serde_json::json!({ "output": output })))
            }

            "code/git-commit" => {
                let _timer = TimingGuard::new("module", "code_git_commit");
                let persona_id = p.str("persona_id")?;
                let message = p.str("message")?;

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                let hash = git_bridge::git_commit(&engine.workspace_root(), message)?;
                log_info!("module", "code", "Git commit by {}: {}", persona_id, message);
                Ok(CommandResult::Json(serde_json::json!({ "hash": hash })))
            }

            "code/git-push" => {
                let _timer = TimingGuard::new("module", "code_git_push");
                let persona_id = p.str("persona_id")?;
                let remote = p.str_or("remote", "");
                let branch = p.str_or("branch", "");

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                let output = git_bridge::git_push(&engine.workspace_root(), remote, branch)?;
                log_info!("module", "code", "Git push by {}", persona_id);
                Ok(CommandResult::Json(serde_json::json!({ "output": output })))
            }

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
                ).map_err(|e| format!("Failed to create shell: {}", e))?;

                let shell_id = shell.id().to_string();
                self.state.shell_sessions.insert(persona_id.to_string(), shell);

                log_info!("module", "code", "Created shell {} for {} at {}", &shell_id[..8], persona_id, workspace_root);
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
                    let mut shell = self.state.shell_sessions.get_mut(persona_id)
                        .ok_or_else(|| format!("No shell session for {}", persona_id))?;

                    let exec_id = shell.execute(cmd, timeout_ms, &self.state.rt_handle)
                        .map_err(|e| format!("{}", e))?;
                    let state = shell.get_execution_state(&exec_id)
                        .ok_or_else(|| "Execution vanished".to_string())?;
                    (exec_id, state)
                };

                if wait {
                    let result = loop {
                        let (is_done, response, notify) = {
                            let s = state_arc.lock()
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
                            let error_preview: String = stderr.lines().take(5).collect::<Vec<_>>().join("\n");
                            self.publish_shell_event(persona_id, "error", serde_json::json!({
                                "execution_id": result.execution_id,
                                "command": cmd,
                                "exit_code": exit_code,
                                "error_preview": error_preview,
                            }));
                        }
                    }

                    Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or_default()))
                } else {
                    self.publish_shell_event(persona_id, "started", serde_json::json!({
                        "execution_id": execution_id,
                        "command": cmd,
                    }));
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

                let shell = self.state.shell_sessions.get(persona_id)
                    .ok_or_else(|| format!("No shell session for {}", persona_id))?;

                let result = shell.poll(execution_id).map_err(|e| format!("{}", e))?;
                Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or_default()))
            }

            "code/shell-kill" => {
                let _timer = TimingGuard::new("module", "code_shell_kill");
                let persona_id = p.str("persona_id")?;
                let execution_id = p.str("execution_id")?;

                let shell = self.state.shell_sessions.get(persona_id)
                    .ok_or_else(|| format!("No shell session for {}", persona_id))?;

                shell.kill(execution_id).map_err(|e| format!("{}", e))?;
                Ok(CommandResult::Json(serde_json::json!({ "killed": true })))
            }

            "code/shell-cd" => {
                let _timer = TimingGuard::new("module", "code_shell_cd");
                let persona_id = p.str("persona_id")?;
                let path = p.str("path")?;

                let mut shell = self.state.shell_sessions.get_mut(persona_id)
                    .ok_or_else(|| format!("No shell session for {}", persona_id))?;

                let new_cwd = shell.cd(path).map_err(|e| format!("{}", e))?;
                Ok(CommandResult::Json(serde_json::json!({ "changed": true, "cwd": new_cwd })))
            }

            "code/shell-status" => {
                let _timer = TimingGuard::new("module", "code_shell_status");
                let persona_id = p.str("persona_id")?;

                let shell = self.state.shell_sessions.get(persona_id)
                    .ok_or_else(|| format!("No shell session for {}", persona_id))?;

                let info = shell.info();
                Ok(CommandResult::Json(serde_json::to_value(&info).unwrap_or_default()))
            }

            "code/shell-watch" => {
                let _timer = TimingGuard::new("module", "code_shell_watch");
                let persona_id = p.str("persona_id")?;
                let execution_id = p.str("execution_id")?;

                let (exec_state, notify) = {
                    let shell = self.state.shell_sessions.get(persona_id)
                        .ok_or_else(|| format!("No shell session for {}", persona_id))?;
                    shell.get_watch_handles(execution_id)
                        .map_err(|e| format!("{}", e))?
                };

                let exec_id = execution_id.to_string();
                let result = self.state.rt_handle.block_on(async {
                    code::watch_execution(&exec_id, exec_state, notify).await
                }).map_err(|e| format!("{}", e))?;

                Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or_default()))
            }

            "code/shell-sentinel" => {
                let _timer = TimingGuard::new("module", "code_shell_sentinel");
                let persona_id = p.str("persona_id")?;
                let execution_id = p.str("execution_id")?;
                let rules: Vec<code::shell_types::SentinelRule> = p.json_or("rules");

                let shell = self.state.shell_sessions.get(persona_id)
                    .ok_or_else(|| format!("No shell session for {}", persona_id))?;

                let count = shell.set_sentinel(execution_id, &rules)
                    .map_err(|e| format!("{}", e))?;
                Ok(CommandResult::Json(serde_json::json!({ "rules_applied": count })))
            }

            "code/shell-destroy" => {
                let _timer = TimingGuard::new("module", "code_shell_destroy");
                let persona_id = p.str("persona_id")?;

                let removed = self.state.shell_sessions.remove(persona_id).is_some();
                log_info!("module", "code", "Destroyed shell for {}", persona_id);
                Ok(CommandResult::Json(serde_json::json!({ "destroyed": removed })))
            }

            _ => Err(format!("Unknown code command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any { self }
}
