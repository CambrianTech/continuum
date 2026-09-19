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
            Domain::Gui(_) => home.join("Library/LaunchAgents").join(format!("{LABEL}.plist")),
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
        let rest = rest.strip_prefix('\\').unwrap_or(rest);
        let close = rest.find(|c| c == '"' || c == '\\')?;
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
    JobPresentCoreOrphaned { job_pid: Option<u32>, core_pid: Option<u32> },
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
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The plist, verbatim what launchd will read. Pure so the round trip is a unit test:
/// [`slot_from_plist`] of the rendered text is the slot that went in — the stage step
/// and the installer can never disagree on where the binary lives.
pub fn render_plist(spec: &PlistSpec<'_>) -> String {
    let data = spec.home.join(".continuum");
    let logs = data.join("logs");
    let mut env = String::new();
    for (k, v) in spec.env {
        env.push_str(&format!("    <key>{}</key><string>{}</string>\n", xml(k), xml(v)));
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
        let out = Command::new("launchctl").args(["print", &domain.target()]).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
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
        let home = std::env::var_os("HOME").map(PathBuf::from).ok_or("HOME is not set")?;
        let plist_path = domain.plist_path(&home);
        let plist = std::fs::read_to_string(&plist_path)
            .map_err(|e| format!("launchd job {} is registered but its plist is unreadable at {}: {e}", domain.target(), plist_path.display()))?;
        let slot = slot_from_plist(&plist).ok_or_else(|| {
            format!("launchd job {} plist at {} names no core artifact to stage into", domain.target(), plist_path.display())
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
            .args(["-f", &format!("^([^ ]*/)?continuum-core-server {}$", regex_escape(socket))])
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
            Ok(o) if o.status.success() => domain_is_on_demand_only(&String::from_utf8_lossy(&o.stdout)),
            _ => false,
        }
    }

    /// Stage a verified artifact INTO the job's slot: move the old aside (a running
    /// binary may be renamed, never overwritten), copy the new one in, and return the
    /// staged path for the caller to re-read its sha. The Windows twin is
    /// `PreparedCoreService::stage`; the slot is the plist's, never a guess.
    pub fn stage(job: &Job, artifact: &Path) -> Result<PathBuf, String> {
        if let Some(dir) = job.slot.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("cannot create slot dir {}: {e}", dir.display()))?;
        }
        let same = std::fs::canonicalize(artifact).ok() == std::fs::canonicalize(&job.slot).ok();
        if !same {
            if job.slot.exists() {
                let prev = job.slot.with_extension("prev");
                let _ = std::fs::remove_file(&prev);
                std::fs::rename(&job.slot, &prev).map_err(|e| format!("cannot move {} aside: {e}", job.slot.display()))?;
            }
            std::fs::copy(artifact, &job.slot)
                .map_err(|e| format!("cannot stage {} into {}: {e}", artifact.display(), job.slot.display()))?;
        }
        Ok(job.slot.clone())
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

    /// Wait until `up()` reports the core answering AND launchd's pid for the job is the
    /// core's own — the receipt refuses an unsupervised core that merely happens to answer.
    pub async fn wait_owned<F, Fut>(job: &Job, core_pid: impl Fn() -> Option<u32>, up: F, ceiling: Duration) -> Result<u32, String>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let started = Instant::now();
        let mut ticks = tokio::time::interval(Duration::from_secs(2));
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
            if started.elapsed() >= ceiling {
                return Err(format!(
                    "{} did not bring a core up within {}s; inspect ~/.continuum/logs/service.err.log",
                    job.domain.target(),
                    ceiling.as_secs()
                ));
            }
        }
    }

    /// The slot every install stages into: the installer's `~/.continuum/bin` location,
    /// second in `resolve_core_artifact`'s order after `/usr/local/bin`.
    pub fn default_slot(home: &Path) -> PathBuf {
        home.join(".continuum").join("bin").join("continuum-core-server")
    }

    fn run(cmd: &mut Command, what: &str) -> Result<String, String> {
        let out = cmd.output().map_err(|e| format!("{what}: {e}"))?;
        if !out.status.success() {
            return Err(format!("{what} failed ({}): {}", out.status, String::from_utf8_lossy(&out.stderr).trim()));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Register the core with launchd — the mac arm of `continuum install` (Joel: "it
    /// needs to be in our own binary"). Stages `artifact` into the slot, writes the plist
    /// with the binary as the command, bootstraps the job into `domain` and enables it.
    /// `System` is the LaunchDaemon: three privileged steps through `sudo` — the one
    /// consent macOS requires, the same class as the 5090's UAC (card 7b56a84b). It does
    /// NOT start the core: the caller kickstarts and waits, so the receipt is one place.
    pub fn install(domain: Domain, artifact: &Path, socket: &str, env: &[(String, String)]) -> Result<Job, String> {
        let home = std::env::var_os("HOME").map(PathBuf::from).ok_or("HOME is not set")?;
        let user = std::env::var("USER").map_err(|_| "USER is not set".to_string())?;
        let job = Job { domain, slot: default_slot(&home) };
        stage(&job, artifact)?;
        let data = home.join(".continuum");
        std::fs::create_dir_all(data.join("logs")).map_err(|e| format!("cannot create ~/.continuum/logs: {e}"))?;
        let plist = render_plist(&PlistSpec { domain: &job.domain, slot: &job.slot, socket, home: &home, user: &user, env });
        let plist_path = job.domain.plist_path(&home);
        // Written under ~/.continuum first so `plutil -lint` reads the exact bytes that
        // will be installed, and a root-owned destination is never half-written.
        let draft = data.join(format!("{LABEL}.plist.draft"));
        std::fs::write(&draft, plist).map_err(|e| format!("cannot write {}: {e}", draft.display()))?;
        run(Command::new("plutil").args(["-lint", &draft.display().to_string()]), "plutil -lint")?;
        let target = job.domain.target();
        match &job.domain {
            Domain::System => {
                // A stale registration in either domain is booted out first: two jobs
                // for one socket is the mistake `choose_domain` names, not one to make.
                let _ = Command::new("launchctl").args(["bootout", &Domain::Gui(uid()).target()]).output();
                let _ = Command::new("sudo").args(["launchctl", "bootout", &target]).output();
                run(
                    Command::new("sudo").args(["install", "-m", "0644", "-o", "root", "-g", "wheel", &draft.display().to_string(), &plist_path.display().to_string()]),
                    "sudo install (plist)",
                )?;
                run(Command::new("sudo").args(["launchctl", "bootstrap", "system", &plist_path.display().to_string()]), "sudo launchctl bootstrap system")?;
                let _ = Command::new("sudo").args(["launchctl", "enable", &target]).output();
            }
            Domain::Gui(uid) => {
                if let Some(dir) = plist_path.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
                }
                let _ = Command::new("launchctl").args(["bootout", &target]).output();
                std::fs::copy(&draft, &plist_path).map_err(|e| format!("cannot write {}: {e}", plist_path.display()))?;
                run(Command::new("launchctl").args(["bootstrap", &format!("gui/{uid}"), &plist_path.display().to_string()]), "launchctl bootstrap")?;
                let _ = Command::new("launchctl").args(["enable", &target]).output();
            }
        }
        let _ = std::fs::remove_file(&draft);
        Ok(job)
    }

    /// Unregister the job from whichever domain holds it (both are tried; the daemon's
    /// half through `sudo`). The staged binary stays — it is the operator's artifact.
    pub fn uninstall() -> Result<Vec<Domain>, String> {
        let home = std::env::var_os("HOME").map(PathBuf::from).ok_or("HOME is not set")?;
        let mut removed = Vec::new();
        for domain in [Domain::System, Domain::Gui(uid())] {
            if launchctl_print(&domain).is_none() && !domain.plist_path(&home).exists() {
                continue;
            }
            let plist = domain.plist_path(&home).display().to_string();
            match domain {
                Domain::System => {
                    let _ = Command::new("sudo").args(["launchctl", "bootout", &domain.target()]).output();
                    run(Command::new("sudo").args(["rm", "-f", &plist]), "sudo rm (plist)")?;
                }
                Domain::Gui(_) => {
                    let _ = Command::new("launchctl").args(["bootout", &domain.target()]).output();
                    let _ = std::fs::remove_file(&plist);
                }
            }
            removed.push(domain);
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (a1bd8b58): the stage step must write where launchd will exec —
    // the installer's wrapper form today, the bare-binary form after audit item 7. A
    // wrong parse stages a binary launchd never runs and the deploy reports success on
    // the old one (#194's shape, one layer down).
    #[test]
    fn the_slot_is_read_from_either_plist_form_and_never_guessed() {
        let wrapper = r#"<key>ProgramArguments</key>
  <array><string>/bin/bash</string><string>-lc</string><string>export PATH="/x:$PATH"; if [ -f "/Users/j/.continuum/config.env" ]; then set -a; . "/Users/j/.continuum/config.env"; set +a; fi; exec "/Users/j/.continuum/bin/continuum-core-server" "/tmp/continuum-core.sock"</string></array>"#;
        assert_eq!(slot_from_plist(wrapper), Some(PathBuf::from("/Users/j/.continuum/bin/continuum-core-server")));
        let direct = r#"<key>ProgramArguments</key>
  <array><string>/Users/j/.continuum/bin/continuum-core-server</string><string>/tmp/continuum-core.sock</string></array>"#;
        assert_eq!(slot_from_plist(direct), Some(PathBuf::from("/Users/j/.continuum/bin/continuum-core-server")));
        assert_eq!(slot_from_plist("<key>Label</key><string>x</string>"), None, "no ProgramArguments = no slot, never a default");
    }

    // what this catches: the receipt's one number. `launchctl print` carries the pid on
    // its own line; a job that is registered but not running has no such line and must
    // read as None, not 0.
    #[test]
    fn the_job_pid_is_read_from_launchctl_print_or_is_none() {
        let running = "com.continuum.core = {\n\tactive count = 1\n\tpath = /x.plist\n\tstate = running\n\n\tpid = 93518\n\tprogram = /bin/bash\n}";
        assert_eq!(pid_from_launchctl_print(running), Some(93518));
        let idle = "com.continuum.core = {\n\tstate = not running\n\tlast exit code = (never exited)\n}";
        assert_eq!(pid_from_launchctl_print(idle), None);
    }

    // what this catches (2026-09-19 13:32Z, IntelMac): the four states a Mac can be in,
    // and that the one launchd's log named — a gui domain in on-demand-only mode — reads as
    // OWNED-BUT-CANNOT-HEAL, never as SUPERVISED, while the same fact never downgrades a
    // system LaunchDaemon. And the daemon wins when both are registered.
    #[test]
    fn the_verdict_names_the_four_states_and_the_daemon_outranks_the_agent() {
        use SupervisionVerdict::*;
        assert_eq!(supervision_verdict(None, None, Some(63839), false), Unsupervised);
        let gui = Some(Domain::Gui(501));
        assert_eq!(
            supervision_verdict(gui.clone(), Some(93518), Some(93518), true),
            OwnedButCannotHeal { domain: Domain::Gui(501) },
            "the IntelMac case: launchd owns it and cannot spawn it"
        );
        assert_eq!(supervision_verdict(gui.clone(), Some(93518), Some(93518), false), Supervised { domain: Domain::Gui(501), pid: 93518 });
        assert_eq!(
            supervision_verdict(gui, None, Some(63839), false),
            JobPresentCoreOrphaned { job_pid: None, core_pid: Some(63839) },
            "an agent installed beside an orphan core is not supervision"
        );
        assert_eq!(
            supervision_verdict(Some(Domain::System), Some(7), Some(7), true),
            Supervised { domain: Domain::System, pid: 7 },
            "on-demand-only is a gui-domain fact; the daemon is not subject to it"
        );
        assert_eq!(choose_domain(true, true, 501), Some(Domain::System));
        assert_eq!(choose_domain(false, true, 501), Some(Domain::Gui(501)));
        assert_eq!(choose_domain(false, false, 501), None);
        assert!(domain_is_on_demand_only("launchd[1] [gui/501 [100002]:] pending spawn, domain in on-demand-only mode: com.continuum.core"));
        assert!(!Unsupervised.is_healthy() && Supervised { domain: Domain::System, pid: 1 }.is_healthy());
    }

    // what this catches (audit #4223 item 7): the plist `continuum install` writes runs the
    // BINARY, not a bash wrapper — and the slot it names is the slot `stage` will read back
    // through `slot_from_plist`, so installer and deploy cannot drift. The daemon form
    // carries the operator (never root), the agent form does not; the environment rides in
    // launchd's own dict, XML-escaped, so a `&` in a value cannot break the plist.
    #[test]
    fn the_installed_plist_runs_the_binary_and_round_trips_its_slot() {
        let slot = PathBuf::from("/Users/j/.continuum/bin/continuum-core-server");
        let env = vec![("PATH".to_string(), "/a:/b".to_string()), ("ORT_DYLIB_PATH".to_string(), "/l/x&y.dylib".to_string())];
        let spec = |domain: &Domain| {
            render_plist(&PlistSpec { domain, slot: &slot, socket: "/tmp/continuum-core.sock", home: Path::new("/Users/j"), user: "j", env: &env })
        };
        let daemon = spec(&Domain::System);
        assert_eq!(slot_from_plist(&daemon), Some(slot.clone()), "the stage step reads back the slot the installer wrote");
        assert!(!daemon.contains("/bin/bash") && !daemon.contains("-lc"), "the binary is the command");
        assert!(daemon.contains("<key>UserName</key><string>j</string>"), "a LaunchDaemon runs as the operator");
        assert!(daemon.contains("<key>ORT_DYLIB_PATH</key><string>/l/x&amp;y.dylib</string>"), "escaped, in launchd's dict");
        assert!(daemon.contains("<key>Crashed</key><true/>") && daemon.contains("<key>RunAtLoad</key><true/>"));
        assert!(
            daemon.contains("<key>AbandonProcessGroup</key><true/>"),
            "a kickstart or a heal must not SIGKILL the serving lanes in the core's process group (M5: a cold 27B load per deploy)"
        );
        let agent = spec(&Domain::Gui(501));
        assert!(!agent.contains("UserName"), "an agent already runs as its user");
        assert_eq!(slot_from_plist(&agent), Some(slot));
    }
}
