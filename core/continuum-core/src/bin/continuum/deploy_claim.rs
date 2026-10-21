//! The deploy claim/lease and repo-checkout bookkeeping shared by deploy verbs.

use super::*;

/// The continuum root (`~/.continuum`) — where the deploy claim lives.
pub(super) fn continuum_root() -> Result<PathBuf, String> {
    Ok(PathBuf::from(home_dir()?).join(".continuum"))
}

/// Consult the deploy claim before minting a core, and REPORT what it found.
///
/// Returns `Err` only while a deploy is genuinely in flight. See
/// [`continuum_cli_lifecycle::deploy_claim`] for the incident: mid-build there is no core
/// answering AND the installed image is still the PREVIOUS build, so any autostart in that
/// window launches stale code, wins the socket, and defeats the deploy.
///
/// A claim whose owner is gone or that has aged out is swept and announced rather than
/// silently obeyed — a claim must never be able to wedge the machine (#355's failure mode).
pub(super) fn deploy_gate(verb: &str) -> Result<(), String> {
    use continuum_cli_lifecycle::deploy_claim::{self, DeployGate};
    let Ok(root) = continuum_root() else {
        return Ok(()); // no HOME → no claim file → nothing to honour
    };
    let claim = deploy_claim::read(&root);
    let alive = claim.as_ref().is_some_and(|c| pid_alive(c.pid));
    let now = continuum_core::time::EpochMs::now().millis();
    match deploy_claim::with_wait(deploy_claim::decide(claim.as_ref(), alive, now), deploy_claim::read_waiting(&root).as_ref(), now) {
        // A deploy waiting on CI has swapped nothing: the installed core is the running build.
        DeployGate::Clear | DeployGate::Waiting { .. } => Ok(()),
        DeployGate::Abandoned { pid, age_ms, why } => {
            eprintln!(
                "⚠ sweeping an abandoned deploy claim (pid {pid}, {}s old, {why:?}) — \
                 proceeding with `{verb}`",
                age_ms / 1000
            );
            let _ = deploy_claim::clear(&root, pid);
            Ok(())
        }
        DeployGate::InProgress {
            pid,
            age_ms,
            target_sha,
        } => Err(format!(
            "a deploy is in flight (pid {pid} shipping build {target_sha}, {}s in) and no core \
             is answering yet. Starting one now would launch the PRE-SWAP installed binary, \
             which would then hold the socket and make the deploy report the OLD build — \
             measured 2026-08-17. Wait for the deploy to finish; `{verb}` works the moment it \
             does. (If that deploy is dead, its claim is swept automatically once its process \
             exits.)",
            age_ms / 1000
        )),
    }
}

/// The installer's existing Windows exclusion, held through the reboot handoff.
#[cfg(windows)]
pub(super) fn take_install_lease(root: &Path) -> Result<std::fs::File, String> {
    continuum_cli_lifecycle::supervisor_install::maintenance::take_install_lease(root)
}

/// RAII deploy claim: published for the length of a swap, released on EVERY exit path
/// (Ok, Err, `?`, panic-unwind). A claim that leaked past its deploy would block autostarts
/// until its owner died, so the release cannot be a line at the end of the happy path.
pub(super) struct DeployClaimGuard {
    pub(super) root: PathBuf,
    /// This process's pid, the claim's owner: the drop releases only a claim that is still ours.
    pub(super) pid: i32,
    /// Dropping this stops the renewer at once (its wait is a channel receive, not a sleep).
    pub(super) stop_renewing: Option<std::sync::mpsc::Sender<()>>,
    pub(super) renewer: Option<std::thread::JoinHandle<()>>,
}

impl DeployClaimGuard {
    /// Best-effort by design: if the claim cannot be written the deploy still proceeds —
    /// losing the guard degrades to the old behaviour (which `deploy-verify` still catches),
    /// whereas refusing to deploy over an unwritable advisory file turns a hint into an outage.
    ///
    /// The claim RENEWS while this guard lives (card 2b226917): a thread restamps it every
    /// `CLAIM_RENEW_EVERY_MS`, so a live deploy's claim never expires on age, however long
    /// the build takes on a slow node (275 min on the IntelMac, 2026-09-26). A thread, not a
    /// task: the warm build waits on a blocking `Command::status()` inside this async fn,
    /// and the renewal must not depend on which runtime worker that blocks.
    pub(super) fn take(target_sha: &str) -> Option<Self> {
        use continuum_cli_lifecycle::deploy_claim::{self, DeployClaim, CLAIM_RENEW_EVERY_MS};
        let root = continuum_root().ok()?;
        let pid = std::process::id() as i32;
        let started = continuum_core::time::EpochMs::now().millis();
        let claim = DeployClaim {
            pid,
            started_ms: started,
            target_sha: target_sha.to_string(),
            renewed_ms: started,
            progress_ms: started,
        };
        match deploy_claim::write(&root, &claim) {
            Ok(()) => {
                let (stop_renewing, stopped) = std::sync::mpsc::channel::<()>();
                let renew_root = root.clone();
                let renewer = std::thread::Builder::new()
                    .name("deploy-claim-renewer".into())
                    .spawn(move || {
                        // One System for the renewer's life: each refresh's cpu_usage is the
                        // average since the previous one, i.e. over the whole renewal interval.
                        let mut sys = sysinfo::System::new();
                        let _ = deploy_working(&mut sys, pid); // the baseline refresh
                        loop {
                            match stopped.recv_timeout(std::time::Duration::from_millis(CLAIM_RENEW_EVERY_MS)) {
                                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                                    let working = deploy_working(&mut sys, pid);
                                    if !deploy_claim::renew(&renew_root, pid, continuum_core::time::EpochMs::now().millis(), working) {
                                        return; // no longer ours (or unwritable): stop renewing
                                    }
                                }
                                // the guard dropped (Disconnected) or said stop
                                _ => return,
                            }
                        }
                    })
                    .ok();
                Some(Self { root, pid, stop_renewing: Some(stop_renewing), renewer })
            }
            Err(e) => {
                eprintln!(
                    "⚠ could not publish a deploy claim ({e}) — a concurrent command could \
                     autostart a stale core during this build; deploy-verify still catches it"
                );
                None
            }
        }
    }
}

/// Was the deploy working since the previous refresh of `sys`? Its process tree's CPU (the
/// owner plus every descendant, so the rustc under cargo under this reboot counts), or a
/// compiler busy anywhere on the host (an owner blocked on the shared cargo lock is waiting on
/// progress). Card 80ead731.
pub(super) fn deploy_working(sys: &mut sysinfo::System, pid: i32) -> bool {
    use continuum_cli_lifecycle::deploy_claim;
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate};
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_cpu());
    let procs: Vec<(i32, Option<i32>, f32)> = sys
        .processes()
        .values()
        .map(|p| (p.pid().as_u32() as i32, p.parent().map(|par| par.as_u32() as i32), p.cpu_usage()))
        .collect();
    let host_compilers: f32 = sys
        .processes()
        .values()
        .filter(|p| deploy_claim::is_compiler(&p.name().to_string_lossy()))
        .map(|p| p.cpu_usage())
        .sum();
    deploy_claim::deploy_working(deploy_claim::tree_cpu_percent(pid, &procs), host_compilers)
}

impl Drop for DeployClaimGuard {
    fn drop(&mut self) {
        // Stop the renewer FIRST, so it cannot restamp a claim this drop is clearing.
        drop(self.stop_renewing.take());
        if let Some(renewer) = self.renewer.take() {
            let _ = renewer.join();
        }
        let _ = continuum_cli_lifecycle::deploy_claim::clear_waiting(&self.root, self.pid);
        let _ = continuum_cli_lifecycle::deploy_claim::clear(&self.root, self.pid);
    }
}

/// The user's home dir — `HOME` (unix) or `USERPROFILE` (Windows). Loud when absent: the
/// resolution order depends on it, and guessing would defeat the shared contract.
pub(super) fn home_dir() -> Result<String, String> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| {
            "HOME/USERPROFILE unset — cannot resolve installed continuum-core-server locations \
             (set CONTINUUM_CORE_BIN explicitly)"
                .to_string()
        })
}

/// The image this CLI runs from, for `CONTINUUM_SKIP_SELF_BUILD`: the build script skips
/// the CLI only when this IS the file it would write (a locked running image). "1" when the
/// OS cannot say — the script's unconditional skip, never a build over a running image.
pub(super) fn running_cli_image() -> std::ffi::OsString {
    std::env::current_exe().map_or_else(|_| "1".into(), PathBuf::into_os_string)
}

/// Ask an on-disk `continuum-core-server` artifact for its embedded build SHA
/// (`--build-sha`, exits before any socket/side-effect). Loud on any failure — an artifact
/// that cannot state its provenance cannot anchor a deploy receipt.
pub(super) async fn binary_build_sha(artifact: &Path) -> Result<String, String> {
    let mut cmd = std::process::Command::new(artifact);
    apply_core_runtime_env(&mut cmd)?;
    cmd.arg("--build-sha").stdin(Stdio::null());
    let out = continuum_cli_lifecycle::prebuilt_validation::run(cmd, Duration::from_secs(30))
        .await
        .map_err(|e| format!("{} --build-sha: {e}", artifact.display()))?;
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if sha.is_empty() {
        return Err(format!(
            "{} --build-sha printed nothing — rebuild the artifact",
            artifact.display()
        ));
    }
    Ok(sha)
}

/// Short git HEAD SHA of the current checkout (matches what `build.rs` embeds), or `None`
/// when not in a git tree.
/// Record the cwd's repo checkout for the core (`modules::repo_registry`): the
/// claim-edge staging of a REPO card needs `owner/name → path`, and the core has
/// no cwd. Idempotent, silent outside a repo.
pub(super) fn record_repo_checkout() {
    let out = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
    };
    // `--show-toplevel` is the WORKTREE when run inside one (a card worktree,
    // deleted on merge — IntelMac's review of #3706); `--git-common-dir` is the
    // clone's .git in both cases, and absolute so the main clone does not
    // answer a relative `.git`.
    let (Some(url), Some(common)) = (
        out(&["remote", "get-url", "origin"]),
        out(&["rev-parse", "--path-format=absolute", "--git-common-dir"]),
    ) else {
        return;
    };
    let Some(root) = continuum_core::modules::repo_registry::clone_root_from_common_dir(&common)
    else {
        return;
    };
    if let Some(repo) = continuum_core::modules::repo_registry::repo_id_from_remote(&url) {
        continuum_core::modules::repo_registry::record(&repo, &root);
    }
}
