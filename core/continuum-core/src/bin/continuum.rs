//! `continuum` — the pure-Rust Continuum CLI: the ONE surface for both lifecycle and
//! commands. Replaces the legacy Node `./jtag` and the bare start scripts.
//!
//! ```text
//! continuum start            # build + run the headless Rust core (detached), wait until ready
//!                     # refuses if a core is running but not answering — `--force` reclaims it
//! continuum reboot           # stop everything, rebuild on a free machine, relaunch
//!                     # refuses while training (mlx_lm) is live — `--force` overrides
//! continuum reboot --prebuilt <path> # validate that core first, then hand off without rebuilding
//!                     # Windows: --service hands off to the installer's prepared task
//! continuum stop             # stop the core; the engine (weights + every mind's KV) stays for the next core to adopt
//! continuum stop --engine    # stop the core and the engine
//! continuum ping             # dispatch a command to the running core
//! continuum ping '{"message":"hi"}'
//! continuum data/list '{"collection":"users"}'
//! ```
//!
//! Lifecycle (`start`/`stop`) wraps the pure-Rust `tools/scripts/start-server.sh`
//! (the implementation detail: cargo-run the core with per-platform GPU features,
//! no Node). Commands dispatch through the SAME uniform [`Connection`] every client
//! uses (CLI/persona/web/mobile) over the core IPC socket via [`CoreIpcTransport`].
//! No tsx, no bundle, no Node anywhere.
//! Ordinary commands, command help, and deploy verification never start a core.
//! Ordinary dispatch/help exit 2 when unavailable; start explicitly with `continuum start`.
//! Deploy verification retains its detailed diagnostic and exit 1 on failure.
//!
//! Env: `CONTINUUM_CORE_SOCKET` (default `/tmp/continuum-core.sock`),
//! `CONTINUUM_START_SCRIPT` (override the start script path).

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use continuum_client::{ClientError, Connection};
use continuum_cli_lifecycle::core_bind_guard::BindDecision;
use continuum_client::core_ipc::CoreIpcTransport;
use continuum_cli_lifecycle::deploy_provenance::{
    cli_replaced_this_run, cli_self_build, cli_staleness_note, deploy_verdict, sha_matches,
    CliSelfBuild,
};
use serde_json::Value;

use continuum_cli_lifecycle::owned_engines;
use continuum_cli_lifecycle::prebuilt_artifact::PublishedCoreError;
use owned_engines::owned_engine_candidate;

#[cfg(windows)]
use continuum_cli_lifecycle::windows_launch;
use continuum_cli_lifecycle::supervisor_install;
use continuum_cli_lifecycle::install_cli;

// The macOS supervisor arm. Every call site is `cfg(target_os = "macos")`, so the
// import carries the same gate rather than an allow — a blanket allow would also hide
// the NEXT unused import in this block.
#[cfg(target_os = "macos")]
use continuum_cli_lifecycle::launchd;

// The teardown DECISION lives in the leaf, where it runs on every machine in seconds.
// What stays here is the Windows half that spends the capability — gated at the
// declaration, because every item inside needs `windows_sys`.
#[cfg(windows)]
#[path = "continuum/elevated_teardown.rs"]
mod elevated_teardown;
#[cfg(windows)]
#[path = "continuum/container_maintenance.rs"]
mod container_maintenance;
#[cfg(windows)]
#[path = "continuum/maintenance_serving.rs"]
mod maintenance_serving;
#[cfg(windows)]
#[path = "continuum/maintenance.rs"]
mod maintenance;
// Child modules by responsibility; each starts with `use super::*;` and exposes its items
// to siblings as `pub(super)`. A crate-root file resolves `mod x;` beside itself, so each
// declaration names its file under `continuum/`.
#[path = "continuum/checkpoint.rs"]
mod checkpoint;
use checkpoint::*;
#[path = "continuum/ci_core.rs"]
mod ci_core;
use ci_core::*;
#[path = "continuum/core_artifact.rs"]
mod core_artifact;
use core_artifact::*;
#[path = "continuum/core_conn.rs"]
mod core_conn;
use core_conn::*;
#[path = "continuum/core_service.rs"]
mod core_service;
use core_service::*;
#[path = "continuum/deploy_claim.rs"]
mod deploy_claim;
use deploy_claim::*;
#[path = "continuum/deploy_consume.rs"]
mod deploy_consume;
use deploy_consume::*;
#[path = "continuum/deploy_ledger.rs"]
mod deploy_ledger;
use deploy_ledger::*;
#[path = "continuum/deploy_verify.rs"]
mod deploy_verify;
use deploy_verify::*;
#[path = "continuum/desktop.rs"]
mod desktop;
use desktop::*;
#[path = "continuum/dispatch.rs"]
mod dispatch;
use dispatch::*;
#[path = "continuum/graceful_stop.rs"]
mod graceful_stop;
use graceful_stop::*;
#[path = "continuum/help.rs"]
mod help;
use help::*;
#[path = "continuum/install.rs"]
mod install;
use install::*;
#[path = "continuum/launch.rs"]
mod launch;
use launch::*;
#[path = "continuum/processes.rs"]
mod processes;
use processes::*;
#[path = "continuum/handoff_admission.rs"]
mod handoff_admission;
use handoff_admission::*;
#[path = "continuum/reboot.rs"]
mod reboot;
use reboot::*;
#[path = "continuum/start.rs"]
mod start;
use start::*;
#[path = "continuum/stop.rs"]
mod stop;
use stop::*;
#[path = "continuum/dark_build.rs"]
mod dark_build;
use dark_build::*;
#[cfg(test)]
#[path = "continuum/tests.rs"]
mod tests;
// The service host's one drop of privilege (S4U ignores RunLevel); Windows-only like its caller.
#[cfg(windows)]
use continuum_cli_lifecycle::unelevated_service;
use continuum_cli_lifecycle::elevated_teardown::StopOptions;

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error("no core answering on {socket}; `{command}` requires a running core. Use `continuum start` explicitly, or wait for the current deploy to finish.")]
    NoCore { socket: String, command: String },
    #[error("{0}")]
    Command(String),
}

impl From<String> for CliError {
    fn from(message: String) -> Self {
        Self::Command(message)
    }
}

impl CliError {
    fn exit_code(&self) -> i32 {
        match self {
            Self::NoCore { .. } => 2,
            Self::Command(_) => 1,
        }
    }
}

/// Where `continuum start` records the detached core's PID so `continuum stop` can find it.
fn pidfile_for(socket: &str) -> String {
    format!("{socket}.pid")
}
fn start_logfile() -> String {
    continuum_core::ipc::endpoint_paths::core_start_logfile()
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("continuum: {e}");
        std::process::exit(e.exit_code());
    }
}

fn local_help_requested(command: &str, args: &[String]) -> bool {
    matches!(command, "-h" | "--help" | "help")
        || (matches!(
            command,
            "start"
                | "reboot"
                | "restart"
                | "boot"
                | "stop"
                | "desktop"
                | "ui"
                | "orphans"
                | "deploy-verify"
                | "deploy-consume"
                | "install"
                | "verify"
                | "checkpoint"
                | "engine"
                | "service-host"
        ) && args
            .iter()
            .any(|arg| matches!(arg.as_str(), "-h" | "--help")))
}

async fn run() -> Result<(), CliError> {
    let mut args = std::env::args().skip(1);
    let first = args.next().ok_or_else(usage)?;
    let rest: Vec<String> = args.collect();
    // Identify this executable without contacting or starting a core.
    if matches!(first.as_str(), "--version" | "-V" | "version") {
        if !rest.is_empty() {
            return Err(CliError::Command("usage: continuum --version".into()));
        }
        println!(
            "continuum {} (build {}, sha {}, built {})",
            env!("CARGO_PKG_VERSION"),
            env!("CONTINUUM_BUILD_NUMBER"),
            env!("CONTINUUM_BUILD_GIT_SHA"),
            env!("CONTINUUM_BUILD_AT"),
        );
        return Ok(());
    }
    // The SHA this CLI was built from, alone on stdout — the same contract as the core's
    // `--build-sha`, so `install` can pair the slot's CLI with its core (#4382 asked the CLI
    // and no CLI ever answered: every install on an already-converged core failed there).
    if first == "--build-sha" {
        if !rest.is_empty() {
            return Err(CliError::Command("usage: continuum --build-sha".into()));
        }
        println!("{}", env!("CONTINUUM_BUILD_GIT_SHA"));
        return Ok(());
    }
    // Lifecycle verbs bypass remote command dispatch. Handle their help before
    // any checkout registration, process inspection, stop, build, or launch.
    if local_help_requested(&first, &rest) {
        eprintln!("{}", usage());
        return Ok(());
    }
    let args = rest.into_iter();
    // Offline recovery must work before a core can start, and inspection must
    // not mutate the checkout registry as a side effect.
    if first == "checkpoint" {
        return checkpoint(CheckpointCommand::parse(args)?).map_err(CliError::from);
    }
    // Engine slots (card 2c5d0ec0): offline, like checkpoint. The installers call these
    // with the deploy lock held; exit 3 means no slot is idle and the engine build is skipped.
    if first == "engine" {
        use continuum_core::inference::engine_slots::{run_verb, VerbError};
        match run_verb(&args.collect::<Vec<_>>()) {
            Ok(out) => {
                println!("{out}");
                return Ok(());
            }
            Err(VerbError::NoIdleSlot) => {
                eprintln!("{}", VerbError::NoIdleSlot);
                std::process::exit(3);
            }
            Err(VerbError::DeferredFailedAttempt(reason)) => {
                eprintln!("{reason}");
                std::process::exit(4);
            }
            Err(e) => return Err(CliError::Command(e.to_string())),
        }
    }
    if first == "installed-service" {
        let code = installed_service(args.collect()).await?;
        std::process::exit(code);
    }
    // The selected release's installer uses the same capacity authority as its
    // download/extraction path, including when the destination is another volume.
    if first == "artifact-capacity" {
        let values: Vec<String> = args.collect();
        if values.len() == 1 && values[0] == "--protocol" {
            println!("1");
            return Ok(());
        }
        if values.len() != 2 {
            return Err(CliError::Command("artifact-capacity requires <destination> <projected-bytes>".into()));
        }
        let bytes = values[1].parse::<u64>().map_err(|error| CliError::Command(error.to_string()))?;
        if let Err(error) = continuum_core::paths::artifacts::check_capacity(Path::new(&values[0]), bytes).await {
            // boundary: selected-artifact installer capacity protocol, not a diagnostic parser.
            println!("{}", serde_json::to_string(&error).map_err(|error| CliError::Command(error.to_string()))?); // boundary: CLI stdout wire consumed by installer capacity admission
            std::process::exit(4);
        }
        return Ok(());
    }
    if first == "prepare-prebuilt" {
        return prepare_prebuilt(args.collect()).await.map_err(CliError::Command);
    }
    if first == "maintenance" {
        #[cfg(windows)]
        return maintenance::run(args.collect()).await.map_err(CliError::Command);
        #[cfg(not(windows))]
        return Err(CliError::Command("managed maintenance currently requires Windows scheduler custody".into()));
    }
    if first == "container-maintenance" {
        #[cfg(windows)]
        return container_maintenance::run(args.collect()).await.map_err(CliError::Command);
        #[cfg(not(windows))]
        return Err(CliError::Command("legacy container maintenance requires Windows kernel custody".into()));
    }
    if first == "service-host" {
        let code = service_host(args.collect()).await?;
        std::process::exit(code);
    }
    // Every CLI run from inside a repo records that checkout for the core
    // (repo-card staging reads it); the first deploy after #3706 would otherwise
    // start with an empty registry until the next `start`/`reboot`.
    record_repo_checkout();
    let result = match first.as_str() {
        "-h" | "--help" | "help" => {
            eprintln!("{}", usage());
            Ok(())
        }
        "start" => {
            // Collect once: `args.any(..)` consumes the iterator, so reading a
            // second flag off it afterwards would silently always be false.
            let flags: Vec<String> = args.collect();
            // `--from-source` is the EXPLICIT opt-in to compiling before
            // starting. Without it, `start` execs the installed server; a
            // build is never the silent default (see `launch_core`).
            if flags.iter().any(|a| a == "--from-source") {
                // SAFETY: single-threaded CLI startup, before any task spawns.
                unsafe { std::env::set_var("CONTINUUM_FROM_SOURCE", "1") };
            }
            start(flags.iter().any(|a| a == "--force")).await
        }
        "reboot" | "restart" => reboot(RebootOptions::parse(args)?).await.map_err(|error| error.to_string()),
        // The typed boot plan (BOOT-IS-A-TYPED-PLAN.md, slice 1): deterministic
        // runtime bring-up of an ALREADY-BUILT binary — lane adopt-or-reap,
        // transport, core launch + #194 verify, optional Beside rails — one
        // receipt row per step. The dev-time source build stays with
        // `reboot`/the script until slice 2 migrates it.
        "boot" => {
            use continuum_core::boot_plan::Outcome;
            let mut receipt = continuum_core::boot_plan::run_before_phase();
            if !receipt.ok {
                return Err(CliError::Command(
                    "boot plan: a REQUIRED step failed (see rows above)".into(),
                ));
            }
            let t = std::time::Instant::now();
            #[cfg(windows)]
            let out = match start(false).await {
                Ok(()) => match verify_deployed_build(false).await {
                    Ok(()) => Outcome::Ok("installed start path verified".into()),
                    Err(e) => Outcome::Failed(format!("verify: {e}")),
                },
                Err(e) => Outcome::Failed(e),
            };
            #[cfg(not(windows))]
            let out = match launch_core(&[], LaunchSource::Installed).await {
                Ok(pid) => match verify_deployed_build(false).await {
                    Ok(()) => Outcome::Ok(format!("pid {pid}, #194 verified")),
                    Err(e) => Outcome::Failed(format!("verify: {e}")),
                },
                Err(e) => Outcome::Failed(e),
            };
            let failed = matches!(out, Outcome::Failed(_));
            receipt.push("core-launch-verify", t, out);
            if failed {
                return Err(CliError::Command(
                    "boot plan: core launch/verify failed".into(),
                ));
            }
            // Resolve tools/scripts/start-server.sh through the shared layout; installed
            // users have no script and the Beside rails skip with a reason.
            let repo_root = locate_start_script()
                .ok()
                .and_then(|s| continuum_core::boot_plan::repo_root_from_start_script(&s));
            continuum_core::boot_plan::run_beside_phase(&mut receipt, repo_root.as_deref());
            println!("boot complete — {} steps receipted", receipt.steps.len());
            Ok(())
        }
        "stop" => stop(StopOptions::parse(args)?).await,
        // The display-manager door: the core serves the built desktop itself
        // (http::desktop, always-current, browsers attach/detach freely) —
        // this verb just verifies the greeter answers and opens the browser.
        "desktop" | "ui" => {
            let port = desktop_port();
            let url = desktop_url();
            let up = desktop_answering().await;
            if !up {
                eprintln!(
                    "✗ the desktop display manager is not answering on :{port}.\n                       Is the core running? `continuum ping` — and `continuum start` \n                       builds the web client and serves it automatically. \n                       (Probe classes desktop.dm.* in the server log say why it stayed off.)"
                );
                std::process::exit(1);
            }
            println!("🖥  {url}");
            #[cfg(target_os = "macos")]
            {
                let _ = std::process::Command::new("open").arg(&url).status();
            }
            #[cfg(target_os = "linux")]
            {
                let _ = std::process::Command::new("xdg-open").arg(&url).status();
            }
            Ok(())
        }
        // Dry-run of the reap `reboot`/`stop` perform. Answers "what is this
        // install still holding that nothing is using?" WITHOUT killing it —
        // the safe way to inspect a suspected leak, and the way to confirm a
        // live serving lane is correctly NOT classified as an orphan.
        "orphans" => {
            let cores = running_core_pids();
            let orphans = owned_engine_orphans(&cores);
            if cores.is_empty() {
                println!("no core running — every owned engine process below is orphaned");
            } else {
                println!(
                    "live core pid(s): {} — their descendants are in service and excluded",
                    cores
                        .iter()
                        .map(|p| p.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                );
            }
            if orphans.is_empty() {
                println!("no orphaned engine processes");
            } else {
                for (pid, what) in &orphans {
                    println!("ORPHAN {what} (pid {pid}) — would be reaped by `continuum reboot`");
                }
            }
            Ok(())
        }
        // Standalone #194 check: prove the RUNNING core is built from current HEAD,
        // without a full reboot. Prints "✅ deploy verified" or fails loud on mismatch.
        "deploy-verify" | "verify" => verify_deployed_build(false).await,
        "deploy-consume" => deploy_consume(DeployConsumeOptions::parse(args)?).await,
        "supervisor-status" => supervisor_status(args.into_iter().any(|a| a == "--crash-test")).await,
        // ONE idempotent verb converges the machine (Joel, 2026-09-19): each arm reads,
        // changes only what drifted, says so. Windows arms in `supervisor_install` /
        // `install_cli`; the macOS supervisor arm is `launchd` (card a1bd8b58).
        "install" => install(supervisor_install::InstallOptions::parse(args)?).await,
        "uninstall" => uninstall().await,
        // Anything else is a command name. `--help`/`-h` renders the manual in the
        // CLI's paradigm (bash flags), adapted from the SAME schema the AI gets as
        // a tool spec. Otherwise dispatch, params adapted procedurally.
        command => {
            // Meet the operator's dialect: `continuum read_file ...` / `continuum code_read ...`
            // resolve to the canonical `code/read` through the SAME tool_dialect
            // section personas and the socket route use — so the CLI accepts the
            // same vocabulary, and help + param-adaptation below key off the real
            // command name. Idempotent for an already-canonical name.
            let command = continuum_core::ai::tool_dialect::resolve_wire_name(command);
            let rest: Vec<String> = args.collect();
            if rest.iter().any(|a| a == "--help" || a == "-h") {
                return help_for(&command).await;
            } else {
                return dispatch(&command, rest).await;
            }
        }
    };
    result.map_err(CliError::from)
}
