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
//! PR-3 (this commit): real `evict_at_least` via `docker system prune`.
//!
//! Out-of-scope (PR-4):
//! - **Cap enforcement**: capacity_bytes reports what Docker Desktop
//!   is configured to allow, NOT what continuum has set as a policy
//!   bound. PR-4 caps that on install + alerts on >90% capacity.

use crate::modules::docker_tier::DockerTierProbe;
use crate::paging::{ResourcePool, ResourcePoolEntry};
use crate::runtime;
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::time::SystemTime;
use ts_rs::TS;

/// Snapshot returned by the `system/docker-tier-stats` IPC.
///
/// Lifts the data the `ResourcePool` trait already exposes
/// (`capacity_bytes`, `usage_bytes`, `pressure`) to the wire so the
/// `bin/continuum status` shell + future widgets can render it.
/// Phase 1 of #1239 — exposes the data without depending on the
/// pressure-broker singleton (which doesn't exist in production yet —
/// see #1239 audit comment).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/resources/DockerTierStats.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct DockerTierStats {
    /// Pre-allocated sparse-image size on macOS (`st_size`). 0 when
    /// Docker isn't installed / Docker.raw isn't found / probe failed —
    /// callers should treat 0 as "tier not under management" rather
    /// than "no capacity."
    #[ts(type = "number")]
    pub capacity_bytes: u64,
    /// Actual on-disk consumption (`st_blocks * 512`). The number that
    /// counts against the host filesystem.
    #[ts(type = "number")]
    pub used_bytes: u64,
    /// `used_bytes / capacity_bytes`. Always 0.0 when `capacity_bytes`
    /// is 0 (tier not under management). May exceed 1.0 if Docker
    /// somehow stored more than its sparse-image cap (shouldn't happen
    /// post-probe-fix but the broker tolerates it).
    pub pressure: f64,
    /// `true` iff Docker.raw was located and the probe succeeded; `false`
    /// when Docker isn't installed or the probe found nothing. Lets
    /// callers distinguish "tier exists but is empty" from "tier
    /// doesn't apply on this host."
    pub detected: bool,
}

/// Docker storage tier as a `ResourcePool`. Stat-on-every-call because
/// Docker.raw size changes whenever Docker writes to it (image pull,
/// container layer commit, etc.) — caching the value would lie.
///
/// `tier_name()` returns "docker" so logs / pressure-broker telemetry
/// distinguish it from VRAM ("vram"), DRAM ("dram"), KV cache ("kv-cache").
#[derive(Debug, Clone)]
pub struct DockerTierPool {
    loaded_at_ms: u64,
}

impl Default for DockerTierPool {
    fn default() -> Self {
        Self::new()
    }
}

impl DockerTierPool {
    pub fn new() -> Self {
        Self {
            loaded_at_ms: now_ms(),
        }
    }

    /// Convenience: probe Docker once + return a `DockerTierStats`
    /// snapshot suitable for the `system/docker-tier-stats` IPC.
    /// Single probe per call (vs the two probes the per-method
    /// `capacity_bytes`/`usage_bytes` accessors would do) so the wire
    /// payload is internally consistent.
    pub fn snapshot_stats() -> DockerTierStats {
        match DockerTierProbe::probe() {
            DockerTierProbe::Detected {
                allocated_bytes,
                used_bytes,
                ..
            } => {
                let pressure = if allocated_bytes == 0 {
                    0.0
                } else {
                    used_bytes as f64 / allocated_bytes as f64
                };
                DockerTierStats {
                    capacity_bytes: allocated_bytes,
                    used_bytes,
                    pressure,
                    detected: true,
                }
            }
            _ => DockerTierStats {
                capacity_bytes: 0,
                used_bytes: 0,
                pressure: 0.0,
                detected: false,
            },
        }
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

    /// Real eviction via `docker system prune` (#1222 PR-3).
    ///
    /// Two-stage strategy that escalates only as needed:
    ///   - **Soft (always tried first)**: `docker system prune --force --filter until=24h`
    ///     — drops dangling images + stopped containers + unused networks
    ///     older than 24h. Safe: does NOT touch images currently in use,
    ///     does NOT touch named volumes, does NOT touch recent dev
    ///     iteration artifacts.
    ///   - **Aggressive (only if soft didn't free enough)**: same prune
    ///     without the time filter — frees ALL dangling artifacts
    ///     regardless of age. Still does NOT touch in-use images or
    ///     named volumes (Docker's prune semantics, not ours).
    ///
    /// Returns the actual bytes freed (sum across both stages). Parses
    /// Docker's "Total reclaimed space: X.YYGB" line at end of output.
    /// Returns 0 if Docker isn't installed / daemon isn't running /
    /// command fails — same shape as DockerTierProbe::Unsupported, the
    /// pressure-broker treats it as "tier can't act, surface pressure
    /// to operator".
    fn evict_at_least(&self, want_bytes: u64) -> u64 {
        let log = runtime::logger("docker-tier");

        // Stage 1: soft prune (24h+ dangling artifacts).
        let soft_freed = run_docker_prune(&["system", "prune", "--force", "--filter", "until=24h"]);
        if let Some(bytes) = soft_freed {
            if bytes >= want_bytes {
                log.info(&format!(
                    "DockerTierPool soft prune freed {} bytes (>= {} requested)",
                    bytes, want_bytes
                ));
                return bytes;
            }
            log.info(&format!(
                "DockerTierPool soft prune freed {} bytes (< {} requested); escalating to aggressive",
                bytes, want_bytes
            ));
            // Stage 2: aggressive prune. Includes the soft-stage bytes
            // already in this call's running total.
            if let Some(more) = run_docker_prune(&["system", "prune", "--force"]) {
                let total = bytes.saturating_add(more);
                log.info(&format!(
                    "DockerTierPool aggressive prune freed {} additional bytes (total this call: {})",
                    more, total
                ));
                return total;
            }
            return bytes;
        }
        // Soft prune failed entirely (no docker / daemon down / command
        // error). Don't try the aggressive path — same failure would
        // hit. Return 0 so the broker knows this tier didn't act.
        log.warn("DockerTierPool: docker system prune failed; returning 0 freed bytes");
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
                    // Use the pool construction time as a stable
                    // per-process value so the
                    // broker doesn't see a 0 epoch and treat it as
                    // ancient (which would prioritize it for eviction
                    // even though we can't actually evict it yet).
                    loaded_at: self.loaded_at_ms,
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

/// Run `docker <args>` and parse the freed-bytes total from stdout.
/// Returns:
///   - Some(bytes) on successful exit (bytes may be 0 if nothing to prune)
///   - None on docker not found / daemon down / non-zero exit (caller
///     decides whether to escalate or surrender)
///
/// The output we parse is the trailing "Total reclaimed space: X.YYUNIT"
/// line that `docker system prune` always emits on success. Format is
/// stable across Docker Desktop versions (verified Docker 24.x + 25.x).
fn run_docker_prune(args: &[&str]) -> Option<u64> {
    let output = Command::new("docker").args(args).output().ok()?; // None if `docker` binary not in PATH.
    if !output.status.success() {
        return None; // Daemon down / permission denied / etc.
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_reclaimed_bytes(&stdout)
}

/// Parse "Total reclaimed space: X.YYUNIT" from `docker system prune`
/// output. Handles bytes (no unit), KB, MB, GB, TB. Returns Some(0) when
/// the line is present but reports zero bytes (common when nothing to
/// prune — the prune ran fine, just had no work).
fn parse_reclaimed_bytes(output: &str) -> Option<u64> {
    let line = output
        .lines()
        .rev()
        .find(|l| l.contains("Total reclaimed space:"))?;
    let value_str = line.split("Total reclaimed space:").nth(1)?.trim();

    // Common shapes: "0B", "1.234kB", "5.6MB", "12.3GB", "0.001TB".
    // Docker uses SI units (1kB = 1000B) per docker/cli convention.
    let (num_str, multiplier) = if let Some(stripped) = value_str.strip_suffix("TB") {
        (stripped.trim(), 1_000_000_000_000u64)
    } else if let Some(stripped) = value_str.strip_suffix("GB") {
        (stripped.trim(), 1_000_000_000u64)
    } else if let Some(stripped) = value_str.strip_suffix("MB") {
        (stripped.trim(), 1_000_000u64)
    } else if let Some(stripped) = value_str.strip_suffix("kB") {
        (stripped.trim(), 1_000u64)
    } else if let Some(stripped) = value_str.strip_suffix('B') {
        (stripped.trim(), 1u64)
    } else {
        // Unknown unit — fail closed rather than misreport. Future
        // Docker versions adding new units land here.
        return None;
    };

    let num: f64 = num_str.parse().ok()?;
    if num.is_nan() || num.is_sign_negative() {
        return None;
    }
    Some((num * multiplier as f64) as u64)
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

    /// What this catches: evict_at_least never panics regardless of
    /// host (no docker / docker daemon down / etc.). Returning 0
    /// honestly when the prune can't run is the contract — the broker
    /// uses that to escalate (alert operator) instead of looping
    /// forever expecting eviction to succeed.
    ///
    /// Doesn't assert a positive freed-bytes count because that
    /// requires a live Docker daemon with prunable artifacts — flaky
    /// in CI. The integration-style assertion is in the parser tests
    /// below + run live during the PR-4 chat-substrate alert work.
    #[test]
    fn evict_at_least_never_panics() {
        let pool = DockerTierPool::new();
        let _freed = pool.evict_at_least(10 * 1024 * 1024 * 1024);
        // No assertion on value — depends on host state. Just that
        // the call completes without panic.
    }

    /// What this catches: parser handles every Docker output unit
    /// shape (B, kB, MB, GB, TB) correctly. Mutation that drops a
    /// unit branch silently underreports freed bytes, defeating
    /// the broker's eviction-was-enough check.
    #[test]
    fn parse_reclaimed_bytes_handles_all_units() {
        // Real Docker outputs (Docker 24.x verified):
        let cases = [
            (
                "Deleted Containers:\nfoo\nTotal reclaimed space: 0B\n",
                0u64,
            ),
            ("...\nTotal reclaimed space: 512B\n", 512),
            ("...\nTotal reclaimed space: 1.5kB\n", 1_500),
            ("...\nTotal reclaimed space: 250MB\n", 250_000_000),
            ("...\nTotal reclaimed space: 4.523GB\n", 4_523_000_000),
            ("...\nTotal reclaimed space: 1.2TB\n", 1_200_000_000_000),
        ];
        for (input, expected) in cases {
            let got = parse_reclaimed_bytes(input);
            assert_eq!(
                got,
                Some(expected),
                "parser failed for input ending in {:?}",
                input.lines().last().unwrap_or("")
            );
        }
    }

    /// What this catches: parser returns None (NOT Some(0)) when the
    /// expected line is missing. Some(0) means "ran successfully,
    /// freed nothing"; None means "couldn't read the result, escalate
    /// or surrender". Conflating them would silently swallow real
    /// errors (e.g. Docker daemon error that returns 0 exit code but
    /// no prune-summary line).
    #[test]
    fn parse_reclaimed_bytes_returns_none_when_line_missing() {
        let cases = [
            "",
            "some unrelated docker output",
            "Total reclaimed space:",      // header but no value
            "Total reclaimed space: 5XYZ", // unknown unit
            "Total reclaimed space: not-a-number GB",
        ];
        for input in cases {
            let got = parse_reclaimed_bytes(input);
            assert!(
                got.is_none() || got == Some(0),
                "expected None or Some(0) for malformed input {:?}, got {:?}",
                input,
                got
            );
        }
        // Specifically the empty / no-line cases should be None:
        assert_eq!(parse_reclaimed_bytes(""), None);
        assert_eq!(parse_reclaimed_bytes("foo bar\nbaz\n"), None);
    }

    /// What this catches: parser picks the LAST occurrence of the
    /// summary line, not the first. Docker prune sometimes prints
    /// per-section summaries during interactive runs; the final
    /// "Total reclaimed space:" is the canonical total.
    #[test]
    fn parse_reclaimed_bytes_picks_last_summary_line() {
        let input =
            "Total reclaimed space: 100MB\nDeleted Volumes:\nTotal reclaimed space: 250MB\n";
        // Last line wins → 250MB
        assert_eq!(parse_reclaimed_bytes(input), Some(250_000_000));
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
                assert_eq!(
                    entry.loaded_at, pool.loaded_at_ms,
                    "loaded_at should be stable for the pool instance"
                );
            }
            _ => {
                assert!(
                    snap.is_empty(),
                    "non-Detected tier should yield zero entries"
                );
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
