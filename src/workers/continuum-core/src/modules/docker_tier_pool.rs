//! `ResourcePool` impl for the Docker storage tier (#1222 PR-2).
//!
//! Wraps `modules::docker_tier::DockerTierProbe` so the resource manager
//! can ask Docker the same questions it asks every other tier
//! (paging, GPU, KV cache): `capacity_bytes()`, `usage_bytes()`,
//! `evict_at_least()`, `snapshot()`.
//!
//! Builds on:
//! - #1222 PR-1 — DockerTierProbe (the discovery primitive)
//! - #1228 — ResourcePool trait (the shared shape sibling shipped)
//!
//! Joel directive 2026-05-14: "code concurrency ONCE then incorporate
//! it. Any hard coded into a subclass or at a lower level use of tokio
//! etc are probably WRONG." Same rule for memory accounting — every
//! tier implements ONE shared trait so the broker treats them
//! uniformly. This is the second non-paging-pool ResourcePool impl
//! (after VRAM/DRAM/KV cache via PagedResourcePool itself), proving
//! the trait fits a fundamentally different storage shape (a single
//! sparse disk file instead of a per-key cache).
//!
//! Out-of-scope for PR-2:
//! - **Eviction implementation**: evict_at_least is a stub that logs
//!   and returns 0. PR-3 wires `docker system prune` (CLI exec) to
//!   free dangling images / unused volumes when over budget.
//! - **Cap enforcement**: capacity_bytes reports what Docker Desktop
//!   is configured to allow, NOT what continuum has set as a policy
//!   bound. PR-2 of #1222 (separate) caps that on install.

use crate::modules::docker_tier::DockerTierProbe;
use crate::paging::{ResourcePool, ResourcePoolEntry};
use std::time::SystemTime;

/// Docker storage tier as a `ResourcePool`. Stat-on-every-call because
/// Docker.raw size changes whenever Docker writes to it (image pull,
/// container layer commit, etc.) — caching the value would lie.
///
/// `tier_name()` returns "docker" so logs / pressure-broker telemetry
/// distinguish it from VRAM ("vram"), DRAM ("dram"), KV cache ("kv-cache").
#[derive(Debug, Clone, Default)]
pub struct DockerTierPool;

impl DockerTierPool {
    pub fn new() -> Self {
        Self
    }
}

impl ResourcePool for DockerTierPool {
    fn tier_name(&self) -> &str {
        "docker"
    }

    /// Pre-allocated sparse-image size on macOS (`st_size`). This IS
    /// the capacity bound — Docker cannot store more than this without
    /// growing the sparse image, and growing-the-image was the failure
    /// mode of the 2026-05-14 incident (Docker.raw silently grew to
    /// fill the whole disk). Returns 0 when not detected so the
    /// pressure-broker treats this tier as "not under management"
    /// rather than "no capacity".
    fn capacity_bytes(&self) -> u64 {
        match DockerTierProbe::probe() {
            DockerTierProbe::Detected {
                allocated_bytes, ..
            } => allocated_bytes,
            _ => 0,
        }
    }

    /// Actual on-disk consumption (`st_blocks * 512`). The number that
    /// counts against the host filesystem.
    fn usage_bytes(&self) -> u64 {
        match DockerTierProbe::probe() {
            DockerTierProbe::Detected { used_bytes, .. } => used_bytes,
            _ => 0,
        }
    }

    /// PR-2 stub: returns 0 (no bytes freed). PR-3 wires
    /// `docker system prune` to free dangling images + unused volumes.
    /// Returning 0 honestly lets the pressure-broker know this tier
    /// can't release pressure on its own yet — it can still SURFACE
    /// the pressure (capacity vs usage), it just can't ACT on it
    /// without operator intervention.
    fn evict_at_least(&self, _want_bytes: u64) -> u64 {
        // TODO(#1222 PR-3): wire `docker system prune --filter "until=24h"`
        // for soft eviction or `--all` for aggressive. Until then, the
        // operator gets a warning surfaced via the broker (PR-4).
        0
    }

    /// Single-entry snapshot representing the Docker.raw sparse image
    /// as the one "page" in this tier. PR-3 may expand this to per-image
    /// granularity once `docker system df --format json` is wired —
    /// that would let the broker pick which images to evict first.
    ///
    /// `size_bytes` carries the actual on-disk consumption (used_bytes).
    /// allocated_bytes is the capacity bound (already on the pool via
    /// `capacity_bytes()`), not a per-entry footprint, so it's not
    /// duplicated into the entry.
    fn snapshot(&self) -> Vec<ResourcePoolEntry> {
        match DockerTierProbe::probe() {
            DockerTierProbe::Detected {
                allocated_bytes: _,
                used_bytes,
                path,
            } => {
                let now = now_ms();
                vec![ResourcePoolEntry {
                    // Use the absolute path as the entry key. Stable
                    // across calls; the broker can correlate snapshots
                    // taken at different times.
                    key: path,
                    size_bytes: used_bytes,
                    pinned_count: 0,
                    // No real "loaded_at" for a sparse disk image —
                    // it's been there since Docker Desktop installed.
                    // Use now_ms as a stable-per-process value so the
                    // broker doesn't see a 0 epoch and treat it as
                    // ancient (which would prioritize it for eviction
                    // even though we can't actually evict it yet).
                    loaded_at: now,
                    last_access_at: now,
                    access_count: 0,
                }]
            }
            _ => Vec::new(),
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: tier_name is the stable string "docker"
    /// that telemetry + pressure-broker dispatch keys off. A rename
    /// would silently break log filtering / per-tier dashboards.
    #[test]
    fn tier_name_is_docker() {
        let pool = DockerTierPool::new();
        assert_eq!(pool.tier_name(), "docker");
    }

    /// What this catches: capacity_bytes / usage_bytes never panic and
    /// return non-negative. usage <= capacity invariant must hold when
    /// both are non-zero (capacity == 0 means "not under management"
    /// and usage being non-zero would just mean Docker is installed
    /// but the probe disagrees — surface as a smell but don't assert).
    #[test]
    fn capacity_and_usage_never_panic_and_invariant_holds_when_managed() {
        let pool = DockerTierPool::new();
        let cap = pool.capacity_bytes();
        let used = pool.usage_bytes();
        if cap > 0 {
            assert!(
                used <= cap,
                "usage {used} should be <= capacity {cap} when tier is managed"
            );
        }
    }

    /// What this catches: evict_at_least is a known-stub. If a future
    /// caller starts depending on it actually freeing bytes, this test
    /// catches the assumption (PR-3 will replace with the real impl
    /// AND replace this test with the actual eviction assertion).
    #[test]
    fn evict_at_least_is_stub_returning_zero() {
        let pool = DockerTierPool::new();
        let freed = pool.evict_at_least(10 * 1024 * 1024 * 1024);
        assert_eq!(
            freed, 0,
            "PR-2 stub should return 0; PR-3 replaces with `docker system prune`"
        );
    }

    /// What this catches: snapshot returns the right shape (one entry
    /// when Docker is detected, empty when it isn't). Mutation that
    /// returns an entry without setting key/size_bytes would surface
    /// as broker-side telemetry holes; this test pins the contract.
    #[test]
    #[cfg(target_os = "macos")]
    fn snapshot_returns_single_entry_when_detected() {
        let pool = DockerTierPool::new();
        let snap = pool.snapshot();
        match DockerTierProbe::probe() {
            DockerTierProbe::Detected { .. } => {
                assert_eq!(snap.len(), 1, "Detected tier should yield one entry");
                let entry = &snap[0];
                assert!(
                    entry.key.ends_with("Docker.raw"),
                    "entry key should be the Docker.raw path, got: {}",
                    entry.key
                );
            }
            _ => {
                assert!(snap.is_empty(), "non-Detected tier should yield zero entries");
            }
        }
    }

    /// What this catches: dyn-dispatching DockerTierPool through the
    /// ResourcePool trait works. If the trait's object-safety changed
    /// (e.g. someone added a generic method), this fails to compile.
    /// The pressure-broker stores tiers as `Box<dyn ResourcePool>`, so
    /// this is the realistic call path.
    #[test]
    fn implements_resource_pool_via_dyn() {
        let pool: Box<dyn ResourcePool> = Box::new(DockerTierPool::new());
        assert_eq!(pool.tier_name(), "docker");
        let _ = pool.capacity_bytes();
        let _ = pool.usage_bytes();
        let _ = pool.evict_at_least(1024);
        let _ = pool.snapshot();
    }
}
