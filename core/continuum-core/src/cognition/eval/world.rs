//! The exam's world: the per-run copy-on-write workspace, its run marker and in-flight listing,
//! the orphan sweep for worlds a killed process left behind, the unresolved-hands keep rule, and
//! `EvalWorld`, which keeps the world until every fork's hands in it are confirmed ended.

use super::*;

/// #312 — the exam's disposable world: a CoW clone of the shared checkout under the
/// system temp dir, one per run. clonefile on APFS / reflink on btrfs-XFS (instant,
/// block-sharing), plain recursive copy where the FS lacks CoW — the same platform
/// split as citizen-layer provisioning (`ensure_citizen_layer_from_base`). `cfg!`
/// (not `#[cfg]`) so both arms type-check on every platform.
pub(super) fn provision_ephemeral_eval_root(tag: &str) -> Result<std::path::PathBuf, String> {
    let base = std::env::current_dir().map_err(|e| format!("cwd: {e}"))?;
    let root = std::env::temp_dir().join("continuum-eval").join(tag);
    let parent = root
        .parent()
        .ok_or_else(|| "eval root has no parent".to_string())?
        .to_path_buf();
    sweep_orphan_eval_roots(&parent, &root);
    if root.is_dir() {
        write_eval_run_marker(&root, tag); // resume re-stamps the owner pid (a new core owns it now)
        return Ok(root); // resume of the same run reuses its world
    }
    std::fs::create_dir_all(&parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    let mut clone = std::process::Command::new("cp");
    if cfg!(target_os = "macos") {
        clone.arg("-cR");
    } else {
        clone.args(["--reflink=auto", "-R"]);
    }
    let out = clone
        .arg(&base)
        .arg(&root)
        .output()
        .map_err(|e| format!("cp spawn: {e}"))?;
    if !out.status.success() {
        let _ = std::fs::remove_dir_all(&root); // never leave a half-clone
        return Err(format!(
            "CoW clone {} -> {} failed: {}",
            base.display(),
            root.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    crate::probe!(
        class = "eval.workspace.provisioned",
        root = root.display().to_string().as_str(),
        "exam world provisioned — CoW clone of the shared checkout, removed when the run ends"
    );
    write_eval_run_marker(&root, tag);
    Ok(root)
}

/// Marker filename inside each eval world naming the run and its owning core
/// pid — the cross-process evidence `continuum reboot`'s guard reads. Solve
/// runs got this day one (the grade-ledger `state: running` rows); eval runs
/// had NO on-disk in-flight signal, so a reboot killed them without ever being
/// asked (measured 2026-08-23: the guard named zero runs while a MirrorCode
/// battery was mid-task). The world dir dies with the run (Drop) and orphans
/// are swept at next provision, so marker lifetime is exactly run lifetime;
/// the guard pid-checks to ignore debris from a killed core.
pub const EVAL_RUN_MARKER: &str = "eval-run.json";

fn write_eval_run_marker(root: &std::path::Path, run_id: &str) {
    let marker = serde_json::json!({
        "runId": run_id,
        "corePid": std::process::id(),
        "startedAtMs": EpochMs::now().millis(),
    });
    // Marker write is best-effort: the run must not die because its guard
    // breadcrumb could not be written; the reboot guard just loses this run.
    if let Err(e) = std::fs::write(
        root.join(EVAL_RUN_MARKER),
        marker.to_string(), // disk marker read by the CLI process — a true process boundary
    ) {
        tracing::warn!(error = %e, "eval run marker not written — reboot guard cannot see this run");
    }
}

/// Every eval run some world dir claims is IN FLIGHT: `(run_id, core_pid)`.
/// The CALLER owns pid-liveness (the CLI has its own pid_alive; this crate-side
/// scan stays pure file reading) — a marker whose core died is debris the next
/// provision sweeps, not an in-flight run.
pub fn in_flight_eval_runs() -> Vec<(String, u32)> {
    let base = std::env::temp_dir().join("continuum-eval");
    let Ok(entries) = std::fs::read_dir(&base) else {
        return Vec::new();
    };
    let mut live = Vec::new();
    for entry in entries.flatten() {
        let marker = entry.path().join(EVAL_RUN_MARKER);
        let Ok(text) = std::fs::read_to_string(&marker) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let (Some(run), Some(pid)) = (
            v.get("runId").and_then(|r| r.as_str()),
            v.get("corePid").and_then(|p| p.as_u64()),
        ) else {
            continue;
        };
        live.push((run.to_string(), pid as u32));
    }
    live.sort();
    live
}

/// Eval worlds LIVE in this process. `Drop` covers every in-process return path,
/// but not a killed process: `continuum reboot` SIGTERMs the core, `Drop` never
/// runs, and the run's world outlives it (measured 2026-08-23 — a reboot mid-run
/// left a 31 GB clone under `$TMPDIR/continuum-eval`, found by hand during the
/// Ornith battery). The sweep below deletes those orphans at the next provision;
/// this set is what stops it deleting a CONCURRENT run's live world.
static LIVE_EVAL_ROOTS: std::sync::OnceLock<
    parking_lot::Mutex<std::collections::HashSet<std::path::PathBuf>>,
> = std::sync::OnceLock::new();

pub(super) fn live_eval_roots() -> &'static parking_lot::Mutex<std::collections::HashSet<std::path::PathBuf>> {
    LIVE_EVAL_ROOTS.get_or_init(|| parking_lot::Mutex::new(std::collections::HashSet::new()))
}

/// Remove sibling eval worlds that belong to no live run in this process. An
/// eval run cannot survive its process (the mind fork, the lane lease, and the
/// progress reporter all die with it), so any world under `continuum-eval/`
/// that is neither the run being provisioned nor in [`live_eval_roots`] is an
/// orphan from a killed process — re-creatable debris by construction (#312:
/// the world is a CoW clone of the checkout; nothing in it is the only copy).
/// Event-driven at the point of use — no boot hook, no background tick.
pub(super) fn sweep_orphan_eval_roots(parent: &std::path::Path, keep: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return; // parent not created yet — nothing to sweep
    };
    // Snapshot the live set and release the lock BEFORE deleting: a multi-GB
    // clone takes seconds to remove, and a concurrent provision must not queue
    // behind filesystem work.
    let live = live_eval_roots().lock().clone();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || path == keep || live.contains(&path) {
            continue;
        }
        if let Some(why) = unresolved_hands_keep_reason(&path) {
            tracing::warn!(root = %path.display(), reason = %why, "an eval world kept for unresolved hands is not swept");
            continue;
        }
        match std::fs::remove_dir_all(&path) {
            Ok(()) => crate::probe!(
                class = "eval.workspace.orphan_swept",
                root = path.display().to_string().as_str(),
                "orphaned exam world from a killed process removed — Drop cannot run \
                 through SIGTERM, so provision-time sweep is the crash-path owner"
            ),
            Err(e) => tracing::warn!(
                root = %path.display(),
                error = %e,
                "orphaned eval world could not be removed — temp-dir debris persists"
            ),
        }
    }
}

/// RAII holder for the run's disposable world — `Drop` removes it on EVERY return
/// path (score, infra-abort, error), so a crashed exam never leaks fixtures into
/// the temp dir long-term and NEVER touches the shared checkout at all. The
/// crash path Drop cannot cover (SIGKILL/SIGTERM) is owned by
/// [`sweep_orphan_eval_roots`] at the next provision.
///
/// Once a fork's hands can act in it, the world is owned by an [`EvalWorld`], which keeps it
/// until every set of hands is confirmed ended (Astra on #5005).
pub(crate) struct EphemeralEvalRoot {
    pub(super) path: std::path::PathBuf,
    retained: bool,
}

impl EphemeralEvalRoot {
    pub(crate) fn new(path: std::path::PathBuf) -> Self {
        live_eval_roots().lock().insert(path.clone());
        Self { path, retained: false }
    }

    /// Keep this world past the run: a fork's hands in it weren't confirmed ended. It stays
    /// registered (this process's sweep spares it), and an [`UNRESOLVED_HANDS_MARKER`] records
    /// which pids weren't confirmed gone, so a later process's sweep deletes it only once each
    /// is positively absent. A restart alone proves nothing: a child needn't die with its parent.
    pub(super) fn retain_unresolved(mut self, pids: Option<Vec<u32>>) {
        let marker = UnresolvedHands { pids };
        let written = serde_json::to_string(&marker) // disk marker read by a later process's sweep — a true process boundary
            .map_err(|e| e.to_string())
            .and_then(|body| std::fs::write(self.path.join(UNRESOLVED_HANDS_MARKER), body).map_err(|e| e.to_string()));
        if let Err(e) = written {
            tracing::error!(root = %self.path.display(), error = %e, "an unresolved eval world's marker could not be written; only this process's registry keeps it from the sweep");
        }
        tracing::error!(
            root = %self.path.display(),
            pids = ?marker.pids,
            "an eval world is KEPT: a fork's hands in it were not confirmed ended, so it is not deleted under them"
        );
        self.retained = true;
    }
}

/// Written into an eval world kept because a fork's hands in it weren't confirmed ended.
pub(crate) const UNRESOLVED_HANDS_MARKER: &str = "unresolved-hands.json";

/// What [`UNRESOLVED_HANDS_MARKER`] holds: the pids not confirmed gone, or `None` when the
/// release couldn't report them (then nothing proves the world unused, and it is kept).
#[derive(Debug, Serialize, Deserialize)]
struct UnresolvedHands {
    pids: Option<Vec<u32>>,
}

/// Why a world must NOT be swept, or `None` when nothing recorded says so: a kept world is
/// swept only once every pid its marker names is positively absent.
fn unresolved_hands_keep_reason(root: &std::path::Path) -> Option<String> {
    let body = match std::fs::read_to_string(root.join(UNRESOLVED_HANDS_MARKER)) {
        Ok(body) => body,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => return Some(format!("its unresolved-hands marker can't be read: {e}")),
    };
    let marker: UnresolvedHands = match serde_json::from_str(&body) {
        Ok(marker) => marker,
        Err(e) => return Some(format!("its unresolved-hands marker isn't its own shape: {e}")),
    };
    let Some(pids) = marker.pids else {
        return Some("its fork's release reported no pids, so nothing proves the world unused".into());
    };
    let present: Vec<u32> = pids
        .into_iter()
        .filter(|pid| !matches!(crate::inference::engine_residency::probe_process(*pid), crate::inference::engine_residency::ProcessProbe::Absent))
        .collect();
    (!present.is_empty()).then(|| format!("pids {present:?} are not confirmed gone"))
}

/// An eval run's disposable world together with every fork's hands that may act in it. Once
/// hands are held the world is RETAINED by default: it is deleted only after each set of hands
/// is confirmed closed. [`Self::finish`] does that in-band; a cancel or an early return drops
/// the world first, and its drop hands the same close to an owned task that outlives the
/// caller (Astra's re-review of #5005). With no hands held, nothing can act in the world, and
/// it goes as an [`EphemeralEvalRoot`] always did.
pub(crate) struct EvalWorld {
    root: Option<EphemeralEvalRoot>,
    /// Hands not yet confirmed closed. One leaves only after its close returned.
    hands: Vec<crate::cognition::persona_workspace::ActingHands>,
    /// Pids not confirmed gone so far, or `None` once a release couldn't report them.
    unresolved: Option<Vec<u32>>,
    any_unresolved: bool,
    /// This world is already the owned cleanup: if it is dropped too (its runtime shutting
    /// down), it keeps the world rather than spawning again.
    owned_cleanup: bool,
}

impl EvalWorld {
    pub(crate) fn new(root: Option<EphemeralEvalRoot>) -> Self {
        Self { root, hands: Vec::new(), unresolved: Some(Vec::new()), any_unresolved: false, owned_cleanup: false }
    }

    /// Hold a fork's hands BEFORE they are rooted in this world.
    pub(crate) fn hold(&mut self, hands: Option<crate::cognition::persona_workspace::ActingHands>) {
        self.hands.extend(hands);
    }

    /// Close every set of hands, awaited, then delete the world, or keep it when any close
    /// is unresolved. A close that fails is logged loudly; it never replaces the measurement.
    pub(crate) async fn finish(mut self) {
        while let Some(hands) = self.hands.last() {
            let outcome = crate::cognition::persona_workspace::close_acting_context(hands).await;
            // Popped only AFTER the close returned: a cancel mid-close leaves it held, for drop.
            self.hands.pop();
            if let Err(not_closed) = outcome {
                tracing::error!(error = %not_closed, "cognition/eval could not cleanly end a fork's own hands; its world is kept");
                self.any_unresolved = true;
                self.unresolved = match (self.unresolved.take(), not_closed.unconfirmed) {
                    (Some(mut known), Some(pids)) => {
                        known.extend(pids);
                        Some(known)
                    }
                    _ => None,
                };
            }
        }
        if self.any_unresolved {
            if let Some(root) = self.root.take() {
                root.retain_unresolved(self.unresolved.take());
            }
        }
        // Otherwise `self` drops with no hands held: the root deletes itself.
    }
}

impl Drop for EvalWorld {
    fn drop(&mut self) {
        if self.hands.is_empty() {
            if self.any_unresolved {
                if let Some(root) = self.root.take() {
                    root.retain_unresolved(self.unresolved.take());
                }
            }
            return;
        }
        let rest = EvalWorld {
            root: self.root.take(),
            hands: std::mem::take(&mut self.hands),
            unresolved: self.unresolved.take(),
            any_unresolved: self.any_unresolved,
            owned_cleanup: true,
        };
        match tokio::runtime::Handle::try_current() {
            Ok(rt) if !self.owned_cleanup => {
                rt.spawn(rest.finish());
            }
            _ => {
                // No runtime to close on, or the owned cleanup itself was dropped: keep the
                // world, and say nothing proves it unused. Dropping `rest` with no hands held
                // and an unresolved verdict retains its root.
                let mut rest = rest;
                rest.hands.clear();
                rest.any_unresolved = true;
                rest.unresolved = None;
            }
        }
    }
}

impl Drop for EphemeralEvalRoot {
    fn drop(&mut self) {
        if self.retained {
            return;
        }
        live_eval_roots().lock().remove(&self.path);
        if let Err(e) = std::fs::remove_dir_all(&self.path) {
            tracing::warn!(
                root = %self.path.display(),
                error = %e,
                "ephemeral eval workspace cleanup failed — temp-dir debris only, shared checkout unaffected"
            );
        }
    }
}
