//! Deploy-consume inputs: options, attempt ledger, request tip and the git helpers it uses.

use super::*;

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
// its own; Linux has no service arm yet), which stops serving, builds dark and puts the
// previous build back if the build fails (`dark_build`). The deploy claim it takes is what the
// tracker's #4187 reconcile reads; `deploy.settled` on this node with no human in the
// loop is the receipt.
//
// WHO RUNS IT (2026-09-20): the core's own `DeployActuator` launches it, detached, the
// moment the tracker records a request — on Windows by firing the `ContinuumDeploy`
// task (the supervisor's sibling, still on its ten-minute schedule as the fallback), on
// Unix as a new process group — unless the bash tracker's agent still owns the node.

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct DeployConsumeOptions {
    pub(super) retry_resource: Option<(String, String)>,
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
pub(super) const CONSUME_MAX_ATTEMPTS: u32 = 3;

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
pub(super) fn consume_attempts_path() -> Result<PathBuf, String> {
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
    #[serde(default)]
    pub(super) selection: Option<continuum_cli_lifecycle::deploy_health::DeploySelection>,
    #[serde(default)]
    pub(super) refusal: Option<continuum_cli_lifecycle::deploy_health::DeployRefusal>,
    #[serde(default, rename = "failures_by_build")]
    pub(super) failures: std::collections::BTreeMap<String, u32>,
    #[serde(default)]
    pub(super) resource_deferrals: std::collections::BTreeMap<String, continuum_cli_lifecycle::prebuilt_artifact::ResourceDeferral>,
}

impl ConsumeLedger {
    /// Only observed adoption resolves an incident. It never changes the newer
    /// request's selected build or forgives failures belonging to another build.
    pub(super) fn observe_running(&mut self, running: &str) -> bool {
        if self.refusal.as_ref().is_some_and(|r| r.resolved_by(running)) {
            self.refusal = None;
            true
        } else {
            false
        }
    }

    pub(super) fn record_refusal(&mut self, request: &str, build: &str,
        kind: continuum_cli_lifecycle::deploy_health::RefusalKind, reason: &str) {
        self.refusal = Some(continuum_cli_lifecycle::deploy_health::DeployRefusal::observe(
            self.refusal.as_ref(), request, build, kind, reason,
            continuum_base::time::EpochMs::now().millis(),
        ));
    }
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
            continuum_host::durable::replace(&temporary, path).map_err(|error| format!("replace deploy ledger: {error}"))?;
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
        if *self == Self::default() {
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

pub(super) fn deploy_request_path() -> Result<PathBuf, String> {
    // The same home the tracker writes under (`paths::continuum_root`: CONTINUUM_HOME
    // or ~/.continuum) — one resolver, so the consumer reads where the decision wrote.
    Ok(continuum_core::paths::continuum_root()
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

/// The checkout the tracker decides over — THE resolver the tracker itself uses
/// (`runtime::tracked_checkout`, card 790c6bcb): the recorded `CONTINUUM_TRACK_REPO_DIR`
/// else the build-time source root, resolved to the DURABLE main checkout of its repo, so
/// a binary built in a lease worktree deploys from the clone the lease came from.
pub(super) fn tracked_repo_dir() -> Result<PathBuf, String> {
    use continuum_core::runtime::tracked_checkout::{tracked_checkout, TRACK_REPO_DIR_KEY};
    match tracked_checkout() {
        Ok(Some(dir)) => Ok(dir),
        Ok(None) => Err(format!("deploy-consume: no checkout to deploy from (set {TRACK_REPO_DIR_KEY})")),
        Err(e) => Err(format!("deploy-consume: {e}")),
    }
}

/// `tip`'s build key in `repo` (`prebuilt_artifact::build_key_log_args`), or `None` if git
/// cannot say; `None` falls back to the tip itself.
pub(super) fn build_key_in(repo: &Path, tip: &str) -> Option<String> {
    let args = continuum_cli_lifecycle::prebuilt_artifact::build_key_log_args(tip);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    git_in(repo, &args).ok().map(|key| key.trim().to_string()).filter(|key| !key.is_empty())
}

pub(super) fn git_in(repo: &Path, args: &[&str]) -> Result<String, String> {
    let out = std::process::Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .map_err(|e| format!("git {}: {e}", args.join(" ")))?;
    if !out.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The deploy tree's `index.lock`, judged and (when stale) removed. `Ok(true)` = the tree
/// may be deployed; `Ok(false)` = a lock that may be live stands, named in the deploy log,
/// retried next tick without spending one of the tip's attempts.
pub(super) fn settle_index_lock(repo: &Path) -> Result<bool, String> {
    use continuum_git::index_lock::{settle, Settled, STALE_INDEX_LOCK};
    match settle(repo, STALE_INDEX_LOCK) {
        Settled::Absent => Ok(true),
        Settled::Held { lock, age } => {
            deploy_note(&format!(
                "deploy-consume: {} is locked ({} s old, a git operation may hold it) — not deploying this tick; \
                 it is removed once it is {} s old with no git running in the tree",
                lock.display(),
                age.as_secs(),
                STALE_INDEX_LOCK.as_secs()
            ));
            Ok(false)
        }
        Settled::Cleared { lock, age } => {
            deploy_note(&format!(
                "deploy.tree.stale_lock_cleared: removed {} ({} s old, no git running in the tree) — a git process died mid-write",
                lock.display(),
                age.as_secs()
            ));
            Ok(true)
        }
        Settled::Failed { lock, error, .. } => Err(format!("deploy-consume: cannot remove stale {}: {error}", lock.display())),
    }
}

pub(super) async fn running_build_sha() -> Option<String> {
    let reply = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        connection()
            .commands()
            .execute_value("ping", Value::Object(Default::default())),
    )
    .await
    .ok()?
    .ok()?;
    reply.get("buildSha").and_then(|v| v.as_str()).map(str::to_string)
}
