//! Eviction owner for `wt-targets`: the directory of per-worktree cargo target dirs.
//!
//! The M5, 2026-10-29 23:13Z: `~/.continuum/cache/wt-targets` held 448 GB (apparent) in one
//! target dir per git worktree, a convention adopted on 2026-10-21 because one shared target
//! ran a stale binary across worktrees. No row tracked it and no pool owned it, so the volume
//! reached ENOSPC with the broker seeing nothing to reclaim; a test window and the residents'
//! core went down with it.
//!
//! Each child is a whole, derived cargo target, so the unit of eviction is the child: least
//! recently built first, never a held one. A child is HELD when any of these says so, each the
//! same check the shared cargo-target owner already makes:
//! 1. cargo's own build locks (`.cargo-lock` at the root and in every profile dir, including
//!    a target-triple's profile dirs) cannot all be taken exclusively;
//! 2. a process is executing an image under it (a test binary, a core started from it);
//! 3. the disk registry (the one root authority over cargo targets) will not grant this
//!    eviction its claim: a writer admitted through `admit_cargo_target` holds a lease
//!    overlapping it. Once granted, the claim refuses every overlapping writer admission
//!    until the target is detached, so admission and eviction cannot interleave;
//! 4. serving has registered an artifact under it.
//!
//! Removal first DETACHES the child (one rename to `.evicting-<name>` under the same root)
//! while the claim and the locks are held (the lock scan runs after the claim, so a profile
//! created since the measurement is locked too), so a cargo that starts afterwards builds a fresh tree at the old
//! name instead of into one being deleted. The detached tree is then removed with the same
//! redirect-safe walk the shared owner uses. A detached tree a crash left behind is reclaimed
//! first on the next eviction: nothing builds into a detached name.
//!
//! NO FILESYSTEM WORK ON THE TICK (CONCURRENCY-STYLE-GUIDE). The first measurement of the
//! class on the M5 walks ~448 GB, and the broker's relief pass runs every pool in turn, so
//! a walk inside `evict_at_least` would stall relief for every other pool. The tick reads
//! the last published [`targets::Snapshot`], plans from it in memory, and hands the work to
//! the blocking pool without awaiting it: one background job at a time, either a
//! measurement (no snapshot yet, or the scanner has observed the class since) or an
//! eviction (which re-measures and republishes when it ends). Bytes a job frees are
//! recorded on the class at once and reported to the broker on the next tick.
//!
//! PRODUCTION OBSERVES, IT DOES NOT DELETE (Codex on #5601). Agents build into these dirs
//! from shells the core never admitted, so the registry's exclusion cannot see them, and
//! naming them out of contract does not protect their work. Until build admission exists
//! (the next slice) the pool runs in [`WorktreeEvictionMode::Observe`]: it measures, plans, and
//! publishes what keep-recent or a hot volume WOULD take, and deletes nothing.

use super::*;

mod targets;
use targets::{Plan, Snapshot, Targets};

/// The prefix a detached child carries until its removal completes.
const DETACHED_PREFIX: &str = ".evicting-";

/// Whether the pool carries its plan out. A typed mode chosen where the pool is built,
/// never a flag or an environment variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeEvictionMode {
    /// Measure, plan, and publish what would be taken (a `disk.wt_targets.would_evict`
    /// probe naming each target and its bytes); delete nothing. Production, until every
    /// build into the class is admitted through the disk registry.
    Observe,
    /// Carry the plan out: detach and remove each target that passes every hold check.
    Delete,
}

pub struct WorktreeTargetsPool {
    targets: Arc<Targets>,
    mode: WorktreeEvictionMode,
    /// The last would-take list published in [`WorktreeEvictionMode::Observe`], so an unchanged
    /// plan is published once rather than once per tick.
    would_take: parking_lot::Mutex<Vec<(PathBuf, u64)>>,
    /// The runtime whose blocking pool runs measurements and evictions.
    runtime: tokio::runtime::Handle,
    /// The last measurement, published by the background job and read by the tick.
    snapshot: Arc<tokio::sync::watch::Sender<Option<Arc<Snapshot>>>>,
    /// A background job is in flight; the tick starts no second one.
    busy: Arc<std::sync::atomic::AtomicBool>,
    /// Bytes finished jobs freed that no tick has reported to the broker yet.
    freed: Arc<std::sync::atomic::AtomicU64>,
}

/// Clears `busy` when a job ends, including by a panic, so one failed job never parks the
/// pool for good.
struct JobSlot(Arc<std::sync::atomic::AtomicBool>);
impl Drop for JobSlot {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::Release);
    }
}

impl WorktreeTargetsPool {
    pub fn new(
        tracked: Arc<TrackedDir>,
        runtime: tokio::runtime::Handle,
        mode: WorktreeEvictionMode,
    ) -> Self {
        Self::from_targets(Targets::new(tracked), runtime, mode)
    }

    fn from_targets(
        targets: Targets,
        runtime: tokio::runtime::Handle,
        mode: WorktreeEvictionMode,
    ) -> Self {
        Self {
            targets: Arc::new(targets),
            mode,
            would_take: parking_lot::Mutex::new(Vec::new()),
            runtime,
            snapshot: Arc::new(tokio::sync::watch::Sender::new(None)),
            busy: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            freed: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Whether a background job is in flight (test seam for waiting one out).
    #[cfg(test)]
    fn busy(&self) -> bool {
        self.busy.load(std::sync::atomic::Ordering::Acquire)
    }

    /// The tick's plan, in memory: keep recent within the budget, and honour the broker's
    /// ask past the trim when the class fit its budget (the volume brought the broker) or
    /// the volume is hot. Keep-recent yields to real volume pressure: then the ask may take
    /// the newest unheld target too. Over budget on a calm volume, the trim alone answers.
    fn plan(&self, snapshot: &Snapshot, want_bytes: u64) -> Plan {
        let sizes: Vec<u64> = snapshot.live.iter().map(|(_, size)| *size).collect();
        let volume = (self.targets.volume_sample)(&snapshot.root);
        let budget = (self.targets.budget_for)(volume.as_ref());
        let within_budget = sizes.iter().sum::<u64>() <= budget;
        Plan {
            beyond: Targets::beyond_budget(&sizes, budget),
            pressure_ask: if within_budget || Self::volume_hot(volume.as_ref()) {
                want_bytes.max(1)
            } else {
                0
            },
        }
    }

    /// What a plan would take, before hold checks (they take locks, which is IO): the
    /// targets outside the budget, then, under a pressure ask, the oldest kept ones until
    /// the ask is covered. Pure, from the snapshot.
    fn planned_targets(snapshot: &Snapshot, plan: Plan) -> Vec<(PathBuf, u64)> {
        let mut taken = Vec::new();
        let mut bytes = 0u64;
        for (index, (candidate, size)) in snapshot.live.iter().enumerate() {
            let asking = plan.pressure_ask > 0 && bytes < plan.pressure_ask;
            if index >= plan.beyond && !asking {
                break;
            }
            bytes = bytes.saturating_add(*size);
            taken.push((candidate.path.clone(), *size));
        }
        taken
    }

    /// Publish the would-take list when it changed: the observation the next slice's
    /// deletion will be judged against.
    fn publish_would_take(&self, taken: Vec<(PathBuf, u64)>) {
        let mut last = self.would_take.lock();
        if *last == taken {
            return;
        }
        let bytes: u64 = taken.iter().map(|(_, size)| *size).sum();
        let targets: Vec<String> = taken
            .iter()
            .map(|(path, size)| format!("{} ({} MB)", path.display(), size / (1024 * 1024)))
            .collect();
        continuum_base::probe!(
            class = "disk.wt_targets.would_evict",
            tier = %self.targets.tier,
            count = taken.len() as u64,
            bytes = bytes,
            targets = ?targets,
            "keep-recent / hot-volume plan for wt-targets, NOT carried out (observe mode until build admission exists); each target would still face every hold check"
        );
        *last = taken;
    }

    /// Is the volume hot by the disk governor's own classification (High or worse)? An
    /// unseen volume reads Unknown, which the governor orders as hot: an unseen disk is not
    /// a disk with room.
    fn volume_hot(volume: Option<&crate::system_resources::drives::DriveInfo>) -> bool {
        use crate::system_resources::DiskPressureLevel;
        let level = match volume {
            Some(volume) if volume.total_bytes > 0 => DiskPressureLevel::from_pressure(
                volume.total_bytes.saturating_sub(volume.available_bytes) as f64
                    / volume.total_bytes as f64,
            ),
            _ => DiskPressureLevel::Unknown,
        };
        level >= DiskPressureLevel::High
    }

    /// Run `job` on the blocking pool, unawaited, holding the one job slot until it ends.
    fn start(&self, job: impl FnOnce(&Targets) -> Option<Snapshot> + Send + 'static) {
        let slot = JobSlot(self.busy.clone());
        let targets = self.targets.clone();
        let publish = self.snapshot.clone();
        self.runtime.spawn_blocking(move || {
            let _slot = slot;
            if let Some(snapshot) = job(&targets) {
                publish.send_replace(Some(Arc::new(snapshot)));
            }
        });
    }
}

impl ResourcePool for WorktreeTargetsPool {
    fn tier_name(&self) -> &str {
        &self.targets.tier
    }

    fn capacity_bytes(&self) -> u64 {
        (self.targets.budget_for)(
            (self.targets.volume_sample)(self.targets.tracked.path()).as_ref(),
        )
    }

    fn usage_bytes(&self) -> u64 {
        self.targets.tracked.bytes()
    }

    /// Over its budget (the keep-recent line: the broker then trims it on its next tick,
    /// whatever the volume) OR on a hot volume. Within budget the class reads only the
    /// volume: a cache that fits its budget is the policy met, not pressure building, so
    /// the broker is not called to free nothing at 80-100% of it. Reads the scanner's
    /// cached size and the volume's published sample: no IO.
    fn pressure(&self) -> f64 {
        let volume = (self.targets.volume_sample)(self.targets.tracked.path());
        let budget = (self.targets.budget_for)(volume.as_ref());
        let bytes = self.targets.tracked.bytes();
        let over = if bytes > budget { bytes } else { 0 };
        cache_pressure_on_volume(over, budget, volume.as_ref())
    }

    /// Never touches the disk: reports what finished jobs freed, then starts at most one
    /// background job (a measurement when the snapshot is missing or stale, else an
    /// eviction planned from it) and returns without waiting for it.
    fn evict_at_least(&self, want_bytes: u64) -> u64 {
        use std::sync::atomic::Ordering;
        let reported = self.freed.swap(0, Ordering::AcqRel);
        if self.busy.swap(true, Ordering::AcqRel) {
            return reported;
        }
        let current = self.snapshot.borrow().clone();
        let fresh =
            current.filter(|snapshot| snapshot.observation == self.targets.tracked.observation());
        match fresh {
            None => self.start(|targets| targets.measure()),
            Some(snapshot) => {
                let plan = self.plan(&snapshot, want_bytes);
                if snapshot.detached.is_empty() && plan.beyond == 0 && plan.pressure_ask == 0
                    || snapshot.live.is_empty() && snapshot.detached.is_empty()
                {
                    self.busy.store(false, Ordering::Release);
                    return reported;
                }
                if self.mode == WorktreeEvictionMode::Observe {
                    self.publish_would_take(Self::planned_targets(&snapshot, plan));
                    self.busy.store(false, Ordering::Release);
                    return reported;
                }
                let freed = self.freed.clone();
                self.start(move |targets| {
                    freed.fetch_add(targets.evict(&snapshot, plan), Ordering::AcqRel);
                    targets.measure()
                });
            }
        }
        reported
    }

    fn snapshot(&self) -> Vec<ResourcePoolEntry> {
        Vec::new()
    }

    /// Observe mode is not a relief candidate: the broker never asks it to free and never
    /// alerts on it (a "0 bytes freed" alert every tick would bury the real ones).
    fn relief(&self) -> crate::paging::pool::Relief {
        match self.mode {
            WorktreeEvictionMode::Observe => crate::paging::pool::Relief::ObservesOnly,
            WorktreeEvictionMode::Delete => crate::paging::pool::Relief::Evicts,
        }
    }

    /// Each relief pass in observe mode: with the ask the broker would make, measure and
    /// publish the would-take list (the same tick path, which deletes nothing in this mode);
    /// below the act line, an empty plan.
    fn observe(&self, would_ask: Option<u64>) {
        match would_ask {
            Some(want) => {
                self.evict_at_least(want);
            }
            None => self.publish_would_take(Vec::new()),
        }
    }
}

#[cfg(test)]
mod tests;
