//! The persona's HANDS as typed commands on the ONE registry.
//!
//! These are the file-operation tools (`code/read`, `code/search`, `code/edit`,
//! …) reimplemented as [`ActionCommand`]s so they live on the SINGLE command
//! surface every interface reads — the persona tool surface
//! ([`authorized_tool_specs`](crate::cognition::persona_tools::authorized_tool_specs)),
//! the grid ACL ([`ai_safe_commands`](crate::modules::grid::acl)), codegen, and
//! runtime dispatch — all from ONE `CommandSpec` declaration per command. No
//! second list, no parallel allow-table. A command becomes a persona tool simply
//! by being an `AiSafe` `ActionCommand`; the descriptor flows everywhere.
//!
//! ## Why this replaces the legacy `code/*` match arms
//!
//! The old surface dispatched these through `CodeModule::handle_command`'s
//! stringly `match` — invisible to `command_registry()`, so a persona was never
//! OFFERED them (it saw the ~3 commands that had migrated). That is the disjoint
//! two-registry tragedy: the real commands existed but the persona couldn't see
//! them. Declaring each as an `ActionCommand` + `register_command!` puts its typed
//! descriptor (name + schema + access + description) into the one registry, and
//! [`CodeModule::commands`](super::code::CodeModule) returns the dep-holding
//! object so the executor routes the name straight to it (winning over the legacy
//! prefix arm, per [`route_object`](crate::runtime::registry)). The match arms for
//! the migrated commands are then deleted — one path, one source of truth.
//!
//! ## Identity is the AUTHENTICATED caller, never a param
//!
//! The legacy arms read `persona_id` from the request body — an identity-axis
//! violation (any caller could name any persona) and an ergonomic tax (the model
//! had to pass its own id). Here the workspace is scoped to `ctx.caller.peer_id`
//! — the airc-verified identity the executor already gated on. A persona acts as
//! ITSELF by construction; there is nothing to spoof. (`None` caller = the
//! substrate-local owner.) This is the encapsulate-airc-identity principle applied
//! at the tool boundary: the peer_id is airc's, not a continuum-minted param.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::code::CodeState;
use crate::code::shell_types::{ShellExecuteResponse, ShellExecutionStatus};
use crate::code::types::{
    ExistsResult, GlobResult, ListResult, ReadResult, SearchMatch, SearchResult, TreeResult,
    WriteResult,
};
use crate::code::{search, tree, EditMode, FileEngine, PathSecurity, ShellSession};
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx, DynCommand};

/// The persona/owner this tool call acts AS — the authenticated caller identity
/// (an airc `peer_id`), never a params field. `None` caller is the
/// substrate-local owner. This is the single point that maps the gated identity
/// to the per-caller workspace; nothing trusts caller-supplied identity.
fn caller_id(ctx: &Ctx) -> String {
    ctx.caller
        .as_ref()
        .map(|c| c.peer_id.to_string())
        .unwrap_or_else(|| "local-owner".to_string())
}

/// Lazily ensure a workspace [`FileEngine`] exists for `who`, rooted at the repo
/// (the core's current working dir) so a persona reads/edits the project like any
/// other peer — no separate `create-workspace` step the model must know to call.
/// Idempotent and defers to an engine a prior call already created (so an explicit
/// `create-workspace` with a specific root still wins). Per-caller `FileEngine`s
/// keep each peer's change-DAG isolated while sharing the repo, exactly like
/// multiple editor tabs.
fn ensure_engine(state: &CodeState, who: &str) -> Result<(), CommandError> {
    if state.file_engines.contains_key(who) {
        return Ok(());
    }
    let root = std::env::current_dir()
        .map_err(|e| CommandError::Internal(format!("workspace root unavailable: {e}")))?;
    let security = PathSecurity::new(&root)
        .map_err(|e| CommandError::Internal(format!("workspace security init failed: {e}")))?;
    state
        .file_engines
        .entry(who.to_string())
        .or_insert_with(|| FileEngine::new(who, security));
    Ok(())
}

/// Lazily ensure a persistent shell session exists for `who`, rooted at the repo
/// (cwd). Idempotent; one bash session per caller, reused across `code/shell`
/// calls so `cd`/env persist like a real terminal.
fn ensure_shell(state: &CodeState, who: &str) -> Result<(), CommandError> {
    if state.shell_sessions.contains_key(who) {
        return Ok(());
    }
    let root = std::env::current_dir()
        .map_err(|e| CommandError::Internal(format!("shell root unavailable: {e}")))?;
    let session_id = uuid::Uuid::new_v4().to_string();
    let shell = ShellSession::new(&session_id, who, &root)
        .map_err(|e| CommandError::Internal(format!("shell init failed: {e}")))?;
    state.shell_sessions.entry(who.to_string()).or_insert(shell);
    Ok(())
}

/// How long `code/shell` waits INLINE for completion before handing back a handle
/// (the execution_id) so the caller polls instead of blocking. Bounded so a
/// long-running command — a build, a test suite, a TRAINING run, a daemon — NEVER
/// forces a promise that blocks the turn or times out. The command keeps running;
/// drive it with `code/shell-poll` / `code/shell-kill`. The model can override.
const DEFAULT_SHELL_WAIT_MS: u64 = 30_000;

/// Resolve the caller's engine, provisioning it on first use. Returned as a
/// `DashMap` ref guard the caller borrows the engine through.
macro_rules! engine {
    ($self:ident, $ctx:ident) => {{
        let who = caller_id($ctx);
        ensure_engine(&$self.state, &who)?;
        $self
            .state
            .file_engines
            .get(&who)
            .ok_or_else(|| CommandError::Internal("workspace vanished after provisioning".into()))?
    }};
}

// ─────────────────────────── code/read ───────────────────────────

/// Read a file (optionally a line range).
pub struct CodeRead {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeReadParams {
    /// Path to the file, relative to the workspace (repo) root.
    pub file_path: String,
    /// 1-indexed first line to return (inclusive). Omit to read from the start.
    #[serde(default)]
    pub start_line: Option<u32>,
    /// 1-indexed last line to return (inclusive). Omit to read to the end.
    #[serde(default)]
    pub end_line: Option<u32>,
}

#[async_trait]
impl ActionCommand for CodeRead {
    const NAME: &'static str = "code/read";
    const DESCRIPTION: &'static str =
        "Read a file from the workspace, optionally a line range. Returns content plus line metadata.";
    type Params = CodeReadParams;
    type Output = ReadResult;

    async fn run(&self, ctx: &Ctx, p: CodeReadParams) -> Result<ReadResult, CommandError> {
        let engine = engine!(self, ctx);
        engine
            .read(&p.file_path, p.start_line, p.end_line)
            .map_err(|e| CommandError::Internal(e.to_string()))
    }
}

// ─────────────────────────── code/write ──────────────────────────

/// Create or overwrite a file with new content.
pub struct CodeWrite {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeWriteParams {
    /// Path to the file, relative to the workspace (repo) root.
    pub file_path: String,
    /// Full new file contents.
    pub content: String,
    /// Optional note describing the change (recorded in the change history).
    #[serde(default)]
    pub description: Option<String>,
}

#[async_trait]
impl ActionCommand for CodeWrite {
    const NAME: &'static str = "code/write";
    const DESCRIPTION: &'static str =
        "Create or overwrite a file with new content. Tracked in the change history (undoable).";
    type Params = CodeWriteParams;
    type Output = WriteResult;

    async fn run(&self, ctx: &Ctx, p: CodeWriteParams) -> Result<WriteResult, CommandError> {
        let engine = engine!(self, ctx);
        engine
            .write(&p.file_path, &p.content, p.description.as_deref())
            .map_err(|e| CommandError::Internal(e.to_string()))
    }
}

// ─────────────────────────── code/edit ───────────────────────────

/// Edit an existing file (line-range / search-replace / insert / append).
pub struct CodeEdit {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeEditParams {
    /// Path to the file, relative to the workspace (repo) root.
    pub file_path: String,
    /// How to edit: line-range replace, search/replace, insert-at, or append.
    pub edit_mode: EditMode,
    /// Optional note describing the change (recorded in the change history).
    #[serde(default)]
    pub description: Option<String>,
}

#[async_trait]
impl ActionCommand for CodeEdit {
    const NAME: &'static str = "code/edit";
    const DESCRIPTION: &'static str =
        "Edit an existing file: line-range replace, search/replace, insert-at, or append. Undoable.";
    type Params = CodeEditParams;
    type Output = WriteResult;

    async fn run(&self, ctx: &Ctx, p: CodeEditParams) -> Result<WriteResult, CommandError> {
        let engine = engine!(self, ctx);
        engine
            .edit(&p.file_path, &p.edit_mode, p.description.as_deref())
            .map_err(|e| CommandError::Internal(e.to_string()))
    }
}

// ─────────────────────────── code/list ───────────────────────────

/// List a directory (flat, non-recursive).
pub struct CodeList {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeListParams {
    /// Directory to list, relative to the workspace root. Defaults to "." (root).
    #[serde(default)]
    pub path: Option<String>,
    /// Include hidden entries (dotfiles, `.git`, …). Defaults to false.
    #[serde(default)]
    pub include_hidden: Option<bool>,
}

#[async_trait]
impl ActionCommand for CodeList {
    const NAME: &'static str = "code/list";
    const DESCRIPTION: &'static str =
        "List a directory (flat, non-recursive): names, kinds, and sizes. Use code/tree for recursion.";
    type Params = CodeListParams;
    type Output = ListResult;

    async fn run(&self, ctx: &Ctx, p: CodeListParams) -> Result<ListResult, CommandError> {
        let engine = engine!(self, ctx);
        engine
            .list_dir(p.path.as_deref().unwrap_or("."), p.include_hidden.unwrap_or(false))
            .map_err(|e| CommandError::Internal(e.to_string()))
    }
}

// ─────────────────────────── code/exists ─────────────────────────

/// Check whether a path exists and what kind of entry it is.
pub struct CodeExists {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeExistsParams {
    /// Path to probe, relative to the workspace root.
    pub file_path: String,
}

#[async_trait]
impl ActionCommand for CodeExists {
    const NAME: &'static str = "code/exists";
    const DESCRIPTION: &'static str =
        "Check whether a path exists and its kind (file / directory / symlink) in one roundtrip.";
    type Params = CodeExistsParams;
    type Output = ExistsResult;

    async fn run(&self, ctx: &Ctx, p: CodeExistsParams) -> Result<ExistsResult, CommandError> {
        let engine = engine!(self, ctx);
        engine
            .exists(&p.file_path)
            .map_err(|e| CommandError::Internal(e.to_string()))
    }
}

// ─────────────────────────── code/glob ───────────────────────────

/// Find files by glob pattern.
pub struct CodeGlob {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeGlobParams {
    /// Glob pattern (e.g. `src/**/*.rs`).
    pub pattern: String,
    /// Optional sub-root to scope the glob, relative to the workspace root.
    #[serde(default)]
    pub root: Option<String>,
}

#[async_trait]
impl ActionCommand for CodeGlob {
    const NAME: &'static str = "code/glob";
    const DESCRIPTION: &'static str =
        "Find files by glob pattern (e.g. src/**/*.rs). Returns matching workspace-relative paths.";
    type Params = CodeGlobParams;
    type Output = GlobResult;

    async fn run(&self, ctx: &Ctx, p: CodeGlobParams) -> Result<GlobResult, CommandError> {
        let engine = engine!(self, ctx);
        engine
            .glob_match(&p.pattern, p.root.as_deref())
            .map_err(|e| CommandError::Internal(e.to_string()))
    }
}

// ─────────────────────────── code/tree ───────────────────────────

/// Print a recursive directory tree.
pub struct CodeTree {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeTreeParams {
    /// Subdirectory to root the tree at, relative to the workspace root. Defaults
    /// to the workspace root.
    #[serde(default)]
    pub path: Option<String>,
    /// Maximum recursion depth. Defaults to 10.
    #[serde(default)]
    pub max_depth: Option<u32>,
    /// Include hidden entries. Defaults to false.
    #[serde(default)]
    pub include_hidden: Option<bool>,
}

#[async_trait]
impl ActionCommand for CodeTree {
    const NAME: &'static str = "code/tree";
    const DESCRIPTION: &'static str =
        "Print a recursive directory tree (bounded depth) — the project's structure at a glance.";
    type Params = CodeTreeParams;
    type Output = TreeResult;

    async fn run(&self, ctx: &Ctx, p: CodeTreeParams) -> Result<TreeResult, CommandError> {
        let engine = engine!(self, ctx);
        // Resolve the target across searchable roots (workspace + any read roots),
        // falling back to the workspace root joined with the requested path.
        let target = match &p.path {
            Some(rel) => engine
                .searchable_roots()
                .into_iter()
                .map(|r| r.join(rel))
                .find(|c| c.is_dir())
                .unwrap_or_else(|| engine.workspace_root().join(rel)),
            None => engine.workspace_root(),
        };
        Ok(tree::generate_tree(
            &target,
            p.max_depth.unwrap_or(10),
            p.include_hidden.unwrap_or(false),
        ))
    }
}

// ─────────────────────────── code/search ─────────────────────────

/// Search file contents for a pattern (grep across the workspace).
pub struct CodeSearch {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeSearchParams {
    /// Text/regex pattern to search for.
    pub pattern: String,
    /// Optional glob to restrict which files are searched (e.g. `*.rs`).
    #[serde(default)]
    pub file_glob: Option<String>,
    /// Cap on the number of matches returned. Defaults to 100.
    #[serde(default)]
    pub max_results: Option<u32>,
}

#[async_trait]
impl ActionCommand for CodeSearch {
    const NAME: &'static str = "code/search";
    const DESCRIPTION: &'static str =
        "Search file contents for a pattern across the workspace (grep). Returns file:line matches.";
    type Params = CodeSearchParams;
    type Output = SearchResult;

    async fn run(&self, ctx: &Ctx, p: CodeSearchParams) -> Result<SearchResult, CommandError> {
        let engine = engine!(self, ctx);
        let max = p.max_results.unwrap_or(100);

        // Search every searchable root (workspace + read-only roots), merging and
        // de-duplicating by (file, line) since roots may overlap.
        let mut matches: Vec<SearchMatch> = Vec::new();
        let mut total_matches = 0u32;
        let mut files_searched = 0u32;
        for root in engine.searchable_roots() {
            let remaining = max.saturating_sub(matches.len() as u32);
            if remaining == 0 {
                break;
            }
            let r = search::search_files(&root, &p.pattern, p.file_glob.as_deref(), remaining);
            total_matches += r.total_matches;
            files_searched += r.files_searched;
            matches.extend(r.matches);
        }
        matches.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then(a.line_number.cmp(&b.line_number))
        });
        matches.dedup_by(|a, b| a.file_path == b.file_path && a.line_number == b.line_number);
        matches.truncate(max as usize);

        Ok(SearchResult {
            success: true,
            matches,
            total_matches,
            files_searched,
            error: None,
        })
    }
}

// ─────────────────────────── code/shell ──────────────────────────

/// Run a shell command in the caller's persistent bash session (the agentic
/// primitive — build, test, git, training, a daemon). `Privileged` → the Trusted
/// tier: a local persona or trusted node may run it; a remote `Provisional` peer
/// may NOT.
///
/// HANDLE-BASED, not a forced promise: the command starts immediately and is
/// waited on INLINE only up to `wait_ms`. If it finishes in that window the full
/// result comes back; if not, the response carries `status: running` + the
/// `execution_id` HANDLE and the command keeps running — poll it with
/// `code/shell-poll`, stop it with `code/shell-kill`. A long build / test suite /
/// training run / daemon therefore never blocks the turn or fails as a timed-out
/// promise (the bug that dumbed down long-running work before).
pub struct CodeShell {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeShellParams {
    /// The shell command line to run (bash), e.g. `cargo check` or `git status`.
    pub cmd: String,
    /// How long to wait INLINE for completion before returning the execution_id
    /// handle to poll. Defaults to 30000 (30s). A long job keeps running past this.
    #[serde(default)]
    pub wait_ms: Option<u64>,
    /// Optional HARD kill timeout (ms) for the command itself. Default: none — a
    /// long-running command (training, a daemon) runs until it finishes or you
    /// `code/shell-kill` it. Set this only when you want the command force-stopped.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// Snapshot an execution's current state into the wire response. Used both when a
/// command COMPLETES within the inline window and when it's still RUNNING (handle
/// handed back with partial output + `exit_code: None`).
fn shell_response(s: &crate::code::shell_session::ExecutionState) -> ShellExecuteResponse {
    ShellExecuteResponse {
        execution_id: s.id.clone(),
        status: s.status.clone(),
        stdout: Some(s.stdout_lines.join("\n")),
        stderr: Some(s.stderr_lines.join("\n")),
        exit_code: s.exit_code,
    }
}

#[async_trait]
impl ActionCommand for CodeShell {
    const NAME: &'static str = "code/shell";
    // Privileged → Trusted tier: arbitrary execution is for high-trust local
    // citizens (a local persona / a trusted node), never a Provisional remote peer.
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Run a shell command (bash) in your persistent workspace session. Waits inline up to \
         wait_ms (default 30s); if it finishes you get stdout/stderr/exit_code, otherwise you get \
         status=running + an execution_id to poll with code/shell-poll. Use for build/test/git/etc.";
    type Params = CodeShellParams;
    type Output = ShellExecuteResponse;

    async fn run(&self, ctx: &Ctx, p: CodeShellParams) -> Result<ShellExecuteResponse, CommandError> {
        let who = caller_id(ctx);
        ensure_shell(&self.state, &who)?;

        // Start the command while briefly holding the shell entry, then DROP the
        // DashMap ref before awaiting — never hold a lock across `.await` (the
        // bounded wait below blocks the turn, not the shard). [[concurrency-style-guide]]
        let state_arc = {
            let mut shell = self
                .state
                .shell_sessions
                .get_mut(&who)
                .ok_or_else(|| CommandError::Internal("shell vanished after provisioning".into()))?;
            let exec_id = shell
                .execute(&p.cmd, p.timeout_ms, &self.state.rt_handle)
                .map_err(CommandError::Internal)?;
            shell
                .get_execution_state(&exec_id)
                .ok_or_else(|| CommandError::Internal("execution vanished".into()))?
        };

        // BOUNDED inline wait: return the moment it completes, or hand back the
        // handle when the window elapses — never block past wait_ms. No DashMap
        // lock held across the await.
        let deadline = Instant::now() + Duration::from_millis(p.wait_ms.unwrap_or(DEFAULT_SHELL_WAIT_MS));
        loop {
            let notify = {
                let s = state_arc
                    .lock()
                    .map_err(|e| CommandError::Internal(format!("execution lock poisoned: {e}")))?;
                if s.status != ShellExecutionStatus::Running {
                    return Ok(shell_response(&s)); // completed in-window
                }
                s.output_notify.clone()
            };
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                let s = state_arc
                    .lock()
                    .map_err(|e| CommandError::Internal(format!("execution lock poisoned: {e}")))?;
                return Ok(shell_response(&s)); // still running → hand back the handle
            }
            // Wake on new output or when the inline window closes, then re-check.
            let _ = tokio::time::timeout(remaining, notify.notified()).await;
        }
    }
}

// ─────────────────────────── code/shell-poll ─────────────────────

/// Poll a running (or finished) shell execution by its handle — the read half of
/// the handle-based long-running contract.
pub struct CodeShellPoll {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeShellPollParams {
    /// The execution_id handle returned by `code/shell`.
    pub execution_id: String,
}

#[async_trait]
impl ActionCommand for CodeShellPoll {
    const NAME: &'static str = "code/shell-poll";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Poll a shell execution by its execution_id handle: current status, accumulated \
         stdout/stderr, and exit_code once finished. The non-blocking way to follow a long command.";
    type Params = CodeShellPollParams;
    type Output = ShellExecuteResponse;

    async fn run(&self, ctx: &Ctx, p: CodeShellPollParams) -> Result<ShellExecuteResponse, CommandError> {
        let who = caller_id(ctx);
        let shell = self
            .state
            .shell_sessions
            .get(&who)
            .ok_or_else(|| CommandError::NotFound("no shell session for caller".into()))?;
        let state_arc = shell
            .get_execution_state(&p.execution_id)
            .ok_or_else(|| CommandError::NotFound(format!("no execution {}", p.execution_id)))?;
        let s = state_arc
            .lock()
            .map_err(|e| CommandError::Internal(format!("execution lock poisoned: {e}")))?;
        Ok(shell_response(&s))
    }
}

// ─────────────────────────── code/shell-kill ─────────────────────

/// Terminate a running shell execution by its handle.
pub struct CodeShellKill {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeShellKillParams {
    /// The execution_id handle returned by `code/shell`.
    pub execution_id: String,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CodeShellKillResult {
    pub killed: bool,
}

#[async_trait]
impl ActionCommand for CodeShellKill {
    const NAME: &'static str = "code/shell-kill";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Terminate a running shell execution by its execution_id handle.";
    type Params = CodeShellKillParams;
    type Output = CodeShellKillResult;

    async fn run(&self, ctx: &Ctx, p: CodeShellKillParams) -> Result<CodeShellKillResult, CommandError> {
        let who = caller_id(ctx);
        let shell = self
            .state
            .shell_sessions
            .get(&who)
            .ok_or_else(|| CommandError::NotFound("no shell session for caller".into()))?;
        shell
            .kill(&p.execution_id)
            .map_err(CommandError::Internal)?;
        Ok(CodeShellKillResult { killed: true })
    }
}

// ─────────────────── one registry: descriptors + objects ─────────────────

// Static descriptors → the ONE `command_registry()` the persona surface + grid
// ACL + codegen read. Each is an `AiSafe` `ActionCommand`, so it auto-appears in
// `authorized_tool_specs(Provisional)` with its real schema + description.
crate::register_command!(CodeRead);
crate::register_command!(CodeWrite);
crate::register_command!(CodeEdit);
crate::register_command!(CodeList);
crate::register_command!(CodeExists);
crate::register_command!(CodeGlob);
crate::register_command!(CodeTree);
crate::register_command!(CodeSearch);
crate::register_command!(CodeShell);
crate::register_command!(CodeShellPoll);
crate::register_command!(CodeShellKill);

/// The dep-holding command objects the [`CodeModule`](super::code::CodeModule)
/// contributes to the kernel's typed object map (via `ServiceModule::commands`),
/// so the executor routes each name DIRECTLY to it — winning over the legacy
/// prefix → `handle_command` arm, which is deleted for these commands.
pub fn command_objects(state: Arc<CodeState>) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(CodeRead { state: state.clone() }),
        Arc::new(CodeWrite { state: state.clone() }),
        Arc::new(CodeEdit { state: state.clone() }),
        Arc::new(CodeList { state: state.clone() }),
        Arc::new(CodeExists { state: state.clone() }),
        Arc::new(CodeGlob { state: state.clone() }),
        Arc::new(CodeTree { state: state.clone() }),
        Arc::new(CodeSearch { state: state.clone() }),
        Arc::new(CodeShell { state: state.clone() }),
        Arc::new(CodeShellPoll { state: state.clone() }),
        Arc::new(CodeShellKill { state }),
    ]
}
