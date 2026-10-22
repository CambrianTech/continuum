//! A citizen's LAYER: her own shared clone of this node's checkout, the root of her hands
//! for repo work. Hands provisioning (path, first clone, refresh from the shared base), so
//! it lives with the hands; the code commands resolve an engine over it, and a mind that
//! needs her workspace (replay, the workspace map) asks it here.

use crate::code::git_bridge::{run_git, run_git_with_env};
use crate::sdk_codegen::CommandError;
use std::time::Duration;

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
    let base = crate::code::repo_registry::own_checkout()
        .await
        .map_err(|e| CommandError::Internal(format!("shared base unavailable: {e}")))?;
    ensure_citizen_layer_from_base(peer, &base).await
}

/// Provision (or refresh) a peer's citizen layer, cloning from `base` — the shared
/// checkout. Split out from [`ensure_citizen_layer`] so the base is an explicit
/// argument (production passes our recorded checkout; tests pass a scoped temp dir)
/// rather than an implicit read of process-global state. See the note above on why that
/// matters for the parallel test suite.
pub(crate) async fn ensure_citizen_layer_from_base(
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
pub(crate) async fn init_vendor_submodule(layer: &std::path::Path, base: &std::path::Path) -> String {
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
pub(crate) async fn protect_shared_objects(base: &std::path::Path) {
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
pub(crate) async fn share_layer_objects_with_base(layer: &std::path::Path, base: &std::path::Path) {
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
pub(crate) fn write_workspace_sync_note(layer: &std::path::Path, summary: &str) {
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
