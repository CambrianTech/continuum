//! Event-driven registry of staged SWE workspaces' candidate diffs.
//!
//! # Why this exists (card f860e59c)
//!
//! The bench board's "ungraded workspace artifact" rows ([`scan_workspace_artifact_cards`])
//! used to call `workspace_candidate_diff` per staged tree on EVERY board tick — two git
//! spawns per tree every 5 s, ≥4.4 git processes a second measured on an idle IntelMac,
//! recomputing answers nothing had changed. The law: never poll to detect. A tree's diff
//! changes only because something wrote that tree, and every write site now announces it
//! ([`crate::code::workspace_events`]). This module is the ONE owner of the answer: it
//! seeds once at boot, then recomputes exactly the trees a `workspace:written` event
//! names. The board scan only READS it — no spawn from a clock.
//!
//! Concurrency (docs/architecture/CONCURRENCY-STYLE-GUIDE.md): one owner task; git work
//! on `spawn_blocking`; the registry lock is never held across `.await` or a spawn, and
//! is taken only for the brief swap-in after the blocking work finished. The owner keeps
//! draining the bus WHILE a recompute runs (a `select!` over the receiver and the
//! in-flight join), so a slow recompute does not itself cause the lag it would then
//! have to answer with a full rescan.
//!
//! [`scan_workspace_artifact_cards`]: crate::commands::benchmark

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, LazyLock};

use parking_lot::RwLock;
use tokio::sync::broadcast::error::{RecvError, TryRecvError};

use crate::code::workspace_events::WORKSPACE_WRITTEN_TOPIC;
use crate::runtime::message_bus::{BusEvent, MessageBus};

/// Instance dir (`<peers>/<peer>/workspace/swe/<instance>`) → its candidate diff.
/// `None` = the diff could not be read (not a repo, git failed) — the same "skip it"
/// answer the old inline scan gave. An instance absent from the map is not tracked yet.
/// The diff is shared (`Arc<str>`) so a board read never copies patch text.
type Registry = HashMap<PathBuf, Option<Arc<str>>>;

static REGISTRY: LazyLock<RwLock<Registry>> = LazyLock::new(|| RwLock::new(HashMap::new()));

/// Read one instance's candidate diff from the registry. Outer `None` = not tracked yet
/// (the watcher has not computed it — it arrives with the seed or the tree's next write);
/// `Some(None)` = tracked but unreadable.
pub(crate) fn candidate_diff(ws: &Path) -> Option<Option<Arc<str>>> {
    REGISTRY.read().get(ws).cloned()
}

/// `<continuum home>/citizens/peers` — the same root the board scan walks, so the
/// registry's keys are byte-identical to the paths the scan looks up.
fn peers_root() -> Option<PathBuf> {
    crate::commands::benchmark::continuum_home()
        .ok()
        .map(|home| home.join("citizens").join("peers"))
}

/// The instance dirs a write to `path` may have changed. Pure path arithmetic, plus a
/// directory listing when the write named something ABOVE the instances:
/// - inside `<peers>/<peer>/workspace/swe/<instance>/…` → that one instance;
/// - the peer dir, its `workspace`, or `workspace/swe` (a shell cwd, a git verb on the
///   layer root) → ALL of that peer's instances — the command could have touched any;
/// - the peers root itself → every peer's instances;
/// - anything else (outside the peers root, or elsewhere in a peer dir) → nothing.
///
/// Keys are built under `peers_root` as given, so they match the board scan's paths
/// even when the writer reported a canonicalized path (macOS `/var` → `/private/var`).
pub(crate) fn instance_dirs_for_written_path(path: &Path, peers_root: &Path) -> Vec<PathBuf> {
    let rel = match path.strip_prefix(peers_root) {
        Ok(rel) => rel.to_path_buf(),
        Err(_) => {
            let canonical = peers_root.canonicalize().ok();
            match canonical.as_deref().and_then(|c| path.strip_prefix(c).ok()) {
                Some(rel) => rel.to_path_buf(),
                None => return Vec::new(),
            }
        }
    };
    let mut parts = Vec::new();
    for component in rel.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_os_string()),
            // `..` / `.` / a root inside a relative remainder: not a path we can place.
            _ => return Vec::new(),
        }
    }
    let Some(peer) = parts.first() else {
        return all_instance_dirs(peers_root);
    };
    if parts.get(1).is_some_and(|p| p.as_os_str() != "workspace")
        || parts.get(2).is_some_and(|p| p.as_os_str() != "swe")
    {
        return Vec::new();
    }
    let swe = peers_root.join(peer).join("workspace").join("swe");
    match parts.get(3) {
        Some(instance) => vec![swe.join(instance)],
        None => instance_dirs_in(&swe),
    }
}

/// The instance directories directly under one peer's `workspace/swe`, sorted.
fn instance_dirs_in(swe: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(swe) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    dirs
}

/// Every peer's instance directories — the seed, and the honest answer after a lag.
fn all_instance_dirs(peers_root: &Path) -> Vec<PathBuf> {
    let Ok(peers) = std::fs::read_dir(peers_root) else {
        return Vec::new();
    };
    let mut dirs = Vec::new();
    for peer in peers.flatten() {
        dirs.extend(instance_dirs_in(&peer.path().join("workspace").join("swe")));
    }
    dirs.sort();
    dirs
}

/// What the next recompute covers.
enum Batch {
    /// Every instance on disk; trees no longer on disk are dropped from the registry.
    All,
    /// The instance dirs these written paths map to.
    Paths(Vec<PathBuf>),
}

/// Recompute a batch. BLOCKING (git spawns + fs reads): call on `spawn_blocking` only.
/// Diffs are computed with no lock held; the registry write lock is taken once, for the
/// swap-in.
fn recompute(peers_root: &Path, batch: Batch, reason: &'static str) {
    let started = std::time::Instant::now();
    let (dirs, full) = match batch {
        Batch::All => (all_instance_dirs(peers_root), true),
        Batch::Paths(paths) => {
            let mut set = BTreeSet::new();
            for path in &paths {
                set.extend(instance_dirs_for_written_path(path, peers_root));
            }
            (set.into_iter().collect::<Vec<_>>(), false)
        }
    };
    // Outer None = the dir is gone (removed / evicted): drop it from the registry.
    let mut results: Vec<(PathBuf, Option<Option<Arc<str>>>)> = Vec::with_capacity(dirs.len());
    for dir in &dirs {
        if !dir.is_dir() {
            results.push((dir.clone(), None));
            continue;
        }
        // The SAME reading of "her work" the grader uses — never a second inline diff.
        let diff = dir
            .to_str()
            .and_then(|ws| crate::commands::benchmark::workspace_candidate_diff(ws).ok())
            .map(Arc::<str>::from);
        results.push((dir.clone(), Some(diff)));
    }
    {
        let mut registry = REGISTRY.write();
        if full {
            let on_disk: HashSet<&PathBuf> = dirs.iter().collect();
            registry.retain(|dir, _| on_disk.contains(dir));
        }
        for (dir, entry) in results {
            match entry {
                Some(diff) => {
                    registry.insert(dir, diff);
                }
                None => {
                    registry.remove(&dir);
                }
            }
        }
    }
    crate::probe!(
        class = "bench.workspace_artifacts.recomputed",
        reason = reason,
        instances = dirs.len(),
        elapsed_ms = started.elapsed().as_millis() as u64,
        "workspace-artifact diffs recomputed — only the trees a write named (or all, on seed/lag)"
    );
}

/// The written path a `workspace:written` event carries; `None` for any other event.
fn written_path(event: &BusEvent) -> Option<PathBuf> {
    if event.name != WORKSPACE_WRITTEN_TOPIC {
        return None;
    }
    event
        .payload
        .get("path")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
}

/// Await the in-flight recompute, or never resolve when there is none (so the `select!`
/// arm is inert without an `unwrap`).
async fn join_inflight(inflight: &mut Option<tokio::task::JoinHandle<()>>) {
    match inflight.as_mut() {
        Some(handle) => {
            if let Err(e) = handle.await {
                tracing::warn!(error = %e, "workspace-artifact recompute task failed — its trees are recomputed on their next write");
            }
        }
        None => std::future::pending::<()>().await,
    }
}

/// Spawn the ONE owner of the workspace-artifact registry: seed once, then recompute
/// only what `workspace:written` events name. Subscribes BEFORE seeding so a write that
/// lands during the seed is still seen.
pub(crate) fn spawn_workspace_artifact_watcher(rt: &tokio::runtime::Handle, bus: Arc<MessageBus>) {
    let mut rx = bus.receiver();
    rt.spawn(async move {
        let Some(peers_root) = peers_root() else {
            tracing::warn!(
                "workspace-artifact watcher: no continuum home — the bench board shows no \
                 ungraded workspace rows this boot"
            );
            return;
        };
        // Pending work: start with the one-time seed.
        let mut pending_all = true;
        let mut pending_reason: &'static str = "seed";
        let mut pending_paths: Vec<PathBuf> = Vec::new();
        let mut inflight: Option<tokio::task::JoinHandle<()>> = None;
        loop {
            if inflight.is_none() && (pending_all || !pending_paths.is_empty()) {
                let batch = if pending_all {
                    pending_paths.clear(); // subsumed by the full pass
                    Batch::All
                } else {
                    Batch::Paths(std::mem::take(&mut pending_paths))
                };
                let reason = pending_reason;
                pending_all = false;
                pending_reason = "event";
                let root = peers_root.clone();
                inflight = Some(tokio::task::spawn_blocking(move || {
                    recompute(&root, batch, reason)
                }));
            }
            let mut finished = false;
            let mut closed = false;
            tokio::select! {
                received = rx.recv() => match received {
                    Ok(event) => {
                        if let Some(path) = written_path(&event) {
                            pending_paths.push(path);
                        }
                    }
                    // Events were dropped: we cannot know which trees they named, so
                    // every tree is dirty — the honest answer, never a guess.
                    Err(RecvError::Lagged(_)) => {
                        pending_all = true;
                        pending_reason = "lagged";
                    }
                    Err(RecvError::Closed) => closed = true,
                },
                _ = join_inflight(&mut inflight) => finished = true,
            }
            if closed {
                return; // the bus is gone — the core is shutting down
            }
            if finished {
                inflight = None;
            }
            // Coalesce: fold everything already queued into this round's dirty set.
            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        if let Some(path) = written_path(&event) {
                            pending_paths.push(path);
                        }
                    }
                    Err(TryRecvError::Lagged(_)) => {
                        pending_all = true;
                        pending_reason = "lagged";
                    }
                    Err(TryRecvError::Empty) | Err(TryRecvError::Closed) => break,
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a write INSIDE one staged tree recomputes that tree only — not
    // its sibling (which would re-spawn git per sibling on every keystroke), and not
    // nothing (which would leave the board row stale).
    // regression for card f860e59c
    #[test]
    fn a_file_inside_an_instance_maps_to_that_instance_only() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let peers = tmp.path().join("peers");
        let swe = peers.join("peer-a").join("workspace").join("swe");
        std::fs::create_dir_all(swe.join("inst-1").join("src")).expect("mkdir");
        std::fs::create_dir_all(swe.join("inst-2")).expect("mkdir");
        let written = swe.join("inst-1").join("src").join("a.py");
        assert_eq!(
            instance_dirs_for_written_path(&written, &peers),
            vec![swe.join("inst-1")]
        );
    }

    // what this catches: a shell command / git verb run at the workspace ROOT may have
    // touched any instance, so it must dirty all of that peer's instance dirs (and only
    // dirs — a stray file under swe/ is not an instance).
    // regression for card f860e59c
    #[test]
    fn the_workspace_root_maps_to_all_of_that_peers_instances() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let peers = tmp.path().join("peers");
        let workspace = peers.join("peer-a").join("workspace");
        let swe = workspace.join("swe");
        std::fs::create_dir_all(swe.join("inst-1")).expect("mkdir");
        std::fs::create_dir_all(swe.join("inst-2")).expect("mkdir");
        std::fs::write(swe.join("notes.txt"), "x").expect("write");
        std::fs::create_dir_all(peers.join("peer-b").join("workspace").join("swe").join("other"))
            .expect("mkdir");
        assert_eq!(
            instance_dirs_for_written_path(&workspace, &peers),
            vec![swe.join("inst-1"), swe.join("inst-2")]
        );
    }

    // what this catches: writes that cannot change any staged tree (outside the peers
    // root, or elsewhere in a peer's dir) must not trigger a recompute at all.
    // regression for card f860e59c
    #[test]
    fn a_path_outside_the_instances_maps_to_nothing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let peers = tmp.path().join("peers");
        std::fs::create_dir_all(peers.join("peer-a").join("workspace").join("swe").join("inst-1"))
            .expect("mkdir");
        assert!(instance_dirs_for_written_path(&tmp.path().join("elsewhere/a.py"), &peers).is_empty());
        assert!(
            instance_dirs_for_written_path(&peers.join("peer-a").join("memory").join("x"), &peers)
                .is_empty()
        );
    }
}
