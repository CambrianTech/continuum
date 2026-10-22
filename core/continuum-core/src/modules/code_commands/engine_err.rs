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
pub(super) fn read_scope_of(ctx: &Ctx, workspace: &std::path::Path) -> crate::code::path_security::ReadScope {
    use crate::code::path_security::ReadScope;
    let trust = crate::routing::grid_trust_policy::caller_trust(ctx.caller.as_ref());
    if !crate::modules::grid::acl::is_command_authorized("code/shell", trust) {
        return ReadScope::Sandbox;
    }
    match confinement_of(ctx, workspace) {
        None => ReadScope::Host { deny: Vec::new() },
        Some(confinement) if confinement.reaches_host() => ReadScope::Host { deny: confinement.host_denials() },
        Some(confinement) => ReadScope::Confined(confinement.materialize()),
    }
}

/// THE one resolver for a verb that takes a path from its caller and reads the file
/// itself (`vision/look`, and any later one): the caller's engine, the caller's read
/// scope, the engine's own security. Returns the absolute canonical path, or the same
/// refusal `code/read` gives — named, correctable, never a bare I/O error.
pub(crate) async fn resolve_readable_path(
    state: &CodeState,
    ctx: &Ctx,
    path: &str,
) -> Result<std::path::PathBuf, CommandError> {
    let key = hands_of(ctx);
    ensure_engine(state, &key).await?;
    let engine = state
        .file_engines
        .get(&key.map_key())
        .ok_or_else(|| CommandError::Internal("workspace vanished after provisioning".into()))?;
    let scope = read_scope_of(ctx, &engine.workspace_root());
    engine
        .security()
        .validate_read_in(path, scope)
        .map_err(|e| engine_err(crate::code::file_engine::FileEngineError::from(e)))
}

/// The persona/owner this tool call acts AS — the authenticated caller identity
/// (an airc `peer_id`), never a params field. `None` caller is the
/// substrate-local owner. This is the single point that maps the gated identity
/// to the per-caller workspace; nothing trusts caller-supplied identity.
pub(crate) fn caller_id(ctx: &Ctx) -> String {
    // ONE resolver (`operator_peer::acting_peer_id`): the same peer the work verbs
    // claim as. `LOCAL_OWNER` survives only for the boot window before the
    // self-peers are online.
    crate::persona::operator_peer::acting_peer_id(ctx)
        .map(|p| p.to_string())
        .unwrap_or_else(|| LOCAL_OWNER.to_string()) // unwrap_or_else: boot window, no self-peer yet — the core's cwd, named below
}

pub(crate) const LOCAL_OWNER: &str = "local-owner";

/// Whose hands this call acts with: the caller, in the acting context its connection is
/// bound to (`CallerIdentity::hands`, set by the kernel side, never by params). The one
/// key every hands map uses; the caller alone stays the authorization identity
/// ([`caller_id`]). See [`crate::code::hands`].
pub(crate) fn hands_of(ctx: &Ctx) -> HandsKey {
    HandsKey::new(caller_id(ctx), ctx.caller.as_ref().and_then(|c| c.hands))
}

/// Resolve (and provision on first use) the citizen LAYER for an identified peer
/// caller: a copy-on-write clone of the shared base (our recorded checkout, `repo_registry::own_checkout`)
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
    let home = crate::paths::continuum_home()
        .map_err(|e| CommandError::Internal(format!("citizen layer: {e}")))?;
    Ok(home
        .join("citizens")
        .join("peers")
        .join(peer)
        .join("workspace"))
}

pub(crate) async fn ensure_citizen_layer(peer: &str) -> Result<std::path::PathBuf, CommandError> {
    // The shared base is OUR checkout, as this node recorded it (never the cwd: a
    // launchd core runs in its data dir, card 2a2cd6f1). Resolved ONCE here, then handed
    // to the provisioning body as a concrete base so it is a pure function of
    // (peer, base): the seam that lets the citizen-layer test inject a fake base without
    // touching process-global state (#191).
    let base = crate::modules::repo_registry::own_checkout()
        .await
        .map_err(|e| CommandError::Internal(format!("shared base unavailable: {e}")))?;
    ensure_citizen_layer_from_base(peer, &base).await
}

/// Provision (or refresh) a peer's citizen layer, cloning from `base` — the shared
/// checkout. Split out from [`ensure_citizen_layer`] so the base is an explicit
/// argument (production passes our recorded checkout; tests pass a scoped temp dir)
/// rather than an implicit read of process-global state. See the note above on why that
/// matters for the parallel test suite.
pub(super) async fn ensure_citizen_layer_from_base(
    peer: &str,
    base: &std::path::Path,
) -> Result<std::path::PathBuf, CommandError> {
    let layer = citizen_layer_path(peer)?;
    if layer.is_dir() {
        share_layer_objects_with_base(&layer, base).await;
        // Self-heal the stale-clone bug ([[citizen-workspaces-are-stale-one-time-
        // clones]]): the layer was a ONE-TIME CoW snapshot that never refreshed, so
        // personas drifted stale (one froze before the workers/→core/ restructure).
        // Now, on every ensure (≈once per persona per boot), sync it forward from
        // the current shared checkout — PRESERVING the persona's work (the sync
        // autocommits + merges, shared wins framework conflicts). Non-fatal: a sync
        // that fails logs LOUD but still returns the usable (if stale) workspace
        // rather than denying the persona her hands.
        match crate::code::git_bridge::git_sync_from_shared(&layer, base).await {
            Ok(report) if report.synced => {
                write_workspace_sync_note(&layer, &report.summary);
                continuum_base::probe!(
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
    // A SHARED CLONE, never a copy (2026-09-06, the day the M5 hit zero free). The
    // previous shape — `cp -cR` of the whole checkout — was copy-on-write for a day and
    // 7–20 GB per citizen a week later: every clone carried the canonical repo's full
    // history, its stale worktree metadata (5 × 462 MB of vendored packs), 4.5 GB of
    // gitignored model caches and node_modules, and as the base moved on, every block
    // materialized. `git clone --shared` writes ONLY the tracked tree; objects come from
    // the base through `.git/objects/info/alternates`, so the layer's marginal disk is
    // her working tree plus her own commits. `git_sync_from_shared` (fetch + merge) is
    // unchanged — it never depended on the copy.
    // The clone source is the base's repository ROOT: production hands our recorded
    // checkout (its root), but a caller standing inside the tree (a test in
    // core/continuum-core) means the same repository, and `git clone` wants the root.
    // A base outside any repository is a loud error — a citizen layer without a
    // repository is not a workspace.
    // This one call lifts the runner's discovery ceiling by name: finding the root from
    // inside the tree IS the upward discovery.
    let toplevel = run_git_with_env(base, &["rev-parse", "--show-toplevel"], &[("GIT_CEILING_DIRECTORIES", "")])
        .await
        .map_err(|e| {
            CommandError::Internal(format!(
                "citizen layer: base {} is not inside a git repository ({e}) — a layer is a shared \
                 clone of the checkout and cannot be provisioned from a bare directory",
                base.display()
            ))
        })?;
    let repo_root = std::path::PathBuf::from(toplevel.trim());
    protect_shared_objects(&repo_root).await;
    let (source, target) = (repo_root.to_string_lossy(), layer.to_string_lossy());
    if let Err(e) = run_git(&repo_root, &["clone", "--shared", "--quiet", &source, &target]).await {
        // Never leave a half-materialized layer for the next call to mistake
        // for a real one.
        let _ = std::fs::remove_dir_all(&layer);
        return Err(CommandError::Internal(format!(
            "citizen layer shared clone failed for peer {peer} (base {}): {e}",
            base.display()
        )));
    }
    // The vendored inference engine rides along as a submodule; reference the base's
    // copy so its objects are shared too. Bounded and named: a layer without the vendor
    // tree still gives her hands (the core serves inference, not her checkout).
    let vendor = init_vendor_submodule(&layer, &repo_root).await;
    continuum_base::probe!(
        class = "workspace.layer.provision",
        peer = %peer,
        base = %base.display(),
        layer = %layer.display(),
        vendor = %vendor,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "citizen layer provisioned — shared clone of the base (objects via alternates, tracked tree only)"
    );
    Ok(layer)
}

/// `git submodule update --init --reference <base>/<path>` for the vendored engine,
/// within its ten-minute bound (carried from the poll loop this replaced, unmeasured —
/// card c1bdc29b measures the git bounds). The git and what it starts end at the bound.
/// Returns the named outcome for the probe: `shared`, `absent` (the base has no vendor
/// tree — a test base), `failed: <why>`.
pub(super) async fn init_vendor_submodule(layer: &std::path::Path, base: &std::path::Path) -> String {
    const VENDOR: &str = "core/vendor/llama.cpp";
    const BOUND: Duration = Duration::from_secs(600);
    let reference = base.join(VENDOR);
    if !reference.join(".git").exists() && !reference.join("HEAD").exists() {
        return "absent".to_string();
    }
    let reference = reference.to_string_lossy();
    let args = ["submodule", "update", "--init", "--quiet", "--reference", &reference, VENDOR];
    let deadline = tokio::time::Instant::now() + BOUND;
    match crate::code::git_bridge::within(deadline, run_git(layer, &args)).await {
        Ok(_) => "shared".to_string(),
        Err(why) if crate::code::git_bridge::out_of_time(&why) => format!("failed: timed out after {} s: {why}", BOUND.as_secs()),
        Err(why) => format!("failed: {why}"),
    }
}

/// The moment a layer starts borrowing the base's objects, the base stops being
/// safe to gc: `git-clone(1)` on `--shared` — "if you delete branches in the source
/// repository, some objects may become dangling" — and a routine auto-gc in the base
/// would then corrupt EVERY resident citizen's workspace at once (review on #3795).
/// `extensions.preciousObjects = true` makes gc/prune in the base refuse to drop
/// objects. Idempotent; a failure to set it is loud and the provisioning stops,
/// because a layer over an unprotected base is a repo-corruption timer, not a workspace.
pub(super) async fn protect_shared_objects(base: &std::path::Path) {
    let already = run_git(base, &["config", "--get", "extensions.preciousObjects"])
        .await
        .is_ok_and(|v| v.trim() == "true"); // a failed read means "not known to be set" — we set it below and check that
    if already {
        return;
    }
    let set = run_git(base, &["config", "extensions.preciousObjects", "true"]).await.is_ok();
    continuum_base::probe!(
        class = "workspace.layer.base_protected",
        base = %base.display(),
        set,
        "extensions.preciousObjects on the base repository — the citizens' shared objects survive its gc"
    );
    if !set {
        tracing::error!(
            base = %base.display(),
            "could not set extensions.preciousObjects on the base — every shared citizen layer is one base gc away from corruption"
        );
    }
}

/// Migrate a layer provisioned by the old `cp -cR` shape to share its objects with
/// the base: write `alternates`, repack keeping only objects the base lacks, and drop
/// the worktree metadata the copy inherited from the base. Idempotent (a layer that
/// already names the base's objects is left alone); every outcome named in a probe.
pub(super) async fn share_layer_objects_with_base(layer: &std::path::Path, base: &std::path::Path) {
    let objects = layer.join(".git").join("objects");
    let alternates = objects.join("info").join("alternates");
    let base_objects = base.join(".git").join("objects");
    if !objects.is_dir() || !base_objects.is_dir() {
        return;
    }
    let want = format!("{}\n", base_objects.display());
    protect_shared_objects(base).await;
    if std::fs::read_to_string(&alternates).map(|s| s == want).unwrap_or(false) { // unwrap_or: no/unreadable alternates = not yet shared; the migration below writes it
        return;
    }
    let started = std::time::Instant::now();
    let outcome = async {
        std::fs::create_dir_all(objects.join("info")).map_err(|e| e.to_string())?;
        std::fs::write(&alternates, &want).map_err(|e| e.to_string())?;
        run_git(layer, &["repack", "-a", "-d", "-l", "-q"]).await?;
        // Worktree entries the copy inherited point at the BASE's trees, never hers.
        let mut dropped = 0usize;
        if let Ok(entries) = std::fs::read_dir(layer.join(".git").join("worktrees")) {
            for entry in entries.flatten() {
                let gitdir = std::fs::read_to_string(entry.path().join("gitdir")).unwrap_or_default(); // unwrap_or_default: an unreadable gitdir marker is a broken entry the copy inherited — empty never names this layer, so it is dropped with the foreign ones
                if !gitdir.trim().starts_with(&layer.display().to_string()) {
                    let _ = std::fs::remove_dir_all(entry.path());
                    dropped += 1;
                }
            }
        }
        Ok::<_, String>(format!("shared; foreign worktree entries dropped: {dropped}"))
    }
    .await;
    match outcome {
        Ok(summary) => continuum_base::probe!(
            class = "workspace.layer.objects_shared",
            layer = %layer.display(),
            summary = %summary,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "citizen layer now shares its objects with the base"
        ),
        Err(why) => tracing::warn!(
            layer = %layer.display(),
            why = %why,
            "citizen layer object sharing failed — layer keeps its own objects"
        ),
    }
}

/// Drop a short teaching note at the workspace root after a shared sync — the
/// "notify + teach" half of the self-heal (Joel 2026-07-13: "preserve and notify…
/// eventually just learn this"). The persona reads it through her normal code
/// tools and learns the workflow: her workspace is a live copy of shared,
/// refreshed each boot, and committing her work is how it survives. Best-effort —
/// a note we couldn't write never blocks her hands. This is the bootstrap of the
/// habit; the durable form is an onboarding lesson she internalizes
/// ([[onboarding-lora-tool-fluency-degree-system]]).
pub(super) fn write_workspace_sync_note(layer: &std::path::Path, summary: &str) {
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
    let local = is_local_who(caller);
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
        None if local => crate::modules::repo_registry::own_checkout()
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
pub(super) fn is_local_who(who: &str) -> bool {
    who == LOCAL_OWNER
        || uuid::Uuid::parse_str(who)
            .map(crate::persona::operator_peer::is_local_identity)
            .unwrap_or(false) // unwrap_or: not a uuid = not a self-peer
}

/// Where THESE hands stand, for a command that runs outside the engine (`code/run`): the
/// root this caller's acting context was rooted at (a held card's checkout on a work turn,
/// an exam directory for an eval), from the one acting-root record her shell and file hands
/// are rooted through. A context that was never rooted gets `None` (its snippet runs in
/// scratch), never the live checkout (Astra's review of #5005). The caller's default context
/// keeps the local identity's held-card lookup.
pub(crate) async fn held_root_for(ctx: &Ctx) -> Option<std::path::PathBuf> {
    let key = hands_of(ctx);
    let persona = uuid::Uuid::parse_str(&key.caller).ok()?;
    crate::cognition::persona_workspace::acting_root_in(persona, key.context)
}

/// The prepared instance environment's python for the root these hands stand at
/// ([`held_root_for`]), if it has one: the interpreter `code/run` runs a snippet with (card
/// 533c2d78). `None` = no root, or no prepared env, and the caller falls back to the PATH
/// interpreter, named.
pub(crate) async fn held_env_python_for(ctx: &Ctx) -> Option<std::path::PathBuf> {
    let root = held_root_for(ctx).await?;
    crate::persona::instance_env_fact::instance_python(&root)
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
    let key = &hands_of(ctx);
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
    if let Some(confinement) = confinement_of(ctx, &root) {
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
pub(crate) fn confinement_of(ctx: &Ctx, workspace: &std::path::Path) -> Option<crate::code::confinement::Confinement> {
    let resident = ctx.caller.as_ref().is_some_and(|c| c.source == crate::routing::auth_policy::CallerSource::LocalPersona);
    let trust = crate::routing::grid_trust_policy::caller_trust(ctx.caller.as_ref());
    if !resident && trust == crate::modules::grid::node::TrustLevel::Owner {
        return None;
    }
    let home = crate::paths::home_dir()?;
    let continuum_root = crate::paths::continuum_home().ok()?;
    let confinement = crate::code::confinement::Confinement::for_caller(&home, &continuum_root, &caller_id(ctx), workspace);
    Some(if resident { confinement.reaching_host() } else { confinement })
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
}

#[async_trait]
impl ActionCommand for CodeRead {
    const NAME: &'static str = "code/read";
    const ALIASES: &'static [&'static str] = &["read_file", "view", "cat", "open_file"];
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
            .read_in(&p.file_path, p.start_line, p.end_line, read_scope_of(ctx, &engine.workspace_root()))
            .map_err(engine_err)
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
        let scope = read_scope_of(ctx, &engine.workspace_root());
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
