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
    DirEntry, ExistsResult, FsEntryKind, GlobResult, ListResult, ReadResult, SearchMatch,
    SearchResult, TreeResult,
    WriteResult,
};
use crate::code::{search, tree, EditMode, FileEngine, PathSecurity, ShellSession};
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx, DynCommand};

/// The persona/owner this tool call acts AS — the authenticated caller identity
/// (an airc `peer_id`), never a params field. `None` caller is the
/// substrate-local owner. This is the single point that maps the gated identity
/// to the per-caller workspace; nothing trusts caller-supplied identity.
pub(crate) fn caller_id(ctx: &Ctx) -> String {
    ctx.caller
        .as_ref()
        .map(|c| c.peer_id.to_string())
        .unwrap_or_else(|| LOCAL_OWNER.to_string())
}

/// The caller id assigned when a command arrives with NO peer identity — the
/// substrate-local operator (cu CLI, boot plumbing). The ONE caller whose
/// workspace is the core's own cwd; every identified peer gets a layer.
pub(crate) const LOCAL_OWNER: &str = "local-owner";

/// Resolve (and provision on first use) the citizen LAYER for an identified peer
/// caller: a copy-on-write clone of the shared base (the core's cwd — the repo)
/// under `<continuum home>/citizens/peers/<peer>/workspace`.
///
/// Joel 2026-07-10: "each persona is only the diff from the shared." On APFS
/// `cp -c` clones via `clonefile(2)`, so the layer SHARES every block with the
/// base and physically stores only what the peer later modifies. It is a real
/// directory (shell, rustc, git all just work), but its marginal disk cost is
/// the diff — and `git diff` inside it against the base branch IS the
/// publishable delta the peer shares over the mesh. The layer is durable
/// citizen state: an existing layer is reused across reboots. Non-CoW
/// filesystems fail LOUD rather than silently eating a full copy per peer
/// ([[fallbacks-are-illegal-fail-loud]]).
///
/// Why this exists (glass-boxed 2026-07-10): all personas shared the core's cwd
/// — OUR checkout — and one persona's misdirected `[dependencies]` edit replaced
/// the root workspace manifest, breaking every build path. Isolation of WRITES
/// with full collaboration over the mesh (chat, board, diffs) is the airc
/// model; a shared mutable checkout never was.
/// The citizen-layer path for a peer — pure path computation, NO provisioning.
/// The read-only sibling of [`ensure_citizen_layer`]: grounding sources (e.g.
/// the workspace map) need to know WHERE a peer's workspace is without triggering
/// a CoW clone — the clone is the hands' job, on first write. `<continuum home>/
/// citizens/peers/<peer>/workspace`, keyed by the peer's `peer_id.to_string()`
/// exactly as [`caller_id`] forms it.
pub(crate) fn citizen_layer_path(peer: &str) -> Result<std::path::PathBuf, CommandError> {
    let home = std::env::var("CONTINUUM_HOME")
        .map(std::path::PathBuf::from)
        .ok()
        .or_else(|| dirs::home_dir().map(|h| h.join(".continuum")))
        .ok_or_else(|| CommandError::Internal("no home dir for citizen layer".into()))?;
    Ok(home.join("citizens").join("peers").join(peer).join("workspace"))
}

pub(crate) fn ensure_citizen_layer(peer: &str) -> Result<std::path::PathBuf, CommandError> {
    // The shared base is the core's cwd — OUR checkout. Resolve it ONCE here, then
    // hand the provisioning body a concrete base so it is a pure function of
    // (peer, base). That seam is why the citizen-layer test can inject a fake base
    // WITHOUT mutating the process-global cwd: a test that chdir'd raced every
    // parallel ts-rs `export_bindings_*` write (each resolves its relative
    // `export_to` against the same process cwd, so a mid-run chdir sent those
    // writes to `/protocol/...` and panicked the whole suite). #191.
    let base = std::env::current_dir()
        .map_err(|e| CommandError::Internal(format!("shared base unavailable: {e}")))?;
    ensure_citizen_layer_from_base(peer, &base)
}

/// Provision (or refresh) a peer's citizen layer, cloning from `base` — the shared
/// checkout. Split out from [`ensure_citizen_layer`] so the base is an explicit
/// argument (production passes the core's cwd; tests pass a scoped temp dir) rather
/// than an implicit read of the process-global cwd. See the note above on why that
/// matters for the parallel test suite.
fn ensure_citizen_layer_from_base(
    peer: &str,
    base: &std::path::Path,
) -> Result<std::path::PathBuf, CommandError> {
    let layer = citizen_layer_path(peer)?;
    if layer.is_dir() {
        // Self-heal the stale-clone bug ([[citizen-workspaces-are-stale-one-time-
        // clones]]): the layer was a ONE-TIME CoW snapshot that never refreshed, so
        // personas drifted stale (one froze before the workers/→core/ restructure).
        // Now, on every ensure (≈once per persona per boot), sync it forward from
        // the current shared checkout — PRESERVING the persona's work (the sync
        // autocommits + merges, shared wins framework conflicts). Non-fatal: a sync
        // that fails logs LOUD but still returns the usable (if stale) workspace
        // rather than denying the persona her hands.
        match crate::code::git_bridge::git_sync_from_shared(&layer, base) {
            Ok(report) if report.synced => {
                write_workspace_sync_note(&layer, &report.summary);
                crate::probe!(
                    class = "workspace.layer.sync",
                    peer = %peer,
                    summary = %report.summary,
                    "citizen workspace refreshed from shared — persona work preserved"
                );
            }
            Ok(_) => {} // already current — nothing to notify.
            Err(e) => tracing::warn!(
                peer = %peer,
                error = %e,
                "citizen workspace sync from shared failed — persona continues on existing workspace"
            ),
        }
        return Ok(layer);
    }
    let parent = layer
        .parent()
        .ok_or_else(|| CommandError::Internal("citizen layer has no parent".into()))?;
    std::fs::create_dir_all(parent)
        .map_err(|e| CommandError::Internal(format!("citizen layer mkdir failed: {e}")))?;
    let started = std::time::Instant::now();
    // Copy-on-write clone of the shared base → the peer's layer via the platform's
    // reflink-capable `cp`. macOS APFS uses `-c` (clonefile); GNU coreutils uses
    // `--reflink=auto` — reflink where the filesystem supports it (btrfs/XFS), a plain
    // recursive copy otherwise, never failing for lack of CoW. `-cR` is macOS-ONLY (GNU
    // cp rejects `-c`), which silently broke citizen provisioning on every Linux deploy
    // until #191 surfaced it. `cfg!` (not `#[cfg]`) so BOTH branches compile and
    // type-check on every platform — a Linux-only arm must never escape a macOS build.
    let mut clone = std::process::Command::new("cp");
    if cfg!(target_os = "macos") {
        clone.arg("-cR");
    } else {
        clone.args(["--reflink=auto", "-R"]);
    }
    let out = clone
        .arg(base)
        .arg(&layer)
        .output()
        .map_err(|e| CommandError::Internal(format!("citizen layer clone spawn failed: {e}")))?;
    if !out.status.success() {
        // Never leave a half-materialized layer for the next call to mistake
        // for a real one.
        let _ = std::fs::remove_dir_all(&layer);
        return Err(CommandError::Internal(format!(
            "citizen layer CoW clone failed for peer {peer} (base {}): {}. \
             Copy-on-write is the intent (APFS clonefile on macOS, reflink on Linux \
             btrfs/XFS); cp falls back to a full recursive copy on a non-CoW filesystem \
             rather than failing, so a hard error here means the base is missing or \
             unreadable, not a missing reflink.",
            base.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    crate::probe!(
        class = "workspace.layer.provision",
        peer = %peer,
        base = %base.display(),
        layer = %layer.display(),
        elapsed_ms = started.elapsed().as_millis() as u64,
        "citizen layer provisioned — CoW clone of shared base (stores only the diff)"
    );
    Ok(layer)
}

/// Drop a short teaching note at the workspace root after a shared sync — the
/// "notify + teach" half of the self-heal (Joel 2026-07-13: "preserve and notify…
/// eventually just learn this"). The persona reads it through her normal code
/// tools and learns the workflow: her workspace is a live copy of shared,
/// refreshed each boot, and committing her work is how it survives. Best-effort —
/// a note we couldn't write never blocks her hands. This is the bootstrap of the
/// habit; the durable form is an onboarding lesson she internalizes
/// ([[onboarding-lora-tool-fluency-degree-system]]).
fn write_workspace_sync_note(layer: &std::path::Path, summary: &str) {
    let note = format!(
        "# Your workspace just synced with the shared codebase\n\n\
         {summary}.\n\n\
         This is YOUR copy-on-write workspace — it starts as a clone of the shared \
         project and is refreshed from it whenever the core restarts, so you always \
         work against current code. Your own changes live on top and are preserved \
         across syncs, but they're safest once committed. To keep your work:\n\n\
         ```\n\
         code/shell command=\"git add -A && git commit -m 'describe your change'\"\n\
         ```\n\n\
         Uncommitted edits are auto-saved before a sync, but committing yourself \
         (with a real message) keeps your history clean and your work clearly yours.\n"
    );
    let _ = std::fs::write(layer.join("WORKSPACE.md"), note);
}

/// Lazily ensure a workspace [`FileEngine`] exists for `who`. The local operator
/// (no peer identity) roots at the core's cwd — their own checkout. An
/// IDENTIFIED peer (persona or remote agent) roots at their citizen LAYER (see
/// [`ensure_citizen_layer`]): writes land in their own copy-on-write clone, and
/// the shared checkout is never writable through a peer's hands. Idempotent and
/// defers to an engine a prior call already created (so an explicit
/// `create-workspace` with a specific root still wins).
pub(crate) fn ensure_engine(state: &CodeState, who: &str) -> Result<(), CommandError> {
    if state.file_engines.contains_key(who) {
        return Ok(());
    }
    let root = if who == LOCAL_OWNER {
        std::env::current_dir()
            .map_err(|e| CommandError::Internal(format!("workspace root unavailable: {e}")))?
    } else {
        let citizen_root = ensure_citizen_layer(who)?;
        // Every citizen workspace is git-backed from birth
        // ([[workspace-is-a-cow-diff-from-shared-always-git]]): no-op when .git
        // exists; otherwise init + root commit, so diff→share→apply works the
        // moment a citizen first touches files. Glass-boxed 2026-07-11: three
        // parallel Conway implementations across un-versioned workspaces had no
        // consolidation path, and the team asked for one. Loud on failure — a
        // workspace that silently can't version work is a quiet defect.
        crate::code::git_bridge::git_init_if_needed(&citizen_root)
            .map_err(|e| CommandError::Internal(format!("workspace git init failed: {e}")))?;
        citizen_root
    };
    let security = PathSecurity::new(&root)
        .map_err(|e| CommandError::Internal(format!("workspace security init failed: {e}")))?;
    state
        .file_engines
        .entry(who.to_string())
        .or_insert_with(|| FileEngine::new(who, security));
    Ok(())
}

/// Lazily ensure a persistent shell session exists for `who`, rooted at the
/// caller's ENGINE root — the one workspace authority per caller (the operator's
/// cwd, a peer's citizen layer, or whatever `create-workspace` pinned). Never a
/// second independent cwd fallthrough: before this, a peer's shell rooted at the
/// core's cwd even when her file engine didn't, which is how narrated shell
/// commands landed in the shared checkout. Idempotent; one bash session per
/// caller, reused across `code/shell` calls so `cd`/env persist like a real
/// terminal.
fn ensure_shell(state: &CodeState, who: &str) -> Result<(), CommandError> {
    if state.shell_sessions.contains_key(who) {
        return Ok(());
    }
    ensure_engine(state, who)?;
    let root = state
        .file_engines
        .get(&who.to_string())
        .map(|e| e.workspace_root())
        .ok_or_else(|| CommandError::Internal("engine vanished after provisioning".into()))?;
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
    const ALIASES: &'static [&'static str] = &["read_file"];
    const NATIVE: bool = true; // core agentic working set — offered natively (auto-derived)
    const DESCRIPTION: &'static str =
        "Read a file from the workspace, optionally a line range. Content comes back with \
         each line NUMBERED (`   12 | text`) — those numbers are the same ones code/edit's \
         line_range and insert_at address, so you never have to count. The `N | ` gutter is \
         display, not file content: never write it back into a file.";
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
    const ALIASES: &'static [&'static str] = &["write_file"];
    const NATIVE: bool = true; // core agentic working set — offered natively (auto-derived)
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

/// The one true shape + the recovery guidance, kept in ONE place so the runtime error and
/// the field docs never drift.
const EDIT_MODE_HELP: &str = "edit_mode must be ONE of (preferred = a tagged object): \
{\"type\":\"search_replace\",\"search\":\"old text\",\"replace\":\"new text\"} | \
{\"type\":\"line_range\",\"start_line\":N,\"end_line\":M,\"new_content\":\"...\"} | \
{\"type\":\"insert_at\",\"line\":N,\"content\":\"...\"} | \
{\"type\":\"append\",\"content\":\"...\"}. A bare mode string (e.g. \"append\") with the fields at top level is also accepted.";

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeEditParams {
    /// Path to the file, relative to the workspace (repo) root.
    pub file_path: String,
    /// How to edit. Preferred: a tagged object like `{"type":"search_replace","search":"old","replace":"new"}`,
    /// `{"type":"line_range","start_line":N,"end_line":M,"new_content":"..."}`,
    /// `{"type":"insert_at","line":N,"content":"..."}`, or `{"type":"append","content":"..."}`.
    /// Forgiving: a bare mode string (e.g. `"append"`) with the fields at top level also works.
    #[serde(default)]
    #[ts(type = "any")]
    pub edit_mode: serde_json::Value,
    // Top-level convenience fields — accepted when edit_mode is given as a bare mode string
    // (or when the mode is inferred from which of these are present). Forgives the common
    // flat-call shape a model reaches for instead of the nested tagged object.
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub replace: Option<String>,
    #[serde(default)]
    pub new_content: Option<String>,
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub start_line: Option<u32>,
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub end_line: Option<u32>,
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub line: Option<u32>,
    #[serde(default)]
    pub all: Option<bool>,
    /// Optional note describing the change (recorded in the change history).
    #[serde(default)]
    pub description: Option<String>,
}

/// Normalize a (possibly mis-shaped) code/edit call into a real [`EditMode`]. Forgives the
/// ways a model mis-calls the tool — glass-boxed live: a 14B sent `edit_mode:"append"` (bare
/// variant string) with no nested fields. Order: (1) strict tagged object, unchanged; (2) a
/// bare mode string, or a mode INFERRED from which top-level fields are present; then build the
/// variant, failing LOUD and naming the missing field (never a silent no-op that scores a false
/// zero). [[px-persona-experience-tools-as-good-ux]] [[fallbacks-are-illegal-fail-loud]]
fn normalize_edit_mode(p: &CodeEditParams) -> Result<EditMode, CommandError> {
    // (1) Strict: already the tagged object — deserialize as-is (the preferred path, unchanged).
    if p.edit_mode.get("type").is_some() {
        return serde_json::from_value::<EditMode>(p.edit_mode.clone())
            .map_err(|e| CommandError::Invalid(format!("code/edit: {e}. {EDIT_MODE_HELP}")));
    }
    // A field pulled from top-level OR from an untyped edit_mode object (flat call shapes).
    let s = |top: &Option<String>, key: &str| -> Option<String> {
        top.clone()
            .or_else(|| p.edit_mode.get(key).and_then(|v| v.as_str().map(str::to_string)))
    };
    let content = s(&p.content, "content");
    let search = s(&p.search, "search");
    let replace = s(&p.replace, "replace");
    let new_content = s(&p.new_content, "new_content");
    // Numeric fields, pulled from top-level OR the nested untyped edit_mode object.
    // Live glass-box (2026-07-14): Devstral emitted `edit_mode:{end_line:65535,
    // new_content:"…"}` — the line numbers were NESTED, not top-level, so the
    // top-level-only read missed them and the edit failed. One extractor, both
    // placements. [[px-persona-experience-tools-as-good-ux]]
    let n = |top: Option<u32>, key: &str| -> Option<u32> {
        top.or_else(|| {
            p.edit_mode
                .get(key)
                .and_then(|v| v.as_u64())
                .map(|x| x.min(u32::MAX as u64) as u32)
        })
    };
    let start_line = n(p.start_line, "start_line");
    let end_line = n(p.end_line, "end_line");
    let line = n(p.line, "line");
    // (2) The mode name: a bare string, else inferred from which fields are present.
    let mode = p.edit_mode.as_str().map(str::to_string).or_else(|| {
        if search.is_some() || replace.is_some() {
            Some("search_replace".into())
        } else if start_line.is_some() || end_line.is_some() || new_content.is_some() {
            Some("line_range".into())
        } else if line.is_some() {
            Some("insert_at".into())
        } else if content.is_some() {
            Some("append".into())
        } else {
            None
        }
    });
    let miss = |what: &str| CommandError::Invalid(format!("code/edit: needs `{what}`. {EDIT_MODE_HELP}"));
    match mode.as_deref().map(|m| m.trim().to_lowercase()).as_deref() {
        Some("search_replace") => Ok(EditMode::SearchReplace {
            search: search.ok_or_else(|| miss("search"))?,
            replace: replace.ok_or_else(|| miss("replace"))?,
            all: p.all.unwrap_or(false),
        }),
        Some("append") => Ok(EditMode::Append {
            content: content.ok_or_else(|| miss("content"))?,
        }),
        Some("insert_at") => Ok(EditMode::InsertAt {
            line: line.ok_or_else(|| miss("line"))?,
            content: content.ok_or_else(|| miss("content"))?,
        }),
        Some("line_range") => Ok(EditMode::LineRange {
            // The reflexive "replace the file" shape a model emits is
            // `{new_content, end_line: 65535}` with NO start_line: it means
            // "from the top, to the end". Default start_line→1 and end_line→MAX
            // (apply_edit clamps MAX to EOF) so that intent LANDS instead of
            // missing a field and looping. new_content is the one truly required
            // field — without it there is nothing to write. [[fallbacks-are-illegal-fail-loud]]
            start_line: start_line.unwrap_or(1),
            end_line: end_line.unwrap_or(u32::MAX),
            new_content: new_content.ok_or_else(|| miss("new_content"))?,
        }),
        _ => Err(CommandError::Invalid(format!(
            "code/edit: could not determine the edit mode. {EDIT_MODE_HELP}"
        ))),
    }
}

/// Reject a `file_path` that is a PLACEHOLDER rather than a real path. Glass-boxed live: a 14B
/// wrote `file_path:"<path_to_blueprints.py>"` — it echoed the schema's angle-bracket placeholder
/// syntax (or a `path_to_X` template) instead of substituting the concrete path it had ALREADY
/// seen in its own search results. Fail LOUD and point it at where the real value lives, rather
/// than a bare "file not found" that reads as "wrong file" instead of "you passed a template".
fn reject_placeholder_path(file_path: &str) -> Result<(), CommandError> {
    let p = file_path.trim();
    let looks_placeholder = (p.starts_with('<') && p.ends_with('>'))
        || p.contains("path_to")
        || p.contains("/path/to/")
        || p.contains("your_file")
        || p.contains("<file")
        || p.contains("<path");
    if looks_placeholder {
        return Err(CommandError::Invalid(format!(
            "code/edit: file_path '{file_path}' is a PLACEHOLDER, not a real path. Use the concrete \
             workspace-relative path you saw in your code/search or code/glob results (e.g. \
             'src/flask/blueprints.py'), not a template."
        )));
    }
    Ok(())
}

#[async_trait]
impl ActionCommand for CodeEdit {
    const NAME: &'static str = "code/edit";
    const ALIASES: &'static [&'static str] = &["edit_file"];
    const NATIVE: bool = true; // core agentic working set — offered natively (auto-derived)
    const DESCRIPTION: &'static str =
        "Edit an existing file: line-range replace, search/replace, insert-at, or append. Undoable.";
    type Params = CodeEditParams;
    type Output = WriteResult;

    async fn run(&self, ctx: &Ctx, p: CodeEditParams) -> Result<WriteResult, CommandError> {
        reject_placeholder_path(&p.file_path)?;
        let engine = engine!(self, ctx);
        let mode = normalize_edit_mode(&p)?;
        engine
            .edit(&p.file_path, &mode, p.description.as_deref())
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
    const ALIASES: &'static [&'static str] = &["list_files"];
    const NATIVE: bool = true; // core agentic working set — offered natively (auto-derived)
    const DESCRIPTION: &'static str =
        "List a directory (flat, non-recursive): names, kinds, and sizes. Use code/tree for recursion.";
    type Params = CodeListParams;
    type Output = ListResult;

    async fn run(&self, ctx: &Ctx, p: CodeListParams) -> Result<ListResult, CommandError> {
        let engine = engine!(self, ctx);
        let requested = p.path.as_deref().unwrap_or(".");
        // Ergonomic recovery: models trained on SWE-agent scaffolds reflexively
        // reach for `list_files("**/*.rs")` — a recursive GLOB where code/list
        // wants a single directory. list_dir rightly fails that (it's a flat
        // lister), but a bare NotFound teaches nothing and the model retries the
        // same glob (mined: 38% of live code/list calls, every failure a glob).
        // Meet the idiom: when the path IS a glob, resolve it via code/glob and
        // project the matches back as a listing — honest (they asked to "list"
        // these files, they get them), and it widens the surface without a new
        // verb. [[px-persona-experience-tools-as-good-ux]]
        if looks_like_file_glob(requested) {
            let glob = engine
                .glob_match(requested, None)
                .map_err(|e| CommandError::Internal(e.to_string()))?;
            let mut result = list_result_from_glob(requested, glob);
            // A zero-match glob is the OTHER "empty workspace" confabulation source
            // (glass-boxed 2026-07-14: a persona ran `code/list(path=**/)` — `**/`
            // matches no FILES, so the glob returns empty and she concluded the
            // workspace was empty). Same teach as the miss path, carried in the note
            // field so the empty listing itself isn't an error: name the real layout +
            // point at code/tree for a recursive view.
            if result.entries.is_empty() {
                if let Some(dirs) = top_level_dir_names(&engine) {
                    result.error = Some(format!(
                        "no files matched '{requested}' — this is NOT an empty workspace. \
                         Top-level directories: {}. For a recursive view use code/tree; to \
                         list one directory use code/list <dir>; for files by extension try \
                         a glob like `**/*.rs`.",
                        dirs.join(", ")
                    ));
                }
            }
            return Ok(result);
        }
        match engine.list_dir(requested, p.include_hidden.unwrap_or(false)) {
            Ok(listing) => Ok(listing),
            // A miss on a NON-glob path is the "empty workspace" confabulation source
            // (glass-boxed 2026-07-13: a persona reached for `src/persona.rs` — no such
            // top-level `src` here — got a bare NotFound, and concluded "the workspace
            // is empty", a false belief that then RECALLED and reinforced). A bare error
            // teaches nothing; enumerate the ACTUAL top-level dirs at the root so the
            // persona sees the true layout at the point of the miss and self-corrects —
            // the same teach-on-miss pattern as the glob recovery above and id_resolve.
            // [[px-persona-experience-tools-as-good-ux]]
            Err(e) => Err(teach_layout_on_miss(&engine, requested, &e.to_string())),
        }
    }
}

/// Turn a `code/list` path miss into a teaching error that names the real top-level
/// layout — so a persona that guessed a nonexistent path (`src/persona.rs`) learns
/// what DOES exist instead of concluding the workspace is empty. Best-effort: if the
/// root itself can't be listed, fall back to the original error rather than inventing
/// a layout ([[fallbacks-are-illegal-fail-loud]] — this is enrichment, never a silent
/// swallow; the miss still fails).
fn teach_layout_on_miss(engine: &FileEngine, requested: &str, original: &str) -> CommandError {
    match top_level_dir_names(engine) {
        Some(dirs) if !dirs.is_empty() => CommandError::Invalid(format!(
            "'{requested}' not found in this workspace. Top-level directories here: {}. \
             Paths are relative to the root — don't assume source lives under `src/`; \
             pick one of these or drill in with code/list <dir>.",
            dirs.join(", ")
        )),
        Some(_) => CommandError::Invalid(format!(
            "'{requested}' not found and the workspace root has no directories ({original})"
        )),
        // Root itself couldn't be listed — never invent a layout, surface the real error.
        None => CommandError::Internal(original.to_string()),
    }
}

/// The sorted top-level directory names at the workspace root — the shared "what
/// actually exists here" fact behind every teach-on-miss ([`teach_layout_on_miss`]
/// and the zero-match-glob note). `None` only when the root itself can't be listed
/// (then the caller surfaces the real error rather than a fabricated layout).
fn top_level_dir_names(engine: &FileEngine) -> Option<Vec<String>> {
    let root = engine.list_dir(".", false).ok()?;
    let mut dirs: Vec<String> = root
        .entries
        .iter()
        .filter(|e| e.kind == FsEntryKind::Directory)
        .map(|e| e.name.clone())
        .collect();
    dirs.sort();
    Some(dirs)
}

/// Project a [`GlobResult`] into a [`ListResult`] so a glob passed to `code/list`
/// returns a listing of the matching files (the reflexive `list_files("**/*.rs")`
/// idiom). Each match is a File entry — `code/glob` yields files only — with size
/// left `None` (a glob can match thousands; per-entry stat would defeat the
/// bounded cost `code/list` promises). `directory_path` records the glob so the
/// persona sees WHY the listing is cross-directory.
fn list_result_from_glob(pattern: &str, glob: GlobResult) -> ListResult {
    let entries: Vec<DirEntry> = glob
        .matches
        .iter()
        .map(|path| DirEntry {
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            path: path.clone(),
            kind: FsEntryKind::File,
            size_bytes: None,
        })
        .collect();
    ListResult {
        success: glob.success,
        directory_path: format!("glob:{pattern}"),
        total_count: entries.len() as u32,
        entries,
        error: glob.error,
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
    /// Glob pattern. Use `**/` to recurse from the workspace root regardless
    /// of layout, e.g. `**/*.rs` matches Rust files anywhere in the tree.
    pub pattern: String,
    /// Optional sub-root to scope the glob, relative to the workspace root.
    #[serde(default)]
    pub root: Option<String>,
}

#[async_trait]
impl ActionCommand for CodeGlob {
    const NAME: &'static str = "code/glob";
    const DESCRIPTION: &'static str =
        "Find files by glob pattern (e.g. `**/*.rs` matches Rust files at any depth, layout-independent). Returns matching workspace-relative paths.";
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
    const ALIASES: &'static [&'static str] = &["file_tree"];
    const NATIVE: bool = true; // core agentic working set — offered natively (auto-derived)
    const DESCRIPTION: &'static str =
        "Print a recursive directory tree (bounded depth) — the project's structure at a glance.";
    type Params = CodeTreeParams;
    type Output = TreeResult;

    async fn run(&self, ctx: &Ctx, p: CodeTreeParams) -> Result<TreeResult, CommandError> {
        let engine = engine!(self, ctx);
        // ONE resolver for every directory-oriented command (FileEngine::resolve_dir):
        // idiom-forgiveness (redundant "workspace/" prefix, leading '/') and honest,
        // actionable errors live there, not hand-rolled here. No path → the whole tree.
        let target = engine
            .resolve_dir(p.path.as_deref().unwrap_or("."))
            .map_err(|e| CommandError::Invalid(e.to_string()))?;
        Ok(tree::generate_tree(
            &target,
            p.max_depth.unwrap_or(10),
            p.include_hidden.unwrap_or(false),
        ))
    }
}

// ─────────────────────────── code/search ─────────────────────────

/// Heuristic: does this string look like a FILE GLOB (a path pattern) rather than a
/// grep term/regex? Conservative — requires a `*`/`?` AND a path-glob shape (`/`, `**`,
/// or a leading `*.ext`) AND no whitespace — so a genuine content regex like `foo.*bar`
/// or `fn .*Params` is NOT misclassified, while `**/*.py`, `*.rs`, `src/**/*.js` are.
/// Used by code/search to auto-recover the common `pattern`↔`file_glob` conflation.
fn looks_like_file_glob(pattern: &str) -> bool {
    let p = pattern.trim();
    !p.is_empty()
        && !p.chars().any(char::is_whitespace)
        && (p.contains('*') || p.contains('?'))
        && (p.contains('/') || p.contains("**") || p.starts_with("*."))
}

/// Search file contents for a pattern (grep across the workspace).
pub struct CodeSearch {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeSearchParams {
    /// Text/regex pattern to search for.
    pub pattern: String,
    /// Optional glob to restrict which files are searched. Omit to search ALL
    /// files. Globs recurse from the workspace root and are layout-independent,
    /// e.g. `**/*.rs` (NOT `src/**/*.rs` — don't assume the code lives under src/).
    #[serde(default)]
    pub file_glob: Option<String>,
    /// Cap on the number of matches returned. Defaults to 100.
    #[serde(default)]
    pub max_results: Option<u32>,
}

#[async_trait]
impl ActionCommand for CodeSearch {
    const NAME: &'static str = "code/search";
    const ALIASES: &'static [&'static str] = &["grep"];
    const NATIVE: bool = true; // core agentic working set — offered natively (auto-derived)
    const DESCRIPTION: &'static str =
        "Search file contents for a pattern across the workspace (grep). Returns file:line matches.";
    type Params = CodeSearchParams;
    type Output = SearchResult;

    async fn run(&self, ctx: &Ctx, p: CodeSearchParams) -> Result<SearchResult, CommandError> {
        let engine = engine!(self, ctx);
        let max = p.max_results.unwrap_or(100);

        // FORGIVENESS: models routinely conflate code/search (grep file CONTENTS — `pattern`
        // is the search TERM, `file_glob` filters which files) with code/glob (find files BY
        // name). A glob-shaped `pattern` with no `file_glob` greps CONTENTS for the literal glob
        // text → 0 matches → the model retries the identical call forever (glass-boxed live: a
        // 14B fired `code/search(pattern="**/*.py")` 12× in a row, never advancing to read/edit).
        // If `pattern` looks like a file glob and no `file_glob` was given, do what it MEANT: list
        // the matching FILES (code/glob's job) and hand them back + a one-line note on the right
        // shape, so a single call makes progress instead of looping. [[px-persona-experience-tools-as-good-ux]]
        if p.file_glob.is_none() && looks_like_file_glob(&p.pattern) {
            let g = engine
                .glob_match(&p.pattern, None)
                .map_err(|e| CommandError::Internal(e.to_string()))?;
            let matches: Vec<SearchMatch> = g
                .matches
                .iter()
                .take(max as usize)
                .map(|path| SearchMatch {
                    file_path: path.clone(),
                    line_number: 0,
                    line_content: String::new(),
                    match_start: 0,
                    match_end: 0,
                })
                .collect();
            return Ok(SearchResult {
                success: true,
                total_matches: g.total_matches,
                files_searched: g.matches.len() as u32,
                matches,
                error: Some(format!(
                    "`pattern` {:?} looked like a FILE GLOB, so I listed matching FILES (that is \
                     code/glob's job). To search file CONTENTS instead, call code/search with a \
                     text/regex `pattern` (e.g. \"Blueprint\") PLUS `file_glob` {:?}. {} file(s) matched.",
                    p.pattern, p.pattern, g.total_matches
                )),
            });
        }

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

        // OVERFLOW SUMMARY: a wall of matches is not an answer. Glass-boxed on
        // SWE flask-4045 (2026-07-11): `pattern:"blueprint"` returned
        // total_matches:101 as a truncated line-by-line dump, and the solver —
        // told the truth but given no next step — re-issued the identical
        // search 9× instead of advancing to read/edit. No mind, human or model,
        // picks a next action from 101 raw hits; a human running grep eyeballs
        // the FILE distribution and opens the hottest file. So above the
        // threshold the result becomes that eyeball: one representative match
        // per file (concrete, carryable paths — the next-step affordance), plus
        // per-file counts and the narrow-or-read guidance in the note field.
        // Counts stay truthful; only the rendering compresses.
        // [[px-persona-experience-tools-as-good-ux]] (overflow → filter
        // suggestions), same forgiveness family as the glob-shaped-pattern and
        // empty-glob notes above.
        const OVERFLOW_MATCHES: usize = 25;
        let mut error = None;
        if matches.len() > OVERFLOW_MATCHES {
            let mut per_file: Vec<(String, u32, SearchMatch)> = Vec::new();
            for m in matches.drain(..) {
                match per_file.iter_mut().find(|(f, _, _)| *f == m.file_path) {
                    Some((_, n, _)) => *n += 1,
                    None => per_file.push((m.file_path.clone(), 1, m)),
                }
            }
            per_file.sort_by(|a, b| b.1.cmp(&a.1));
            let top: Vec<String> = per_file
                .iter()
                .take(10)
                .map(|(f, n, _)| format!("{f} ({n})"))
                .collect();
            let files_total = per_file.len();
            matches = per_file
                .into_iter()
                .take(10)
                .map(|(_, _, first)| first)
                .collect();
            error = Some(format!(
                "{total_matches} matches across {files_total} file(s) — too many to list \
                 line-by-line, so this shows ONE representative match per file for the top \
                 {} file(s) by match count: [{}]. Next: read the most relevant file \
                 (code/read file_path=...) or narrow the search (more specific `pattern`, \
                 or `file_glob` limiting which files).",
                matches.len(),
                top.join(", "),
            ));
        }

        // Grounding for an empty search: a glob that matched ZERO files almost
        // always means the caller guessed a wrong path prefix (observed live —
        // globbed "continuum-core/*" when the tree is "core/continuum-core/…",
        // then confabulated a fake "agent_loop.py" rather than recover). Hand back
        // the workspace root + its top-level entries so the caller can re-orient
        // from the real layout instead of inventing one. Pure grounding DATA via
        // the existing `error` field — not a behavior gate, not the answer; the
        // map, not the route. (CLAUDE.md AI-QA: make tool failures recoverable.)
        if error.is_none() && files_searched == 0 && p.file_glob.is_some() {
            let root = engine.workspace_root();
            let mut tops: Vec<String> = std::fs::read_dir(&root)
                .map(|rd| {
                    rd.flatten()
                        .filter(|e| e.path().is_dir())
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .filter(|n| !n.starts_with('.') && n != "target" && n != "node_modules")
                        .collect()
                })
                .unwrap_or_default();
            tops.sort();
            tops.truncate(24);
            error = Some(format!(
                "No files matched glob {:?}. Workspace root is {} and all paths are \
                 relative to it; its top-level directories are: [{}]. Re-check the \
                 glob against this actual layout.",
                p.file_glob.as_deref().unwrap_or(""),
                root.display(),
                tops.join(", "),
            ));
        }

        Ok(SearchResult {
            success: true,
            matches,
            total_matches,
            files_searched,
            error,
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
    const ALIASES: &'static [&'static str] = &["bash"];
    const NATIVE: bool = true; // core agentic working set — offered natively (auto-derived)
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

// ─────────────────────────── code/delete ─────────────────────────

/// Delete a file from the workspace (tracked in the change history, undoable).
pub struct CodeDelete {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeDeleteParams {
    /// Path to the file to delete, relative to the workspace (repo) root.
    pub file_path: String,
    /// Optional note describing why (recorded in the change history).
    #[serde(default)]
    pub description: Option<String>,
}

#[async_trait]
impl ActionCommand for CodeDelete {
    const NAME: &'static str = "code/delete";
    const DESCRIPTION: &'static str =
        "Delete a file from the workspace. Tracked in the change history (undoable via code/undo).";
    type Params = CodeDeleteParams;
    type Output = WriteResult;

    async fn run(&self, ctx: &Ctx, p: CodeDeleteParams) -> Result<WriteResult, CommandError> {
        let engine = engine!(self, ctx);
        engine
            .delete(&p.file_path, p.description.as_deref())
            .map_err(|e| CommandError::Internal(e.to_string()))
    }
}

// ─────────────────────────── code/diff ───────────────────────────

/// Preview the unified diff an edit WOULD produce, without applying it.
pub struct CodeDiff {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeDiffParams {
    /// Path to the file, relative to the workspace (repo) root.
    pub file_path: String,
    /// The edit to preview: line-range replace, search/replace, insert-at, or append.
    pub edit_mode: EditMode,
}

#[async_trait]
impl ActionCommand for CodeDiff {
    const NAME: &'static str = "code/diff";
    const DESCRIPTION: &'static str =
        "Preview the unified diff an edit would produce WITHOUT applying it — a dry run before code/edit.";
    type Params = CodeDiffParams;
    type Output = crate::code::types::FileDiff;

    async fn run(
        &self,
        ctx: &Ctx,
        p: CodeDiffParams,
    ) -> Result<crate::code::types::FileDiff, CommandError> {
        let engine = engine!(self, ctx);
        engine
            .preview_diff(&p.file_path, &p.edit_mode)
            .map_err(|e| CommandError::Internal(e.to_string()))
    }
}

// ─────────────────────────── code/undo ───────────────────────────

/// Undo a tracked change: a specific change by id, or the last N changes.
pub struct CodeUndo {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeUndoParams {
    /// Undo a SPECIFIC change by its id (from a WriteResult / code/history). Takes
    /// precedence over `count` when present.
    #[serde(default)]
    pub change_id: Option<String>,
    /// Undo the last N changes (default 1). Ignored when `change_id` is given.
    #[serde(default)]
    pub count: Option<u32>,
}

#[async_trait]
impl ActionCommand for CodeUndo {
    const NAME: &'static str = "code/undo";
    const DESCRIPTION: &'static str =
        "Undo tracked workspace changes: a specific change by change_id, or the last N (default 1). \
         Returns the reverting WriteResults.";
    type Params = CodeUndoParams;
    type Output = crate::code::types::UndoResult;

    async fn run(
        &self,
        ctx: &Ctx,
        p: CodeUndoParams,
    ) -> Result<crate::code::types::UndoResult, CommandError> {
        let engine = engine!(self, ctx);
        // By-id undo and last-N undo unify on UndoResult (the by-id legacy arm built
        // the same {success, changes_undone, error} shape ad-hoc; here it's typed).
        match p.change_id {
            Some(id) => {
                let change_uuid = uuid::Uuid::parse_str(&id)
                    .map_err(|e| CommandError::Invalid(format!("invalid change_id: {e}")))?;
                let reverted = engine
                    .undo(&change_uuid)
                    .map_err(|e| CommandError::Internal(e.to_string()))?;
                Ok(crate::code::types::UndoResult {
                    success: true,
                    changes_undone: vec![reverted],
                    error: None,
                })
            }
            None => engine
                .undo_last(p.count.unwrap_or(1) as usize)
                .map_err(|e| CommandError::Internal(e.to_string())),
        }
    }
}

// ─────────────────────────── code/history ────────────────────────

/// The change history for one file, or the whole workspace.
pub struct CodeHistory {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeHistoryParams {
    /// Scope to one file (relative to the workspace root). Omit for the whole
    /// workspace's history.
    #[serde(default)]
    pub file_path: Option<String>,
    /// Cap on the number of changes returned (most recent first). Defaults to 50.
    #[serde(default)]
    pub limit: Option<u32>,
}

#[async_trait]
impl ActionCommand for CodeHistory {
    const NAME: &'static str = "code/history";
    const DESCRIPTION: &'static str =
        "The change history (most recent first) for one file or the whole workspace — what changed, \
         when, and the change_ids to undo.";
    type Params = CodeHistoryParams;
    type Output = crate::code::types::HistoryResult;

    async fn run(
        &self,
        ctx: &Ctx,
        p: CodeHistoryParams,
    ) -> Result<crate::code::types::HistoryResult, CommandError> {
        let engine = engine!(self, ctx);
        let limit = p.limit.unwrap_or(50) as usize;
        Ok(match p.file_path {
            Some(fp) => engine.file_history(&fp, limit),
            None => engine.workspace_history(limit),
        })
    }
}

// ─────────────────── code/create-workspace ───────────────────

/// Establish (or re-root) the caller's workspace sandbox at a specific path.
pub struct CodeCreateWorkspace {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeCreateWorkspaceParams {
    /// Absolute path to the workspace root — the read/write sandbox boundary. All
    /// subsequent `code/*` file ops for this caller are confined to it.
    pub workspace_root: String,
    /// Additional read-only roots the caller may read from but not write to (e.g.
    /// a shared dependency tree). Omit for a write-only-within-root sandbox.
    #[serde(default)]
    pub read_roots: Vec<String>,
    /// Directories to PREPEND to `PATH` for this caller's shell.
    ///
    /// Why this exists: the SWE harness provisions an era-matched interpreter per
    /// instance (`uv venv --python 3.9|3.11`) and used it ONLY for grading. Her hands
    /// got the bare inherited PATH — so `python` did not exist for her at all.
    /// Glass-boxed on sympy-21379: she wrote a correct reproduction script, ran it,
    /// and got `bash: python: command not found` (exit 127). A persona who cannot
    /// EXECUTE cannot verify a fix, which makes the whole iterate-and-observe loop
    /// impossible on any Python repo — and silently scores it as a capability failure.
    ///
    /// Environment, not steering: it grants the interpreter the task already implies.
    #[serde(default)]
    pub path_prepend: Vec<String>,
    /// Harden this workspace for a SCORED run: an edit whose inserted code would land inside a
    /// string literal is REFUSED instead of warned (#317). Default `false` — a living citizen
    /// writes code as text all the time (docstring examples, fixtures, quoted snippets) and the
    /// warning on the success path already tells her it will not execute. Only a measurement,
    /// whose deliverable IS an executing patch, has no such ambiguity.
    #[serde(default)]
    pub refuse_inert_edits: bool,
}

/// What `code/create-workspace` established.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CreateWorkspaceResult {
    /// Always `true` on success — the sandbox is now live for this caller.
    pub created: bool,
    /// The root the sandbox was bound to (echoed for confirmation).
    pub workspace_root: String,
    /// How many extra read-only roots were granted.
    pub read_root_count: usize,
}

#[async_trait]
impl ActionCommand for CodeCreateWorkspace {
    const NAME: &'static str = "code/create-workspace";
    // Privileged: this DEFINES the filesystem sandbox boundary (an arbitrary root +
    // read-roots). Operations WITHIN an established sandbox (read/write/edit) are
    // AiSafe; choosing where the sandbox is rooted is infrastructure, not a persona
    // toolbelt action — letting a persona re-root itself to `/` would be an escape.
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Establish or re-root the caller's workspace sandbox at a specific path, with optional \
         read-only roots. Overrides the auto-provisioned default (repo cwd) for this caller.";
    type Params = CodeCreateWorkspaceParams;
    type Output = CreateWorkspaceResult;

    async fn run(
        &self,
        ctx: &Ctx,
        p: CodeCreateWorkspaceParams,
    ) -> Result<CreateWorkspaceResult, CommandError> {
        let who = caller_id(ctx);
        let root = std::path::Path::new(&p.workspace_root);
        let mut security = PathSecurity::new(root).map_err(|e| {
            CommandError::Invalid(format!("invalid workspace root '{}': {e}", p.workspace_root))
        })?;
        for rr in &p.read_roots {
            security
                .add_read_root(std::path::Path::new(rr))
                .map_err(|e| CommandError::Invalid(format!("invalid read root '{rr}': {e}")))?;
        }
        // An explicit create-workspace OVERRIDES any engine `ensure_engine` lazily
        // provisioned for this caller (its doc reserves this override path). Keyed by
        // caller, so each peer's change-DAG stays isolated — exactly like the migrated
        // read/write/edit siblings, and never on a spoofable persona_id param.
        let policy = if p.refuse_inert_edits {
            crate::code::file_engine::WritePolicy::RefuseInert
        } else {
            crate::code::file_engine::WritePolicy::Warn
        };
        self.state.file_engines.insert(
            who.clone(),
            FileEngine::new(&who, security).with_write_policy(policy),
        );
        // DROP the caller's shell session so it is re-created at the NEW root.
        // `ensure_shell` early-returns when a session exists, so without this a
        // re-root moved her FILE engine and left her SHELL in the old directory —
        // the two halves of her hands pointing at different workspaces.
        self.state.shell_sessions.remove(&who);
        if !p.path_prepend.is_empty() {
            ensure_shell(&self.state, &who)?;
            if let Some(mut shell) = self.state.shell_sessions.get_mut(&who) {
                let inherited = std::env::var("PATH").unwrap_or_default();
                let prepend = p.path_prepend.join(":");
                shell.set_env("PATH".to_string(), format!("{prepend}:{inherited}"));
                crate::probe!(
                    class = "code.workspace.path_prepend",
                    caller = who.as_str(),
                    prepend = prepend.as_str(),
                    "granted the caller's shell an explicit PATH prefix (era-matched interpreter)"
                );
            }
        }
        Ok(CreateWorkspaceResult {
            created: true,
            workspace_root: p.workspace_root,
            read_root_count: p.read_roots.len(),
        })
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
crate::register_command!(CodeDelete);
crate::register_command!(CodeDiff);
crate::register_command!(CodeUndo);
crate::register_command!(CodeHistory);
crate::register_command!(CodeCreateWorkspace);

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
        Arc::new(CodeShellKill { state: state.clone() }),
        Arc::new(CodeDelete { state: state.clone() }),
        Arc::new(CodeDiff { state: state.clone() }),
        Arc::new(CodeUndo { state: state.clone() }),
        Arc::new(CodeHistory { state: state.clone() }),
        Arc::new(CodeCreateWorkspace { state }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, Ctx};
    use dashmap::DashMap;

    // what this catches: code/edit forgives the ways a model mis-calls it (glass-boxed: a 14B
    // sent edit_mode:"append" as a bare string) — strict tagged object still works, a bare mode
    // string + top-level fields is accepted, the mode is inferred from present fields, and a
    // MISSING required field fails loud NAMING it (never a silent no-op that scores false-zero).
    #[test]
    fn code_edit_normalizes_forgiving_shapes_and_fails_loud() {
        let mk = |v: serde_json::Value| serde_json::from_value::<CodeEditParams>(v).expect("params");
        // (1) strict tagged object — unchanged
        let m = normalize_edit_mode(&mk(serde_json::json!({
            "file_path": "a.py", "edit_mode": {"type":"append","content":"X"}
        }))).unwrap();
        assert!(matches!(m, EditMode::Append { content } if content == "X"));
        // (2) bare mode string + top-level content
        let m = normalize_edit_mode(&mk(serde_json::json!({
            "file_path": "a.py", "edit_mode": "append", "content": "Y"
        }))).unwrap();
        assert!(matches!(m, EditMode::Append { content } if content == "Y"));
        // (3) bare "search_replace" + top-level fields
        let m = normalize_edit_mode(&mk(serde_json::json!({
            "file_path": "a.py", "edit_mode": "search_replace", "search": "old", "replace": "new"
        }))).unwrap();
        assert!(matches!(m, EditMode::SearchReplace { search, replace, .. } if search=="old" && replace=="new"));
        // (4) inferred from present fields (no edit_mode at all)
        let m = normalize_edit_mode(&mk(serde_json::json!({
            "file_path": "a.py", "search": "o", "replace": "n"
        }))).unwrap();
        assert!(matches!(m, EditMode::SearchReplace { .. }));
        // (5) bare "append" with NO content → loud error naming the missing field (the exact
        //     glass-boxed failure: edit_mode:"append", no content).
        let err = normalize_edit_mode(&mk(serde_json::json!({
            "file_path": "a.py", "edit_mode": "append"
        }))).unwrap_err();
        assert!(format!("{err}").contains("content"), "names the missing field: {err}");
    }

    // what this catches: THE live edit-stall (2026-07-14). Devstral personas
    // emitted edit_mode as a NESTED, untyped object with the line numbers inside
    // it and NO start_line — `{"edit_mode":{"end_line":65535,"new_content":"…"}}`
    // — their reflexive "replace the whole file" shape. The old normalizer read
    // start/end only from top-level, missed them, and failed `needs start_line`;
    // personas re-emitted the identical call forever and never reached run. The
    // nested numbers must resolve and the missing start_line must default to 1.
    #[test]
    fn code_edit_forgives_nested_lines_and_the_reflexive_whole_file_shape() {
        let mk = |v: serde_json::Value| serde_json::from_value::<CodeEditParams>(v).expect("params");

        // (a) nested end_line + new_content, no start_line → LineRange{1, 65535, …}
        let m = normalize_edit_mode(&mk(serde_json::json!({
            "file_path": "src/main.rs",
            "edit_mode": {"end_line": 65535, "new_content": "fn main() {}"}
        }))).unwrap();
        match m {
            EditMode::LineRange { start_line, end_line, new_content } => {
                assert_eq!(start_line, 1);
                assert_eq!(end_line, 65535);
                assert_eq!(new_content, "fn main() {}");
            }
            other => panic!("expected LineRange, got {other:?}"),
        }

        // (b) ONLY new_content (no lines at all) → whole-file replace: start 1, end MAX
        let m = normalize_edit_mode(&mk(serde_json::json!({
            "file_path": "src/main.rs", "new_content": "whole new file"
        }))).unwrap();
        match m {
            EditMode::LineRange { start_line, end_line, .. } => {
                assert_eq!(start_line, 1);
                assert_eq!(end_line, u32::MAX);
            }
            other => panic!("expected LineRange, got {other:?}"),
        }

        // (c) nested line for insert_at resolves from inside edit_mode too
        let m = normalize_edit_mode(&mk(serde_json::json!({
            "file_path": "a.py", "edit_mode": {"line": 3, "content": "x"}
        }))).unwrap();
        assert!(matches!(m, EditMode::InsertAt { line, .. } if line == 3));

        // (d) still loud when there's genuinely nothing to write
        let err = normalize_edit_mode(&mk(serde_json::json!({
            "file_path": "a.py", "edit_mode": {"end_line": 10}
        }))).unwrap_err();
        assert!(format!("{err}").contains("new_content"), "names missing field: {err}");
    }

    // what this catches: the code/list glob-recovery (#160). A model that reflexively
    // passes a recursive glob (`**/*.rs`) to code/list must get the MATCHING FILES back
    // as a listing, not a bare NotFound it retries forever (mined: 38% of live calls).
    // Guards both halves: the glob DETECTION that triggers the reroute, and the
    // GlobResult→ListResult projection (each match a File entry, name = basename,
    // directory_path records the glob so the persona sees why it's cross-directory).
    #[test]
    fn code_list_recovers_a_glob_into_a_listing() {
        // detection: the reflexive recursive patterns route; a plain dir does not.
        assert!(looks_like_file_glob("**/*.rs"));
        assert!(looks_like_file_glob("src/**/*.toml"));
        assert!(!looks_like_file_glob("src"));
        assert!(!looks_like_file_glob("."));

        let glob = GlobResult {
            success: true,
            pattern: "**/*.rs".to_string(),
            matches: vec!["core/main.rs".to_string(), "lib.rs".to_string()],
            total_matches: 2,
            truncated: false,
            error: None,
        };
        let listing = list_result_from_glob("**/*.rs", glob);
        assert!(listing.success);
        assert_eq!(listing.total_count, 2);
        assert_eq!(listing.directory_path, "glob:**/*.rs", "records the glob provenance");
        assert_eq!(listing.entries[0].name, "main.rs", "name is the basename");
        assert_eq!(listing.entries[0].path, "core/main.rs", "path stays workspace-relative");
        assert!(matches!(listing.entries[0].kind, FsEntryKind::File));
    }

    // what this catches: a placeholder file_path (the 14B echoed `<path_to_blueprints.py>` instead
    // of the real path it had seen) fails LOUD with guidance toward the concrete path — while a real
    // path passes untouched (no false positives on ordinary filenames).
    #[test]
    fn code_edit_rejects_placeholder_paths_but_passes_real_ones() {
        for ph in ["<path_to_blueprints.py>", "path_to_file.py", "/path/to/x.py", "<file>", "your_file.rs"] {
            assert!(reject_placeholder_path(ph).is_err(), "should reject placeholder: {ph}");
        }
        for real in ["src/flask/blueprints.py", "core/continuum-core/src/lib.rs", "a.py", "example.py"] {
            assert!(reject_placeholder_path(real).is_ok(), "should accept real path: {real}");
        }
    }

    // what this catches: code/search auto-detects a glob-shaped `pattern` (the misuse a local
    // model makes — `**/*.py` as the grep term, glass-boxed looping 12× on it) so it can list
    // FILES instead of grepping contents for the literal glob → 0 → loop. Must NOT misclassify a
    // genuine content regex, or real greps would be silently redirected. Regression for the
    // flask SWE 0-edit search-loop.
    #[test]
    fn glob_shaped_search_patterns_detected_content_regexes_spared() {
        for g in ["**/*.py", "*.rs", "src/**/*.js", "**/blueprints.py", "*.{rs,py}"] {
            assert!(looks_like_file_glob(g), "should be treated as a file glob: {g}");
        }
        for r in ["Blueprint", "foo.*bar", "fn .*Params", "self.name = name", "TODO", "raise ValueError"] {
            assert!(!looks_like_file_glob(r), "must NOT be treated as a glob (real content pattern): {r}");
        }
    }

    // what this catches: the overflow summary (glass-boxed on SWE flask-4045 —
    // pattern:"blueprint" → 101 raw hits → the solver re-searched 9× instead of
    // advancing). Above OVERFLOW_MATCHES the result must compress to ONE
    // representative match per file (concrete carryable paths), name the top
    // files BY COUNT in the note with read/narrow guidance, and keep
    // total_matches truthful; a small result stays untouched (no false
    // compression on ordinary searches). // regression for SWE cell one
    #[tokio::test]
    async fn search_overflow_compresses_to_per_file_summary_with_guidance() {
        let dir = std::env::temp_dir().join(format!("search-overflow-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // hot.py: 30 hits; warm.py: 5; cold.py: 2 — 37 total, uneven on purpose.
        std::fs::write(dir.join("hot.py"), "needle\n".repeat(30)).unwrap();
        std::fs::write(dir.join("warm.py"), "needle\n".repeat(5)).unwrap();
        std::fs::write(dir.join("cold.py"), "needle\n".repeat(2)).unwrap();

        let security = PathSecurity::new(&dir).expect("temp subdir is a valid root");
        let file_engines = Arc::new(DashMap::new());
        file_engines.insert("local-owner".to_string(), FileEngine::new("local-owner", security));
        let state = Arc::new(CodeState::new(
            file_engines,
            Arc::new(DashMap::new()),
            tokio::runtime::Handle::current(),
        ));
        let cmd = CodeSearch { state };
        let out = cmd
            .run(
                &Ctx::default(),
                CodeSearchParams {
                    pattern: "needle".to_string(),
                    file_glob: None,
                    max_results: None,
                },
            )
            .await
            .expect("search runs");

        assert_eq!(out.total_matches, 37, "counts stay truthful");
        assert!(
            out.matches.len() <= 10,
            "overflow compresses to per-file representatives: {}",
            out.matches.len()
        );
        let files: Vec<&str> = out.matches.iter().map(|m| m.file_path.as_str()).collect();
        assert_eq!(
            files.iter().filter(|f| f.contains("hot.py")).count(),
            1,
            "one representative per file: {files:?}"
        );
        let note = out.error.expect("overflow note present");
        assert!(
            note.contains("too many to list") && note.contains("code/read"),
            "note carries the read/narrow guidance: {note}"
        );
        assert!(
            note.find("hot.py (30)").unwrap_or(usize::MAX)
                < note.find("warm.py (5)").unwrap_or(usize::MAX),
            "top files ordered by match count: {note}"
        );

        // Small search stays raw — no false compression.
        let small = cmd
            .run(
                &Ctx::default(),
                CodeSearchParams {
                    pattern: "needle".to_string(),
                    file_glob: Some("cold.py".to_string()),
                    max_results: None,
                },
            )
            .await
            .expect("small search runs");
        assert_eq!(small.total_matches, 2);
        assert_eq!(small.matches.len(), 2, "under threshold keeps every line");
        assert!(small.error.is_none(), "no note on a small result");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: a code/list miss on a NONEXISTENT path (the "empty workspace"
    // confabulation source — a persona reached for `src/persona.rs` where no top-level
    // `src` exists, got a bare NotFound, and concluded the workspace was empty) must
    // TEACH the real top-level layout instead of a contentless error. The persona then
    // sees `apps, core, docs` in the very error and self-corrects. // regression: live
    // 2026-07-13 empty-workspace confabulation
    #[test]
    fn list_miss_teaches_the_real_top_level_layout() {
        let dir = std::env::temp_dir().join(format!("list-miss-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("apps")).unwrap();
        std::fs::create_dir_all(dir.join("core")).unwrap();
        std::fs::create_dir_all(dir.join("docs")).unwrap();
        let security = PathSecurity::new(&dir).expect("temp subdir is a valid root");
        let engine = FileEngine::new("local-owner", security);

        let err = teach_layout_on_miss(&engine, "src/persona.rs", "no such file or directory");
        let msg = err.to_string();
        assert!(msg.contains("apps") && msg.contains("core") && msg.contains("docs"),
            "enumerates the real top-level dirs so the persona self-corrects: {msg}");
        assert!(msg.contains("src/persona.rs"), "names what was actually missed: {msg}");
        assert!(msg.contains("don't assume source lives under `src/`"),
            "carries the same anti-assumption teaching as the workspace-map: {msg}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: a code/list GLOB that matches zero files (the live `**/`
    // idiom — a persona reaching for "everything") must not read as an empty
    // workspace. The empty listing carries a note naming the real top-level dirs +
    // pointing at code/tree, so the persona doesn't confabulate emptiness.
    // regression: live 2026-07-14 `code/list(path=**/)` → empty
    #[tokio::test]
    async fn zero_match_glob_note_names_the_layout_not_emptiness() {
        let dir = std::env::temp_dir().join(format!("zero-glob-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("apps")).unwrap();
        std::fs::create_dir_all(dir.join("core")).unwrap();
        std::fs::write(dir.join("core/main.rs"), "fn main() {}").unwrap();
        let security = PathSecurity::new(&dir).expect("temp subdir is a valid root");
        let file_engines = Arc::new(DashMap::new());
        file_engines.insert("local-owner".to_string(), FileEngine::new("local-owner", security));
        let state = Arc::new(CodeState::new(
            file_engines,
            Arc::new(DashMap::new()),
            tokio::runtime::Handle::current(),
        ));
        let cmd = CodeList { state };

        // A glob that matches no files → empty entries + a teaching note.
        let out = cmd
            .run(&Ctx::default(), CodeListParams { path: Some("**/*.nonexistent".to_string()), include_hidden: None })
            .await
            .expect("a zero-match glob is not an error");
        assert!(out.entries.is_empty(), "no files match the glob");
        let note = out.error.expect("zero-match glob carries a teaching note");
        assert!(note.contains("NOT an empty workspace"), "corrects the confabulation: {note}");
        assert!(note.contains("apps") && note.contains("core"), "names the real dirs: {note}");
        assert!(note.contains("code/tree"), "points at the recursive view: {note}");

        // A glob that DOES match keeps working (no false note).
        let hit = cmd
            .run(&Ctx::default(), CodeListParams { path: Some("**/*.rs".to_string()), include_hidden: None })
            .await
            .expect("glob runs");
        assert!(!hit.entries.is_empty(), "the .rs glob finds main.rs");
        assert!(hit.error.is_none(), "a matching glob carries no note");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A `CodeState` whose `local-owner` engine (the `Ctx::default` caller) is rooted
    /// at the OS temp dir, so read-only commands run without touching the repo or
    /// depending on the process cwd. Pre-seeding the engine makes `ensure_engine`
    /// short-circuit on `contains_key`, so no `current_dir` lookup happens.
    fn state_rooted_at_temp() -> Arc<CodeState> {
        let security = PathSecurity::new(&std::env::temp_dir()).expect("temp dir is a valid root");
        let file_engines = Arc::new(DashMap::new());
        file_engines.insert(
            "local-owner".to_string(),
            FileEngine::new("local-owner", security),
        );
        Arc::new(CodeState::new(
            file_engines,
            Arc::new(DashMap::new()),
            tokio::runtime::Handle::current(),
        ))
    }

    // what this catches: the four file-op hands migrated off the legacy persona_id
    // arms (#62) reach the persona tool surface under their canonical names and the
    // AiSafe gating a workspace-mutating-but-undoable hand warrants (same class as
    // code/write). A wrong NAME (persona can't find the tool) or a drifted ACCESS
    // (security-boundary regression) is caught here.
    #[test]
    fn migrated_file_ops_are_aisafe_named_hands() {
        assert_eq!(CodeDelete::NAME, "code/delete");
        assert_eq!(CodeDiff::NAME, "code/diff");
        assert_eq!(CodeUndo::NAME, "code/undo");
        assert_eq!(CodeHistory::NAME, "code/history");
        for access in [
            CodeDelete::ACCESS,
            CodeDiff::ACCESS,
            CodeUndo::ACCESS,
            CodeHistory::ACCESS,
        ] {
            assert!(matches!(access, AccessLevel::AiSafe), "file-op hands are AiSafe");
        }
    }

    // what this catches: code/history runs caller-scoped (Ctx::default → local-owner)
    // through the lazy engine and returns a typed, well-formed HistoryResult — an
    // empty history on a fresh workspace — proving the migrated run path + identity
    // scoping work end to end WITHOUT a persona_id param. A regression to the legacy
    // param shape (or a panic on the empty change graph) is caught.
    #[tokio::test]
    async fn history_on_fresh_workspace_is_typed_and_empty() {
        let cmd = CodeHistory {
            state: state_rooted_at_temp(),
        };
        let out = cmd
            .run(
                &Ctx::default(),
                CodeHistoryParams {
                    file_path: None,
                    limit: None,
                },
            )
            .await
            .expect("history never errors on a fresh workspace");
        assert!(out.success, "fresh history is a success");
        assert!(out.nodes.is_empty(), "no changes recorded yet");
        let json = serde_json::to_value(&out).unwrap();
        assert!(json["nodes"].is_array(), "nodes is the wire array");
        assert!(json["total_count"].is_number(), "total_count present on the wire");
    }

    // what this catches: the four hands are contributed to the module object map via
    // command_objects — a regression that drops one (the descriptor still registers,
    // but the persona has no runtime object to route to) is caught.
    #[tokio::test]
    async fn command_objects_includes_the_file_op_hands() {
        let names: Vec<&str> = command_objects(state_rooted_at_temp())
            .iter()
            .map(|c| c.name())
            .collect();
        for n in ["code/delete", "code/diff", "code/undo", "code/history"] {
            assert!(names.contains(&n), "command_objects missing {n}; got {names:?}");
        }
    }

    // what this catches: the citizen-layer workspace policy (Joel 2026-07-10:
    // "each persona is only the diff from the shared"). An IDENTIFIED peer's
    // engine must NEVER root at the core's cwd — the live incident: a persona's
    // misdirected [dependencies] edit replaced the repo-root Cargo.toml through
    // the shared-cwd default, breaking every build path. The peer's root is a
    // CoW clone under <CONTINUUM_HOME>/citizens/peers/<peer>/workspace, seeded
    // from the base, durable across calls; the anonymous local operator keeps
    // cwd. regression for commit 3ad97cc2e (the manifest-clobber repair).
    #[test]
    fn identified_peer_roots_at_citizen_layer_never_shared_cwd() {
        // Scoped homes so the test never touches the real ~/.continuum. A tiny
        // base dir keeps the CoW clone instant.
        //
        // The base is passed EXPLICITLY via `ensure_citizen_layer_from_base` — this
        // test used to `set_current_dir(base)` to steer the clone source, but process
        // cwd is global: while it was pointed at this temp dir, every parallel ts-rs
        // `export_bindings_*` test resolved its relative `export_to` against it and
        // panicked trying to write outside the tree. Injecting the base keeps the
        // clone exercised end-to-end with zero global-cwd mutation. #191.
        let base = tempfile::tempdir().expect("base");
        std::fs::write(base.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        let home = tempfile::tempdir().expect("home");
        std::env::set_var("CONTINUUM_HOME", home.path());

        let peer = "test-peer-1234";
        let layer = ensure_citizen_layer_from_base(peer, base.path()).expect("layer provisions");
        assert!(
            layer.starts_with(home.path()),
            "layer lives under CONTINUUM_HOME: {layer:?}"
        );
        assert_ne!(
            layer.canonicalize().unwrap(),
            base.path().canonicalize().unwrap(),
            "peer layer is NOT the shared base"
        );
        assert!(
            layer.join("Cargo.toml").is_file(),
            "layer is seeded from the base (CoW clone)"
        );
        // A peer WRITE lands in the layer, never the base.
        std::fs::write(layer.join("Cargo.toml"), "[dependencies]\n").unwrap();
        assert_eq!(
            std::fs::read_to_string(base.path().join("Cargo.toml")).unwrap(),
            "[workspace]\n",
            "the shared base is untouched by layer writes"
        );
        // Idempotent: second call reuses the existing layer (her diff survives).
        let again = ensure_citizen_layer_from_base(peer, base.path()).expect("layer reused");
        assert_eq!(again, layer);
        assert_eq!(
            std::fs::read_to_string(again.join("Cargo.toml")).unwrap(),
            "[dependencies]\n",
            "the peer's divergence is durable"
        );

        std::env::remove_var("CONTINUUM_HOME");
    }
}
