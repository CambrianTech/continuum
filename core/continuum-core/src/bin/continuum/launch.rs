//! `continuum start` and the one launch rail every verb shares: the bind decision
//! (reclaim-or-refuse, never blind-bind), the launch policy (what the CALLER needs, not
//! what is on disk), spawning the core or the start script detached, and the honest wait
//! — progress by log output and group CPU, never a wall clock — until the NEW core answers.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

#[cfg(windows)]
use continuum_cli_lifecycle::windows_launch;
use continuum_cli_lifecycle::core_bind_guard::BindDecision;

use continuum_cli_lifecycle::deploy_provenance::{cli_self_build, CliSelfBuild};
use super::checkout::running_cli_image;
use super::client::{core_is_up, pidfile_for, socket_path, start_logfile};
use super::deploy_claim::deploy_gate;
use super::deploy_verify::verify_deployed_build_against;
use super::prebuilt::PrebuiltCore;
use super::process::{bound_elsewhere_hint, kill_pid_tree, pid_alive, running_core_pids, running_core_sockets_for};
use super::runtime_env::{apply_core_runtime_env, direct_core_command};
#[cfg(windows)]
use super::runtime_env::explicit_prebuilt_command;
use super::service::PreparedCoreService;

/// Gather the two observations [`BindDecision`] is a function of — a real `ping`
/// round-trip and the core process table — and hand them to the shared truth table.
///
/// The observation half lives here because it is platform- and transport-shaped; the
/// DECISION half lives in the lib so it is unit-tested by the `--lib` CI gate.
/// Explicit starts go through this seam; ordinary dispatch only checks reachability:
/// the split brain existed because each launch path had its own ad-hoc guard.
async fn bind_decision() -> BindDecision {
    let ping_ok = core_is_up().await;
    let running: Vec<i32> = running_core_pids()
        .into_iter()
        .filter(|p| pid_alive(*p))
        .collect();
    continuum_cli_lifecycle::core_bind_guard::decide(ping_ok, &running)
}

/// `continuum start` — build + run the headless Rust core (detached), wait until it
/// answers `ping`. Idempotent: a no-op if a core is already up.
///
/// Reclaim-or-refuse, never blind-bind. The old guard was `core_is_up()` alone, so a
/// core that was RUNNING but not answering (wedged, mid-boot, bound where this CLI
/// cannot reach) was invisible and `start` launched a second one on top of it. That is
/// the same missing constraint `stop` got in #2287, on the other side of the lifecycle.
pub(super) async fn start(force: bool) -> Result<(), String> {
    let socket = socket_path();
    let decision = bind_decision().await;
    if matches!(decision, BindDecision::AlreadyServing { .. }) {
        println!("core already running (socket={socket})");
        return Ok(());
    }
    // Validate installed task selection before --force may reclaim anything.
    // A source override remains the explicit opt-out from installed deployment.
    let service = PreparedCoreService::for_start(&socket).await?;
    match decision {
        BindDecision::AlreadyServing { .. } => {
            println!("core already running (socket={socket})");
            return Ok(());
        }
        // Only the path that actually LAUNCHES consults the deploy claim. An already-serving
        // core is a no-op and must stay one — gating a no-op would turn a mid-deploy
        // `continuum start` into a spurious error about a core that is already fine.
        // `--force` is the documented override, consistent with the Occupied arm below.
        BindDecision::Free => {
            if !force {
                deploy_gate("start")?;
            }
        }
        BindDecision::Occupied { pids } => {
            let list = pids
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(",");
            if !force {
                return Err(format!(
                    "{} core process(es) are running (pid(s) {list}) but NONE is answering on \
                     {socket}. Refusing to start a second core: both would hold the same socket \
                     and whichever one the kernel hands a connection to would answer, which is \
                     how a shipped fix comes to look intermittently broken.\n  \
                     • still booting? wait — a healthy core answers shortly, then `continuum start` \
                     is a no-op\n  \
                     • wedged? `continuum stop` reaps every core, then start\n  \
                     • sure it is dead weight? `continuum start --force` reclaims it here{}",
                    pids.len(),
                    bound_elsewhere_hint(&running_core_sockets_for(&pids), &socket)
                        .unwrap_or_default()
                ));
            }
            // Explicit reclaim. `reboot` guards destructive restarts behind live-training
            // and live-benchmark leases; `start --force` deliberately carries no such
            // lease check, so say plainly what is being killed rather than implying a
            // vetted teardown.
            println!(
                "⚠ --force: reclaiming {} unresponsive core(s) (pid(s) {list}) — no training or \
                 benchmark lease is checked on this path; use `continuum reboot` if those matter",
                pids.len()
            );
            for pid in &pids {
                kill_pid_tree(*pid);
            }
            // Hand the reaped pids to launch_core as its death-wait set, so readiness is
            // only reported once the OLD cores are gone and the ping provably came from
            // the NEW one — the same honesty check reboot relies on.
            let secs = launch_installed_core(&pids, service).await?;
            println!("✅ core ready (socket={socket}) after ~{secs}s");
            return Ok(());
        }
    }

    let secs = launch_installed_core(&[], service).await?;
    println!("✅ core ready (socket={socket}) after ~{secs}s");
    Ok(())
}

async fn launch_installed_core(
    old: &[i32],
    service: Option<(PreparedCoreService, PrebuiltCore)>,
) -> Result<u64, String> {
    match service {
        Some((service, candidate)) => {
            let secs = service.launch(old).await?;
            verify_deployed_build_against(false, Some(&candidate)).await?;
            Ok(secs)
        }
        None => launch_core(old, LaunchSource::Installed).await,
    }
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
fn build_group_cpu_secs(pgid: u32) -> Option<u64> {
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
fn build_group_cpu_secs(_pgid: u32) -> Option<u64> {
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

pub(super) fn locate_bash() -> Result<PathBuf, String> {
    // Body moved to `continuum_core::shell_portable` — a private `fn` here could
    // not be reused, so `code/shell` (a persona's HANDS) grew the identical
    // WSL-shim bug one directory away and stayed broken after this was fixed.
    // A portability decision belongs in exactly one place.
    continuum_core::shell_portable::locate_bash()
}

/// What the CALLER needs out of a launch — not what happens to be on disk.
///
/// `start` needs a core RUNNING. `reboot` needs the core to be built FROM THE
/// SOURCE IN THIS CHECKOUT, because that is the whole meaning of the deploy
/// verb. Collapsing the two is what produced a `reboot` that re-ran a
/// month-old artifact under a banner promising a fresh build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LaunchSource<'a> {
    /// Whatever is already installed is fine — the caller wants a live core.
    Installed,
    /// Build first. The caller is deploying source they just edited.
    FromSource,
    /// A caller-selected artifact, validated before reboot's teardown.
    Prebuilt(&'a PrebuiltCore),
    /// An explicitly supplied Windows core uses the installed runtime closure,
    /// never the developer preparation script. The caller retains its file pins.
    #[cfg(any(windows, test))]
    ExplicitPrebuilt(&'a PrebuiltCore, Option<&'a continuum_cli_lifecycle::installed_release::Release>),
}

/// The resolved launch, given the policy and what actually exists on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LaunchPlan {
    /// Run the build-and-start script (compiles, then execs).
    Script,
    /// Exec the installed artifact — what the caller asked for.
    Installed,
    /// Exec the installed artifact even though a source build was wanted, because
    /// this machine has no source tree. Legal, but it must SAY SO: it is a restart,
    /// not a deploy.
    InstalledWithoutRebuild,
    /// Exec the already-validated caller-selected artifact, without source work.
    Prebuilt,
    /// Nothing to run.
    NoLaunchable,
}

/// Pure resolution so the policy is testable without a filesystem, a build, or a
/// process. Every branch below cost a real outage or a false success line at some
/// point; the table in the tests is the record of which.
pub(super) fn plan_launch(
    policy: LaunchSource<'_>,
    env_from_source: bool,
    have_script: bool,
    have_installed: bool,
) -> LaunchPlan {
    #[cfg(any(windows, test))]
    if matches!(policy, LaunchSource::ExplicitPrebuilt(..)) {
        return LaunchPlan::Prebuilt;
    }
    if matches!(policy, LaunchSource::Prebuilt(_)) {
        // The explicit command-line intent wins over inherited source/build env.
        return LaunchPlan::Prebuilt;
    }
    let want_source = env_from_source || policy == LaunchSource::FromSource;
    match (want_source, have_script, have_installed) {
        (true, true, _) => LaunchPlan::Script,
        // An explicit CONTINUUM_FROM_SOURCE is an operator DEMAND to compile: silently
        // running a prebuilt binary instead would answer a different question than the
        // one asked. Fail loud rather than substitute.
        (true, false, _) if env_from_source => LaunchPlan::NoLaunchable,
        // `reboot` on an installed node (no checkout): restarting the artifact is the
        // only meaningful thing reboot can do there, and deploy-verify still proves the
        // running SHA against that artifact.
        (true, false, true) => LaunchPlan::InstalledWithoutRebuild,
        (true, false, false) => LaunchPlan::NoLaunchable,
        (false, _, true) => LaunchPlan::Installed,
        (false, true, false) => LaunchPlan::Script,
        (false, false, false) => LaunchPlan::NoLaunchable,
    }
}

pub(super) async fn launch_core(wait_for_death: &[i32], policy: LaunchSource<'_>) -> Result<u64, String> {
    let socket = socket_path();
    let logfile = start_logfile();
    let log = std::fs::File::create(&logfile)
        .map_err(|e| format!("cannot open start log {logfile}: {e}"))?;
    let log_err = log
        .try_clone()
        .map_err(|e| format!("cannot clone start log handle: {e}"))?;

    // THE INSTALLED BINARY IS THE DEFAULT START PATH — for `start`, not for `reboot`.
    //
    // `start` used to shell unconditionally into tools/scripts/start-server.sh,
    // which runs a full cargo build. That made a "governed" lifecycle verb a
    // wrapper around a bash file that only exists inside a repo checkout, with
    // three consequences (BigMama, 2026-08-13):
    //   - a user holding ONLY the installed binary, with no source tree, could
    //     not start a core at all;
    //   - the CLI printed one line and went silent for the length of a compile,
    //     which reads as HUNG and was called hung three separate times;
    //   - the façade's honesty depended entirely on the script underneath.
    //
    // All true — but `launch_core` is shared with `reboot`, and reboot is THE
    // DEPLOY PATH ("edit → reboot → exercise"). Giving it the installed artifact
    // made the verb structurally unable to ship an edit: it printed "building
    // fresh binary, then swapping" and then exec'd a MONTH-OLD binary. That is
    // why the source policy is now an explicit argument instead of an ambient
    // default — the two callers want opposite things and neither should have to
    // infer the other's intent.
    let env_from_source = std::env::var("CONTINUUM_FROM_SOURCE").is_ok();
    #[cfg(any(windows, test))]
    let explicit_prebuilt = matches!(policy, LaunchSource::ExplicitPrebuilt(..));
    #[cfg(not(any(windows, test)))]
    let explicit_prebuilt = false;
    let (script, server_bin) = if explicit_prebuilt {
        (None, None)
    } else if matches!(policy, LaunchSource::Prebuilt(_)) {
        // Already prepared: an installed override may not redirect this explicit
        // artifact — but the source script, in PREBUILT MODE, is how the artifact
        // gets its launch environment (desktop dist, power assertion, airc daemon,
        // llama-server PATH). Without it the M5 came up dark on 2026-09-16.
        (locate_start_script().ok(), None)
    } else {
        (locate_start_script().ok(), locate_core_server_binary()?)
    };
    let plan = plan_launch(
        policy,
        env_from_source,
        script.is_some(),
        server_bin.is_some(),
    );

    let mut cmd = match plan {
        LaunchPlan::Prebuilt => match policy {
            #[cfg(windows)]
            LaunchSource::ExplicitPrebuilt(candidate, runtime) => {
                eprintln!("▶ starting explicit prebuilt core: {} (build {}, no source builders; log: {logfile})", candidate.path.display(), candidate.build_sha);
                explicit_prebuilt_command(candidate, &socket, runtime, &PreparedCoreService::shell()?)
            }
            #[cfg(all(test, not(windows)))]
            LaunchSource::ExplicitPrebuilt(..) => unreachable!("Windows launch policy is exercised through command fixtures on this platform"),
            LaunchSource::Prebuilt(candidate) => {
            match script.as_ref() {
                // ONE start path (2026-09-16): the verified artifact launches THROUGH
                // start-server.sh in prebuilt mode — every cargo build skipped, every
                // piece of launch environment kept (desktop dist, power assertion, airc
                // daemon, llama-server PATH). The bare direct launch darkened the M5's
                // desktop and dropped its sleep assertion on the first warm-build reboot.
                Some(script) => {
                    eprintln!(
                        "▶ starting verified prebuilt core: {} (build {}, no rebuild) via {} (log: {logfile})",
                        candidate.path.display(),
                        candidate.build_sha,
                        script.display()
                    );
                    let mut c = std::process::Command::new(locate_bash()?);
                    c.arg(script);
                    c.env("CONTINUUM_PREBUILT_CORE", &candidate.path);
                    c
                }
                // No source tree (an installed-only box): the bare launch is all there is.
                None => {
                    eprintln!(
                        "▶ starting verified prebuilt core: {} (build {}, no rebuild, no start script; log: {logfile})",
                        candidate.path.display(),
                        candidate.build_sha
                    );
                    direct_core_command(&candidate.path, &socket)
                }
            }
            }
            _ => unreachable!("only a validated prebuilt source selects this plan"),
        },
        LaunchPlan::Script => {
            let script = script.expect("plan_launch only picks Script when one was found");
            if env_from_source || policy == LaunchSource::FromSource {
                eprintln!(
                    "▶ building from source, then starting via {} (log: {logfile}) — \
                     this compiles and can take minutes",
                    script.display()
                );
            } else {
                eprintln!(
                    "▶ no installed continuum-core-server found; falling back to {} \
                     (log: {logfile}) — this COMPILES FIRST and can take minutes. \
                     Install the binary to start without a source tree.",
                    script.display()
                );
            }
            let mut c = std::process::Command::new(locate_bash()?);
            c.arg(&script);
            c
        }
        LaunchPlan::Installed | LaunchPlan::InstalledWithoutRebuild => {
            let bin = server_bin
                .as_ref()
                .expect("plan_launch only picks Installed when one was found");
            if plan == LaunchPlan::InstalledWithoutRebuild {
                // Say it. A reboot that restarts the same artifact is a legitimate
                // operation on an installed node, but calling it a deploy without
                // saying "no source tree, nothing was rebuilt" is exactly the false
                // deploy receipt #194 exists to prevent.
                eprintln!(
                    "▶ no source tree here — restarting the installed artifact, NOT rebuilding \
                     (deploy provenance is still verified below)"
                );
            }
            // stderr, not stdout: stdout carries the dispatched command's JSON
            // result and has to stay machine-parseable when a command
            // auto-starts the core on its way through.
            eprintln!("▶ starting core: {} (log: {logfile})", bin.display());
            // THE SOCKET PATH IS A POSITIONAL ARGUMENT, and this call site is the
            // only one that ever forgot it. `main.rs` requires argv[1] and exits 1
            // with its usage text when it is missing — so from the moment the
            // direct-exec path landed, every `start`/`reboot` on a machine with an
            // installed binary died in ~2s having printed "Usage:" into the start
            // log. The env var below is set too (and `endpoint_paths::core_socket`
            // now honours it server-side), but argv is the binary's documented
            // contract and is what `ps` shows an operator.
            direct_core_command(bin, &socket)
        }
        LaunchPlan::NoLaunchable => {
            return Err(if env_from_source || policy == LaunchSource::FromSource {
                "a source build was requested but no start script was found — \
                 run from a checkout, or set CONTINUUM_START_SCRIPT"
                    .to_string()
            } else {
                "no continuum-core-server binary and no start script — nothing to launch. \
                 Install the binary (tools/scripts/install-service.sh) or run from a checkout."
                    .to_string()
            });
        }
    };
    // ~/.continuum/config.env reaches the core on EVERY launch path, not just the
    // scripted one.
    //
    // start-server.sh `source`s this file under `set -a`, so a core launched through
    // the script inherits every key. The direct-exec path set none of them — the child
    // simply inherited the calling CLI's environment. Measured 2026-08-14: a core
    // auto-started by a dispatched command was running with the agent session's env
    // (CLAUDECODE=1, CLAUDE_CODE_SESSION_ID=…) and NO `CONTINUUM_PROBE_DIR`, while
    // config.env sets it on line 24 — so the glass box was OFF, silently, on an
    // installed node. `ping`, `serving/status` and `deploy-verify` all read healthy;
    // only `debug/probes/query` said otherwise, and only when asked.
    //
    // Applied BEFORE the explicit `.env()` calls below so per-launch facts (the socket
    // this invocation is binding, the self-build guard) still win over the file, and in
    // file order so duplicate assignments resolve last-wins exactly as `source` would.
    // On the Script path the script re-sources the same file afterwards — same values,
    // so this is idempotent there rather than a second source of truth.
    // …and so do the manifest's RUNTIME LIBRARY DIRS, for the same reason and
    // the same class of bug one layer down.
    //
    // The install manifest declares `runtime_path` per module precisely
    // because some artifacts need their DLLs found at RUN time — cuda's entry
    // is `~/.continuum/cuda-*/Library/bin`. `windows-build-env.sh` applies
    // them, so a core launched through start-server.sh inherits them. The
    // direct-exec path did not, and on a CUDA Windows node that is fatal
    // BEFORE main(): the loader fails, the process dies with
    // STATUS_DLL_NOT_FOUND (0xC0000135), and it produces NO output at all —
    // so the operator sees an empty start log and a core that "just doesn't
    // come up".
    //
    // Measured on the 5090 node 2026-09-04, positive control both ways:
    // launching the freshly built server with the CUDA bin dir absent from
    // PATH exits 0xC0000135 silently; prepending it makes the SAME binary
    // print its version and run. Nothing about the build was wrong.
    //
    // Resolved from `~/.continuum` rather than by reading the manifest file,
    // because a binary-only install has no repo to read — and that layout is
    // not an independent guess: it is the manifest's own `extract`
    // destination, the same contract expressed at the other end.
    apply_core_runtime_env(&mut cmd)?;
    // We ARE the continuum binary — may this deploy rebuild our own image?
    //
    // The guard below used to be unconditional, and that is the whole of #422: a
    // Windows file-locking accommodation charged to every platform, which made the
    // documented deploy path structurally unable to ship a fix living in the CLI.
    // The decision now names the ONE platform it is for; everywhere else the script
    // builds the CLI and installs it with the temp+mv swap it already performs.
    match cli_self_build(std::env::consts::OS) {
        CliSelfBuild::Rebuild => {}
        CliSelfBuild::Skip { reason } => {
            // Say it out loud. A skipped build that looks like a completed one is how
            // stale binaries survive a "successful" deploy — #194, one tier up.
            eprintln!("▶ {reason}");
            cmd.env("CONTINUUM_SKIP_SELF_BUILD", running_cli_image());
        }
    }
    cmd.env("CONTINUUM_CORE_SOCKET", &socket);
    #[cfg(not(windows))]
    cmd.stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err));
    // Detach so the core outlives this CLI invocation. Unix: setsid() in the
    // forked child before exec, off continuum's session/controlling terminal. Windows:
    // a new process group + detached process (no console tie).
    #[cfg(unix)]
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    #[cfg(windows)]
    let spawned = {
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        // NOT DETACHED_PROCESS. That flag gives the child NO console at all, and the start script
        // is bash — a console-subsystem program that needs one. Under DETACHED_PROCESS it died
        // with exit 1 in ~2s having written nothing at all, which is indistinguishable from "the
        // script is broken" and is why the core appeared to be unstartable on Windows.
        // CREATE_NO_WINDOW gives it a console with no visible window, so it runs normally and its
        // redirected stdout/stderr still land in the start log. Survival past this CLI exiting does
        // not need detachment on Windows: a child is not killed when its parent exits, and
        // CREATE_NEW_PROCESS_GROUP already keeps our Ctrl+C from reaching it.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // THE CORE IS NEVER A CHILD OF ITS LAUNCHER'S JOB (card 82af11f5). A core started
        // from an agent session on Windows died within ~45 s all evening on the 5090:
        // the session's job object closes (KILL_ON_JOB_CLOSE) and every child dies with
        // it. CREATE_BREAKAWAY_FROM_JOB lifts the core out of that job when the job
        // permits breakaway; when it does not, CreateProcess fails with access denied and
        // the spawn below retries WITHOUT the flag, naming the outcome in the start
        // receipt — the operator then launches through a Scheduled Task (schtasks), which
        // runs outside any job, until the supervisor slice lands.
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        // Redirecting stdio does not prevent Rust's Windows spawn from also
        // inheriting the launcher's other pipe writers. Those kept PowerShell
        // waiting for EOF after a successful reboot (card 9bc0fc5e). The native
        // boundary allows only its owned null/log handles, on BOTH attempts.
        match windows_launch::spawn_logged(
            &cmd,
            &log,
            &log_err,
            CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW | CREATE_BREAKAWAY_FROM_JOB,
        ) {
            Ok(child) => {
                eprintln!("▶ supervisor=breakaway: the core left this session's job object (survives the session)");
                Ok(child)
            }
            Err(e) if e.raw_os_error() == Some(5) => {
                eprintln!(
                    "▶ supervisor=child (WARNING): this session's job object forbids breakaway \
                     (access denied); the core will DIE when this session's job closes. Launch it \
                     through a Scheduled Task instead: schtasks /create /tn continuum-core \
                     /tr \"<the same start command>\" /sc once /st 00:00 /ru <user> /rl highest /f \
                     && schtasks /run /tn continuum-core"
                );
                windows_launch::spawn_logged(
                    &cmd,
                    &log,
                    &log_err,
                    CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW,
                )
            }
            Err(e) => Err(e),
        }
    };
    #[cfg(not(windows))]
    let spawned = cmd.spawn();
    let mut child = spawned.map_err(|e| match plan {
        LaunchPlan::Script => format!(
            "failed to spawn the source-build start script via bash: {e}. The start script \
             is bash; on Windows that needs bash on PATH (Git Bash). Installing \
             continuum-core-server avoids the script entirely."
        ),
        _ => format!(
            "failed to spawn the core server {}: {e}",
            cmd.get_program().to_string_lossy()
        ),
    })?;

    // Record the PID so `continuum stop` can find the detached process group.
    let pidfile = pidfile_for(&socket);
    let _ = std::fs::write(&pidfile, child.id().to_string());

    // Wait until the core answers ping AND every old core PID has exited. The
    // first build (cargo) can take minutes; poll generously, then fail loud with
    // the log tail rather than hang forever. The death-check is what makes a
    // reboot's success signal honest: the ping must come from the NEW core.
    // A COLD start legitimately takes far longer than the old 300s ceiling: the script builds
    // llama-server (CUDA) and then the core, which is tens of minutes on a first run. 300s was not
    // a safety margin, it was a guaranteed false failure on any clean checkout. Raising it is only
    // safe because a DEAD script is now detected within one 2s tick below, so the long ceiling
    // only ever applies to a build that is genuinely still making progress.
    const TICK_SECS: u64 = 2;
    const MAX_WAIT_SECS: u64 = 30 * 60;
    // A WALL-CLOCK bound on a BUILD is the wrong instrument, and no constant can be the right
    // one: a full rebuild with CUDA measured 1,404s on the 5090 (BigMama) and SUCCEEDED, while a
    // cold build on the Intel Mac exceeded even the 30-minute ceiling below and ALSO succeeded —
    // the core came up ~40 minutes after this function had already returned Err. Any number safe
    // for the slowest cold build is uselessly long for detecting a real stall on the fastest.
    //
    // So bound PROGRESS instead: a script that is still emitting log lines is working, however
    // long it takes; a script that has emitted nothing for `STALL_SECS` has stopped, whatever the
    // elapsed total. The absolute ceiling stays only as a backstop for a script that babbles
    // forever without ever becoming ready.
    const STALL_SECS: u64 = 5 * 60;
    let mut last_progress = String::new();
    let mut last_progress_at = std::time::Instant::now();
    // The script is spawned detached into its own process group, so its pid IS the pgid — the
    // same fact `stop` relies on to reap the tree.
    let build_pgid = child.id();
    // "Cannot read CPU yet" starts the baseline at zero, so the FIRST real reading looks like
    // advancement and resets the clock — the generous direction. Starting high would make a hung
    // build's flat CPU appear to be a drop and never reset, which is the failure that matters;
    // starting low can only cost one extra tick of patience.
    let mut last_cpu_secs = build_group_cpu_secs(build_pgid).unwrap_or(0); // no reading yet = 0 baseline, the generous direction
    for i in 0..(MAX_WAIT_SECS / TICK_SECS) {
        tokio::time::sleep(Duration::from_secs(TICK_SECS)).await;
        let old_still_alive = wait_for_death.iter().any(|p| pid_alive(*p));
        if !old_still_alive && core_is_up().await {
            return Ok((i + 1) * TICK_SECS);
        }
        // Sample EVERY tick (printing stays throttled below): the log line is now load-bearing
        // evidence that the build is alive, not just something nice to show, so it cannot be read
        // once every fifteen ticks and still bound a stall to five minutes.
        let line = tail(&logfile, 1).trim().to_string();
        if !line.is_empty() && line != last_progress {
            last_progress = line.clone();
            last_progress_at = std::time::Instant::now();
            // Show the build advancing. A multi-minute silent wait is indistinguishable from a
            // hang, and guessing which one you are in is how a long build gets killed and
            // hand-worked around.
            if (i + 1) % 15 == 0 {
                eprintln!("  … {line}");
            }
        }
        // A QUIET build is not a stopped one. Cargo compiling one large crate prints nothing for
        // minutes, so the log signal alone condemns exactly the cold low-end build this watchdog
        // exists to protect. CPU burned by the process group is the second signal, and it is the
        // one that distinguishes working-silently from hung: a stuck exec sits at 0.0%.
        if let Some(cpu) = build_group_cpu_secs(build_pgid) {
            if cpu_reading_shows_progress(cpu, last_cpu_secs) {
                last_cpu_secs = cpu;
                last_progress_at = std::time::Instant::now();
            }
        }
        // A DEAD CHILD IS NOT A SILENT ONE — ask waitpid BEFORE consulting any timer (Benchy, via
        // M5, 2026-09-05). This check used to sit BELOW the stall check, so on the one tick where
        // a quiet build finally died, the stall arm won the race: it reported "emitted nothing for
        // 300s (STILL RUNNING)" about a corpse — a lie in the message, and the exit status thrown
        // away in favour of the strictly less informative verdict. A process that has exited has a
        // certain, authored answer and no elapsed time can improve on it, so it is asked first.
        //
        // Not checking for exit AT ALL was the original defect: a script that died in 200ms was
        // indistinguishable from one still doing a multi-minute cargo build, so every startup
        // failure cost the full wait and then reported a log tail. Observed shape: long wait,
        // empty log, no cause — which is how "the core never starts on this box" stayed invisible
        // long enough to make hand-rolled downloads feel like the only option.
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!(
                "the start script exited ({status}) after ~{}s without the core coming up.\n{}",
                (i + 1) * TICK_SECS,
                start_log_report(&logfile)
            ));
        }
        let stalled_for = last_progress_at.elapsed().as_secs();
        if matches!(wait_verdict(stalled_for, STALL_SECS), WaitVerdict::Stalled) {
            return Err(format!(
                "the start script has emitted nothing and burned no CPU for {stalled_for}s \
                 (alive, {}s elapsed) — it is stalled, not slow.\n{}",
                (i + 1) * TICK_SECS,
                start_log_report(&logfile)
            ));
        }
    }
    // The literal here said "300s" while MAX_WAIT_SECS was already 30*60 — the string was left
    // behind when the ceiling was raised, so the message under-reported the budget by 6x. That is
    // not cosmetic: it is the number an operator reasons from. Measured 2026-09-05 — a reboot on
    // the Intel Mac reported "did not become ready within 300s" after waiting THIRTY MINUTES, and
    // a card was filed against the wrong budget by two nodes before anyone read the constant.
    // Interpolated now so the message cannot drift from the bound again.
    Err(format!(
        "core did not become ready within {MAX_WAIT_SECS}s, and the start script is STILL RUNNING \
         and still emitting output — so it is progressing, not hung. This is the absolute ceiling, \
         not a stall: the build may well finish on its own after this command gives up.\n{}",
        start_log_report(&logfile)
    ))
}

/// Render the start log for a failure message, and say so plainly when there is nothing in it.
/// "Last log lines:" followed by an empty string is worse than no diagnostic at all: it reads as
/// "the log had nothing interesting" when the truth is "the script produced no output whatsoever",
/// which is itself the strongest clue available (it never got far enough to print).
fn start_log_report(logfile: &str) -> String {
    let t = tail(logfile, 20);
    if t.trim().is_empty() {
        let exists = Path::new(logfile).exists();
        format!(
            "The start log {logfile} is {} -- the script produced NO output at all, so it failed \
             before reaching its first message. Run it directly to see why:\n  bash {}",
            if exists { "empty" } else { "missing" },
            locate_start_script()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "tools/scripts/start-server.sh".to_string())
        )
    } else {
        format!("Last log lines:\n{t}")
    }
}

/// Find `tools/scripts/start-server.sh`: an explicit `CONTINUUM_START_SCRIPT`
/// override, else walk up from the cwd until the repo's script is found.
/// Find the INSTALLED `continuum-core-server` binary — the thing `start`
/// should be launching. Returns `None` when no built server exists, which is
/// the only case that justifies falling back to a source build.
///
/// Search order is "closest to how this binary was invoked" first, so a
/// developer running out of a target dir gets that server, and an installed
/// user gets the installed one:
///   1. `CONTINUUM_CORE_SERVER` — explicit override, same shape as
///      `CONTINUUM_START_SCRIPT`. Refuses loudly if set and not a file, rather
///      than silently searching on (a wrong override must not look like an
///      absent one).
///   2. next to the running `continuum` executable (how an install lays out).
///   3. `~/.continuum/bin`.
///   4. `target/{release,debug}` walking up from cwd — the dev case.
fn locate_core_server_binary() -> Result<Option<PathBuf>, String> {
    const BIN: &str = if cfg!(windows) {
        "continuum-core-server.exe"
    } else {
        "continuum-core-server"
    };

    if let Ok(explicit) = std::env::var("CONTINUUM_CORE_SERVER") {
        let p = PathBuf::from(&explicit);
        if p.is_file() {
            return Ok(Some(p));
        }
        eprintln!(
            "continuum: CONTINUUM_CORE_SERVER={explicit} is not a file — ignoring the \
             override and searching normally"
        );
    }

    let payload = continuum_core::paths::payload_root(&continuum_core::paths::continuum_home()?)?;
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(BIN);
            if candidate.is_file() {
                return Ok(Some(candidate));
            }
        }
    }

    {
        let candidate = payload.join("bin").join(BIN);
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
    }

    let mut dir = std::env::current_dir().map_err(|error| format!("cwd: {error}"))?;
    loop {
        for profile in ["release", "debug"] {
            let candidate = dir.join("target").join(profile).join(BIN);
            if candidate.is_file() {
                return Ok(Some(candidate));
            }
        }
        if !dir.pop() {
            return Ok(None);
        }
    }
}

pub(super) fn locate_start_script() -> Result<PathBuf, String> {
    if let Ok(explicit) = std::env::var("CONTINUUM_START_SCRIPT") {
        let p = PathBuf::from(&explicit);
        if p.is_file() {
            return Ok(p);
        }
        return Err(format!("CONTINUUM_START_SCRIPT={explicit} is not a file"));
    }
    let rel = Path::new("tools/scripts/start-server.sh");
    let mut dir = std::env::current_dir().map_err(|e| format!("cwd: {e}"))?;
    loop {
        let candidate = dir.join(rel);
        if candidate.is_file() {
            return Ok(candidate);
        }
        if !dir.pop() {
            return Err(
                "could not find tools/scripts/start-server.sh by walking up from the \
                 cwd. Run `continuum start` from inside the repo, or set CONTINUUM_START_SCRIPT."
                    .to_string(),
            );
        }
    }
}

/// Last `n` lines of a file (best-effort; for error context).
fn tail(path: &str, n: usize) -> String {
    let Ok(content) = std::fs::read_to_string(path) else {
        return format!("(could not read {path})");
    };
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}
