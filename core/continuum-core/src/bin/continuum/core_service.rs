//! Windows installed-service hosting and the prepared core service task.

use super::*;

/// A marker avoids a relaunch loop; it never substitutes for a token query.
#[cfg(windows)]
pub(super) async fn require_service_medium() -> Result<Option<i32>, String> {
    if !continuum_cli_lifecycle::installed_release::needs_medium_relaunch(
        unelevated_service::token_integrity(),
        std::env::var_os(unelevated_service::UNELEVATED_MARKER).is_some(),
    )? { return Ok(None); }
    let code = tokio::task::spawn_blocking(unelevated_service::relaunch_unelevated)
        .await.map_err(|e| format!("service privilege drop failed: {e}"))??;
    Ok(Some(code))
}

/// The runtime launcher starts media before service-host can lower privileges.
/// A prebuilt reboot already owns the install lease: refuse here instead of
/// recursively relaunching the CLI and deadlocking on that same lease.
#[cfg(any(windows, test))]
pub(super) fn require_prebuilt_runtime_medium(integrity: Result<u32, String>, marker: bool) -> Result<(), String> {
    if continuum_cli_lifecycle::installed_release::needs_medium_relaunch(integrity, marker)? {
        return Err("explicit prebuilt reboot must run from a normal-user terminal; no shutdown or media launch was performed".into());
    }
    Ok(())
}

/// Fixed protected task action: verify normal-user integrity BEFORE reading the
/// caller-owned selection. Core and deploy share this boundary and receipt owner.
pub(super) async fn installed_service(args: Vec<String>) -> Result<i32, String> {
    if args == ["--protocol"] { println!("{}", continuum_cli_lifecycle::installed_release::SUPERVISOR_PROTOCOL_VERSION); return Ok(0); }
    #[cfg(not(windows))]
    { let _ = args; Err("installed-service is supported only on Windows".into()) }
    #[cfg(windows)]
    {
        use continuum_cli_lifecycle::installed_release;
        if args.len()!=2 || !matches!(args[0].as_str(), "core"|"deploy") {
            return Err("installed-service requires core|deploy <active-receipt>".into());
        }
        if let Some(code) = require_service_medium().await? { return Ok(code); }
        let home = PathBuf::from(home_dir()?).join(".continuum");
        let sid = supervisor_install::caller_sid().await?;
        let program_files = PathBuf::from(std::env::var_os("ProgramFiles").ok_or("ProgramFiles is unset")?);
        let executable = std::env::current_exe().map_err(|e|e.to_string())?;
        let envelope = installed_release::Envelope { schema:2, active_release:args[1].clone(), bootstrap:executable.to_string_lossy().into_owned() };
        envelope.validate(&home,&program_files,&sid)?;
        let startup = supervisor_install::maintenance::StartupPermit::acquire(&home).await?;
        let path = Path::new(&envelope.active_release);
        let text = installed_release::read_receipt(path)?;
        let payload = continuum_core::paths::payload_root(&home)?;
        let selected = installed_release::validate_receipt(&text,&home,&payload,&sid)?;
        let release = &selected.release;
        let mut command = if args[0] == "core" {
            // The validated launcher owns media startup as well as the core host.
            // Select it only after the shared Medium gate and receipt validation.
            installed_runtime_command(&PreparedCoreService::shell()?, release, Path::new(&release.artifact), &release.socket)
        } else {
            let mut command = std::process::Command::new(&release.cli);
            command.arg("deploy-consume");
            command
        };
        command.current_dir(Path::new(&release.cli).parent().ok_or("release has no directory")?);
        command.env("CONTINUUM_CORE_SOCKET", &release.socket);
        std::fs::create_dir_all(&release.log_directory).map_err(|e|e.to_string())?;
        let logfile = Path::new(&release.log_directory).join(if args[0]=="core" { "service-bootstrap.log" } else { "deploy-bootstrap.log" });
        let log = std::fs::OpenOptions::new().create(true).append(true).open(logfile).map_err(|e|e.to_string())?;
        // Atomic selection may advance after this final read; pins guarantee this
        // selected generation remains valid. The installing parent checks its SHA.
        if installed_release::read_receipt(path)? != text { return Err("active release changed during supervisor selection".into()); }
        let tree = continuum_cli_lifecycle::windows_launch::spawn_owned_logged(&command,&log,&log,0x0800_0000).map_err(|e|e.to_string())?;
        drop(startup); // Maintenance must drain this already-created child.
        let status = tree.wait().await.map_err(|e|e.to_string())?;
        drop(selected);
        Ok(status.code().unwrap_or(1)) // No normal exit code means the supervised child failed.
    }
}

/// The scheduler owns this foreground host and its core as one process tree.
/// Runtime DLL/config resolution is the same as every other native CLI launch.
pub(super) async fn service_host(args: Vec<String>) -> Result<i32, String> {
    #[cfg(not(windows))]
    {
        let _ = args;
        Err("service-host is supported only on Windows".to_string())
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if !(3..=4).contains(&args.len()) || args.iter().any(|arg| arg.is_empty()) {
            return Err("service-host requires <core-path> <socket> <engine-path> [eye-root]".to_string());
        }
        if let Some(code) = require_service_medium().await? { return Ok(code); }
        let mut command = direct_core_command(Path::new(&args[0]), &args[1]);
        apply_core_runtime_env(&mut command)?;
        // The engine the core runs is the slot `current` names, never an injected
        // `LLAMA_SERVER_BIN` (the core reads that as an operator's pin and never converges it,
        // card 2c5d0ec0). `current` is the one truth on every OS (card d5584dfc): the registered
        // release only BOOTSTRAPS it when none is recorded, so an unattended deploy's promote and
        // an automatic rollback survive this restart. An operator's own `LLAMA_SERVER_BIN` in the
        // environment still passes through untouched.
        if command_env(&command, "LLAMA_SERVER_BIN").is_none_or(|value| value.is_empty()) {
            let engine = Path::new(&args[2]);
            // A standing `current` is the engine, whatever the release still names: the release's
            // own binary may be gone (its slot rebuilt or reclaimed) and the core must still start
            // on the engine `current` names (Codex on #4509, card 6de412bb). Only with nothing
            // standing does the release's engine matter, and then it must exist.
            let home = continuum_core::paths::continuum_home()?;
            let root = continuum_core::inference::engine_slots::root(&home)?;
            match continuum_core::inference::engine_slots::bootstrap_service_engine(&root, engine) {
                Ok(true) => eprintln!("service-host: {} is now the current engine", engine.display()),
                Ok(false) => {}
                Err(why) => {
                    return Err(format!("service-host cannot register managed engine {}: {why}", engine.display()));
                }
            }
        }
        command.env("CONTINUUM_CORE_SOCKET", &args[1]);
        // Use the effective CHILD environment, including config.env, exactly as
        // the core listener will. Task-host environment can differ from it.
        let tcp = command_env(&command, "CONTINUUM_CORE_TCP");
        let endpoint = format!("tcp://127.0.0.1:{}",
            continuum_core::ipc::endpoint_paths::tcp_port_from(tcp.as_deref().and_then(|s| s.to_str())));
        command.stdin(Stdio::null()).creation_flags(0x0800_0000);
        let mut command = tokio::process::Command::from(command);
        command.kill_on_drop(true);
        let mut child = command
            .spawn()
            .map_err(|e| format!("service-host cannot launch {}: {e}", args[0]))?;
        let pid = child
            .id()
            .ok_or("service-host core exited before PID registration")?;
        std::fs::write(pidfile_for(&args[1]), pid.to_string())
            .map_err(|e| format!("service-host cannot record core PID: {e}"))?;
        if args.len() == 3 {
            eprintln!("service-host: browser unavailable: no eyeRoot registered; reinstall to register browser assets");
        }
        let eye = args.get(3).and_then(|root| {
            match continuum_core::boot_plan::start_service_eye(Path::new(root), &endpoint) {
                Ok(eye) => {
                    eprintln!("service-host: browser worker {} spawned for {endpoint}; registration pending", eye.id());
                    Some(eye)
                }
                Err(why) => {
                    eprintln!("service-host: browser unavailable: {why}; core remains serving");
                    None
                }
            }
        });
        let status = if let Some(mut eye) = eye {
            tokio::select! {
                // Core shutdown owns the tree's lifetime, including descendants.
                status = child.wait() => {
                    eye.request_stop();
                    const EYE_STOP_GRACE: Duration = Duration::from_secs(15);
                    match tokio::time::timeout(EYE_STOP_GRACE, eye.wait()).await {
                        Ok(Ok(_)) => {},
                        Ok(Err(error)) => eprintln!("service-host: browser shutdown observation failed ({error}); closing owned tree"),
                        Err(_) => eprintln!("service-host: browser shutdown grace expired; closing owned tree"),
                    }
                    status
                },
                status = eye.wait() => {
                    match status {
                        Ok(status) => eprintln!("service-host: browser unavailable: worker exited ({status}); core remains serving"),
                        Err(error) => eprintln!("service-host: browser unavailable: worker exit observation failed ({error}); core remains serving"),
                    }
                    // Remove any surviving descendants before considering a later
                    // recovery. Never leave a browser orphan or kill the core.
                    drop(eye);
                    child.wait().await
                }
            }
        } else {
            child.wait().await
        }
            .map_err(|e| format!("service-host cannot wait for core: {e}"))?;
        Ok(status.code().unwrap_or(1))
    }
}

#[cfg(any(windows, test))]
pub(super) use continuum_cli_lifecycle::installed_release::Release as CoreServiceDescription;

#[cfg(any(windows, test))]
#[derive(Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct CoreServiceTask {
    pub(super) description: String,
    pub(super) command: String,
    pub(super) arguments: String,
    pub(super) enabled: bool,
    pub(super) state: String,
    #[serde(default)]
    pub(super) working_directory: String,
    #[serde(skip)]
    pub(super) installed: Option<continuum_cli_lifecycle::installed_release::Envelope>,
}

/// A path the ContinuumCore descriptor or the installer's receipt names, absolute and resolved,
/// or why not. THE one resolver for both sides of [`CoreServiceTask::validate`]'s comparison:
/// a receipt holds an ordinary `C:\...` path while a resolved one carries Windows' verbatim
/// `\\?\` prefix, so a receipt compared raw refused every unattended deploy on the 5090
/// (ce84f63ac, 2026-10-12 19:31Z; Codex traced it).
#[cfg(any(windows, test))]
pub(super) fn resolve_service_path(path: impl AsRef<Path>) -> Result<PathBuf, String> {
    let path = path.as_ref();
    if !path.is_absolute() {
        return Err(format!("ContinuumCore path must be absolute: {}", path.display()));
    }
    std::fs::canonicalize(path).map_err(|e| format!("ContinuumCore path {} cannot be resolved: {e}", path.display()))
}

#[cfg(any(windows, test))]
impl CoreServiceTask {
    pub(super) fn validate_description_sha(&self, expected: &str) -> Result<(), String> {
        use sha2::{Digest, Sha256};
        let actual = format!("{:x}", Sha256::digest(self.description.as_bytes()));
        if actual != expected {
            return Err(
                "installed release changed during installer-to-reboot transfer; refusing teardown"
                    .into(),
            );
        }
        Ok(())
    }

    /// What a restart needs from the registered release that staging does not write: a
    /// parseable descriptor whose engine is an installed file and whose log directory
    /// exists. The reboot checks this BEFORE it stops the running core, and `validate`
    /// re-checks it after staging. Measured on the 5090 2026-10-06: the descriptor named an
    /// engine slot that had been moved aside, `validate` first looked after the stop, and the
    /// node sat with no core until a hand restored the slot. A deploy that cannot start the
    /// next core must refuse while the old one still serves.
    pub(super) fn runtime_paths_resolve(&self) -> Result<CoreServiceDescription, String> {
        let description: CoreServiceDescription =
            serde_json::from_str(&self.description).map_err(|e| {
                format!("ContinuumCore has no valid installer artifact descriptor: {e}")
            })?;
        if !resolve_service_path(&description.log_directory)?.is_dir() {
            return Err("ContinuumCore logDirectory is not a directory".to_string());
        }
        if !resolve_service_path(&description.engine)?.is_file() {
            return Err("ContinuumCore engine is not an installed file".to_string());
        }
        Ok(description)
    }

    pub(super) fn validate(&self, candidate: &PrebuiltCore, socket: &str, shell: &Path) -> Result<(), String> {
        let description = self.runtime_paths_resolve()?;
        if self.installed.is_some() { description.installer_root()?; }
        let resolve = resolve_service_path;
        // Each refusal names its condition and both values: one message for four conditions
        // left the 5090's deploy log unable to say which held (card f5fd64ad).
        if !self.enabled {
            return Err("ContinuumCore's scheduled task is disabled; rerun the installer".to_string());
        }
        let selected = resolve(&description.artifact)?;
        if selected != candidate.path {
            return Err(format!(
                "ContinuumCore selects artifact {} but the requested core is {}; rerun the installer",
                selected.display(),
                candidate.path.display()
            ));
        }
        if description.socket != socket {
            return Err(format!(
                "ContinuumCore selects socket {} but the requested socket is {socket}; rerun the installer",
                description.socket
            ));
        }
        if self.installed.is_none() {
            let command = resolve(&self.command)?;
            let shell = shell.canonicalize().map_err(|e| e.to_string())?;
            if command != shell {
                return Err(format!(
                    "ContinuumCore's task runs {} but this host's shell is {}; rerun the installer",
                    command.display(),
                    shell.display()
                ));
            }
        }
        // Paired host/launcher files live in the same installed slot, never in
        // the mutable build cache or a different release's directory.
        for path in [&description.cli, &description.launcher] {
            let path = resolve(path)?;
            if !path.is_file() || path.parent() != candidate.path.parent() {
                return Err(
                    "ContinuumCore host/launcher must be files beside the selected artifact"
                        .to_string(),
                );
            }
        }
        if Path::new(&description.cli).file_name() != Some(std::ffi::OsStr::new("continuum.exe"))
            || Path::new(&description.launcher).file_name()
                != Some(std::ffi::OsStr::new("run-service-hidden.ps1"))
        {
            return Err("ContinuumCore requires the installed CLI and hidden launcher".to_string());
        }
        for value in [
            &description.artifact,
            &description.socket,
            &description.launcher,
            &description.cli,
            &description.engine,
            &description.log_directory,
        ] {
            if value.contains(['"', '\r', '\n']) || value.ends_with('\\') {
                return Err(
                    "ContinuumCore descriptor cannot be quoted as a task argument".to_string(),
                );
            }
        }
        let mut expected = format!(
            "-NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy RemoteSigned -File \"{}\" -ExecutablePath \"{}\" -CorePath \"{}\" -SocketPath \"{}\" -EnginePath \"{}\" -LogDirectory \"{}\"",
            description.launcher, description.cli, description.artifact,
            description.socket, description.engine, description.log_directory,
        );
        if let Some(root) = &description.eye_root {
            if !Path::new(root).is_absolute() || root.contains(['"', '\r', '\n']) || root.ends_with('\\') {
                return Err("ContinuumCore eyeRoot must be an absolute, quotable asset directory".into());
            }
            expected.push_str(&format!(" -EyeRoot \"{root}\""));
        }
        if let Some(envelope) = &self.installed {
            if resolve(&self.command)? != resolve(&envelope.bootstrap)? || self.arguments != envelope.arguments("core") {
                return Err("ContinuumCore stable bootstrap action differs".into());
            }
        } else if self.arguments != expected {
            return Err("ContinuumCore action differs from its artifact descriptor; rerun the installer before reboot".to_string());
        }
        Ok(())
    }
}

pub(super) struct PreparedCoreService {
    #[cfg(windows)]
    pub(super) task: CoreServiceTask,
    #[cfg(windows)]
    pub(super) engine_selection: Option<continuum_core::inference::engine_slots::HandoffSelection>,
    /// macOS: the launchd job that owns the core and the slot its plist execs (a1bd8b58).
    #[cfg(target_os = "macos")]
    pub(super) job: launchd::live::Job,
}

impl PreparedCoreService {
    #[cfg(windows)]
    pub(super) async fn unwind_engine_promotion(&self, failed: String) -> String {
        let Some(selection) = &self.engine_selection else { return failed; };
        match Self::query().await {
            Ok(current) if current.description == self.task.description => match selection.restore() {
                Ok(()) => failed,
                Err(reason) => format!("{failed}; engine recovery refused: {reason}"),
            },
            _ => format!("{failed}; preserved changed or uninspectable supervisor selection"),
        }
    }

    #[cfg(windows)]
    pub(super) async fn installer_root() -> Result<PathBuf, String> {
        Self::query().await?.runtime_paths_resolve()?.installer_root()
    }
    #[cfg(windows)]
    pub(super) async fn browser_release(repo: &Path, check: bool) -> Result<String, String> {
        let repo = repo.to_string_lossy().replace('\'', "''");
        let action = if check { "Get-CoreBrowserReleaseDrift" } else { "Update-CoreBrowserRelease" };
        let script = format!(
            "$ErrorActionPreference='Stop'; . '{repo}/tools/scripts/lib/install-common.ps1'; . '{repo}/tools/scripts/lib/windows-service.ps1'; . '{repo}/tools/scripts/lib/win-modules.ps1'; {action} -RepoRoot '{repo}'"
        );
        if check {
            Self::powershell(&script).await
        } else {
            // Registration/consent is a foreground install operation, not a
            // 30-second scheduler probe. Hold the install lease until it ends.
            Self::run_installer_script(&script)?;
            Ok(String::new())
        }
    }

    #[cfg(windows)]
    pub(super) fn run_installer_script(script: &str) -> Result<(), String> {
        supervisor_install::run_installer_script(&Self::shell()?, script)
    }

    #[cfg(windows)]
    pub(super) async fn engine_drift(repo: &Path) -> Result<String, String> {
        let task = Self::query().await?;
        let description = task.runtime_paths_resolve()?;
        // The engine the core RUNS: the slot `current` names (card d5584dfc), else, before any
        // slot is recorded, the one the release registered.
        let directory = match continuum_core::inference::engine_slots::active_engine_dir()? {
            Some(dir) => dir,
            None => Path::new(&description.engine)
                .parent()
                .ok_or("installed engine has no directory")?
                .to_path_buf(),
        };
        let repo = repo.to_string_lossy().replace('\'', "''");
        Self::powershell(&format!(
            "$ErrorActionPreference='Stop'; . '{repo}/tools/scripts/lib/windows-service.ps1'; . '{repo}/tools/scripts/lib/win-modules.ps1'; Get-CoreEngineDrift -Directory '{}' -Requirement (Get-CoreEngineRequirement -RepoRoot '{repo}')",
            directory.to_string_lossy().replace('\'', "''")
        )).await
    }

    #[cfg(windows)]
    pub(super) async fn prepare_engine(repo: &Path, prebuilt_only: bool) -> Result<Option<(String, PathBuf)>, String> {
        if !prebuilt_only && Self::engine_drift(repo).await?.is_empty() {
            return Ok(None);
        }
        if !prebuilt_only { warm_build_allowed(available_memory_bytes(), locate_start_script().ok())?; }
        let original = Self::query().await?.description;
        let receipt = WarmBuildReceipt::create()?;
        let repo_arg = repo.to_string_lossy().replace('\'', "''");
        let environment = if prebuilt_only { String::new() } else {
            let cuda = build_env_cuda(repo)?;
            format!("$env:CUDA_PATH='{}'; $env:NVCC_PREPEND_FLAGS='{}';", cuda.cuda_path.replace('\'', "''"), cuda.nvcc_prepend_flags.replace('\'', "''"))
        };
        let mode = if prebuilt_only { " -PrebuiltOnly" } else { "" };
        let script = format!(
            "$ErrorActionPreference='Stop'; {environment} . '{repo_arg}/tools/scripts/lib/install-common.ps1'; . '{repo_arg}/tools/scripts/lib/windows-service.ps1'; . '{repo_arg}/tools/scripts/lib/win-modules.ps1'; Prepare-CoreServiceEngine -RepoRoot '{repo_arg}' -Description '{}' -ReceiptPath '{}'{mode}",
            original.replace('\'', "''"), receipt.0.to_string_lossy().replace('\'', "''")
        );
        Self::run_installer_script(&script)?;
        // Every engine slot is busy (lanes still relaunching onto the last engine): the core
        // deploys on the engine it has and the next deploy builds this one (card 3f8f5754, the
        // bash installer's exit 3). An explicit line, never an empty receipt, so a script that
        // wrote nothing is still a failure.
        if let Some(reason) = receipt.skipped()? {
            deploy_note(&format!("▶ engine handoff skipped this deploy: {reason}"));
            return Ok(None);
        }
        Ok(Some((original, receipt.artifact()?)))
    }

    #[cfg(windows)]
    pub(super) async fn register_engine(repo: &Path, original: &str, engine: &Path) -> Result<(), String> {
        if Self::query().await?.description != original {
            return Err("installed release changed before engine handoff".into());
        }
        let mut release: CoreServiceDescription = serde_json::from_str(original)
            .map_err(|e| format!("installed service descriptor: {e}"))?;
        release.engine = engine.to_string_lossy().into_owned();
        let json = serde_json::to_string(&release) // Task Scheduler descriptor crosses the PowerShell process boundary.
            .map_err(|e| e.to_string())?
            .replace('\'', "''");
        let repo = repo.to_string_lossy().replace('\'', "''");
        Self::run_installer_script(&format!(
            "$ErrorActionPreference='Stop'; . '{repo}/tools/scripts/lib/install-common.ps1'; . '{repo}/tools/scripts/lib/windows-service.ps1'; . '{repo}/tools/scripts/lib/win-modules.ps1'; $r='{json}' | ConvertFrom-Json; $drift=Get-CoreEngineDrift -Directory (Split-Path $r.engine) -Requirement (Get-CoreEngineRequirement -RepoRoot '{repo}'); if ($drift) {{ throw $drift }}; try {{ Register-CoreServiceRelease -Release $r -RepoRoot '{repo}' }} finally {{ Clear-Elevation }}"
        ))?;
        Ok(())
    }

    pub(super) async fn for_start(socket: &str) -> Result<Option<(Self, PrebuiltCore)>, String> {
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            let _ = socket;
            Ok(None)
        }
        #[cfg(target_os = "macos")]
        {
            // The supervisor owns the launch when one is registered (a1bd8b58): `start`
            // resumes the artifact in the job's slot through launchd, never beside it.
            // A source override stays the explicit opt-out, exactly as on Windows.
            let _ = socket;
            if std::env::var_os("CONTINUUM_FROM_SOURCE").is_some() {
                return Ok(None);
            }
            let Some(job) = launchd::live::job()? else {
                return Ok(None);
            };
            let path = job
                .slot
                .canonicalize()
                .map_err(|e| format!("launchd job {} execs {} but it cannot be resolved: {e}", job.domain.target(), job.slot.display()))?;
            let sha = binary_build_sha(&path).await?;
            let candidate = PrebuiltCore::from_report(path, sha, None)?;
            Ok(Some((Self { job }, candidate)))
        }
        #[cfg(windows)]
        {
            if std::env::var_os("CONTINUUM_FROM_SOURCE").is_some() {
                return Ok(None);
            }
            let Some(task) = Self::query_optional().await? else {
                return Ok(None);
            };
            let description: CoreServiceDescription = serde_json::from_str(&task.description)
                .map_err(|e| format!("ContinuumCore is not prepared by the current installer: {e}; rerun the installer"))?;
            let path = Path::new(&description.artifact)
                .canonicalize()
                .map_err(|e| format!("cannot resolve installed service artifact: {e}"))?;
            // Start resumes an installed artifact; a developer's current checkout
            // need not match it. Reboot's explicit candidate still MUST match HEAD.
            let sha = binary_build_sha(&path).await?;
            let candidate = PrebuiltCore::from_report(path, sha, None)?;
            task.validate(&candidate, socket, &Self::shell()?)?;
            Ok(Some((Self { task, engine_selection: None }, candidate)))
        }
    }

    /// Stage a freshly built artifact INTO the slot the supervisor is bound to, and
    /// return the staged core as the candidate the handoff validates. The supervisor's
    /// descriptor names one artifact path and the CLI beside it; a warm build lands in
    /// the cargo target dir. This is the copy every hand deploy did before
    /// `--prebuilt <slot>` — the old files move aside as `.prev.exe` (a running exe can
    /// be renamed, never overwritten), the new ones copy in, and the slot's CLI follows
    /// the core so the two never drift (#422's STALE CLI). Nothing here stops the core.
    pub(super) async fn stage(built: &PrebuiltCore, socket: &str) -> Result<PrebuiltCore, PublishedCoreError> {
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            let _ = (built, socket);
            Err("reboot --service is supported only on Windows and macOS".into())
        }
        #[cfg(target_os = "macos")]
        {
            // Same contract as Windows: into the slot the supervisor execs, re-read the
            // sha off the staged file, refuse the handoff on any mismatch.
            let _ = socket;
            let job = launchd::live::job()?
                .ok_or("no launchd job for the core is registered; run install-service.sh install --system")?;
            let staged = launchd::live::stage(&job, &built.path)?;
            let staged = staged
                .canonicalize()
                .map_err(|e| format!("staged artifact cannot be resolved: {e}"))?;
            let sha = binary_build_sha(&staged).await?;
            if sha != built.build_sha {
                return Err(format!(
                    "staged artifact reports build {sha}, the warm build was {} — refusing the handoff",
                    built.build_sha
                ).into());
            }
            println!("✓ staged build {} into the launchd slot: {}", built.build_sha, staged.display());
            PrebuiltCore::from_report(staged, sha, None).map_err(Into::into)
        }
        #[cfg(windows)]
        {
            // One staging owner: preserve the active slot, carry the declared DLL
            // closure, and atomically select the validated inactive release.
            let repo = tracked_repo_dir()?;
            let source = built.path.parent().ok_or("prebuilt artifact has no directory")?;
            let existing = Self::query().await?.runtime_paths_resolve()?;
            let receipt = WarmBuildReceipt::create().map_err(PublishedCoreError::OperationalBlocked)?;
            let quote = |p: &Path| p.to_string_lossy().replace('\'', "''");
            let engine_stage = if source.join("engine/engine-install.json").is_file() {
                format!("$r=New-CoreServiceRelease -RepoRoot '{repo}' -ArtifactDirectory '{source}'; Copy-CorePublishedEngine -RepoRoot '{repo}' -ArtifactDirectory '{source}' -InstallDirectory (Split-Path $r.engine)", repo=quote(&repo), source=quote(source))
            } else {
                format!("$r=New-CoreServiceRelease -RepoRoot '{repo}' -ArtifactDirectory '{source}' -EnginePath '{engine}'", repo=quote(&repo), source=quote(source), engine=quote(Path::new(&existing.engine)))
            };
            let script = format!(
                "$ErrorActionPreference='Stop'; . '{repo}/tools/scripts/lib/install-common.ps1'; . '{repo}/tools/scripts/lib/windows-service.ps1'; . '{repo}/tools/scripts/lib/win-modules.ps1'; . '{repo}/tools/scripts/lib/windows-prebuilt.ps1'; $env:CONTINUUM_CORE_SOCKET='{socket}'; try {{ {engine_stage}; Register-CoreServiceRelease -Release $r -RepoRoot '{repo}' -WorkingDirectory (Split-Path $r.artifact); [IO.File]::WriteAllText('{receipt}', $r.artifact) }} catch {{ $failure = Get-CoreDeploymentFailure -ErrorRecord $_; [IO.File]::WriteAllText('{receipt}', ($failure | ConvertTo-Json -Depth 8 -Compress), (New-Object Text.UTF8Encoding $false)); throw }}",
                repo=quote(&repo), socket=socket.replace('\'', "''"), receipt=quote(&receipt.0),
            );
            if let Err(reason) = Self::run_installer_script(&script) {
                return Err(receipt.classify_deployment_failure(reason));
            }
            let staged_path=resolve_service_path(receipt.artifact().map_err(PublishedCoreError::OperationalBlocked)?)?;
            let sha=binary_build_sha(&staged_path).await?;
            if !sha_matches(&sha,&built.build_sha) { return Err("staged core revision differs from validated prebuilt".into()); }
            PrebuiltCore::from_report(staged_path,sha,None).map_err(Into::into)
        }
    }

    pub(super) async fn prepare(candidate: &PrebuiltCore, socket: &str) -> Result<Self, String> {
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            let _ = (candidate, socket);
            Err("reboot --service is supported only on Windows and macOS".to_string())
        }
        #[cfg(target_os = "macos")]
        {
            // The candidate must BE the slot's file: launchd execs the slot, so a candidate
            // anywhere else would be validated here and never run (#194's shape).
            let _ = socket;
            let job = launchd::live::job()?
                .ok_or("no launchd job for the core is registered; run install-service.sh install --system")?;
            let slot = job.slot.canonicalize().map_err(|e| format!("slot {} cannot be resolved: {e}", job.slot.display()))?;
            let cand = candidate.path.canonicalize().map_err(|e| format!("candidate {} cannot be resolved: {e}", candidate.path.display()))?;
            if slot != cand {
                return Err(format!(
                    "candidate {} is not the launchd slot {} — stage it first; the supervisor only ever runs the slot",
                    cand.display(),
                    slot.display()
                ));
            }
            Ok(Self { job })
        }
        #[cfg(windows)]
        {
            let task = Self::query().await?;
            task.validate(candidate, socket, &Self::shell()?)?;
            Ok(Self { task, engine_selection: None })
        }
    }

    #[cfg(windows)]
    pub(super) fn shell() -> Result<PathBuf, String> {
        let root = std::env::var_os("SystemRoot").ok_or("SystemRoot is unset")?;
        Ok(PathBuf::from(root).join("System32/WindowsPowerShell/v1.0/powershell.exe"))
    }

    #[cfg(windows)]
    pub(super) async fn powershell(script: &str) -> Result<String, String> {
        supervisor_install::powershell(script, Duration::from_secs(30))
            .await
            .map_err(|e| format!("ContinuumCore: {e}"))
    }

    #[cfg(windows)]
    pub(super) async fn query() -> Result<CoreServiceTask, String> {
        Self::query_optional().await?.ok_or_else(|| {
            "ContinuumCore is not registered; run the normal installer first".to_string()
        })
    }

    #[cfg(windows)]
    pub(super) async fn query_optional() -> Result<Option<CoreServiceTask>, String> {
        let json = Self::powershell(
            "$ErrorActionPreference='Stop'; [Console]::OutputEncoding=[Text.UTF8Encoding]::new($false); $t=Get-ScheduledTask | Where-Object { $_.TaskName -eq 'ContinuumCore' -and $_.TaskPath -eq '\\' }; if ($null -eq $t) { 'null'; exit 0 }; if (@($t.Actions).Count -ne 1) { throw 'Expected one core action' }; [pscustomobject]@{description=$t.Description; command=$t.Actions[0].Execute; arguments=$t.Actions[0].Arguments; workingDirectory=$t.Actions[0].WorkingDirectory; enabled=[bool]$t.Settings.Enabled; state=[string]$t.State} | ConvertTo-Json -Compress",
        ).await?;
        let mut task: Option<CoreServiceTask> = serde_json::from_str(&json).map_err(|e| format!("cannot read ContinuumCore task: {e}"))?;
        if let Some(task) = &mut task {
            if let Some(envelope) = continuum_cli_lifecycle::installed_release::envelope(&task.description)? {
                let home = PathBuf::from(home_dir()?).join(".continuum");
                let sid = supervisor_install::caller_sid().await?;
                let program_files = PathBuf::from(std::env::var_os("ProgramFiles").ok_or("ProgramFiles is unset")?);
                envelope.validate(&home, &program_files, &sid)?;
                if task.command.to_lowercase() != envelope.bootstrap.to_lowercase() || task.arguments != envelope.arguments("core") || Path::new(&task.working_directory) != Path::new(&envelope.bootstrap).parent().ok_or("bootstrap has no directory")? {
                    return Err("ContinuumCore stable task action differs from its envelope".into());
                }
                let text = continuum_cli_lifecycle::installed_release::read_receipt(Path::new(&envelope.active_release))?;
                let payload = continuum_core::paths::payload_root(&home)?;
                let _resolved = continuum_cli_lifecycle::installed_release::resolve_receipt(&text,&home,&payload,&sid)?;
                // Preserve PowerShell's descriptor field ordering for the existing SHA handoff.
                // This serializes the SAME validated bytes, never a second racy file read.
                use base64::Engine;
                let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes()); // Active receipt bytes cross the PowerShell process boundary without argument quoting ambiguity.
                task.description = Self::powershell(&format!("$r=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{encoded}')) | ConvertFrom-Json; $r.release | ConvertTo-Json -Compress")).await?;
                task.installed = Some(envelope);
            }
        }
        Ok(task)
    }

    pub(super) async fn launch(self, wait_for_death: &[i32]) -> Result<u64, String> {
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            let _ = wait_for_death;
            Err("reboot --service is supported only on Windows and macOS".to_string())
        }
        #[cfg(target_os = "macos")]
        {
            let started = std::time::Instant::now();
            // The old core is gone (stop_with ran) or launchd's `-k` ends it; either way
            // wait for the pids we were told about before the receipt is read.
            let deadline = std::time::Instant::now() + Duration::from_secs(60);
            while wait_for_death.iter().any(|pid| pid_alive(*pid)) {
                if std::time::Instant::now() >= deadline {
                    return Err("the old core did not exit within 60 s; no kickstart was issued".to_string());
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            eprintln!(
                "▶ supervisor={}: kickstarting the staged artifact ({})",
                self.job.domain.target(),
                self.job.slot.display()
            );
            let socket = socket_path();
            let core_pid = {
                let socket = socket.clone();
                move || launchd::live::serving_core_pid(&socket)
            };
            // One road for a refused kickstart and a refused spawn: the old core is
            // already stopped, so either one with nothing answering is a dark node.
            let runs_before = launchd::live::spawn_runs(&self.job.domain);
            let started_new = match launchd::live::kickstart(&self.job.domain) {
                Ok(()) => launchd::live::wait_owned(&self.job, runs_before, core_pid, core_is_up, Duration::from_secs(5 * 60))
                    .await
                    .map(|_| ()),
                Err(refused) => Err(refused),
            };
            let staged_failed = match launchd::after_staged_start(started_new, core_is_up().await) {
                launchd::HandoffNext::Done => return Ok(started.elapsed().as_secs()),
                launchd::HandoffNext::Report(why) => return Err(why),
                launchd::HandoffNext::RollBack(why) => why,
            };
            eprintln!("⚠ {staged_failed}\n▶ restoring the previous build and kickstarting it");
            let kept = launchd::live::restore_previous(&self.job)
                .map_err(|e| format!("{staged_failed}; the node is DARK: {e}"))?;
            // A kickstart error is not a dark node: on the M5 (2026-10-03) `kickstart -k`
            // reported failure at 10:08:02Z while the restored core started in that same
            // second, most likely spawned by that kickstart before it exited non-zero. (Not
            // KeepAlive: the plist sets it to Crashed only, which never retries a job that did
            // not start.) Only the bounded wait decides.
            let runs_before = launchd::live::spawn_runs(&self.job.domain);
            if let Err(e) = launchd::live::kickstart(&self.job.domain) {
                eprintln!("⚠ kickstart of the restored build reported: {e}; waiting for launchd to start it");
            }
            let core_pid = move || launchd::live::serving_core_pid(&socket);
            match launchd::live::wait_owned(&self.job, runs_before, core_pid, core_is_up, Duration::from_secs(5 * 60)).await {
                // Answering under launchd is not yet "restored": the receipt names the build that
                // is actually running and refuses one that is not the build put back (#194).
                Ok(pid) => Err(match restored_build_identity(&self.job.slot).await {
                    Ok(sha) => format!(
                        "{staged_failed}; rolled back: the previous build {sha} is serving under launchd (pid {pid}), the refused one is kept at {}",
                        kept.display()
                    ),
                    Err(why) => format!("{staged_failed}; rolled back, but the restored core's identity is unproven: {why}"),
                }),
                Err(again) => Err(format!("{staged_failed}; the previous build did not come up either, the node is DARK: {again}")),
            }
        }
        #[cfg(windows)]
        {
            match self.launch_windows_once(wait_for_death).await {
                Ok(elapsed) => Ok(elapsed),
                Err(failed) => {
                    if self.task.installed.is_none() || core_is_up().await { return Err(failed); }
                    let current = Self::query().await?;
                    if current.state == "Running" || current.description != self.task.description {
                        return Err(format!("{failed}; preserved running or concurrently changed supervisor"));
                    }
                    if let Some(selection) = &self.engine_selection {
                        selection.restore().map_err(|e| format!("{failed}; engine recovery refused: {e}"))?;
                    }
                    // The caller still holds the install lease. Restoration validates
                    // the prior payload and refuses a changed active selection.
                    let repo = self.task.runtime_paths_resolve()?.installer_root()?;
                    let root = repo.to_string_lossy().replace('\'', "''");
                    let expected = self.task.description.replace('\'', "''");
                    Self::run_installer_script(&format!("$ErrorActionPreference='Stop'; . '{root}/tools/scripts/lib/install-common.ps1'; . '{root}/tools/scripts/lib/windows-service.ps1'; Restore-CoreActiveRelease -ExpectedDescription '{expected}' | Out-Null"))
                        .map_err(|e|format!("{failed}; previous release recovery refused: {e}"))?;
                    let restored = Self { task:Self::query().await?, engine_selection: None };
                    let release = restored.task.runtime_paths_resolve()?;
                    restored.launch_windows_once(wait_for_death).await
                        .map_err(|e|format!("{failed}; previous release also failed: {e}"))?;
                    let sha = restored_build_identity(Path::new(&release.artifact)).await
                        .map_err(|e|format!("{failed}; restored service identity unproven: {e}"))?;
                    Err(format!("{failed}; recovered previous installed build {sha} under the same supervisor; candidate was not adopted"))
                }
            }
        }
    }

    #[cfg(windows)]
    pub(super) async fn launch_windows_once(&self, wait_for_death: &[i32]) -> Result<u64,String> {

            let started = std::time::Instant::now();
            // A task may still be finishing its foreground host after the core
            // exits. IgnoreNew would silently swallow our start in that window.
            let mut ticks = tokio::time::interval(Duration::from_secs(1));
            loop {
                ticks.tick().await;
                let task = Self::query().await?;
                if task.description != self.task.description
                    || task.command != self.task.command
                    || task.arguments != self.task.arguments
                    || !task.enabled
                {
                    return Err("ContinuumCore task changed during reboot; refusing to launch a different action".to_string());
                }
                if !wait_for_death.iter().any(|pid| pid_alive(*pid)) && task.state == "Ready" {
                    break;
                }
                if started.elapsed() >= Duration::from_secs(60) {
                    return Err("ContinuumCore old process/task did not finish within 60 s; no second core was launched".to_string());
                }
            }
            Self::powershell("$ErrorActionPreference='Stop'; Start-ScheduledTask -TaskName 'ContinuumCore' -TaskPath '\\'").await?;
            eprintln!("▶ supervisor=ContinuumCore: starting the prepared installed artifact");
            let ready = std::time::Instant::now();
            let mut ticks = tokio::time::interval(Duration::from_secs(2));
            loop {
                ticks.tick().await;
                if !wait_for_death.iter().any(|pid| pid_alive(*pid)) && core_is_up().await {
                    if Self::query().await?.state != "Running" {
                        return Err("A core answered but ContinuumCore is not running; refusing an unsupervised startup receipt".to_string());
                    }
                    return Ok(started.elapsed().as_secs());
                }
                if ready.elapsed() >= Duration::from_secs(5 * 60) {
                    return Err("ContinuumCore did not answer within 5 minutes; inspect its task result and service logs".to_string());
                }
                if ready.elapsed() >= Duration::from_secs(5)
                    && Self::query().await?.state != "Running"
                {
                    return Err("ContinuumCore task stopped before the core answered; inspect its task result and service logs".to_string());
                }
            }
    }
}
