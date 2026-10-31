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
use crate::runtime::{CommandExecutor, CommandResult, LateBound, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
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
    /// The executor, bound at boot: its policy is the ONE owner of caller trust
    /// ([`Self::caller_trust`]). Unbound resolves no trust.
    pub executor: LateBound<CommandExecutor>,
    /// Who is acting and where, handed in at construction by the layer above the kernel
    /// (never a global).
    pub acting: Arc<dyn crate::identity::acting::ActingProvider>,
}

impl CodeState {
    pub fn new(
        file_engines: Arc<DashMap<String, FileEngine>>,
        shell_sessions: Arc<DashMap<String, ShellSession>>,
        rt_handle: tokio::runtime::Handle,
        acting: Arc<dyn crate::identity::acting::ActingProvider>,
    ) -> Self {
        Self {
            acting,
            file_engines,
            shell_sessions,
            rt_handle,
            executor: LateBound::new("code::executor"),
        }
    }

    /// The trust the executor's policy resolves for `caller`. `None` (an unbound executor,
    /// or a policy that cannot resolve) is LEAST privilege: callers treat it as no grant,
    /// never as the owner.
    pub fn caller_trust(&self, caller: Option<&crate::routing::CallerIdentity>) -> Option<crate::routing::trust_level::TrustLevel> {
        self.executor.get().and_then(|e| e.caller_trust(caller))
    }

    /// Begin ending one set of hands: an acting context that is over (an eval or a fork
    /// finished, or a card claim ended). Kills every execution its shell still RUNS, and returns
    /// every execution not yet PROVEN ended, including ones already killed whose child hasn't
    /// been reaped, so the caller can wait each one out. Nothing is removed here: the context's
    /// handles stay until the caller has confirmed termination and calls
    /// [`Self::finish_release`], so a release that can't confirm can be retried (Astra on #5005).
    /// Never called for a caller's default (live) hands; `code/release-hands` refuses that.
    ///
    /// A kill ends the shell and everything it started that is still its descendant (found and
    /// killed by the execution's own task, deepest first); what survives is kept on the
    /// execution and re-checked here. A process that double-forked away from the shell is not a
    /// descendant and is not reached.
    pub fn begin_release(&self, who: &str) -> Vec<std::sync::Arc<std::sync::Mutex<crate::code::shell_session::ExecutionState>>> {
        let mut pending = Vec::new();
        if let Some(shell) = self.shell_sessions.get(who) {
            let ids: Vec<String> = shell.execution_ids().map(str::to_string).collect();
            for id in ids {
                let Ok(state) = shell.get_execution_state(&id) else { continue };
                // Ended means the shell was reaped AND nothing it started is known to survive.
                let (running, ended) = match state.lock() {
                    Ok(s) => (s.status == crate::code::shell_types::ShellExecutionStatus::Running, s.process_ended && s.unconfirmed_descendants.is_empty()),
                    Err(p) => (p.get_ref().status == crate::code::shell_types::ShellExecutionStatus::Running, p.get_ref().process_ended && p.get_ref().unconfirmed_descendants.is_empty()), // poisoned: read the last state
                };
                if ended {
                    continue;
                }
                if running {
                    let _ = shell.kill(&id); // a kill that lost a race with the exit is fine: the wait-out below decides
                }
                pending.push(state);
            }
        }
        pending
    }

    /// Finish ending one set of hands, once every execution `begin_release` returned is
    /// confirmed gone: drop its file engine and its shell.
    pub fn finish_release(&self, who: &str) {
        self.shell_sessions.remove(who);
        self.file_engines.remove(who);
    }

    /// Apply `f` to the session holding `handle` (a full execution id or a unique prefix),
    /// resolved ONCE across the sessions THESE hands reach ([`HandsKey::reaches`]): her default
    /// and card hands reach each other, so a handle from her claim A stays reachable after her
    /// focus moved to claim B; an isolated eval or fork reaches only itself, and nothing of hers
    /// reaches into it; another caller's never (Astra on #5026). A prefix that matches
    /// executions in two sessions is refused as ambiguous (Codex on #4586).
    pub fn with_execution<T>(
        &self,
        hands: &crate::code::hands::HandsKey,
        handle: &str,
        f: impl FnOnce(&ShellSession, &str) -> Result<T, String>,
    ) -> Result<T, ExecutionLookup> {
        let hers = |key: &str| hands.reaches(key);
        // Collect (session key, execution ids) shard by shard, then act on ONE session: no
        // guard is held across the scan.
        let sessions: Vec<(String, Vec<String>)> = self
            .shell_sessions
            .iter()
            .filter(|entry| hers(entry.key()))
            .map(|entry| (entry.key().clone(), entry.value().execution_ids().map(str::to_string).collect()))
            .collect();
        if sessions.is_empty() {
            return Err(ExecutionLookup::NoSession);
        }
        let ids: Vec<uuid::Uuid> = sessions
            .iter()
            .flat_map(|(_, ids)| ids.iter())
            .filter_map(|id| uuid::Uuid::parse_str(id).ok())
            .collect();
        let id = continuum_base::id_resolve::resolve_handle(handle, &ids, "shell execution")
            .map_err(ExecutionLookup::Unresolved)?
            .to_string();
        let holder = sessions
            .iter()
            .find(|(_, ids)| ids.iter().any(|e| *e == id))
            .map(|(key, _)| key.clone())
            .ok_or_else(|| ExecutionLookup::Unresolved(format!("shell execution {id} vanished")))?;
        let shell = self
            .shell_sessions
            .get(&holder)
            .ok_or_else(|| ExecutionLookup::Unresolved(format!("shell execution {id}'s hands were released")))?;
        f(&shell, &id).map_err(ExecutionLookup::Failed)
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
        objs.push(Arc::new(crate::commands::code::run::CodeRun { state: self.state.clone() }));
        objs
    }

    fn install_executor(&self, executor: Arc<CommandExecutor>) {
        self.state.executor.install(executor);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::ServiceModule;

    // what this catches (Kimi, 2026-09-29; Codex on #4586; Astra on #5026): a command started
    // on one turn unreachable on the next, or reachable across an isolation boundary. One set
    // of hands keeps ONE session for its life (hands are never re-rooted, card fc9e9fd7); her
    // default and card hands reach each other's handles (claim A's after her focus moved to B);
    // an isolated eval reaches only its own and nothing of hers reaches into it; another
    // caller (or a caller-id prefix) reaches nothing.
    #[tokio::test]
    async fn a_running_command_stays_reachable_by_its_handle_in_any_of_her_hands() {
        crate::system_resources::disk_reporters::prepare_writer_fixture().await;
        use crate::code::hands::HandsKey;
        use crate::identity::hands_context::HandsContext;
        let rt = tokio::runtime::Handle::current();
        let root = tempfile::tempdir().unwrap();
        let state = CodeState::new(Arc::new(DashMap::new()), Arc::new(DashMap::new()), rt.clone(), Arc::new(crate::test_acting::TestActing));
        let home = HandsKey::new("kimi".into(), None);
        let mut shell = ShellSession::new("s0", "kimi", root.path()).expect("session");
        let exec = shell.execute("sleep 30", Some(60_000), &rt).expect("a long command starts");
        state.shell_sessions.insert(home.map_key(), shell);

        let running = |s: &ShellSession, id: &str| s.get_execution_state(id);
        let by_full = state.with_execution(&home, &exec, running).expect("reachable by its handle");
        assert_eq!(by_full.lock().unwrap().status, crate::code::shell_types::ShellExecutionStatus::Running);
        assert!(state.with_execution(&home, &exec[..8], running).is_ok(), "a unique prefix resolves");
        let mara = HandsKey::new("mara".into(), None);
        assert!(matches!(state.with_execution(&mara, &exec, running), Err(ExecutionLookup::NoSession)), "another caller never reaches her command");

        // A command in her claim A's hands, polled from claim B: still hers.
        let claim_a = HandsKey::new("kimi".into(), Some(HandsContext::for_claim(uuid::Uuid::new_v4(), uuid::Uuid::new_v4(), uuid::Uuid::new_v4())));
        let claim_b = HandsKey::new("kimi".into(), Some(HandsContext::for_claim(uuid::Uuid::new_v4(), uuid::Uuid::new_v4(), uuid::Uuid::new_v4())));
        let mut a_shell = ShellSession::new("sa", "kimi", root.path()).expect("session");
        let a_exec = a_shell.execute("sleep 30", Some(60_000), &rt).expect("a command in claim A");
        state.shell_sessions.insert(claim_a.map_key(), a_shell);
        state.shell_sessions.insert(claim_b.map_key(), ShellSession::new("sb", "kimi", root.path()).expect("session"));
        assert!(state.with_execution(&claim_b, &a_exec, running).is_ok(), "claim A's handle is reachable while she works claim B");
        assert!(matches!(state.with_execution(&HandsKey::new("kim".into(), None), &a_exec, running), Err(ExecutionLookup::NoSession)), "a caller-id prefix is not her");

        // An isolated eval reaches only itself, and nothing of hers reaches into it.
        let eval = HandsKey::new("kimi".into(), Some(HandsContext::fresh()));
        let mut e_shell = ShellSession::new("se", "kimi", root.path()).expect("session");
        let e_exec = e_shell.execute("sleep 30", Some(60_000), &rt).expect("a command in the eval");
        state.shell_sessions.insert(eval.map_key(), e_shell);
        assert!(state.with_execution(&eval, &exec, running).is_err(), "an eval never reaches her live command");
        assert!(state.with_execution(&home, &e_exec, running).is_err(), "her live hands never reach into an eval");
        assert!(state.with_execution(&eval, &e_exec, running).is_ok(), "the eval reaches its own");

        for (hands, id) in [(&eval, &e_exec), (&home, &a_exec), (&home, &exec)] {
            state.with_execution(hands, id, |s, id| s.kill(id)).expect("clean up the test commands");
        }
    }

    // what this catches (Astra's review of #5005): ending a context's hands dropping its
    // handles before every execution is proven gone, and skipping one already killed but not
    // yet reaped. Both executions must be returned to wait out, and the shell must stay
    // reachable (so a release that can't confirm can retry) until `finish_release`.
    #[tokio::test]
    async fn ending_hands_keeps_every_handle_until_each_execution_is_proven_ended() {
        crate::system_resources::disk_reporters::prepare_writer_fixture().await;
        let rt = tokio::runtime::Handle::current();
        let root = tempfile::tempdir().unwrap();
        let state = CodeState::new(Arc::new(DashMap::new()), Arc::new(DashMap::new()), rt.clone(), Arc::new(crate::test_acting::TestActing));
        let who = "kimi#context";
        let mut shell = ShellSession::new("s0", who, root.path()).expect("session");
        let running = shell.execute("sleep 30", Some(60_000), &rt).expect("a command starts");
        let killed = shell.execute("sleep 30", Some(60_000), &rt).expect("a command starts");
        // Killed, and on this single-threaded test runtime its task can't have reaped it yet.
        shell.kill(&killed).expect("kill");
        state.shell_sessions.insert(who.into(), shell);

        let pending = state.begin_release(who);
        assert_eq!(pending.len(), 2, "the running execution AND the killed-but-unreaped one are both waited out");
        assert!(state.shell_sessions.contains_key(who), "nothing is dropped before termination is confirmed");
        let shell = state.shell_sessions.get(who).expect("still there");
        for id in [&running, &killed] {
            assert_eq!(
                shell.get_execution_state(id).unwrap().lock().unwrap().status,
                crate::code::shell_types::ShellExecutionStatus::Killed,
                "every execution not proven ended is killed"
            );
        }
        drop(shell);

        for exec in &pending {
            while !exec.lock().unwrap().process_ended {
                assert!(exec.lock().unwrap().process_end_error.is_none(), "a direct child's reap succeeds");
                tokio::time::sleep(continuum_process::KILL_POLL).await;
            }
        }
        assert!(state.begin_release(who).is_empty(), "once reaped, nothing is left to wait out");
        state.finish_release(who);
        assert!(!state.shell_sessions.contains_key(who), "a confirmed release drops the context's shell");
    }

    fn module() -> CodeModule {
        let state = Arc::new(CodeState::new(
            Arc::new(DashMap::new()),
            Arc::new(DashMap::new()),
            tokio::runtime::Handle::current(),
            Arc::new(crate::test_acting::TestActing),
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
