//! Eviction owner for the substrate's own rotation-generation dirs —
//! `~/.continuum/logs` and `~/.continuum/probes`.
//!
//! ## Why this exists
//!
//! Both directories are written continuously by the substrate itself
//! ([`crate::routing::capped_appender::CappedAppender`]), and until
//! 2026-08-06 their ONLY bound was a private constant inside that
//! writer: `MAX_LOG_BYTES * (KEEP + 1)`. That is a bound, but it is not
//! GOVERNANCE — the writer decides its own ceiling, the disk monitor
//! cannot see the class at all (neither dir appeared in
//! [`standard_tracked_dirs`](super::disk_reporters::standard_tracked_dirs)),
//! and the [`PressureBroker`](crate::paging::PressureBroker) has no way
//! to ask for bytes back when the volume is actually tight. Per Joel:
//! *"everything consuming resources needs to act via these traits and
//! authorities"* — a writer that caps itself is exactly the hack that
//! rule names.
//!
//! It is also the same shape as the 2026-07-13 incident
//! (`[[no-new-cache-dir-without-an-eviction-decision]]`): every
//! individual component worked, and 460 GB accumulated anyway, because
//! nothing OWNED the eviction. The probe stream is by
//! `tracing_init`'s own description "the HIGHEST-volume writer in the
//! substrate," so it is the last class that should have been invisible.
//!
//! ## Why eviction here is safe by construction
//!
//! [`CappedAppender`](crate::routing::capped_appender) rotates
//! `x.log` → `x.log.1` → … → `x.log.N`. The LIVE file is the
//! unsuffixed one; every `.N` is history the writer has already moved
//! past and holds no handle to. Dropping the oldest generations is
//! therefore safe without any in-flight-set knowledge — unlike
//! `benchmarks` (must not delete a grading instance) or
//! `genome-models` (must not delete a served artifact). That is why
//! this class gets a real owner instead of a deferred entry: there is
//! no hazard to defer FOR.
//!
//! The live file is never touched. Under sustained pressure the pool
//! frees what history it has and reports honestly; the floor is one
//! generation, which the writer needs to keep working.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::disk_reporters::TrackedDir;
use crate::paging::pool::{ResourcePool, ResourcePoolEntry};

/// Governed eviction owner over one rotation-generation directory.
///
/// Shares its [`TrackedDir`] with the disk reporter — one measurement,
/// two consumers, same as [`CargoTargetPool`](super::disk_eviction::CargoTargetPool).
pub struct RotationLogPool {
    tracked: Arc<TrackedDir>,
    budget_bytes: u64,
    tier: String,
}

impl RotationLogPool {
    /// `budget_bytes` is the GOVERNED ceiling, which is a different
    /// number from the writer's rotation arithmetic. Rotation decides
    /// how much history exists in the steady state; this decides how
    /// much the machine is willing to spend on it right now. The
    /// governor may set it below the rotation ceiling, and then the
    /// broker will actually claw generations back — the capability
    /// that did not exist while the writer bounded itself.
    pub fn new(tracked: Arc<TrackedDir>, budget_bytes: u64) -> Self {
        let tier = format!("disk-{}", tracked.name());
        Self {
            tracked,
            budget_bytes: budget_bytes.max(1),
            tier,
        }
    }

    /// Rotated generations under `root`, oldest first.
    ///
    /// A generation is a file whose final extension parses as a
    /// number — the `.N` suffix `CappedAppender::generation_path`
    /// appends. Higher N = older. The live (unsuffixed) file never
    /// parses, so it can never enter this list; that is the safety
    /// property, enforced by the parse rather than by a name match.
    fn generations(root: &Path) -> Vec<(usize, PathBuf, u64)> {
        let Ok(entries) = std::fs::read_dir(root) else {
            return Vec::new();
        };
        let mut out: Vec<(usize, PathBuf, u64)> = entries
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                let meta = e.metadata().ok()?;
                if !meta.is_file() {
                    return None;
                }
                let generation: usize = path.extension()?.to_str()?.parse().ok()?;
                Some((generation, path, meta.len()))
            })
            .collect();
        // Oldest (highest generation) first — cheapest regret leaves first.
        out.sort_by(|a, b| b.0.cmp(&a.0));
        out
    }
}

impl ResourcePool for RotationLogPool {
    fn tier_name(&self) -> &str {
        &self.tier
    }

    fn capacity_bytes(&self) -> u64 {
        self.budget_bytes
    }

    fn usage_bytes(&self) -> u64 {
        self.tracked.bytes()
    }

    fn evict_at_least(&self, want_bytes: u64) -> u64 {
        let root = self.tracked.path().to_path_buf();
        if !root.exists() {
            return 0;
        }
        let mut freed = 0u64;
        for (_generation, path, size) in Self::generations(&root) {
            if freed >= want_bytes.max(1) {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                freed = freed.saturating_add(size);
            }
        }
        // Tell the shared measurement immediately. The broker ticks every
        // 5 s but the DiskUsageScanner only walks every 5 min, so without
        // this the pool keeps reporting the pre-eviction size and the
        // broker re-fires relief against space that is already gone —
        // burning a rung per tick until the scanner catches up. Same
        // contract CargoTargetPool observes; it is the reason
        // `TrackedDir::record_freed` exists.
        if freed > 0 {
            self.tracked.record_freed(freed);
        }
        freed
    }

    fn snapshot(&self) -> Vec<ResourcePoolEntry> {
        Self::generations(self.tracked.path())
            .into_iter()
            .map(|(generation, path, size)| ResourcePoolEntry {
                key: path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                size_bytes: size,
                pinned_count: 0,
                // Generation number IS the age ordering the appender
                // maintains; we don't restate it as a fabricated
                // timestamp. 0 = "not a clock-derived value" rather
                // than a plausible-looking wrong one.
                loaded_at: 0,
                last_access_at: 0,
                access_count: generation as u64,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a rotation dir: one live file plus `gens` generations.
    /// Seeds the tracked size through the scanner's own write seam so
    /// the pool reads the same measurement the reporter would.
    fn seeded(root: &Path, live_bytes: usize, gens: &[(usize, usize)]) -> Arc<TrackedDir> {
        std::fs::create_dir_all(root).expect("mkdir");
        std::fs::write(root.join("continuum-probes.jsonl"), vec![b'L'; live_bytes]).expect("live");
        let mut total = live_bytes as u64;
        for (generation, size) in gens {
            std::fs::write(
                root.join(format!("continuum-probes.jsonl.{generation}")),
                vec![b'G'; *size],
            )
            .expect("generation");
            total += *size as u64;
        }
        let tracked = TrackedDir::new("probes", root.to_path_buf());
        tracked.set_bytes(total);
        tracked
    }

    /// what this catches: an eviction that takes the file the writer
    /// currently holds open. The live file has no `.N` suffix and must
    /// survive every rung, including a want_bytes larger than the whole
    /// directory.
    #[test]
    fn eviction_never_touches_the_live_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("probes");
        let tracked = seeded(&root, 500, &[(1, 100), (2, 100), (3, 100)]);
        let pool = RotationLogPool::new(tracked, 200);

        let freed = pool.evict_at_least(u64::MAX);

        assert_eq!(freed, 300, "all three generations are reclaimable");
        assert!(
            root.join("continuum-probes.jsonl").exists(),
            "the live file must survive unbounded eviction pressure"
        );
        assert!(!root.join("continuum-probes.jsonl.1").exists());
        assert!(!root.join("continuum-probes.jsonl.3").exists());
    }

    /// what this catches: evicting newest-first, which would throw away
    /// the history closest to the incident an operator is debugging
    /// while keeping the stalest bytes on disk.
    #[test]
    fn eviction_drops_the_oldest_generation_first() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("probes");
        let tracked = seeded(&root, 10, &[(1, 100), (2, 100), (3, 100)]);
        let pool = RotationLogPool::new(tracked, 50);

        // Ask for less than one generation — exactly one must go, and
        // it must be the oldest (.3).
        let freed = pool.evict_at_least(1);

        assert_eq!(freed, 100, "one generation satisfies the request");
        assert!(
            !root.join("continuum-probes.jsonl.3").exists(),
            ".3 is oldest"
        );
        assert!(
            root.join("continuum-probes.jsonl.1").exists(),
            ".1 is newest history"
        );
        assert!(root.join("continuum-probes.jsonl.2").exists());
    }

    /// what this catches: eviction that frees bytes on disk but leaves
    /// the shared measurement stale. The broker ticks every 5 s and the
    /// scanner walks every 5 min, so a pool that doesn't call
    /// `record_freed` reports its pre-eviction size for minutes and the
    /// broker keeps asking for relief it already got — one rung burned
    /// per tick.
    #[test]
    fn eviction_updates_the_shared_measurement_immediately() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("probes");
        let tracked = seeded(&root, 100, &[(1, 100), (2, 100)]);
        let before = tracked.bytes();
        let pool = RotationLogPool::new(tracked.clone(), 50);

        let freed = pool.evict_at_least(100);

        assert!(freed > 0, "must free something");
        assert_eq!(
            tracked.bytes(),
            before - freed,
            "TrackedDir must reflect the delete before the next scanner walk"
        );
        assert_eq!(pool.usage_bytes(), before - freed);
    }

    /// what this catches: the pool reporting pressure against a budget
    /// it doesn't actually own. Over-budget must be visible as >1.0 so
    /// the broker acts — the whole point of registering the class.
    #[test]
    fn over_budget_usage_surfaces_as_pressure_above_one() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("probes");
        let tracked = seeded(&root, 400, &[(1, 400)]);
        let pool = RotationLogPool::new(tracked, 100);

        assert!(
            pool.pressure() > 1.0,
            "800 bytes against a 100-byte budget must read as over-budget, got {}",
            pool.pressure()
        );
    }

    /// what this catches: a missing directory being treated as an
    /// error or a panic. A node that never armed probes has no dir,
    /// and the pool must simply report nothing to give.
    #[test]
    fn absent_directory_frees_nothing_without_panicking() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let tracked = TrackedDir::new("probes", tmp.path().join("never-created"));
        let pool = RotationLogPool::new(tracked, 1024);
        assert_eq!(pool.evict_at_least(4096), 0);
        assert!(pool.snapshot().is_empty());
    }
}
