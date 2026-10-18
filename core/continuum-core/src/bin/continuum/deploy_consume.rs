//! `continuum deploy-consume`: the ACTION half of the deploy seam, in Rust, on every
//! platform — read the tracker's request, decide (pure verdict), check the tip out,
//! wait for CI's published core, hand it to `reboot`, and remember per BUILD what would
//! not land here (the ledger), with the explicit resource-retry path.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use continuum_cli_lifecycle::prebuilt_artifact::PublishedCoreError;
use continuum_cli_lifecycle::supervisor_install;
use serde_json::Value;

use super::checkout::{build_key_in, git_in, settle_index_lock, tracked_repo_dir};
use super::ci_core::{ci_core_for, install_ci_companions, CiCore};
use super::client::{running_build_sha, running_core_provenance};
use super::deploy_claim::{continuum_root, DeployClaimGuard};
use super::deploy_log::{deploy_note, DEPLOY_LOG};
use super::process::pid_alive;
use super::reboot::{reboot, RebootOptions};
#[cfg(windows)]
use super::service::take_install_lease;

// =============================================================================
// deploy-consume — the ACTION half of the deploy seam, on Windows (card 82af11f5)
// =============================================================================
//
// The DECISION half is Rust and runs on every node: `DeployTrackerModule` records a
// `DeployRequest` (state/deploy-request.json) when canary's tip is green and not the
// running build. The ACTION half was per-platform: on the Macs a launchd script
// consumes the request and runs `continuum reboot`; on Windows nothing did, so the
// 5090 ran whatever a hand last typed — every merged fix sat unfelt on the one seat
// that mattered (2026-09-18: five hand deploys in a day, Fable's count).
//
// This verb IS that consumer, in Rust, on EVERY platform: read the request → nothing
// owed if the running build already is the tip → refuse a dirty checkout (the same law
// the tracker's RefuseDirty applies; a consumer that stashes an operator's work is a
// consumer that loses it) → check the tip out detached → `reboot` (`--service` on
// Windows, where the handoff goes to the ContinuumCore supervisor, never a child of
// this process; on macOS a bare reboot hands the launch to a registered launchd job on
// its own; Linux has no service arm yet), whose warm build is RAM-gated
// (`warm_build_allowed`, Fable's build-path rule). The deploy claim it takes is what the
// tracker's #4187 reconcile reads; `deploy.settled` on this node with no human in the
// loop is the receipt.
//
// WHO RUNS IT (2026-09-20): the core's own `DeployActuator` launches it, detached, the
// moment the tracker records a request — on Windows by firing the `ContinuumDeploy`
// task (the supervisor's sibling, still on its ten-minute schedule as the fallback), on
// Unix as a new process group — unless the bash tracker's agent still owns the node.

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct DeployConsumeOptions {
    retry_resource: Option<(String, String)>,
    pub(super) protocol: bool,
}

impl DeployConsumeOptions {
    pub(super) fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        if let Some(arg) = args.next() {
            if arg == "--protocol" && args.next().is_none() {
                return Ok(Self { protocol: true, ..Self::default() });
            }
            if arg == "--retry-resource" {
                let build = args.next().ok_or("--retry-resource requires exact build and saved-condition sha256")?;
                let digest = args.next().ok_or("--retry-resource requires saved-condition sha256")?;
                if args.next().is_some() || build.len() != 40 || !build.bytes().all(|byte| byte.is_ascii_hexdigit())
                    || digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return Err("--retry-resource requires exactly one full build SHA and condition SHA256".into());
                }
                return Ok(Self { retry_resource: Some((build, digest)), ..Self::default() });
            }
            if matches!(arg.as_str(), "--install" | "--uninstall") {
                // ONE registrar for the consumer's task: `continuum install --supervisor`
                // registers it S4U beside ContinuumCore. The unelevated `--install` of
                // 2026-09-18 made an Interactive task that died with the session.
                return Err(format!(
                    "deploy-consume {arg} is gone: the ContinuumDeploy task is registered by `continuum install --supervisor` (S4U, beside ContinuumCore)"
                ));
            }
            return Err(format!("unknown deploy-consume option {arg}"));
        }
        Ok(Self::default())
    }
}

/// What the consumer decided about one request — PURE, so the vocabulary is pinned by
/// a test without a checkout, a core, or a scheduler.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ConsumeVerdict {
    /// No request stands: nothing owed.
    NothingOwed,
    /// The running build already is the requested tip: the tracker retires it.
    AlreadyRunning,
    /// The checkout has uncommitted work: never stash an operator's tree.
    RefuseDirty,
    /// Check the tip out and hand it to `reboot --service`.
    Deploy,
    /// A deploy claim is live (its owner alive, under the ceiling): a build is in
    /// flight from an earlier tick. The task fires every 10 min and a build takes
    /// 50 on this box (70 on IntelMac); without this arm tick N+10 fired a second
    /// `reboot --service` into the first one's build — two builds racing one target
    /// dir, the second swap shipping whichever artifact it found (Cormac, review of
    /// #4208; `reboot` itself never consults the claim — `deploy_gate` is the START
    /// verb's). The fifth arm, mirroring `continuum_cli_lifecycle::deploy_tracker::decide`.
    BuildInFlight,
    /// This tip failed to build/hand off [`CONSUME_MAX_ATTEMPTS`] times here: a tip
    /// green on CI can still fail on THIS box (a Windows-only wall, a vendor drift).
    /// Without a bound the consumer would rebuild it every tick until the claim aged
    /// out (Fable, review of #4208). The tracker's `deploy.stranded` is the receipt.
    GaveUp,
}

/// Attempts per tip before the consumer stops trying it and lets `deploy.stranded`
/// speak. Three: one for a transient (a fetch hiccup, a RAM dip), one to confirm it
/// is not, one more than that is a rebuild loop.
const CONSUME_MAX_ATTEMPTS: u32 = 3;

pub(super) fn consume_verdict(
    request_tip: Option<&str>,
    tip_build_key: Option<&str>,
    running_sha: Option<&str>,
    checkout_dirty: bool,
    build_in_flight: bool,
    prior_failures_for_tip: u32,
) -> ConsumeVerdict {
    let Some(tip) = request_tip else {
        return ConsumeVerdict::NothingOwed;
    };
    // ONE rule for "already running what the tip asks for", the tracker's: the tip itself
    // or its build key (a docs-only tip names the same core, card 9080ffb0).
    if running_sha.is_some_and(|running| {
        continuum_cli_lifecycle::deploy_tracker::running_satisfies(running, tip, tip_build_key)
    }) {
        return ConsumeVerdict::AlreadyRunning;
    }
    if build_in_flight {
        return ConsumeVerdict::BuildInFlight;
    }
    if checkout_dirty {
        return ConsumeVerdict::RefuseDirty;
    }
    if prior_failures_for_tip >= CONSUME_MAX_ATTEMPTS {
        return ConsumeVerdict::GaveUp;
    }
    ConsumeVerdict::Deploy
}

/// The consumer's own memory of a tip that would not land here: `{tip, failures}`
/// beside the request, cleared the moment the requested tip changes.
fn consume_attempts_path() -> Result<PathBuf, String> {
    deploy_request_path().map(|p| p.with_file_name("deploy-consume-attempts.json"))
}

/// Failed handoffs on this box, per BUILD tried (the build key the pass deployed),
/// never per request. On 2026-10-13 the 5090's request for 34e8db15d spent all three
/// attempts installing the older published 254d11e1c, whose installer predated the fix,
/// and gave up on 34e8db15d minutes before its own artifact published (card f7826d97).
/// Charged to the build, a failing older core is skipped while the tip's is awaited, and
/// the tip is given up on only when ITS build failed here.
///
/// A ledger in the old per-request shape (`{tip, failures}`) names the request, not the
/// build that failed, so its failures cannot be attributed: it reads as none.
#[derive(Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(super) struct ConsumeLedger {
    #[serde(default, rename = "failures_by_build")]
    failures: std::collections::BTreeMap<String, u32>,
    #[serde(default)]
    pub(super) resource_deferrals: std::collections::BTreeMap<String, continuum_cli_lifecycle::prebuilt_artifact::ResourceDeferral>,
}

impl ConsumeLedger {
    /// The existing consumer owns its ledger for the command's lifetime. The OS
    /// releases this nonblocking lease on crash; cooperating retries cannot race
    /// its writer. Older binaries require the explicit rollout/quiescence guard.
    pub(super) fn take_lease(path: &Path) -> Result<std::fs::File, String> {
        use fs2::FileExt;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| format!("create consumer ledger directory: {error}"))?;
        }
        let lease = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false)
            .open(path.with_extension("lock")).map_err(|error| format!("open consumer lease: {error}"))?;
        lease.try_lock_exclusive().map_err(|error| format!("another deploy consumer/retry owns the ledger, or its lease is unavailable: {error}"))?;
        Ok(lease)
    }

    pub(super) fn condition_digest(condition: &continuum_cli_lifecycle::prebuilt_artifact::ResourceDeferral) -> Result<String, String> {
        use sha2::{Digest, Sha256};
        let bytes = serde_json::to_vec(condition).map_err(|error| format!("encode resource condition: {error}"))?; // boundary: exact retry receipt digest
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }

    /// Caller holds the consumer lease and has refused any older live deploy owner.
    /// Compare against a fresh disk read, then atomically preserve every other entry.
    pub(super) fn retry_resource(path: &Path, build: &str, expected: &str, owner_active: bool) -> Result<(), String> {
        if owner_active { return Err("a deploy owner is active; resource retry refused".into()); }
        let mut ledger = Self::read(path)?;
        let condition = ledger.resource_deferrals.get(build).ok_or("this exact build has no saved resource condition")?;
        if Self::condition_digest(condition)? != expected {
            return Err("saved resource condition changed; retry digest does not match".into());
        }
        ledger.resource_deferrals.remove(build);
        // Keep an explicit empty receipt rather than a remove: compare-and-clear is
        // one durable replacement even when this was the final resource condition.
        ledger.write(path)
    }

    pub(super) fn read(path: &Path) -> Result<Self, String> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(format!("cannot read deploy resource/failure ledger {}: {error}; preserved", path.display())),
        };
        let invalid = |error| format!("invalid deploy resource/failure ledger {}: {error}; preserve evidence and repair before retry", path.display());
        let value: Value = serde_json::from_str(&text).map_err(|error| invalid(format!("{error}")))?; // boundary: durable lifecycle receipt
        if value.get("failures_by_build").is_none() && value.get("resource_deferrals").is_none() {
            if value.get("tip").is_some_and(Value::is_string) && value.get("failures").and_then(Value::as_u64).is_some() {
                return Ok(Self::default()); // The documented old per-request shape attributes no build.
            }
            return Err(invalid("unrecognized receipt shape".into()));
        }
        serde_json::from_value(value).map_err(|error| invalid(format!("{error}"))) // boundary: validated lifecycle receipt
    }

    pub(super) fn write(&self, path: &Path) -> Result<(), String> {
        use std::io::Write;
        let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| -> Result<(), String> {
            let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temporary)
                .map_err(|error| format!("create deploy ledger replacement: {error}"))?;
            serde_json::to_writer(&mut file, self).map_err(|error| format!("encode deploy ledger: {error}"))?; // boundary: durable lifecycle receipt
            file.flush().and_then(|_| file.sync_all()).map_err(|error| format!("sync deploy ledger: {error}"))?;
            drop(file);
            continuum_core::fs_portable::durable_rename(&temporary, path).map_err(|error| format!("replace deploy ledger: {error}"))?;
            Ok(())
        })();
        if result.is_err() { let _ = std::fs::remove_file(&temporary); }
        result
    }

    pub(super) fn failures(&self, build: &str) -> u32 {
        self.failures.get(build).copied().unwrap_or(0) // unwrap_or: a build never tried here has no failures
    }

    pub(super) fn record_failure(&mut self, build: &str) -> u32 {
        let n = self.failures.entry(build.to_string()).or_insert(0);
        *n += 1;
        *n
    }

    /// The build a failed pass is charged to: the one it SELECTED (the tip's, or the
    /// older published core it fell back to), or the tip's build when none was selected
    /// yet (no artifact, a refused one, a failure before the CI wait ended).
    pub(super) fn charge_failure(&mut self, selected: Option<&str>, tip_build: &str) -> (String, u32) {
        let build = selected.unwrap_or(tip_build).to_string(); // unwrap_or: nothing selected yet = the tip's attempt
        let failures = self.record_failure(&build);
        (build, failures)
    }

    /// A build landed: forget ITS failures only. Another build that already failed here
    /// stays excluded (an exhausted newer build must not become eligible again because
    /// an older one deployed, Astra review of #5011).
    pub(super) fn landed(&mut self, build: &str) {
        self.failures.remove(build);
        self.resource_deferrals.remove(build); // Actual successful handoff supersedes only this build's condition.
    }

    /// Persist, or remove the file when nothing is left to remember.
    pub(super) fn save(&self, path: &Path) -> Result<(), String> {
        if self.failures.is_empty() && self.resource_deferrals.is_empty() {
            match std::fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(format!("remove empty deploy ledger: {error}")),
            }
        } else {
            self.write(path)
        }
    }

    /// The candidates still worth a pass: the tip's own build always (whether to give it
    /// up is the verdict's call), and any older one that has not already failed here
    /// CONSUME_MAX_ATTEMPTS times.
    pub(super) fn admissible(&self, candidates: Vec<String>, tip_build: &str) -> Vec<String> {
        candidates
            .into_iter()
            .filter(|key| key == tip_build || self.failures(key) < CONSUME_MAX_ATTEMPTS)
            .collect()
    }
}

fn deploy_request_path() -> Result<PathBuf, String> {
    // The same home the tracker writes under (`resolve_continuum_root`: CONTINUUM_HOME
    // or ~/.continuum) — one resolver, so the consumer reads where the decision wrote.
    Ok(continuum_core::modules::persona_instance_manager::resolve_continuum_root()
        .join("state")
        .join("deploy-request.json"))
}

pub(super) fn read_deploy_request_tip(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<Value>(&text)
        .ok()?
        .get("tip_sha")
        .and_then(|v| v.as_str())
        .map(str::to_string)
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

async fn verify_consumer_protocol(cli: &Path) -> Result<(), String> {
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
async fn verify_registered_consumer(root: &Path) -> Result<continuum_cli_lifecycle::installed_release::ValidatedRelease, String> {
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
pub(super) async fn deploy_consume(options: DeployConsumeOptions) -> Result<(), String> {
    if options.protocol {
        println!("{}", continuum_cli_lifecycle::prebuilt_artifact::CONSUMER_LEASE_PROTOCOL); // boundary: read-only CLI capability wire for retry rollout admission
        return Ok(());
    }
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
enum PassEnd {
    Done,
    ResourceBlocked(String),
    Superseded(String),
}

async fn deploy_consume_pass() -> Result<PassEnd, String> {
    let request_path = deploy_request_path()?;
    let _ = DEPLOY_LOG.set(
        continuum_core::modules::persona_instance_manager::resolve_continuum_root()
            .join("logs")
            .join("deploy-consume.log"),
    );
    if let Some(dir) = DEPLOY_LOG.get().and_then(|p| p.parent()) {
        let _ = std::fs::create_dir_all(dir);
    }
    let tip = read_deploy_request_tip(&request_path);
    let running = running_build_sha().await;
    let repo = tracked_repo_dir()?;
    // Submodule pointers are not an operator's work here: aligning them to the tip's gitlinks
    // is this consumer's own job (below), and counting a stale one as dirt wedged the node
    // after the first pin bump (2026-09-27: every later tip refused as "uncommitted work").
    let dirty = !git_in(&repo, &["status", "--porcelain", "--untracked-files=no", "--ignore-submodules=all"])?.is_empty();
    // The deploy claim is the tracker's own input (`deploy_claim::in_flight`). A LIVE owner
    // excludes this consumer whether or not its claim expired: its checkout is its own, and
    // checking out a new tip under it dooms its build (card 634f644d). Only a dead owner's
    // claim is swept (by `reboot` itself).
    let gate = continuum_root()
        .map(|root| continuum_cli_lifecycle::deploy_claim::in_flight(&root, continuum_core::time::EpochMs::now().millis()))
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
    // The tip's build key, read from the same checkout the tracker fetched into.
    let tip_build_key = tip.as_deref().and_then(|t| build_key_in(&repo, t));
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
                // A checkout moves gitlinks, not submodule trees. Without this the llama.cpp
                // pin moved in the index while the engine source stayed at the old commit
                // (2026-09-27, #4437: the core deployed, core/vendor/llama.cpp stayed at
                // 965d38a90). Initialized submodules only, so a deploy never starts a fresh
                // clone; a submodule with local changes fails this loudly instead of losing them.
                git_in(&repo, &["submodule", "update", "--quiet", "--recursive"])?;
                // The warm build locates tools/scripts/start-server.sh by walking UP FROM
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
                            git_in(&repo, &["submodule", "update", "--quiet", "--recursive"])?;
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
                // so it is STAGED into the slot after the stop, like a warm build's artifact.
                let stage_prebuilt = true;
                // install_ci_companions above already put this core's CLI on PATH.
                let cli_installed = true;
                let preparation = match companion_result {
                    Ok(()) => reboot(RebootOptions { service, prebuilt: Some(prebuilt), stage_prebuilt, cli_installed, ..Default::default() }).await,
                    Err(error) => Err(error),
                };
                let preparation = preparation.map_err(|error| match error {
                    PublishedCoreError::OperationalBlocked(reason) | PublishedCoreError::CapabilityDeferred(reason) => PublishedCoreError::ResourceDeferred(continuum_cli_lifecycle::prebuilt_artifact::ResourceDeferral {
                        condition: continuum_cli_lifecycle::prebuilt_artifact::ResourceCondition::OperationalUncertainty,
                        path: preparation_path, projected_bytes: 0, reason,
                    }),
                    other => other,
                });
                match preparation {
                    Ok(()) => {}
                    Err(PublishedCoreError::ResourceDeferred(condition)) => {
                        let digest = match ConsumeLedger::condition_digest(&condition) {
                            Ok(digest) => digest,
                            Err(reason) => return Ok(PassEnd::ResourceBlocked(reason)),
                        };
                        let reason = format!("prepared build {deployed} awaits destination resource admission: {}; explicit retry: continuum deploy-consume --retry-resource {deployed} {digest}", condition.reason);
                        ledger.resource_deferrals.insert(deployed.clone(), condition);
                        if let Err(reason) = ledger.save(&attempts_path) { return Ok(PassEnd::ResourceBlocked(reason)); }
                        return Ok(PassEnd::ResourceBlocked(reason));
                    }
                    Err(PublishedCoreError::CapabilityDeferred(reason) | PublishedCoreError::OperationalBlocked(reason)) => return Ok(PassEnd::ResourceBlocked(reason)),
                    Err(PublishedCoreError::Artifact(reason)) => return Err(reason),
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
                Ok(PassEnd::ResourceBlocked(why)) => deploy_note(&format!("deploy resource custody refused: {why}; no build failure charged")),
                Ok(PassEnd::Superseded(_)) => {} // not this tip's failure; the outer loop follows the new request
                Err(why) => {
                    // Charged to the build this pass tried: the tip's, or the older published
                    // core it fell back to. A failure before any build was chosen is the tip's.
                    let tip_build = tip_build_key.clone().unwrap_or_else(|| tip.clone()); // unwrap_or_else: no build key means the tip names its own core
                    let (tried, failures) = ledger.charge_failure(selected.as_deref(), &tip_build);
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

/// The build keys this node could move to, newest first: those after the running core up
/// to the tip (bounded by `DEPLOY_CANDIDATE_LIMIT`), with the tip's own key always first.
/// With no running core, or a history git cannot walk, it is the tip's key alone, said in
/// the log.
fn deploy_candidates(repo: &Path, running: Option<&str>, tip: &str, newest_key: &str) -> Vec<String> {
    let mut keys: Vec<String> = match running {
        None => Vec::new(),
        Some(running) => {
            let args = continuum_cli_lifecycle::prebuilt_artifact::candidate_keys_log_args(running, tip);
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            match git_in(repo, &args) {
                Ok(out) => out.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect(),
                Err(why) => {
                    deploy_note(&format!("deploy-consume: cannot list builds since {running}: {why}; waiting on the tip's core only"));
                    Vec::new()
                }
            }
        }
    };
    keys.retain(|key| key != newest_key);
    keys.insert(0, newest_key.to_string());
    keys
}
