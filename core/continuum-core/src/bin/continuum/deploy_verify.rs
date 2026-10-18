//! #194 deploy provenance: prove the RUNNING core is built from what this deploy shipped,
//! from the process itself over the socket, never from a path guessed on disk. Owns the
//! expected-SHA ladder (prebuilt → git HEAD → resolved artifact) and the artifact
//! resolution order shared with `install-service.sh`.

use std::path::{Path, PathBuf};

use continuum_cli_lifecycle::deploy_provenance::{cli_staleness_note, deploy_verdict};
#[cfg(any(windows, target_os = "macos"))]
use continuum_cli_lifecycle::deploy_provenance::sha_matches;
use super::checkout::git_head_short_sha;
use super::client::{running_core_build_sha, socket_path};
use super::desktop::desktop_receipt_line;
use super::prebuilt::{binary_build_sha, PrebuiltCore};
use super::process::{running_core_pids, running_core_sockets_for};

/// Prove the running core is built from the source this deploy shipped — the honest half of
/// "reboot succeeded". The RUNNING core self-reports its compiled-in SHA over the socket
/// (`ping` → `buildSha`): the live process image answers for itself, so this can't be fooled
/// by re-exec'ing an on-disk file a rebuild already swapped under the still-running old core,
/// and it needs no path guessing at all (the 2026-08-01 Windows-node incident: a "next to the
/// CLI exe" guess printed "could not locate continuum-core-server" while a 2-day-old binary
/// kept serving — and the reboot still said success).
///
/// Expected SHA precedence:
/// 1. git HEAD of the checkout the reboot built from (start-server.sh's freshness guard
///    already pins artifact == source at build time), when run inside a git tree;
/// 2. otherwise the deployable artifact resolved by the SAME order install-service.sh uses
///    (`CONTINUUM_CORE_BIN` env → installed locations → cargo target dir), asked for its
///    `--build-sha` — the installed-node path, where there is no git tree.
///
/// NEVER skips soft: any gap (no core answering, a pre-#194 core without `buildSha`,
/// `unknown` provenance, no resolvable artifact) is an ERROR — a reboot must not print a
/// success line it cannot back with provenance ([[fallbacks-are-illegal-fail-loud]]).
/// This CLI's own build SHA, stamped at compile time by `build.rs` for the whole crate —
/// the `continuum` bin lives in `continuum-core`, so it gets the same constant the core does,
/// describing the binary you are RUNNING rather than one found on disk.
const CLI_BUILD_SHA: &str = env!("CONTINUUM_BUILD_GIT_SHA");

/// The build a rollback put back, proven: the restored slot binary's embedded SHA must be
/// the one the answering core reports. `Ok(sha)` only when they match.
#[cfg(any(windows, target_os = "macos"))]
pub(super) async fn restored_build_identity(slot: &Path) -> Result<String, String> {
    let expected = binary_build_sha(slot).await?;
    let actual = running_core_build_sha("restore-verify").await?.unwrap_or_default(); // unwrap_or_default: an absent sha is the mismatch the check below names
    if sha_matches(&actual, &expected) {
        Ok(expected)
    } else {
        Err(format!("the slot holds {expected} but the answering core reports {actual:?}"))
    }
}

/// `rebuilt_cli` says whether THIS invocation replaced the installed CLI — true from
/// `reboot` (start-server.sh rebuilds + reinstalls it unless `cli_self_build` skips the
/// platform), false from a bare `deploy-verify`. It is what lets the CLI-provenance note
/// tell a HANDOFF ("the next run gets the new CLI") apart from real STALENESS, instead of
/// warning on every successful deploy.
pub(super) async fn verify_deployed_build(rebuilt_cli: bool) -> Result<(), String> {
    verify_deployed_build_against(rebuilt_cli, None).await
}

pub(super) async fn verify_deployed_build_against(
    rebuilt_cli: bool,
    prebuilt: Option<&PrebuiltCore>,
) -> Result<(), String> {
    let socket = socket_path();
    // The RUNNING core's provenance, from the process itself (bounded; see the helper).
    let actual = running_core_build_sha("deploy-verify").await?;

    // What the deploy SHOULD have shipped.
    let (expected, expected_source) = match prebuilt {
        Some(candidate) => (
            candidate.build_sha.clone(),
            format!(
                "prebuilt artifact {} (no rebuild)",
                candidate.path.display()
            ),
        ),
        None => match git_head_short_sha() {
            Some(head) => (head, "git HEAD of this checkout".to_string()),
            None => {
                let artifact = resolve_core_artifact()?;
                let sha = binary_build_sha(&artifact).await?;
                (sha, format!("artifact {}", artifact.display()))
            }
        },
    };

    let running_desc = describe_running_core(&socket);
    // The CLI's own provenance rides alongside the core's, on BOTH outcomes: a stale CLI
    // is relevant whether or not the core swap took.
    let cli_note = cli_staleness_note(CLI_BUILD_SHA, &expected, &expected_source, rebuilt_cli);
    match deploy_verdict(
        actual.as_deref(),
        &expected,
        &expected_source,
        &running_desc,
    ) {
        Ok(line) => {
            println!("{line}");
            if let Some(note) = cli_note {
                println!("{note}");
            }
            // The last line of a verified deploy is WHERE to look.
            println!("{}", desktop_receipt_line().await);
            Ok(())
        }
        Err(e) => Err(match cli_note {
            Some(note) => format!("{e}\n{note}"),
            None => e,
        }),
    }
}

/// Best-effort human identity of the running core for error messages: socket + pid(s) +
/// process image path where resolvable. Diagnostics ONLY — the SHA itself always comes from
/// the process over the socket, never from re-executing a path guessed here.
///
/// `socket=` is the socket THIS CLI resolved. When a listed core is provably bound
/// somewhere else, that is said too rather than left for the reader to assume the two
/// agree — this is the third site (with the two `Occupied` refusals) where a client-side
/// path was being printed as though it were the core's.
fn describe_running_core(socket: &str) -> String {
    let pids = running_core_pids();
    let mut desc = format!("socket={socket}");
    if !pids.is_empty() {
        desc.push_str(&format!(
            ", pid(s) {}",
            pids.iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(",")
        ));
        let elsewhere: Vec<String> = running_core_sockets_for(&pids)
            .into_iter()
            .filter(|(_, s)| s != socket)
            .map(|(pid, s)| format!("pid {pid} bound to {s}"))
            .collect();
        if !elsewhere.is_empty() {
            desc.push_str(&format!(" — NOT this socket: {}", elsewhere.join(", ")));
        }
    }
    if let Some(img) = running_core_binary() {
        desc.push_str(&format!(", image {}", img.display()));
    }
    desc
}

/// Resolve the deployable `continuum-core-server` artifact — the SAME order
/// `tools/scripts/install-service.sh::resolve_core_bin` uses, centralized so the CLI and the
/// service installer can never disagree: `CONTINUUM_CORE_BIN` env → installed locations
/// (`/usr/local/bin`, `~/.continuum/bin`) → cargo target dir (release, then debug; default
/// target dir `~/.continuum/cache/cargo-target`, matching start-server.sh). NEVER "next to
/// the CLI exe" — that guess is what printed "could not locate continuum-core-server" on the
/// 2026-08-01 Windows node while a stale core kept serving.
pub(super) fn resolve_core_artifact() -> Result<PathBuf, String> {
    let env_bin = std::env::var("CONTINUUM_CORE_BIN").ok();
    if let Some(explicit) = &env_bin {
        let p = PathBuf::from(explicit);
        if p.is_file() {
            return Ok(p);
        }
        return Err(format!("CONTINUUM_CORE_BIN={explicit} is not a file"));
    }
    let target = continuum_core::paths::cargo::target_dir(&std::env::current_dir().map_err(|error| error.to_string())?)?;
    let payload = continuum_core::paths::payload_root(&continuum_core::paths::continuum_home()?)?;
    let candidates = core_artifact_candidates(&target, &payload);
    candidates
        .iter()
        .find(|p| p.is_file())
        .cloned()
        .ok_or_else(|| {
            format!(
                "no continuum-core-server artifact found. Searched (install-service.sh order): {}. \
                 Build one (`continuum reboot` from the repo, or npm start) or set CONTINUUM_CORE_BIN.",
                candidates
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

/// The ordered candidate list behind [`resolve_core_artifact`] — pure (paths in, paths out)
/// so the shared resolution ORDER is pinned by a unit test against install-service.sh.
pub(super) fn core_artifact_candidates(target: &Path, payload: &Path) -> Vec<PathBuf> {
    let exe = if cfg!(windows) {
        "continuum-core-server.exe"
    } else {
        "continuum-core-server"
    };
    let mut out = Vec::new();
    if !cfg!(windows) {
        out.push(PathBuf::from("/usr/local/bin").join(exe));
    }
    out.push(payload.join("bin").join(exe));
    out.push(target.join("release").join(exe));
    out.push(target.join("debug").join(exe));
    out
}

/// Path to the executable image of the ACTUALLY-RUNNING core, resolved from its live pid.
/// DIAGNOSTICS ONLY (error-message context via [`describe_running_core`]): the deploy receipt
/// itself never re-execs this path — after a rebuild the on-disk file at the running pid's
/// path is already the NEW binary while the old image keeps serving, so exec'ing it
/// false-passes a stale deploy (the inverse of the 2026-07-25 release-leftover false alarm).
/// The running core's SHA always comes from the process itself over the socket (`ping`).
fn running_core_binary() -> Option<std::path::PathBuf> {
    let pid = running_core_pids().into_iter().next()?;
    #[cfg(target_os = "linux")]
    {
        std::fs::read_link(format!("/proc/{pid}/exe")).ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        // macOS/BSD have no /proc; `ps -o comm=` prints the full executable path.
        let out = std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .output()
            .ok()
            .filter(|o| o.status.success())?;
        let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!path.is_empty()).then(|| std::path::PathBuf::from(path))
    }
}
