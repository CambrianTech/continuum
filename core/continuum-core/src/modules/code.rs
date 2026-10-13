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
    /// One lock per set of hands, held while their root is MOVED (engine and shell together)
    /// or a command is STARTED in it, never across an await. Two turns moving the same hands
    /// used to interleave the engine insert and the shell re-root, leaving her code/* tools
    /// in the card while her shell stood at home (card 2d58192d, Kimi #11697). Taken before
    /// any map guard, so it nests outside every DashMap shard and never inside one.
    /// The flag is CLOSED: set by [`Self::finish_release`] while it holds the lock, so a
    /// waiter that wakes on a released key's lock retakes the key's current one instead of
    /// proceeding beside a newcomer on a fresh lock (Astra on #5023).
    hands_locks: Arc<DashMap<String, Arc<std::sync::Mutex<bool>>>>,
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
            hands_locks: Arc::new(DashMap::new()),
        }
    }

    /// Run `f` holding `who`'s hands lock: the ONE serialization point for moving her root,
    /// provisioning her engine, starting a command and releasing her hands. `f` never awaits.
    /// A lock whose release closed it is never acted under: the key's current lock is taken.
    fn holding<R>(&self, who: &str, f: impl FnOnce(&mut bool) -> R) -> R {
        loop {
            let lock = self.hands_locks.entry(who.to_string()).or_default().clone();
            let mut closed = lock.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: a poisoned lock still serializes
            if *closed {
                continue;
            }
            return f(&mut closed);
        }
    }

    fn move_locked(&self, who: &str, engine: Option<FileEngine>, root: &std::path::Path) {
        match engine {
            Some(engine) => {
                self.file_engines.insert(who.to_string(), engine);
            }
            None => {
                self.file_engines.remove(who);
            }
        }
        self.re_root_shell(who, root);
    }

    fn shell_locked<R>(
        &self,
        who: &str,
        open: impl FnOnce(&std::path::Path) -> Result<ShellSession, String>,
        f: impl FnOnce(&mut ShellSession) -> Result<R, String>,
    ) -> Result<R, String> {
        let root = self
            .file_engines
            .get(who)
            .map(|e| e.workspace_root())
            .ok_or_else(|| "these hands have no workspace (no file engine)".to_string())?;
        self.re_root_shell(who, &root);
        if !self.shell_sessions.contains_key(who) {
            let shell = open(&root)?;
            self.shell_sessions.insert(who.to_string(), shell);
        }
        let mut shell = self
            .shell_sessions
            .get_mut(who)
            .ok_or_else(|| "the shell vanished under the hands lock".to_string())?;
        f(&mut shell)
    }

    /// Move `who`'s hands to `root`: her file engine (replaced by `engine`, or dropped for the
    /// next call to provision when `None`) and her shell TOGETHER, under the hands lock, so
    /// two movers serialize instead of interleaving (card 2d58192d).
    pub fn move_hands(&self, who: &str, engine: Option<FileEngine>, root: &std::path::Path) {
        self.holding(who, |_| self.move_locked(who, engine, root));
    }

    /// [`Self::move_hands`] and then `f` on the shell at the NEW root, in one hold: what a
    /// move installs into her shell (an era PATH prefix, PYTHONPATH at the new workspace)
    /// lands in the session at the root it was computed for, never in one a later mover
    /// put there while this caller awaited (Astra on #5023).
    pub fn move_hands_then<R>(
        &self,
        who: &str,
        engine: FileEngine,
        root: &std::path::Path,
        open: impl FnOnce(&std::path::Path) -> Result<ShellSession, String>,
        f: impl FnOnce(&mut ShellSession) -> Result<R, String>,
    ) -> Result<R, String> {
        self.holding(who, |_| {
            self.move_locked(who, Some(engine), root);
            self.shell_locked(who, open, f)
        })
    }

    /// Give `who` a file engine when she has none, under the hands lock: a mover that
    /// installed one while this caller computed its root wins, and the computed one is dropped.
    pub fn provision_engine(&self, who: &str, make: impl FnOnce() -> FileEngine) {
        self.holding(who, |_| {
            self.file_engines.entry(who.to_string()).or_insert_with(make);
        });
    }

    /// Run `f` on the shell standing where `who`'s FILE ENGINE stands. The engine is the one
    /// owner of the root; the shell follows it here, on every call, so however the two came
    /// to disagree, the next command runs where her code/* tools act (card 2d58192d). Under
    /// the hands lock: a session at another root is set aside (a running command in it keeps
    /// going and stays pollable), one is opened with `open` when none stands at the root, and
    /// `f` acts on THAT session, never one re-fetched after a later move. Nothing here awaits.
    pub fn with_current_shell<R>(
        &self,
        who: &str,
        open: impl FnOnce(&std::path::Path) -> Result<ShellSession, String>,
        f: impl FnOnce(&mut ShellSession) -> Result<R, String>,
    ) -> Result<R, String> {
        self.holding(who, |_| self.shell_locked(who, open, f))
    }

    /// Begin ending one set of hands: an acting context that is over (an eval or a fork
    /// finished). Kills every execution its current and parked shells still RUN, and returns
    /// every execution not yet PROVEN ended, including ones already killed whose child hasn't
    /// been reaped, so the caller can wait each one out. Nothing is removed here: the context's
    /// handles stay until the caller has confirmed termination and calls
    /// [`Self::finish_release`], so a release that can't confirm can be retried (Astra on #5005).
    /// Never called for a caller's default (live) hands; `code/release-hands` refuses that.
    ///
    /// Scope, stated: a kill ends the execution's DIRECT child. The shell isn't spawned in its
    /// own process group, so a compound command's grandchildren can outlive it; that is a
    /// shell-wide gap (code/shell-kill has it too), carded separately (ea06701d).
    pub fn begin_release(&self, who: &str) -> Vec<std::sync::Arc<std::sync::Mutex<crate::code::shell_session::ExecutionState>>> {
        let mut pending = Vec::new();
        let mut visit = |shell: &ShellSession| {
            let ids: Vec<String> = shell.execution_ids().map(str::to_string).collect();
            for id in ids {
                let Ok(state) = shell.get_execution_state(&id) else { continue };
                let (running, ended) = match state.lock() {
                    Ok(s) => (s.status == crate::code::shell_types::ShellExecutionStatus::Running, s.process_ended),
                    Err(p) => (p.get_ref().status == crate::code::shell_types::ShellExecutionStatus::Running, p.get_ref().process_ended), // poisoned: read the last state
                };
                if ended {
                    continue;
                }
                if running {
                    let _ = shell.kill(&id); // a kill that lost a race with the exit is fine: the wait-out below decides
                }
                pending.push(state);
            }
        };
        if let Some(current) = self.shell_sessions.get(who) {
            visit(&current);
        }
        if let Some(parked) = self.parked_shells.get(who) {
            for shell in parked.iter() {
                visit(shell);
            }
        }
        pending
    }

    /// Finish ending one set of hands, once every execution `begin_release` returned is
    /// confirmed gone: drop its file engine, its shell and its parked shells.
    pub fn finish_release(&self, who: &str) {
        self.holding(who, |closed| {
            self.shell_sessions.remove(who);
            self.parked_shells.remove(who);
            self.file_engines.remove(who);
            // Closed and unlisted while still held: a waiter on this lock wakes, sees it
            // closed and takes the key's current lock, so the key never has two (Astra on #5023).
            *closed = true;
            self.hands_locks.remove(who);
        });
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
        // LOCK ORDER (Cormac on #4586): never hold a `parked_shells` guard while touching
        // `shell_sessions`. `with_execution` takes shell_sessions then parked_shells; holding
        // them the other way round here could deadlock two callers on shared shards. So each
        // map is touched in its own statement: take the current out, work the parked set, put
        // the destination in.
        let current = self.shell_sessions.remove(who).map(|(_, s)| s);
        let destination = {
            let mut parked = self.parked_shells.entry(who.to_string()).or_default();
            let destination = parked
                .iter()
                .position(|s| same(s.workspace_root(), new_root))
                .map(|i| parked.remove(i));
            if let Some(current) = current {
                parked.push(current);
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
            destination
        };
        if let Some(back) = destination {
            self.shell_sessions.insert(who.to_string(), back);
        }
    }

    /// Holds `shell_sessions` then `parked_shells`, the one order used anywhere both are held.
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

    // what this catches (Astra's review of #5005): ending a context's hands dropping its
    // handles before every execution is proven gone, and skipping one already killed but not
    // yet reaped. Both executions must be returned to wait out, and the shell must stay
    // reachable (so a release that can't confirm can retry) until `finish_release`.
    #[tokio::test]
    async fn ending_hands_keeps_every_handle_until_each_execution_is_proven_ended() {
        let rt = tokio::runtime::Handle::current();
        let root = tempfile::tempdir().unwrap();
        let state = CodeState::new(Arc::new(DashMap::new()), Arc::new(DashMap::new()), rt.clone());
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
                tokio::time::sleep(crate::code::shell_session::KILL_POLL).await;
            }
        }
        assert!(state.begin_release(who).is_empty(), "once reaped, nothing is left to wait out");
        state.finish_release(who);
        assert!(!state.shell_sessions.contains_key(who), "a confirmed release drops the context's shell");
    }

    // what this catches (card 2d58192d, Kimi #11697): her file engine and her shell standing in
    // different roots. Two turns moving one set of hands interleaved the engine insert and the
    // shell re-root, and an existing session was reused without comparing roots, so code/* acted
    // in the card while code/shell ran at home. Here the interleave is simulated (the engine
    // moved, the shell did not) with a command RUNNING in the old root: the next start runs where
    // the engine stands, the running command keeps going and stays pollable, and move_hands
    // takes engine and shell together.
    #[tokio::test]
    async fn a_command_starts_where_her_engine_stands_whatever_moved_the_shell() {
        let rt = tokio::runtime::Handle::current();
        let (home, card) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let state = CodeState::new(Arc::new(DashMap::new()), Arc::new(DashMap::new()), rt.clone());
        let engine_at = |root: &std::path::Path| FileEngine::new("kimi", crate::code::PathSecurity::new(root).expect("a root"));
        let open = |root: &std::path::Path| ShellSession::new(&uuid::Uuid::new_v4().to_string(), "kimi", root).map_err(|e| e.to_string());
        let canon = |p: &std::path::Path| std::fs::canonicalize(p).expect("a real dir");

        state.move_hands("kimi", Some(engine_at(home.path())), home.path());
        let long = state
            .with_current_shell("kimi", open, |s| s.execute("sleep 30", Some(60_000), &rt))
            .expect("a command starts at home");

        // the interleave: the engine moved to the card and the shell did not
        state.file_engines.insert("kimi".into(), engine_at(card.path()));
        let root = state
            .with_current_shell("kimi", open, |s| Ok(s.workspace_root().to_path_buf()))
            .expect("the next start");
        assert_eq!(canon(&root), canon(card.path()), "the next command runs where her code/* tools act");
        let earlier = state
            .with_execution("kimi", &long, |s, id| s.get_execution_state(id))
            .expect("the earlier command stays pollable");
        assert_eq!(
            earlier.lock().unwrap().status,
            crate::code::shell_types::ShellExecutionStatus::Running,
            "and keeps running in its own root"
        );

        state.move_hands("kimi", Some(engine_at(home.path())), home.path());
        let shell_root = state.shell_sessions.get("kimi").map(|s| canon(s.workspace_root())).expect("a session");
        assert_eq!(shell_root, canon(home.path()), "move_hands moves the shell with the engine");
        state.with_execution("kimi", &long, |s, id| s.kill(id)).expect("clean up the test command");
    }

    // what this catches (Astra on #5023): two locks for one key, and a stale engine winning.
    // A release removed the key's lock after unlocking, so a waiter holding the old lock and a
    // newcomer on a fresh one could both act; the release now CLOSES the old lock while held,
    // and the next hold takes a fresh, open one. Lazy provisioning computed her root across
    // awaits and inserted outside the lock; an engine a mover installed meanwhile now wins.
    #[tokio::test]
    async fn a_released_key_never_has_two_locks_and_provisioning_never_overwrites_a_move() {
        let rt = tokio::runtime::Handle::current();
        let (moved, stale) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let state = CodeState::new(Arc::new(DashMap::new()), Arc::new(DashMap::new()), rt);
        let engine_at = |root: &std::path::Path| FileEngine::new("kimi", crate::code::PathSecurity::new(root).expect("a root"));
        let canon = |p: &std::path::Path| std::fs::canonicalize(p).expect("a real dir");

        state.move_hands("kimi", Some(engine_at(moved.path())), moved.path());
        state.provision_engine("kimi", || engine_at(stale.path()));
        let root = state.file_engines.get("kimi").map(|e| canon(&e.workspace_root())).expect("an engine");
        assert_eq!(root, canon(moved.path()), "the mover's engine stands; the stale computed one is dropped");

        let who = "kimi#context";
        state.move_hands(who, Some(engine_at(moved.path())), moved.path());
        let old = state.hands_locks.get(who).map(|l| l.clone()).expect("the key has a lock");
        state.finish_release(who);
        assert!(*old.lock().unwrap(), "a waiter on the released lock wakes to find it closed");
        assert!(!state.hands_locks.contains_key(who), "the released key's lock is unlisted");
        state.holding(who, |closed| assert!(!*closed, "the next hold takes a fresh, open lock"));
        assert!(!state.file_engines.contains_key(who), "and the released hands are gone");
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
