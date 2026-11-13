//! The deploy build, run DARK: serving is stopped first, the build takes the whole machine,
//! and a failed build puts the previous core back.
//!
//! Joel, 2026-10-24: "No. Take down serving while building." The warm build that compiled
//! beside a serving core is gone, and its admission machinery with it (a free-memory gate, a
//! job count derived from constants, nice 19). On a unified-memory node the compiler and the
//! residents' KV share one pool: 2026-10-23 a crate-local check at -j4 beside serving pushed
//! the M5 into a Metal out-of-memory, the engine wedged 10 minutes and 32 acts were recorded
//! as completed when they had failed. No one-time gate can follow KV growth and rustc peaks
//! mid-build, so the build no longer shares the machine with serving at all.

use super::*;

/// The build that was serving when a dark build stopped it: what a failed build puts back,
/// so a broken tree never leaves the node dark.
pub(super) enum PreviousCore {
    /// The supervisor's slot still holds it: a build is staged into the slot only after it
    /// succeeds, so restoring is the supervisor starting the slot it already has.
    Supervised(PreparedCoreService, PrebuiltCore),
    /// A directly launched core, copied aside BEFORE the build, which writes the same cargo
    /// path the running core was exec'd from and may leave a half-built file there.
    Direct(PrebuiltCore),
}

impl PreviousCore {
    /// Read before the stop. `None` = no core was serving, so nothing can be left dark.
    /// An error refuses the reboot while the core still serves: without a way back, a
    /// failed build would leave the node down.
    pub(super) async fn capture(old: &[i32], service: bool, socket: &str) -> Result<Option<Self>, String> {
        if old.is_empty() {
            return Ok(None);
        }
        if service {
            return match PreparedCoreService::for_start(socket).await? {
                Some((supervisor, core)) => Ok(Some(Self::Supervised(supervisor, core))),
                None => Err("no registered supervisor holds the serving build; nothing could restore it if the build failed".to_string()),
            };
        }
        let exe = running_core_exe(old)?;
        let kept_dir = continuum_core::paths::continuum_home()?.join("state").join("reboot-previous-core");
        std::fs::create_dir_all(&kept_dir).map_err(|e| format!("cannot create {}: {e}", kept_dir.display()))?;
        let name = exe.file_name().ok_or_else(|| format!("serving core path {} has no file name", exe.display()))?;
        let kept = kept_dir.join(name);
        std::fs::copy(&exe, &kept).map_err(|e| format!("cannot keep the serving build {} aside: {e}", exe.display()))?;
        let sha = binary_build_sha(&kept).await?;
        Ok(Some(Self::Direct(PrebuiltCore::from_report(kept, sha, None)?)))
    }

    /// Start the previous build again and say what is serving. Always returns the report
    /// the caller fails with: `failed` first, then the restore's outcome.
    pub(super) async fn restore(self, failed: String, old: &[i32]) -> String {
        eprintln!("⚠ {failed}\n▶ restarting the previous build so the node does not stay dark");
        let (launched, core) = match self {
            Self::Supervised(supervisor, core) => (supervisor.launch(old).await, core),
            Self::Direct(core) => (launch_core(old, LaunchSource::Prebuilt(&core)).await, core),
        };
        match launched {
            Err(again) => format!("{failed}; the previous build {} did not come back either, the node is DARK: {again}", core.build_sha),
            Ok(secs) => match restored_build_identity(&core.path).await {
                Ok(sha) => format!("{failed}; rolled back: the previous build {sha} is serving again ({secs}s to answer)"),
                Err(why) => format!("{failed}; the previous build was restarted but what answers is unproven: {why}"),
            },
        }
    }
}

/// A failure after the stop: restart the previous build when there is one, and report both,
/// keeping the failure's class (a deploy consumer retries a resource deferral, never a broken
/// build).
pub(super) async fn restore_or_report(previous: Option<PreviousCore>, failed: PublishedCoreError, old: &[i32]) -> PublishedCoreError {
    let Some(previous) = previous else {
        return failed;
    };
    let report = previous.restore(failed.to_string(), old).await;
    match failed {
        PublishedCoreError::Artifact(_) => PublishedCoreError::Artifact(report),
        PublishedCoreError::SourcePending(_) => PublishedCoreError::SourcePending(report),
        PublishedCoreError::CapabilityDeferred(_) => PublishedCoreError::CapabilityDeferred(report),
        PublishedCoreError::OperationalBlocked(_) => PublishedCoreError::OperationalBlocked(report),
        PublishedCoreError::Held(_) => PublishedCoreError::Held(report),
        PublishedCoreError::ResourceDeferred(deferral) => PublishedCoreError::ResourceDeferred(
            continuum_cli_lifecycle::prebuilt_artifact::ResourceDeferral { reason: report, ..deferral },
        ),
    }
}

/// The executable of the serving core, from the process itself (never re-resolved from a
/// path guess, which could name a different file than the one that is running).
pub(super) fn running_core_exe(old: &[i32]) -> Result<PathBuf, String> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let pids: Vec<Pid> = old.iter().map(|pid| Pid::from_u32(*pid as u32)).collect();
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&pids),
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::Always),
    );
    pids.iter()
        .find_map(|pid| sys.process(*pid).and_then(|p| p.exe()).map(Path::to_path_buf))
        .ok_or_else(|| format!("cannot read the executable of serving core pid(s) {old:?}"))
}

/// How long the node was dark, by phase, printed into the deploy log every reboot so the
/// cost of building dark is a number on each node, never a guess.
pub(super) struct DarkWindow {
    stopping: std::time::Instant,
    stopped: Option<Duration>,
    built: Option<Duration>,
}

impl DarkWindow {
    pub(super) fn start() -> Self {
        Self { stopping: std::time::Instant::now(), stopped: None, built: None }
    }
    pub(super) fn stopped(&mut self) {
        self.stopped = Some(self.stopping.elapsed());
    }
    pub(super) fn built(&mut self) {
        self.built = Some(self.stopping.elapsed());
    }
    /// The receipt once the new core answers.
    pub(super) fn receipt(&self) -> String {
        let total = self.stopping.elapsed();
        let stop = self.stopped.unwrap_or(total);
        let build = self.built.map_or(Duration::ZERO, |b| b.saturating_sub(stop));
        let start = total.saturating_sub(stop).saturating_sub(build);
        format!(
            "dark window {}s: stop {}s, build {}s, start to answering {}s",
            total.as_secs(), stop.as_secs(), build.as_secs(), start.as_secs()
        )
    }
}

/// A per-attempt receipt owned by this invocation, never a cached artifact hint.
/// The build script writes its selected native path only after all checks pass.
pub(super) struct BuildReceipt(pub(super) PathBuf);

impl BuildReceipt {
    pub(super) fn create() -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!(
            "continuum-build-{}.receipt",
            uuid::Uuid::new_v4()
        ));
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| format!("cannot create build receipt: {e}"))?;
        Ok(Self(path))
    }

    /// `Some(reason)` when the script recorded a deliberate skip (`SKIP: <reason>`) instead of an
    /// artifact path.
    #[cfg(windows)]
    pub(super) fn skipped(&self) -> Result<Option<String>, String> {
        let report = std::fs::read_to_string(&self.0).map_err(|e| format!("cannot read build receipt: {e}"))?;
        Ok(report.strip_prefix("SKIP: ").map(|reason| reason.trim().to_string()))
    }

    pub(super) fn read_report(&self) -> Result<String, String> {
        use std::io::Read;
        const MAX_RECEIPT_BYTES: u64 = 16 * 1024;
        let mut report = String::new();
        std::fs::File::open(&self.0)
            .and_then(|file| file.take(MAX_RECEIPT_BYTES + 1).read_to_string(&mut report))
            .map_err(|e| format!("cannot read build receipt: {e}"))?;
        if report.len() > MAX_RECEIPT_BYTES as usize { return Err("build receipt exceeds its existing boundary".into()); }
        Ok(report)
    }

    #[cfg(any(windows, test))]
    pub(super) fn deployment_failure(&self) -> Result<Option<PublishedCoreError>, String> {
        let report = self.read_report()?;
        if report.is_empty() { return Ok(None); }
        // boundary: this invocation's installer writes the typed failure only on error.
        serde_json::from_str(&report).map(Some).map_err(|error| format!("invalid installer failure receipt: {error}"))
    }

    #[cfg(any(windows, test))]
    pub(super) fn classify_deployment_failure(&self, original: String) -> PublishedCoreError {
        match self.deployment_failure() {
            Ok(Some(failure)) => failure,
            Ok(None) => PublishedCoreError::OperationalBlocked(format!("{original}; installer failure classification receipt is empty; no build failure charged")),
            Err(error) => PublishedCoreError::OperationalBlocked(format!("{original}; {error}; installer failure classification unavailable; no build failure charged")),
        }
    }

    pub(super) fn artifact(&self) -> Result<PathBuf, String> {
        let report = self.read_report()?;
        let path = report.strip_suffix('\n').unwrap_or(&report); // No optional framing newline means the entire report is the path.
        if path.is_empty()
            || path.contains(['\n', '\r', '\0'])
            || !Path::new(path).is_absolute()
        {
            return Err("the build did not report one absolute native artifact path".to_string());
        }
        Ok(PathBuf::from(path))
    }
}

impl Drop for BuildReceipt {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Build through the start script's own definition (`CONTINUUM_BUILD_ONLY=1`) with serving
/// already stopped: cargo's own job count at normal priority, since nothing else is running.
/// The exact artifact the script reports is validated before anything launches it.
pub(super) async fn build_dark(mut cmd: std::process::Command) -> Result<PrebuiltCore, String> {
    let receipt = BuildReceipt::create()?;
    cmd.env("CONTINUUM_BUILD_ONLY", "1")
        .env("CONTINUUM_BUILD_RECEIPT", &receipt.0)
        .stdin(Stdio::null());
    // Under the deploy consumer there is no terminal: the build's output goes to the
    // consumer's log, or a failing build leaves no reason anywhere (2026-09-19, the
    // 5090's first unattended deploy: 30 minutes of rustc, then nothing to read).
    #[cfg(not(windows))]
    if let Some(log) = DEPLOY_LOG.get().and_then(|p| open_log_for_child(p).ok()) {
        if let Ok(err) = log.try_clone() {
            cmd.stdout(Stdio::from(log)).stderr(Stdio::from(err));
        }
    }
    #[cfg(not(windows))]
    let status = cmd.status();
    #[cfg(windows)]
    let status = {
        // Cancelling the deploy task must cancel Bash, Cargo and rustc together.
        // The existing job boundary assigns ownership before the first child
        // instruction; killing only the installer left orphan compilers on the
        // 5090 (2026-10-01), including a fallback compile after cancellation.
        use std::os::windows::io::AsHandle;
        let outputs = match DEPLOY_LOG.get() {
            Some(path) => open_log_for_child(path)
                .and_then(|out| out.try_clone().map(|err| (out, err))),
            None => std::io::stdout().as_handle().try_clone_to_owned().and_then(|out| {
                std::io::stderr().as_handle().try_clone_to_owned()
                    .map(|err| (std::fs::File::from(out), std::fs::File::from(err)))
            }),
        };
        match outputs.and_then(|(out, err)| {
            continuum_cli_lifecycle::windows_launch::spawn_owned_logged(
                &cmd, &out, &err, 0x0800_0000, // hidden (CREATE_NO_WINDOW), normal priority: nothing is serving
            )
        }) {
            Ok(tree) => tree.wait().await,
            Err(error) => Err(error),
        }
    };
    let status = status.map_err(|e| {
        format!("the build could not start: {e}")
    })?;
    if !status.success() {
        // Say WHAT ran. Three consumer attempts on the 5090 (2026-09-19) each died
        // in one second with nothing in the log; the exit code alone named nothing.
        return Err(format!(
            "the build exited {status}
  command: {cmd:?}
  envs: {:?}
  cwd: {:?}",
            cmd.get_envs()
                .map(|(k, v)| {
                    let value = v.map(|v| v.to_string_lossy().into_owned()).unwrap_or("<unset>".into()); // unwrap_or: a None env value IS an unset (env_remove), shown as such
                    format!("{}={value}", k.to_string_lossy())
                })
                .collect::<Vec<_>>(),
            cmd.get_current_dir().map(|p| p.to_path_buf()).or_else(|| std::env::current_dir().ok())
        ));
    }
    PrebuiltCore::prepare(&receipt.artifact()?).await
}
