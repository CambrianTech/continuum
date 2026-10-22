//! Warm build before stop: headroom gate, priority yield, build receipt and preparation.

use super::*;

/// The floor below which a warm build refuses: one rustc job plus the reserve kept for the
/// citizens and the core. Between it and plenty the build takes fewer jobs
/// ([`continuum_core::inference::llama_server::warm_build_jobs_for_memory`]).
pub(super) const WARM_BUILD_MIN_FREE_BYTES: u64 = continuum_core::inference::llama_server::WARM_BUILD_MIN_FREE_BYTES;

/// May this reboot build before it stops? Needs a start script (one build definition)
/// and headroom. The refusal names the number so the operator sees why the dark
/// window will be long this time.
pub(super) fn warm_build_allowed(free_bytes: u64, script: Option<PathBuf>) -> Result<PathBuf, String> {
    let Some(script) = script else {
        return Err("no start script (installed node) — nothing to build".to_string());
    };
    if free_bytes < WARM_BUILD_MIN_FREE_BYTES {
        return Err(format!(
            "{:.1} GiB free, the warm build wants {} GiB beside the serving core",
            free_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
            WARM_BUILD_MIN_FREE_BYTES / (1024 * 1024 * 1024)
        ));
    }
    Ok(script)
}

/// Available system memory, through the ONE derivation the substrate already owns.
///
/// NOT `sysinfo::available_memory()`. That call returns **0 on macOS** while `total`
/// and `used` are both correct — `system_resources::memory_pressure` found this, says
/// so in so many words at its own call site, and exists as `available_from` precisely
/// so "every reader" shares one answer. This function was not one of those readers.
///
/// The cost was the whole warm-build path on every Mac. `warm_build_allowed` compares
/// this against `WARM_BUILD_MIN_FREE_BYTES` (then 12 GiB), so a permanent 0 meant the gate
/// could never open: every deploy stopped the core first and built afterwards, and every
/// stop cut whatever was mid-turn. Measured on the M5 2026-09-21, two consecutive
/// deploys 35 minutes apart printed `no warm build: 0.0 GiB free` and reported
/// `cognition (drain Incomplete { in_flight: 7 })` then `{ in_flight: 9 }` — sixteen
/// citizen turns — while `memory.pressure`, reading `available_from` at the same
/// moments, published `avail_mb` of 9,009 and 8,324. Two measurements of one quantity,
/// nine gigabytes apart, and the deploy gate held the one that is always zero here.
pub(super) fn available_memory_bytes() -> u64 {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    continuum_core::system_resources::memory_pressure::available_from(&sys)
}

/// A per-attempt receipt owned by this invocation, never a cached artifact hint.
/// The build script writes its selected native path only after all checks pass.
pub(super) struct WarmBuildReceipt(pub(super) PathBuf);

impl WarmBuildReceipt {
    pub(super) fn create() -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!(
            "continuum-warm-build-{}.receipt",
            uuid::Uuid::new_v4()
        ));
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| format!("cannot create warm-build receipt: {e}"))?;
        Ok(Self(path))
    }

    /// `Some(reason)` when the script recorded a deliberate skip (`SKIP: <reason>`) instead of an
    /// artifact path.
    #[cfg(windows)]
    pub(super) fn skipped(&self) -> Result<Option<String>, String> {
        let report = std::fs::read_to_string(&self.0).map_err(|e| format!("cannot read warm-build receipt: {e}"))?;
        Ok(report.strip_prefix("SKIP: ").map(|reason| reason.trim().to_string()))
    }

    pub(super) fn read_report(&self) -> Result<String, String> {
        use std::io::Read;
        const MAX_RECEIPT_BYTES: u64 = 16 * 1024;
        let mut report = String::new();
        std::fs::File::open(&self.0)
            .and_then(|file| file.take(MAX_RECEIPT_BYTES + 1).read_to_string(&mut report))
            .map_err(|e| format!("cannot read warm-build receipt: {e}"))?;
        if report.len() > MAX_RECEIPT_BYTES as usize { return Err("warm build receipt exceeds its existing boundary".into()); }
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
            return Err("warm build did not report one absolute native artifact path; leaving the running core untouched".to_string());
        }
        Ok(PathBuf::from(path))
    }
}

impl Drop for WarmBuildReceipt {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// A warm build beside a serving core: below the core in the run queue, never throttled.
///
/// Joel, 2026-10-03: "Speed of engineering iteration is everything now and building isn't a
/// runtime feature." End users never compile (they get binaries); the machines that build are
/// the fleet's own, where a deploy gates every fix's verification. So the build takes every
/// core the core is not using:
/// - nice 19 (Windows: below-normal class), inherited by cargo and every rustc, so a persona's
///   decode still preempts it; that only orders the run queue, so idle cores all go to the build;
/// - NO CPU-lane job cap and NO macOS background band. Those were removed: on the IntelMac they
///   held a deploy's final links to ~95 CPU-minutes per bin across ~5h40m of wall time, and on
///   the M5 the band alone held rustc at 8% CPU where clearing it gave 58% (#4679);
/// - the MEMORY budget stays (`warm_build_jobs_for_memory`): that one prevents an OOM kill of the
///   serving node, which is the never-crash contract, not a speed knob.
pub(super) fn yield_to_serving(cmd: &mut std::process::Command) {
    if let Some(jobs) =
        continuum_core::inference::llama_server::warm_build_jobs_for_memory(available_memory_bytes())
    {
        cmd.env("CARGO_BUILD_JOBS", jobs.to_string());
    }
    #[cfg(unix)]
    // SAFETY: the closure runs in the forked child before exec and calls only
    // setpriority, which is async-signal-safe; it touches no memory of the parent.
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            // PRIO_PROCESS on the child itself (who = 0); cargo and rustc inherit it.
            libc::setpriority(libc::PRIO_PROCESS, 0, 19);
            Ok(())
        });
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Children of a below-normal process inherit its class by default.
        const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
        cmd.creation_flags(BELOW_NORMAL_PRIORITY_CLASS);
    }
}

pub(super) async fn prepare_warm_build(mut cmd: std::process::Command) -> Result<PrebuiltCore, String> {
    let receipt = WarmBuildReceipt::create()?;
    cmd.env("CONTINUUM_BUILD_ONLY", "1")
        .env("CONTINUUM_BUILD_RECEIPT", &receipt.0)
        .stdin(Stdio::null());
    yield_to_serving(&mut cmd);
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
                &cmd, &out, &err, 0x0800_4000, // hidden, below-normal priority
            )
        }) {
            Ok(tree) => tree.wait().await,
            Err(error) => Err(error),
        }
    };
    let status = status.map_err(|e| {
        format!("warm build could not start: {e}; leaving the running core untouched")
    })?;
    if !status.success() {
        // Say WHAT ran. Three consumer attempts on the 5090 (2026-09-19) each died
        // in one second with nothing in the log; the exit code alone named nothing.
        return Err(format!(
            "warm build exited {status}; leaving the running core untouched
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
