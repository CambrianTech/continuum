use continuum_process::{command_name, kill9};

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
    let out = airc_core::process::background("netstat")
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
/// Known blind spot (Cormac, #4252): a lane still LOADING its weights answers `/health`
/// 503 (`non-2xx`) and can sit near 0% CPU in I/O wait — it reads Dead here exactly as
/// it did under the single-probe check (no regression), but its evidence line will say
/// "frozen" for a process that was merely mmap-reading; a boot landing seconds after a
/// relaunch is the case. If that ever shows in a receipt, the third signal is the
/// process's RSS / I/O advancing, not CPU — do not chase it as a wedge.
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
pub(super) fn probe_health_once(port: u16, bound_s: u64) -> Result<(), String> {
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

/// The KV cache decision a running llama-server was LAUNCHED with, read off its argv —
/// the process's truth about the geometry the plan cannot change without a relaunch.
/// `cache_type` is `--cache-type-k <t>` (or `=`); absent = the engine's default (f16).
/// `flash_attn` is `--flash-attn on` (a bare `--flash-attn` or `on`/`auto`; `off` is off).
/// Card 977842fd: the KV cache type is applied at SPAWN, an engine now survives a deploy
/// (#4284), and the adopt rail compared model + window + lanes + sight — so a new
/// decision (q8_0 on the 5090) never reached the f16 engine the new core adopted.
/// `cache_ram_mib` is its `--cache-ram` grant ([`cache_ram_mib_in`]), the host-RAM store of
/// saved KV states: fixed at spawn like the type, so the adopt rail must compare it too
/// (card d7294f0b).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchKv {
    pub cache_type: Option<String>,
    pub flash_attn: bool,
    pub cache_ram_mib: Option<u32>,
}

impl LaunchKv {
    /// f16 is the engine's default, so "no flag" and "f16" are the same launch.
    pub fn cache_type_or_default(&self) -> &str {
        self.cache_type.as_deref().unwrap_or(crate::inference::kv_cache_plan::F16) // unwrap_or: no flag = the engine's f16 default, a value not an absence
    }
}

/// PURE: the KV decision in a llama-server argv (see [`LaunchKv`]).
pub fn launch_kv_in(argv: &[String]) -> LaunchKv {
    let mut cache_type = None;
    let mut flash_attn = false;
    let mut it = argv.iter().peekable();
    while let Some(a) = it.next() {
        if a == "--cache-type-k" || a == "-ctk" {
            cache_type = it.next().cloned();
        } else if let Some(v) = a.strip_prefix("--cache-type-k=") {
            cache_type = Some(v.to_string());
        } else if a == "--flash-attn" || a == "-fa" {
            flash_attn = match it.peek().map(|s| s.as_str()) {
                Some("off") => {
                    it.next();
                    false
                }
                Some("on") | Some("auto") => {
                    it.next();
                    true
                }
                _ => true,
            };
        } else if let Some(v) = a.strip_prefix("--flash-attn=") {
            flash_attn = v != "off";
        }
    }
    LaunchKv { cache_type, flash_attn, cache_ram_mib: cache_ram_mib_in(argv) }
}
