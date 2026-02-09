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
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(
        &self,
        command: &str,
        params: Value,
    ) -> Result<CommandResult, String> {
        match command {
            "code/create-workspace" => {
                let _timer = TimingGuard::new("module", "code_create_workspace");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let workspace_root = params.get("workspace_root")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing workspace_root")?;
                let read_roots: Vec<String> = params.get("read_roots")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();

                let root = std::path::Path::new(workspace_root);
                let mut security = PathSecurity::new(root)
                    .map_err(|e| format!("Invalid workspace: {}", e))?;

                for rr in &read_roots {
                    security.add_read_root(std::path::Path::new(rr))
                        .map_err(|e| format!("Invalid read root '{}': {}", rr, e))?;
                }

                let engine = FileEngine::new(persona_id, security);
                self.state.file_engines.insert(persona_id.to_string(), engine);

                log_info!("module", "code", "Created workspace for {} at {}", persona_id, workspace_root);
                Ok(CommandResult::Json(serde_json::json!({ "created": true })))
            }

            "code/read" => {
                let _timer = TimingGuard::new("module", "code_read");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let file_path = params.get("file_path")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing file_path")?;
                let start_line = params.get("start_line")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u32);
                let end_line = params.get("end_line")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u32);

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                let result = engine.read(file_path, start_line, end_line)
                    .map_err(|e| format!("{}", e))?;

                Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or_default()))
            }

            "code/write" => {
                let _timer = TimingGuard::new("module", "code_write");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let file_path = params.get("file_path")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing file_path")?;
                let content = params.get("content")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing content")?;
                let description = params.get("description")
                    .and_then(|v| v.as_str());

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                let result = engine.write(file_path, content, description)
                    .map_err(|e| format!("{}", e))?;

                log_info!("module", "code", "Write {} ({} bytes) by {}", file_path, result.bytes_written, persona_id);
                Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or_default()))
            }

            "code/edit" => {
                let _timer = TimingGuard::new("module", "code_edit");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let file_path = params.get("file_path")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing file_path")?;
                let edit_mode = params.get("edit_mode")
                    .ok_or("Missing edit_mode")?;
                let description = params.get("description")
                    .and_then(|v| v.as_str());

                let edit: crate::code::EditMode = serde_json::from_value(edit_mode.clone())
                    .map_err(|e| format!("Invalid edit_mode: {}", e))?;

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                let result = engine.edit(file_path, &edit, description)
                    .map_err(|e| format!("{}", e))?;

                log_info!("module", "code", "Edit {} by {}", file_path, persona_id);
                Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or_default()))
            }

            "code/delete" => {
                let _timer = TimingGuard::new("module", "code_delete");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let file_path = params.get("file_path")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing file_path")?;
                let description = params.get("description")
                    .and_then(|v| v.as_str());

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                let result = engine.delete(file_path, description)
                    .map_err(|e| format!("{}", e))?;

                log_info!("module", "code", "Delete {} by {}", file_path, persona_id);
                Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or_default()))
            }

            "code/diff" => {
                let _timer = TimingGuard::new("module", "code_diff");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let file_path = params.get("file_path")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing file_path")?;
                let edit_mode = params.get("edit_mode")
                    .ok_or("Missing edit_mode")?;

                let edit: crate::code::EditMode = serde_json::from_value(edit_mode.clone())
                    .map_err(|e| format!("Invalid edit_mode: {}", e))?;

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                let result = engine.preview_diff(file_path, &edit)
                    .map_err(|e| format!("{}", e))?;

                Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or_default()))
            }

            "code/undo" => {
                let _timer = TimingGuard::new("module", "code_undo");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let change_id = params.get("change_id")
                    .and_then(|v| v.as_str());
                let count = params.get("count")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as usize);

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

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let file_path = params.get("file_path")
                    .and_then(|v| v.as_str());
                let limit = params.get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as usize)
                    .unwrap_or(50);

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

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let pattern = params.get("pattern")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing pattern")?;
                let file_glob = params.get("file_glob")
                    .and_then(|v| v.as_str());
                let max_results = params.get("max_results")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u32)
                    .unwrap_or(100);

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

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let path = params.get("path")
                    .and_then(|v| v.as_str());
                let max_depth = params.get("max_depth")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u32)
                    .unwrap_or(10);
                let include_hidden = params.get("include_hidden")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                let root = engine.workspace_root();
                let target = path.map(|p| root.join(p)).unwrap_or_else(|| root.to_path_buf());

                let result = tree::generate_tree(&target, max_depth, include_hidden);

                Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or_default()))
            }

            // Git commands
            "code/git-status" => {
                let _timer = TimingGuard::new("module", "code_git_status");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                let result = git_bridge::git_status(&engine.workspace_root());
                Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or_default()))
            }

            "code/git-diff" => {
                let _timer = TimingGuard::new("module", "code_git_diff");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let staged = params.get("staged")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                match git_bridge::git_diff(&engine.workspace_root(), staged) {
                    Ok(diff) => Ok(CommandResult::Json(serde_json::json!({ "diff": diff }))),
                    Err(e) => Err(e),
                }
            }

            "code/git-log" => {
                let _timer = TimingGuard::new("module", "code_git_log");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let count = params.get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u32)
                    .unwrap_or(10);

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                match git_bridge::git_log(&engine.workspace_root(), count) {
                    Ok(log) => Ok(CommandResult::Json(serde_json::json!({ "log": log }))),
                    Err(e) => Err(e),
                }
            }

            "code/git-add" => {
                let _timer = TimingGuard::new("module", "code_git_add");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let paths: Vec<String> = params.get("paths")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                let path_refs: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
                match git_bridge::git_add(&engine.workspace_root(), &path_refs) {
                    Ok(output) => Ok(CommandResult::Json(serde_json::json!({ "output": output }))),
                    Err(e) => Err(e),
                }
            }

            "code/git-commit" => {
                let _timer = TimingGuard::new("module", "code_git_commit");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let message = params.get("message")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing message")?;

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                match git_bridge::git_commit(&engine.workspace_root(), message) {
                    Ok(hash) => {
                        log_info!("module", "code", "Git commit by {}: {}", persona_id, message);
                        Ok(CommandResult::Json(serde_json::json!({ "hash": hash })))
                    }
                    Err(e) => Err(e),
                }
            }

            "code/git-push" => {
                let _timer = TimingGuard::new("module", "code_git_push");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let remote = params.get("remote")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let branch = params.get("branch")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let engine = self.state.file_engines.get(persona_id)
                    .ok_or_else(|| format!("No workspace for persona {}", persona_id))?;

                match git_bridge::git_push(&engine.workspace_root(), remote, branch) {
                    Ok(output) => {
                        log_info!("module", "code", "Git push by {}", persona_id);
                        Ok(CommandResult::Json(serde_json::json!({ "output": output })))
                    }
                    Err(e) => Err(e),
                }
            }

            // Shell commands
            "code/shell-create" => {
                let _timer = TimingGuard::new("module", "code_shell_create");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let workspace_root = params.get("workspace_root")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing workspace_root")?;

                // Generate unique session ID for this shell
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

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let command = params.get("command")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing command")?;
                let timeout_ms = params.get("timeout_ms")
                    .and_then(|v| v.as_u64());
                let wait = params.get("wait")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let mut shell = self.state.shell_sessions.get_mut(persona_id)
                    .ok_or_else(|| format!("No shell session for {}", persona_id))?;

                if wait {
                    let result = shell.execute_and_wait(command, timeout_ms, &self.state.rt_handle)
                        .map_err(|e| format!("{}", e))?;
                    Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or_default()))
                } else {
                    let execution_id = shell.execute(command, timeout_ms, &self.state.rt_handle)
                        .map_err(|e| format!("{}", e))?;
                    Ok(CommandResult::Json(serde_json::json!({
                        "execution_id": execution_id,
                        "started": true,
                    })))
                }
            }

            "code/shell-poll" => {
                let _timer = TimingGuard::new("module", "code_shell_poll");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let execution_id = params.get("execution_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing execution_id")?;

                let shell = self.state.shell_sessions.get(persona_id)
                    .ok_or_else(|| format!("No shell session for {}", persona_id))?;

                let result = shell.poll(execution_id)
                    .map_err(|e| format!("{}", e))?;
                Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or_default()))
            }

            "code/shell-kill" => {
                let _timer = TimingGuard::new("module", "code_shell_kill");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let execution_id = params.get("execution_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing execution_id")?;

                let shell = self.state.shell_sessions.get(persona_id)
                    .ok_or_else(|| format!("No shell session for {}", persona_id))?;

                shell.kill(execution_id)
                    .map_err(|e| format!("{}", e))?;
                Ok(CommandResult::Json(serde_json::json!({ "killed": true })))
            }

            "code/shell-cd" => {
                let _timer = TimingGuard::new("module", "code_shell_cd");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let path = params.get("path")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing path")?;

                let mut shell = self.state.shell_sessions.get_mut(persona_id)
                    .ok_or_else(|| format!("No shell session for {}", persona_id))?;

                let new_cwd = shell.cd(path)
                    .map_err(|e| format!("{}", e))?;

                Ok(CommandResult::Json(serde_json::json!({ "changed": true, "cwd": new_cwd })))
            }

            "code/shell-status" => {
                let _timer = TimingGuard::new("module", "code_shell_status");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;

                let shell = self.state.shell_sessions.get(persona_id)
                    .ok_or_else(|| format!("No shell session for {}", persona_id))?;

                let info = shell.info();
                Ok(CommandResult::Json(serde_json::to_value(&info).unwrap_or_default()))
            }

            "code/shell-watch" => {
                let _timer = TimingGuard::new("module", "code_shell_watch");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let execution_id = params.get("execution_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing execution_id")?;

                // Get watch handles while holding the DashMap lock, then release
                let (exec_state, notify) = {
                    let shell = self.state.shell_sessions.get(persona_id)
                        .ok_or_else(|| format!("No shell session for {}", persona_id))?;
                    shell.get_watch_handles(execution_id)
                        .map_err(|e| format!("{}", e))?
                };

                // Now call async watch with DashMap lock released
                let exec_id = execution_id.to_string();
                let result = self.state.rt_handle.block_on(async {
                    code::watch_execution(&exec_id, exec_state, notify).await
                }).map_err(|e| format!("{}", e))?;

                Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or_default()))
            }

            "code/shell-sentinel" => {
                let _timer = TimingGuard::new("module", "code_shell_sentinel");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let execution_id = params.get("execution_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing execution_id")?;
                let rules: Vec<code::shell_types::SentinelRule> = params.get("rules")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();

                let shell = self.state.shell_sessions.get(persona_id)
                    .ok_or_else(|| format!("No shell session for {}", persona_id))?;

                let count = shell.set_sentinel(execution_id, &rules)
                    .map_err(|e| format!("{}", e))?;

                Ok(CommandResult::Json(serde_json::json!({
                    "rules_applied": count,
                })))
            }

            "code/shell-destroy" => {
                let _timer = TimingGuard::new("module", "code_shell_destroy");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;

                let removed = self.state.shell_sessions.remove(persona_id).is_some();

                log_info!("module", "code", "Destroyed shell for {}", persona_id);
                Ok(CommandResult::Json(serde_json::json!({ "destroyed": removed })))
            }

            _ => Err(format!("Unknown code command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any { self }
}
