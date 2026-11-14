//! The engine's host prompt cache gives host RAM back FIRST (card 0f631c46, "one memory control").
//!
//! The M5 (64 GB, Qwen3.6-35B-A3B, four residents) on 2026-11-13 23:40Z, canary 0ed0d231b: 0.4 GB
//! free, 42 GB in the compressor, swap 14.5 of 15 GB, while the engine held a host prompt cache
//! launched at `--cache-ram 16525` MiB. The lever that could lower it (#5969) hung off
//! `ResourceConsumer::reclaim`, which the resource authority calls only when a resource KIND is
//! over its own ceiling: `serving.reclaim.prompt_cache` fired zero times. The host's memory
//! deficit drives a different path, the memory monitor's shedders (`MemoryReporter::shed_load`).
//! So the cache answers there, and only there: ONE reclaim path for host RAM.
//!
//! The cache is the cheapest memory on the box: a dropped state costs one re-prefill of an old
//! prompt, while every other holder (mind results, rendering, the model itself) costs a mind
//! something it is using. It reports at [`MemoryPriority::Batch`], the priority that yields first:
//! the monitor charges the deficit to it before any higher priority (`deficit_at`), and it lowers
//! its limit by its share, planned from what the cache holds ([`lever`]). When the OS has read
//! itself normal on two polls in a row the monitor calls [`MemoryReporter::take_again`] and the
//! cache goes back to its launch grant, the engine's `/prompt-cache/limit` reply the receipt.
//!
//! Only the OS's own deficit lowers the cache, never the monitor's fixed RAM gates alone: those
//! read pressure on a host the OS calls normal, and would lower what the take-again half raises.
//!
//! The lever is async (an HTTP call to the engine) and the monitor's calls are synchronous, so each
//! call starts at most one step on the runtime; a poll that finds one in flight leaves it, and the
//! next poll recomputes the share from what is still held and still owed.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::inference::llama_server::ServingSnapshot;
use crate::system_resources::{
    MemoryBudgetSpec, MemoryPressureMonitor, MemoryPriority, MemoryReporter, ModuleMemoryReport,
    PressureLevel, PressureSnapshot,
};

mod lever;
pub use lever::PromptCacheControl;
use lever::PromptCacheLever;

/// The priority the cache sheds at: the one that yields first.
const PRIORITY: MemoryPriority = MemoryPriority::Batch;
const NAME: &str = "serving.prompt_cache";

/// The engine's host prompt cache as a memory reporter: lowered first under the host's deficit,
/// taken back when the OS reads normal.
pub struct PromptCacheShedder {
    inner: Arc<Inner>,
}

struct Inner {
    lever: PromptCacheLever,
    serving: watch::Receiver<ServingSnapshot>,
    pressure: watch::Receiver<PressureSnapshot>,
    runtime: tokio::runtime::Handle,
    /// One step on the engine at a time.
    in_flight: AtomicBool,
    /// A step that could not run (at the floor, unknown, refused) is said once until one runs.
    stuck_said: AtomicBool,
}

/// Holds `in_flight` for one step; dropping it (the step finished, or its task was torn down)
/// frees the next.
struct InFlight(Arc<Inner>);

impl InFlight {
    fn begin(inner: &Arc<Inner>) -> Option<Self> {
        (!inner.in_flight.swap(true, Ordering::AcqRel)).then(|| Self(Arc::clone(inner)))
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        self.0.in_flight.store(false, Ordering::Release);
    }
}

impl PromptCacheShedder {
    /// The cache behind `control`, its launch grant read off `serving`, its share off `pressure`;
    /// each step runs on `runtime`.
    pub fn new(
        control: Arc<dyn PromptCacheControl>,
        serving: watch::Receiver<ServingSnapshot>,
        pressure: watch::Receiver<PressureSnapshot>,
        runtime: tokio::runtime::Handle,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                lever: PromptCacheLever::new(control),
                serving,
                pressure,
                runtime,
                in_flight: AtomicBool::new(false),
                stuck_said: AtomicBool::new(false),
            }),
        }
    }

    /// Answer to `monitor` from now on: it reads the cache's bytes, asks it first for the host's
    /// deficit and lets it take the memory back when the OS reads normal.
    pub fn register(control: Arc<dyn PromptCacheControl>, serving: watch::Receiver<ServingSnapshot>, monitor: &MemoryPressureMonitor, runtime: tokio::runtime::Handle) {
        let shedder = Self::new(control, serving, monitor.subscribe(), runtime);
        monitor.add_reporter(Arc::new(shedder));
    }

    /// Start lowering the cache by its share of the host's deficit, if it owes one and no step is
    /// in flight. The handle lets a test wait for the step; the monitor drops it.
    pub(crate) fn shed_now(&self) -> Option<JoinHandle<()>> {
        let launched_mib = self.inner.launched_mib()?;
        let held = self.inner.held_bytes(launched_mib);
        let share = {
            let snapshot = self.inner.pressure.borrow();
            if snapshot.deficit_bytes == 0 {
                return None;
            }
            snapshot.share_of_deficit(PRIORITY, held)
        };
        if share == 0 {
            return None;
        }
        let guard = InFlight::begin(&self.inner)?;
        Some(self.inner.runtime.spawn(async move {
            guard.0.lower(launched_mib, share).await;
        }))
    }

    /// Start taking the cache back to its launch grant, if serving lowered it and no step is in
    /// flight. The handle lets a test wait for the step; the monitor drops it.
    pub(crate) fn take_again_now(&self) -> Option<JoinHandle<()>> {
        let launched_mib = self.inner.launched_mib()?;
        let lowered = self.inner.lever.last().is_some_and(|last| last.limit_mib.get() < u64::from(launched_mib));
        if !lowered {
            return None;
        }
        let guard = InFlight::begin(&self.inner)?;
        Some(self.inner.runtime.spawn(async move {
            guard.0.take_back(launched_mib).await;
        }))
    }
}

impl Inner {
    /// The serving engine's launch grant in MiB (0 = unknown); `None` while no engine is ready.
    fn launched_mib(&self) -> Option<u32> {
        let snapshot = self.serving.borrow();
        snapshot.ready.then_some(snapshot.host_prompt_cache_mib)
    }

    /// What the cache holds as far as serving knows: what the engine last said, or, before serving
    /// first asked, its launch grant (the most it can hold).
    fn held_bytes(&self, launched_mib: u32) -> u64 {
        match self.lever.last() {
            Some(last) => last.held_bytes,
            None => u64::from(launched_mib) << 20,
        }
    }

    async fn lower(&self, launched_mib: u32, share: u64) {
        let step = match self.lever.plan(launched_mib, share) {
            Ok(step) => step,
            Err(why) => {
                if !self.stuck_said.swap(true, Ordering::AcqRel) {
                    continuum_base::probe!(
                        class = "serving.reclaim.prompt_cache",
                        target_bytes = share,
                        no_step = ?why,
                        "the prompt cache cannot be lowered further: the higher priorities give back the rest of the host's deficit"
                    );
                }
                return;
            }
        };
        match self.lever.apply(step, share).await {
            Ok(applied) => {
                self.stuck_said.store(false, Ordering::Release);
                continuum_base::probe!(
                    class = "serving.reclaim.prompt_cache",
                    limit_before_mib = step.before_mib.get(),
                    limit_after_mib = applied.limit_after_mib.get(),
                    calls = applied.calls,
                    target_bytes = share,
                    freed_bytes = applied.freed_bytes,
                    held_bytes = applied.held_bytes,
                    "serving lowered its host prompt cache by its share of the host's memory deficit, first of every holder"
                );
            }
            Err(error) => {
                if !self.stuck_said.swap(true, Ordering::AcqRel) {
                    continuum_base::probe!(
                        class = "serving.reclaim.prompt_cache",
                        limit_before_mib = step.before_mib.get(),
                        limit_after_mib = step.after_mib.get(),
                        target_bytes = share,
                        refusal = %error,
                        "the engine did not lower its prompt cache: the higher priorities give back the host's deficit"
                    );
                }
            }
        }
    }

    async fn take_back(&self, launched_mib: u32) {
        match self.lever.take_back(launched_mib).await {
            Ok(Some(raised)) => {
                self.stuck_said.store(false, Ordering::Release);
                continuum_base::probe!(
                    class = "serving.prompt_cache.take_again",
                    limit_before_mib = raised.before_mib.get(),
                    limit_after_mib = raised.after_mib.get(),
                    held_bytes = raised.held_bytes,
                    "the OS reads normal: serving took its host prompt cache back to its launch grant"
                );
            }
            Ok(None) => {}
            Err(error) => continuum_base::probe!(
                class = "serving.prompt_cache.take_again",
                limit_after_mib = launched_mib,
                refusal = %error,
                "the engine did not take its prompt cache back: the next calm poll asks again"
            ),
        }
    }
}

impl MemoryReporter for PromptCacheShedder {
    fn name(&self) -> &'static str {
        NAME
    }

    fn budget(&self) -> MemoryBudgetSpec {
        let held = self.inner.launched_mib().map_or(0, |launched_mib| self.inner.held_bytes(launched_mib));
        MemoryBudgetSpec { name: NAME.to_string(), priority: PRIORITY, min_bytes: 0, preferred_bytes: held, max_bytes: held }
    }

    fn report(&self) -> ModuleMemoryReport {
        let (bytes, detail) = match (self.inner.launched_mib(), self.inner.lever.last()) {
            (None, _) => (0, "no engine serving".to_string()),
            (Some(launched_mib), Some(last)) => (
                last.held_bytes,
                format!("held as the engine last said, limit {} of launch grant {launched_mib} MiB", last.limit_mib),
            ),
            (Some(launched_mib), None) => (
                u64::from(launched_mib) << 20,
                format!("launch grant {launched_mib} MiB, contents not yet asked"),
            ),
        };
        ModuleMemoryReport { name: NAME.to_string(), bytes, detail, can_shed: true }
    }

    fn priority(&self) -> MemoryPriority {
        PRIORITY
    }

    fn can_shed(&self) -> bool {
        true
    }

    /// Lower the cache by its share of the host's deficit.
    fn shed_load(&self, _level: PressureLevel) {
        drop(self.shed_now());
    }

    /// Take the cache back to its launch grant.
    fn take_again(&self) {
        drop(self.take_again_now());
    }
}

#[cfg(test)]
pub(crate) mod tests;
