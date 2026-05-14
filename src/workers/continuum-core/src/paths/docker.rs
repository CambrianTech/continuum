//! Docker Desktop path policy. Single source of truth for "where does
//! Docker put X on this OS?" questions.
//!
//! Today: just the macOS sparse-image path that `modules::docker_tier`
//! needs. Grows as #1222 / ResourcePool integration adds more
//! Docker-related path resolution (image cache root, settings.json
//! location, etc.).
//!
//! Why this lives in `paths::` and not `modules::docker_tier`: the
//! probe + the path are different concerns. The probe is "go ask the
//! filesystem about a known path"; the policy is "what IS the known
//! path on this OS." Separating them means the next consumer (e.g.
//! the cap-on-install logic in #1222 PR-2 that touches Docker
//! settings.json) doesn't have to import the probe module just to
//! know the path.

use std::path::PathBuf;

/// Result of asking "where is the Docker Desktop sparse disk image
/// on this host?" Total enum so callers handle every case
/// exhaustively (no silent fallback to a wrong-OS path).
#[derive(Debug, Clone)]
pub enum DockerRawPath {
    /// Path resolved successfully. May or may not exist on disk —
    /// the caller does the existence check (typically via stat(2)).
    Resolved(PathBuf),
    /// macOS-specific: `$HOME` env var was unset, so we can't resolve
    /// the path under `~/Library/...`. Distinct from "platform not
    /// supported" because macOS IS supported, the host is just
    /// misconfigured.
    HomeUnset,
    /// This OS isn't yet wired with a path policy. Carries the OS
    /// name so the caller can surface the right diagnostic.
    Unsupported(&'static str),
}

/// Resolve the Docker Desktop sparse-image path for the current OS.
///
/// - **macOS** — `$HOME/Library/Containers/com.docker.docker/Data/vms/0/data/Docker.raw`
///   (returns `HomeUnset` if `$HOME` isn't set, distinct from `Resolved` to a wrong path)
/// - **Windows / Linux / other** — `Unsupported` (PR-2/PR-3 of #1222 will wire these)
pub fn raw_image_path() -> DockerRawPath {
    if cfg!(target_os = "macos") {
        match std::env::var("HOME") {
            Ok(home) if !home.is_empty() => DockerRawPath::Resolved(
                PathBuf::from(home)
                    .join("Library/Containers/com.docker.docker/Data/vms/0/data/Docker.raw"),
            ),
            _ => DockerRawPath::HomeUnset,
        }
    } else if cfg!(target_os = "windows") {
        DockerRawPath::Unsupported("windows")
    } else if cfg!(target_os = "linux") {
        DockerRawPath::Unsupported("linux")
    } else {
        DockerRawPath::Unsupported(std::env::consts::OS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: the policy never panics regardless of host
    /// state. Callers (modules::docker_tier::probe) rely on this total
    /// shape; a panic here would crash the resource manager on hosts
    /// without `$HOME` set OR on un-supported OSes.
    #[test]
    fn raw_image_path_never_panics() {
        let _ = raw_image_path();
    }

    /// What this catches: on macOS WITH `$HOME` set (CI, dev, etc.)
    /// the policy returns `Resolved` ending in `Docker.raw`. Mutation
    /// that points the resolver at a different file (e.g. typo) would
    /// fail this assertion. cfg-gated to macOS so other platforms
    /// don't trip on the HOME assumption.
    #[test]
    #[cfg(target_os = "macos")]
    fn macos_with_home_resolves_to_docker_raw() {
        if std::env::var("HOME").map(|h| !h.is_empty()).unwrap_or(false) {
            match raw_image_path() {
                DockerRawPath::Resolved(p) => {
                    assert!(
                        p.to_string_lossy().ends_with("Docker.raw"),
                        "expected path to end with Docker.raw, got: {}",
                        p.display()
                    );
                    assert!(
                        p.to_string_lossy().contains("com.docker.docker"),
                        "expected path under com.docker.docker, got: {}",
                        p.display()
                    );
                }
                other => panic!("expected Resolved, got {other:?}"),
            }
        }
    }
}
