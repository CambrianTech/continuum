//! The macOS supervisor boundary: launchd owns the core, the CLI hands it the launch.
//!
//! Card a1bd8b58, audit #4223 row D-mac. Measured 2026-09-19 on IntelMac: the core ran
//! as `ppid 1, sess 0` with NO launchd job — an orphan of `start-server.sh`, alive until
//! the first crash and then dark until a human typed `continuum start`. Installing the
//! LaunchAgent did not close it: `continuum reboot` stopped the core (a clean exit, which
//! a `KeepAlive { Crashed }` job honours by staying down) and then SPAWNED a new one
//! through the start script, outside launchd — an orphan again from the first deploy.
//!
//! What this module makes true: when a launchd job for the core exists, `start` and
//! `reboot` do not spawn. They STAGE the verified artifact into the slot the job execs
//! (rename-aside, copy, re-read the sha) and `launchctl kickstart -k` the job, then wait
//! for the socket and refuse the receipt unless launchd's pid IS the core's. The
//! supervisor is the only launcher; the deploy is a swap under it.
//!
//! What it does NOT make true, said plainly because the receipt failed on it: a per-USER
//! agent cannot heal on a Mac whose gui domain is in on-demand-only mode — launchd's own
//! words at 13:32Z: `pending spawn, domain in on-demand-only mode: com.continuum.core`.
//! `RunAtLoad` did not fire at bootstrap and `KeepAlive` did not relaunch after `kill -9`
//! (127 s dark). The system LaunchDaemon (`install-service.sh install --system`, sudo
//! once) is the session-independent answer, the same elevation class the 5090 needs for
//! S4U (card 7b56a84b). [`supervision_verdict`] says which of these a node is in, so the
//! receipt is a self-running check ([`SupervisorReport`]), never a hand test.
//!
//! Pure functions (parsing, the verdict) compile and test everywhere; only the parts that
//! touch `launchctl`, the filesystem or signals are `cfg(target_os = "macos")`.

use std::path::{Path, PathBuf};

/// The launchd label the installer registers (`install-service.sh`: `LABEL`).
pub const LABEL: &str = "com.continuum.core";

/// Which launchd domain owns the job. `System` is the LaunchDaemon (survives logout,
/// immune to the gui domain's on-demand mode); `Gui(uid)` is the per-user agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Domain {
    System,
    Gui(u32),
}

impl Domain {
    /// The `launchctl` service target, e.g. `system/com.continuum.core` or
    /// `gui/501/com.continuum.core`.
    pub fn target(&self) -> String {
        match self {
            Domain::System => format!("system/{LABEL}"),
            Domain::Gui(uid) => format!("gui/{uid}/{LABEL}"),
        }
    }
    /// Where the installer wrote the plist for this domain.
    pub fn plist_path(&self, home: &Path) -> PathBuf {
        match self {
            Domain::System => PathBuf::from(format!("/Library/LaunchDaemons/{LABEL}.plist")),
            Domain::Gui(_) => home
                .join("Library/LaunchAgents")
                .join(format!("{LABEL}.plist")),
        }
    }
}

/// Pick the domain from what `launchctl print` answered for each. The daemon wins when
/// both are registered: it is the one that outlives the session, and two jobs for one
/// socket is an installer mistake this should name, not silently pick the weaker of.
pub fn choose_domain(system_present: bool, gui_present: bool, uid: u32) -> Option<Domain> {
    match (system_present, gui_present) {
        (true, _) => Some(Domain::System),
        (false, true) => Some(Domain::Gui(uid)),
        (false, false) => None,
    }
}

/// The artifact path the job execs, read from the installer's plist. The installer
/// writes `ProgramArguments = [/bin/bash, -lc, "…; exec \"<slot>\" \"<socket>\""]` — the
/// wrapper is the launch environment (PATH, config.env, ORT), the `exec` is the core —
/// or, once the wrapper is gone (audit item 7), `[<slot>, <socket>]` directly. Both are
/// read here so the stage step writes where launchd will look, never a guessed path.
pub fn slot_from_plist(plist: &str) -> Option<PathBuf> {
    // The wrapper form: the slot is the first quoted path after `exec `.
    if let Some(i) = plist.find("exec \\\"").or_else(|| plist.find("exec \"")) {
        let rest = &plist[i..];
        let open = rest.find('"')? + 1;
        let rest = &rest[open..];
        let rest = rest.strip_prefix('\\').unwrap_or(rest); // unwrap_or: no backslash = the quote was not escaped; the same slice either way
        let close = rest.find(['"', '\\'])?;
        let p = &rest[..close];
        if p.starts_with('/') {
            return Some(PathBuf::from(p));
        }
    }
    // The direct form: the first <string> under ProgramArguments is the binary.
    let args = plist.find("<key>ProgramArguments</key>")?;
    let rest = &plist[args..];
    let s = rest.find("<string>")? + "<string>".len();
    let e = rest[s..].find("</string>")? + s;
    let p = &rest[s..e];
    if p.starts_with('/') && !p.starts_with("/bin/bash") && !p.starts_with("/bin/sh") {
        return Some(PathBuf::from(p));
    }
    None
}

/// The one fact `launchctl print <target>` carries that a launch receipt needs: the pid
/// launchd holds for the job, if it is running. `None` = registered but not running.
pub fn pid_from_launchctl_print(output: &str) -> Option<u32> {
    output
        .lines()
        .map(str::trim)
        .find_map(|l| l.strip_prefix("pid = "))
        .and_then(|v| v.trim().parse().ok())
}

/// How many times launchd has spawned the job (`runs = N`), refused spawns included.
/// A spawn failure is evidence about THIS start only when `runs` moved after it was asked
/// for; the `job state = spawn failed` line otherwise lingers from the previous attempt.
pub fn runs_from_launchctl_print(output: &str) -> Option<u64> {
    output
        .lines()
        .map(str::trim)
        .find_map(|l| l.strip_prefix("runs = "))
        .and_then(|v| v.trim().parse().ok())
}

/// PURE: whether launchd has spawned the job since a wait began. An unreadable count on
/// either side is not evidence of a fresh attempt, so the wait falls back to its ceiling.
pub fn fresh_spawn_attempt(at_start: Option<u64>, now: Option<u64>) -> bool {
    matches!((at_start, now), (Some(a), Some(n)) if n > a)
}

/// PURE: launchd's refusal of the start this wait is for, if the print shows one. A
/// `spawn failed` line counts only when launchd has spawned since `runs_before` (read
/// before the trigger); otherwise it is the previous attempt's and says nothing about this
/// one. Pinned by a test so a caller that reads its baseline after the trigger is caught.
pub fn refusal_of_this_start(runs_before: Option<u64>, print: &str) -> Option<SpawnFailed> {
    fresh_spawn_attempt(runs_before, runs_from_launchctl_print(print))
        .then(|| spawn_failed_from_launchctl_print(print))
        .flatten()
}

/// Where a start stands after launchd's attempts since the trigger, as [`wait_owned`] acts on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartVerdict {
    /// No refusal of this start yet: keep waiting for the core to answer.
    Pending,
    /// The first spawn of a new build was refused by its launch constraint, and the repair
    /// spawn has not happened yet. Not a verdict on the build: [`wait_owned`] triggers it.
    AwaitingRepairRespawn(SpawnFailed),
    /// launchd refused this start and, for a launch-constraint refusal, its repair respawn too.
    Refused(SpawnFailed),
}

/// How long [`wait_owned`] waits for the repair spawn it triggered after a launch-constraint
/// refusal. launchd throttles a job that ran under its minimum runtime and defers the next
/// spawn about 8-10 s (M5: refused 15:02:28.820, kicked 15:02:30.845, spawned 15:02:38.855);
/// the rest is headroom. Past it, the refusal stands.
pub const LWCR_REPAIR_GRACE: std::time::Duration = std::time::Duration::from_secs(30);

/// PURE: the verdict on this start. Background Task Management pins a legacy daemon to a
/// lightweight code requirement (LWCR) for the binary it last ran. An ad-hoc signed build
/// has a new code hash every time, so the FIRST spawn of a newly staged build is killed with
/// `OS_REASON_CODESIGNING | Launch Constraint Violation`, launchd logs `Requesting LWCR
/// update on next spawn`, and that next spawn re-pins the job to whatever binary is in the
/// slot and runs it (M5 2026-10-03, system log at 13:54 and 15:02). launchd does NOT make
/// that spawn on its own: the job is `KeepAlive { Crashed }` and a codesigning kill is not a
/// crash, so no respawn came in 30 s (M5 23:35Z); in the 15:02 log it was the rollback's
/// kickstart that caused it, which is why the OLD build got re-pinned. So a codesigning
/// refusal is final only once launchd has spawned TWICE since `runs_before` and the second
/// was refused too. Any other refusal is final on the first spawn, as before.
pub fn start_verdict(runs_before: Option<u64>, print: &str) -> StartVerdict {
    let Some(failed) = refusal_of_this_start(runs_before, print) else {
        return StartVerdict::Pending;
    };
    let codesigning = failed
        .reason
        .as_deref()
        .is_some_and(|r| r.starts_with("OS_REASON_CODESIGNING"));
    let repaired_and_refused = matches!(
        (runs_before, runs_from_launchctl_print(print)),
        (Some(before), Some(now)) if now >= before + 2
    );
    if codesigning && !repaired_and_refused {
        StartVerdict::AwaitingRepairRespawn(failed)
    } else {
        StartVerdict::Refused(failed)
    }
}

/// launchd refused to start the job's program: `job state = spawn failed` with no pid.
/// `reason` is launchd's `last exit reason` verbatim (M5 2026-10-02:
/// `OS_REASON_CODESIGNING` on a freshly staged core, node dark until a hand).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnFailed {
    pub reason: Option<String>,
}

/// The spawn failure `launchctl print <target>` reports, if any. A running job (a pid)
/// is never a spawn failure, whatever an older `last exit reason` line still says.
pub fn spawn_failed_from_launchctl_print(output: &str) -> Option<SpawnFailed> {
    let field = |key: &str| {
        output
            .lines()
            .map(str::trim)
            .find_map(|l| l.strip_prefix(key))
            .map(|v| v.trim().to_string())
    };
    if pid_from_launchctl_print(output).is_some()
        || field("job state = ").as_deref() != Some("spawn failed")
    {
        return None;
    }
    Some(SpawnFailed {
        reason: field("last exit reason = "),
    })
}

/// What a deploy does after it stopped the serving core and asked launchd to start the
/// staged one. `started` is the kickstart AND the wait for launchd to own a core, so a
/// refused kickstart and a refused spawn take the same road.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandoffNext {
    /// launchd owns the new core.
    Done,
    /// A core answers outside launchd: a different defect, reported as it is.
    Report(String),
    /// Nothing answers: the node is dark, so the build that was serving goes back.
    RollBack(String),
}

pub fn after_staged_start(started: Result<(), String>, core_answering: bool) -> HandoffNext {
    match (started, core_answering) {
        (Ok(()), _) => HandoffNext::Done,
        (Err(why), true) => HandoffNext::Report(why),
        (Err(why), false) => HandoffNext::RollBack(why),
    }
}

/// Put the build a stage moved aside (`<slot>.prev`) back into `slot`. The refused
/// build is kept for inspection under a name of its own (`<slot>.failed-<now_ms>`):
/// an earlier `.failed` someone is still looking at is never overwritten or deleted.
/// Returns where the refused build was kept.
pub fn restore_previous_in(slot: &Path, now_ms: u64) -> Result<PathBuf, String> {
    let prev = slot.with_extension("prev");
    if !prev.exists() {
        return Err(format!(
            "no previous build at {} to restore",
            prev.display()
        ));
    }
    let kept = slot.with_extension(format!("failed-{now_ms}"));
    if slot.exists() {
        if kept.exists() {
            return Err(format!(
                "{} already exists; not overwriting a kept build",
                kept.display()
            ));
        }
        std::fs::rename(slot, &kept)
            .map_err(|e| format!("cannot move {} aside: {e}", slot.display()))?;
    }
    std::fs::rename(&prev, slot).map_err(|e| {
        format!(
            "cannot restore {} into {}: {e}",
            prev.display(),
            slot.display()
        )
    })?;
    Ok(kept)
}

/// Whether launchd's own log says the domain cannot spawn on demand — the line that
/// explained the failed receipt. Matched on the substring launchd prints, verbatim.
pub fn domain_is_on_demand_only(launchd_log: &str) -> bool {
    launchd_log.contains("domain in on-demand-only mode")
}

/// What a node's supervision actually is, from facts a check can read without
/// crashing anything. The receipt verb turns this into a line and an exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisionVerdict {
    /// No launchd job at all: the core (if any) is an orphan; a crash is dark until a hand.
    Unsupervised,
    /// A job exists but launchd's pid is not the core answering on the socket: the core
    /// was spawned outside the job (the pre-#a1bd8b58 `reboot`), so a crash is dark.
    JobPresentCoreOrphaned {
        job_pid: Option<u32>,
        core_pid: Option<u32>,
    },
    /// launchd owns the running core, but its domain cannot spawn on demand — a crash
    /// will not be healed. The user agent on a gui domain in on-demand-only mode.
    OwnedButCannotHeal { domain: Domain },
    /// launchd owns the running core and can relaunch it.
    Supervised { domain: Domain, pid: u32 },
}

/// The pure rule. `on_demand_only` is what launchd's log said about the domain; it is
/// only decisive for `Gui` (a LaunchDaemon in the system domain is never on-demand-only).
pub fn supervision_verdict(
    domain: Option<Domain>,
    job_pid: Option<u32>,
    core_pid: Option<u32>,
    on_demand_only: bool,
) -> SupervisionVerdict {
    let Some(domain) = domain else {
        return SupervisionVerdict::Unsupervised;
    };
    match (job_pid, core_pid) {
        (Some(j), Some(c)) if j == c => {
            if matches!(domain, Domain::Gui(_)) && on_demand_only {
                SupervisionVerdict::OwnedButCannotHeal { domain }
            } else {
                SupervisionVerdict::Supervised { domain, pid: j }
            }
        }
        (job_pid, core_pid) => SupervisionVerdict::JobPresentCoreOrphaned { job_pid, core_pid },
    }
}

impl SupervisionVerdict {
    /// One line, the operator's and the room's. Names the owed thing.
    pub fn line(&self) -> String {
        match self {
            SupervisionVerdict::Unsupervised => {
                "UNSUPERVISED: no launchd job — a crash is dark until a hand; run `continuum install` (sudo, once)".to_string()
            }
            SupervisionVerdict::JobPresentCoreOrphaned { job_pid, core_pid } => format!(
                "ORPHANED: launchd job pid {:?} is not the core answering (pid {:?}) — the core was spawned outside the job; \
                 `continuum reboot` hands the launch to launchd from now on",
                job_pid, core_pid
            ),
            SupervisionVerdict::OwnedButCannotHeal { domain } => format!(
                "OWNED, CANNOT HEAL: {} runs the core but its domain is on-demand-only (launchd: \"pending spawn, domain in \
                 on-demand-only mode\") — KeepAlive will not relaunch; run `continuum install` for the system LaunchDaemon (sudo, once)",
                domain.target()
            ),
            // Structural, not proven: launchd owns the pid and the log has not said the domain
            // cannot spawn. Only `--crash-test` proves the heal — the domain's print carries no
            // mode field, and the on-demand line appears only when a spawn is attempted.
            SupervisionVerdict::Supervised { domain, pid } => format!(
                "SUPERVISED (structurally): {} owns the core (pid {pid}) — run `continuum supervisor-status --crash-test` for the heal receipt",
                domain.target()
            ),
        }
    }
    pub fn is_healthy(&self) -> bool {
        matches!(self, SupervisionVerdict::Supervised { .. })
    }
}

/// One way the registered supervision differs from the contract — the read half of the
/// macOS supervisor arm of `continuum install` (Joel: one idempotent verb; each arm
/// reads, changes only what drifted, says so). The write half is the register + hand-off;
/// the verb re-reads after it and demands this list empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacDrift {
    /// No launchd job at all.
    Absent,
    /// Registered in the other domain than asked for (agent vs daemon).
    Domain { have: Domain, want: Domain },
    /// The plist execs a bash wrapper, not the binary (audit item 7).
    WrapperCommand,
    /// The plist lacks `AbandonProcessGroup`: a kickstart or heal would kill the lanes.
    KillsLanes,
    /// The job exists but the core on the socket is not its pid (or nothing answers).
    NotOwned {
        job_pid: Option<u32>,
        core_pid: Option<u32>,
    },
    /// The agent's domain is on-demand-only: registered, cannot heal.
    CannotHeal,
}

/// The pure rule over what the node can read: the job (if any) and its plist text, the
/// pids, launchd's word on the domain. `want` is the domain asked for.
pub fn mac_drift(
    have: Option<(&Domain, &str)>,
    want: &Domain,
    job_pid: Option<u32>,
    core_pid: Option<u32>,
    on_demand_only: bool,
) -> Vec<MacDrift> {
    let Some((domain, plist)) = have else {
        return vec![MacDrift::Absent];
    };
    let mut out = Vec::new();
    if domain != want {
        out.push(MacDrift::Domain {
            have: domain.clone(),
            want: want.clone(),
        });
    }
    if plist.contains("<string>/bin/bash</string>") || plist.contains("<string>/bin/sh</string>") {
        out.push(MacDrift::WrapperCommand);
    }
    if !plist.contains("<key>AbandonProcessGroup</key><true/>") {
        out.push(MacDrift::KillsLanes);
    }
    match supervision_verdict(Some(domain.clone()), job_pid, core_pid, on_demand_only) {
        SupervisionVerdict::Supervised { .. } => {}
        SupervisionVerdict::OwnedButCannotHeal { .. } => out.push(MacDrift::CannotHeal),
        SupervisionVerdict::JobPresentCoreOrphaned { job_pid, core_pid } => {
            out.push(MacDrift::NotOwned { job_pid, core_pid })
        }
        SupervisionVerdict::Unsupervised => out.push(MacDrift::Absent),
    }
    out
}

/// Everything the plist `continuum install` writes is built from — the binary as the
/// command (audit #4223 item 7: no `bash -lc` wrapper). `env` is what launchd must
/// carry for the exec to find its libraries and tools (PATH, the runtime library dirs);
/// it is NOT `config.env` — that file is applied by the core to its own process on every
/// boot (`config_env::apply_to_process`), so an edit followed by `reboot` takes effect
/// under launchd exactly as it does on the direct path, with no re-install.
pub struct PlistSpec<'a> {
    pub domain: &'a Domain,
    /// The artifact launchd execs — the slot `stage` writes into.
    pub slot: &'a Path,
    pub socket: &'a str,
    pub home: &'a Path,
    /// The operator; a LaunchDaemon runs AS them (never root — `~/.continuum` is theirs).
    pub user: &'a str,
    pub env: &'a [(String, String)],
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The digest that binds a draft plist to the elevated argv. The consent a human gives
/// to `sudo` covers the command line it shows and nothing read later from a file any
/// same-user process can rewrite — so the elevated half refuses a draft whose bytes are
/// not the ones the unelevated half hashed (Fable on #4232; the same window here).
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// The plist, verbatim what launchd will read. Pure so the round trip is a unit test:
/// [`slot_from_plist`] of the rendered text is the slot that went in — the stage step
/// and the installer can never disagree on where the binary lives.
pub fn render_plist(spec: &PlistSpec<'_>) -> String {
    let data = spec.home.join(".continuum");
    let logs = data.join("logs");
    let mut env = String::new();
    for (k, v) in spec.env {
        env.push_str(&format!(
            "    <key>{}</key><string>{}</string>\n",
            xml(k),
            xml(v)
        ));
    }
    let user = match spec.domain {
        Domain::System => format!("  <key>UserName</key><string>{}</string>\n", xml(spec.user)),
        Domain::Gui(_) => String::new(),
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array><string>{slot}</string><string>{socket}</string></array>
  <key>RunAtLoad</key><true/>
  <!-- Crash-only relaunch: survive a CRASH (abnormal exit) and a reboot (RunAtLoad),
       but honour an explicit `continuum stop` (clean exit 0) so it stays down for dev.
       Unconditional KeepAlive would relaunch a deliberate stop, fighting the developer
       AND an operator draining a grid node. -->
  <key>KeepAlive</key><dict><key>Crashed</key><true/></dict>
  <!-- The serving lanes (llama-server) share the core's process group. launchd's
       default on a job exit is to SIGKILL the rest of that group, which would turn
       every kickstart and every heal into a cold model load; `reboot` leaves a lane
       up for the next core to ADOPT, and this keeps that true under launchd. -->
  <key>AbandonProcessGroup</key><true/>
  <key>WorkingDirectory</key><string>{data}</string>
  <key>StandardOutPath</key><string>{out}</string>
  <key>StandardErrorPath</key><string>{err}</string>
  <key>ProcessType</key><string>Background</string>
{user}  <key>EnvironmentVariables</key>
  <dict>
{env}  </dict>
</dict>
</plist>
"#,
        slot = xml(&spec.slot.display().to_string()),
        socket = xml(spec.socket),
        data = xml(&data.display().to_string()),
        out = xml(&logs.join("service.out.log").display().to_string()),
        err = xml(&logs.join("service.err.log").display().to_string()),
    )
}

#[cfg(target_os = "macos")]
pub mod live {
    //! The parts that touch launchd, the slot and the socket.
    use super::*;
    use std::process::Command;
    use std::time::{Duration, Instant};

    /// A registered launchd job for the core, with the slot its plist execs.
    #[derive(Debug, Clone)]
    pub struct Job {
        pub domain: Domain,
        pub slot: PathBuf,
    }

    fn uid() -> u32 {
        // SAFETY: getuid has no preconditions and cannot fail.
        unsafe { libc::getuid() }
    }

    fn launchctl_print(domain: &Domain) -> Option<String> {
        let out = Command::new("launchctl")
            .args(["print", &domain.target()])
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// The job that owns the core on this Mac, if the installer registered one. `None`
    /// is the unsupervised node — every launch then falls back to today's path.
    pub fn job() -> Result<Option<Job>, String> {
        let uid = uid();
        let system = launchctl_print(&Domain::System).is_some();
        let gui = launchctl_print(&Domain::Gui(uid)).is_some();
        let Some(domain) = choose_domain(system, gui, uid) else {
            return Ok(None);
        };
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or("HOME is not set")?;
        let plist_path = domain.plist_path(&home);
        let plist = std::fs::read_to_string(&plist_path).map_err(|e| {
            format!(
                "launchd job {} is registered but its plist is unreadable at {}: {e}",
                domain.target(),
                plist_path.display()
            )
        })?;
        let slot = slot_from_plist(&plist).ok_or_else(|| {
            format!(
                "launchd job {} plist at {} names no core artifact to stage into",
                domain.target(),
                plist_path.display()
            )
        })?;
        Ok(Some(Job { domain, slot }))
    }

    /// The pid of the core SERVING `socket` — the process whose argv is the core binary
    /// with this socket, the exact form launchd's wrapper execs and `ps` shows. NOT the
    /// CLI's `<socket>.pid` file: only the CLI's own launcher writes that, so a core
    /// launchd started has none, and the first live run of this verb read the launchd
    /// job as ORPHANED on that missing file (2026-09-19 14:1xZ). The socket's holder is
    /// the truth; a pidfile is a note the launcher left.
    pub fn serving_core_pid(socket: &str) -> Option<u32> {
        // Anchored: on a node still on the script path the core's argv also appears
        // inside `caffeinate -s -i …continuum-core-server …sock`, a lower pid that an
        // unanchored match returned first (Fable, #4228 review).
        let out = Command::new("pgrep")
            .args([
                "-f",
                &format!("^([^ ]*/)?continuum-core-server {}$", regex_escape(socket)),
            ])
            .output()
            .ok()?;
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| l.trim().parse::<u32>().ok())
            .next()
    }

    /// Escape a path for the ERE `pgrep -f` matches: the socket path is data, not pattern.
    fn regex_escape(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for c in s.chars() {
            if "\\.^$|()[]{}*+?".contains(c) {
                out.push('\\');
            }
            out.push(c);
        }
        out
    }

    /// The pid launchd holds for the job right now, if running.
    pub fn job_pid(domain: &Domain) -> Option<u32> {
        launchctl_print(domain).and_then(|o| pid_from_launchctl_print(&o))
    }

    /// Whether launchd said, in the last `window`, that the job's domain is on-demand-only.
    /// Reads the unified log; an unreadable log reads as "not said" and the verdict then
    /// over-reports health — so the caller prints the window it read.
    pub fn domain_on_demand_only_recently(window: Duration) -> bool {
        let last = format!("{}s", window.as_secs().max(60));
        let out = Command::new("log")
            .args([
                "show",
                "--last",
                &last,
                "--style",
                "compact",
                "--predicate",
                &format!("process == \"launchd\" AND eventMessage CONTAINS \"{LABEL}\""),
            ])
            .output();
        match out {
            Ok(o) if o.status.success() => {
                domain_is_on_demand_only(&String::from_utf8_lossy(&o.stdout))
            }
            _ => false,
        }
    }

    /// Stage a verified artifact INTO the job's slot: move the old aside (a running
    /// binary may be renamed, never overwritten), copy the new one in, and return the
    /// staged path for the caller to re-read its sha. The Windows twin is
    /// `PreparedCoreService::stage`; the slot is the plist's, never a guess.
    pub fn stage(job: &Job, artifact: &Path) -> Result<PathBuf, String> {
        if let Some(dir) = job.slot.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("cannot create slot dir {}: {e}", dir.display()))?;
        }
        let same = std::fs::canonicalize(artifact).ok() == std::fs::canonicalize(&job.slot).ok();
        if !same {
            if job.slot.exists() {
                let prev = job.slot.with_extension("prev");
                let _ = std::fs::remove_file(&prev);
                std::fs::rename(&job.slot, &prev)
                    .map_err(|e| format!("cannot move {} aside: {e}", job.slot.display()))?;
            }
            std::fs::copy(artifact, &job.slot).map_err(|e| {
                format!(
                    "cannot stage {} into {}: {e}",
                    artifact.display(),
                    job.slot.display()
                )
            })?;
        }
        Ok(job.slot.clone())
    }

    /// Put the build `stage` moved aside back into the slot; see [`super::restore_previous_in`].
    pub fn restore_previous(job: &Job) -> Result<PathBuf, String> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0); // unwrap_or: a clock before 1970 only names the kept file 0
        super::restore_previous_in(&job.slot, now_ms)
    }

    /// `launchctl kickstart -k`: launchd stops the running instance (SIGTERM — the core's
    /// save-and-join exit) and starts the job again from the slot. Works even on a gui
    /// domain in on-demand-only mode, which is why a deploy through it succeeds where
    /// KeepAlive does not.
    pub fn kickstart(domain: &Domain) -> Result<(), String> {
        let out = Command::new("launchctl")
            .args(["kickstart", "-k", &domain.target()])
            .output()
            .map_err(|e| format!("launchctl kickstart: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "launchctl kickstart -k {} failed: {}",
                domain.target(),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(())
    }

    /// `launchctl kickstart` WITHOUT `-k`: start the job if it is not running, never stop
    /// one that is. The LWCR repair spawn after a launch-constraint refusal (see
    /// [`super::start_verdict`]): the job is down, so there is nothing to stop, and a plain
    /// kickstart of a system job is permitted to its owning user where `-k` is not (M5
    /// 2026-10-03: `kickstart -k` refused with `1: Operation not permitted`).
    pub fn kickstart_if_stopped(domain: &Domain) -> Result<(), String> {
        let out = Command::new("launchctl")
            .args(["kickstart", &domain.target()])
            .output()
            .map_err(|e| format!("launchctl kickstart: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "launchctl kickstart {} failed: {}",
                domain.target(),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(())
    }

    /// launchd's spawn count for the job right now. Read it BEFORE the action that should
    /// make launchd spawn (a kickstart, a kill), and hand it to [`wait_owned`]: read after,
    /// an instant refusal is already counted and looks stale (BIGGIEDESK on #4681).
    pub fn spawn_runs(domain: &Domain) -> Option<u64> {
        launchctl_print(domain)
            .as_deref()
            .and_then(runs_from_launchctl_print)
    }

    /// Wait until `up()` reports the core answering AND launchd's pid for the job is the
    /// core's own — the receipt refuses an unsupervised core that merely happens to answer.
    /// `runs_before` is [`spawn_runs`] read before the triggering action; a spawn failure
    /// ends the wait early only once launchd has spawned since then.
    pub async fn wait_owned<F, Fut>(
        job: &Job,
        runs_before: Option<u64>,
        core_pid: impl Fn() -> Option<u32>,
        up: F,
        ceiling: Duration,
    ) -> Result<u32, String>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let started = Instant::now();
        // A spawn failure counts only once launchd has spawned again since `runs_before`:
        // right after a refusal, `job state = spawn failed` is the previous attempt's, and
        // reading it as this one's declared a restored node dark while launchd was starting
        // it (M5 2026-10-03 10:08Z: DARK logged, the restored core answered 30 s later).
        let mut ticks = tokio::time::interval(Duration::from_secs(2));
        let mut awaiting_repair_since: Option<Instant> = None;
        loop {
            ticks.tick().await;
            if up().await {
                let j = job_pid(&job.domain);
                let c = core_pid();
                match (j, c) {
                    (Some(j), Some(c)) if j == c => return Ok(j),
                    (j, c) => {
                        return Err(format!(
                            "a core answered but launchd does not own it (job pid {j:?}, core pid {c:?}) — refusing an unsupervised receipt"
                        ))
                    }
                }
            }
            // A refusal of THIS start ends the wait at once, with launchd's own reason;
            // waiting out the ceiling would only keep the node dark longer. The exception is
            // a new build's launch-constraint refusal: the NEXT spawn repairs it (see
            // `start_verdict`), so the wait triggers that spawn once, on the new build still
            // in the slot, and gives it LWCR_REPAIR_GRACE.
            let print = launchctl_print(&job.domain);
            let refused = |failed: &SpawnFailed, note: &str| {
                format!(
                    "launchd could not start {} ({}){note}; job state = spawn failed",
                    job.slot.display(),
                    failed.reason.as_deref().unwrap_or("no exit reason given")
                )
            };
            match print.as_deref().map(|p| start_verdict(runs_before, p)) {
                Some(StartVerdict::Refused(failed)) => return Err(refused(&failed, "")),
                Some(StartVerdict::AwaitingRepairRespawn(failed)) => {
                    if awaiting_repair_since.is_none() {
                        awaiting_repair_since = Some(Instant::now());
                        if let Err(e) = kickstart_if_stopped(&job.domain) {
                            return Err(refused(
                                &failed,
                                &format!(", and the repair spawn could not be triggered: {e}"),
                            ));
                        }
                    }
                    if awaiting_repair_since
                        .is_some_and(|since| since.elapsed() >= LWCR_REPAIR_GRACE)
                    {
                        return Err(refused(&failed, ", and the repair spawn did not come up"));
                    }
                }
                Some(StartVerdict::Pending) | None => {}
            }
            if started.elapsed() >= ceiling {
                return Err(format!(
                    "{} did not bring a core up within {}s; inspect ~/.continuum/logs/service.err.log",
                    job.domain.target(),
                    ceiling.as_secs()
                ));
            }
        }
    }

    fn run(cmd: &mut Command, what: &str) -> Result<String, String> {
        let out = cmd.output().map_err(|e| format!("{what}: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "{what} failed ({}): {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Register the core with launchd — the mac arm of `continuum install` (Joel: "it
    /// needs to be in our own binary"). Stages `artifact` into the slot, writes the plist
    /// with the binary as the command, bootstraps the job into `domain` and enables it.
    /// `System` is the LaunchDaemon: three privileged steps through `sudo` — the one
    /// consent macOS requires, the same class as the 5090's UAC (card 7b56a84b). It does
    /// NOT start the core: the caller kickstarts and waits, so the receipt is one place.
    pub fn install(
        domain: Domain,
        artifact: &Path,
        slot: &Path,
        socket: &str,
        env: &[(String, String)],
    ) -> Result<Job, String> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or("HOME is not set")?;
        let user = std::env::var("USER").map_err(|_| "USER is not set".to_string())?;
        let job = Job {
            domain,
            slot: slot.to_path_buf(),
        };
        stage(&job, artifact)?;
        let data = home.join(".continuum");
        std::fs::create_dir_all(data.join("logs"))
            .map_err(|e| format!("cannot create ~/.continuum/logs: {e}"))?;
        let plist = render_plist(&PlistSpec {
            domain: &job.domain,
            slot: &job.slot,
            socket,
            home: &home,
            user: &user,
            env,
        });
        let plist_path = job.domain.plist_path(&home);
        // Written under ~/.continuum first so `plutil -lint` reads the exact bytes that
        // will be installed, and a root-owned destination is never half-written.
        let draft = data.join(format!("{LABEL}.plist.draft"));
        std::fs::write(&draft, plist)
            .map_err(|e| format!("cannot write {}: {e}", draft.display()))?;
        run(
            Command::new("plutil").args(["-lint", &draft.display().to_string()]),
            "plutil -lint",
        )?;
        let target = job.domain.target();
        match &job.domain {
            Domain::System => {
                // ONE consent, for this binary with the draft's digest on the argv. The
                // elevated half (`register_elevated`) re-reads the draft, refuses it if a
                // byte moved, installs it root-owned, bootstraps (RunAtLoad starts the
                // core) — and nothing else. `sudo install`/`launchctl` on the draft path
                // directly would register whatever the file said at that moment.
                let bytes = std::fs::read(&draft)
                    .map_err(|e| format!("cannot re-read {}: {e}", draft.display()))?;
                let digest = sha256_hex(&bytes);
                let exe = std::env::current_exe().map_err(|e| format!("own path: {e}"))?;
                let receipt = run(
                    Command::new("sudo").arg(&exe).args([
                        "install",
                        "--elevated",
                        "--plan",
                        &draft.display().to_string(),
                        "--plan-sha",
                        &digest,
                    ]),
                    "sudo continuum install --elevated",
                )?;
                for line in receipt.lines().filter(|l| !l.trim().is_empty()) {
                    println!("  {line}");
                }
            }
            Domain::Gui(uid) => {
                if let Some(dir) = plist_path.parent() {
                    std::fs::create_dir_all(dir)
                        .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
                }
                let _ = Command::new("launchctl")
                    .args(["bootout", &target])
                    .output();
                std::fs::copy(&draft, &plist_path)
                    .map_err(|e| format!("cannot write {}: {e}", plist_path.display()))?;
                run(
                    Command::new("launchctl").args([
                        "bootstrap",
                        &format!("gui/{uid}"),
                        &plist_path.display().to_string(),
                    ]),
                    "launchctl bootstrap",
                )?;
                let _ = Command::new("launchctl").args(["enable", &target]).output();
            }
        }
        let _ = std::fs::remove_file(&draft);
        Ok(job)
    }

    /// The elevated half of a system install, run as root by `sudo` with the draft's
    /// digest on its argv: verify, install root-owned under /Library/LaunchDaemons,
    /// bootstrap into the system domain. No query, no decision, no other write.
    pub fn register_elevated(plan: &Path, sha256: &str) -> Result<Vec<String>, String> {
        // SAFETY: geteuid has no preconditions and cannot fail.
        if unsafe { libc::geteuid() } != 0 {
            return Err(
                "install --elevated runs as root under sudo; it is not a verb to type".to_string(),
            );
        }
        let bytes = std::fs::read(plan)
            .map_err(|e| format!("cannot read the plan {}: {e}", plan.display()))?;
        let actual = sha256_hex(&bytes);
        if actual != sha256 {
            return Err(format!(
                "REFUSED: the plan at {} is not the one consented to (sha256 {actual} ≠ {sha256}) — it changed between the consent and this read; nothing was registered",
                plan.display()
            ));
        }
        if slot_from_plist(&String::from_utf8_lossy(&bytes)).is_none() {
            return Err(
                "REFUSED: the plan names no core artifact; nothing was registered".to_string(),
            );
        }
        let plist_path = Domain::System.plist_path(Path::new("/"));
        let target = Domain::System.target();
        // The invoking user's agent, if any, goes first: two jobs for one socket is the
        // mistake `choose_domain` names, not one to make. SUDO_UID is sudo's own record.
        if let Some(uid) = std::env::var("SUDO_UID")
            .ok()
            .and_then(|u| u.parse::<u32>().ok())
        {
            let _ = Command::new("launchctl")
                .args(["bootout", &Domain::Gui(uid).target()])
                .output();
        }
        let _ = Command::new("launchctl")
            .args(["bootout", &target])
            .output();
        std::fs::write(&plist_path, &bytes)
            .map_err(|e| format!("cannot write {}: {e}", plist_path.display()))?;
        std::os::unix::fs::chown(&plist_path, Some(0), Some(0))
            .map_err(|e| format!("cannot chown {}: {e}", plist_path.display()))?;
        std::fs::set_permissions(
            &plist_path,
            std::os::unix::fs::PermissionsExt::from_mode(0o644),
        )
        .map_err(|e| format!("cannot chmod {}: {e}", plist_path.display()))?;
        run(
            Command::new("launchctl").args([
                "bootstrap",
                "system",
                &plist_path.display().to_string(),
            ]),
            "launchctl bootstrap system",
        )?;
        let _ = Command::new("launchctl").args(["enable", &target]).output();
        Ok(vec![
            format!(
                "registered {target} from a plan whose sha256 matched the consent ({})",
                &sha256[..12]
            ),
            format!(
                "plist {} root:wheel 0644; bootstrapped (RunAtLoad starts the core)",
                plist_path.display()
            ),
        ])
    }

    /// Unregister the job from whichever domain holds it (both are tried; the daemon's
    /// half through `sudo`). The staged binary stays — it is the operator's artifact.
    pub fn uninstall() -> Result<Vec<Domain>, String> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or("HOME is not set")?;
        let mut removed = Vec::new();
        for domain in [Domain::System, Domain::Gui(uid())] {
            if launchctl_print(&domain).is_none() && !domain.plist_path(&home).exists() {
                continue;
            }
            let plist = domain.plist_path(&home).display().to_string();
            match domain {
                Domain::System => {
                    let _ = Command::new("sudo")
                        .args(["launchctl", "bootout", &domain.target()])
                        .output();
                    run(
                        Command::new("sudo").args(["rm", "-f", &plist]),
                        "sudo rm (plist)",
                    )?;
                }
                Domain::Gui(_) => {
                    let _ = Command::new("launchctl")
                        .args(["bootout", &domain.target()])
                        .output();
                    let _ = std::fs::remove_file(&plist);
                }
            }
            removed.push(domain);
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests;
