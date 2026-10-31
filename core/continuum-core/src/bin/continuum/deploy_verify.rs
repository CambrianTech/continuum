//! Post-deploy verification: the running core's provenance against the deployed source.

use super::*;

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
pub(super) const CLI_BUILD_SHA: &str = env!("CONTINUUM_BUILD_GIT_SHA");

/// The build a rollback put back, proven: the restored slot binary's embedded SHA must be
/// the one the answering core reports. `Ok(sha)` only when they match. Every platform: the
/// dark reboot's rollback (`dark_build::PreviousCore::restore`) proves what came back with it.
pub(super) async fn restored_build_identity(slot: &Path) -> Result<String, String> {
    let expected = binary_build_sha(slot).await?;
    let actual = running_core_build_sha("restore-verify").await?.unwrap_or_default(); // unwrap_or_default: an absent sha is the mismatch the check below names
    if sha_matches(&actual, &expected) {
        Ok(expected)
    } else {
        Err(format!("the slot holds {expected} but the answering core reports {actual:?}"))
    }
}

/// The RUNNING core's build SHA, from the process itself over `ping`, bounded at 30 s: a
/// core that accepts the socket mid-boot but never answers made `continuum reboot` hang
/// for good (IntelMac, 50 min, 2026-09-05). `what` names the caller in the error. The one
/// read both the deploy receipt and the rollback's identity check use.
pub(super) async fn running_core_build_sha(what: &str) -> Result<Option<String>, String> {
    let reply = running_core_provenance("ping", what).await?;
    Ok(reply.get("buildSha").and_then(|v| v.as_str()).map(str::to_string))
}

pub(super) async fn running_core_provenance(command: &str, what: &str) -> Result<Value, String> {
    running_core_call(command, Value::Object(Default::default()), what).await
}

/// [`running_core_provenance`] with `params`: the one bounded call to the running core.
pub(super) async fn running_core_call(command: &str, params: Value, what: &str) -> Result<Value, String> {
    let socket = socket_path();
    let reply = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        connection()
            .commands()
            .execute_value(command, params),
    )
    .await
    .map_err(|_| {
        format!(
            "{what}: the core on {socket} accepted the socket but did not answer `{command}` \
             within 30 s — mid-boot (retry) or wedged (read boot.phase / boot.module_init)"
        )
    })?
    .map_err(|e| format!("{what}: no core answering on {socket}: {e}"))?;
    Ok(reply)
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
pub(super) fn describe_running_core(socket: &str) -> String {
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
