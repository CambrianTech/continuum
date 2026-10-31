use super::*;

/// A file-engine error in the class the caller can act on. Every code verb used to send
/// these through `CommandError::Internal`, so a sandbox boundary, a missing file and a
/// failed edit all reached Kimi as "[internal]", a substrate fault, when each one is a
/// "fix your call" (2026-10-05: a write to Temp outside her workspace, a read of a path
/// that does not exist). Only a real I/O failure is the substrate's.
pub(super) fn engine_err(e: crate::code::file_engine::FileEngineError) -> CommandError {
    use crate::code::file_engine::FileEngineError as E;
    let said = e.to_string();
    match e {
        E::Security(_) => CommandError::Denied(said),
        E::NotFound(_) => CommandError::NotFound(said),
        E::EditFailed(_) => CommandError::Invalid(said),
        E::Io(_) => CommandError::Internal(said),
    }
}

/// How far THIS caller's reads may reach: the host for the operator; her CONFINEMENT
/// when her trust admits `code/shell` (a read sandbox cannot contain a caller who may
/// run `cat`, so it only cost her turns: Kimi, 2026-10-06 — but her `cat` is now held
/// to the same roots, card d598c806, so the reads are too); the sandbox for everyone
/// else. Asked through the gate's own resolvers, so the read scope can never be wider
/// than the shell grant.
pub(super) fn read_scope_of(state: &CodeState, ctx: &Ctx, workspace: &std::path::Path) -> Result<crate::code::path_security::ReadScope, CommandError> {
    use crate::code::path_security::ReadScope;
    // no resolved trust (executor unbound, or a policy that cannot resolve) is the sandbox
    let Some(trust) = state.caller_trust(ctx.caller.as_ref()) else {
        return Ok(ReadScope::Sandbox);
    };
    if !crate::routing::command_acl::is_command_authorized("code/shell", trust) {
        return Ok(ReadScope::Sandbox);
    }
    Ok(match confinement_of(state, ctx, workspace)? {
        None => ReadScope::Host { deny: Vec::new() },
        Some(confinement) if confinement.reaches_host() => ReadScope::Host { deny: confinement.host_denials() },
        Some(confinement) => ReadScope::Confined(confinement.materialize()),
    })
}

/// THE one resolver for a verb that takes a path from its caller and reads the file
/// itself (`vision/look`, and any later one): the caller's engine, the caller's read
/// scope, the engine's own security. Returns the absolute canonical path, or the same
/// refusal `code/read` gives — named, correctable, never a bare I/O error.
pub async fn resolve_readable_path(
    state: &CodeState,
    ctx: &Ctx,
    path: &str,
) -> Result<std::path::PathBuf, CommandError> {
    let key = hands_of(state, ctx);
    ensure_engine(state, &key).await?;
    let engine = state
        .file_engines
        .get(&key.map_key())
        .ok_or_else(|| CommandError::Internal("workspace vanished after provisioning".into()))?;
    let scope = read_scope_of(state, ctx, &engine.workspace_root())?;
    engine
        .security()
        .validate_read_in(path, scope)
        .map_err(|e| engine_err(crate::code::file_engine::FileEngineError::from(e)))
}

/// The persona/owner this tool call acts AS — the authenticated caller identity
/// (an airc `peer_id`), never a params field. `None` caller is the
/// substrate-local owner. This is the single point that maps the gated identity
/// to the per-caller workspace; nothing trusts caller-supplied identity.
pub(crate) fn caller_id(state: &CodeState, ctx: &Ctx) -> String {
    // ONE resolver (`operator_peer::acting_peer_id`): the same peer the work verbs
    // claim as. `LOCAL_OWNER` survives only for the boot window before the
    // self-peers are online.
    state
        .acting
        .acting_peer_id(ctx)
        .map(|p| p.to_string())
        .unwrap_or_else(|| LOCAL_OWNER.to_string()) // unwrap_or_else: boot window, no self-peer yet — the core's cwd, named below
}

pub(crate) const LOCAL_OWNER: &str = "local-owner";

/// Whose hands this call acts with: the caller, in the acting context its connection is
/// bound to (`CallerIdentity::hands`, set by the kernel side, never by params). The one
/// key every hands map uses; the caller alone stays the authorization identity
/// ([`caller_id`]). See [`crate::code::hands`].
pub(crate) fn hands_of(state: &CodeState, ctx: &Ctx) -> HandsKey {
    HandsKey::new(caller_id(state, ctx), ctx.caller.as_ref().and_then(|c| c.hands))
}

/// Lazily ensure a workspace [`FileEngine`] exists for `who`. The local operator
/// (no peer identity) roots at our recorded checkout (`repo_registry::own_checkout`). An
/// IDENTIFIED peer (persona or remote agent) roots at their citizen LAYER (see
/// [`ensure_citizen_layer`]): writes land in their own copy-on-write clone, and
/// the shared checkout is never writable through a peer's hands. Idempotent and
/// defers to an engine a prior call already created (so an explicit
/// `create-workspace` with a specific root still wins).
pub(crate) async fn ensure_engine(state: &CodeState, key: &HandsKey) -> Result<(), CommandError> {
    // `caller` is who acts (her layer, her trust, her held card); `who` is which of her
    // hands (the map key: her default context keeps the bare caller).
    let caller = key.caller.as_str();
    let map_key = key.map_key();
    let who = map_key.as_str();
    if state.file_engines.contains_key(who) {
        return Ok(());
    }
    let local = is_local_who(state, caller);
    let root = match key.context {
        // An acting context's workspace is what it was rooted at; it never falls back to her
        // home layer, which would put a fork's or a card's edits on her desk. Fail closed and
        // say so (Astra on #5005 and #5026). A card whose hands couldn't be bound this turn
        // lands here too: its stand-in context is never rooted.
        Some(_) => {
            return Err(CommandError::Invalid(
                "these hands have no workspace: they belong to an eval or a fork not yet rooted, or \
                 to a card whose checkout isn't on this node (or couldn't be bound) this turn. Code \
                 tools can't act here; other tools can"
                    .into(),
            ))
        }
        // the local operator acts in OUR checkout, as recorded (never the cwd, card 2a2cd6f1)
        None if local => crate::code::repo_registry::own_checkout()
            .await
            .map_err(|e| CommandError::Internal(format!("workspace root unavailable: {e}")))?,
        None => {
            let citizen_root = ensure_citizen_layer(caller).await?;
            // Every citizen workspace is git-backed from birth
            // ([[workspace-is-a-cow-diff-from-shared-always-git]]): no-op when .git
            // exists; otherwise init + root commit, so diff→share→apply works the
            // moment a citizen first touches files. Glass-boxed 2026-07-11: three
            // parallel Conway implementations across un-versioned workspaces had no
            // consolidation path, and the team asked for one. Loud on failure — a
            // workspace that silently can't version work is a quiet defect.
            crate::code::git_bridge::git_init_if_needed(&citizen_root)
                .await
                .map_err(|e| CommandError::Internal(format!("workspace git init failed: {e}")))?;
            citizen_root
        }
    };
    let security = PathSecurity::new(&root)
        .map_err(|e| CommandError::Internal(format!("workspace security init failed: {e}")))?;
    // Every racer for one key computes the same root (a key's root is a function of the key),
    // so the first insert wins and the rest are the same engine's twin, dropped.
    state
        .file_engines
        .entry(who.to_string())
        .or_insert_with(|| FileEngine::new(caller, security));
    Ok(())
}

/// Is `who` a LOCAL identity — the boot-window `LOCAL_OWNER`, or the operator / agent
/// self-peer? Local identities work in the core's own checkout; everyone else in a layer.
pub(super) fn is_local_who(state: &CodeState, who: &str) -> bool {
    who == LOCAL_OWNER
        || uuid::Uuid::parse_str(who)
            .map(|peer| state.acting.is_local_identity(peer))
            .unwrap_or(false) // unwrap_or: not a uuid = not a self-peer
}

/// Where THESE hands stand, for a command that runs outside the engine (`code/run`): the
/// root this caller's acting context was rooted at (a held card's checkout on a work turn,
/// an exam directory for an eval), from the one acting-root record her shell and file hands
/// are rooted through. A context that was never rooted gets `None` (its snippet runs in
/// scratch), never the live checkout (Astra's review of #5005). The caller's default context
/// keeps the local identity's held-card lookup.
pub(crate) fn held_root_for(state: &CodeState, ctx: &Ctx) -> Option<std::path::PathBuf> {
    let key = hands_of(state, ctx);
    let persona = uuid::Uuid::parse_str(&key.caller).ok()?;
    state.acting.acting_root_in(persona, key.context)
}

/// The prepared instance environment's python for the root these hands stand at
/// ([`held_root_for`]), if it has one: the interpreter `code/run` runs a snippet with (card
/// 533c2d78). `None` = no root, or no prepared env, and the caller falls back to the PATH
/// interpreter, named.
pub(crate) fn held_env_python_for(state: &CodeState, ctx: &Ctx) -> Option<std::path::PathBuf> {
    let root = held_root_for(state, ctx)?;
    state.acting.instance_python(&root)
}

/// Lazily ensure shell execution state exists for `who`, rooted at the
/// caller's ENGINE root — the one workspace authority per caller (the operator's
/// cwd, a peer's citizen layer, or whatever `create-workspace` pinned). Never a
/// second independent cwd fallthrough: before this, a peer's shell rooted at the
/// core's cwd even when her file engine didn't, which is how narrated shell
/// commands landed in the shared checkout. Idempotent; one execution registry per
/// caller retains handles and configured cwd/env. Each command starts a fresh
/// shell process: shell `cd`, assignments, exports and options do not flow back
/// into this registry or into later calls.
pub(super) async fn ensure_shell(state: &CodeState, ctx: &Ctx) -> Result<String, CommandError> {
    // The hands this call is bound to: the shell is provisioned in exactly those.
    let key = &hands_of(state, ctx);
    let map_key = key.map_key();
    let who = map_key.as_str();
    // The ENGINE decides the root, once for these hands' life, and the shell opens there.
    // Hands are never re-rooted (card fc9e9fd7), so a shell that exists stands at its
    // engine's root.
    ensure_engine(state, key).await?;
    if state.shell_sessions.contains_key(who) {
        return Ok(map_key);
    }
    let root = state
        .file_engines
        .get(&who.to_string())
        .map(|e| e.workspace_root())
        .ok_or_else(|| CommandError::Internal("engine vanished after provisioning".into()))?;
    let session_id = uuid::Uuid::new_v4().to_string();
    let mut shell = ShellSession::new(&session_id, &key.caller, &root)
        .map_err(|e| CommandError::Internal(format!("shell init failed: {e}")))?;
    // A citizen's shell is confined to her roots (card d598c806); the operator's is the
    // operator's. Decided by the SAME trust the gate saw, never by a params field.
    if let Some(confinement) = confinement_of(state, ctx, &root)? {
        shell.confine(confinement);
    }
    state.shell_sessions.entry(who.to_string()).or_insert(shell);
    Ok(map_key)
}

/// The confinement this caller's children and reads are held to: `None` for the
/// operator, the whole host minus credentials for this node's own resident, and her
/// workspace for a caller from outside this node (an airc peer, a socket).
///
/// This node's residents are not restricted (Joel 2026-10-17: "Don't restrict them. If she
/// wants to write to my documents folder for no apparent reason so be it") EXCEPT the
/// credential paths and other citizens' directories (Joel 2026-10-18: "keep creds
/// off-limits"): she quotes tool output into rooms, and a room reaches every enrolled
/// peer. Her shell also never inherits the core's secrets (`strip_secret_env`).
pub(crate) fn confinement_of(state: &CodeState, ctx: &Ctx, workspace: &std::path::Path) -> Result<Option<crate::system_resources::confinement::Confinement>, CommandError> {
    let resident = ctx.caller.as_ref().is_some_and(|c| c.source == crate::routing::auth_policy::CallerSource::LocalPersona);
    // only a RESOLVED owner is unconfined; no resolved trust confines
    let trust = state.caller_trust(ctx.caller.as_ref());
    if !resident && trust == Some(crate::routing::trust_level::TrustLevel::Owner) {
        return Ok(None);
    }
    let Some(home) = crate::paths::home_dir() else {
        return Ok(None);
    };
    let Ok(continuum_root) = crate::paths::continuum_home() else {
        return Ok(None);
    };
    let confinement = crate::system_resources::confinement::Confinement::for_caller(&home, &continuum_root, &caller_id(state, ctx), workspace);
    Ok(Some(if resident { confinement.reaching_host() } else { confinement }))
}

/// How long `code/shell` waits INLINE for completion before handing back a handle
/// (the execution_id) so the caller polls instead of blocking. Bounded so a
/// long-running command — a build, a test suite, a TRAINING run, a daemon — NEVER
/// forces a promise that blocks the turn or times out. The command keeps running;
/// drive it with `code/shell-poll` / `code/shell-kill`. The model can override.
pub(super) const DEFAULT_SHELL_WAIT_MS: u64 = 30_000;

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
    /// With no range, the most characters this read answers with: the reading mind's share
    /// of her view's measured per-act headroom, filled by her act layer on the call it
    /// dispatches, never by her (so not in the schema she is shown). Absent: whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    #[ts(skip)]
    pub page_budget: Option<crate::code::file_engine::PageBudget>,
}

#[async_trait]
impl ActionCommand for CodeRead {
    const NAME: &'static str = "code/read";
    const EFFECT: continuum_base::tool_effect::ToolEffect = continuum_base::tool_effect::ToolEffect::Observes;
    const ALIASES: &'static [&'static str] = &["read_file", "view", "cat", "open_file"];
    const NATIVE: bool = true; // core agentic working set — offered natively (auto-derived)
    const DESCRIPTION: &'static str =
        "Read a file from the workspace, optionally a line range; a large file without one \
         returns its first page. Lines come back NUMBERED (`   12 | text`), the numbers \
         code/edit's line_range and insert_at address. The `N | ` gutter is display: never \
         write it back into a file.";
    type Params = CodeReadParams;
    type Output = ReadResult;

    async fn run(&self, ctx: &Ctx, p: CodeReadParams) -> Result<ReadResult, CommandError> {
        let engine = engine!(self, ctx);
        let read = engine
            .read_in(&p.file_path, p.start_line, p.end_line, read_scope_of(&self.state, ctx, &engine.workspace_root())?)
            .map_err(engine_err)?;
        Ok(crate::code::file_engine::PageBudget::answer(read, p.start_line, p.end_line, p.page_budget))
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
    const ALIASES: &'static [&'static str] = &["write_file", "create_file", "save_file"];
    const NATIVE: bool = true; // core agentic working set — offered natively (auto-derived)
    const DESCRIPTION: &'static str =
        "Create or overwrite a file with new content. Tracked in the change history (undoable).";
    type Params = CodeWriteParams;
    type Output = WriteResult;

    async fn run(&self, ctx: &Ctx, p: CodeWriteParams) -> Result<WriteResult, CommandError> {
        let engine = engine!(self, ctx);
        // THE WRITE PATH IS WHERE SOLVED WORK DIES (#: flask-4045 was solved and then
        // destroyed TWICE, and both times we diagnosed it from PROSE — the persona's own
        // account of what she did — rather than from what actually hit disk. This probe
        // is the receipt: sizes before/after and whether the file already existed, so a
        // patch that lands wrong is visible AT THE MOMENT IT LANDS.
        //
        // `existed` + `before_bytes` are the load-bearing pair: an overwrite of a large
        // existing file with a tiny body is the exact shape of "solved, then destroyed"
        // (the model re-emits a stub or a fragment over a working file). A count alone
        // could never show that.
        let before = std::fs::metadata(engine.workspace_root().join(&p.file_path))
            .ok()
            .map(|m| m.len());
        let scope = read_scope_of(&self.state, ctx, &engine.workspace_root())?;
        let out = engine
            .write_in(&p.file_path, &p.content, p.description.as_deref(), &scope)
            .map_err(engine_err);
        continuum_base::probe!(
            class = "code.write.landed",
            path = %p.file_path,
            existed = before.is_some(),
            before_bytes = before.unwrap_or(0),
            after_bytes = p.content.len() as u64,
            shrank = before.is_some_and(|b| (p.content.len() as u64) < b / 2),
            ok = out.is_ok(),
            "a file write reached disk — before/after sizes so a destructive overwrite is \
             visible without reconstructing it from the persona's account"
        );
        out
    }
}

// ─────────────────────────── code/edit ───────────────────────────
/// Edit an existing file (line-range / search-replace / insert / append).
pub struct CodeEdit {
    pub state: Arc<CodeState>,
}

/// The one true shape + the recovery guidance, kept in ONE place so the runtime error and
/// the field docs never drift.
pub(super) const EDIT_MODE_HELP: &str = "edit_mode must be ONE of (preferred = a tagged object): \
{\"type\":\"search_replace\",\"search\":\"old text\",\"replace\":\"new text\"} | \
{\"type\":\"line_range\",\"start_line\":N,\"end_line\":M,\"new_content\":\"...\"} | \
{\"type\":\"insert_at\",\"line\":N,\"content\":\"...\"} | \
{\"type\":\"append\",\"content\":\"...\"}. A bare mode string (e.g. \"append\") with the fields at top level is also accepted.";

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
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
    // `old_string`/`new_string` (Claude Code / most agents) and `old_str`/`new_str` are the
    // UNIVERSAL edit idiom every citizen — persona, Claude, or human-over-Positron — reaches for
    // first. Dogfooded 2026-08-09: a bare `code/edit old_string=… new_string=…` was refused
    // ("could not determine the edit mode") because these weren't fields. Serde aliases map them
    // straight onto search/replace at the shared handler, so search_replace is INFERRED and the
    // edit lands first-try for everyone. [[dogfood-the-continuum-command-surface]]
    #[serde(default, alias = "old_string", alias = "old_str")]
    pub search: Option<String>,
    #[serde(default, alias = "new_string", alias = "new_str")]
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

// Advertise one typed call shape; the serde structure above deliberately also accepts
// legacy flat and alias forms. Deriving its schema would make the defaulted operation
// optional (even `schemars(required)` cannot override serde(default) in schemars 0.8).
// Keep compatibility parsing out of the native tool contract and recovery example.
impl JsonSchema for CodeEditParams {
    fn schema_name() -> String {
        "CodeEditParams".into()
    }

    fn json_schema(generator: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        CanonicalEdit::json_schema(generator)
    }
}

/// The normalized edit consumed by the executor and advertised to callers.
#[derive(JsonSchema)]
pub(super) struct CanonicalEdit {
    /// Path relative to the workspace root.
    pub(super) file_path: String,
    /// Operation to apply.
    pub(super) edit_mode: EditMode,
    /// Optional note recorded in change history.
    pub(super) description: Option<String>,
}

impl TryFrom<CodeEditParams> for CanonicalEdit {
    type Error = CommandError;

    fn try_from(params: CodeEditParams) -> Result<Self, Self::Error> {
        let edit_mode = normalize_edit_mode(&params)?;
        Ok(Self {
            file_path: params.file_path,
            edit_mode,
            description: params.description,
        })
    }
}
