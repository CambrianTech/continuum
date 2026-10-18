//! `continuum install`: ONE idempotent verb converges the machine — supervisor, core,
//! CLI on PATH, airc — each arm reads, changes only what drifted, says so. With the
//! macOS `uninstall` and `supervisor-status --crash-test` receipts.

use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(target_os = "macos")]
use continuum_cli_lifecycle::launchd;
#[cfg(windows)]
use continuum_cli_lifecycle::install_cli;
use continuum_cli_lifecycle::supervisor_install;

use super::checkout::{git_in, home_dir};
#[cfg(any(windows, target_os = "macos"))]
use super::checkout::tracked_repo_dir;
use super::client::{core_is_up, socket_path};
#[cfg(any(windows, target_os = "macos"))]
use super::client::running_build_sha;
#[cfg(target_os = "macos")]
use super::deploy_verify::resolve_core_artifact;
#[cfg(windows)]
use super::prebuilt::{binary_build_sha, PrebuiltCore};
#[cfg(any(windows, target_os = "macos"))]
use super::reboot::{reboot, RebootOptions};
#[cfg(target_os = "macos")]
use super::runtime_env::{apply_runtime_library_path, direct_core_command};
#[cfg(windows)]
use super::service::{CoreServiceDescription, PreparedCoreService};
#[cfg(target_os = "macos")]
use super::stop::stop_with;

// =============================================================================
// install — converge this machine to the contract, from the binary (Joel 2026-09-18:
// "What's the repeatable process or inside an install? … Do NOT hand jack"; "it
// needs to be in our own binary here"). Arms land one at a time; each is a module
// under `continuum/`. First: the OS supervisor (`supervisor_install`).
// =============================================================================

pub(super) async fn install(options: supervisor_install::InstallOptions) -> Result<(), String> {
    #[cfg(any(windows, target_os = "macos"))]
    use supervisor_install::{Arm, ArmReport};
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = options;
        return Err(format!(
            "continuum install: this platform's arms are not in the binary yet — \
             the Linux arm (`systemd --user` + linger) is owed; Windows and macOS are here. \
             Until it lands: tools/scripts/install-service.sh ({} arm).",
            std::env::consts::OS
        ));
    }
    #[cfg(target_os = "macos")]
    {
        if let (true, Some(plan), Some(sha)) = (options.elevated, options.plan.as_deref(), options.plan_sha.as_deref()) {
            for line in launchd::live::register_elevated(plan, sha)? {
                println!("{line}");
            }
            return Ok(());
        }
        let check = options.check;
        let mut reports: Vec<(Arm, ArmReport)> = Vec::new();
        let mut failed: Vec<(Arm, String)> = Vec::new();
        let mut take = |arm: Arm, r: Result<ArmReport, String>| match r {
            Ok(report) => reports.push((arm, report)),
            Err(why) => {
                println!("✗ {}: {why}", arm.name());
                failed.push((arm, why));
            }
        };
        if options.runs(Arm::Supervisor) {
            take(Arm::Supervisor, install_supervisor_macos(check, options.user).await);
        }
        if options.runs(Arm::Core) {
            take(Arm::Core, install_core(check).await);
        }
        if options.runs(Arm::Airc) {
            take(Arm::Airc, install_airc(check).await);
        }
        if options.runs(Arm::Cli) {
            // The macOS CLI arm (PATH copies follow the slot's CLI) is owed: on this OS the
            // slot carries no CLI descriptor yet. Said, never counted as converged.
            if options.cli {
                return Err("install --cli: the macOS CLI arm is not in the binary yet (the slot carries no CLI descriptor on this OS)".to_string());
            }
            println!("  cli: the macOS arm is owed — skipped, not counted");
        }
        return finish_install(check, &reports, &failed);
    }
    #[cfg(windows)]
    {
        if let (true, Some(plan), Some(sha)) = (options.elevated, options.plan.as_deref(), options.plan_sha.as_deref()) {
            return supervisor_install::install_supervisor_elevated(plan, sha);
        }
        if options.user {
            return Err("install --user is the macOS agent choice; Windows registers the S4U task".to_string());
        }
        let check = options.check;
        // Every arm runs, whatever the one before it did: an arm that cannot do its
        // job is REPORTED as one remaining drift (with its reason) and the next arm
        // still runs. 2026-09-19, Joel's first run: the core arm's error ended the
        // verb before the CLI arm ever ran — the supervisor was converged, the CLI on
        // PATH stayed stale, and the line said "run from the repository".
        let mut reports: Vec<(Arm, ArmReport)> = Vec::new();
        let mut failed: Vec<(Arm, String)> = Vec::new();
        let mut take = |arm: Arm, r: Result<ArmReport, String>| match r {
            Ok(report) => reports.push((arm, report)),
            Err(why) => {
                println!("✗ {}: {why}", arm.name());
                failed.push((arm, why));
            }
        };
        // 1. The supervisor: a core can only be handed to a prepared one.
        if options.runs(Arm::Supervisor) {
            take(Arm::Supervisor, supervisor_install::install_supervisor(check, installed_cli_from_descriptor).await);
        }
        // 2. The core: the running build is the tracked checkout's HEAD, or it is built,
        //    staged into the slot and handed to the supervisor — `reboot --service`, the
        //    same path the unattended consumer takes. Hand-run, the tip is what you have.
        if options.runs(Arm::Core) {
            take(Arm::Core, install_core(check).await);
        }
        // 3. The CLI on PATH follows the slot's CLI (fresh after a stage).
        if options.runs(Arm::Cli) {
            take(Arm::Cli, install_cli(check).await);
        }
        // 4. The mesh: airc installed and started at login.
        if options.runs(Arm::Airc) {
            take(Arm::Airc, install_airc(check).await);
        }
        finish_install(check, &reports, &failed)
    }
}

/// The orchestrator's verdict, one for every platform: drift summed across the arms
/// that ran, exit non-zero only on what remains; twice = "nothing changed".
#[cfg(any(windows, target_os = "macos"))]
fn finish_install(
    check: bool,
    reports: &[(supervisor_install::Arm, supervisor_install::ArmReport)],
    failed: &[(supervisor_install::Arm, String)],
) -> Result<(), String> {
    // An arm that could not run is one remaining drift with its reason (#4236) — the
    // verb's exit is the sum over every arm, never the first error.
    let remaining: usize = reports.iter().map(|(_, r)| r.drift_after).sum::<usize>() + failed.len();
    let found: usize = reports.iter().map(|(_, r)| r.drift_before).sum::<usize>() + failed.len();
    let ran = reports.len() + failed.len();
    if remaining > 0 {
        let could_not = if failed.is_empty() {
            String::new()
        } else {
            format!(
                " ({} arm(s) could not run: {})",
                failed.len(),
                failed.iter().map(|(a, _)| a.name()).collect::<Vec<_>>().join(", ")
            )
        };
        return Err(if check {
            format!("install --check: {remaining} way(s) drifted across {ran} arm(s){could_not}; `continuum install` converges them")
        } else {
            format!("install: {remaining} way(s) still drifted after running {ran} arm(s){could_not} — read the lines above")
        });
    }
    println!(
        "✓ install: {ran} arm(s) converged{}",
        if found == 0 { " — nothing changed" } else { "" }
    );
    Ok(())
}

/// The installed CLI, read out of the ContinuumCore task's description (the release
/// descriptor) — the slot's single source.
#[cfg(windows)]
fn installed_cli_from_descriptor(description: &str) -> Result<String, String> {
    let d: CoreServiceDescription = serde_json::from_str(description).map_err(|e| {
        format!("ContinuumCore is not prepared by the current installer (its description is not a release descriptor): {e}; rerun the installer")
    })?;
    Ok(d.cli)
}

/// The core arm: running build vs the TRACKED checkout's HEAD. The checkout is the
/// one the consumer deploys from (`CONTINUUM_TRACK_REPO_DIR`, else the source tree
/// this binary was built from) — never the current directory: a user types
/// `uu install` from wherever they are (Joel, from his home dir, 2026-09-19:
/// "no checkout HEAD to converge to — run from the repository" was the wrong answer).
#[cfg(any(windows, target_os = "macos"))]
async fn install_core(check: bool) -> Result<supervisor_install::ArmReport, String> {
    use supervisor_install::ArmReport;
    let repo = tracked_repo_dir().map_err(|e| format!("{e} — `uu install` converges the tracked checkout, from any directory"))?;
    let head = git_in(&repo, &["rev-parse", "--short", "HEAD"])?;
    println!("  core: checkout {} at {head}", repo.display());
    // RECORD the checkout as the fact every owner reads (card 790c6bcb): the running core
    // built from this install must track this clone, not the dir it was compiled in. Read
    // mode records nothing; a converged write records the same value it already reads.
    if !check {
        let recorded = continuum_core::config_env::read(continuum_core::runtime::tracked_checkout::TRACK_REPO_DIR_KEY);
        if recorded.as_deref() != Some(&*repo.to_string_lossy()) {
            continuum_core::runtime::tracked_checkout::record(&repo)?;
            println!("  core: recorded {} as the tracked checkout (config.env)", repo.display());
        }
    }
    // The handoff path (`reboot --service`) locates its script and registers the
    // checkout from the working directory, as the consumer does before it.
    std::env::set_current_dir(&repo).map_err(|e| format!("install: cannot enter {}: {e}", repo.display()))?;
    let running = running_build_sha().await;
    #[cfg(windows)]
    let engine_drift = PreparedCoreService::engine_drift(&repo).await?;
    #[cfg(not(windows))]
    let engine_drift = String::new();
    if !engine_drift.is_empty() {
        println!("  engine: {engine_drift}");
    }
    #[cfg(windows)]
    let browser_drift = PreparedCoreService::browser_release(&repo, true).await?;
    #[cfg(not(windows))]
    let browser_drift = String::new();
    if !browser_drift.is_empty() {
        println!("  browser release: {browser_drift}");
    }
    match running.as_deref() {
        Some(r)
            if continuum_cli_lifecycle::deploy_tracker::same_commit(r, &head)
                && engine_drift.is_empty() && browser_drift.is_empty() =>
        {
            println!("✓ core: converged — running build {r} is HEAD");
            return Ok(ArmReport::converged());
        }
        Some(r) => println!("  core: running build {r}, HEAD is {head}"),
        None => println!("  core: no core answering; HEAD is {head}"),
    }
    if check {
        println!("✗ core: drifted; `continuum install` builds HEAD, stages it and hands it to the supervisor");
        return Ok(ArmReport::read_only(1));
    }
    // Windows hands the built core to the prepared task. macOS `reboot` implies the
    // supervisor when a launchd job exists (stage + kickstart) and builds direct when
    // none does — the same idempotent answer either way.
    // `install` is a command a human typed, so it may spend ONE consent to borrow the
    // teardown privilege when this caller has none over the running core — the whole
    // reason the node in front of us cannot be replaced. Same command every time; the
    // escalation is inside it, never a second verb the user has to discover.
    #[cfg(windows)]
    let prepared = prepared_install_core(&repo, &head).await?;
    #[cfg(windows)]
    let stage_prebuilt = prepared.is_some();
    #[cfg(not(windows))]
    let stage_prebuilt = false;
    #[cfg(windows)]
    let prebuilt = if prepared.is_some() {
        prepared
    } else if running
        .as_deref()
        .is_some_and(|r| continuum_cli_lifecycle::deploy_tracker::same_commit(r, &head))
    {
        let task = PreparedCoreService::query().await?;
        let release: CoreServiceDescription =
            serde_json::from_str(&task.description).map_err(|e| e.to_string())?;
        // Reuse the installed core only when the slot's CLI is from the same commit. A CLI
        // that is older, or cannot say what it is, is not a reason to stop: it is drift the
        // build path below converges (it rebuilds the CLI and stages the pair together).
        match binary_build_sha(Path::new(&release.cli)).await {
            Ok(cli_sha) if continuum_cli_lifecycle::deploy_tracker::same_commit(&cli_sha, &head) => {
                Some(PathBuf::from(release.artifact))
            }
            Ok(cli_sha) => {
                println!("  core: the slot's CLI is build {cli_sha}, HEAD is {head} — rebuilding the pair");
                None
            }
            Err(e) => {
                println!("  core: the slot's CLI cannot state its build ({e}) — rebuilding the pair");
                None
            }
        }
    } else {
        None
    };
    #[cfg(not(windows))]
    let prebuilt = None;
    let options = RebootOptions {
        service: cfg!(windows),
        operator_present: true,
        stage_prebuilt,
        prebuilt,
        ..Default::default()
    };
    #[cfg(windows)]
    let options = RebootOptions {
        require_engine_receipt: true,
        ..options
    };
    reboot(options).await.map_err(|error| error.to_string())?;
    #[cfg(windows)]
    {
        let drift = PreparedCoreService::engine_drift(&repo).await?;
        if !drift.is_empty() {
            return Err(format!("install: engine handoff did not converge: {drift}"));
        }
    }
    let now = running_build_sha().await;
    match now.as_deref() {
        Some(r) if continuum_cli_lifecycle::deploy_tracker::same_commit(r, &head) => {
            println!("✓ core: converged — running build {r} is HEAD");
            Ok(ArmReport { drift_before: 1, drift_after: 0 })
        }
        other => Err(format!(
            "install: the handoff ran but the running core reports {} against HEAD {head}",
            other.unwrap_or("nothing") // unwrap_or: None = no core answering, reported as such — never a sha
        )),
    }
}

/// An install retry can reuse the verified pair in the configured Cargo release directory. Never
/// discover an arbitrary installed binary or silently replace uncommitted work.
#[cfg(windows)]
async fn prepared_install_core(repo: &Path, head: &str) -> Result<Option<PathBuf>, String> {
    if !git_in(repo, &["status", "--porcelain", "--untracked-files=normal"])?.is_empty() {
        return Ok(None);
    }
    let target = continuum_core::paths::cargo::target_dir(repo)?;
    let cli = target.join("release/continuum.exe");
    let Ok(cli_sha) = binary_build_sha(&cli).await else { return Ok(None) };
    if !continuum_cli_lifecycle::deploy_tracker::same_commit(&cli_sha, head) {
        return Ok(None);
    }
    let Some(dir) = cli.parent() else { return Ok(None) };
    let artifact = dir.join("continuum-core-server.exe");
    // A missing/stale candidate falls back to the normal build. The existing
    // reboot path revalidates the selected artifact and owns claim/staging/drain.
    match PrebuiltCore::prepare(&artifact).await {
        Ok(candidate) => {
            println!("  core: reusing prepared build {} beside {}", candidate.build_sha, cli.display());
            Ok(Some(candidate.path))
        }
        Err(_) => Ok(None),
    }
}

/// The CLI arm: `~/.local/bin/{continuum,uu}.exe` are the slot's CLI, and the dir is
/// on the user's PATH.
#[cfg(windows)]
pub(super) async fn install_cli(check: bool) -> Result<supervisor_install::ArmReport, String> {
    use install_cli::CliDrift;
    use supervisor_install::ArmReport;
    let core = supervisor_install::task_report(supervisor_install::CORE_TASK).await?;
    if !core.present {
        println!("  cli: no installed release (no ContinuumCore task) — nothing to follow yet");
        return Ok(ArmReport::read_only(1));
    }
    let selected = PreparedCoreService::query().await?;
    let slot_cli = PathBuf::from(installed_cli_from_descriptor(&selected.description)?);
    let dir = install_cli::cli_dir(Path::new(&home_dir()?));
    let user_path = supervisor_install::powershell(
        "[Environment]::GetEnvironmentVariable('PATH','User')",
        Duration::from_secs(30),
    )
    .await?;
    let drift = install_cli::cli_drift(&slot_cli, &dir, &user_path)?;
    if drift.is_empty() {
        println!("✓ cli: converged — {} and uu on PATH are the slot's CLI ({})", dir.join("continuum.exe").display(), slot_cli.display());
        return Ok(ArmReport::converged());
    }
    for d in &drift {
        println!("  cli: {d:?}");
    }
    if check {
        println!("✗ cli: drifted; `continuum install` refreshes the copies and the user PATH");
        return Ok(ArmReport::read_only(drift.len()));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("install: cannot create {}: {e}", dir.display()))?;
    for d in &drift {
        match d {
            CliDrift::Missing(name) | CliDrift::Stale(name) => {
                let to = dir.join(install_cli::cli_file_name(name));
                if name == "uu" {
                    install_cli::hard_link_with_retry(&dir.join(install_cli::cli_file_name("continuum")), &to, Duration::from_secs(10))?;
                } else {
                    install_cli::copy_with_retry(&slot_cli, &to, Duration::from_secs(10))?;
                }
                println!("  cli: refreshed {}", to.display());
            }
            CliDrift::DuplicateAlias => {
                let to = dir.join(install_cli::cli_file_name("uu"));
                install_cli::hard_link_with_retry(&dir.join(install_cli::cli_file_name("continuum")), &to, Duration::from_secs(10))?;
                println!("  cli: linked {} to continuum.exe", to.display());
            }
            CliDrift::NotOnPath(dir) => {
                let script = format!(
                    "$d='{}'; $p=[Environment]::GetEnvironmentVariable('PATH','User'); if (-not $p) {{ $p='' }}; [Environment]::SetEnvironmentVariable('PATH', ($d + ';' + $p).TrimEnd(';'), 'User')",
                    dir.display().to_string().replace('\'', "''")
                );
                supervisor_install::powershell(&script, Duration::from_secs(30)).await?;
                println!("  cli: added {} to the user PATH (new terminals see it)", dir.display());
            }
        }
    }
    let after = install_cli::cli_drift(
        &slot_cli,
        &dir,
        &supervisor_install::powershell("[Environment]::GetEnvironmentVariable('PATH','User')", Duration::from_secs(30)).await?,
    )?;
    if !after.is_empty() {
        return Err(format!("install: the CLI still drifts after the refresh: {after:?}"));
    }
    println!("✓ cli: converged — continuum and uu on PATH are the slot's CLI");
    Ok(ArmReport { drift_before: drift.len(), drift_after: 0 })
}

/// `continuum supervisor-status [--crash-test]` — the supervision receipt as a verb
/// (a1bd8b58; Joel 2026-09-19: "careful methodical TDD and vdd" — every change ships its
/// receipt as a self-running check, not a hand test). Reads the launchd job, its pid, the
/// core's pid and what launchd's log said about the domain, and prints ONE of the four
/// verdicts in [`launchd::SupervisionVerdict`]. Exit 0 only for SUPERVISED.
///
/// `--crash-test` is the receipt itself: `kill -9` the core and time launchd's relaunch,
/// refusing success unless the core that answers is the job's own pid. It is refused when
/// no job is registered — killing an unsupervised core is not a test, it is an outage.
pub(super) async fn supervisor_status(crash_test: bool) -> Result<(), String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = crash_test;
        Err("supervisor-status reads launchd; on Windows the ContinuumCore task is the supervisor (card 7b56a84b)".to_string())
    }
    #[cfg(target_os = "macos")]
    {
        use launchd::{live, supervision_verdict};
        let socket = socket_path();
        let core_pid = || live::serving_core_pid(&socket);
        let job = live::job()?;
        let job_pid = job.as_ref().and_then(|j| live::job_pid(&j.domain));
        let on_demand = live::domain_on_demand_only_recently(Duration::from_secs(15 * 60));
        let verdict: launchd::SupervisionVerdict = supervision_verdict(job.as_ref().map(|j| j.domain.clone()), job_pid, core_pid(), on_demand);
        eprintln!("{}", verdict.line());
        println!(
            "{}",
            serde_json::json!({
                "domain": job.as_ref().map(|j| j.domain.target()),
                "slot": job.as_ref().map(|j| j.slot.display().to_string()),
                "job_pid": job_pid,
                "core_pid": core_pid(),
                "domain_on_demand_only_last_15m": on_demand,
                "verdict": format!("{verdict:?}"),
                "healthy": verdict.is_healthy(),
            })
        );
        if !crash_test {
            return if verdict.is_healthy() { Ok(()) } else { Err(verdict.line()) };
        }
        let Some(job) = job else {
            return Err("--crash-test refused: no launchd job is registered — killing an unsupervised core is an outage, not a test".to_string());
        };
        let Some(victim) = core_pid() else {
            return Err("--crash-test refused: no core is answering to crash".to_string());
        };
        if job_pid != Some(victim) {
            return Err(format!(
                "--crash-test refused: launchd's pid {job_pid:?} is not the core {victim}; the core is an orphan and would not come back"
            ));
        }
        eprintln!("▶ crash test: kill -9 {victim}; waiting for {} to relaunch it", job.domain.target());
        let runs_before = live::spawn_runs(&job.domain);
        // SAFETY: a plain signal to a pid this process just read as the supervised core.
        let rc = unsafe { libc::kill(victim as i32, libc::SIGKILL) };
        if rc != 0 {
            return Err(format!("kill -9 {victim} failed: {}", std::io::Error::last_os_error()));
        }
        let t0 = std::time::Instant::now();
        let fresh = move || core_pid().filter(|p| *p != victim);
        match live::wait_owned(&job, runs_before, fresh, core_is_up, Duration::from_secs(120)).await {
            Ok(pid) => {
                let secs = t0.elapsed().as_secs();
                eprintln!("✅ healed: {} relaunched the core (pid {pid}) in {secs}s", job.domain.target());
                println!("{}", serde_json::json!({ "crash_test": "healed", "seconds": secs, "pid": pid }));
                if secs > 60 {
                    return Err(format!("healed, but in {secs}s — the receipt is 60 s"));
                }
                Ok(())
            }
            Err(why) => {
                let again = live::domain_on_demand_only_recently(Duration::from_secs(5 * 60));
                Err(format!(
                    "NOT HEALED after {}s: {why}{}",
                    t0.elapsed().as_secs(),
                    if again { " — launchd: \"pending spawn, domain in on-demand-only mode\"; install the system LaunchDaemon" } else { "" }
                ))
            }
        }
    }
}

/// The airc arm: continuum makes sure the mesh it talks through is installed and comes
/// back after a reboot. airc stays its own product: installed by its own installer,
/// and its login supervisor judged and repaired by its own registrar
/// (`unix/register-autostart.sh --check`, `windows/register-autostart.ps1 -Check`).
/// This arm only invokes them; it never reads airc's task or plist itself.
#[cfg(any(windows, target_os = "macos"))]
async fn install_airc(check: bool) -> Result<supervisor_install::ArmReport, String> {
    use supervisor_install::{airc_drift, AircDrift, ArmReport};
    let read = || {
        let airc = airc_on_path();
        let supervisor = airc.as_deref().and_then(|a| airc_registrar(a, RegistrarMode::Check).err());
        let runs = airc.is_some();
        (airc, airc_drift(runs, supervisor))
    };
    let (mut airc, drift) = read();
    if drift.is_empty() {
        println!("✓ airc: installed; its registrar reports the login supervisor converged");
        return Ok(ArmReport::converged());
    }
    for d in &drift {
        println!("  airc: {d:?}");
    }
    if check {
        println!("✗ airc: drifted; `continuum install` installs airc and has airc's registrar repair its supervisor");
        return Ok(ArmReport::read_only(drift.len()));
    }
    if drift.contains(&AircDrift::Missing) {
        println!("▶ airc: installing through airc's own installer");
        continuum_core::airc::discovery::install_airc()
            .await
            .map_err(|e| format!("airc install: {e}"))?;
        airc = airc_on_path();
    }
    let airc = airc.ok_or("airc installed but is still not runnable on PATH; open a new shell or check ~/.local/bin")?;
    if airc_registrar(&airc, RegistrarMode::Check).is_err() {
        airc_registrar(&airc, RegistrarMode::Repair)?;
    }
    let (_, after) = read();
    if !after.is_empty() {
        return Err(format!("airc still drifted after its registrar ran: {after:?}"));
    }
    println!("✓ airc: installed; its registrar reports the login supervisor converged ({})", airc.display());
    Ok(ArmReport { drift_before: drift.len(), drift_after: 0 })
}

/// The `airc` a login shell would run, if it runs at all (`--version` answers).
#[cfg(any(windows, target_os = "macos"))]
fn airc_on_path() -> Option<PathBuf> {
    use supervisor_install::quiet_command;
    let finder = if cfg!(windows) { "where" } else { "which" };
    let out = quiet_command(finder).arg("airc").output().ok()?;
    let found = String::from_utf8_lossy(&out.stdout).lines().next().map(|l| PathBuf::from(l.trim()))?;
    let runs = out.status.success()
        && quiet_command(&found)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
    runs.then_some(found)
}

#[cfg(any(windows, target_os = "macos"))]
#[derive(Clone, Copy, PartialEq, Eq)]
enum RegistrarMode {
    /// Read only: `--check` / `-Check`. Never elevates.
    Check,
    /// Register or repair; on Windows the registrar asks for elevation only if the
    /// existing task needs it, through its own consent path.
    Repair,
}

/// Run airc's own supervisor registrar from the checkout airc was installed from.
/// `Err` carries the registrar's own words (the drift, or why it could not run).
#[cfg(any(windows, target_os = "macos"))]
fn airc_registrar(airc: &Path, mode: RegistrarMode) -> Result<(), String> {
    use supervisor_install::quiet_command;
    let home = PathBuf::from(home_dir()?);
    let marker = home.join(".airc").join("install-source");
    let source = std::fs::read_to_string(&marker)
        .map_err(|e| format!("cannot read {} to find airc's checkout: {e}; run `airc update`", marker.display()))?;
    let source = PathBuf::from(source.trim());
    #[cfg(target_os = "macos")]
    let mut command = {
        let registrar = source.join("unix").join("register-autostart.sh");
        if !registrar.is_file() {
            return Err(format!("this airc predates its supervisor registrar ({} is missing); run `airc update`", registrar.display()));
        }
        let mut command = quiet_command("bash");
        command.arg(&registrar).arg(airc);
        if mode == RegistrarMode::Check {
            command.arg("--check");
        }
        command
    };
    #[cfg(windows)]
    let mut command = {
        let registrar = source.join("windows").join("register-autostart.ps1");
        if !registrar.is_file() {
            return Err(format!("this airc predates its supervisor registrar ({} is missing); run `airc update`", registrar.display()));
        }
        let root = std::env::var_os("SystemRoot").ok_or("SystemRoot is unset")?;
        let shell = PathBuf::from(root).join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let mut command = quiet_command(shell);
        command
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "RemoteSigned", "-File"])
            .arg(&registrar)
            .arg("-AircPath")
            .arg(airc)
            // Native module discovery across pwsh callers, as the scheduler seam does.
            .env_remove("PSModulePath");
        if mode == RegistrarMode::Check {
            command.arg("-Check");
        }
        command
    };
    let out = command
        .current_dir(&home)
        .output()
        .map_err(|e| format!("cannot run airc's supervisor registrar: {e}"))?;
    if out.status.success() {
        if mode == RegistrarMode::Repair {
            print!("  airc: {}", String::from_utf8_lossy(&out.stdout));
        }
        return Ok(());
    }
    let said = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Err(if said.is_empty() { format!("airc's registrar exited {}", out.status) } else { said })
}

/// The macOS supervisor arm of `continuum install` (card a1bd8b58): READ the registered
/// supervision against the contract ([`launchd::mac_drift`]), and on drift — unless
/// `--check` — WRITE: stop any core on the socket through the save rail, register with
/// the binary as the command (the system LaunchDaemon by default, one sudo, the draft
/// bound to the consent by its digest; `--user` = the agent), hand it the launch, and
/// refuse a core launchd does not own. Then read again: the report is what remains.
#[cfg(target_os = "macos")]
async fn install_supervisor_macos(check: bool, user: bool) -> Result<supervisor_install::ArmReport, String> {
    use launchd::{live, mac_drift, Domain, MacDrift};
    use supervisor_install::ArmReport;
    let socket = socket_path();
    let want = if user {
        // SAFETY: getuid has no preconditions and cannot fail.
        Domain::Gui(unsafe { libc::getuid() })
    } else {
        Domain::System
    };
    let home = home_dir()?;
    let read = || -> Result<Vec<MacDrift>, String> {
        let job = live::job()?;
        let plist = match &job {
            Some(j) => {
                let path = j.domain.plist_path(Path::new(&home));
                Some(std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?)
            }
            None => None,
        };
        let job_pid = job.as_ref().and_then(|j| live::job_pid(&j.domain));
        let on_demand = live::domain_on_demand_only_recently(Duration::from_secs(15 * 60));
        Ok(mac_drift(
            job.as_ref().zip(plist.as_deref()).map(|(j, p)| (&j.domain, p)),
            &want,
            job_pid,
            live::serving_core_pid(&socket),
            on_demand,
        ))
    };
    let before = read()?;
    if before.is_empty() {
        println!("✓ supervisor: converged — {} owns the core, binary as the command, lanes survive a kickstart", want.target());
        return Ok(ArmReport::converged());
    }
    for d in &before {
        println!("  supervisor: {d:?}");
    }
    if check {
        println!("✗ supervisor: drifted; `continuum install` registers {} and hands it the launch", want.target());
        return Ok(ArmReport::read_only(before.len()));
    }

    let artifact = resolve_core_artifact()?;
    let payload = continuum_core::paths::payload_root(&continuum_core::paths::continuum_home()?)?;
    let slot = payload.join("bin").join("continuum-core-server");
    // What launchd must carry for the exec to find its libraries: ORT and the runtime
    // library dirs, read off a Command so it is the direct launch's computation. NOT
    // config.env — the core applies that file to itself on every boot
    // (`config_env::apply_to_process`); frozen into the plist it would outlive an edit
    // until the next `install` (Fable, #4228 review).
    let mut probe = direct_core_command(&artifact, &socket);
    apply_runtime_library_path(&mut probe)?;
    let mut env: Vec<(String, String)> = probe
        .get_envs()
        .filter_map(|(k, v)| Some((k.to_str()?.to_string(), v?.to_str()?.to_string())))
        .collect();
    // launchd's PATH is the system's; the core finds airc and the engines on the
    // operator's. Captured at install, visible in the plist, no login shell needed.
    if let Ok(path) = std::env::var("PATH") {
        env.push(("PATH".to_string(), path));
    }
    // Whatever core holds the socket — an orphan, or the previous registration's own pid
    // — goes down through the save rail before the job is booted out, which would end it
    // without one. Like `reboot`, an unsaved module is printed, not fatal: the goal is a
    // supervised core, and refusing would leave the node down.
    if let Some(pid) = live::serving_core_pid(&socket) {
        let owned = live::job()?.and_then(|j| live::job_pid(&j.domain)) == Some(pid);
        println!(
            "▶ a core (pid {pid}) is serving {socket}{} — stopping it through the save rail before re-registering",
            if owned { " under the existing launchd job" } else { " outside launchd" }
        );
        let _ = stop_with(true).await?;
    }
    let job = live::install(want.clone(), &artifact, &slot, &socket, &env)?;
    // `resolve_core_artifact` prefers the installed slot over a fresh build, so on a node
    // that already has one this REGISTERS what is in the slot; the core arm (or `reboot`)
    // is what brings HEAD to the slot.
    if artifact.canonicalize().ok() == job.slot.canonicalize().ok() {
        println!("  artifact: the slot's own binary (the core arm brings HEAD to the slot)");
    } else {
        println!("  artifact: staged {} into the slot", artifact.display());
    }
    println!("✓ registered {} → {} {socket}", job.domain.target(), job.slot.display());
    // The system daemon was started by its bootstrap (RunAtLoad) inside the elevated half
    // — a kickstart there needs root and is not owed. The agent is kickstarted: on a gui
    // domain in on-demand-only mode bootstrap does NOT start it (measured).
    // A system daemon was spawned by that bootstrap already, so a refusal there may be in
    // this count; the wait then runs to its ceiling instead of failing fast (not wrong).
    let runs_before = live::spawn_runs(&job.domain);
    if matches!(job.domain, Domain::Gui(_)) {
        live::kickstart(&job.domain)?;
    }
    let core_pid = || live::serving_core_pid(&socket);
    match live::wait_owned(&job, runs_before, core_pid, core_is_up, Duration::from_secs(5 * 60)).await {
        Ok(pid) => println!("✓ {} owns the core (pid {pid}); `continuum supervisor-status --crash-test` proves the heal", job.domain.target()),
        Err(why) => {
            let on_demand = live::domain_on_demand_only_recently(Duration::from_secs(3 * 60));
            return Err(format!(
                "registered but NOT supervised: {why}{}",
                if on_demand {
                    " — launchd: \"pending spawn, domain in on-demand-only mode\": the user agent cannot run on this Mac; run `continuum install` (system, sudo once)"
                } else {
                    ""
                }
            ));
        }
    }
    let after = read()?;
    for d in &after {
        println!("  supervisor: still {d:?}");
    }
    Ok(ArmReport { drift_before: before.len(), drift_after: after.len() })
}

/// `continuum uninstall`: unregister the job from launchd (both domains). The staged
/// binary and `~/.continuum` stay; a running core is stopped by the bootout.
pub(super) async fn uninstall() -> Result<(), String> {
    #[cfg(not(target_os = "macos"))]
    {
        Err("continuum uninstall: the macOS arm is here; Windows unregisters through register-core-service.ps1 until its arm lands in this verb".to_string())
    }
    #[cfg(target_os = "macos")]
    {
        let removed = launchd::live::uninstall()?;
        if removed.is_empty() {
            println!("no launchd job registered for {}", launchd::LABEL);
        }
        for domain in removed {
            println!("✓ removed {}", domain.target());
        }
        Ok(())
    }
}
