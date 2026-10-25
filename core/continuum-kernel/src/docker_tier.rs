//! Docker storage tier discovery (#1222 PR-1).
//!
//! Surfaces the size + on-disk usage of Docker Desktop's sparse disk
//! image so the resource manager can account for it as part of the
//! unified system memory pool. This module is **discovery only** —
//! capping, eviction, and scheduler integration are PR-2 / PR-3 / PR-4
//! under the same card.
//!
//! ## Why this exists
//!
//! Joel directive 2026-05-14: "memory in this system, including the
//! docker allotment needs to be managed by the system, FULLY."
//!
//! The 2026-05-14 incident proved the cost of NOT measuring this:
//! Docker.raw silently grew to 926GB (the entire disk), every tool call
//! started failing with ENOSPC, recovery required `rm Docker.raw`
//! (destructive, manual). The first step toward Joel's "FULLY managed"
//! is **knowing the number** — this module returns it.
//!
//! ## Cross-platform
//!
//! - **macOS** — Docker Desktop stores its raw disk image at
//!   `~/Library/Containers/com.docker.docker/Data/vms/0/data/Docker.raw`.
//!   `apparent size` (the size Docker pre-allocated as a sparse file)
//!   and `on-disk size` (the actual blocks consumed) are different
//!   numbers; both matter. `stat(2)` returns both via `st_size` (apparent)
//!   and `st_blocks` (on-disk, in 512-byte units).
//! - **Windows** — Docker Desktop on WSL2 stores its data inside the
//!   WSL2 ext4 partition; the equivalent file is per-distro and not
//!   cleanly probable from the host. Returns `Probe::Unsupported` with
//!   a reason; PR-2 will handle this via WSL exec or Windows-side
//!   Docker Desktop API.
//! - **Linux** — native Docker uses overlay2 on `/var/lib/docker`; the
//!   per-image / per-volume usage is exposed via `docker system df`,
//!   not a single file. Returns `Probe::Unsupported`; PR-2 wires
//!   `docker system df --format json`.

use crate::paths::docker::{raw_image_path, DockerRawPath};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Result of probing the Docker storage tier on the current host.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/system/DockerTierProbe.ts"
)]
pub enum DockerTierProbe {
    /// Probe succeeded; Docker storage is detected and reportable.
    Detected {
        /// Pre-allocated capacity (`st_size` on macOS for the sparse
        /// disk image). This is the upper bound — the system cannot
        /// store more Docker content than this without growing the
        /// sparse image.
        #[ts(type = "number")]
        allocated_bytes: u64,
        /// Actual on-disk consumption (`st_blocks * 512` on macOS).
        /// This is what counts against the host filesystem's usage,
        /// because `apparent size` for a sparse file overstates the
        /// real block count when most of the file is unallocated.
        #[ts(type = "number")]
        used_bytes: u64,
        /// Path the probe inspected. Surfaced for diagnostics.
        path: String,
    },
    /// Docker is installed but the file expected by the probe is
    /// missing (e.g., user uninstalled Docker Desktop but left the
    /// directory; OS-specific path moved). Distinct from `Unsupported`
    /// because the platform CAN be probed, just not on this host.
    NotFound {
        /// Path the probe attempted to inspect.
        path: String,
        reason: String,
    },
    /// This OS / configuration is not yet implemented for direct probe.
    /// Returning the variant rather than panicking lets callers carry
    /// on (the resource manager treats unprobeable tiers as `unknown
    /// capacity` and refuses to bound on them).
    Unsupported { os: String, reason: String },
}

impl DockerTierProbe {
    /// Run the probe for the current host. Pure (no allocations beyond
    /// the returned variant + path string).
    ///
    /// Pure synchronous I/O — `stat(2)` syscall only on the supported
    /// path. Fast enough to call from any context; no need to push to
    /// a worker thread.
    pub fn probe() -> Self {
        if cfg!(target_os = "macos") {
            Self::probe_macos()
        } else if cfg!(target_os = "windows") {
            Self::Unsupported {
                os: "windows".to_string(),
                reason: "Docker Desktop on WSL2 stores per-distro inside the WSL2 partition; \
                         not directly probeable from the host. PR-2 will wire via WSL exec."
                    .to_string(),
            }
        } else if cfg!(target_os = "linux") {
            Self::Unsupported {
                os: "linux".to_string(),
                reason: "Native Docker on Linux uses overlay2 on /var/lib/docker; \
                         per-image / per-volume usage requires `docker system df`. \
                         PR-2 will wire that path."
                    .to_string(),
            }
        } else {
            Self::Unsupported {
                os: std::env::consts::OS.to_string(),
                reason: "no probe implemented for this OS".to_string(),
            }
        }
    }

    /// macOS-specific probe. Inspects the Docker Desktop sparse disk
    /// image at the path resolved by `paths::docker::raw_image_path()`.
    /// `stat(2)` returns both the apparent size (`st_size`) and the
    /// on-disk block count (`st_blocks` × 512 bytes).
    ///
    /// Defers path resolution to the policy module so the same path
    /// answer is shared by future consumers (cap-on-install logic in
    /// #1222 PR-2, etc.) without copy-pasting the path string.
    #[cfg(target_os = "macos")]
    fn probe_macos() -> Self {
        let path = match raw_image_path() {
            DockerRawPath::Resolved(p) => p,
            DockerRawPath::HomeUnset => {
                return Self::Unsupported {
                    os: "macos".to_string(),
                    reason: "$HOME env var not set; cannot resolve \
                             ~/Library/Containers/com.docker.docker path"
                        .to_string(),
                };
            }
            DockerRawPath::Unsupported(os) => {
                return Self::Unsupported {
                    os: os.to_string(),
                    reason: "paths::docker::raw_image_path returned Unsupported \
                             from macos branch — should be unreachable"
                        .to_string(),
                };
            }
        };
        let path_string = path.display().to_string();
        match std::fs::metadata(&path) {
            Ok(meta) => {
                // Unix: apparent size + real allocated blocks. Windows has no
                // st_blocks in std; len() serves both fields (honest apparent
                // size, no fake allocation number).
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    Self::Detected {
                        allocated_bytes: meta.size(),
                        used_bytes: meta.blocks() * 512,
                        path: path_string,
                    }
                }
                #[cfg(not(unix))]
                {
                    Self::Detected {
                        allocated_bytes: meta.len(),
                        used_bytes: meta.len(),
                        path: path_string,
                    }
                }
            }
            Err(err) => Self::NotFound {
                path: path_string,
                reason: err.to_string(),
            },
        }
    }

    /// Stub for non-macOS — never called because `probe` short-circuits
    /// to the OS-specific variants. Kept so the conditional-compile
    /// shape is explicit.
    #[cfg(not(target_os = "macos"))]
    fn probe_macos() -> Self {
        Self::Unsupported {
            os: std::env::consts::OS.to_string(),
            reason: "probe_macos() called on non-macos host".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: the probe should NEVER panic, regardless of
    /// host. If `Docker.raw` doesn't exist, it returns `NotFound`. If
    /// the OS isn't implemented, it returns `Unsupported`. Callers
    /// rely on this total-shape contract — a panic here would crash
    /// the resource manager on systems without Docker installed.
    #[test]
    fn probe_never_panics() {
        let _ = DockerTierProbe::probe();
    }

    /// What this catches: serde round-trip preserves the discriminant
    /// + payload fields. If `tag = "kind"` or `rename_all` drift, the
    /// TS side that reads `probe.kind` breaks. Same shape rule as
    /// AnalysisError (#1207) — typed errors at IPC boundaries.
    #[test]
    fn detected_variant_serde_round_trip() {
        let original = DockerTierProbe::Detected {
            allocated_bytes: 100 * 1024 * 1024 * 1024,
            used_bytes: 5 * 1024 * 1024 * 1024,
            path: "/Users/test/Library/.../Docker.raw".to_string(),
        };
        let json = serde_json::to_string(&original).unwrap();
        assert!(
            json.contains("\"kind\":\"detected\""),
            "expected kind=detected discriminant in {json}"
        );
        assert!(
            json.contains("\"allocatedBytes\":107374182400"),
            "expected camelCase allocatedBytes in {json}"
        );
        let round: DockerTierProbe = serde_json::from_str(&json).unwrap();
        match round {
            DockerTierProbe::Detected {
                allocated_bytes,
                used_bytes,
                ..
            } => {
                assert_eq!(allocated_bytes, 100 * 1024 * 1024 * 1024);
                assert_eq!(used_bytes, 5 * 1024 * 1024 * 1024);
            }
            other => panic!("round-trip changed variant: {other:?}"),
        }
    }

    /// What this catches: NotFound variant carries actionable
    /// diagnostics (the path it tried + a reason). If those drop out,
    /// debugging "why isn't continuum seeing my Docker?" becomes
    /// guesswork. Pin the contract.
    #[test]
    fn not_found_variant_carries_path_and_reason() {
        let v = DockerTierProbe::NotFound {
            path: "/nonexistent".to_string(),
            reason: "No such file".to_string(),
        };
        let json = serde_json::to_string(&v).unwrap();
        assert!(json.contains("\"kind\":\"notFound\""));
        assert!(json.contains("/nonexistent"));
        assert!(json.contains("No such file"));
    }

    /// What this catches: on macOS, when Docker IS installed, the
    /// probe returns Detected with non-zero allocated_bytes. This
    /// runs only on macOS; cfg-gated so other platforms don't fail.
    #[test]
    #[cfg(target_os = "macos")]
    fn macos_detects_or_reports_not_found() {
        // Either the test machine has Docker installed (Detected with
        // non-zero allocated) OR doesn't (NotFound with the expected
        // path). Both outcomes are valid — the test exists to assert
        // the macos branch returns one of those two, not Unsupported.
        match DockerTierProbe::probe() {
            DockerTierProbe::Detected {
                allocated_bytes,
                used_bytes,
                path,
            } => {
                assert!(allocated_bytes > 0, "allocated_bytes should be non-zero");
                assert!(
                    used_bytes <= allocated_bytes,
                    "used_bytes {used_bytes} should be <= allocated_bytes {allocated_bytes}"
                );
                assert!(
                    path.ends_with("Docker.raw"),
                    "path should end with Docker.raw: {path}"
                );
            }
            DockerTierProbe::NotFound { path, .. } => {
                assert!(
                    path.ends_with("Docker.raw"),
                    "NotFound path should still be the expected probe target: {path}"
                );
            }
            DockerTierProbe::Unsupported { .. } => {
                panic!("macos branch should never return Unsupported");
            }
        }
    }
}
