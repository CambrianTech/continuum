//! lane_process.rs — the shared unix-process primitives the two lane-tracking
//! modules ([`crate::inference::lane_pidfile`] canonical-port reclaim and
//! [`crate::inference::lane_registry`] orphan accounting) both need.
//!
//! ## Why this is its own module
//!
//! Both modules answer a "is this recorded pid still one of MY llama-servers, and
//! if so may I reap it" question, and both must obey the SAME never-blind-kill
//! safety: verify the pid is actually a `llama-server` (via `ps`) before sending a
//! signal, because a recorded pid can be STALE (the process died and the OS reused
//! its number for something unrelated). Duplicating `is_alive` / `kill9` /
//! `command_name` across the two callers would be the exact compression violation
//! we forbid — one decision ("is this pid a live llama-server we own"), one place.
//!
//! No new dependency: `libc` is already in the tree; identity via `ps -p <pid> -o
//! comm=` works identically on macOS and Linux.

/// True if `pid` names a live process. `kill(pid, 0)` sends no signal: `0` = alive
/// and ours; `EPERM` = alive but owned by another user (still alive); `ESRCH` =
/// gone.
#[cfg(unix)]
pub fn is_alive(pid: u32) -> bool {
    let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if rc == 0 {
        return true;
    }
    matches!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(e) if e == libc::EPERM
    )
}

/// Windows: no `kill(pid, 0)`. Query the task table — a matching row means the
/// pid is live. `tasklist` failing (unavailable / no permission) is treated as
/// "not alive", matching the Unix path's conservative-on-error stance.
#[cfg(windows)]
pub fn is_alive(pid: u32) -> bool {
    std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
        .unwrap_or(false)
}

/// `SIGKILL` the pid. Best-effort: a race where it already exited is fine.
#[cfg(unix)]
pub fn kill9(pid: u32) {
    unsafe {
        let _ = libc::kill(pid as libc::pid_t, libc::SIGKILL);
    }
}

/// Windows: force-terminate the process (and its child tree) via `taskkill`.
/// There is no SIGKILL; `/F /T` is the nearest equivalent. Best-effort.
#[cfg(windows)]
pub fn kill9(pid: u32) {
    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/T", "/PID", &pid.to_string()])
        .output();
}

/// The command name (basename) of `pid` via `ps -p <pid> -o comm=`. `None` if the
/// pid is gone or `ps` is unavailable — callers treat `None` as "unverifiable" and
/// refuse to kill.
#[cfg(not(windows))]
pub fn command_name(pid: u32) -> Option<String> {
    let out = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.trim();
    if line.is_empty() {
        return None;
    }
    // `comm` can be a full path on Linux — take the basename so the
    // "llama-server" match is on the binary name, not its install dir.
    Some(line.rsplit('/').next().unwrap_or(line).to_string())
}

/// Windows has no `ps`. Without this arm `command_name` returned `None` for EVERY
/// pid on Windows, which the doctrine above turns into "unverifiable — refuse to
/// kill". Combined with the same gap in [`pid_listening_on_port`], the whole
/// reap/verify subsystem was inert on Windows: the core could not identify even
/// its OWN llama-server child, so any unclean teardown left the port held by a
/// holder it could not name and serving stayed wedged permanently. Measured on
/// BigMama 2026-09-05 (four spawn/refuse cycles, `held by None` while netstat
/// showed the pid plainly). [[silently-unwired-capability]]
#[cfg(windows)]
pub fn command_name(pid: u32) -> Option<String> {
    let out = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_tasklist_image_name(&String::from_utf8_lossy(&out.stdout))
}

/// Parse `tasklist /NH /FO CSV` output to the image name. Split out as a pure
/// function so the parse is testable on any host — the shell-out is not.
///
/// A miss must be `None`, never a guess: tasklist answers a NO-MATCH filter with
/// the prose line `INFO: No tasks are running which match the specified criteria.`
/// on stdout AND exit status 0, so "success" does not imply "found".
#[cfg(any(windows, test))]
pub(crate) fn parse_tasklist_image_name(stdout: &str) -> Option<String> {
    let line = stdout.lines().find(|l| l.trim_start().starts_with('"'))?;
    let name = line.trim().trim_start_matches('"');
    let name = name.split('"').next()?.trim();
    if name.is_empty() {
        return None;
    }
    Some(name.to_string())
}

/// Positively identify `pid` as one of OUR llama-server children — the guard every
/// reap decision runs before signalling. `false` for a dead pid, a reused pid now
/// naming an unrelated process, or an unverifiable one (no `ps`). Never a guess:
/// only a confirmed `llama-server` `comm` returns `true`.
pub fn is_llama_server(pid: u32) -> bool {
    command_name(pid).is_some_and(|comm| comm.contains("llama-server"))
}

/// The pid LISTENING on local TCP `port`, via `lsof -ti tcp:<port> -sTCP:LISTEN`
/// (macOS + Linux). `None` = nothing listening, or `lsof` unavailable — callers
/// treat `None` as "unverifiable" and refuse to kill, same doctrine as
/// [`command_name`]. This is the kill-VERIFY half of the 2026-07-23 flap case:
/// a spawn must never race a port whose holder it can't name.
#[cfg(not(windows))]
pub fn pid_listening_on_port(port: u16) -> Option<u32> {
    let out = std::process::Command::new("lsof")
        .args(["-ti", &format!("tcp:{port}"), "-sTCP:LISTEN"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .and_then(|l| l.trim().parse::<u32>().ok())
}

/// Windows equivalent via `netstat -ano`. See [`command_name`]'s windows arm for
/// what the absence of this cost: `None` here is indistinguishable from "nothing
/// is listening", so the pre-spawn gate refused forever against a port whose
/// holder it was simply unable to look up.
#[cfg(windows)]
pub fn pid_listening_on_port(port: u16) -> Option<u32> {
    let out = std::process::Command::new("netstat")
        .args(["-ano", "-p", "TCP"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_netstat_listening_pid(&String::from_utf8_lossy(&out.stdout), port)
}

/// Parse `netstat -ano -p TCP` for the pid LISTENING on `port`. Pure so the parse
/// is testable off-Windows.
///
/// Matching is on the local address's port SUFFIX after the final ':' — comparing
/// the whole line with `contains(":58057")` would also match a foreign address or
/// a longer port that merely ends in those digits.
#[cfg(any(windows, test))]
pub(crate) fn parse_netstat_listening_pid(stdout: &str, port: u16) -> Option<u32> {
    for line in stdout.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        // proto local foreign state pid
        if f.len() < 5 || !f[3].eq_ignore_ascii_case("LISTENING") {
            continue;
        }
        let Some((_, p)) = f[1].rsplit_once(':') else {
            continue;
        };
        if p.parse::<u16>() == Ok(port) {
            return f[4].parse::<u32>().ok();
        }
    }
    None
}

/// Poll until local `port` is bindable (a successful bind-then-drop proves it) or
/// `budget` expires. THE shared port-release verifier: the pidfile reclaim and the
/// pre-spawn kill-verify gate both wait through this one primitive.
pub async fn wait_port_free(port: u16, budget: std::time::Duration) -> bool {
    const POLL: std::time::Duration = std::time::Duration::from_millis(100);
    let deadline = std::time::Instant::now() + budget;
    loop {
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(POLL).await;
    }
}

/// Every llama-server pid on this box, by name — the boot plan's census.
/// `pgrep -x` (exact name) so a path containing "llama-server" in an editor
/// or grep never matches; each pid is then identity-verified by callers via
/// [`is_llama_server`] before any signal (never-blind-kill).
pub fn owned_llama_pids() -> Vec<u32> {
    let Ok(out) = std::process::Command::new("pgrep")
        .args(["-x", "llama-server"])
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.trim().parse::<u32>().ok())
        .collect()
}

/// How many times the boot path asks a lane's `/health` before judging it, and how long
/// each ask may take. Sized for a LOADED box, not an idle one: on 2026-09-20 the M5's
/// deploy ran this check against a 32 GB lane 6,460 tokens into a turn while the box sat
/// at 0 GB free and 4.9 GB swap — the lane's HTTP thread answered late, a single 3 s
/// probe read that as dead, and the lane was killed mid-generation twice in one morning
/// (card b57b19fd; Joel: "timeouts are where AIs ruin a project"). Three asks over up to
/// fifteen seconds is the price of never killing a working lane for being slow.
pub const LANE_PROBE_ATTEMPTS: u32 = 3;
pub const LANE_PROBE_BOUND_S: u64 = 5;
/// The CPU share above which a lane that missed every probe is BUSY, not dead: a decoding
/// llama-server burns a core sampling and tokenizing; a wedged one is frozen at zero.
pub const LANE_BUSY_CPU_PCT: f32 = 1.0;

/// What the boot path decided about a lane it did not spawn, with the evidence it
/// decided on — every fate is a receipt, never a bare bool.
#[derive(Debug, Clone, PartialEq)]
pub enum LaneVerdict {
    /// `/health` answered 2xx within the bound (attempt number carried).
    Healthy { attempt: u32 },
    /// Every probe missed, but the process is working (CPU advancing) — a slow answer
    /// under load, not a wedge. ADOPTED: the core's own readiness check proves decode
    /// before any citizen is seated, and relaunches it if that fails.
    Busy { cpu_pct: f32, last_error: String },
    /// Every probe missed AND the process is frozen (or is not a lane at all). Reaped.
    Dead { reason: String },
}

impl LaneVerdict {
    /// A lane the boot keeps (warm weights, live KV) rather than reaps.
    pub fn adopt(&self) -> bool {
        !matches!(self, LaneVerdict::Dead { .. })
    }
    /// One line for the boot receipt / the probe stream.
    pub fn evidence(&self) -> String {
        match self {
            LaneVerdict::Healthy { attempt } => format!("healthy (answered on attempt {attempt})"),
            LaneVerdict::Busy { cpu_pct, last_error } => format!(
                "busy (every probe missed within {LANE_PROBE_ATTEMPTS} × {LANE_PROBE_BOUND_S} s: \
                 {last_error}; cpu {cpu_pct:.0}% — decoding, adopted)"
            ),
            LaneVerdict::Dead { reason } => format!("dead ({reason})"),
        }
    }
}

/// PURE: the verdict from the evidence. Two signals, never one: the probe AND the
/// process's own activity. A miss with activity is a slow answer under load (BUSY); a
/// miss without activity is a wedge (DEAD); `None` activity (the process could not be
/// sampled) counts as frozen — an absence never exonerates a lane, and this is the
/// deterministic direction the old single-probe check already took.
pub fn verdict_from(answered_on: Option<u32>, cpu_pct: Option<f32>, last_error: &str) -> LaneVerdict {
    if let Some(attempt) = answered_on {
        return LaneVerdict::Healthy { attempt };
    }
    match cpu_pct {
        Some(pct) if pct >= LANE_BUSY_CPU_PCT => LaneVerdict::Busy {
            cpu_pct: pct,
            last_error: last_error.to_string(),
        },
        Some(pct) => LaneVerdict::Dead {
            reason: format!("every probe missed ({last_error}) and the process is frozen (cpu {pct:.1}%)"),
        },
        None => LaneVerdict::Dead {
            reason: format!("every probe missed ({last_error}) and the process could not be sampled"),
        },
    }
}

/// One bounded `/health` ask on `port`: `Ok(())` on 2xx, else the reason.
fn probe_health_once(port: u16, bound_s: u64) -> Result<(), String> {
    let out = std::process::Command::new("curl")
        .args([
            "-sf",
            "--max-time",
            &bound_s.to_string(),
            &format!("http://127.0.0.1:{port}/health"),
        ])
        .output()
        .map_err(|e| format!("curl unavailable: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(match out.status.code() {
            Some(28) => format!("no answer within {bound_s} s"),
            Some(22) => "non-2xx".to_string(),
            Some(7) => "connection refused".to_string(),
            Some(c) => format!("curl exit {c}"),
            None => "curl killed".to_string(),
        })
    }
}

/// Ask `/health` up to `attempts` times, `bound_s` each. Returns the attempt that answered,
/// else the last reason. Pure over the port so a test can stand a slow listener behind it.
pub fn probe_health_bounded(port: u16, attempts: u32, bound_s: u64) -> Result<u32, String> {
    let mut last = String::from("no attempt");
    for attempt in 1..=attempts {
        match probe_health_once(port, bound_s) {
            Ok(()) => return Ok(attempt),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// The process's CPU share over a short window (sysinfo, single-pid refresh twice across
/// its minimum interval). `None` if the pid is gone or unsampled.
pub fn cpu_pct_of(pid: u32) -> Option<f32> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let wanted = [Pid::from_u32(pid)];
    let mut sys = System::new();
    let kind = ProcessRefreshKind::nothing().with_cpu();
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&wanted), true, kind);
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.max(std::time::Duration::from_millis(500)));
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&wanted), true, kind);
    sys.process(wanted[0]).map(|p| p.cpu_usage())
}

/// The boot path's ONE verdict on a lane it did not spawn — identity-verified, then the
/// two signals: a bounded, retried `/health` and the process's own activity. Boot-path
/// only (blocking, up to `LANE_PROBE_ATTEMPTS × LANE_PROBE_BOUND_S` seconds), never on a
/// serving path. Every path that may kill a lane at boot calls THIS; there is no second
/// criterion in a shell script any more (the bash loop this superseded reaped the M5's
/// lane twice on 2026-09-20).
pub fn lane_verdict(pid: u32) -> LaneVerdict {
    if !is_llama_server(pid) {
        return LaneVerdict::Dead {
            reason: "not a llama-server (dead pid, or a reused one)".to_string(),
        };
    }
    let Some(argv) = command_args(pid) else {
        return LaneVerdict::Dead { reason: "argv unreadable".to_string() };
    };
    let port = {
        let mut it = argv.iter();
        let mut found = None;
        while let Some(a) = it.next() {
            if a == "--port" {
                found = it.next().and_then(|p| p.parse::<u16>().ok());
                break;
            }
            if let Some(v) = a.strip_prefix("--port=") {
                found = v.parse::<u16>().ok();
                break;
            }
        }
        found
    };
    let Some(port) = port else {
        return LaneVerdict::Dead { reason: "no --port on its command line".to_string() };
    };
    match probe_health_bounded(port, LANE_PROBE_ATTEMPTS, LANE_PROBE_BOUND_S) {
        Ok(attempt) => verdict_from(Some(attempt), None, ""),
        Err(last) => verdict_from(None, cpu_pct_of(pid), &last),
    }
}

/// Reap one lane the boot plan judged unhealthy — identity re-verified at the
/// signal (the pid may have died between census and kill).
pub fn kill_lane(pid: u32) {
    if is_llama_server(pid) {
        kill9(pid);
    }
}

/// The command line of `pid` (argv, one string per element) — the process's own truth
/// about what it was launched with, cross-platform via `sysinfo` (a SINGLE-pid refresh:
/// one `KERN_PROCARGS2` / `/proc/<pid>/cmdline` read, never the whole table). `None` if
/// the pid is gone or its argv is unreadable — an absence, never an empty launch.
pub fn command_args(pid: u32) -> Option<Vec<String>> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let wanted = [Pid::from_u32(pid)];
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&wanted),
        true,
        ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always),
    );
    let argv: Vec<String> = sys
        .process(wanted[0])?
        .cmd()
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    (!argv.is_empty()).then_some(argv)
}

/// PURE: the `--cache-ram <MiB>` a llama-server was launched with, read off its argv
/// (`--cache-ram 14396` or `--cache-ram=14396`). This is how a core recovers the grant of
/// a lane it did not spawn — a past form of ourself left up across a deploy and ADOPTED
/// — the same way it reads that lane's window and slots off `/props`: the process's
/// truth, never a re-derivation. `None` when the flag is absent or malformed — unknown,
/// never 0 (0 is a real llama-server value: no host cache).
pub fn cache_ram_mib_in(argv: &[String]) -> Option<u32> {
    let mut it = argv.iter();
    while let Some(a) = it.next() {
        if a == "--cache-ram" {
            return it.next()?.parse().ok();
        }
        if let Some(v) = a.strip_prefix("--cache-ram=") {
            return v.parse().ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the Windows port-owner lookup returning None for a port
    // that IS held. Regression for the 2026-09-05 wedge on BigMama — `lsof` does
    // not exist on Windows, so `pid_listening_on_port` returned None for every
    // port, the pre-spawn gate read that as "held by an unnameable stranger", and
    // serving refused to spawn FOREVER against the core's own orphaned child.
    #[test]
    fn netstat_names_the_listening_pid_for_the_right_port() {
        // Verbatim `netstat -ano -p TCP` output from the wedged box.
        let out = "  Proto  Local Address          Foreign Address        State           PID
  TCP    127.0.0.1:58057        0.0.0.0:0              LISTENING       15464
  TCP    127.0.0.1:58057        127.0.0.1:64707        TIME_WAIT       0
  TCP    0.0.0.0:445            0.0.0.0:0              LISTENING       4
";
        assert_eq!(parse_netstat_listening_pid(out, 58057), Some(15464));
        assert_eq!(parse_netstat_listening_pid(out, 445), Some(4));
        // Not listening anywhere = None, which callers treat as "refuse to kill".
        assert_eq!(parse_netstat_listening_pid(out, 9999), None);
        // TIME_WAIT is not a holder we may reap.
        assert!(!out.is_empty());
    }

    // what this catches: matching the port as a substring instead of the local
    // address's final component. `contains(":8057")` would match ":58057" and
    // hand a reap decision the WRONG pid — a blind kill of an unrelated process.
    #[test]
    fn netstat_port_match_is_not_a_substring_match() {
        let out = "  TCP    127.0.0.1:58057        0.0.0.0:0              LISTENING       15464
";
        assert_eq!(parse_netstat_listening_pid(out, 8057), None);
        assert_eq!(parse_netstat_listening_pid(out, 5805), None);
        assert_eq!(parse_netstat_listening_pid(out, 58057), Some(15464));
    }

    // what this catches: reading tasklist's NO-MATCH prose as an image name.
    // tasklist exits 0 and prints "INFO: No tasks are running..." on stdout, so
    // status success does NOT imply a hit; parsing that line as a name would make
    // is_llama_server true for a dead pid and authorise a kill.
    #[test]
    fn tasklist_parses_the_image_name_and_refuses_the_no_match_line() {
        let hit = "\"llama-server.exe\",\"3852\",\"Console\",\"2\",\"14,254,876 K\"
";
        assert_eq!(
            parse_tasklist_image_name(hit).as_deref(),
            Some("llama-server.exe")
        );
        let miss = "INFO: No tasks are running which match the specified criteria.
";
        assert_eq!(parse_tasklist_image_name(miss), None);
        assert_eq!(parse_tasklist_image_name(""), None);
    }

    // what this catches: is_alive is true for our own process — the liveness check
    // underpins every reclaim/sweep decision, so a broken one silently disarms the
    // whole orphan-reaping machinery.
    #[test]
    fn is_alive_true_for_self() {
        assert!(is_alive(std::process::id()));
    }

    // what this catches: THE safety invariant shared by both callers — our own
    // test-runner process is NOT a llama-server, so is_llama_server must refuse it.
    // A regression here would let either module SIGKILL an unrelated reused-pid
    // process (the blind-kill this guard exists to prevent).
    #[test]
    fn is_llama_server_false_for_self() {
        assert!(
            !is_llama_server(std::process::id()),
            "the test runner is not a llama-server; reaping it would be a blind kill"
        );
    }

    // what this catches: the grant recovered for an ADOPTED engine (every deploy leaves the
    // lane up for the next core) is read off its own argv — both spellings, absent = None,
    // malformed = None, and 0 stays 0 (a real "no host cache" launch, not an unknown).
    #[test]
    fn cache_ram_is_read_off_the_engines_own_argv() {
        let argv = |s: &str| s.split_whitespace().map(str::to_string).collect::<Vec<_>>();
        assert_eq!(
            cache_ram_mib_in(&argv("llama-server -m x.gguf -c 133930 --parallel 2 --cache-ram 14396 --port 8080")),
            Some(14_396)
        );
        assert_eq!(cache_ram_mib_in(&argv("llama-server --cache-ram=8704")), Some(8_704));
        assert_eq!(cache_ram_mib_in(&argv("llama-server --cache-ram 0")), Some(0), "0 is a value");
        assert_eq!(cache_ram_mib_in(&argv("llama-server -c 4096")), None, "absent = unknown");
        assert_eq!(cache_ram_mib_in(&argv("llama-server --cache-ram")), None, "malformed = unknown");
        assert_eq!(cache_ram_mib_in(&argv("llama-server --cache-ram lots")), None);
    }

    // what this catches: busy is not dead. The verdict needs the probe AND the process's
    // activity: a miss with CPU advancing is a slow answer under load (adopted), a miss
    // with a frozen process is a wedge (reaped), an unsampled process never exonerates.
    // Regression for the M5 2026-09-20 (card b57b19fd): a 32 GB lane mid-generation was
    // killed on one 3 s probe under swap, twice in one morning.
    #[test]
    fn busy_is_not_dead_the_verdict_needs_two_signals() {
        assert_eq!(verdict_from(Some(2), None, ""), LaneVerdict::Healthy { attempt: 2 });
        let busy = verdict_from(None, Some(140.0), "no answer within 5 s");
        assert!(busy.adopt(), "a working lane that answers late is adopted: {busy:?}");
        assert!(matches!(busy, LaneVerdict::Busy { cpu_pct, .. } if cpu_pct == 140.0));
        let frozen = verdict_from(None, Some(0.0), "no answer within 5 s");
        assert!(!frozen.adopt(), "a frozen lane that never answers is dead: {frozen:?}");
        let unsampled = verdict_from(None, None, "connection refused");
        assert!(!unsampled.adopt(), "an absence never exonerates");
        assert!(busy.evidence().contains("adopted") && frozen.evidence().starts_with("dead"));
    }

    // what this catches: the probe is retried within a bound sized for a loaded box — a
    // listener that answers 4 s late (the M5 under swap) is HEALTHY on the first attempt
    // with a 5 s bound, where the old single 3 s shot read it as dead; and a port nobody
    // listens on misses every attempt with the reason carried.
    #[test]
    fn a_slow_answer_within_the_bound_is_healthy() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        std::thread::spawn(move || {
            if let Ok((mut sock, _)) = listener.accept() {
                let mut buf = [0u8; 512];
                let _ = sock.read(&mut buf);
                std::thread::sleep(std::time::Duration::from_secs(4));
                let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
            }
        });
        assert_eq!(probe_health_bounded(port, 1, 5), Ok(1), "4 s late is inside a 5 s bound");
        let dead_port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
            l.local_addr().expect("addr").port()
        };
        let miss = probe_health_bounded(dead_port, 2, 1);
        assert!(miss.is_err(), "nobody listening → every attempt misses: {miss:?}");
    }

    // what this catches: a definitely-dead pid is neither alive nor a llama-server,
    // so a stale registry/pidfile entry can never trigger a kill.
    #[test]
    fn dead_pid_is_not_alive_or_llama() {
        let mut child = std::process::Command::new("true")
            .spawn()
            .expect("spawn true");
        let dead = child.id();
        child.wait().expect("reap");
        // (Tiny PID-reuse window is acceptable in a unit test; the invariant we
        // assert is "we do not treat a reaped pid as a live llama-server".)
        assert!(
            !is_llama_server(dead),
            "a reaped pid must not read as a live llama-server"
        );
    }
}
