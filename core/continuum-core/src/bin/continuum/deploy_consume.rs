//! `deploy-consume`: the deploy consumer, its log and per-pass flow.

use super::*;
use continuum_cli_lifecycle::deploy_health::RefusalKind;

/// The consumer's log — the only place its output can land, since the scheduled task
/// has no stdout. Set once by `deploy-consume`; read by the dark build to redirect
/// the build's own output there too.
pub(super) static DEPLOY_LOG: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// The parent's OWN notes: `.append(true)` is the atomic append (FILE_APPEND_DATA
/// positions every write at the end, whoever else is writing); a Rust `writeln!` is
/// fine with it. Only a CHILD needs `open_log_for_child` (Fable, #4233).
pub(super) fn deploy_log_file() -> Option<std::fs::File> {
    let path = DEPLOY_LOG.get()?;
    std::fs::OpenOptions::new().create(true).append(true).open(path).ok()
}

/// A log handle a CHILD can be handed as its stdout/stderr. NOT `.append(true)`:
/// on Windows that opens the handle with FILE_APPEND_DATA and no FILE_WRITE_DATA,
/// and an MSYS bash handed such a handle as its stdout exits 1 before running a
/// line — every unattended deploy on the 5090 died in one second with an EMPTY
/// log, "warm build exited exit code: 1" and nothing else, for a day (2026-09-19).
/// Full write access, positioned at the end: the same append, in a handle every
/// child accepts.
pub(super) fn open_log_for_child(path: &Path) -> std::io::Result<std::fs::File> {
    use std::io::Seek;
    let mut file = std::fs::OpenOptions::new().create(true).write(true).open(path)?;
    file.seek(std::io::SeekFrom::End(0))?;
    Ok(file)
}

/// Say it on stdout AND in the log, stamped.
pub(super) fn deploy_note(line: &str) {
    println!("{line}");
    record_deploy_note(line);
}

/// Protocol commands reserve stdout for their result; progress remains visible
/// on stderr and in the same deployment log as interactive consumption.
pub(super) fn prepare_note(line: &str) {
    eprintln!("{line}");
    record_deploy_note(line);
}

pub(super) fn record_deploy_note(line: &str) {
    if let Some(mut f) = deploy_log_file() {
        use std::io::Write;
        let stamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
        let _ = writeln!(f, "{stamp} {line}");
    }
}

/// Whether the consumer's reboot goes through `--service`: only Windows needs the flag
/// (the supervisor handoff); macOS promotes a bare reboot to its launchd job itself, and
/// `reboot` refuses `--service` everywhere else. Pure over the OS name.
pub(super) fn consumer_uses_service(os: &str) -> bool {
    os == "windows"
}

/// Missing capability is a rollout refusal, never permission to clear a condition.
pub(super) fn retry_launcher(ping: &Value) -> Result<PathBuf, String> {
    use continuum_cli_lifecycle::prebuilt_artifact::CONSUMER_LEASE_PROTOCOL;
    if ping.get("deployConsumerProtocol").and_then(Value::as_u64) != Some(u64::from(CONSUMER_LEASE_PROTOCOL)) {
        return Err("resource retry requires a running core with the cooperating consumer lease protocol; finish the supported rollout and quiesce older consumers first".into());
    }
    ping.get("deployConsumerCli").and_then(Value::as_str).map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| "resource retry cannot prove the active launcher CLI (external, legacy or unreadable ownership)".into())
}

pub(super) async fn verify_consumer_protocol(cli: &Path) -> Result<(), String> {
    let mut command = supervisor_install::quiet_command(cli);
    command.args(["deploy-consume", "--protocol"]).stdin(Stdio::null());
    let mut command = tokio::process::Command::from(command);
    command.kill_on_drop(true);
    // Same bounded startup diagnostic as the existing live-core provenance probe.
    let output = tokio::time::timeout(std::time::Duration::from_secs(30), command.output())
        .await.map_err(|_| "consumer protocol probe exceeded the existing 30s provenance bound".to_string())?
        .map_err(|e| format!("cannot verify consumer protocol at {}: {e}", cli.display()))?;
    let expected = continuum_cli_lifecycle::prebuilt_artifact::CONSUMER_LEASE_PROTOCOL.to_string();
    if !output.status.success() || String::from_utf8_lossy(&output.stdout).trim() != expected {
        return Err(format!("registered consumer {} does not declare the cooperating lease protocol; finish its supported rollout before retry", cli.display()));
    }
    Ok(())
}

#[cfg(windows)]
pub(super) fn retry_task_matches(report: &supervisor_install::TaskReport, envelope: &continuum_cli_lifecycle::installed_release::Envelope, mode: &str, sid: &str) -> bool {
    report.present && report.enabled && report.actions == 1
        && report.command.eq_ignore_ascii_case(&envelope.bootstrap)
        && report.arguments == envelope.arguments(mode) && report.user_sid == sid
}

#[cfg(windows)]
pub(super) async fn verify_registered_consumer(root: &Path) -> Result<continuum_cli_lifecycle::installed_release::ValidatedRelease, String> {
    use continuum_cli_lifecycle::installed_release;
    let sid = supervisor_install::caller_sid().await?;
    let program_files = PathBuf::from(std::env::var_os("ProgramFiles").ok_or("ProgramFiles is unset")?);
    let mut selected = None;
    let mut selected_text = None;
    for (task, mode) in [(supervisor_install::CORE_TASK, "core"), (supervisor_install::DEPLOY_TASK, "deploy")] {
        let report = supervisor_install::task_report(task).await?;
        let envelope = installed_release::envelope(&report.description)?
            .ok_or_else(|| format!("{task} has a legacy/unreadable launcher; complete supervisor rollout before retry"))?;
        envelope.validate(root, &program_files, &sid)?;
        if !retry_task_matches(&report, &envelope, mode, &sid) {
            return Err(format!("{task} does not match its registered supervisor authority; preserve resource conditions"));
        }
        let text = installed_release::read_receipt(Path::new(&envelope.active_release))?;
        if selected_text.as_ref().is_some_and(|previous| previous != &text) {
            return Err("registered release changed while verifying consumer rollout".into());
        }
        if selected.is_none() {
            let payload = continuum_core::paths::payload_root(root)?;
            let release = installed_release::validate_receipt(&text, root, &payload, &sid)?;
            verify_consumer_protocol(Path::new(&release.release.cli)).await?;
            selected = Some(release); // retain sealed payload pins through compare-and-clear
            selected_text = Some(text);
        }
    }
    selected.ok_or_else(|| "registered consumer release is absent".into())
}

/// The consumer follows the REQUEST, not a tip it once read: while it waits on CI for one
/// tip the tracker keeps writing newer requests beside it, and on canary GitHub keeps the
/// running build and only the newest QUEUED push (older queued pushes are dropped and show
/// as `cancelled`; #4729 keeps cancel-in-progress PR-only). Waiting for an artifact that
/// will never exist held the M5 at a stale core for two hours on 2026-10-04 while three
/// built tips went by. A pass that finds its request superseded starts over on the new one.
/// How a refused handoff is charged to its build.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum HandoffRefusal {
    /// A destination resource condition, kept durably until its explicit retry.
    Deferred(continuum_cli_lifecycle::prebuilt_artifact::ResourceDeferral),
    /// Rechecked on the next pass: no attempt charged, nothing kept. A training hold (card
    /// 9beab2da) releases on its own, so it is never a failure or a durable deferral.
    Blocked(String),
    /// A handoff failure, charged to the build (three give up on it).
    Failed(String),
}

/// PURE: how `error`, refusing the handoff of the core at `preparation`, is charged.
pub(super) fn handoff_refusal(error: PublishedCoreError, preparation: PathBuf) -> HandoffRefusal {
    match error {
        PublishedCoreError::OperationalBlocked(reason) | PublishedCoreError::CapabilityDeferred(reason) => {
            HandoffRefusal::Deferred(continuum_cli_lifecycle::prebuilt_artifact::ResourceDeferral {
                condition: continuum_cli_lifecycle::prebuilt_artifact::ResourceCondition::OperationalUncertainty,
                path: preparation,
                projected_bytes: 0,
                reason,
            })
        }
        PublishedCoreError::ResourceDeferred(deferral) => HandoffRefusal::Deferred(deferral),
        PublishedCoreError::SourcePending(reason) | PublishedCoreError::Held(reason) => HandoffRefusal::Blocked(reason),
        PublishedCoreError::Artifact(reason) => HandoffRefusal::Failed(reason),
    }
}

pub(super) async fn deploy_consume(options: DeployConsumeOptions) -> Result<(), String> {
    if options.protocol {
        println!("{}", continuum_cli_lifecycle::prebuilt_artifact::CONSUMER_LEASE_PROTOCOL); // boundary: read-only CLI capability wire for retry rollout admission
        return Ok(());
    }
    // The parent's launch permit ends at spawn. Reacquire in the consumer and
    // retain custody for its whole pass so a delayed Scheduler/detached child
    // cannot begin work inside a maintenance window.
    #[cfg(windows)]
    let _maintenance_permit = supervisor_install::maintenance::StartupPermit::acquire_deploy(&continuum_root()?)?;
    let attempts_path = consume_attempts_path()?;
    let _consumer_lease = ConsumeLedger::take_lease(&attempts_path)?;
    if let Some((build, digest)) = options.retry_resource {
        let root = continuum_root()?;
        #[cfg(windows)]
        let _install_lease = take_install_lease(&root)?;
        // Supported launchers must have completed the cooperating-protocol rollout.
        // Previously launched old consumers must be quiesced by their lifecycle owner;
        // this lease cannot exclude arbitrary manually launched noncooperating binaries.
        let cli = retry_launcher(&running_core_provenance("deploy/consumer-capability", "resource retry rollout").await?)?;
        verify_consumer_protocol(&cli).await?;
        #[cfg(windows)]
        let _registered_release = verify_registered_consumer(&root).await?;
        let claim_path = continuum_cli_lifecycle::deploy_claim::claim_path(&root);
        if claim_path.exists() && continuum_cli_lifecycle::deploy_claim::read(&root).is_none() {
            return Err("deploy claim is unreadable; preserve it and resolve ownership before resource retry".into());
        }
        let owner_active = continuum_cli_lifecycle::deploy_claim::read(&root).is_some_and(|claim| pid_alive(claim.pid));
        ConsumeLedger::retry_resource(&attempts_path, &build, &digest, owner_active)?;
        println!("resource retry authorized for exact build {build}; other conditions and failure counts preserved; normal consumer will recheck admission");
        return Ok(());
    }
    loop {
        match deploy_consume_pass().await? {
            PassEnd::Superseded(new_tip) => {
                deploy_note(&format!("deploy-consume: request superseded by {new_tip}; starting over on it"));
                continue;
            }
            PassEnd::Done => return Ok(()),
            PassEnd::ResourceBlocked(why) => return Err(why),
        }
    }
}

/// How a consume pass ended. Typed, so "start over" can never be forged by an error text
/// (Cormac's condition on #4750): a superseded pass charges no attempt to the old tip.
pub(super) enum PassEnd {
    Done,
    ResourceBlocked(String),
    Superseded(String),
}

pub(super) async fn deploy_consume_pass() -> Result<PassEnd, String> {
    let request_path = deploy_request_path()?;
    let _ = DEPLOY_LOG.set(
        continuum_core::paths::continuum_root()
            .join("logs")
            .join("deploy-consume.log"),
    );
    if let Some(dir) = DEPLOY_LOG.get().and_then(|p| p.parent()) {
        let _ = std::fs::create_dir_all(dir);
    }
    let tip = read_deploy_request_tip(&request_path);
    let running = running_build_sha().await;
    let repo = tracked_repo_dir()?;
    // An operator's work, judged by the one predicate the tracker reads too: a moved submodule
    // pointer is this consumer's to align (below), a change inside one is not (card a1e1af51).
    let dirty = continuum_git::submodules::has_operator_work(&git_in(&repo, &continuum_git::submodules::STATUS_ARGS)?);
    // The deploy claim is the tracker's own input (`deploy_claim::in_flight`). A LIVE owner
    // excludes this consumer whether or not its claim expired: its checkout is its own, and
    // checking out a new tip under it dooms its build (card 634f644d). Only a dead owner's
    // claim is swept (by `reboot` itself).
    let gate = continuum_root()
        .map(|root| continuum_cli_lifecycle::deploy_claim::in_flight(&root, continuum_base::time::EpochMs::now().millis()))
        .ok(); // no root = no claim file = nothing in flight
    if let Some(continuum_cli_lifecycle::deploy_claim::DeployGate::Abandoned { pid, age_ms, .. }) = gate
        .as_ref()
        .filter(|g| g.excludes_deploy())
    {
        deploy_note(&format!(
            "deploy owner pid {pid} is alive but made no progress ({}s old): not checking out under \
             it. Sample it (`sample {pid}`), then stop it, to release the deploy tree.",
            age_ms / 1000
        ));
    }
    let build_in_flight = gate.as_ref().is_some_and(|g| g.excludes_deploy());
    let attempts_path = consume_attempts_path()?;
    let mut ledger = ConsumeLedger::read(&attempts_path)?;
    if running.as_deref().is_some_and(|sha| ledger.observe_running(sha)) {
        ledger.save(&attempts_path)?;
    }
    // The tip's build key, read from the same checkout the tracker fetched into.
    let tip_build_key = tip.as_deref().and_then(|t| build_key_in(&repo, t));
    if let (Some(request), Some(build)) = (tip.as_ref(), tip_build_key.as_ref()) {
        let selection = continuum_cli_lifecycle::deploy_health::DeploySelection {
            request: request.clone(), build: build.clone(),
        };
        if ledger.selection.as_ref() != Some(&selection) {
            ledger.selection = Some(selection);
            ledger.save(&attempts_path)?;
        }
    }
    // Give-up is judged on the TIP's own build only: an older core that would not land
    // here is skipped below, never charged to the request.
    let prior_failures = tip
        .as_deref()
        .map(|t| ledger.failures(tip_build_key.as_deref().unwrap_or(t))) // unwrap_or: no build key means the tip names its own core
        .unwrap_or(0); // unwrap_or: no request = nothing to have failed
    let verdict = consume_verdict(
        tip.as_deref(),
        tip_build_key.as_deref(),
        running.as_deref(),
        dirty,
        build_in_flight,
        prior_failures,
    );
    if let Some(request) = tip.as_deref() {
        let refusal = match &verdict {
            ConsumeVerdict::RefuseDirty => Some((RefusalKind::CheckoutDirty,
                "tracked checkout has uncommitted work; consumer preserves it".to_string())),
            ConsumeVerdict::GaveUp => Some((RefusalKind::AttemptsExhausted,
                format!("requested build exhausted {prior_failures} attempts"))),
            _ => None,
        };
        if let Some((kind, reason)) = refusal {
            let build = match tip_build_key.as_deref() { Some(key) => key, None => request };
            ledger.record_refusal(request, build, kind, &reason);
            ledger.save(&attempts_path)?;
        }
    }
    deploy_note(&format!(
        "deploy-consume: request={} running={} dirty={dirty} in_flight={build_in_flight} prior_failures={prior_failures} → {verdict:?}",
        tip.as_deref().unwrap_or("none"), // unwrap_or: display only — "none" is the honest word for no request
        running.as_deref().unwrap_or("none") // unwrap_or: display only — no core answering prints as "none"
    ));
    match verdict {
        ConsumeVerdict::NothingOwed | ConsumeVerdict::AlreadyRunning | ConsumeVerdict::BuildInFlight => Ok(PassEnd::Done),
        ConsumeVerdict::GaveUp => Err(format!(
            "deploy-consume: tip {} failed {prior_failures} times on this box — not retrying; \
             the tracker's deploy.stranded is the receipt, and a NEW tip resets this",
            tip.as_deref().unwrap_or("?") // unwrap_or: GaveUp is only returned with a tip present; "?" would mean the verdict lied
        )),
        ConsumeVerdict::RefuseDirty => Err(format!(
            "deploy-consume: {} has uncommitted work — a consumer never stashes an operator's \
             tree; commit or stash it and the next tick deploys",
            repo.display()
        )),
        ConsumeVerdict::Deploy => {
            let tip = tip.unwrap_or_default(); // unwrap_or_default: Deploy is only returned with a tip present
            // A lock left by a git that died mid-write refused every checkout for hours and
            // spent the tip's attempts (card 677437fa). Judge it before the attempt: a stale
            // one is cleared with a receipt, a possibly-live one is named and waited on.
            if !settle_index_lock(&repo)? {
                return Ok(PassEnd::Done);
            }
            // The build this pass deploys: the tip's, or the newest CI has published when the
            // tip's is not out yet. The log names THIS, never the request in its place: on
            // 2026-10-07 the pass deployed 69a2c2b96 for request d2cf23221 and logged
            // "d2cf23221 handed off", a build that was not running (card 4752fea6).
            let mut deployed = tip.clone();
            // The build this pass SELECTED, set the moment CI's answer is known and before
            // any fallible handoff step (clearing the wait marker, the checkout, the
            // submodules, the reboot), so a failure in any of them is charged to it, never
            // to the tip (Astra review of #5011).
            let mut selected: Option<String> = None;
            let attempt = async {
                // The deploy claim, from BEFORE the checkout to the handoff (Fable on #4702).
                // The CI wait below can run for hours, and without a claim nothing marks
                // this deploy in flight: the actuator re-launches a consumer once the request
                // looks stranded (1.5x the last deploy time), and that second consumer would
                // check the tree out under this one and reboot alongside it. `reboot` re-takes
                // the same claim for this pid, and its guard releases it at the handoff.
                let _claim = DeployClaimGuard::take(&tip);
                git_in(&repo, &["fetch", "--quiet", "origin"])?;
                git_in(&repo, &["checkout", "--quiet", "--detach", &tip])?;
                // A checkout moves gitlinks, not submodule trees: every present submodule
                // checkout follows the tip's pin, registered or not, and none is cloned
                // (2026-09-27 #4437; card a1e1af51, enrico's unregistered engine checkout).
                continuum_git::submodules::align(&repo, &|args| git_in(&repo, args))?;
                // The dark build locates tools/scripts/start-server.sh by walking UP FROM
                // THE CWD, and a scheduled task starts in System32 — the same wall the
                // Macs' launchd tracker hit ("under launchd the cwd is /; continuum reboot
                // then finds no source"). The consumer knows the repo; it stands in it.
                std::env::set_current_dir(&repo)
                    .map_err(|e| format!("deploy-consume: cannot enter {}: {e}", repo.display()))?;
                let service = consumer_uses_service(std::env::consts::OS);
                // Only a verified CI artifact reaches the handoff. Missing or rejected
                // artifacts refuse this attempt before touching the running core.
                // CI publishes a core for the commit that last touched a build input; a
                // docs-only tip is served by that commit's core (card 9080ffb0).
                // Not only the tip's core: the newest one CI has PUBLISHED since the running
                // core (card d1db1a83). On a busy canary the tip's build has not started
                // while an older one is out, and waiting only on the tip never deployed.
                let newest_key = tip_build_key.clone().unwrap_or_else(|| tip.clone()); // unwrap_or_else: no build key means the tip names its own core
                let candidates = ledger.admissible(deploy_candidates(&repo, running.as_deref(), &tip, &newest_key), &newest_key);
                let ci_core = ci_core_for(&repo, &candidates, &request_path, &tip, &mut ledger, &attempts_path).await;
                if let CiCore::Built { key, .. } = &ci_core {
                    selected = Some(key.clone());
                    deployed = key.clone();
                }
                // The wait is over, whatever it found: from here the deploy promotes the
                // engine and swaps the core, which the claim must block. A marker that cannot
                // be removed ends this attempt rather than let a swap read as a wait.
                if let Ok(root) = continuum_root() {
                    continuum_cli_lifecycle::deploy_claim::clear_waiting(&root, std::process::id() as i32)
                        .map_err(|e| format!("deploy-consume: cannot clear the wait marker: {e}"))?;
                }
                let prebuilt = match ci_core {
                    CiCore::Built { core, key } => {
                        if key != newest_key {
                            // An older published core: stand the checkout on ITS commit, so
                            // the reboot's build-sha check and deploy-verify compare like with
                            // like. The next pass moves on toward the tip.
                            git_in(&repo, &["checkout", "--quiet", "--detach", &key])?;
                            continuum_git::submodules::align(&repo, &|args| git_in(&repo, args))?;
                            deploy_note(&format!(
                                "deploy-consume: {key} is the newest core CI has published (the request is {tip}); deploying it now"
                            ));
                        }
                        // CI's core for this pass: an older published one, or the build-key
                        // commit's core serving a docs-only tip (card 9080ffb0).
                        core
                    }
                    CiCore::ResourceBlocked(why) => return Ok(PassEnd::ResourceBlocked(why)),
                    CiCore::Refused(why) => return Err(why),
                    CiCore::Superseded(new_tip) => return Ok(PassEnd::Superseded(new_tip)),
                };
                let preparation_path = prebuilt.clone();
                let companion_result = install_ci_companions(&repo, &prebuilt).await;
                deploy_note(&format!(
                    "▶ deploy-consume: {} — reboot into {deployed}{} --prebuilt (the CI build)",
                    repo.display(),
                    if service { " --service" } else { "" }
                ));
                // A downloaded core lives in the artifact cache; launchd execs only its slot,
                // so it is STAGED into the slot after the stop, like a dark build's artifact.
                let stage_prebuilt = true;
                // install_ci_companions above already put this core's CLI on PATH.
                let cli_installed = true;
                let preparation = match companion_result {
                    Ok(()) => reboot(RebootOptions { service, prebuilt: Some(prebuilt), stage_prebuilt, cli_installed, ..Default::default() }).await,
                    Err(error) => Err(error),
                };
                match preparation.map_err(|error| handoff_refusal(error, preparation_path)) {
                    Ok(()) => {}
                    Err(HandoffRefusal::Deferred(condition)) => {
                        let digest = match ConsumeLedger::condition_digest(&condition) {
                            Ok(digest) => digest,
                            Err(reason) => return Ok(PassEnd::ResourceBlocked(reason)),
                        };
                        let reason = format!("prepared build {deployed} awaits destination resource admission: {}; explicit retry: continuum deploy-consume --retry-resource {deployed} {digest}", condition.reason);
                        ledger.resource_deferrals.insert(deployed.clone(), condition);
                        if let Err(reason) = ledger.save(&attempts_path) { return Ok(PassEnd::ResourceBlocked(reason)); }
                        return Ok(PassEnd::ResourceBlocked(reason));
                    }
                    Err(HandoffRefusal::Blocked(reason)) => return Ok(PassEnd::ResourceBlocked(reason)),
                    Err(HandoffRefusal::Failed(reason)) => return Err(reason),
                }
                Ok(PassEnd::Done)
            }
            .await;
            match &attempt {
                Ok(PassEnd::Done) => {
                    if let Some(build) = &selected {
                        ledger.landed(build); // a pass that handed off selected a build
                    }
                    ledger.save(&attempts_path)?;
                    let request = if deployed == tip { String::new() } else { format!(" (request {tip})") };
                    deploy_note(&format!("✓ deploy-consume: {deployed} handed off{request}"));
                }
                Ok(PassEnd::ResourceBlocked(why)) => {
                    let build = match selected.as_deref().or(tip_build_key.as_deref()) {
                        Some(build) => build,
                        None => &tip, // No artifact selected: evidence belongs to the request itself.
                    };
                    ledger.record_refusal(&tip, build, RefusalKind::ResourceAdmission, why);
                    ledger.save(&attempts_path)?;
                    deploy_note(&format!("deploy resource custody refused: {why}; no build failure charged"));
                }
                Ok(PassEnd::Superseded(_)) => {} // not this tip's failure; the outer loop follows the new request
                Err(why) => {
                    // Charged to the build this pass tried: the tip's, or the older published
                    // core it fell back to. A failure before any build was chosen is the tip's.
                    let tip_build = tip_build_key.clone().unwrap_or_else(|| tip.clone()); // unwrap_or_else: no build key means the tip names its own core
                    let (tried, failures) = ledger.charge_failure(selected.as_deref(), &tip_build);
                    ledger.record_refusal(&tip, &tried, RefusalKind::HandoffFailed, why);
                    ledger.save(&attempts_path)?;
                    deploy_note(&format!(
                        "deploy-consume: attempt {failures} of {CONSUME_MAX_ATTEMPTS} for build {tried} (request {tip}) failed: {why}"
                    ));
                }
            }
            attempt
        }
    }
}

/// The core to deploy, downloaded, verified and extracted, for `reboot --prebuilt` (card
/// 50ca737e): the newest of `candidates` (build keys newer than the running core, newest
/// first, the tip's key at their head) that CI has PUBLISHED (card d1db1a83). While none
/// is, this WAITS, in this detached consumer, rather than returning: the actuator
/// re-launches a consumer only after the request is stranded (1.5x the last deploy time),
/// so a "come back later" would idle the node for hours and spend an actuation. The CI
/// budget is the tip's. `Refused` leaves the running core intact: an unavailable or
/// invalid artifact never authorizes a source build. `Superseded` = the request
/// (`request_path`) no longer names `requested_tip`:
/// the caller lists candidates again against the new tip.
pub(super) async fn ci_core_for(repo: &Path, candidates: &[String], request_path: &Path, requested_tip: &str, ledger: &mut ConsumeLedger, ledger_path: &Path) -> CiCore {
    use continuum_cli_lifecycle::prebuilt_artifact::{select_published_core, platform_key, when_artifact_missing, MissingArtifact, PublishedCoreError, recheck_resource_deferral};
    let platform = platform_key(std::env::consts::OS, std::env::consts::ARCH);
    let newest = &candidates[0]; // deploy_candidates always puts the tip's key first
    let mut tick = tokio::time::interval(Duration::from_secs(60));
    loop {
        tick.tick().await;
        if let Some(new_tip) = continuum_cli_lifecycle::prebuilt_artifact::request_superseded(
            requested_tip,
            read_deploy_request_tip(request_path).as_deref(),
        ) {
            return CiCore::Superseded(new_tip);
        }
        let previous_conditions = ledger.resource_deferrals.clone();
        for key in candidates {
            let Some(deferred) = ledger.resource_deferrals.get(key).cloned() else { continue };
            if deferred.condition != continuum_cli_lifecycle::prebuilt_artifact::ResourceCondition::VolumeCapacity {
                continue; // A volume sample cannot authorize retry of an unmeasured filesystem refusal.
            }
            let measured = continuum_core::paths::artifacts::check_capacity(&deferred.path, deferred.projected_bytes).await;
            let mut pending = Some((key.clone(), deferred));
            if let Some(MissingArtifact::Refuse(why)) = recheck_resource_deferral(&mut pending, measured) {
                return CiCore::Refused(why);
            }
            if let Some((_, updated)) = pending { ledger.resource_deferrals.insert(key.clone(), updated); }
            else { ledger.resource_deferrals.remove(key); }
        }
        if ledger.resource_deferrals != previous_conditions {
            if let Err(why) = ledger.save(ledger_path) { return CiCore::ResourceBlocked(why); }
        }
        let missing = match platform {
            None => when_artifact_missing(None, 0),
            Some(p) => {
                let source = CiPublishedCore { repo, platform: p, deferred: &ledger.resource_deferrals };
                match select_published_core(candidates, &source).await {
                    None => when_artifact_missing(Some(p), tip_age_secs(repo, newest)),
                    Some((key, artifact)) => match artifact {
                        Ok(Some(core)) => return CiCore::Built { core, key: key.to_string() },
                        // published a moment ago and gone now: the next tick asks again
                        Ok(None) => when_artifact_missing(Some(p), tip_age_secs(repo, newest)),
                        Err(PublishedCoreError::ResourceDeferred(deferred)) => {
                            let digest = match ConsumeLedger::condition_digest(&deferred) {
                                Ok(digest) => digest,
                                Err(why) => return CiCore::ResourceBlocked(why),
                            };
                            let why = format!("published build {key} awaits resource admission: {}; condition_sha256={digest}; explicit retry: continuum deploy-consume --retry-resource {key} {digest}", deferred.reason);
                            let explicit_retry = deferred.condition != continuum_cli_lifecycle::prebuilt_artifact::ResourceCondition::VolumeCapacity;
                            // A capacity refusal can stay inside this polling loop indefinitely.
                            // Persist its health evidence here, before the outer pass gets a result;
                            // otherwise a live waiting claim hides a blocked installation.
                            ledger.record_refusal(requested_tip, &key,
                                continuum_cli_lifecycle::deploy_health::RefusalKind::ResourceAdmission, &why);
                            if ledger.resource_deferrals.get(&key) != Some(&deferred) {
                                ledger.resource_deferrals.insert(key, deferred);
                            }
                            if let Err(why) = ledger.save(ledger_path) { return CiCore::ResourceBlocked(why); }
                            if explicit_retry {
                                // Release consumer custody so the explicit compare-and-clear
                                // command can acquire it. A later automatic pass reads the
                                // same durable condition and still cannot fetch this build.
                                return CiCore::ResourceBlocked(why);
                            }
                            MissingArtifact::AdmissionWait(why)
                        }
                        // Source reads can disagree during GitHub's suite propagation.
                        // Keep this consumer's custody and recheck; exiting here strands
                        // the actuation even after the same candidate becomes green.
                        Err(PublishedCoreError::SourcePending(why)) => {
                            // The candidate is known even though fetch was not authorized.
                            // Keep its identity: attributing this to requested_tip strands
                            // the incident after this older candidate subsequently runs.
                            ledger.record_refusal(requested_tip, &key,
                                continuum_cli_lifecycle::deploy_health::RefusalKind::SourceAuthorization, &why);
                            if let Err(error) = ledger.save(ledger_path) { return CiCore::ResourceBlocked(error); }
                            MissingArtifact::AdmissionWait(why)
                        },
                        Err(PublishedCoreError::CapabilityDeferred(why) | PublishedCoreError::OperationalBlocked(why) | PublishedCoreError::Held(why)) => return CiCore::ResourceBlocked(why),
                        Err(PublishedCoreError::Artifact(why)) if key == newest.as_str() => MissingArtifact::Refuse(format!(
                            "the CI build for {key} was refused: {why}; repair the prebuilt artifact before retrying; running core preserved"
                        )),
                        // an older core refused: keep waiting for the tip's, within its budget
                        Err(PublishedCoreError::Artifact(why)) => {
                            deploy_note(&format!("deploy-consume: the CI build for {key} was refused: {why}"));
                            when_artifact_missing(Some(p), tip_age_secs(repo, newest))
                        }
                    },
                }
            }
        };
        let wait_kind = match &missing {
            MissingArtifact::AdmissionWait(_) => continuum_cli_lifecycle::deploy_claim::WaitKind::Admission,
            _ => continuum_cli_lifecycle::deploy_claim::WaitKind::Publication,
        };
        match missing {
            MissingArtifact::Wait(why) | MissingArtifact::AdmissionWait(why) => {
                deploy_note(&format!("deploy-consume: {why}"));
                // Waiting on publication or admission preserves this deploy's custody. The renewer judges progress
                // by CPU, and a wait has none, so without this the claim would read Stalled
                // after an hour; still excluding, but naming a healthy wait as a hang.
                // The wait marker says it is only a wait: nothing builds beside the core and
                // nothing is swapped, so launches and decode samples go on (the caller clears
                // it before the install).
                if let Ok(root) = continuum_root() {
                    let pid = std::process::id() as i32;
                    let now = continuum_base::time::EpochMs::now().millis();
                    let _ = continuum_cli_lifecycle::deploy_claim::renew(&root, pid, now, true);
                    if let Err(e) = continuum_cli_lifecycle::deploy_claim::mark_waiting_for(&root, pid, now, wait_kind) {
                        deploy_note(&format!(
                            "deploy-consume: could not mark the wait ({e}); the claim keeps blocking as a build would"
                        ));
                    }
                }
            }
            MissingArtifact::Refuse(why) => {
                deploy_note(&format!("deploy-consume: {why}"));
                return CiCore::Refused(why);
            }
        }
    }
}
