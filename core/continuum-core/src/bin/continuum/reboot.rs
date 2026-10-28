//! `continuum reboot`: options, prebuilt core hand-off and the stop/rebuild/relaunch path.

use super::*;

/// `continuum reboot` — stop + rebuild + relaunch the core.
/// Reboot keeps its existing lease/force contract for both source and prebuilt
/// deployment. An invalid or repeated option must not silently request a swap.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct RebootOptions {
    pub(super) force: bool,
    pub(super) prebuilt: Option<PathBuf>,
    /// Install retry: validated Cargo output still needs staging into the service slot.
    pub(super) stage_prebuilt: bool,
    pub(super) service: bool,
    pub(super) validate_only: bool,
    /// Explicit CPU embedding validation, performed only in a bounded child.
    pub(super) embedding_model: Option<PathBuf>,
    /// Is a human AT this machine, able to answer one consent prompt?
    ///
    /// NOT a CLI flag, and deliberately: the user-facing contract is one command,
    /// `continuum install` (Joel, 2026-09-22), so the only caller that sets this is the
    /// install core arm. Everything else — the deploy consumer above all — runs under
    /// the supervisor with nobody to see a dialog, and a consent raised there would hang
    /// the tick until it times out.
    pub(super) operator_present: bool,
    /// Internal convergence requirement; the public command remains `install`.
    #[cfg(windows)]
    pub(super) require_engine_receipt: bool,
    /// Installer-to-reboot transfer binds the entire selected release.
    pub(super) service_descriptor_sha: Option<String>,
    /// The caller already installed the CLI that ships with `prebuilt` (deploy-consume's
    /// CI companions, card 50ca737e). Not a CLI flag. Without it, a download deploy that
    /// DID replace the CLI reported "⚠ STALE CLI ... nothing in this run replaced it" and
    /// told the operator to `reboot`, which on a slow node is a multi-hour compile that
    /// fixes nothing (IntelMac, 2026-10-04, card cca11352).
    pub(super) cli_installed: bool,
}

impl RebootOptions {
    pub(super) fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut options = Self::default();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--force" if !options.force => options.force = true,
                "--service" if !options.service => options.service = true,
                "--validate-only" if !options.validate_only => options.validate_only = true,
                "--embedding-model" if options.embedding_model.is_none() => {
                    let path = args
                        .next()
                        .filter(|p| !p.is_empty() && !p.starts_with('-'))
                        .ok_or("--embedding-model requires a local GGUF path")?;
                    options.embedding_model = Some(PathBuf::from(path));
                }
                "--prebuilt" if options.prebuilt.is_none() => {
                    let path = args
                        .next()
                        .filter(|p| !p.is_empty() && !p.starts_with('-'))
                        .ok_or("reboot --prebuilt requires a core binary path")?;
                    options.prebuilt = Some(PathBuf::from(path));
                }
                "--service-descriptor-sha" if options.service_descriptor_sha.is_none() => {
                    let sha = args
                        .next()
                        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
                        .ok_or("--service-descriptor-sha requires a SHA-256 digest")?;
                    options.service_descriptor_sha = Some(sha.to_ascii_lowercase());
                }
                "--force" | "--prebuilt" | "--service" | "--validate-only"
                | "--embedding-model" => return Err(format!("duplicate reboot option {arg}")),
                _ => {
                    return Err(format!(
                        "unknown reboot option {arg}; use --force or --prebuilt <path> [--service]"
                    ))
                }
            }
        }
        // `--service` alone (2026-09-19, card 82af11f5): the dark build produces the
        // artifact and the supervisor takes it — the path the deploy consumer runs
        // unattended. Until now `--service` demanded a hand-supplied `--prebuilt`,
        // which is exactly why the 5090 ran whatever a hand last typed.
        if options.service && !(cfg!(windows) || cfg!(target_os = "macos")) {
            return Err("reboot --service is supported only on Windows and macOS".to_string());
        }
        if options.validate_only && (options.prebuilt.is_none() || options.service || options.force)
        {
            return Err(
                "--validate-only requires --prebuilt and cannot combine with --force or --service"
                    .to_string(),
            );
        }
        if options.embedding_model.is_some() && !options.validate_only {
            return Err("--embedding-model requires --prebuilt and --validate-only".into());
        }
        if options.service_descriptor_sha.is_some()
            && (!cfg!(windows)
                || !options.service
                || options.prebuilt.is_none()
                || options.validate_only)
        {
            return Err(
                "--service-descriptor-sha requires a Windows prebuilt service handoff".into(),
            );
        }
        Ok(options)
    }
}

/// An explicitly selected artifact whose loader and provenance were checked while
/// the previous core was still alive. Launch and verification borrow this SAME
/// path/SHA pair; neither may re-resolve an ambient installed-binary override.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct PrebuiltCore {
    pub(super) path: PathBuf,
    pub(super) build_sha: String,
}

impl PrebuiltCore {
    pub(super) async fn prepare(path: &Path) -> Result<Self, String> {
        let path = path
            .canonicalize()
            .map_err(|e| format!("prebuilt core {} cannot be resolved: {e}", path.display()))?;
        if !path.is_file() {
            return Err(format!("prebuilt core {} is not a file", path.display()));
        }
        let build_sha = binary_build_sha(&path).await?;
        let cwd = std::env::current_dir()
            .map_err(|e| format!("cannot locate checkout for prebuilt verification: {e}"))?;
        let git_dir = std::env::var_os("GIT_DIR");
        let checkout_sha = prebuilt_checkout_sha(&cwd, git_dir.as_deref()).await?;
        // THE BUILD KEY, NOT HEAD (card 9080ffb0 / #4732). The core is a function of the
        // build inputs, so CI publishes one artifact per commit that touched them and a
        // docs-only tip is served by its key's artifact. Checking that artifact against
        // HEAD refused every such tip in two seconds ("DEPLOY MISMATCH ... shipped build
        // 937976559 (git HEAD)" on the M5, 2026-10-05 01:55Z, for the key f8c69966e).
        let expected = match checkout_sha.as_deref() {
            Some(head) => Some(continuum_cli_lifecycle::prebuilt_artifact::prebuilt_expectation(&cwd, git_dir.as_deref(), head, &build_sha)),
            None => None,
        };
        Self::from_report(path, build_sha, expected.as_ref().map(|(sha, src)| (sha.as_str(), *src)))
    }

    pub(super) fn from_report(
        path: PathBuf,
        build_sha: String,
        expected: Option<(&str, &'static str)>,
    ) -> Result<Self, String> {
        // Reuse #194's credible-SHA and short/full-SHA comparison. Without a
        // checkout the artifact anchors its own receipt; unknown/malformed
        // provenance still fails. With a checkout it must match the checkout's
        // build key (its HEAD when git cannot name the key).
        let (expected_sha, source) = expected.unwrap_or((&build_sha, "selected prebuilt artifact"));
        deploy_verdict(Some(&build_sha), expected_sha, source, &path.display().to_string())?;
        Ok(Self { path, build_sha })
    }
}

/// What the running core's own `serving/status` hold says its engine carries. A read that fails
/// is `NoCore` only when no core process is alive; with one alive it is `Unreadable` (a busy
/// training core answers slowly), never "nothing held" (Codex on #5551).
async fn observe_engine_hold() -> continuum_cli_lifecycle::deploy_hold::HoldObservation {
    use continuum_cli_lifecycle::deploy_hold::HoldObservation;
    let snapshot = running_core_provenance("serving/status", "deploy training guard").await.and_then(|response| {
        serde_json::from_value::<continuum_core::inference::llama_server::ServingSnapshot>(response) // IPC boundary: decode the running core's serving/status response.
            .map_err(|e| format!("serving snapshot: {e}"))
    });
    match snapshot {
        Ok(snapshot) => observation_of(snapshot.held_for.as_ref()),
        Err(_) if !running_core_pids().into_iter().any(pid_alive) => HoldObservation::NoCore,
        Err(why) => HoldObservation::Unreadable(why),
    }
}

/// PURE: what a decoded hold says. A hold of unknown cause (its residency record unreadable) is
/// not a confirmed absence of work: Unreadable, deferred like a failed read (Codex on #5551).
pub(super) fn observation_of(
    held: Option<&continuum_core::inference::llama_server::serving_hold::ServingHold>,
) -> continuum_cli_lifecycle::deploy_hold::HoldObservation {
    use continuum_cli_lifecycle::deploy_hold::HoldObservation;
    use continuum_core::inference::llama_server::serving_hold::HoldCause;
    match held {
        None => HoldObservation::Free,
        Some(hold) => match (&hold.by, hold.live_work()) {
            (HoldCause::Unknown { reason }, _) => HoldObservation::Unreadable(format!("the serving hold's cause is unknown: {reason}")),
            (_, Some(work)) => HoldObservation::LiveWork(work),
            (HoldCause::Resident { .. }, None) => HoldObservation::Free,
        },
    }
}

/// Rebuild source by default, or hand off to an explicitly validated prebuilt
/// core. Both use the same leases, selective teardown and deploy verification.
///
/// A handoff that closes in-engine training admission (card 91921d0f) reopens it on every path
/// that returns an error: refused, deferred or failed before the stop, the core still runs and
/// must admit again. Past the stop the core is gone, and a restored or new one boots open.
pub(super) async fn reboot(options: RebootOptions) -> Result<(), PublishedCoreError> {
    let admission = HandoffAdmission::default();
    let result = reboot_admitted(options, &admission).await;
    if result.is_err() {
        admission.reopen().await;
    }
    result
}

async fn reboot_admitted(options: RebootOptions, admission: &HandoffAdmission) -> Result<(), PublishedCoreError> {
    // macOS: a registered launchd job owns the launch (a1bd8b58). `--service` is implied
    // by its presence, so a bare `continuum reboot` stages + kickstarts instead of
    // spawning an orphan beside the supervisor. `CONTINUUM_FROM_SOURCE` opts out.
    #[cfg(target_os = "macos")]
    let options = {
        let mut options = options;
        if !options.service && std::env::var_os("CONTINUUM_FROM_SOURCE").is_none() {
            if let Some(job) = launchd::live::job()? {
                println!("▶ launchd job {} owns the core — this reboot hands it the launch", job.domain.target());
                options.service = true;
            }
        }
        options
    };
    let mut prebuilt = match options.prebuilt {
        Some(path) => Some(PrebuiltCore::prepare(&path).await?),
        None => None,
    };
    let requested_source_build = prebuilt.is_none();
    let cli_installed = options.cli_installed;
    if options.validate_only {
        let candidate = prebuilt
            .as_ref()
            .ok_or("--validate-only requires --prebuilt")?;
        if let Some(model) = options.embedding_model {
            let model = model
                .canonicalize()
                .map_err(|e| format!("embedding model cannot be resolved: {e}"))?;
            if !model.is_file() {
                return Err("embedding model must be a local GGUF file".into());
            }
            let report = continuum_cli_lifecycle::prebuilt_validation::embedding(
                |probe| {
                    let mut command = std::process::Command::new(&candidate.path);
                    apply_core_runtime_env(&mut command)?;
                    probe.configure(&mut command);
                    Ok(command)
                },
                &model,
                &candidate.build_sha,
            )
            .await?;
            println!(
                "embedding validated: dimensions={}, load_ms={}, embed_ms={} (CPU)",
                report.dimensions, report.load_ms, report.embed_ms
            );
        }
        println!(
            "prebuilt validated: {} (build {})",
            candidate.path.display(),
            candidate.build_sha
        );
        #[cfg(windows)]
        println!("continuum-install-lease-protocol:1");
        return Ok(());
    }
    #[cfg(windows)]
    let _install_lease = take_install_lease(&PathBuf::from(home_dir()?).join(".continuum"))?;
    #[cfg(windows)]
    if let Some(expected) = &options.service_descriptor_sha {
        PreparedCoreService::query()
            .await?
            .validate_description_sha(expected)?;
    }
    let force = options.force;
    let socket = socket_path();
    // With a hand-supplied artifact the supervisor is prepared NOW, while the old core
    // still serves (its loader + provenance checked before anything stops). Without
    // one, it is prepared right after the dark build below — same check, the artifact
    // simply comes from the build instead of a hand.
    let mut service = match (options.service, prebuilt.as_ref()) {
        (true, Some(candidate)) if !options.stage_prebuilt => Some(PreparedCoreService::prepare(candidate, &socket).await?),
        _ => None,
    };
    // Training guard (task #137, Joel's consent-gate doctrine: the denial names
    // the policy AND the path). A core swap kills spawned trainer children
    // (mlx_lm.lora) and their in-process watchers — glass-boxed 2026-07-11: 41
    // jobs submitted, zero outcomes recorded, all orphaned by reboots. Live
    // training is a lease the reboot must respect, not silently revoke.
    let trainers = running_trainer_pids();
    if !trainers.is_empty() && !force {
        return Err(format!(
            "training in flight (mlx_lm pid(s) {}) — a reboot would kill it and the \
             run would be journaled killed-by-reboot. Wait for it to finish, or rerun \
             with `continuum reboot --force` if losing the run is acceptable.",
            trainers
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(",")
        ).into());
    }
    // Eval-run guard — the second lease, same shape. cognition/eval runs had NO
    // on-disk in-flight signal until 2026-08-23 (the marker in each eval world),
    // so this guard named zero runs while a MirrorCode battery was mid-task and
    // a reboot killed it unasked. pid-checked here: a marker whose core died is
    // debris for the next provision's sweep, not an in-flight run.
    let evals: Vec<String> = continuum_core::cognition::eval::in_flight_eval_runs()
        .into_iter()
        .filter(|(_, pid)| pid_alive(*pid as i32))
        .map(|(run, _)| run)
        .collect();
    if !evals.is_empty() && !force {
        return Err(format!(
            "eval run(s) in flight ({}) — a reboot would kill them mid-exam. Wait for them, \
             or rerun with `continuum reboot --force` if losing the runs is acceptable.",
            evals.join(", ")
        ).into());
    }
    // In-engine training guard (card 9beab2da; the windows 5090, 2026-10-28: the consumer's
    // handoff killed job 8b46862c at 41% while serving/status read held_for it, and the mlx_lm
    // guard above never sees an engine's own run). The running core's hold is the one owner of
    // what live work its engine carries (#5543); read even under --force, so what a forced
    // handoff ends is said. A deferral is Held: the consumer rechecks on its next pass with no
    // attempt charged. Admission is closed FIRST (card 91921d0f), so this read is final: no run is
    // admitted between it and the stop.
    let closed = admission.close().await;
    match continuum_cli_lifecycle::deploy_hold::verdict(closed, observe_engine_hold().await, force) {
        continuum_cli_lifecycle::deploy_hold::HoldVerdict::Defer(why) => return Err(PublishedCoreError::Held(why)),
        continuum_cli_lifecycle::deploy_hold::HoldVerdict::Proceed(Some(said)) => println!("⚠ {said}"),
        continuum_cli_lifecycle::deploy_hold::HoldVerdict::Proceed(None) => {}
    }
    if !evals.is_empty() {
        println!(
            "⚠ --force: rebooting over {} live eval run(s) — they die here; their worlds are \
             swept at the next provision",
            evals.len()
        );
    }
    if !trainers.is_empty() {
        println!(
            "⚠ --force: rebooting over live training (mlx_lm pid(s) {}) — the run dies \
             here and the job ledger will record killed-by-reboot at next boot",
            trainers
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );
    }
    // Snapshot the running core PIDs, then STOP EVERYTHING BEFORE THE BUILD.
    //
    // This deliberately retires the "~0 downtime" overlap (old core keeps
    // serving while the fresh binary builds). That overlap made the build
    // CONTEND with the dying core for the whole machine — a 35B llama-server
    // saturating GPU/RAM/CPU while cargo crawls beside it, both slower, to keep
    // alive a process the very next step kills (Joel, 2026-08-23: "you are
    // forced to contend for resources with something that's going away and
    // it's silly"). On a dev box the build-window downtime is free; the
    // contention is not. Stop-first also collapses the Windows special case
    // (a running .exe locks its own binary against rebuild) into the normal
    // path instead of an ordering quirk buried in start-server.sh.
    //
    // `stop()` is the ONE owner of teardown — pidfile core + split-brain
    // sweep + owned-orphan reap + serving-lane sweep — so reboot inherits every
    // lesson encoded there (two-cores-at-once 2026-08-14, the 24h orphaned
    // llama-server, the 2026-08-17 two-resident-lanes starvation) instead of
    // re-implementing a partial copy.
    let old = running_core_pids();
    if old.is_empty() {
        println!("▶ no core running — starting fresh (socket={socket})");
    } else if let Some(candidate) = &prebuilt {
        println!(
            "▶ rebooting core (socket={socket}): replacing pid(s) {} with verified prebuilt {} (build {}) — no rebuild",
            old.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(","),
            candidate.path.display(), candidate.build_sha,
        );
    } else {
        println!(
            "▶ rebooting core (socket={socket}): stopping pid(s) {} FIRST, then building on a free machine",
            old.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(",")
        );
    }
    // Publish the claim before teardown and hold it through the dark build and the swap
    // (until this function returns), so a concurrent `continuum <verb>` refuses instead of
    // autostarting the pre-swap installed image and stealing the socket (the DEPLOY
    // MISMATCH measured 2026-08-17).
    let target_sha = prebuilt
        .as_ref()
        .map(|p| p.build_sha.clone())
        .or_else(git_head_short_sha);
    let _deploy_claim = DeployClaimGuard::take(target_sha.as_deref().unwrap_or("unknown"));
    // THE BUILD RUNS DARK (Joel, 2026-10-24: "No. Take down serving while building."). The
    // warm build beside a serving core is gone: on unified memory the compiler and the
    // residents' KV share one pool, and no gate taken at start follows either of them for
    // the length of a build (the M5's Metal OOM, 2026-10-23). Serving stops gracefully, the
    // build takes the whole machine at cargo's own job count, and a build that fails puts
    // the previous core back, so a broken tree never leaves the node dark. The previous
    // build is captured HERE, while it still serves; a node with no way back refuses now.
    let build_script = if requested_source_build { locate_start_script().ok() } else { None };
    if options.service && requested_source_build && build_script.is_none() {
        return Err("reboot --service: no --prebuilt artifact and no source tree to build one; the running core stands".into());
    }
    // Windows' attended install also builds and registers the engine after the stop.
    #[cfg(windows)]
    let engine_phase = options.require_engine_receipt;
    #[cfg(not(windows))]
    let engine_phase = false;
    // A supervised handoff stages after the stop too, so it always has a way back.
    let previous = if build_script.is_some() || engine_phase || options.service {
        PreviousCore::capture(&old, options.service, &socket).await?
    } else {
        None
    };
    // Reboot deliberately does NOT fail on an unsaved module: the caller's goal is a
    // running core, and refusing to continue would leave the node down over a module that
    // could not flush. The warning is printed by `stop_with`; `stop` is the verb whose
    // exit code carries it.
    // Registration may require consent and must succeed before draining the core.
    // The unattended consumer cannot manufacture that consent; attended install
    // converges the launcher contract even when the binary SHA is already current.
    #[cfg(windows)]
    if options.require_engine_receipt {
        // Only install_core sets this flag, after entering the tracked checkout.
        let repo = std::env::current_dir().map_err(|e| e.to_string())?;
        PreparedCoreService::browser_release(&repo, false).await?;
        if service.is_some() {
            let candidate = prebuilt.as_ref().ok_or("release migration requires a verified core")?;
            service = Some(PreparedCoreService::prepare(candidate, &socket).await?);
        }
    }
    // Inactive Windows staging/provisioning may fail. Complete it before drain,
    // keeping the serving generation intact on any copy, hash or consent error.
    #[cfg(windows)]
    if options.service && service.is_none() {
        if let Some(built) = prebuilt.take() {
            let staged = PreparedCoreService::stage(&built, &socket).await?;
            service = Some(PreparedCoreService::prepare(&staged, &socket).await?);
            prebuilt = Some(staged);
        }
    }
    #[cfg(windows)]
    if options.service && !requested_source_build && !options.require_engine_receipt {
        // Staging above may have selected a new release. Promote only that
        // registered published engine, before stopping any serving generation.
        let selected = service.as_mut().ok_or("published handoff has no prepared supervisor")?;
        let release = selected.task.runtime_paths_resolve()?;
        let repo = release.installer_root()?;
        let home = continuum_core::paths::continuum_home()?;
        let root = continuum_core::inference::engine_slots::root(&home)?;
        let selection = continuum_core::inference::engine_slots::HandoffSelection::capture(&root, Path::new(&release.engine))?;
        selected.engine_selection = Some(selection);
        let promotion = async {
            let promoted = PreparedCoreService::prepare_engine(&repo, true).await?;
            if PreparedCoreService::query().await?.description != selected.task.description {
                return Err("installed release changed during published engine preparation".into());
            }
            let selection = selected.engine_selection.as_ref().ok_or("missing captured engine selection")?;
            match promoted {
                Some((description, _)) => {
                    if description != selected.task.description { return Err("installed release changed during published engine promotion".into()); }
                    selection.confirm()
                }
                None => selection.confirm_preserved(),
            }
        }.await;
        if let Err(failed) = promotion { return Err(selected.unwind_engine_promotion(failed).await.into()); }
    }
    // Explicit core-only prebuilt runs borrow the installed runtime closure.
    // Resolve and pin it BEFORE drain: a missing companion is not permission
    // to run source builders after the current core has stopped.
    #[cfg(windows)]
    let prebuilt_runtime = if !requested_source_build && !options.service {
        require_prebuilt_runtime_medium(unelevated_service::token_integrity(), std::env::var_os(unelevated_service::UNELEVATED_MARKER).is_some())?;
        if let Some(task) = PreparedCoreService::query_optional().await? {
            let envelope = task.installed.as_ref().ok_or("core-only prebuilt launch requires a sealed installed runtime; update the legacy installer before stopping the core")?;
            let home = continuum_core::paths::continuum_home()?;
            let payload = continuum_core::paths::payload_root(&home)?;
            let sid = supervisor_install::caller_sid().await?;
            let text = continuum_cli_lifecycle::installed_release::read_receipt(Path::new(&envelope.active_release))?;
            let selected = continuum_cli_lifecycle::installed_release::validate_receipt(&text, &home, &payload, &sid)?;
            if selected.release.socket != socket { return Err("installed runtime socket differs from the requested prebuilt handoff".into()); }
            PreparedCoreService::shell()?;
            Some(selected)
        } else {
            let home = continuum_core::paths::continuum_home()?;
            match std::fs::symlink_metadata(home.join("install-active.json")) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Ok(_) => return Err("an installed runtime receipt exists without its registered task; refusing bare prebuilt fallback".into()),
                Err(error) => return Err(format!("cannot inspect installed runtime receipt before prebuilt launch: {error}").into()),
            }
        }
    } else { None };
    #[cfg(windows)]
    let _prebuilt_startup = if !requested_source_build && !options.service {
        Some(supervisor_install::maintenance::StartupPermit::acquire(&continuum_core::paths::continuum_home()?).await?)
    } else { None };
    let mut dark = DarkWindow::start();
    if let Err(failed) = stop_with_authority(true, options.operator_present).await {
        #[cfg(windows)]
        if let Some(selected) = &service { return Err(selected.unwind_engine_promotion(failed).await.into()); }
        return Err(failed.into());
    }
    dark.stopped();
    // Everything that can fail between the stop and the launch is one phase: a failure
    // anywhere in it restarts the previous build (`PreviousCore::restore`) and reports both.
    let service_mode = options.service;
    let dark_phase = async {
        // UNATTENDED Windows deploy (card d5584dfc, option (b)): build the pinned engine into the
        // idle slot and promote `current`, the same unprivileged sequence the bash installer runs,
        // now with serving stopped like every other build. It never re-registers the scheduled
        // task (an elevation nobody is there to answer), and an engine that does not build never
        // fails the core deploy: the lanes keep the engine they have. `install` takes the
        // attended path below. Published/prepared handoffs carry their verified engine closure,
        // so an explicit prebuilt artifact never enters this source-building refresh.
        #[cfg(windows)]
        if service_mode && prebuilt.is_none() && !engine_phase {
            match PreparedCoreService::installer_root().await {
                Ok(repo) => match PreparedCoreService::prepare_engine(&repo, false).await {
                    Ok(Some((_, engine))) => deploy_note(&format!("▶ verified engine slot promoted ({}); the next core converges its lanes onto it", engine.display())),
                    Ok(None) => {}
                    Err(e) => deploy_note(&format!("⚠ engine not updated this deploy ({e}); the core deploys on the engine it has")),
                },
                Err(e) => deploy_note(&format!("⚠ engine not updated this deploy (installer root unavailable: {e})")),
            }
        }
        // Attended Windows install: the engine is built and its receipt required.
        #[cfg(windows)]
        let prepared_engine = match engine_phase {
            true => {
                let repo = std::env::current_dir().map_err(|e| e.to_string())?;
                PreparedCoreService::prepare_engine(&repo, false).await?.map(|prepared| (repo, prepared))
            }
            false => None,
        };
        let mut built = None;
        if let Some(script) = &build_script {
            let mut cmd = std::process::Command::new(locate_bash()?);
            cmd.arg(script);
            apply_core_runtime_env(&mut cmd)?;
            if let CliSelfBuild::Skip { .. } = cli_self_build(std::env::consts::OS) {
                cmd.env("CONTINUUM_SKIP_SELF_BUILD", running_cli_image());
            }
            println!("▶ building dark: serving is stopped, the build takes the whole machine (build-only pass of {})", script.display());
            let artifact = build_dark(cmd).await?;
            // A shared checkout can advance while Cargo is reading it. A binary reporting
            // either endpoint is not proof of a coherent source tree (2026-10-01).
            let after = git_head_short_sha();
            continuum_cli_lifecycle::deploy_provenance::source_build_verdict(
                target_sha.as_deref(), after.as_deref(), &artifact.build_sha,
            )?;
            dark.built();
            println!("✓ dark build validated: {} (build {})", artifact.path.display(), artifact.build_sha);
            built = Some(artifact);
        }
        #[cfg(windows)]
        if let Some((repo, (original, engine))) = &prepared_engine {
            if PreparedCoreService::query().await?.description != *original {
                return Err("installed release changed during preparation; the previous core is restored".into());
            }
            let repo_arg = repo.to_string_lossy().replace('\'', "''");
            let directory = engine
                .parent()
                .ok_or("prepared engine has no directory")?
                .to_string_lossy()
                .replace('\'', "''");
            let drift = PreparedCoreService::powershell(&format!(
                "$ErrorActionPreference='Stop'; . '{repo_arg}/tools/scripts/lib/windows-service.ps1'; . '{repo_arg}/tools/scripts/lib/win-modules.ps1'; Get-CoreEngineDrift -Directory '{directory}' -Requirement (Get-CoreEngineRequirement -RepoRoot '{repo_arg}')"
            )).await?;
            if !drift.is_empty() {
                return Err(format!("prepared engine changed during the dark build: {drift}").into());
            }
            let candidate = built
                .as_ref()
                .or(prebuilt.as_ref())
                .ok_or("engine handoff requires a verified Core artifact")?;
            PrebuiltCore::prepare(&candidate.path).await?;
            // Registration can fail even after Task Scheduler accepted the new engine
            // (Windows install, 2026-09-28). Register-CoreServiceRelease validates and
            // registers, but never launches; a failure here restarts the previous core.
            PreparedCoreService::register_engine(repo, original, engine).await?;
        }
        if let Some(artifact) = built {
            prebuilt = Some(artifact);
        }
        // A built artifact is STAGED into the slot the supervisor is bound to, then prepared:
        // the supervisor validates the exact artifact path in its descriptor, and a build lands
        // in the cargo target dir (the 5090's first unattended deploy, 2026-09-19, refused a
        // validated 1,145 s build at the door for want of this copy).
        if service_mode && service.is_none() {
            if let Some(built) = prebuilt.take() {
                let staged = PreparedCoreService::stage(&built, &socket).await?;
                service = Some(PreparedCoreService::prepare(&staged, &socket).await?);
                prebuilt = Some(staged);
            }
        }
        #[cfg(windows)]
        return Ok::<_, PublishedCoreError>(prepared_engine);
        #[cfg(not(windows))]
        Ok::<_, PublishedCoreError>(())
    }.await;
    #[cfg(windows)]
    let prepared_engine = match dark_phase {
        Ok(done) => done,
        Err(failed) => return Err(restore_or_report(previous, failed, &old).await),
    };
    #[cfg(not(windows))]
    if let Err(failed) = dark_phase {
        return Err(restore_or_report(previous, failed, &old).await);
    }
    #[cfg(windows)]
    if prepared_engine.is_some() {
        // A source build is still a Cargo artifact until the staging above. Do
        // not mark the service prepared before that copy, or it would skip
        // staging and compare the registered slot against the Cargo path.
        let candidate = prebuilt
            .as_ref()
            .ok_or("engine handoff requires a verified Core artifact")?;
        service = Some(PreparedCoreService::prepare(candidate, &socket).await?);
    }
    // Keep the launcher's wait as the honesty check that teardown actually took.
    let source = prebuilt
        .as_ref()
        .map_or(LaunchSource::FromSource, LaunchSource::Prebuilt);
    #[cfg(windows)]
    let source = if !requested_source_build && !options.service {
        LaunchSource::ExplicitPrebuilt(prebuilt.as_ref().ok_or("explicit prebuilt artifact was lost before launch")?, prebuilt_runtime.as_ref().map(|runtime| &runtime.release))
    } else { source };
    let secs = match service {
        Some(service) => service.launch(&old).await?,
        // A supervised launch rolls back inside `launch`; a direct one restarts the copy kept.
        None => match launch_core(&old, source).await {
            Ok(secs) => secs,
            Err(failed) => return Err(restore_or_report(previous, failed.into(), &old).await),
        },
    };
    // Deploy-verification (#194): a new core is up — but is it the FRESHLY-BUILT one? If
    // start-server.sh's build was a stale cache no-op or silently failed, an OLD binary would
    // answer on the same socket and this reboot would report success while running dead code.
    // NO success line before provenance is proven ("core ready" without provenance is a false
    // deploy receipt — the 2026-08-01 Windows-node incident): announce liveness neutrally,
    // then verify, and let "✅ deploy verified" be the ONLY checkmark a reboot prints.
    println!(
        "core answering (socket={socket}) after ~{secs}s — verifying deploy provenance (#194)"
    );
    println!("▶ {}", dark.receipt());
    // Did THIS reboot replace the installed CLI? Only when it went through the build script
    // (a source tree exists — the same condition `plan_launch` uses to pick `Script` for a
    // FromSource launch) AND the platform allows a self-build. Both terms matter: on an
    // installed node with no checkout nothing was rebuilt, and on Windows `cli_self_build`
    // deliberately skips. Getting this wrong in either direction re-creates the noise this
    // flag exists to remove, or hides a genuinely stale CLI behind a reassuring handoff line.
    let rebuilt_cli = cli_replaced_this_run(
        cli_installed,
        requested_source_build,
        locate_start_script().is_ok(),
        matches!(cli_self_build(std::env::consts::OS), CliSelfBuild::Rebuild),
    );
    verify_deployed_build_against(rebuilt_cli, prebuilt.as_ref()).await.map_err(Into::into)
}
