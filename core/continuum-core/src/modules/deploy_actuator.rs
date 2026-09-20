//! THE DEPLOY ACTUATOR — the core acts on its own deploy request, on every platform.
//!
//! The DECISION half ([`crate::modules::deploy_tracker`]) runs in Rust on every node and
//! records a [`DeployRequest`] when a green tip differs from the running build. The
//! ACTION half was per-platform and outside the binary: on the Macs a bash tracker under
//! launchd (`tools/scripts/track-canary.sh`, a SECOND owner of the same decision), on
//! Windows a scheduled task firing `continuum deploy-consume` every ten minutes — a task
//! the core never asked for anything and never heard back from. Joel, 2026-09-20: "the
//! 5090 doesn't add much, almost drags the whole grid down" — that box ran a stale build
//! (214 `deploy.track.request_written` rows, nothing consumed) until a person rebooted it.
//!
//! This module closes the seam from inside the core: the moment a request is recorded,
//! the actuator launches the consumer verb DETACHED from the core (a new process group
//! on Unix; on Windows the registered `ContinuumDeploy` task, which the scheduler runs
//! outside the core's job object — falling back to a break-away spawn when no task is
//! registered), writes an ACTUATION RECEIPT (`<state>/deploy-actuation.json`), and at the
//! next boot says whether the actuation produced the tip.
//!
//! OWNERSHIP — exactly one owner per node. Where the bash tracker's launchd agent (or its
//! systemd timer) is installed, or the operator switch `CONTINUUM_DEPLOY_ACTUATOR=off`
//! is set, the actuator DEFERS and says so; nothing changes on a node that already has an
//! owner. The switch is an operator switch, never a tuning knob. The follow-up deletes
//! `track-canary.sh` once the actuator has produced one verified deploy on a Unix node.
//!
//! WHY THE CONSUMER VERB AND NOT A BARE `reboot`: `continuum reboot` builds the tracked
//! checkout's CURRENT HEAD. The request names a TIP. `deploy-consume` is the verb that
//! already checks the tip out detached, refuses a dirty tree, honors the deploy claim,
//! bounds its own attempts per tip, then runs the reboot — one action, one place.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use crate::runtime::deploy_tracker::{same_commit, DeployRequest, STRANDED_GRACE_MS};

/// The receipt, beside the request it answered: `<state>/deploy-actuation.json`.
pub(crate) const ACTUATION_FILE: &str = "deploy-actuation.json";
/// The actuator's stdout/stderr for the spawned consumer: `<root>/logs/<this>`.
pub(crate) const ACTUATION_LOG: &str = "deploy-actuate.log";
/// The operator switch. `off` (also `0` / `false`) = never act on this node. Read as a
/// switch only — there is no other value and no tuning here.
pub(crate) const ACTUATOR_SWITCH_KEY: &str = "CONTINUUM_DEPLOY_ACTUATOR";
/// The CLI verb the actuator launches — the consumer that turns a request into a reboot.
pub(crate) const CONSUMER_VERB: &str = "deploy-consume";
/// Actuations per request before the actuator stops and lets `deploy.stranded` speak.
/// Mirrors the consumer's own per-tip bound: one for a transient, one to confirm, no
/// third of a rebuild loop.
pub(crate) const ACTUATE_MAX_ATTEMPTS: u32 = 3;
/// The Windows consumer task `continuum install --supervisor` registers (S4U, every ten
/// minutes). Firing it on demand is how the core actuates without becoming the parent of
/// a process that will stop the core.
#[cfg(windows)]
pub(crate) const DEPLOY_TASK: &str = "ContinuumDeploy";
#[cfg(windows)]
const TASK_RUN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// What one actuation recorded about itself. Kept as the standing evidence of the last
/// action so the next tick can refuse to act twice on one request and the next boot can
/// grade the action against the build it produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ActuationReceipt {
    /// The tip the request asked for.
    pub tip_sha: String,
    /// Unix ms the consumer was launched.
    pub spawned_ms: u64,
    /// The consumer's pid when this process spawned it directly; `None` when the
    /// scheduler ran it (a task run returns no pid).
    pub pid: Option<u32>,
    /// How it was launched — readable, for the operator and the boot probe.
    pub mode: String,
    /// 1-based actuation count for this request.
    pub attempt: u32,
    /// Filled in at the next boot: `landed` / `stale`.
    #[serde(default)]
    pub boot_outcome: Option<String>,
    #[serde(default)]
    pub booted_ms: Option<u64>,
    /// Set when the tracker saw the requested tip running: request → settle, ms. The
    /// measured cost of a deploy on THIS tier.
    #[serde(default)]
    pub landed_waited_ms: Option<u64>,
    /// The previous cycle's measurement, carried forward so a re-actuation bound can be
    /// derived from a real number rather than a guess.
    #[serde(default)]
    pub last_deploy_ms: Option<u64>,
}

/// Who else owns deploy on this node. When one is present the actuator does not act.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExternalOwner {
    /// macOS: `~/Library/LaunchAgents/com.continuum.track-canary.plist` — the bash tracker.
    LaunchdTracker(PathBuf),
    /// Linux: `~/.config/systemd/user/continuum-track-canary.timer` — the same tracker.
    SystemdTimer(PathBuf),
    /// `CONTINUUM_DEPLOY_ACTUATOR=off`.
    OperatorSwitch,
}

impl ExternalOwner {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            ExternalOwner::LaunchdTracker(_) => "launchd track-canary agent",
            ExternalOwner::SystemdTimer(_) => "systemd track-canary timer",
            ExternalOwner::OperatorSwitch => "operator switch",
        }
    }
}

pub(crate) fn launchd_tracker_plist(user_home: &Path) -> PathBuf {
    user_home.join("Library").join("LaunchAgents").join("com.continuum.track-canary.plist")
}

pub(crate) fn systemd_tracker_timer(user_home: &Path) -> PathBuf {
    user_home.join(".config").join("systemd").join("user").join("continuum-track-canary.timer")
}

/// The ownership rule, over paths and the switch value — pure enough to assert with a
/// temp dir. The switch outranks everything; then whichever tracker install is present.
pub(crate) fn external_owner(user_home: &Path, switch: Option<&str>) -> Option<ExternalOwner> {
    if switch.is_some_and(|s| matches!(s.trim().to_ascii_lowercase().as_str(), "off" | "0" | "false")) {
        return Some(ExternalOwner::OperatorSwitch);
    }
    let plist = launchd_tracker_plist(user_home);
    if plist.is_file() {
        return Some(ExternalOwner::LaunchdTracker(plist));
    }
    let timer = systemd_tracker_timer(user_home);
    if timer.is_file() {
        return Some(ExternalOwner::SystemdTimer(timer));
    }
    None
}

/// What this tick does about a wanted deploy, given the last receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ActuateDecision {
    /// Launch the consumer; this is actuation number `attempt` for the request.
    Act { attempt: u32 },
    /// Already launched for this request and the tracker has not called it stranded —
    /// the deploy is working (or inside its grace). Quiet.
    AlreadyActuated { attempt: u32, elapsed_ms: u64 },
    /// Stranded, but not yet past the bound derived from the last measured deploy — a
    /// slow tier is not re-actuated blindly into its own handoff window.
    AwaitingBound { attempt: u32, elapsed_ms: u64, bound_ms: u64 },
    /// [`ACTUATE_MAX_ATTEMPTS`] launches did not land this tip here. Stop; the
    /// tracker's `deploy.stranded` is the standing receipt and a new tip resets this.
    GaveUp { attempts: u32 },
}

/// How long after an actuation a stranded request may be re-actuated: the tracker's own
/// grace at least, and 1.5× the last measured request-to-settle time on this tier when
/// one exists (the Intel Mac deploys in ~70 min; a 20-min bound there would re-launch a
/// build into a build).
pub(crate) fn reactuation_bound_ms(last_deploy_ms: Option<u64>) -> u64 {
    last_deploy_ms
        .map(|d| d.saturating_mul(3) / 2)
        .unwrap_or(0) // unwrap_or: no measurement yet = the tracker's grace alone bounds it
        .max(STRANDED_GRACE_MS)
}

/// The pure decision: one actuation per request; a retry only after the tracker's
/// stranded verdict, only past the bound, only up to the attempt cap. A receipt covers a
/// request only if it names the same tip AND was spawned no earlier than the request was
/// written — a request re-recorded after a settle (an operator rolled the box back by
/// hand) is a NEW request, whatever the last cycle's receipt says.
pub(crate) fn decide_actuation(
    prev: Option<&ActuationReceipt>,
    req: &DeployRequest,
    stranded: bool,
    now_ms: u64,
) -> ActuateDecision {
    let Some(prev) = prev
        .filter(|p| same_commit(&p.tip_sha, &req.tip_sha))
        .filter(|p| p.spawned_ms >= req.requested_ms)
    else {
        return ActuateDecision::Act { attempt: 1 };
    };
    let elapsed_ms = now_ms.saturating_sub(prev.spawned_ms);
    if !stranded {
        return ActuateDecision::AlreadyActuated { attempt: prev.attempt, elapsed_ms };
    }
    if prev.attempt >= ACTUATE_MAX_ATTEMPTS {
        return ActuateDecision::GaveUp { attempts: prev.attempt };
    }
    let bound_ms = reactuation_bound_ms(prev.last_deploy_ms);
    if elapsed_ms < bound_ms {
        return ActuateDecision::AwaitingBound { attempt: prev.attempt, elapsed_ms, bound_ms };
    }
    ActuateDecision::Act { attempt: prev.attempt + 1 }
}

/// What the last actuation produced, judged at boot against the build now running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BootOutcome {
    /// The running build is the tip the actuation asked for.
    Landed,
    /// A receipt stands and the running build is not its tip — the action did not take,
    /// or this boot is the old build coming back.
    Stale,
    /// No receipt: this boot was not produced by an actuation this module can see.
    Unknown,
}

impl BootOutcome {
    pub(crate) fn word(self) -> &'static str {
        match self {
            BootOutcome::Landed => "landed",
            BootOutcome::Stale => "stale",
            BootOutcome::Unknown => "unknown",
        }
    }
}

/// Pure: receipt × running sha → outcome.
pub(crate) fn classify_boot(receipt: Option<&ActuationReceipt>, running_sha: &str) -> BootOutcome {
    match receipt {
        None => BootOutcome::Unknown,
        Some(r) if same_commit(&r.tip_sha, running_sha) => BootOutcome::Landed,
        Some(_) => BootOutcome::Stale,
    }
}

/// Everything a spawn needs, resolved before the spawner is asked — so a test can assert
/// the exact launch without a process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpawnPlan {
    pub cli: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub log: PathBuf,
}

/// The spawner's answer. `child` is held only for a process THIS core spawned, so the
/// caller can reap it off the tick (a task run has none).
pub(crate) struct Spawned {
    pub pid: Option<u32>,
    pub mode: String,
    pub child: Option<std::process::Child>,
}

/// The seam between the actuator's bookkeeping and the OS. One real implementation
/// ([`CliSpawner`]); the recording one is a test fixture.
pub(crate) trait DeploySpawner: Send + Sync {
    fn spawn(&self, plan: &SpawnPlan) -> Result<Spawned, String>;
}

/// The installed CLI, as the core can find it: beside its own executable (a build dir,
/// a slot that carries both), else the user's `~/.local/bin` copy that every deploy
/// refreshes and `continuum install` converges, else the name on PATH.
pub(crate) fn locate_cli(user_home: &Path) -> PathBuf {
    let name = if cfg!(windows) { "continuum.exe" } else { "continuum" };
    if let Some(sibling) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.join(name)))
        .filter(|p| p.is_file())
    {
        return sibling;
    }
    let local = user_home.join(".local").join("bin").join(name);
    if local.is_file() {
        return local;
    }
    PathBuf::from(name)
}

/// A log handle a child can be handed as stdout/stderr on every platform: full write
/// access positioned at the end (an append-only Windows handle makes a bash child exit
/// 1 before its first line — #4233, a day of empty logs on the 5090).
fn open_log_for_child(path: &Path) -> std::io::Result<std::fs::File> {
    use std::io::Seek;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut file = std::fs::OpenOptions::new().create(true).write(true).open(path)?;
    file.seek(std::io::SeekFrom::End(0))?;
    Ok(file)
}

/// The real spawner.
pub(crate) struct CliSpawner;

#[cfg(unix)]
impl DeploySpawner for CliSpawner {
    fn spawn(&self, plan: &SpawnPlan) -> Result<Spawned, String> {
        use std::os::unix::process::CommandExt;
        let log = open_log_for_child(&plan.log).map_err(|e| format!("cannot open {}: {e}", plan.log.display()))?;
        let err = log.try_clone().map_err(|e| format!("cannot clone the log handle: {e}"))?;
        // A NEW PROCESS GROUP: the reboot the consumer runs stops the core by signalling
        // the core's group (the start script's setsid), and the consumer must not be in it.
        let child = Command::new(&plan.cli)
            .args(&plan.args)
            .current_dir(&plan.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(err))
            .process_group(0)
            .spawn()
            .map_err(|e| format!("cannot spawn {} {}: {e}", plan.cli.display(), plan.args.join(" ")))?;
        Ok(Spawned {
            pid: Some(child.id()),
            mode: format!("{} (new process group)", plan.args.join(" ")),
            child: Some(child),
        })
    }
}

#[cfg(windows)]
impl DeploySpawner for CliSpawner {
    fn spawn(&self, plan: &SpawnPlan) -> Result<Spawned, String> {
        use std::os::windows::process::CommandExt;
        // FIRST the registered consumer task: the scheduler runs it under the S4U
        // principal, outside this core's job object, and serializes it against its own
        // ten-minute firing (MultipleInstancesPolicy IgnoreNew) — one consumer at a time
        // on this box, whoever asked. `continuum install --supervisor` grants the user
        // RX on the task so this needs no elevation.
        let run = crate::system_resources::bounded_command::probe(
            "schtasks",
            &["/Run", "/TN", DEPLOY_TASK],
            TASK_RUN_TIMEOUT,
        );
        if run.stdout_if_ok().is_some() {
            return Ok(Spawned { pid: None, mode: format!("schtasks /Run {DEPLOY_TASK}"), child: None });
        }
        // No task (or not runnable by this user): spawn the consumer ourselves, breaking
        // away from any job object first (a child of the core's job dies when the core's
        // reboot ends that job — card 82af11f5), then without when the job forbids it.
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        let launch = |flags: u32| -> std::io::Result<std::process::Child> {
            let log = open_log_for_child(&plan.log)?;
            let err = log.try_clone()?;
            Command::new(&plan.cli)
                .args(&plan.args)
                .current_dir(&plan.cwd)
                .stdin(Stdio::null())
                .stdout(Stdio::from(log))
                .stderr(Stdio::from(err))
                .creation_flags(flags)
                .spawn()
        };
        let task_outcome = run.outcome();
        let (child, how) = match launch(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW | CREATE_BREAKAWAY_FROM_JOB) {
            Ok(c) => (c, "breakaway"),
            Err(e) if e.raw_os_error() == Some(5) => (
                launch(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW)
                    .map_err(|e| format!("cannot spawn {} {}: {e}", plan.cli.display(), plan.args.join(" ")))?,
                "child-of-job (breakaway denied)",
            ),
            Err(e) => return Err(format!("cannot spawn {} {}: {e}", plan.cli.display(), plan.args.join(" "))),
        };
        Ok(Spawned {
            pid: Some(child.id()),
            mode: format!("{} ({how}; task {DEPLOY_TASK} {task_outcome})", plan.args.join(" ")),
            child: Some(child),
        })
    }
}

/// A spawner that records every plan and spawns nothing. The ONE fixture for this seam.
#[cfg(any(test, feature = "test-fixtures"))]
pub(crate) struct RecordingSpawner {
    pub plans: parking_lot::Mutex<Vec<SpawnPlan>>,
    /// `Some(pid)` = report a launch with this pid; `None` = fail every spawn.
    pub pid: Option<u32>,
}

#[cfg(any(test, feature = "test-fixtures"))]
impl RecordingSpawner {
    pub(crate) fn new(pid: Option<u32>) -> Self {
        Self { plans: parking_lot::Mutex::new(Vec::new()), pid }
    }
}

#[cfg(any(test, feature = "test-fixtures"))]
impl DeploySpawner for RecordingSpawner {
    fn spawn(&self, plan: &SpawnPlan) -> Result<Spawned, String> {
        self.plans.lock().push(plan.clone());
        match self.pid {
            Some(pid) => Ok(Spawned { pid: Some(pid), mode: "recorded".into(), child: None }),
            None => Err("recording spawner refuses".into()),
        }
    }
}

/// What one call to [`DeployActuator::on_deploy_wanted`] did.
pub(crate) enum ActuateOutcome {
    Deferred(ExternalOwner),
    Spawned { attempt: u32, pid: Option<u32>, mode: String, child: Option<std::process::Child> },
    SpawnFailed { attempt: u32, error: String },
    Skipped(ActuateDecision),
}

pub(crate) struct DeployActuator {
    spawner: Box<dyn DeploySpawner>,
    /// The user's home — where a tracker install lives and where `~/.local/bin` is.
    user_home: PathBuf,
    /// The continuum home (`~/.continuum`), joined once.
    root: PathBuf,
    state_dir: PathBuf,
    switch: Option<String>,
    /// Said once per tip, not every tick: a deferral and a give-up are durable conditions.
    said_deferred: parking_lot::Mutex<Option<String>>,
    said_gave_up: parking_lot::Mutex<Option<String>>,
}

impl DeployActuator {
    /// The production actuator over `root` (the continuum home). The switch is read from
    /// the process environment (which the core seeds from `config.env` at boot) or from
    /// `config.env` directly.
    pub(crate) fn new(root: PathBuf) -> Self {
        let user_home = dirs::home_dir()
            .or_else(|| root.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| root.clone()); // unwrap_or_else: no home at all = look beside the root; no tracker can be installed there, so the actuator acts
        let switch = std::env::var(ACTUATOR_SWITCH_KEY)
            .ok()
            .or_else(|| crate::config_env::read(ACTUATOR_SWITCH_KEY));
        Self::with_spawner(user_home, root, switch, Box::new(CliSpawner))
    }

    pub(crate) fn with_spawner(
        user_home: PathBuf,
        root: PathBuf,
        switch: Option<String>,
        spawner: Box<dyn DeploySpawner>,
    ) -> Self {
        let state_dir = root.join("state");
        Self {
            spawner,
            user_home,
            root,
            state_dir,
            switch,
            said_deferred: parking_lot::Mutex::new(None),
            said_gave_up: parking_lot::Mutex::new(None),
        }
    }

    fn receipt_path(&self) -> PathBuf {
        self.state_dir.join(ACTUATION_FILE)
    }

    /// The standing receipt, if any. A malformed file reads as none: the next actuation
    /// overwrites it, and the attempt ladder starts over rather than wedging.
    pub(crate) fn receipt(&self) -> Option<ActuationReceipt> {
        let text = std::fs::read_to_string(self.receipt_path()).ok()?;
        serde_json::from_str(&text).ok()
    }

    fn write_receipt(&self, receipt: &ActuationReceipt) {
        let _ = std::fs::create_dir_all(&self.state_dir);
        if let Ok(bytes) = serde_json::to_vec_pretty(receipt) {
            let path = self.receipt_path();
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, bytes).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }

    /// The tracker wants `req` deployed this tick (`stranded` = its reconcile called the
    /// standing request stranded). Defer to an external owner, or decide and act. Sync;
    /// the tracker runs it off its tick.
    pub(crate) fn on_deploy_wanted(&self, req: &DeployRequest, stranded: bool, now_ms: u64) -> ActuateOutcome {
        if let Some(owner) = external_owner(&self.user_home, self.switch.as_deref()) {
            let mut said = self.said_deferred.lock();
            if said.as_deref() != Some(req.tip_sha.as_str()) {
                *said = Some(req.tip_sha.clone());
                crate::probe!(
                    class = "deploy.actuate.deferred_to_external_owner",
                    tip = req.tip_sha.as_str(),
                    owner = owner.name(),
                    detail = ?owner,
                    "a deploy is wanted and another owner acts on this node — the actuator stands down"
                );
            }
            return ActuateOutcome::Deferred(owner);
        }
        let prev = self.receipt();
        let decision = decide_actuation(prev.as_ref(), req, stranded, now_ms);
        let attempt = match decision.clone() {
            ActuateDecision::Act { attempt } => attempt,
            ActuateDecision::GaveUp { attempts } => {
                let mut said = self.said_gave_up.lock();
                if said.as_deref() != Some(req.tip_sha.as_str()) {
                    *said = Some(req.tip_sha.clone());
                    crate::probe!(
                        class = "deploy.actuate.gave_up",
                        tip = req.tip_sha.as_str(),
                        attempts,
                        "the consumer was launched this many times for this tip and it is not running — not launching again; a new tip resets this"
                    );
                }
                return ActuateOutcome::Skipped(decision);
            }
            ActuateDecision::AwaitingBound { attempt, elapsed_ms, bound_ms } => {
                crate::probe!(
                    class = "deploy.actuate.decision",
                    verdict = "awaiting_bound",
                    tip = req.tip_sha.as_str(),
                    attempt,
                    elapsed_ms,
                    bound_ms,
                    "stranded, but inside the bound derived from the last measured deploy — not re-actuated blindly"
                );
                return ActuateOutcome::Skipped(decision);
            }
            ActuateDecision::AlreadyActuated { .. } => return ActuateOutcome::Skipped(decision),
        };
        let plan = SpawnPlan {
            cli: locate_cli(&self.user_home),
            args: vec![CONSUMER_VERB.to_string()],
            cwd: self.root.clone(),
            log: self.root.join("logs").join(ACTUATION_LOG),
        };
        // The previous cycle's measurement rides along so the re-actuation bound is real.
        let last_deploy_ms = prev
            .as_ref()
            .and_then(|p| p.landed_waited_ms.or(p.last_deploy_ms));
        match self.spawner.spawn(&plan) {
            Ok(spawned) => {
                self.write_receipt(&ActuationReceipt {
                    tip_sha: req.tip_sha.clone(),
                    spawned_ms: now_ms,
                    pid: spawned.pid,
                    mode: spawned.mode.clone(),
                    attempt,
                    boot_outcome: None,
                    booted_ms: None,
                    landed_waited_ms: None,
                    last_deploy_ms,
                });
                crate::probe!(
                    class = "deploy.actuate.spawned",
                    tip = req.tip_sha.as_str(),
                    pid = spawned.pid.unwrap_or(0), // unwrap_or: a task run has no pid; 0 is the honest "none" in a numeric field
                    mode = spawned.mode.as_str(),
                    attempt,
                    cli = %plan.cli.display(),
                    log = %plan.log.display(),
                    "the consumer is launched, detached from this core — the receipt is deploy-actuation.json"
                );
                ActuateOutcome::Spawned { attempt, pid: spawned.pid, mode: spawned.mode, child: spawned.child }
            }
            Err(error) => {
                // A failed launch is an attempt too: it gets the same stranded → bound →
                // cap ladder, never a fresh try every tick.
                self.write_receipt(&ActuationReceipt {
                    tip_sha: req.tip_sha.clone(),
                    spawned_ms: now_ms,
                    pid: None,
                    mode: format!("spawn failed: {error}"),
                    attempt,
                    boot_outcome: None,
                    booted_ms: None,
                    landed_waited_ms: None,
                    last_deploy_ms,
                });
                crate::probe!(
                    class = "deploy.actuate.spawn_failed",
                    tip = req.tip_sha.as_str(),
                    attempt,
                    cli = %plan.cli.display(),
                    error = error.as_str(),
                    "the consumer could not be launched — the request stands; retried after the tracker's stranded verdict"
                );
                ActuateOutcome::SpawnFailed { attempt, error }
            }
        }
    }

    /// At boot: did the last actuation produce the build now running? Said once per
    /// boot, recorded on the receipt.
    pub(crate) fn report_boot_outcome(&self, running_sha: &str, now_ms: u64) -> BootOutcome {
        let receipt = self.receipt();
        let outcome = classify_boot(receipt.as_ref(), running_sha);
        crate::probe!(
            class = "deploy.actuate.outcome",
            outcome = outcome.word(),
            running = running_sha,
            tip = receipt.as_ref().map(|r| r.tip_sha.as_str()).unwrap_or(""), // unwrap_or: no receipt = no tip to name
            mode = receipt.as_ref().map(|r| r.mode.as_str()).unwrap_or(""), // unwrap_or: no receipt = no mode to name
            spawned_ms = receipt.as_ref().map(|r| r.spawned_ms).unwrap_or(0), // unwrap_or: no receipt = 0
            attempt = receipt.as_ref().map(|r| r.attempt).unwrap_or(0), // unwrap_or: no receipt = 0
            "this boot, judged against the last deploy actuation"
        );
        if let Some(mut r) = receipt {
            r.boot_outcome = Some(outcome.word().to_string());
            r.booted_ms = Some(now_ms);
            self.write_receipt(&r);
        }
        outcome
    }

    /// The tracker saw the requested tip running: record the measured cost on the receipt
    /// that produced it, so the next cycle's bound is derived from this tier's number.
    pub(crate) fn record_settled(&self, tip_sha: &str, waited_ms: u64) {
        if let Some(mut r) = self.receipt() {
            if same_commit(&r.tip_sha, tip_sha) {
                r.landed_waited_ms = Some(waited_ms);
                self.write_receipt(&r);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_700_000_000_000;

    fn actuator(dir: &Path, switch: Option<&str>, pid: Option<u32>) -> (DeployActuator, std::sync::Arc<RecordingSpawner>) {
        // Two homes under ONE temp dir: the user home (where a tracker install would be)
        // and the continuum home (where state lives). Never the operator's ~/.continuum.
        let user_home = dir.join("home");
        let root = user_home.join(".continuum");
        std::fs::create_dir_all(&root).unwrap(); // unwrap: the test's own dir
        let spawner = std::sync::Arc::new(RecordingSpawner::new(pid));
        struct Shared(std::sync::Arc<RecordingSpawner>);
        impl DeploySpawner for Shared {
            fn spawn(&self, plan: &SpawnPlan) -> Result<Spawned, String> {
                self.0.spawn(plan)
            }
        }
        let act = DeployActuator::with_spawner(user_home, root, switch.map(str::to_string), Box::new(Shared(spawner.clone())));
        (act, spawner)
    }

    // what this catches: OWNERSHIP — exactly one owner per node. With the bash tracker's
    // launchd agent installed (the M5, the IntelMac tonight), or the operator switch off,
    // the actuator records nothing and spawns nothing; with neither, it acts. A second
    // owner of the same decision is how two builds race one target dir.
    #[test]
    fn the_actuator_does_not_act_when_an_external_owner_is_present() {
        let dir = tempfile::tempdir().unwrap(); // unwrap: the test's own dir
        let req = DeployRequest::new("bbbbbbbbb222", NOW);
        // The launchd plist present → deferred, nothing spawned, no receipt.
        let (act, spawner) = actuator(dir.path(), None, Some(4242));
        let plist = launchd_tracker_plist(&dir.path().join("home"));
        std::fs::create_dir_all(plist.parent().unwrap()).unwrap(); // unwrap: the test's own dir
        std::fs::write(&plist, "<plist/>").unwrap(); // unwrap: the test's own file
        assert!(matches!(act.on_deploy_wanted(&req, false, NOW), ActuateOutcome::Deferred(ExternalOwner::LaunchdTracker(_))));
        assert!(spawner.plans.lock().is_empty(), "an external owner means no spawn");
        assert!(act.receipt().is_none(), "a deferral writes no receipt");
        std::fs::remove_file(&plist).unwrap(); // unwrap: the test's own file
        // The systemd timer is the same owner on Linux.
        let timer = systemd_tracker_timer(&dir.path().join("home"));
        std::fs::create_dir_all(timer.parent().unwrap()).unwrap(); // unwrap: the test's own dir
        std::fs::write(&timer, "[Timer]").unwrap(); // unwrap: the test's own file
        assert!(matches!(act.on_deploy_wanted(&req, false, NOW), ActuateOutcome::Deferred(ExternalOwner::SystemdTimer(_))));
        std::fs::remove_file(&timer).unwrap(); // unwrap: the test's own file
        // The operator switch outranks everything, and it is a switch: only "off" is off.
        let (off, off_spawner) = actuator(dir.path(), Some("off"), Some(4242));
        assert!(matches!(off.on_deploy_wanted(&req, false, NOW), ActuateOutcome::Deferred(ExternalOwner::OperatorSwitch)));
        assert!(off_spawner.plans.lock().is_empty());
        assert_eq!(external_owner(&dir.path().join("home"), Some("on")), None, "any other value is not a switch position");
        // No owner → the actuator acts.
        assert!(matches!(act.on_deploy_wanted(&req, false, NOW), ActuateOutcome::Spawned { attempt: 1, pid: Some(4242), .. }));
        let plans = spawner.plans.lock();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].args, vec![CONSUMER_VERB.to_string()], "the consumer verb, not a bare reboot: the request names a tip, reboot builds HEAD");
        assert_eq!(plans[0].log, dir.path().join("home").join(".continuum").join("logs").join(ACTUATION_LOG));
    }

    // what this catches: ONE actuation per request. The tracker re-decides Deploy every
    // 300 s tick while the tip is not running; without the receipt the actuator would
    // launch the consumer every tick into its own build. A retry happens only after the
    // tracker's stranded verdict, only past the bound derived from the last measured
    // deploy, only up to the cap — and a request written AFTER the receipt is a new one.
    #[test]
    fn one_actuation_per_request_retried_only_after_a_strand_and_past_the_bound() {
        let dir = tempfile::tempdir().unwrap(); // unwrap: the test's own dir
        let (act, spawner) = actuator(dir.path(), None, Some(7));
        let req = DeployRequest::new("c2344d758225d87911d1ee2934b4e7e42673c26e", NOW);
        assert!(matches!(act.on_deploy_wanted(&req, false, NOW), ActuateOutcome::Spawned { attempt: 1, .. }));
        let receipt = act.receipt().expect("a spawn writes its receipt");
        assert_eq!((receipt.tip_sha.as_str(), receipt.spawned_ms, receipt.pid, receipt.attempt), (req.tip_sha.as_str(), NOW, Some(7), 1));
        // The next ticks: not stranded → quiet, no second launch.
        assert!(matches!(act.on_deploy_wanted(&req, false, NOW + 300_000), ActuateOutcome::Skipped(ActuateDecision::AlreadyActuated { attempt: 1, .. })));
        assert_eq!(spawner.plans.lock().len(), 1);
        // Stranded, but inside the bound (no measurement yet = the tracker's grace) → wait.
        assert!(matches!(
            act.on_deploy_wanted(&req, true, NOW + STRANDED_GRACE_MS - 1),
            ActuateOutcome::Skipped(ActuateDecision::AwaitingBound { attempt: 1, bound_ms, .. }) if bound_ms == STRANDED_GRACE_MS
        ));
        assert_eq!(spawner.plans.lock().len(), 1);
        // Stranded and past the bound → the second actuation.
        assert!(matches!(act.on_deploy_wanted(&req, true, NOW + STRANDED_GRACE_MS), ActuateOutcome::Spawned { attempt: 2, .. }));
        assert_eq!(spawner.plans.lock().len(), 2);
        // A measured deploy on this tier stretches the bound: 60 min measured → 90 min.
        act.record_settled(&req.tip_sha, 60 * 60 * 1000);
        assert_eq!(act.receipt().unwrap().landed_waited_ms, Some(3_600_000)); // unwrap: the receipt was just written
        let later = DeployRequest::new("dddddddddddd", NOW + 10_000_000);
        assert!(matches!(act.on_deploy_wanted(&later, false, NOW + 10_000_000), ActuateOutcome::Spawned { attempt: 1, .. }), "a different tip is a new request");
        assert_eq!(act.receipt().unwrap().last_deploy_ms, Some(3_600_000), "the measurement is carried onto the next receipt"); // unwrap: the receipt was just written
        assert_eq!(reactuation_bound_ms(Some(3_600_000)), 5_400_000);
        assert_eq!(reactuation_bound_ms(Some(60_000)), STRANDED_GRACE_MS, "a fast tier never drops under the tracker's grace");
        assert_eq!(reactuation_bound_ms(None), STRANDED_GRACE_MS);
        assert!(matches!(
            act.on_deploy_wanted(&later, true, NOW + 10_000_000 + 5_400_000 - 1),
            ActuateOutcome::Skipped(ActuateDecision::AwaitingBound { bound_ms: 5_400_000, .. })
        ));
        // The cap: three launches of one tip, then GaveUp — said, not looped.
        let t = NOW + 10_000_000 + 5_400_000;
        assert!(matches!(act.on_deploy_wanted(&later, true, t), ActuateOutcome::Spawned { attempt: 2, .. }));
        assert!(matches!(act.on_deploy_wanted(&later, true, t + 5_400_000), ActuateOutcome::Spawned { attempt: 3, .. }));
        assert!(matches!(act.on_deploy_wanted(&later, true, t + 20_000_000), ActuateOutcome::Skipped(ActuateDecision::GaveUp { attempts: 3 })));
        let n = spawner.plans.lock().len();
        // A request for the SAME tip written after the receipt (a rollback by hand) is new.
        let again = DeployRequest::new("dddddddddddd", t + 30_000_000);
        assert!(matches!(act.on_deploy_wanted(&again, false, t + 30_000_000), ActuateOutcome::Spawned { attempt: 1, .. }));
        assert_eq!(spawner.plans.lock().len(), n + 1);
        // A failed launch is an attempt too — the same ladder, never a fresh try per tick.
        let (failing, failing_spawner) = actuator(&dir.path().join("f"), None, None);
        assert!(matches!(failing.on_deploy_wanted(&req, false, NOW), ActuateOutcome::SpawnFailed { attempt: 1, .. }));
        assert!(matches!(failing.on_deploy_wanted(&req, false, NOW + 300_000), ActuateOutcome::Skipped(ActuateDecision::AlreadyActuated { .. })));
        assert_eq!(failing_spawner.plans.lock().len(), 1);
    }

    // what this catches: the boot-time grade of the last actuation — the short stamped
    // sha IS the requested full tip (landed), a different running build is stale, no
    // receipt is unknown (never stale by absence) — and it lands on the receipt so an
    // operator reading the state dir sees the verdict beside the action.
    #[test]
    fn the_boot_outcome_is_landed_stale_or_unknown() {
        let dir = tempfile::tempdir().unwrap(); // unwrap: the test's own dir
        let (act, _) = actuator(dir.path(), None, Some(1));
        assert_eq!(act.report_boot_outcome("c2344d758", NOW), BootOutcome::Unknown, "no receipt = this boot was not ours to grade");
        let req = DeployRequest::new("c2344d758225d87911d1ee2934b4e7e42673c26e", NOW);
        act.on_deploy_wanted(&req, false, NOW);
        assert_eq!(classify_boot(act.receipt().as_ref(), "c2344d758"), BootOutcome::Landed, "the short running sha is the requested tip");
        assert_eq!(classify_boot(act.receipt().as_ref(), "38be2e1a9"), BootOutcome::Stale);
        assert_eq!(classify_boot(act.receipt().as_ref(), "c2344"), BootOutcome::Stale, "five characters is a coincidence, not a commit");
        assert_eq!(act.report_boot_outcome("38be2e1a9", NOW + 1), BootOutcome::Stale);
        let r = act.receipt().unwrap(); // unwrap: the receipt stands
        assert_eq!((r.boot_outcome.as_deref(), r.booted_ms), (Some("stale"), Some(NOW + 1)));
        assert_eq!(act.report_boot_outcome("c2344d758", NOW + 2), BootOutcome::Landed);
        assert_eq!(act.receipt().unwrap().boot_outcome.as_deref(), Some("landed")); // unwrap: the receipt stands
    }
}
