use super::*;

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

    async fn run(
        &self,
        ctx: &Ctx,
        p: CodeShellKillParams,
    ) -> Result<CodeShellKillResult, CommandError> {
        self.state
            .with_execution(&hands_of(&self.state, ctx), &p.execution_id, |s, id| s.kill(id))
            .map_err(execution_lookup_error)?;
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
            .map_err(engine_err)
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
            .map_err(engine_err)
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
                    .map_err(|e| CommandError::Arguments(format!("invalid change_id: {e}")))?;
                let reverted = engine
                    .undo(&change_uuid)
                    .map_err(engine_err)?;
                Ok(crate::code::types::UndoResult {
                    success: true,
                    changes_undone: vec![reverted],
                    error: None,
                })
            }
            None => engine
                .undo_last(p.count.unwrap_or(1) as usize)
                .map_err(engine_err),
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

// ─────────────────── code/release-hands ───────────────────
/// End the caller's current acting context: kill anything its shells still run, wait each
/// one out, and only once every one is confirmed gone drop its workspace engine and shells.
/// What a fork's hands do when the fork is over (an eval or a solve), so its context leaves
/// nothing behind. A release that can't confirm keeps the handles and says so
/// (`released: false`), so a retry finds them. Refused for a caller's default (live) hands:
/// those are never ended by a verb (docs/architecture/HER-HANDS.md).
pub struct CodeReleaseHands {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeReleaseHandsParams {}

/// Whether a release may drop the context's handles: only when nothing is left unconfirmed
/// and no exit failed to be awaited. One place, so the command and its test agree.
pub(crate) fn release_confirmed(unconfirmed: &[u32], errors: &[String]) -> bool {
    unconfirmed.is_empty() && errors.is_empty()
}

#[async_trait]
impl ActionCommand for CodeReleaseHands {
    const NAME: &'static str = "code/release-hands";
    // Privileged like create-workspace: it ends a context's sandbox. A persona never calls it
    // as a tool; her fork's own hands send it when the fork is dropped.
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "End the caller's current acting context (an eval's or a fork's own hands): kill what its \
         shells still run, wait each one out, and only once every one is confirmed gone drop its \
         workspace engine and shells (released: true). Otherwise they are kept and the unconfirmed \
         pids are named (released: false), so a retry finds them. Refused for the caller's default \
         (live) hands.";
    type Params = CodeReleaseHandsParams;
    type Output = ReleaseHandsResult;

    async fn run(&self, ctx: &Ctx, _p: CodeReleaseHandsParams) -> Result<ReleaseHandsResult, CommandError> {
        let key = hands_of(&self.state, ctx);
        let Some(context) = key.context else {
            return Err(CommandError::Invalid(
                "code/release-hands ends an acting context's hands; these are the caller's default \
                 (live) hands, which are never ended by a verb"
                    .into(),
            ));
        };
        // THE ONE RELEASE BOUNDARY (Astra on #5026): a release is the kernel's, run with an
        // identity marked as releasing (never admitted to the hands it ends). It closes them to
        // new work and waits out every admitted command and turn, THEN ends what still runs.
        if !ctx.caller.as_ref().is_some_and(|c| c.releases_hands) {
            return Err(CommandError::Invalid(
                "code/release-hands is the kernel's release of an acting context; it runs as a release, \
                 never as work admitted to the hands it ends"
                    .into(),
            ));
        }
        crate::identity::hands_gate::close(context).await;
        let who = key.map_key();
        let pending = self.state.begin_release(&who);
        // Detached shells hold process custody, not foreground command admission.
        // Their owner has now received the kill request; drain their full task
        // lifetime before reading uncertainty or deleting the acting world.
        let lingering = crate::identity::hands_gate::drain_processes(context).await;
        // WAIT each one out: its own task sets `process_ended` after the reap, or
        // `process_end_error` when the reap itself failed, at the cadence the task notices a
        // kill (`KILL_POLL`). SIGKILL can't be ignored, so a direct child's reap always comes;
        // no timeout is guessed. A failed reap is NOT an exit: it stays unconfirmed. Then ask
        // the kernel, with positive evidence only, whether each reaped pid is gone.
        let mut pids = Vec::new();
        let mut reaped = Vec::new();
        let mut unconfirmed = Vec::new();
        let mut errors = Vec::new();
        for state in &pending {
            loop {
                let (ended, error, pid) = match state.lock() {
                    Ok(s) => (s.process_ended, s.process_end_error.clone(), s.pid),
                    Err(p) => (p.get_ref().process_ended, p.get_ref().process_end_error.clone(), p.get_ref().pid), // poisoned: read the last state
                };
                if let Some(why) = error {
                    pids.extend(pid);
                    unconfirmed.extend(pid);
                    errors.push(match pid {
                        Some(pid) => format!("pid {pid}: {why}"),
                        None => why,
                    });
                    break;
                }
                if ended {
                    pids.extend(pid);
                    reaped.extend(pid);
                    // What the shell started and the task couldn't confirm gone: kill each again
                    // while its pid still names it, then re-check. Each release is one recheck,
                    // so a retry can confirm what this one couldn't.
                    let survivors = match state.lock() {
                        Ok(s) => s.unconfirmed_descendants.clone(),
                        Err(p) => p.get_ref().unconfirmed_descendants.clone(), // poisoned: read the last state
                    };
                    if !survivors.is_empty() {
                        let still = tokio::task::spawn_blocking(move || continuum_process::recheck(&survivors))
                        .await
                        .map_err(|e| CommandError::Internal(format!("release-hands: re-checking descendants failed to run: {e}")))?;
                        if let Ok(mut s) = state.lock() {
                            s.unconfirmed_descendants = still.clone();
                        }
                        if !still.is_empty() {
                            unconfirmed.extend(still.iter().map(|who| who.pid));
                            errors.push(format!("descendants {:?} of pid {pid:?} are not confirmed gone", still.iter().map(|who| who.pid).collect::<Vec<_>>()));
                        }
                    }
                    break;
                }
                tokio::time::sleep(continuum_process::KILL_POLL).await;
            }
        }
        // What admitted work started and couldn't confirm gone (a gh's git): the same recheck
        // as a shell's survivors, and the hands are kept while any is present.
        if !lingering.processes.is_empty() {
            let processes = lingering.processes;
            let still = tokio::task::spawn_blocking(move || continuum_process::recheck(&processes))
                .await
                .map_err(|e| CommandError::Internal(format!("release-hands: re-checking its work's processes failed to run: {e}")))?;
            if !still.is_empty() {
                unconfirmed.extend(still.iter().map(|who| who.pid));
                errors.push(format!("processes {:?} its work started are not confirmed gone", still.iter().map(|who| who.pid).collect::<Vec<_>>()));
            }
            crate::identity::hands_gate::retain_processes(context, still);
        }
        // a tree no walk could read: nothing can confirm it, so the hands are held and it is named
        errors.extend(lingering.unknown.iter().map(|why| format!("a process its work started ended unconfirmed: {why}")));
        let still_there = tokio::task::spawn_blocking(move || {
            reaped
                .into_iter()
                .filter(|pid| !matches!(crate::inference::engine_residency::probe_process(*pid), crate::inference::engine_residency::ProcessProbe::Absent))
                .collect::<Vec<u32>>()
        })
        .await
        .map_err(|e| CommandError::Internal(format!("release-hands: confirming the kills failed to run: {e}")))?;
        unconfirmed.extend(still_there);
        let released = release_confirmed(&unconfirmed, &errors);
        if released {
            self.state.finish_release(&who);
        }
        continuum_base::probe!(
            class = "code.hands.released",
            caller = key.caller.as_str(),
            context = %context.id(),
            killed = pids.len() as u64,
            unconfirmed = unconfirmed.len() as u64,
            released = released,
            "an acting context's executions were waited out; its engine and shells are dropped only when every one is confirmed gone"
        );
        Ok(ReleaseHandsResult { context: context.id().to_string(), killed: pids, unconfirmed, errors, released })
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
        "Bind the caller's hands (in their acting context) to a workspace sandbox at a specific \
         path, with optional read-only roots. Hands are bound once: binding them to another root \
         is refused; an eval, a fork or a card claim opens hands of its own.";
    type Params = CodeCreateWorkspaceParams;
    type Output = CreateWorkspaceResult;

    async fn run(
        &self,
        ctx: &Ctx,
        p: CodeCreateWorkspaceParams,
    ) -> Result<CreateWorkspaceResult, CommandError> {
        // These hands, in this caller's acting context: an eval's create-workspace roots the
        // eval's hands, never her live ones (docs/architecture/HER-HANDS.md).
        let key = &hands_of(&self.state, ctx);
        // The OWNER of a workspace refuses one for a card turn's unbound stand-in: those hands
        // exist so code tools refuse rather than act anywhere (Astra on #5026).
        if key.context.is_some_and(|c| c.kind() == crate::identity::hands_context::ContextKind::Unbound) {
            return Err(CommandError::Invalid(
                "these are a card turn's unbound hands (its checkout couldn't be bound): they never get \
                 a workspace, so code tools can't act in them this turn"
                    .into(),
            ));
        }
        let who = key.map_key();
        let root = std::path::Path::new(&p.workspace_root);
        let mut security = PathSecurity::new(root).map_err(|e| {
            CommandError::Invalid(format!(
                "invalid workspace root '{}': {e}",
                p.workspace_root
            ))
        })?;
        for rr in &p.read_roots {
            security
                .add_read_root(std::path::Path::new(rr))
                .map_err(|e| CommandError::Invalid(format!("invalid read root '{rr}': {e}")))?;
        }
        // HANDS ARE BOUND ONCE (card fc9e9fd7). These hands get this workspace if they have
        // none; the same root again is a no-op; ANOTHER root is refused by name. Moving one
        // set of hands between roots is what split Kimi's shell from her code tools when two
        // turns moved them (#11697); an eval, a fork or a card claim opens hands of its own
        // instead. Keyed by caller and context, never a spoofable persona_id param.
        let policy = if p.refuse_inert_edits {
            crate::code::file_engine::WritePolicy::RefuseInert
        } else {
            crate::code::file_engine::WritePolicy::Warn
        };
        let canon = |p: &std::path::Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()); // unwrap_or_else: an uncanonicalizable path compares as written
        match self.state.file_engines.entry(who.clone()) {
            dashmap::mapref::entry::Entry::Occupied(bound) => {
                let at = bound.get().workspace_root();
                if canon(&at) != canon(root) {
                    return Err(CommandError::Invalid(format!(
                        "these hands are bound to '{}'; hands are bound once and never moved. Work \
                         at '{}' in hands of its own (an eval, a fork or a card claim opens them)",
                        at.display(),
                        p.workspace_root
                    )));
                }
            }
            dashmap::mapref::entry::Entry::Vacant(free) => {
                free.insert(FileEngine::new(&key.caller, security).with_write_policy(policy));
            }
        }
        if !p.path_prepend.is_empty() {
            let who = ensure_shell(&self.state, ctx).await?;
            if let Some(mut shell) = self.state.shell_sessions.get_mut(&who) {
                let prepend = p.path_prepend.join(":");
                // Hand the per-task prefix to the shell as CONTINUUM_PATH_PREPEND, NOT as a
                // reconstructed PATH. The shell runs as the user's LOGIN shell
                // (shell_session.rs), which already carries the user's full toolchain and
                // re-sets PATH from the profile — so an inherited PATH override is clobbered.
                // The shell prepends this prefix AFTER the profile loads (like `activate`), so
                // the era venv layers on top of the user's real environment. No hand-rolled PATH.
                shell.set_env("CONTINUUM_PATH_PREPEND".to_string(), prepend.clone());
                // HER TREE MUST WIN THE IMPORT (due-diligence find, 2026-08-08,
                // gold-through-her-hands on sympy-24066): the era venv's package is
                // `pip install -e` bound to the GRADER'S work repo, so `import sympy`
                // from her python loaded the pristine grader tree and HER EDITS WERE
                // INVISIBLE TO HER OWN TEST RUNS — anti-verification: a perfect fix
                // still "failed" when she checked it. Positive-control proven both
                // ways: with PYTHONPATH at her workspace the issue repro fails
                // pre-fix and passes post-fix through her exact edit semantics.
                // PYTHONPATH precedes the venv's .pth entries, so her tree wins.
                shell.set_env("PYTHONPATH".to_string(), p.workspace_root.clone());
                continuum_base::probe!(
                    class = "code.workspace.path_prepend",
                    caller = who.as_str(),
                    prepend = prepend.as_str(),
                    pythonpath = p.workspace_root.as_str(),
                    "granted the caller's shell an era PATH prefix + PYTHONPATH at her \
                     workspace so her edits are what her interpreter imports"
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

// ─────────────── the shell's own verdict (card f6c50a49) ────────────────
/// Project a shell execution's OWN declared status into the executor's verdict.
///
/// No key scanning and no guessing: `ShellExecutionStatus` is already the typed
/// terminal state the session computes at
/// `shell_session.rs` (`status.success()` → `Completed`, else `Failed`), and
/// `code/shell` documents itself as "always returns immediately with the
/// execution handle". Reading that field is reading what the command declares.
///
/// `Running` is deliberately NOT collapsed into either terminal arm: a handed-back
/// handle is an ACCEPTED command, not a completed one, and only a carrier that can
/// say so can stop a receipt from claiming work that has not finished.
pub(super) fn shell_verdict(r: &ShellExecuteResponse) -> ToolVerdict {
    match r.status {
        ShellExecutionStatus::Running => ToolVerdict::Running,
        ShellExecutionStatus::Completed => ToolVerdict::Succeeded,
        // A nonzero exit, a timeout and a kill are all failures the model must
        // SEE — the stderr that comes with them is the feedback it acts on.
        ShellExecutionStatus::Failed
        | ShellExecutionStatus::TimedOut
        | ShellExecutionStatus::Killed => ToolVerdict::Failed,
    }
}

/// The execution handle this response hands back, declared by the COMMAND.
///
/// The act seam used to recover this by re-parsing the result TEXT
/// (`apply.rs`, pre-#4352) — which read the FOLDED preview, so a flood-sized
/// result no longer parsed, and matched the command name by hand so a `bash`
/// alias never reached the branch at all. Declared here, it is decoded once from
/// the pre-fold value alongside the verdict.
pub(super) fn shell_handle(r: &ShellExecuteResponse) -> Option<uuid::Uuid> {
    uuid::Uuid::parse_str(&r.execution_id).ok()
}

impl crate::sdk_codegen::ProjectsOutcome for CodeShell {
    fn outcome(output: &ShellExecuteResponse) -> ToolVerdict {
        shell_verdict(output)
    }
    fn dispatch_handle(output: &ShellExecuteResponse) -> Option<uuid::Uuid> {
        shell_handle(output)
    }
}

/// The poll half reads the SAME projection, so a `code/shell` handed back as
/// `Running` and the `code/shell-poll` that later observes it terminal cannot
/// disagree about what the same execution did.
impl crate::sdk_codegen::ProjectsOutcome for CodeShellPoll {
    fn outcome(output: &ShellExecuteResponse) -> ToolVerdict {
        shell_verdict(output)
    }
    fn dispatch_handle(output: &ShellExecuteResponse) -> Option<uuid::Uuid> {
        shell_handle(output)
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

crate::register_command!(CodeReleaseHands);

crate::register_command!(CodeCreateWorkspace);

// Opt-in outcome projection: ONLY these two commands pay the `DeserializeOwned`
// bound, and only they change how the executor flags their results.
crate::register_outcome!(CodeShell);

crate::register_outcome!(CodeShellPoll);

/// The dep-holding command objects the [`CodeModule`](super::code::CodeModule)
/// contributes to the kernel's typed object map (via `ServiceModule::commands`),
/// so the executor routes each name DIRECTLY to it — winning over the legacy
/// prefix → `handle_command` arm, which is deleted for these commands.
pub fn command_objects(state: Arc<CodeState>) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(CodeRead {
            state: state.clone(),
        }),
        Arc::new(CodeWrite {
            state: state.clone(),
        }),
        Arc::new(CodeEdit {
            state: state.clone(),
        }),
        Arc::new(CodeList {
            state: state.clone(),
        }),
        Arc::new(CodeExists {
            state: state.clone(),
        }),
        Arc::new(CodeGlob {
            state: state.clone(),
        }),
        Arc::new(CodeTree {
            state: state.clone(),
        }),
        Arc::new(CodeSearch {
            state: state.clone(),
        }),
        Arc::new(CodeShell {
            state: state.clone(),
        }),
        Arc::new(CodeShellPoll {
            state: state.clone(),
        }),
        Arc::new(CodeShellKill {
            state: state.clone(),
        }),
        Arc::new(CodeDelete {
            state: state.clone(),
        }),
        Arc::new(CodeDiff {
            state: state.clone(),
        }),
        Arc::new(CodeUndo {
            state: state.clone(),
        }),
        Arc::new(CodeHistory {
            state: state.clone(),
        }),
        Arc::new(CodeReleaseHands {
            state: state.clone(),
        }),
        Arc::new(CodeCreateWorkspace { state }),
    ]
}
