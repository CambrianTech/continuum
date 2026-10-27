//! `continuum start`: build + launch the headless core and wait until it answers.

use super::*;

/// `continuum start` — build + run the headless Rust core (detached), wait until it
/// answers `ping`. Idempotent: a no-op if a core is already up.
///
/// Reclaim-or-refuse, never blind-bind. The old guard was `core_is_up()` alone, so a
/// core that was RUNNING but not answering (wedged, mid-boot, bound where this CLI
/// cannot reach) was invisible and `start` launched a second one on top of it. That is
/// the same missing constraint `stop` got in #2287, on the other side of the lifecycle.
pub(super) async fn start(force: bool) -> Result<(), String> {
    let socket = socket_path();
    let decision = bind_decision().await;
    if matches!(decision, BindDecision::AlreadyServing { .. }) {
        println!("core already running (socket={socket})");
        return Ok(());
    }
    // Validate installed task selection before --force may reclaim anything.
    // A source override remains the explicit opt-out from installed deployment.
    let service = PreparedCoreService::for_start(&socket).await?;
    match decision {
        BindDecision::AlreadyServing { .. } => {
            println!("core already running (socket={socket})");
            return Ok(());
        }
        // Only the path that actually LAUNCHES consults the deploy claim. An already-serving
        // core is a no-op and must stay one — gating a no-op would turn a mid-deploy
        // `continuum start` into a spurious error about a core that is already fine.
        // `--force` is the documented override, consistent with the Occupied arm below.
        BindDecision::Free => {
            if !force {
                deploy_gate("start")?;
            }
        }
        BindDecision::Occupied { pids } => {
            let list = pids
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(",");
            if !force {
                return Err(format!(
                    "{} core process(es) are running (pid(s) {list}) but NONE is answering on \
                     {socket}. Refusing to start a second core: both would hold the same socket \
                     and whichever one the kernel hands a connection to would answer, which is \
                     how a shipped fix comes to look intermittently broken.\n  \
                     • still booting? wait — a healthy core answers shortly, then `continuum start` \
                     is a no-op\n  \
                     • wedged? `continuum stop` reaps every core, then start\n  \
                     • sure it is dead weight? `continuum start --force` reclaims it here{}",
                    pids.len(),
                    bound_elsewhere_hint(&running_core_sockets_for(&pids), &socket)
                        .unwrap_or_default()
                ));
            }
            // Explicit reclaim. `reboot` guards destructive restarts behind live-training
            // and live-benchmark leases; `start --force` deliberately carries no such
            // lease check, so say plainly what is being killed rather than implying a
            // vetted teardown.
            println!(
                "⚠ --force: reclaiming {} unresponsive core(s) (pid(s) {list}) — no training or \
                 benchmark lease is checked on this path; use `continuum reboot` if those matter",
                pids.len()
            );
            for pid in &pids {
                kill_pid_tree(*pid);
            }
            // Hand the reaped pids to launch_core as its death-wait set, so readiness is
            // only reported once the OLD cores are gone and the ping provably came from
            // the NEW one — the same honesty check reboot relies on.
            let secs = launch_installed_core(&pids, service).await?;
            println!("✅ core ready (socket={socket}) after ~{secs}s");
            return Ok(());
        }
    }

    let secs = launch_installed_core(&[], service).await?;
    println!("✅ core ready (socket={socket}) after ~{secs}s");
    Ok(())
}

pub(super) async fn launch_installed_core(
    old: &[i32],
    service: Option<(PreparedCoreService, PrebuiltCore)>,
) -> Result<u64, String> {
    match service {
        Some((service, candidate)) => {
            let secs = service.launch(old).await?;
            verify_deployed_build_against(false, Some(&candidate)).await?;
            Ok(secs)
        }
        None => launch_core(old, LaunchSource::Installed).await,
    }
}
