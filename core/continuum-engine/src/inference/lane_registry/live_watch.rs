//! Verified live-lane publication and its owned process-exit watch.
use super::{lane_pidfile, lane_process, lanes_dir, live_incarnation, record_path, writes, LaneRecord, LaneRole};
use std::path::Path;

/// The live lane THIS PROCESS has verified, as a watch: a waiter (page IO's retained custody, waiting for an
/// adopted engine's identity) awaits a change instead of polling. Single-process by design. Its value is always
/// the registry's VERIFIED truth, recomputed under the registry's write owner ([`writes`]) whenever it may have
/// changed: a live record written ([`record`]), a record removed ([`remove`]), or an engine adopted
/// ([`sync_live`], from the reconcile's `AlreadyServing`). Verified means the canonical pidfile and the live
/// record agree AND the pid is a running llama-server whose OS start time equals the record's: a recycled pid
/// running another llama-server is never published. No generation counter is needed: every write recomputes
/// the one truth under the lock, so a stale read cannot republish over a newer state.
///
/// A published engine that EXITS is revoked by the process-completion owner ([`continuum_host::process_exit`]):
/// publishing first ARMS one kernel exit watch bound to that exact incarnation (binding, then start-time check),
/// and its exit re-verifies under [`writes`].
/// So a dead original is never kept published while its pidfile and record stay unchanged (Codex on #6099).
pub(super) static LIVE_LANE: std::sync::LazyLock<std::sync::Arc<LiveLane>> = std::sync::LazyLock::new(LiveLane::new);

type LiveIncarnation = Option<crate::inference::engine_residency::EngineIncarnation>;

/// One registry's verified live lane, as a watch, and the exit watch of the published engine.
pub(crate) struct LiveLane {
    published: tokio::sync::watch::Sender<LiveIncarnation>,
    exit_watch: parking_lot::Mutex<Option<ExitWatch>>,
    generations: std::sync::atomic::AtomicU64,
    watcher: Box<dyn EngineExitWatcher>,
}

/// How a lane awaits its engine's exit. In production the process-completion owner
/// ([`continuum_host::process_exit`]); a test injects its own to drive the failure paths.
pub(crate) trait EngineExitWatcher: Send + Sync {
    /// Bind the exit watch of `pid`, then prove it with `is_expected` ([`continuum_host::process_exit::arm`]); the
    /// future resolves on that process's exit.
    fn arm(
        &self,
        pid: u32,
        is_expected: &dyn Fn() -> bool,
    ) -> Result<std::pin::Pin<Box<dyn std::future::Future<Output = continuum_host::process_exit::ExitObserved> + Send>>, continuum_host::process_exit::ArmRefusal>;
}

/// The kernel's exit event, through the one process-completion owner.
struct KernelExitWatcher;

impl EngineExitWatcher for KernelExitWatcher {
    fn arm(
        &self,
        pid: u32,
        is_expected: &dyn Fn() -> bool,
    ) -> Result<std::pin::Pin<Box<dyn std::future::Future<Output = continuum_host::process_exit::ExitObserved> + Send>>, continuum_host::process_exit::ArmRefusal> {
        let armed = continuum_host::process_exit::arm(pid, is_expected)?;
        Ok(Box::pin(armed.exited()))
    }
}

/// The exit watch of the published engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExitWatch {
    /// Armed and awaited: the engine is trusted without a probe. `generation` names this registration, so an
    /// older watch's guard can never clear a newer one.
    Armed { engine: crate::inference::engine_residency::EngineIncarnation, generation: u64 },
    /// Its exit cannot be awaited (the kernel refused, or the wait ended without an answer): published but never
    /// trusted, so every sync verifies it against the OS, and never armed again for this incarnation (a
    /// persistent failure is one attempt, not a spin of threads; Codex on #6099 b4c75bc57).
    Unobservable { engine: crate::inference::engine_residency::EngineIncarnation },
}

/// Owned by the waiting task from before it is spawned: however the task ends (exit seen, abort, runtime
/// shutdown before its first poll), the armed watch it stood for stops being trusted. It clears only its own
/// generation.
struct ExitWatchGuard {
    lane: std::sync::Arc<LiveLane>,
    generation: u64,
}

impl ExitWatchGuard {
    /// The armed watch could not be awaited: keep the engine published, untrusted, and never re-armed.
    fn unobservable(self, engine: crate::inference::engine_residency::EngineIncarnation) {
        let mut watch = self.lane.exit_watch.lock();
        if matches!(*watch, Some(ExitWatch::Armed { generation, .. }) if generation == self.generation) {
            *watch = Some(ExitWatch::Unobservable { engine });
        }
    }
}

impl Drop for ExitWatchGuard {
    fn drop(&mut self) {
        let mut watch = self.lane.exit_watch.lock();
        if matches!(*watch, Some(ExitWatch::Armed { generation, .. }) if generation == self.generation) {
            *watch = None;
        }
    }
}

impl LiveLane {
    pub(crate) fn new() -> std::sync::Arc<Self> {
        Self::with_watcher(Box::new(KernelExitWatcher))
    }

    pub(crate) fn with_watcher(watcher: Box<dyn EngineExitWatcher>) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            published: tokio::sync::watch::channel(None).0,
            exit_watch: parking_lot::Mutex::new(None),
            generations: std::sync::atomic::AtomicU64::new(0),
            watcher,
        })
    }

    pub(crate) fn subscribe(&self) -> tokio::sync::watch::Receiver<LiveIncarnation> {
        self.published.subscribe()
    }

    /// Whether the published value stands for `on_disk` without a process probe: they agree, and a published
    /// engine has an ARMED exit watch (so its death would already have revoked it).
    fn trusts(&self, on_disk: LiveIncarnation) -> bool {
        let published = *self.published.borrow();
        on_disk == published
            && published.is_none_or(|engine| matches!(*self.exit_watch.lock(), Some(ExitWatch::Armed { engine: armed, .. }) if armed == engine))
    }

    /// What to publish for `engine`, just verified from `dir` + `pidfile`: arm its exit watch FIRST (the kernel
    /// binding, then the start-time check against it). Armed, already armed, or already known unobservable: the
    /// engine (the last untrusted, never re-armed). Refused (exited, or the pid now names another process):
    /// nothing, never the earlier verification. Unobservable now, or no runtime to await on: the engine, verified
    /// now but untrusted, so every sync verifies against the OS.
    fn publishable(self: &std::sync::Arc<Self>, engine: crate::inference::engine_residency::EngineIncarnation, dir: &Path, pidfile: &Path) -> LiveIncarnation {
        use continuum_host::process_exit::{ArmRefusal, ExitObserved};
        if matches!(*self.exit_watch.lock(), Some(ExitWatch::Armed { engine: e, .. } | ExitWatch::Unobservable { engine: e }) if e == engine) {
            return Some(engine);
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return Some(engine);
        };
        let is_engine = || crate::inference::engine_residency::process_start_s(engine.pid) == Some(engine.started_s);
        let exited = match self.watcher.arm(engine.pid, &is_engine) {
            Ok(exited) => exited,
            Err(ArmRefusal::AlreadyExited | ArmRefusal::NotThatProcess) => return None,
            Err(ArmRefusal::Unobservable(why)) => {
                *self.exit_watch.lock() = Some(ExitWatch::Unobservable { engine });
                continuum_base::probe!(
                    class = "lane.live.exit_unobservable",
                    pid = engine.pid,
                    why = %why,
                    "the live engine's exit cannot be awaited: published untrusted, verified against the OS on every sync"
                );
                return Some(engine);
            }
        };
        let generation = self.generations.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        *self.exit_watch.lock() = Some(ExitWatch::Armed { engine, generation });
        let guard = ExitWatchGuard { lane: std::sync::Arc::clone(self), generation };
        let (dir, pidfile) = (dir.to_path_buf(), pidfile.to_path_buf());
        runtime.spawn(async move {
            let guard = guard;
            match exited.await {
                ExitObserved::Exited => {
                    let lane = std::sync::Arc::clone(&guard.lane);
                    drop(guard); // the watch is spent: untrusted before the re-verify below
                    let _writes = writes();
                    sync_live_locked(&dir, &pidfile, &lane);
                }
                ExitObserved::Unobservable(why) => {
                    continuum_base::probe!(
                        class = "lane.live.exit_unobservable",
                        pid = engine.pid,
                        why = %why,
                        "the live engine's exit watch ended without an answer: published untrusted, never re-armed"
                    );
                    guard.unobservable(engine);
                }
            }
        });
        Some(engine)
    }
}

/// Subscribe to the verified live lane of this process ([`LIVE_LANE`]). The first subscription brings the watch
/// to the truth on disk (an adopting core may never record).
pub fn subscribe_live() -> tokio::sync::watch::Receiver<LiveIncarnation> {
    sync_live();
    LIVE_LANE.subscribe()
}

/// Bring the watch to the verified truth. Cheap when nothing changed: the file-only incarnation
/// ([`live_incarnation`], two small reads) agrees with the published one AND that engine's exit is awaited, so
/// the reconcile may call it on every `AlreadyServing` without a process probe per tick.
pub fn sync_live() {
    if LIVE_LANE.trusts(live_incarnation()) {
        return;
    }
    let (Some(dir), Some(pidfile)) = (lanes_dir(), lane_pidfile::pidfile_path()) else {
        return;
    };
    let _writes = writes();
    sync_live_locked(&dir, &pidfile, &LIVE_LANE);
}

/// Publish the verified truth of `dir` + `pidfile` on `live`; a live engine is published only once its exit
/// watch is armed (or found unobservable). The caller holds [`writes`].
pub(crate) fn sync_live_locked(dir: &Path, pidfile: &Path, live: &std::sync::Arc<LiveLane>) {
    let truth = verified_live_in(dir, pidfile).and_then(|engine| live.publishable(engine, dir, pidfile));
    live.published.send_if_modified(|current| {
        let changed = *current != truth;
        if changed {
            *current = truth;
        }
        changed
    });
}

/// The live lane of `dir` + `pidfile`, verified against the OS: pidfile and live record agree, the record names
/// a start time, the pid is a running llama-server, and its OS start time is the record's.
fn verified_live_in(dir: &Path, pidfile: &Path) -> Option<crate::inference::engine_residency::EngineIncarnation> {
    let pid = lane_pidfile::read_at(pidfile)?;
    let body = std::fs::read_to_string(record_path(dir, pid)).ok()?;
    let rec = serde_json::from_str::<LaneRecord>(&body).ok()?;
    let live = rec.role == LaneRole::Live && rec.pid == pid && rec.started_s != 0;
    (live
        && lane_process::is_llama_server(pid)
        && crate::inference::engine_residency::process_start_s(pid) == Some(rec.started_s))
    .then(|| rec.incarnation())
}


#[cfg(test)]
mod tests;
