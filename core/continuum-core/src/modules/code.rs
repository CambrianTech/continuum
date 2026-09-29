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
    /// Sessions set aside when her hands moved to another root, newest last, keyed by
    /// caller. A work turn roots her at the card and restores her home afterwards, so the
    /// session a long command runs in is left behind every turn; parking it keeps its
    /// execution handles pollable (Kimi, 2026-09-29: code/shell-poll failing on every
    /// handle from an earlier turn).
    parked_shells: Arc<DashMap<String, Vec<ShellSession>>>,
}

/// How many set-aside sessions one caller keeps. Her roots per turn are few (home, the
/// held card, sometimes a review checkout); a longer tail only holds dead handles.
const PARKED_SHELLS_PER_CALLER: usize = 4;

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
            parked_shells: Arc::new(DashMap::new()),
        }
    }

    /// Move `who`'s shell to `new_root`. A no-op at the same root. Otherwise: first take back
    /// a session she parked at `new_root` (so returning is never lost to eviction), then set
    /// the current one aside, then trim the parked set, dropping only IDLE sessions, oldest
    /// first. A session with a running execution is never dropped, however many roots she
    /// works across (Codex on #4586). With nothing to reinstate, the next command opens a
    /// session at the new root, as before.
    pub fn re_root_shell(&self, who: &str, new_root: &std::path::Path) {
        let same = |a: &std::path::Path, b: &std::path::Path| {
            let canon = |p: &std::path::Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()); // unwrap_or_else: an uncanonicalizable path compares as written
            canon(a) == canon(b)
        };
        if self
            .shell_sessions
            .get(who)
            .is_some_and(|s| same(s.workspace_root(), new_root))
        {
            return;
        }
        let mut parked = self.parked_shells.entry(who.to_string()).or_default();
        let destination = parked
            .iter()
            .position(|s| same(s.workspace_root(), new_root))
            .map(|i| parked.remove(i));
        if let Some((_, current)) = self.shell_sessions.remove(who) {
            parked.push(current);
        }
        if let Some(back) = destination {
            self.shell_sessions.insert(who.to_string(), back);
        }
        while parked.len() > PARKED_SHELLS_PER_CALLER {
            let Some(idle) = parked.iter().position(|s| !s.has_running()) else {
                break; // every parked session has live work: keep them all
            };
            let dropped = parked.remove(idle);
            crate::probe!(
                class = "code.shell.parked_session_dropped",
                caller = %who,
                root = %dropped.workspace_root().display(),
                "an idle set-aside shell session was dropped; only finished executions went with it"
            );
        }
    }

    /// Apply `f` to the session holding `handle` (a full execution id or a prefix), resolved
    /// ONCE across all of `who`'s sessions, current and parked. A prefix matching executions in
    /// two sessions is refused as ambiguous, never resolved to whichever is searched first, so
    /// a kill never lands on the wrong command (Codex on #4586).
    pub fn with_execution<T>(
        &self,
        who: &str,
        handle: &str,
        f: impl FnOnce(&ShellSession, &str) -> Result<T, String>,
    ) -> Result<T, ExecutionLookup> {
        let current = self.shell_sessions.get(who);
        let parked = self.parked_shells.get(who);
        let sessions: Vec<&ShellSession> = current
            .iter()
            .map(|s| &**s)
            .chain(parked.iter().flat_map(|p| p.iter()))
            .collect();
        if sessions.is_empty() {
            return Err(ExecutionLookup::NoSession);
        }
        let ids: Vec<uuid::Uuid> = sessions
            .iter()
            .flat_map(|s| s.execution_ids())
            .filter_map(|id| uuid::Uuid::parse_str(id).ok())
            .collect();
        let id = crate::id_resolve::resolve_handle(handle, &ids, "shell execution")
            .map_err(ExecutionLookup::Unresolved)?
            .to_string();
        let holder = sessions
            .into_iter()
            .find(|s| s.execution_ids().any(|e| e == id))
            .ok_or_else(|| ExecutionLookup::Unresolved(format!("shell execution {id} vanished")))?;
        f(holder, &id).map_err(ExecutionLookup::Failed)
    }
}

/// Why a handle lookup across a caller's sessions did not reach a command.
#[derive(Debug)]
pub enum ExecutionLookup {
    /// She has no shell session at all, current or set aside.
    NoSession,
    /// She has sessions, but the handle names none (unknown, expired, or ambiguous).
    Unresolved(String),
    /// The handle resolved, and the operation on it failed.
    Failed(String),
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

    // what this catches (Kimi, 2026-09-29; Codex's review of #4586): re-rooting her hands
    // dropping the shell session and with it every execution handle, so a command started
    // on one turn could not be polled on the next. A REAL running command survives a
    // re-root and is reachable by its handle (and a prefix of it); another caller cannot
    // reach it; returning reinstates the same session; and working across more roots than
    // the cap never drops a session whose command is still running.
    #[tokio::test]
    async fn a_running_command_survives_re_rooting_and_is_never_evicted() {
        let rt = tokio::runtime::Handle::current();
        let roots: Vec<_> = (0..6).map(|_| tempfile::tempdir().unwrap()).collect();
        let state = CodeState::new(Arc::new(DashMap::new()), Arc::new(DashMap::new()), rt.clone());

        let mut first = ShellSession::new("s0", "kimi", roots[0].path()).expect("session");
        let exec = first.execute("sleep 30", Some(60_000), &rt).expect("a long command starts");
        state.shell_sessions.insert("kimi".into(), first);

        state.re_root_shell("kimi", roots[0].path());
        assert!(state.shell_sessions.contains_key("kimi"), "the same root keeps the session");

        // Work across five more roots, each with its own (idle) session, past the cap of 4.
        for (i, root) in roots.iter().enumerate().skip(1) {
            state.re_root_shell("kimi", root.path());
            let idle = ShellSession::new(&format!("s{i}"), "kimi", root.path()).expect("session");
            state.shell_sessions.insert("kimi".into(), idle);
        }

        let running = |s: &ShellSession, id: &str| s.get_execution_state(id);
        let by_full = state.with_execution("kimi", &exec, running).expect("still reachable after 5 re-roots");
        assert_eq!(by_full.lock().unwrap().status, crate::code::shell_types::ShellExecutionStatus::Running);
        let prefix = &exec[..8];
        assert!(state.with_execution("kimi", prefix, running).is_ok(), "a unique prefix resolves across sessions");
        assert!(
            matches!(state.with_execution("mara", &exec, running), Err(ExecutionLookup::NoSession)),
            "another caller never reaches her command"
        );

        state.re_root_shell("kimi", roots[0].path());
        let back = state.shell_sessions.get("kimi").expect("returning reinstates it");
        assert_eq!(back.id(), "s0", "the very same session, with its handles");
        back.kill(&exec).expect("clean up the test command");
    }

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
