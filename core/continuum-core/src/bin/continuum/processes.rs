//! Process discovery and termination: core/engine pids, build progress and kill plans.

use super::*;

/// Path to the executable image of the ACTUALLY-RUNNING core, resolved from its live pid.
/// DIAGNOSTICS ONLY (error-message context via [`describe_running_core`]): the deploy receipt
/// itself never re-execs this path — after a rebuild the on-disk file at the running pid's
/// path is already the NEW binary while the old image keeps serving, so exec'ing it
/// false-passes a stale deploy (the inverse of the 2026-07-25 release-leftover false alarm).
/// The running core's SHA always comes from the process itself over the socket (`ping`).
pub(super) fn running_core_binary() -> Option<std::path::PathBuf> {
    let pid = running_core_pids().into_iter().next()?;
    #[cfg(target_os = "linux")]
    {
        std::fs::read_link(format!("/proc/{pid}/exe")).ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        // macOS/BSD have no /proc; `ps -o comm=` prints the full executable path.
        let out = std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .output()
            .ok()
            .filter(|o| o.status.success())?;
        let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!path.is_empty()).then(|| std::path::PathBuf::from(path))
    }
}

/// PIDs of live training runs (`mlx_lm` trainers spawned by the MLX adapter) —
/// the reboot guard's evidence.
///
/// Matches the COMMAND LINE, not the executable name, and that distinction is load-bearing:
/// `mlx_lm` is a Python module, so the process is called `python` and the only place the trainer's
/// identity appears is in its arguments. Matching on name here would find nothing and the reboot
/// guard would happily kill live training — the exact outcome its doc warns about (41 jobs
/// submitted, zero outcomes recorded, all orphaned by reboots).
pub(super) fn running_trainer_pids() -> Vec<i32> {
    processes_with_cmdline("mlx_lm")
}

/// PIDs whose command line contains `fragment` (the old `pgrep -f` behaviour, cross-platform).
/// Kept separate from [`processes_named`] deliberately: command-line matching is what finds an
/// interpreted process, but it is also what makes `pgrep -f continuum-core-server` match the shell
/// that is merely LAUNCHING the core. Use the name-based one where precision matters.
pub(super) fn processes_with_cmdline(fragment: &str) -> Vec<i32> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always),
    );
    sys.processes()
        .values()
        .filter(|p| {
            p.cmd()
                .iter()
                .any(|a| a.to_string_lossy().contains(fragment))
        })
        .map(|p| p.pid().as_u32() as i32)
        .collect()
}

/// Is a still-running start script PROGRESSING or STALLED?
///
/// The whole point of `launch_core`'s wait, reduced to the one decision that is not IO. A
/// wall-clock bound cannot answer it: measured 2026-09-05, a full CUDA rebuild took 1,404s on
/// the 5090 and SUCCEEDED, while a cold build on the Intel Mac exceeded the 1,800s ceiling and
/// ALSO succeeded — the core came up roughly forty minutes after the CLI had already returned
/// Err. Any number long enough for the slowest cold build is far too long to catch a real stall
/// on the fastest, so elapsed time carries no verdict on either machine.
///
/// Time-since-last-output does. A script still emitting is working however long it takes; a
/// script silent past `stall_limit` has stopped, whatever the total. Pure so the priority can be
/// pinned without spawning a build.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum WaitVerdict {
    Progressing,
    Stalled,
}

pub(super) fn wait_verdict(silent_for_secs: u64, stall_limit_secs: u64) -> WaitVerdict {
    if silent_for_secs >= stall_limit_secs {
        WaitVerdict::Stalled
    } else {
        WaitVerdict::Progressing
    }
}

/// Cumulative CPU seconds burned by the build's whole process group.
///
/// LOG OUTPUT IS NOT THE ONLY PROGRESS SIGNAL, and treating it as one was a regression I
/// shipped in the commit that introduced `wait_verdict`: cargo compiling a single large crate
/// is SILENT for minutes, so a stall clock reset only by new log lines fires on exactly the
/// cold low-end build the change was written to protect. M5 caught it on canary within the
/// hour — the dd71a114 symptom moved from 30 minutes to 5, which is worse, not better.
///
/// CPU time separates the two states the watchdog actually cares about. A build working
/// quietly BURNS CPU; a hung exec does not — the `llama-server --version` hang that started
/// this whole thread sits in uninterruptible wait at 0.0%. One `ps` on the process GROUP covers
/// the entire tree (cargo, rustc, cc, cmake) without enumerating it, and the start script is
/// already spawned into its own group so `stop` can find it.
///
/// `None` when the group is gone or `ps` is unavailable, so the caller falls back to the log
/// signal rather than reading "cannot tell" as "stalled" — the same refusal-to-guess the socket
/// and VRAM fixes landed on tonight.
///
/// THE READING IS NOT MONOTONIC, and assuming it was is what made this watchdog kill healthy
/// builds (card 6c91036c). `ps -g` sums cputime over the group members alive AT THAT INSTANT, so
/// when a long-running rustc finishes its crate and cargo replaces it, that child's accumulated
/// time LEAVES the sum and the total DROPS. Measured on this Intel Mac 20s apart, mid-build:
/// `0:00.03 0:01.59 43:08.67` (~2589s) then `0:00.03 0:01.64 0:01.40` (~3s) — a 43-minute rustc
/// exited at a crate boundary and the group total fell by three orders of magnitude while the
/// build was making perfectly ordinary progress. See [`cpu_reading_shows_progress`].
#[cfg(unix)]
pub(super) fn build_group_cpu_secs(pgid: u32) -> Option<u64> {
    let out = std::process::Command::new("ps")
        .args(["-o", "cputime=", "-g", &pgid.to_string()])
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut total = 0u64;
    let mut saw_any = false;
    for line in text.lines() {
        total += cputime_to_secs(line);
        if !line.trim().is_empty() {
            saw_any = true;
        }
    }
    saw_any.then_some(total)
}

#[cfg(not(unix))]
pub(super) fn build_group_cpu_secs(_pgid: u32) -> Option<u64> {
    None
}

/// Did this CPU reading show the build group doing work since the last one?
///
/// Card 6c91036c. The obvious test — `reading > last` — is WRONG, because group cputime is not
/// monotonic (see [`build_group_cpu_secs`]). Treating only an increase as progress latches a
/// high-water mark that no later reading can beat: once a 43-minute rustc has been counted and
/// then exits, every subsequent sample is far BELOW the mark, the comparison is false forever,
/// and the stall clock is never reset again. Five minutes later the watchdog kills a healthy
/// build and reports that it "burned no CPU" — the one thing the readings prove it did.
///
/// ANY MOVEMENT is progress; direction never carried information. A genuinely stuck build holds
/// the sum CONSTANT — a hung exec burns nothing and spawns nothing — so `!=` still catches every
/// stall the `>` version caught. It only stops mis-reading normal crate-boundary churn as death.
///
/// WHAT NEITHER COMPARISON CATCHES, and an earlier draft of this comment claimed the opposite
/// (Astra caught it on review): a SPINNING hang. The log arm and the CPU arm reset the SAME
/// `last_progress_at`, so a spin that keeps burning CPU keeps resetting that one clock and the
/// stall check can never fire, no matter how silent the log goes. Log silence is not an
/// independent signal here. The only thing bounding a spin is the outer `MAX_WAIT_SECS` ceiling.
/// That was equally true of `>` — this change neither introduces nor fixes it — but the comment
/// must not claim a guarantee the code does not make, or the next reader will build on it.
///
/// This bites the LOW END HARDEST, which is the opposite of the intent: the high-water mark is
/// set by the longest-running single rustc, so a slow machine compiling one crate for 43 minutes
/// wedges the detector permanently the moment that crate lands, while a fast machine cycling
/// children quickly never accumulates a mark big enough to matter.
pub(super) fn cpu_reading_shows_progress(reading: u64, last: u64) -> bool {
    reading != last
}

/// `ps -o cputime=` renders `[[dd-]hh:]mm:ss`. Sum right-to-left so every shape parses without
/// branching on which one it is; a day field arrives glued as `dd-hh`, so split it off.
pub(super) fn cputime_to_secs(field: &str) -> u64 {
    let mut secs = 0u64;
    for (i, part) in field.trim().rsplit(':').enumerate() {
        // An unreadable DAY field contributes no days rather than a guess. This total only ever
        // RESETS a stall clock, so under-counting costs at worst one extra tick of patience,
        // while over-counting would mask a real stall.
        let (days, value) = match part.split_once('-') {
            Some((d, v)) => (d.trim().parse::<u64>().unwrap_or(0), v), // unreadable day field = 0 days, never a guessed one
            None => (0, part),
        };
        // Same direction, and it is the tested contract
        // (`unparseable_cputime_contributes_nothing_rather_than_guessing`): a field we cannot read
        // must not invent CPU that was never burned, because inventing it would reset the clock on
        // a genuinely hung build and turn this watchdog back into the thing it replaced.
        secs += value.trim().parse::<u64>().unwrap_or(0) * 60u64.pow(i.min(2) as u32); // unreadable field burns no CPU, so it adds none
        secs += days * 86_400;
    }
    secs
}

/// PIDs of every running `continuum-core-server`, via `pgrep` (pure unix, no
/// Node). Empty on no match or if pgrep is unavailable.
pub(super) fn running_core_pids() -> Vec<i32> {
    processes_named("continuum-core-server")
}

/// The socket path in a core server's argv, or `None` when it passed none.
///
/// Delegates to [`extract_boot_mode`] — the SAME function `main.rs` uses — rather than
/// re-deriving the rule. `main.rs` strips the `--mode` tokens and then takes the first
/// remaining positional; anything that re-implements "skip the flags" drifts from it.
///
/// It already had, on its first commit. This was `find(|a| !a.starts_with('-'))`, which
/// silently disagrees with `main.rs` on the SPACE form of the flag:
///
/// ```text
///   continuum-core-server --mode fail-fast /tmp/x.sock
///     heuristic → Some("fail-fast")      ← a boot mode reported as a socket path
///     main.rs   → Some("/tmp/x.sock")
/// ```
///
/// which put a flag's VALUE into the operator's remedy line —
/// `CONTINUUM_CORE_SOCKET=fail-fast` — a confidently-wrong path with a copy-pasteable
/// command that makes things worse. Strictly worse than the message it replaced, and
/// exactly the failure [`bound_elsewhere_hint`] exists to prevent. Root cause worth
/// naming: the rule was encoded from the binary's HELP TEXT
/// (`[--mode=<MODE>] <socket-path>`), and the help text is an incomplete description
/// of the parser — `boot_mode.rs` accepts `--mode VALUE` too and has a test pinning it.
///
/// A core that passed no positional resolved the path from ITS environment at launch,
/// which this process cannot read after the fact, so `None` means "unknown", never "the
/// default".
pub(super) fn socket_from_core_argv(argv: &[String]) -> Option<String> {
    // A malformed `--mode` is not ours to report — the core either never started or is
    // already failing louder than this diagnostic. Unknown, not a guess.
    let (_, positional) = continuum_core::runtime::extract_boot_mode(argv.to_vec()).ok()?;
    positional.get(1).cloned()
}

/// The sockets bound by the cores at `pids`, read from each process's own argv.
///
/// Takes the pids the bind guard already resolved rather than re-scanning for cores, so
/// the hint can never name a process the refusal did not list, nor miss one it did. Those
/// were two independent `System` snapshots taken at different moments, matched by
/// different predicates — [`processes_named`] tests name OR exe, this tested name alone —
/// and on Linux the divergence is not hypothetical: `/proc/pid/stat`'s comm is capped at
/// 15 characters, so `"continuum-core-server"` (21) is truncated and a name-only match
/// never fires.
///
/// Uses the same `sysinfo` command-line refresh as [`processes_with_cmdline`], so it works
/// on every platform rather than shelling out to a Unix-only tool.
pub(super) fn running_core_sockets_for(pids: &[i32]) -> Vec<(i32, String)> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    // `Some(&pids)`, never `All`: reading a command line costs a syscall PER PROCESS
    // (`KERN_PROCARGS2` / `/proc/<pid>/cmdline`), and `describe_running_core` runs on
    // `deploy-verify`'s SUCCESS path — where the string it builds is then discarded.
    // Refreshing the whole table to keep a handful of pids we were already handed is
    // backwards; the same idiom is used elsewhere in this file. It also makes the
    // membership filter unnecessary: the refresh IS the filter.
    let wanted: Vec<Pid> = pids.iter().map(|p| Pid::from(*p as usize)).collect();
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&wanted),
        true,
        ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always),
    );
    sys.processes()
        .values()
        .filter(|p| pids.contains(&(p.pid().as_u32() as i32)))
        .filter_map(|p| {
            let argv: Vec<String> = p
                .cmd()
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect();
            socket_from_core_argv(&argv).map(|s| (p.pid().as_u32() as i32, s))
        })
        .collect()
}

/// The line to append to an "Occupied" refusal when a running core is bound to a socket
/// this CLI is not looking at — the difference between "nothing is answering" (a wedged
/// core) and "you are listening at the wrong door" (a healthy one).
///
/// `None` when nothing is provably bound elsewhere, so the caller's message is unchanged in
/// the ordinary wedged case.
///
/// Why this is worth its own function: the server takes `argv[1]` over the shared resolver,
/// so an explicitly-launched core serves where the client's default never looks — and every
/// client-side diagnostic then reports the CLIENT's path as though it were the core's. That
/// is TRUE and it names the one path that is not the answer. Measured on the Intel Mac
/// 2026-09-04: pid 83712 served `~/.continuum/intelmac-core.sock` for an hour while `ping`
/// said "NONE is answering on /tmp/continuum-core.sock" — four commands and one wrong
/// "the core is wedged" hypothesis to find a core that was healthy the whole time. The
/// argv was readable throughout. `start`'s own doc already names this state ("bound where
/// this CLI cannot reach"); this makes the message say it too.
pub(super) fn bound_elsewhere_hint(bound: &[(i32, String)], socket: &str) -> Option<String> {
    let mut elsewhere: Vec<&(i32, String)> = bound.iter().filter(|(_, s)| s != socket).collect();
    if elsewhere.is_empty() {
        return None;
    }
    elsewhere.sort_by_key(|(pid, _)| *pid);
    let list = elsewhere
        .iter()
        .map(|(pid, s)| format!("pid {pid} → {s}"))
        .collect::<Vec<_>>()
        .join(", ");
    // A core bound to the socket we ASKED for is not reached by any of this — it really is
    // unresponsive. Leading with an unconditional "NOT wedged" would deny that in the mixed
    // fleet, which is the one case where the operator has both problems at once and needs
    // to be told they are different.
    let some_here = bound.iter().any(|(_, s)| s == socket);
    let lead = if some_here {
        "at least one core is bound elsewhere"
    } else {
        "NOT wedged — bound elsewhere"
    };
    // Name the remedy with the path already filled in: the operator's next keystroke.
    let first = &elsewhere[0].1;
    Some(format!(
        "\n  • {lead}: {list}. This CLI is looking at {socket}. \
         Reach it with `CONTINUUM_CORE_SOCKET={first} continuum <command>`, or \
         `continuum stop` and start one on {socket}."
    ))
}

/// PIDs of running processes whose executable name contains `fragment`.
///
/// This was `pgrep -f`, which does not exist on Windows — and the error was swallowed by `.ok()`,
/// so it returned "nothing is running" every single time. `continuum reboot` therefore believed
/// there was no core to stop, never swapped the binary, then watched the OLD core answer ping and
/// would have reported success. Only the #194 deploy-provenance check caught it:
///
///   DEPLOY MISMATCH: the running core is build 4250b4ce8, but the deploy shipped f2ed295da.
///
/// Every "deployed and verified" claim made on a Windows box before this is suspect.
///
/// sysinfo (already a direct dependency; same fix as the RSS/RAM readers) enumerates processes on
/// every platform, so there is one implementation instead of a Unix tool plus an unported gap.
/// Matching on the executable NAME rather than `pgrep -f`'s full command line is also more precise
/// here: it cannot accidentally match the bash process that is merely launching the core.
pub(super) struct CoreProcessEvidence {
    pub(super) core_pids: Vec<i32>,
    pub(super) observed_pids: std::collections::HashSet<i32>,
}

impl CoreProcessEvidence {
    pub(super) fn from_processes<'a>(
        own_pid: i32,
        processes: impl IntoIterator<Item = (i32, &'a std::ffi::OsStr, Option<&'a Path>)>,
    ) -> std::io::Result<Self> {
        let mut evidence = Self {
            core_pids: Vec::new(),
            observed_pids: Default::default(),
        };
        for (pid, name, exe) in processes {
            evidence.observed_pids.insert(pid);
            if process_matches_fragment(name, exe, "continuum-core-server") {
                evidence.core_pids.push(pid);
            } else if exe.is_none() && name == std::ffi::OsStr::new("continuum-core-") {
                // Linux comm is truncated. Missing exe evidence is uncertainty;
                // it must not count as absence or become a broad kill match.
                return Err(std::io::Error::other(format!("cannot identify possible core PID {pid}: truncated name and unavailable executable")));
            }
        }
        if !evidence.observed_pids.contains(&own_pid) {
            return Err(std::io::Error::other(
                "cannot establish core absence: this process is missing from the process table",
            ));
        }
        Ok(evidence)
    }

    pub(super) fn ensure_offline(&self, recorded_pid: Option<i32>) -> std::io::Result<()> {
        if !self.core_pids.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                format!(
                    "checkpoint adoption requires stopped cores; running PID(s): {:?}",
                    self.core_pids
                ),
            ));
        }
        if let Some(pid) = recorded_pid.filter(|pid| self.observed_pids.contains(pid)) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                format!("checkpoint adoption refused: core PID file names a live process ({pid})"),
            ));
        }
        Ok(())
    }
}

pub(super) fn core_process_evidence() -> std::io::Result<CoreProcessEvidence> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
    CoreProcessEvidence::from_processes(
        std::process::id() as i32,
        sys.processes()
            .values()
            .map(|process| (process.pid().as_u32() as i32, process.name(), process.exe())),
    )
}

pub(super) fn process_matches_fragment(name: &std::ffi::OsStr, exe: Option<&Path>, fragment: &str) -> bool {
    name.to_string_lossy().contains(fragment)
        || exe
            .and_then(Path::file_name)
            .is_some_and(|name| name.to_string_lossy().contains(fragment))
}

pub(super) fn processes_named(fragment: &str) -> Vec<i32> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
    sys.processes()
        .values()
        .filter(|p| process_matches_fragment(p.name(), p.exe(), fragment))
        .map(|p| p.pid().as_u32() as i32)
        .collect()
}

/// Reap a process and everything under it. ONE definition of "kill this tree",
/// because the platform split is the kind of detail that rots into a
/// Unix-only arm nobody notices (see [`processes_named`]'s `pgrep` history).
///
/// Unix: signal the process GROUP (negative pid) — the start script's `setsid`
/// makes the core a group leader — then the pid itself. Windows: `taskkill /T`.
pub(super) fn kill_pid_tree(pid: i32) {
    // FAST SHUTDOWN (Joel 2026-09-02: "make it shut down fast too"). TERM is
    // the courtesy; the DEADLINE is the contract: 3 seconds for the process to
    // save-and-exit, then KILL. Durable state is save-on-write by design
    // (rounds, rooms, memories persist as they change), so a slow drain buys
    // nothing a KILL loses — and an unbounded graceful shutdown is where
    // stop's seconds became minutes.
    #[cfg(unix)]
    unsafe {
        libc::kill(-pid, libc::SIGTERM);
        libc::kill(pid, libc::SIGTERM);
        for _ in 0..30 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if libc::kill(pid, 0) != 0 {
                return; // gone — the fast path, usually well under a second
            }
        }
        libc::kill(-pid, libc::SIGKILL);
        libc::kill(pid, libc::SIGKILL);
    }
    #[cfg(windows)]
    {
        let _ = KillStep::Tree(pid).command().output();
    }
}

/// One step of a kill plan. The distinction matters before the OS sees a kill: a
/// TREE kill on a core also terminates its warm gateway, even if a later orphan sweep
/// excludes that lane. Windows: `taskkill /T` vs `/PID`. Unix: the process GROUP vs the
/// pid alone — and that arm is the one that rotted: until 2026-09-20 the Unix executor
/// discarded `keep` (`let _ = keep;`) and sent every root a group kill, so the live
/// llama-server — a plain child in the core's group, no `setsid` of its own — took the
/// SIGTERM at the old core's stop on every reboot, eleven lines before "leaving serving
/// lane(s) up for adoption" printed over it (M5 20:58:22Z pid 61259 RemovedDead 19 s
/// later; the IntelMac's adoptee answered `/v1/models` while exiting and failed the
/// decode probe). Every deploy was a cold prefill for every seated mind (card 59052747).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum KillStep {
    /// This pid alone — its children are visited by the plan, the protected one spared.
    Process(i32),
    /// This pid and everything under it — no protected lane in this branch.
    Tree(i32),
}

impl KillStep {
    #[cfg(any(windows, test))]
    pub(super) fn command(self) -> std::process::Command {
        let mut cmd = std::process::Command::new("taskkill");
        cmd.arg("/F");
        let pid = match self {
            Self::Process(pid) => pid,
            Self::Tree(pid) => {
                cmd.arg("/T");
                pid
            }
        };
        cmd.args(["/PID", &pid.to_string()]);
        cmd
    }

    #[cfg(unix)]
    pub(super) fn execute(self) {
        match self {
            Self::Tree(pid) => kill_pid_tree(pid),
            Self::Process(pid) => kill_pid_alone(pid),
        }
    }
}

/// Unix: TERM this pid ONLY — never its group — with the same 3 s deadline then KILL as
/// [`kill_pid_tree`]. The step a plan takes on an ancestor of a protected lane.
#[cfg(unix)]
pub(super) fn kill_pid_alone(pid: i32) {
    unsafe {
        libc::kill(pid, libc::SIGTERM);
        for _ in 0..30 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if libc::kill(pid, 0) != 0 {
                return;
            }
        }
        libc::kill(pid, libc::SIGKILL);
    }
}

/// Split only the branches containing a verified live lane. All other branches
/// remain tree kills, so new eye/browser children are still owned by the reap.
/// PURE over a parent map, so both platforms execute the SAME plan.
pub(super) fn kill_plan(
    roots: &[i32],
    parents: &std::collections::HashMap<i32, i32>,
    keep: &[i32],
) -> Vec<KillStep> {
    let mut plan = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut pending = Vec::new();
    for &root in roots {
        // An ancestor root already owns this subtree. Break a malformed cycle
        // by choosing its lowest PID; `seen` also bounds the child traversal.
        if roots.iter().any(|&other| {
            other != root
                && descends_from(parents, root, &[other])
                && (!descends_from(parents, other, &[root]) || other < root)
        }) {
            continue;
        }
        pending.push(root);
        while let Some(pid) = pending.pop() {
            if !seen.insert(pid) || descends_from(parents, pid, keep) {
                continue;
            }
            if !keep
                .iter()
                .any(|lane| descends_from(parents, *lane, &[pid]))
            {
                plan.push(KillStep::Tree(pid));
                continue;
            }
            // Stop the ancestor spawning more workers, then reap its unprotected
            // children. Killing its whole tree would cross the keep set.
            plan.push(KillStep::Process(pid));
            pending.extend(
                parents
                    .iter()
                    .filter_map(|(&child, &parent)| (parent == pid).then_some(child)),
            );
        }
    }
    plan
}

/// THE REAPER IS NEVER IN ITS OWN PLAN. The deploy consumer is a CHILD of the core it
/// stops — the actuator spawns `continuum deploy-consume` from the running core (in its
/// own process group, but parentage is parentage) — so on a reboot the core's tree
/// contains the very process walking it. Whether that process survives came down to a
/// race: if the graceful stop had already taken the core out of the process table when
/// sysinfo refreshed, the consumer read as init's and was untouched (IntelMac 05:16Z,
/// staged + kickstarted + verified); if the core was still draining (4 turns in flight),
/// the consumer read as its child, `Tree(core)` reaped it mid-handoff, and the node had
/// no core for 40 minutes because the kickstart it owed was never issued (IntelMac
/// 11:20Z, card 3ef0986c — the launchd `KeepAlive={Crashed}` policy correctly declines
/// to relaunch a clean stop). Adding ourselves to the keep set gives every ancestor of
/// the reaper a `Process` step instead of a `Tree` one — the same protection a live
/// lane gets — so the core still dies and the hand that stops it keeps its grip.
pub(super) fn keep_with_self(keep: &[i32]) -> Vec<i32> {
    let mut with_self = keep.to_vec();
    with_self.push(std::process::id() as i32);
    with_self
}

pub(super) fn process_parents(sys: &sysinfo::System) -> std::collections::HashMap<i32, i32> {
    sys.processes()
        .values()
        .filter_map(|p| {
            p.parent()
                .map(|par| (p.pid().as_u32() as i32, par.as_u32() as i32))
        })
        .collect()
}

pub(super) fn kill_pid_trees_preserving(roots: &[i32], keep: &[i32]) {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing(),
    );
    let parents = process_parents(&sys);
    for step in kill_plan(roots, &parents, &keep_with_self(keep)) {
        #[cfg(windows)]
        {
            let _ = step.command().output();
        }
        #[cfg(unix)]
        step.execute();
    }
}

/// Engine processes THIS installation owns that are no longer under any live
/// core — the orphans a parent-tree reap can never see.
///
/// Ownership is the executable living under `~/.continuum/bin`, not parentage.
/// That distinction is the whole point: when a core dies without taking its
/// children down, the child is reparented (Windows) or adopted by init (Unix),
/// so `taskkill /T` on the new core's pid will never touch it. It survives
/// every subsequent reboot, holding its port and its VRAM, forever.
///
/// Measured on BIGMAMA 2026-08-05: `llama-server.exe` pid 37148, started
/// 2026-08-04 14:24, parent long dead, still holding 127.0.0.1:8090 — the
/// embedding port. Every persona that resolved a chat model through 8090 got
/// an EMBEDDING model instead, which is the entire degenerate-output mystery.
/// Its command line still carried `D:continuum-cold\...` (no separator after
/// the drive letter), the pre-quoting-fix path corruption — so it was also a
/// live artifact of a bug we had already fixed in the reader but never retired
/// in the processes that bug had spawned.
///
/// `keep` is the set of live core pids whose descendants are legitimately in
/// service; anything owned-but-not-descended is an orphan.
/// Does `pid`'s ancestor chain reach any pid in `keep`?
///
/// Pure over a child→parent snapshot so the traversal — including its
/// termination — is testable without live processes. The snapshot size is the
/// load-bearing part: a pid table can present a CYCLE (pid reuse during a
/// racing scan, or a reparent to a descendant), and an unbounded walk would
/// hang `reboot` forever. Bounded, an unresolvable chain answers "not
/// descended", which is the safe direction only because the caller pairs it
/// with an ownership test — we never kill something we do not own.
pub(super) fn descends_from(parents: &std::collections::HashMap<i32, i32>, pid: i32, keep: &[i32]) -> bool {
    let mut current = pid;
    // No acyclic chain can contain more parent edges than the snapshot. Unlike
    // a fixed hop limit, this never mistakes a deep protected lane for absence.
    for _ in 0..=parents.len() {
        if keep.contains(&current) {
            return true;
        }
        match parents.get(&current) {
            // pid 0 / self-parent terminates the chain on both platforms.
            Some(&parent) if parent != current && parent != 0 => current = parent,
            _ => return false,
        }
    }
    false
}

pub(super) fn owned_engine_orphans(keep: &[i32]) -> Vec<(i32, String)> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    let owned_root = match continuum_core::paths::continuum_home()
        .and_then(|home| continuum_core::paths::payload_root(&home)) {
        Ok(root) => root.join("bin"),
        Err(error) => {
            eprintln!("cannot establish managed engine ownership: {error}");
            return Vec::new();
        }
    };
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::everything(),
    );

    // Snapshot child -> parent once, then decide with pure logic. Reading the
    // live table inside the walk would let a process exiting mid-scan change
    // the answer halfway through.
    let parents = process_parents(&sys);
    let caller = std::process::id();

    sys.processes()
        .values()
        .filter(|p| {
            p.exe()
                .map(|exe| owned_engine_candidate(exe, &owned_root, p.pid().as_u32(), caller))
                .unwrap_or(false)
        })
        .filter(|p| !descends_from(&parents, p.pid().as_u32() as i32, keep))
        .map(|p| {
            let pid = p.pid().as_u32() as i32;
            let what = p
                .exe()
                .and_then(|e| e.file_name().map(|n| n.to_string_lossy().to_string()))
                .unwrap_or_else(|| p.name().to_string_lossy().to_string());
            (pid, what)
        })
        .collect()
}

/// True if `pid` is still alive. Unix: signal 0 is the canonical liveness probe.
/// Windows has no signals — query the task list for the pid.
pub(super) fn pid_alive(pid: i32) -> bool {
    // ONE enumerator, shared with the core's own read of the claim (`deploy_claim::in_flight`).
    continuum_cli_lifecycle::deploy_claim::owner_alive(pid)
}
