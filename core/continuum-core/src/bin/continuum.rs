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
//! continuum stop             # stop the running core
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
use continuum_core::runtime::core_bind_guard::BindDecision;
use continuum_core::runtime::core_ipc_transport::CoreIpcTransport;
use continuum_core::runtime::deploy_provenance::{
    cli_self_build, cli_staleness_note, deploy_verdict, CliSelfBuild,
};
use serde_json::Value;

#[path = "continuum/owned_engines.rs"]
mod owned_engines;
use owned_engines::owned_engine_candidate;

#[cfg(windows)]
#[path = "continuum/windows_launch.rs"]
mod windows_launch;
#[path = "continuum/supervisor_install.rs"]
mod supervisor_install;
#[path = "continuum/install_cli.rs"]
mod install_cli;

#[path = "continuum/launchd.rs"]
mod launchd;

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
                | "service-host"
        ) && args
            .iter()
            .any(|arg| matches!(arg.as_str(), "-h" | "--help")))
}

async fn run() -> Result<(), CliError> {
    let mut args = std::env::args().skip(1);
    let first = args.next().ok_or_else(usage)?;
    let rest: Vec<String> = args.collect();
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
        "reboot" | "restart" => reboot(RebootOptions::parse(args)?).await,
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
            // Repo root (dev tree) = two up from the start script; installed
            // users have no script and the Beside rails skip with a reason.
            let repo_root = locate_start_script()
                .ok()
                .and_then(|s| s.parent().and_then(|p| p.parent()).map(|p| p.to_path_buf()));
            continuum_core::boot_plan::run_beside_phase(&mut receipt, repo_root.as_deref());
            println!("boot complete — {} steps receipted", receipt.steps.len());
            Ok(())
        }
        "stop" => stop().await,
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
            let command = continuum_core::cognition::tool_dialect::resolve_wire_name(command);
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

/// `continuum <command> --help` — the CLI adapter for the command's manual: query the
/// live registry (`commands/list`) for the command's description + params schema,
/// then render it as bash usage. Same single source the AI tool adapter reads;
/// only the rendering differs by paradigm ("the manual matches the paradigm").
async fn help_for(command: &str) -> Result<(), CliError> {
    ensure_core_running(command).await?; // the manual comes from the live registry
    let list = connection()
        .commands()
        .execute_value("commands/list", serde_json::json!({ "filter": command }))
        .await
        .map_err(|e| format!("commands/list: {e}"))?;
    let info = list
        .get("commands")
        .and_then(|c| c.as_array())
        .and_then(|cmds| {
            cmds.iter()
                .find(|c| c.get("name").and_then(|n| n.as_str()) == Some(command))
        })
        .ok_or_else(|| format!("unknown command `{command}` (try: continuum commands/list)"))?;
    println!("{}", render_cli_help(command, info));
    Ok(())
}

/// Render a command's `CommandInfo` (from commands/list) as CLI/bash help. Pure
/// (Value in, String out) so it's unit-testable without a running core.
fn render_cli_help(command: &str, info: &Value) -> String {
    let desc = info
        .get("description")
        .and_then(|d| d.as_str())
        .unwrap_or("");
    let mut out = format!("{command} — {desc}\n\n");
    out.push_str(&format!(
        "Usage: continuum {command} [--flag value ...]   (or a single JSON object)\n"
    ));

    let schema = info.get("paramsSchema");
    let required: Vec<&str> = schema
        .and_then(|s| s.get("required"))
        .and_then(|r| r.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
        .unwrap_or_default();
    match schema
        .and_then(|s| s.get("properties"))
        .and_then(|p| p.as_object())
    {
        Some(props) if !props.is_empty() => {
            out.push_str("\nParams:\n");
            for (name, spec) in props {
                let ty = schema_type_str(spec);
                let pdesc = spec
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("");
                let req = if required.contains(&name.as_str()) {
                    "  (required)"
                } else {
                    ""
                };
                out.push_str(&format!(
                    "  --{:<22} {:<9} {}{}\n",
                    camel_to_kebab(name),
                    ty,
                    pdesc,
                    req
                ));
            }
        }
        _ => out.push_str("\n(no params)\n"),
    }
    out
}

/// Best-effort JSON-Schema type label for a property (handles `"type":"string"`,
/// `"type":["string","null"]` for optionals, a `$ref` to a nested type, or an
/// unschematized field).
fn schema_type_str(spec: &Value) -> String {
    // A nested struct/enum is a `$ref` (e.g. "#/definitions/Foo") — show its name
    // rather than an opaque <value>.
    if let Some(r) = spec.get("$ref").and_then(|v| v.as_str()) {
        return format!("<{}>", r.rsplit('/').next().unwrap_or(r));
    }
    match spec.get("type") {
        Some(Value::String(s)) => format!("<{s}>"),
        Some(Value::Array(a)) => {
            let first = a
                .iter()
                .filter_map(|v| v.as_str())
                .find(|s| *s != "null")
                .unwrap_or("any");
            format!("<{first}>")
        }
        _ => "<value>".to_string(),
    }
}

/// camelCase → kebab-case for display (`roundTripMs` → `round-trip-ms`). continuum's
/// adapter accepts either form, so the displayed flag is also a valid one.
fn camel_to_kebab(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for ch in s.chars() {
        if ch.is_ascii_uppercase() {
            out.push('-');
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn socket_path() -> String {
    continuum_core::ipc::endpoint_paths::core_socket_path()
}

/// Explicit legacy selection, independent of a live core's command registry.
/// Inspect emits a digest-bound plan; adopt consumes that exact plan offline.
#[derive(Debug, PartialEq, Eq)]
enum CheckpointCommand {
    Inspect {
        source: PathBuf,
        persona_id: uuid::Uuid,
        plan: PathBuf,
    },
    Adopt {
        plan: PathBuf,
    },
}

impl CheckpointCommand {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let verb = args.next().ok_or("checkpoint requires inspect or adopt")?;
        let mut source = None;
        let mut persona_id = None;
        let mut plan = None;
        let mut legacy_writers_stopped = false;
        while let Some(flag) = args.next() {
            if verb == "adopt" && flag == "--legacy-writers-stopped" && !legacy_writers_stopped {
                legacy_writers_stopped = true;
                continue;
            }
            let slot = match (verb.as_str(), flag.as_str()) {
                ("inspect", "--source") => &mut source,
                ("inspect", "--persona-id") => &mut persona_id,
                ("inspect" | "adopt", "--plan") => &mut plan,
                _ => {
                    return Err(format!(
                        "unknown or repeated checkpoint {verb} option {flag}"
                    ))
                }
            };
            if slot.is_some() {
                return Err(format!("duplicate checkpoint option {flag}"));
            }
            *slot = Some(
                args.next()
                    .filter(|value| !value.is_empty() && !value.starts_with('-'))
                    .ok_or_else(|| format!("{flag} requires a value"))?,
            );
        }
        let plan = PathBuf::from(plan.ok_or("checkpoint requires --plan <path>")?);
        match verb.as_str() {
            "inspect" => Ok(Self::Inspect {
                source: PathBuf::from(source.ok_or("inspect requires --source <volatile.json>")?),
                persona_id: persona_id.ok_or("inspect requires --persona-id <uuid>")?
                    .parse().map_err(|error| format!("invalid persona UUID: {error}"))?,
                plan,
            }),
            "adopt" if legacy_writers_stopped => Ok(Self::Adopt { plan }),
            "adopt" => Err("adopt requires --legacy-writers-stopped: stop legacy cores and their automatic launchers before applying the inspected plan".into()),
            _ => Err(format!("unknown checkpoint operation {verb}; use inspect or adopt")),
        }
    }
}

fn checkpoint(command: CheckpointCommand) -> Result<(), String> {
    use continuum_core::cognition::persona_workspace::checkpoint_adoption::{
        adopt, inspect, AdoptionPlan,
    };
    use std::io::{Read, Write};

    match command {
        CheckpointCommand::Inspect {
            source,
            persona_id,
            plan,
        } => {
            let selection = inspect(&source, persona_id).map_err(|error| error.to_string())?;
            let plan =
                checkpoint_plan_output(&plan, &selection.source.path, &selection.destination_path)?;
            let bytes = serde_json::to_vec_pretty(&selection).map_err(|error| error.to_string())?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&plan)
                .map_err(|error| format!("create inspection plan {}: {error}", plan.display()))?;
            file.write_all(&bytes)
                .and_then(|()| file.sync_all())
                .map_err(|error| format!("write inspection plan {}: {error}", plan.display()))?;
            println!("{}", String::from_utf8_lossy(&bytes));
            eprintln!(
                "checkpoint plan saved to {}; no checkpoint changed",
                plan.display()
            );
        }
        CheckpointCommand::Adopt { plan } => {
            // context-budget-exempt: bounds offline plan decoding, not model input.
            const MAX_PLAN_BYTES: u64 = 1024 * 1024;
            let mut bytes = Vec::new();
            std::fs::File::open(&plan)
                .and_then(|file| file.take(MAX_PLAN_BYTES + 1).read_to_end(&mut bytes))
                .map_err(|error| format!("read inspection plan {}: {error}", plan.display()))?;
            if bytes.len() as u64 > MAX_PLAN_BYTES {
                return Err("checkpoint plan exceeds the offline decoding limit".into());
            }
            let selection: AdoptionPlan = serde_json::from_slice(&bytes)
                .map_err(|error| format!("invalid checkpoint plan: {error}"))?;
            let receipt =
                adopt(&selection, ensure_checkpoint_offline).map_err(|error| error.to_string())?;
            println!(
                "{}",
                serde_json::to_string_pretty(&receipt).map_err(|error| error.to_string())?
            );
        }
    }
    Ok(())
}

fn ensure_checkpoint_offline() -> std::io::Result<()> {
    // A launcher can have written its PID before the executable-name probe
    // identifies it. Read failure or a malformed PID is uncertainty, not absence.
    let recorded_pid = match std::fs::read_to_string(pidfile_for(&socket_path())) {
        Ok(contents) => {
            let pid = contents
                .trim()
                .parse::<i32>()
                .ok()
                .filter(|pid| *pid > 0)
                .ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid core PID file")
                })?;
            Some(pid)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    core_process_evidence()?.ensure_offline(recorded_pid)
}

/// A plan is metadata, never a checkpoint. In particular an absent destination
/// must not be poisoned with plan JSON by an otherwise successful create_new.
fn checkpoint_plan_output(
    output: &Path,
    source: &Path,
    destination: &Path,
) -> Result<PathBuf, String> {
    let absolute = std::path::absolute(output).map_err(|error| error.to_string())?;
    let parent = absolute
        .parent()
        .ok_or("plan output has no parent")?
        .canonicalize()
        .map_err(|error| format!("plan output directory: {error}"))?;
    let filename = absolute.file_name().ok_or("plan output has no filename")?;
    if ["volatile.json", ".volatile.lock"]
        .iter()
        .any(|reserved| filename.to_string_lossy().eq_ignore_ascii_case(reserved))
    {
        return Err("inspection metadata cannot use a reserved checkpoint filename".into());
    }
    #[cfg(windows)]
    if parent.components().any(|component| {
        matches!(component,
        std::path::Component::Prefix(prefix) if matches!(prefix.kind(),
            std::path::Prefix::UNC(_, _) | std::path::Prefix::VerbatimUNC(_, _)))
    }) {
        // An SMB alias can identify the same Persona directory through a
        // different namespace, which lexical containment cannot establish.
        return Err("write the inspection plan to a local path, not a network share".into());
    }
    let output = parent.join(filename);
    let native_personas = destination
        .parent()
        .and_then(Path::parent)
        .ok_or("checkpoint destination has no Persona store")?;
    let source_personas = source
        .parent()
        .and_then(Path::parent)
        .ok_or("checkpoint source has no Persona store")?;
    if output.starts_with(native_personas) || output.starts_with(source_personas) {
        return Err("write the inspection plan outside managed Persona checkpoint storage".into());
    }
    Ok(output)
}

/// Dispatch through the uniform Connection to an already-running core. Lifecycle is
/// explicit: probing a node during a deploy must never launch its pre-swap image.
/// This applies to every command, including dynamically registered ML commands;
/// there is no read-verb allowlist that can drift from the registry.
async fn dispatch(command: &str, args: Vec<String>) -> Result<(), CliError> {
    ensure_core_running(command).await?;
    let canonical = canonical_param_names(command).await;
    let params = params_from_args(command, &args, &canonical)?;
    let result = match connection().commands().execute_value(command, params).await {
        Ok(result) => result,
        // A REFUSAL WITH DATA SHOWS ITS DATA (card f4d2fa49). A handler that answers
        // `{ success: false, errorKind, nextHistoryOffset, malformed, … }` is refusing
        // with the fields the operator needs next; the sentence alone is not enough.
        // The typed outcome goes to stdout as JSON, the refusal line to stderr as
        // before, and the exit code is still the refusal's.
        Err(e) => {
            if let ClientError::Refused {
                outcome: Some(outcome),
                ..
            } = &e
            {
                println!(
                    "{}",
                    serde_json::to_string_pretty(outcome).unwrap_or_else(|_| outcome.to_string()) // boundary: CLI stdout; a Value always encodes, its Display is the same JSON compact
                );
            }
            return Err(format!("{command}: {e}").into());
        }
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string())
    );
    Ok(())
}

/// Adapt CLI args into command params — the CLI's edge of the uniform
/// param-adaptation principle (meet humans/AIs in the middle at every interface).
/// PROCEDURAL and generic: ONE rule for ALL commands, never a per-command switch.
///
/// Three forms, in precedence:
/// 1. nothing → `{}`.
/// 2. a single positional JSON object/array → used verbatim (the AI / power-user
///    path; same payload a tool call would send).
/// 3. `--key value` / `--flag` pairs → a JSON object, built by one loop:
///    - keys are normalized kebab/snake → camelCase (`--round-trip-ms` →
///      `roundTripMs`), matching the canonical camelCase wire fields;
///    - values are coerced by trying JSON first (`5`→number, `true`→bool,
///      `{...}`→object), falling back to a string — so humans type `--count 5`
///      and the typed command still receives a number;
///    - a bare `--flag` (no following value) is `true`.
///
/// `canonical` is the command's exact param field names (from its `commands/list`
/// schema). A user flag is matched against them separator/case-insensitively, so
/// `--persona_id`, `--persona-id`, `--personaId` all map to the schema's real
/// `persona_id`. This is what makes snake_case Rust-native commands invokable by
/// flag (regression: continuum used to blanket-camelCase every key, turning `--persona_id`
/// into `personaId`, which the server rejected as `missing field persona_id`).
/// A flag NOT in the schema is REFUSED by name — with the command and the flags it
/// does accept — but only when the schema is KNOWN (`canonical` non-empty). With no
/// schema (registry unreachable, or a command that publishes no params) "unknown"
/// is unprovable, so the historical camelCase guess still stands; refusing there
/// would make every command the registry cannot describe uncallable. Measured
/// 2026-09-05: `continuum ping --nonsense-flag` returned a healthy pong, exit 0 —
/// the typo became a junk param the command ignored, and the caller got a
/// successful-looking answer to a question nobody asked (#3724).
///
/// Schema-AWARE coercion/validation (knowing each field's exact type) is the next
/// step on the same `commands/list` schema; this canonicalizes the KEY today.
fn params_from_args(
    command: &str,
    args: &[String],
    canonical: &[String],
) -> Result<Value, String> {
    if args.is_empty() {
        return Ok(Value::Object(Default::default()));
    }
    if args.len() == 1 {
        let t = args[0].trim_start();
        if t.starts_with('{') || t.starts_with('[') {
            return serde_json::from_str(&args[0])
                .map_err(|e| format!("invalid JSON params: {e}\n(got: {})", args[0]));
        }
    }

    // {normalized form → exact schema field}. One generic rule for ALL commands.
    let canon_by_norm: std::collections::HashMap<String, &str> = canonical
        .iter()
        .map(|c| (normalize_key(c), c.as_str()))
        .collect();
    // Universally-common flag synonyms. If the user typed one spelling and the
    // SCHEMA uses the other, resolve to the schema's canonical field — so muscle
    // memory ("--command" for a shell, the name Claude's Bash tool + most CLIs use)
    // is served, not bounced (Joel's rule #328: canonical follows the common
    // standard, aliases resolve). Only consulted when the raw flag is NOT itself a
    // canonical field; data-driven, so it never overrides a command's real param.
    const SYNONYMS: &[(&str, &str)] = &[("command", "cmd")];
    // `None` = a flag the command's KNOWN schema does not have — see the last arm.
    let field = |raw: &str| -> Option<String> {
        let norm = normalize_key(raw);
        if let Some(c) = canon_by_norm.get(&norm) {
            return Some((*c).to_string());
        }
        for (a, b) in SYNONYMS {
            let other = if normalize_key(a) == norm {
                Some(*b)
            } else if normalize_key(b) == norm {
                Some(*a)
            } else {
                None
            };
            if let Some(o) = other {
                if let Some(c) = canon_by_norm.get(&normalize_key(o)) {
                    return Some((*c).to_string());
                }
            }
        }
        // NOT a schema field, and not a synonym of one.
        //
        // When `canonical` is EMPTY we do not know the schema — the registry was
        // unreachable, or this command publishes no params — so "unknown" is
        // unprovable and the historical camelCase guess stands. When it is
        // NON-empty we DO know, and a flag matching nothing is a typo the caller
        // wants told about, not silently coerced into a junk key the command
        // ignores. Same unset-vs-unknown ladder as #3717's room probe: refuse only
        // where the knowledge to refuse actually exists.
        if canon_by_norm.is_empty() {
            return Some(to_camel_case(raw));
        }
        None
    };
    // Name the flag AND what the command actually takes. A refusal that only says
    // "unknown" sends the caller to `--help`; one that lists the fields IS the
    // answer they were about to look up.
    let unknown = |k: &str| -> String {
        let mut known: Vec<&str> = canon_by_norm.values().copied().collect();
        known.sort_unstable();
        format!(
            "unknown flag `--{k}` for `{command}`.\n  accepted: {}\n  (or pass a single JSON \
             object; `continuum {command} --help` shows types and which are required)",
            known
                .iter()
                .map(|f| format!("--{f}"))
                .collect::<Vec<_>>()
                .join(" ")
        )
    };

    let mut map = serde_json::Map::new();
    let mut i = 0;
    while i < args.len() {
        let raw_key = args[i].strip_prefix("--").ok_or_else(|| {
            format!(
                "expected `--key value` or a single JSON object, got `{}`",
                args[i]
            )
        })?;
        // Support BOTH `--key value` and the muscle-memory `--key=value` form.
        // Splitting on the first `=` means `--filter=data/` parses to
        // {filter: "data/"} instead of a junk `{"filter=data/": true}` key.
        if let Some((k, v)) = raw_key.split_once('=') {
            map.insert(field(k).ok_or_else(|| unknown(k))?, coerce(v));
            i += 1;
            continue;
        }
        // A value follows unless the next arg is another flag (or there is none).
        let has_value = args.get(i + 1).is_some_and(|n| !n.starts_with("--"));
        if has_value {
            map.insert(
                field(raw_key).ok_or_else(|| unknown(raw_key))?,
                coerce(&args[i + 1]),
            );
            i += 2;
        } else {
            map.insert(
                field(raw_key).ok_or_else(|| unknown(raw_key))?,
                Value::Bool(true),
            );
            i += 1;
        }
    }
    Ok(Value::Object(map))
}

/// Normalize a flag key for schema matching: lowercase, separators removed. So
/// `persona_id`, `personaId`, `persona-id`, `PERSONA_ID` share one normal form.
fn normalize_key(s: &str) -> String {
    s.chars()
        .filter(|c| *c != '-' && *c != '_')
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Best-effort fetch of a command's canonical param field names from the live
/// registry (`commands/list` filtered to this one command — cheap, not the full
/// catalog). Returns the exact serde field names (`persona_id`, `roomId`, …) so the
/// arg adapter can canonicalize a user flag to the schema's real name regardless of
/// separator/case. Empty on ANY failure: the adapter then uses its generic
/// camelCase normalization, which is the pre-schema behavior — a command that needs
/// no canonicalization (camelCase fields) still works without the registry.
async fn canonical_param_names(command: &str) -> Vec<String> {
    let list = match connection()
        .commands()
        .execute_value("commands/list", serde_json::json!({ "filter": command }))
        .await
    {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    list.get("commands")
        .and_then(|c| c.as_array())
        .and_then(|cmds| {
            cmds.iter()
                .find(|c| c.get("name").and_then(|n| n.as_str()) == Some(command))
        })
        .and_then(|info| info.get("paramsSchema"))
        .and_then(|s| s.get("properties"))
        .and_then(|p| p.as_object())
        .map(|props| props.keys().cloned().collect())
        .unwrap_or_default()
}

/// Coerce a CLI string value: try JSON first (`5`→number, `true`→bool, `{…}`→
/// object), else keep it a string. The one rule for every command, no schema
/// needed (schema-aware coercion is a follow-up once commands/list schemas drive it).
fn coerce(raw: &str) -> Value {
    serde_json::from_str::<Value>(raw).unwrap_or_else(|_| Value::String(raw.to_string()))
}

/// kebab/snake → camelCase (`round-trip-ms`/`round_trip_ms` → `roundTripMs`).
fn to_camel_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper = false;
    for ch in s.chars() {
        if ch == '-' || ch == '_' {
            upper = true;
        } else if upper {
            out.extend(ch.to_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    out
}

fn connection() -> Connection<CoreIpcTransport> {
    Connection::new(CoreIpcTransport::new(socket_path()))
}

/// How long a `ping` may take before we call the core "not answering".
///
/// Measured on a healthy local core: 20ms, five samples, no variance — this is a
/// unix-socket round trip, not a network call. 5s is 250× that, so it cannot fire on
/// a merely-busy core; it exists solely to bound a core that will NEVER answer.
const PING_BUDGET: Duration = Duration::from_secs(5);

/// Is a core already answering on the socket? A real ping round-trip, not just a
/// socket-file existence check (a stale socket file lies) — and BOUNDED, which is
/// load-bearing rather than defensive.
///
/// An unbounded ping does not fail against an unresponsive core, it HANGS: the kernel
/// completes `connect()` into the listen backlog whether or not the process is
/// scheduled, the write succeeds, and the read then waits for a reply that never comes.
/// A timeout counts as NOT answering, which is the safe direction — paired with a
/// visible core process it yields `Occupied`, so the caller refuses and names the pids
/// rather than launching a competitor.
///
/// This bound is what makes the [`BindDecision::Occupied`] arm REACHABLE, not a
/// nicety. Glass-boxed 2026-08-14 on this box, both directions:
///
/// - unbounded: SIGSTOP every core, run `start` → no output, no decision, still hung
///   at 90s. The guard was green in unit tests and dead on the live path.
/// - bounded:   same setup → exits 1 in 8s with the refusal naming both live pids
///   (5s here plus the process-table read), and the cores resume unharmed on SIGCONT.
///
/// The 90s figure came from a binary whose build had failed, so it measures the
/// unbounded path either way — but it is the OLD behaviour, not evidence against the
/// timeout, which the 8s run then confirmed directly.
async fn core_is_up() -> bool {
    // Bound to locals: `connection()` and `.commands()` yield temporaries that the
    // future borrows, so building the future inline drops them at the end of the
    // statement (E0716).
    let conn = connection();
    let cmds = conn.commands();
    let ping = cmds.execute_value("ping", Value::Object(Default::default()));
    matches!(tokio::time::timeout(PING_BUDGET, ping).await, Ok(Ok(_)))
}

/// Check the existing core without invoking any lifecycle path. Even a Free socket
/// can belong to a deploy whose compiler has not launched its server yet (cf63a02e).
async fn ensure_core_running(command: &str) -> Result<(), CliError> {
    if core_is_up().await {
        Ok(())
    } else {
        Err(CliError::NoCore {
            socket: socket_path(),
            command: command.to_owned(),
        })
    }
}

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
    continuum_core::runtime::core_bind_guard::decide(ping_ok, &running)
}

/// `continuum start` — build + run the headless Rust core (detached), wait until it
/// answers `ping`. Idempotent: a no-op if a core is already up.
///
/// Reclaim-or-refuse, never blind-bind. The old guard was `core_is_up()` alone, so a
/// core that was RUNNING but not answering (wedged, mid-boot, bound where this CLI
/// cannot reach) was invisible and `start` launched a second one on top of it. That is
/// the same missing constraint `stop` got in #2287, on the other side of the lifecycle.
async fn start(force: bool) -> Result<(), String> {
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

/// `continuum reboot` — stop + rebuild + relaunch the core.
/// Reboot keeps its existing lease/force contract for both source and prebuilt
/// deployment. An invalid or repeated option must not silently request a swap.
#[derive(Debug, Default, PartialEq, Eq)]
struct RebootOptions {
    force: bool,
    prebuilt: Option<PathBuf>,
    service: bool,
    validate_only: bool,
}

impl RebootOptions {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut options = Self::default();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--force" if !options.force => options.force = true,
                "--service" if !options.service => options.service = true,
                "--validate-only" if !options.validate_only => options.validate_only = true,
                "--prebuilt" if options.prebuilt.is_none() => {
                    let path = args
                        .next()
                        .filter(|p| !p.is_empty() && !p.starts_with('-'))
                        .ok_or("reboot --prebuilt requires a core binary path")?;
                    options.prebuilt = Some(PathBuf::from(path));
                }
                "--force" | "--prebuilt" | "--service" | "--validate-only" => {
                    return Err(format!("duplicate reboot option {arg}"))
                }
                _ => {
                    return Err(format!(
                        "unknown reboot option {arg}; use --force or --prebuilt <path> [--service]"
                    ))
                }
            }
        }
        // `--service` alone (2026-09-19, card 82af11f5): the warm build produces the
        // artifact and the supervisor takes it — the path the deploy consumer runs
        // unattended. Until now `--service` demanded a hand-supplied `--prebuilt`,
        // which is exactly why the 5090 ran whatever a hand last typed.
        if options.service && !(cfg!(windows) || cfg!(target_os = "macos")) {
            return Err("reboot --service is supported only on Windows and macOS".to_string());
        }
        if options.validate_only && (options.prebuilt.is_none() || options.service || options.force)
        {
            return Err(
                "--validate-only requires --prebuilt and cannot combine with --force or --service"
                    .to_string(),
            );
        }
        Ok(options)
    }
}

/// An explicitly selected artifact whose loader and provenance were checked while
/// the previous core was still alive. Launch and verification borrow this SAME
/// path/SHA pair; neither may re-resolve an ambient installed-binary override.
#[derive(Debug, PartialEq, Eq)]
struct PrebuiltCore {
    path: PathBuf,
    build_sha: String,
}

impl PrebuiltCore {
    async fn prepare(path: &Path) -> Result<Self, String> {
        let path = path
            .canonicalize()
            .map_err(|e| format!("prebuilt core {} cannot be resolved: {e}", path.display()))?;
        if !path.is_file() {
            return Err(format!("prebuilt core {} is not a file", path.display()));
        }
        let build_sha = binary_build_sha(&path).await?;
        let cwd = std::env::current_dir()
            .map_err(|e| format!("cannot locate checkout for prebuilt verification: {e}"))?;
        let git_dir = std::env::var_os("GIT_DIR");
        let checkout_sha = prebuilt_checkout_sha(&cwd, git_dir.as_deref()).await?;
        Self::from_report(path, build_sha, checkout_sha.as_deref())
    }

    fn from_report(
        path: PathBuf,
        build_sha: String,
        checkout_sha: Option<&str>,
    ) -> Result<Self, String> {
        // Reuse #194's credible-SHA and short/full-SHA comparison. Without a
        // checkout the artifact anchors its own receipt; unknown/malformed
        // provenance still fails. With a checkout it must also match HEAD.
        deploy_verdict(
            Some(&build_sha),
            checkout_sha.unwrap_or(&build_sha),
            if checkout_sha.is_some() {
                "git HEAD of this checkout"
            } else {
                "selected prebuilt artifact"
            },
            &path.display().to_string(),
        )?;
        Ok(Self { path, build_sha })
    }
}

/// Free memory a warm build needs beside a serving core: rustc's codegen wants ~7 GiB
/// (BigMama, 2026-09-05: test builds killed at 2.59 GiB free beside a 39 GiB server) —
/// twelve leaves the server, the citizens and the build their room.
const WARM_BUILD_MIN_FREE_BYTES: u64 = 12 * 1024 * 1024 * 1024;

/// The scheduler owns this foreground host and its core as one process tree.
/// Runtime DLL/config resolution is the same as every other native CLI launch.
async fn service_host(args: Vec<String>) -> Result<i32, String> {
    #[cfg(not(windows))]
    {
        let _ = args;
        Err("service-host is supported only on Windows".to_string())
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if args.len() != 3 || args.iter().any(|arg| arg.is_empty()) {
            return Err("service-host requires <core-path> <socket> <engine-path>".to_string());
        }
        let mut command = direct_core_command(Path::new(&args[0]), &args[1]);
        apply_core_runtime_env(&mut command);
        if command_env(&command, "LLAMA_SERVER_BIN").is_none_or(|value| value.is_empty()) {
            let engine = Path::new(&args[2]);
            if !engine.is_file() {
                return Err(format!("service-host engine missing: {}", engine.display()));
            }
            command.env("LLAMA_SERVER_BIN", engine);
        }
        command.env("CONTINUUM_CORE_SOCKET", &args[1]);
        command.stdin(Stdio::null()).creation_flags(0x0800_0000);
        let mut command = tokio::process::Command::from(command);
        command.kill_on_drop(true);
        let mut child = command
            .spawn()
            .map_err(|e| format!("service-host cannot launch {}: {e}", args[0]))?;
        let pid = child
            .id()
            .ok_or("service-host core exited before PID registration")?;
        std::fs::write(pidfile_for(&args[1]), pid.to_string())
            .map_err(|e| format!("service-host cannot record core PID: {e}"))?;
        let status = child
            .wait()
            .await
            .map_err(|e| format!("service-host cannot wait for core: {e}"))?;
        Ok(status.code().unwrap_or(1))
    }
}

#[cfg(any(windows, test))]
#[derive(Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct CoreServiceDescription {
    artifact: String,
    socket: String,
    launcher: String,
    cli: String,
    engine: String,
    log_directory: String,
}

#[cfg(any(windows, test))]
#[derive(Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct CoreServiceTask {
    description: String,
    command: String,
    arguments: String,
    enabled: bool,
    state: String,
}

#[cfg(any(windows, test))]
impl CoreServiceTask {
    fn validate(&self, candidate: &PrebuiltCore, socket: &str, shell: &Path) -> Result<(), String> {
        let description: CoreServiceDescription =
            serde_json::from_str(&self.description).map_err(|e| {
                format!("ContinuumCore has no valid installer artifact descriptor: {e}")
            })?;
        let resolve = |path: &str| {
            if !Path::new(path).is_absolute() {
                return Err(format!("ContinuumCore path must be absolute: {path}"));
            }
            std::fs::canonicalize(path)
                .map_err(|e| format!("ContinuumCore path {path} cannot be resolved: {e}"))
        };
        if !self.enabled
            || resolve(&description.artifact)? != candidate.path
            || description.socket != socket
            || resolve(&self.command)? != shell.canonicalize().map_err(|e| e.to_string())?
        {
            return Err("ContinuumCore does not select the requested artifact/socket or is disabled; rerun the installer".to_string());
        }
        // Paired host/launcher files live in the same installed slot, never in
        // the mutable build cache or a different release's directory.
        for path in [&description.cli, &description.launcher] {
            let path = resolve(path)?;
            if !path.is_file() || path.parent() != candidate.path.parent() {
                return Err(
                    "ContinuumCore host/launcher must be files beside the selected artifact"
                        .to_string(),
                );
            }
        }
        if !resolve(&description.log_directory)?.is_dir() {
            return Err("ContinuumCore logDirectory is not a directory".to_string());
        }
        if !resolve(&description.engine)?.is_file() {
            return Err("ContinuumCore engine is not an installed file".to_string());
        }
        if Path::new(&description.cli).file_name() != Some(std::ffi::OsStr::new("continuum.exe"))
            || Path::new(&description.launcher).file_name()
                != Some(std::ffi::OsStr::new("run-service-hidden.ps1"))
        {
            return Err("ContinuumCore requires the installed CLI and hidden launcher".to_string());
        }
        for value in [
            &description.artifact,
            &description.socket,
            &description.launcher,
            &description.cli,
            &description.engine,
            &description.log_directory,
        ] {
            if value.contains(['"', '\r', '\n']) || value.ends_with('\\') {
                return Err(
                    "ContinuumCore descriptor cannot be quoted as a task argument".to_string(),
                );
            }
        }
        let expected = format!(
            "-NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy RemoteSigned -File \"{}\" -ExecutablePath \"{}\" -CorePath \"{}\" -SocketPath \"{}\" -EnginePath \"{}\" -LogDirectory \"{}\"",
            description.launcher, description.cli, description.artifact,
            description.socket, description.engine, description.log_directory,
        );
        if self.arguments != expected {
            return Err("ContinuumCore action differs from its artifact descriptor; rerun the installer before reboot".to_string());
        }
        Ok(())
    }
}

struct PreparedCoreService {
    #[cfg(windows)]
    task: CoreServiceTask,
    /// macOS: the launchd job that owns the core and the slot its plist execs (a1bd8b58).
    #[cfg(target_os = "macos")]
    job: launchd::live::Job,
}

impl PreparedCoreService {
    async fn for_start(socket: &str) -> Result<Option<(Self, PrebuiltCore)>, String> {
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            let _ = socket;
            Ok(None)
        }
        #[cfg(target_os = "macos")]
        {
            // The supervisor owns the launch when one is registered (a1bd8b58): `start`
            // resumes the artifact in the job's slot through launchd, never beside it.
            // A source override stays the explicit opt-out, exactly as on Windows.
            let _ = socket;
            if std::env::var_os("CONTINUUM_FROM_SOURCE").is_some() {
                return Ok(None);
            }
            let Some(job) = launchd::live::job()? else {
                return Ok(None);
            };
            let path = job
                .slot
                .canonicalize()
                .map_err(|e| format!("launchd job {} execs {} but it cannot be resolved: {e}", job.domain.target(), job.slot.display()))?;
            let sha = binary_build_sha(&path).await?;
            let candidate = PrebuiltCore::from_report(path, sha, None)?;
            Ok(Some((Self { job }, candidate)))
        }
        #[cfg(windows)]
        {
            if std::env::var_os("CONTINUUM_FROM_SOURCE").is_some() {
                return Ok(None);
            }
            let Some(task) = Self::query_optional().await? else {
                return Ok(None);
            };
            let description: CoreServiceDescription = serde_json::from_str(&task.description)
                .map_err(|e| format!("ContinuumCore is not prepared by the current installer: {e}; rerun the installer"))?;
            let path = Path::new(&description.artifact)
                .canonicalize()
                .map_err(|e| format!("cannot resolve installed service artifact: {e}"))?;
            // Start resumes an installed artifact; a developer's current checkout
            // need not match it. Reboot's explicit candidate still MUST match HEAD.
            let sha = binary_build_sha(&path).await?;
            let candidate = PrebuiltCore::from_report(path, sha, None)?;
            task.validate(&candidate, socket, &Self::shell()?)?;
            Ok(Some((Self { task }, candidate)))
        }
    }

    /// Stage a freshly built artifact INTO the slot the supervisor is bound to, and
    /// return the staged core as the candidate the handoff validates. The supervisor's
    /// descriptor names one artifact path and the CLI beside it; a warm build lands in
    /// the cargo target dir. This is the copy every hand deploy did before
    /// `--prebuilt <slot>` — the old files move aside as `.prev.exe` (a running exe can
    /// be renamed, never overwritten), the new ones copy in, and the slot's CLI follows
    /// the core so the two never drift (#422's STALE CLI). Nothing here stops the core.
    async fn stage(built: &PrebuiltCore, socket: &str) -> Result<PrebuiltCore, String> {
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            let _ = (built, socket);
            Err("reboot --service is supported only on Windows and macOS".to_string())
        }
        #[cfg(target_os = "macos")]
        {
            // Same contract as Windows: into the slot the supervisor execs, re-read the
            // sha off the staged file, refuse the handoff on any mismatch.
            let _ = socket;
            let job = launchd::live::job()?
                .ok_or("no launchd job for the core is registered; run install-service.sh install --system")?;
            let staged = launchd::live::stage(&job, &built.path)?;
            let staged = staged
                .canonicalize()
                .map_err(|e| format!("staged artifact cannot be resolved: {e}"))?;
            let sha = binary_build_sha(&staged).await?;
            if sha != built.build_sha {
                return Err(format!(
                    "staged artifact reports build {sha}, the warm build was {} — refusing the handoff",
                    built.build_sha
                ));
            }
            println!("✓ staged build {} into the launchd slot: {}", built.build_sha, staged.display());
            PrebuiltCore::from_report(staged, sha, None)
        }
        #[cfg(windows)]
        {
            let task = Self::query().await?;
            let description: CoreServiceDescription = serde_json::from_str(&task.description)
                .map_err(|e| format!("ContinuumCore is not prepared by the current installer: {e}; rerun the installer"))?;
            let slot_core = PathBuf::from(&description.artifact);
            let slot_cli = PathBuf::from(&description.cli);
            let built_cli = built
                .path
                .with_file_name("continuum.exe");
            let move_aside_and_copy = |from: &Path, to: &Path| -> Result<(), String> {
                if to.exists() {
                    let prev = to.with_extension("prev.exe");
                    // `.prev.exe` IS A SINGLE PARKING SPACE, and a running image can be
                    // RENAMED on Windows (see this function's doc) — so a predecessor that
                    // was never reaped keeps executing FROM `.prev.exe` and pins that exact
                    // name. The removal below then fails, and it used to fail silently
                    // (`let _ = remove_file`), after which the rename reported AccessDenied
                    // against `to` — naming the CURRENT file for a lock held on a DIFFERENT
                    // one. Measured 2026-09-22 on Astra's node: two staging attempts denied
                    // while core 25040 ran from `.prev.exe`, read as a permissions problem
                    // for an hour. There is no permission to grant; a process is running
                    // from the file.
                    if let Err(e) = std::fs::remove_file(&prev) {
                        if prev.exists() {
                            return Err(format!(
                                "a PREVIOUS artifact is still present at {} and could not be \
                                 removed ({e}) — a process is almost certainly still running \
                                 from it, which also means the kill before this stage did not \
                                 take. Staging cannot proceed until that process exits; this \
                                 is not a file-permission fault",
                                prev.display()
                            ));
                        }
                    }
                    std::fs::rename(to, &prev)
                        .map_err(|e| format!("cannot move {} aside: {e}", to.display()))?;
                }
                std::fs::copy(from, to)
                    .map(|_| ())
                    .map_err(|e| format!("cannot stage {} into {}: {e}", from.display(), to.display()))
            };
            move_aside_and_copy(&built.path, &slot_core)?;
            if built_cli.is_file() {
                move_aside_and_copy(&built_cli, &slot_cli)?;
            } else {
                println!(
                    "⚠ no CLI beside the warm artifact ({}) — the slot's CLI stays as it was",
                    built_cli.display()
                );
            }
            let staged_path = slot_core
                .canonicalize()
                .map_err(|e| format!("staged artifact cannot be resolved: {e}"))?;
            let sha = binary_build_sha(&staged_path).await?;
            if sha != built.build_sha {
                return Err(format!(
                    "staged artifact reports build {sha}, the warm build was {} — refusing the handoff",
                    built.build_sha
                ));
            }
            println!(
                "✓ staged build {} into the supervisor's slot: {}",
                built.build_sha,
                staged_path.display()
            );
            let _ = socket;
            PrebuiltCore::from_report(staged_path, sha, None)
        }
    }

    async fn prepare(candidate: &PrebuiltCore, socket: &str) -> Result<Self, String> {
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            let _ = (candidate, socket);
            Err("reboot --service is supported only on Windows and macOS".to_string())
        }
        #[cfg(target_os = "macos")]
        {
            // The candidate must BE the slot's file: launchd execs the slot, so a candidate
            // anywhere else would be validated here and never run (#194's shape).
            let _ = socket;
            let job = launchd::live::job()?
                .ok_or("no launchd job for the core is registered; run install-service.sh install --system")?;
            let slot = job.slot.canonicalize().map_err(|e| format!("slot {} cannot be resolved: {e}", job.slot.display()))?;
            let cand = candidate.path.canonicalize().map_err(|e| format!("candidate {} cannot be resolved: {e}", candidate.path.display()))?;
            if slot != cand {
                return Err(format!(
                    "candidate {} is not the launchd slot {} — stage it first; the supervisor only ever runs the slot",
                    cand.display(),
                    slot.display()
                ));
            }
            Ok(Self { job })
        }
        #[cfg(windows)]
        {
            let task = Self::query().await?;
            task.validate(candidate, socket, &Self::shell()?)?;
            Ok(Self { task })
        }
    }

    #[cfg(windows)]
    fn shell() -> Result<PathBuf, String> {
        let root = std::env::var_os("SystemRoot").ok_or("SystemRoot is unset")?;
        Ok(PathBuf::from(root).join("System32/WindowsPowerShell/v1.0/powershell.exe"))
    }

    #[cfg(windows)]
    async fn powershell(script: &str) -> Result<String, String> {
        supervisor_install::powershell(script, Duration::from_secs(30))
            .await
            .map_err(|e| format!("ContinuumCore: {e}"))
    }

    #[cfg(windows)]
    async fn query() -> Result<CoreServiceTask, String> {
        Self::query_optional().await?.ok_or_else(|| {
            "ContinuumCore is not registered; run the normal installer first".to_string()
        })
    }

    #[cfg(windows)]
    async fn query_optional() -> Result<Option<CoreServiceTask>, String> {
        let json = Self::powershell(
            "$ErrorActionPreference='Stop'; [Console]::OutputEncoding=[Text.UTF8Encoding]::new($false); $t=Get-ScheduledTask | Where-Object { $_.TaskName -eq 'ContinuumCore' -and $_.TaskPath -eq '\\' }; if ($null -eq $t) { 'null'; exit 0 }; if (@($t.Actions).Count -ne 1) { throw 'Expected one core action' }; [pscustomobject]@{description=$t.Description; command=$t.Actions[0].Execute; arguments=$t.Actions[0].Arguments; enabled=[bool]$t.Settings.Enabled; state=[string]$t.State} | ConvertTo-Json -Compress",
        ).await?;
        serde_json::from_str(&json).map_err(|e| format!("cannot read ContinuumCore task: {e}"))
    }

    async fn launch(self, wait_for_death: &[i32]) -> Result<u64, String> {
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            let _ = wait_for_death;
            Err("reboot --service is supported only on Windows and macOS".to_string())
        }
        #[cfg(target_os = "macos")]
        {
            let started = std::time::Instant::now();
            // The old core is gone (stop_with ran) or launchd's `-k` ends it; either way
            // wait for the pids we were told about before the receipt is read.
            let deadline = std::time::Instant::now() + Duration::from_secs(60);
            while wait_for_death.iter().any(|pid| pid_alive(*pid)) {
                if std::time::Instant::now() >= deadline {
                    return Err("the old core did not exit within 60 s; no kickstart was issued".to_string());
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            eprintln!(
                "▶ supervisor={}: kickstarting the staged artifact ({})",
                self.job.domain.target(),
                self.job.slot.display()
            );
            launchd::live::kickstart(&self.job.domain)?;
            let socket = socket_path();
            let core_pid = move || launchd::live::serving_core_pid(&socket);
            launchd::live::wait_owned(&self.job, core_pid, core_is_up, Duration::from_secs(5 * 60)).await?;
            Ok(started.elapsed().as_secs())
        }
        #[cfg(windows)]
        {
            let started = std::time::Instant::now();
            // A task may still be finishing its foreground host after the core
            // exits. IgnoreNew would silently swallow our start in that window.
            let mut ticks = tokio::time::interval(Duration::from_secs(1));
            loop {
                ticks.tick().await;
                let task = Self::query().await?;
                if task.description != self.task.description
                    || task.command != self.task.command
                    || task.arguments != self.task.arguments
                    || !task.enabled
                {
                    return Err("ContinuumCore task changed during reboot; refusing to launch a different action".to_string());
                }
                if !wait_for_death.iter().any(|pid| pid_alive(*pid)) && task.state == "Ready" {
                    break;
                }
                if started.elapsed() >= Duration::from_secs(60) {
                    return Err("ContinuumCore old process/task did not finish within 60 s; no second core was launched".to_string());
                }
            }
            Self::powershell("$ErrorActionPreference='Stop'; Start-ScheduledTask -TaskName 'ContinuumCore' -TaskPath '\\'").await?;
            eprintln!("▶ supervisor=ContinuumCore: starting the prepared installed artifact");
            let ready = std::time::Instant::now();
            let mut ticks = tokio::time::interval(Duration::from_secs(2));
            loop {
                ticks.tick().await;
                if !wait_for_death.iter().any(|pid| pid_alive(*pid)) && core_is_up().await {
                    if Self::query().await?.state != "Running" {
                        return Err("A core answered but ContinuumCore is not running; refusing an unsupervised startup receipt".to_string());
                    }
                    return Ok(started.elapsed().as_secs());
                }
                if ready.elapsed() >= Duration::from_secs(5 * 60) {
                    return Err("ContinuumCore did not answer within 5 minutes; inspect its task result and service logs".to_string());
                }
                if ready.elapsed() >= Duration::from_secs(5)
                    && Self::query().await?.state != "Running"
                {
                    return Err("ContinuumCore task stopped before the core answered; inspect its task result and service logs".to_string());
                }
            }
        }
    }
}

/// May this reboot build before it stops? Needs a start script (one build definition)
/// and headroom. The refusal names the number so the operator sees why the dark
/// window will be long this time.
fn warm_build_allowed(free_bytes: u64, script: Option<PathBuf>) -> Result<PathBuf, String> {
    let Some(script) = script else {
        return Err("no start script (installed node) — nothing to build".to_string());
    };
    if free_bytes < WARM_BUILD_MIN_FREE_BYTES {
        return Err(format!(
            "{:.1} GiB free, the warm build wants {} GiB beside the serving core",
            free_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
            WARM_BUILD_MIN_FREE_BYTES / (1024 * 1024 * 1024)
        ));
    }
    Ok(script)
}

/// Available system memory, through the ONE derivation the substrate already owns.
///
/// NOT `sysinfo::available_memory()`. That call returns **0 on macOS** while `total`
/// and `used` are both correct — `system_resources::memory_pressure` found this, says
/// so in so many words at its own call site, and exists as `available_from` precisely
/// so "every reader" shares one answer. This function was not one of those readers.
///
/// The cost was the whole warm-build path on every Mac. `warm_build_allowed` compares
/// this against `WARM_BUILD_MIN_FREE_BYTES` (12 GiB), so a permanent 0 meant the gate
/// could never open: every deploy stopped the core first and built afterwards, and every
/// stop cut whatever was mid-turn. Measured on the M5 2026-09-21, two consecutive
/// deploys 35 minutes apart printed `no warm build: 0.0 GiB free` and reported
/// `cognition (drain Incomplete { in_flight: 7 })` then `{ in_flight: 9 }` — sixteen
/// citizen turns — while `memory.pressure`, reading `available_from` at the same
/// moments, published `avail_mb` of 9,009 and 8,324. Two measurements of one quantity,
/// nine gigabytes apart, and the deploy gate held the one that is always zero here.
fn available_memory_bytes() -> u64 {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    continuum_core::system_resources::memory_pressure::available_from(&sys)
}

/// A per-attempt receipt owned by this invocation, never a cached artifact hint.
/// The build script writes its selected native path only after all checks pass.
struct WarmBuildReceipt(PathBuf);

impl WarmBuildReceipt {
    fn create() -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!(
            "continuum-warm-build-{}.receipt",
            uuid::Uuid::new_v4()
        ));
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| format!("cannot create warm-build receipt: {e}"))?;
        Ok(Self(path))
    }

    fn artifact(&self) -> Result<PathBuf, String> {
        use std::io::Read;
        const MAX_RECEIPT_BYTES: u64 = 16 * 1024;
        let mut report = String::new();
        std::fs::File::open(&self.0)
            .and_then(|file| file.take(MAX_RECEIPT_BYTES + 1).read_to_string(&mut report))
            .map_err(|e| format!("cannot read warm-build receipt: {e}"))?;
        let path = report.strip_suffix('\n').unwrap_or(&report); // No optional framing newline means the entire report is the path.
        if report.len() > MAX_RECEIPT_BYTES as usize
            || path.is_empty()
            || path.contains(['\n', '\r', '\0'])
            || !Path::new(path).is_absolute()
        {
            return Err("warm build did not report one absolute native artifact path; leaving the running core untouched".to_string());
        }
        Ok(PathBuf::from(path))
    }
}

impl Drop for WarmBuildReceipt {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

async fn prepare_warm_build(mut cmd: std::process::Command) -> Result<PrebuiltCore, String> {
    let receipt = WarmBuildReceipt::create()?;
    cmd.env("CONTINUUM_BUILD_ONLY", "1")
        .env("CONTINUUM_BUILD_RECEIPT", &receipt.0)
        .stdin(Stdio::null());
    // Under the deploy consumer there is no terminal: the build's output goes to the
    // consumer's log, or a failing build leaves no reason anywhere (2026-09-19, the
    // 5090's first unattended deploy: 30 minutes of rustc, then nothing to read).
    if let Some(log) = DEPLOY_LOG.get().and_then(|p| open_log_for_child(p).ok()) {
        if let Ok(err) = log.try_clone() {
            cmd.stdout(Stdio::from(log)).stderr(Stdio::from(err));
        }
    }
    let status = cmd.status().map_err(|e| {
        format!("warm build could not start: {e}; leaving the running core untouched")
    })?;
    if !status.success() {
        // Say WHAT ran. Three consumer attempts on the 5090 (2026-09-19) each died
        // in one second with nothing in the log; the exit code alone named nothing.
        return Err(format!(
            "warm build exited {status}; leaving the running core untouched
  command: {cmd:?}
  envs: {:?}
  cwd: {:?}",
            cmd.get_envs()
                .map(|(k, v)| {
                    let value = v.map(|v| v.to_string_lossy().into_owned()).unwrap_or("<unset>".into()); // unwrap_or: a None env value IS an unset (env_remove), shown as such
                    format!("{}={value}", k.to_string_lossy())
                })
                .collect::<Vec<_>>(),
            cmd.get_current_dir().map(|p| p.to_path_buf()).or_else(|| std::env::current_dir().ok())
        ));
    }
    PrebuiltCore::prepare(&receipt.artifact()?).await
}

/// Rebuild source by default, or hand off to an explicitly validated prebuilt
/// core. Both use the same leases, selective teardown and deploy verification.
async fn reboot(options: RebootOptions) -> Result<(), String> {
    // macOS: a registered launchd job owns the launch (a1bd8b58). `--service` is implied
    // by its presence, so a bare `continuum reboot` stages + kickstarts instead of
    // spawning an orphan beside the supervisor. `CONTINUUM_FROM_SOURCE` opts out.
    #[cfg(target_os = "macos")]
    let options = {
        let mut options = options;
        if !options.service && std::env::var_os("CONTINUUM_FROM_SOURCE").is_none() {
            if let Some(job) = launchd::live::job()? {
                println!("▶ launchd job {} owns the core — this reboot hands it the launch", job.domain.target());
                options.service = true;
            }
        }
        options
    };
    let mut prebuilt = match options.prebuilt {
        Some(path) => Some(PrebuiltCore::prepare(&path).await?),
        None => None,
    };
    let requested_source_build = prebuilt.is_none();
    if options.validate_only {
        let candidate = prebuilt
            .as_ref()
            .ok_or("--validate-only requires --prebuilt")?;
        println!(
            "prebuilt validated: {} (build {})",
            candidate.path.display(),
            candidate.build_sha
        );
        return Ok(());
    }
    let force = options.force;
    let socket = socket_path();
    // With a hand-supplied artifact the supervisor is prepared NOW, while the old core
    // still serves (its loader + provenance checked before anything stops). Without
    // one, it is prepared right after the warm build below — same check, same
    // ordering, the artifact simply comes from the build instead of a hand.
    let mut service = match (options.service, prebuilt.as_ref()) {
        (true, Some(candidate)) => Some(PreparedCoreService::prepare(candidate, &socket).await?),
        _ => None,
    };
    // Training guard (task #137, Joel's consent-gate doctrine: the denial names
    // the policy AND the path). A core swap kills spawned trainer children
    // (mlx_lm.lora) and their in-process watchers — glass-boxed 2026-07-11: 41
    // jobs submitted, zero outcomes recorded, all orphaned by reboots. Live
    // training is a lease the reboot must respect, not silently revoke.
    let trainers = running_trainer_pids();
    if !trainers.is_empty() && !force {
        return Err(format!(
            "training in flight (mlx_lm pid(s) {}) — a reboot would kill it and the \
             run would be journaled killed-by-reboot. Wait for it to finish, or rerun \
             with `continuum reboot --force` if losing the run is acceptable.",
            trainers
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    // Benchmark guard — the same lease, one level up. A detached `benchmark/swe-solve` is a
    // tokio task inside the core, so a swap kills it with no child process to notice and no
    // partial result: minutes of a persona's drive, gone. Found the honest way the day it
    // shipped — two reboots silently killed a run and its ledger simply never appeared.
    let benches = continuum_core::cognition::swe_bench::in_flight_solve_runs();
    if !benches.is_empty() && !force {
        return Err(format!(
            "benchmark run(s) in flight ({}) — a reboot kills them mid-drive. Stop them \
             cleanly first: `continuum benchmark/round-stop --run_id <id>` cancels at the \
             next task boundary and keeps every streamed grade (benchmark/pause is a HOLD \
             and leaves them running). Or wait for them, or rerun with `continuum reboot \
             --force` to take the restart-and-resume path (the boot resume re-fires each \
             killed solve once serving + citizens are back).",
            benches
                .iter()
                .map(|(run, inst)| format!("{run} on {inst}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    // Eval-run guard — the third lease, same shape. cognition/eval runs had NO
    // on-disk in-flight signal until 2026-08-23 (the marker in each eval world),
    // so this guard named zero runs while a MirrorCode battery was mid-task and
    // a reboot killed it unasked. pid-checked here: a marker whose core died is
    // debris for the next provision's sweep, not an in-flight run.
    let evals: Vec<String> = continuum_core::cognition::eval::in_flight_eval_runs()
        .into_iter()
        .filter(|(_, pid)| pid_alive(*pid as i32))
        .map(|(run, _)| run)
        .collect();
    if !evals.is_empty() && !force {
        return Err(format!(
            "eval run(s) in flight ({}) — a reboot would kill them mid-exam. Wait for them, \
             or rerun with `continuum reboot --force` if losing the runs is acceptable.",
            evals.join(", ")
        ));
    }
    if !evals.is_empty() {
        println!(
            "⚠ --force: rebooting over {} live eval run(s) — they die here; their worlds are \
             swept at the next provision",
            evals.len()
        );
    }
    if !benches.is_empty() {
        println!(
            "⚠ --force: rebooting over {} live benchmark run(s) — they die here and the \
             ledger will record killed-by-restart at next boot",
            benches.len()
        );
    }
    if !trainers.is_empty() {
        println!(
            "⚠ --force: rebooting over live training (mlx_lm pid(s) {}) — the run dies \
             here and the job ledger will record killed-by-reboot at next boot",
            trainers
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );
    }
    // Snapshot the running core PIDs, then STOP EVERYTHING BEFORE THE BUILD.
    //
    // This deliberately retires the "~0 downtime" overlap (old core keeps
    // serving while the fresh binary builds). That overlap made the build
    // CONTEND with the dying core for the whole machine — a 35B llama-server
    // saturating GPU/RAM/CPU while cargo crawls beside it, both slower, to keep
    // alive a process the very next step kills (Joel, 2026-08-23: "you are
    // forced to contend for resources with something that's going away and
    // it's silly"). On a dev box the build-window downtime is free; the
    // contention is not. Stop-first also collapses the Windows special case
    // (a running .exe locks its own binary against rebuild) into the normal
    // path instead of an ordering quirk buried in start-server.sh.
    //
    // `stop()` is the ONE owner of teardown — pidfile core + split-brain
    // sweep + owned-orphan reap + serving-lane sweep — so reboot inherits every
    // lesson encoded there (two-cores-at-once 2026-08-14, the 24h orphaned
    // llama-server, the 2026-08-17 two-resident-lanes starvation) instead of
    // re-implementing a partial copy.
    let old = running_core_pids();
    if old.is_empty() {
        println!("▶ no core running — starting fresh (socket={socket})");
    } else if let Some(candidate) = &prebuilt {
        println!(
            "▶ rebooting core (socket={socket}): replacing pid(s) {} with verified prebuilt {} (build {}) — no rebuild",
            old.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(","),
            candidate.path.display(), candidate.build_sha,
        );
    } else {
        println!(
            "▶ rebooting core (socket={socket}): stopping pid(s) {} FIRST, then building on a free machine",
            old.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(",")
        );
    }
    // Publish the claim before teardown for the WHOLE build/swap. Held until this function returns, so a
    // concurrent `continuum <verb>` refuses instead of autostarting the pre-swap installed
    // image and stealing the socket (the DEPLOY MISMATCH measured 2026-08-17).
    // THE WARM BUILD. Stop-first is right when a build beside a serving core would
    // starve it (Joel, 2026-08-23); on a machine with headroom it is ten minutes of
    // darkness per deploy for nothing — eight deploys on 2026-09-13 cost the round
    // eighty dark minutes, dropped turns and lapsed leases each time. So: with a
    // source tree, no --prebuilt, and enough free memory, build FIRST through the
    // start script's own definition (CONTINUUM_BUILD_ONLY=1) while the core serves;
    // the exact reported artifact is validated before teardown and launched directly.
    // The claim is published BEFORE the warm build, not after: the build is the deploy's
    // longest phase and it runs beside the serving core (rustc on every core for minutes).
    // The running core reads the claim (`deploy_claim::in_flight`) to know that a decode
    // measured now measures the compiler, not the lane — 2026-09-16 the decode knee fell
    // 4 → 2 on samples taken inside its own reboot's build. The claim's only other reader
    // is the CLI's autostart gate, which is right to refuse during a build too.
    let target_sha = prebuilt
        .as_ref()
        .map(|p| p.build_sha.clone())
        .or_else(git_head_short_sha);
    let _deploy_claim = DeployClaimGuard::take(target_sha.as_deref().unwrap_or("unknown"));
    // A failed attempted warm build returns without stopping the serving core.
    // Below the headroom line the existing stop-first path remains available.
    if prebuilt.is_none() {
        match warm_build_allowed(available_memory_bytes(), locate_start_script().ok()) {
            Ok(script) => {
                let started = std::time::Instant::now();
                let mut cmd = std::process::Command::new(locate_bash()?);
                cmd.arg(&script);
                apply_core_runtime_env(&mut cmd);
                if let CliSelfBuild::Skip { .. } = cli_self_build(std::env::consts::OS) {
                    cmd.env("CONTINUUM_SKIP_SELF_BUILD", "1");
                }
                println!("▶ warm build: compiling from source while the core keeps serving (build-only pass of {})", script.display());
                prebuilt = Some(prepare_warm_build(cmd).await?);
                println!(
                    "✓ warm artifact validated in {}s — stopping now for direct artifact handoff",
                    started.elapsed().as_secs()
                );
                // `--service` with no hand-supplied artifact: the warm build's artifact is
                // the one the supervisor takes — STAGED into the slot the supervisor is
                // bound to, then prepared, all before the old core stops. The supervisor
                // validates the exact artifact path in its descriptor; a warm build lands
                // in the cargo target dir, so without this step the handoff refused
                // ("does not select the requested artifact") — measured 2026-09-19
                // 05:1xZ, the 5090's first unattended deploy: a 1,145 s build, validated,
                // then refused at the door. Every hand deploy had done this copy by hand.
                if options.service && service.is_none() {
                    if let Some(built) = prebuilt.take() {
                        let staged = PreparedCoreService::stage(&built, &socket).await?;
                        service = Some(PreparedCoreService::prepare(&staged, &socket).await?);
                        prebuilt = Some(staged);
                    }
                }
            }
            Err(why) => {
                if options.service {
                    // The supervisor path has no source-launch fallback: a service handoff
                    // without an artifact would be the child-of-launcher launch this card
                    // (82af11f5) exists to end. Say so and leave the running core standing.
                    return Err(format!(
                        "reboot --service: no artifact to hand the supervisor — {why}; the running core stands"
                    ));
                }
                println!("▶ no warm build: {why} — stopping first, then building")
            }
        }
    }
    // Reboot deliberately does NOT fail on an unsaved module: the caller's goal is a
    // running core, and refusing to continue would leave the node down over a module that
    // could not flush. The warning is printed by `stop_with`; `stop` is the verb whose
    // exit code carries it.
    let _ = stop_with(true).await?;
    // Keep the launcher's wait as the honesty check that teardown actually took.
    let source = prebuilt
        .as_ref()
        .map_or(LaunchSource::FromSource, LaunchSource::Prebuilt);
    let secs = match service {
        Some(service) => service.launch(&old).await?,
        None => launch_core(&old, source).await?,
    };
    // Deploy-verification (#194): a new core is up — but is it the FRESHLY-BUILT one? If
    // start-server.sh's build was a stale cache no-op or silently failed, an OLD binary would
    // answer on the same socket and this reboot would report success while running dead code.
    // NO success line before provenance is proven ("core ready" without provenance is a false
    // deploy receipt — the 2026-08-01 Windows-node incident): announce liveness neutrally,
    // then verify, and let "✅ deploy verified" be the ONLY checkmark a reboot prints.
    println!(
        "core answering (socket={socket}) after ~{secs}s — verifying deploy provenance (#194)"
    );
    // Did THIS reboot replace the installed CLI? Only when it went through the build script
    // (a source tree exists — the same condition `plan_launch` uses to pick `Script` for a
    // FromSource launch) AND the platform allows a self-build. Both terms matter: on an
    // installed node with no checkout nothing was rebuilt, and on Windows `cli_self_build`
    // deliberately skips. Getting this wrong in either direction re-creates the noise this
    // flag exists to remove, or hides a genuinely stale CLI behind a reassuring handoff line.
    let rebuilt_cli = requested_source_build
        && locate_start_script().is_ok()
        && matches!(cli_self_build(std::env::consts::OS), CliSelfBuild::Rebuild);
    verify_deployed_build_against(rebuilt_cli, prebuilt.as_ref()).await
}

/// Prove the running core is built from the source this deploy shipped — the honest half of
/// "reboot succeeded". The RUNNING core self-reports its compiled-in SHA over the socket
/// (`ping` → `buildSha`): the live process image answers for itself, so this can't be fooled
/// by re-exec'ing an on-disk file a rebuild already swapped under the still-running old core,
/// and it needs no path guessing at all (the 2026-08-01 Windows-node incident: a "next to the
/// CLI exe" guess printed "could not locate continuum-core-server" while a 2-day-old binary
/// kept serving — and the reboot still said success).
///
/// Expected SHA precedence:
/// 1. git HEAD of the checkout the reboot built from (start-server.sh's freshness guard
///    already pins artifact == source at build time), when run inside a git tree;
/// 2. otherwise the deployable artifact resolved by the SAME order install-service.sh uses
///    (`CONTINUUM_CORE_BIN` env → installed locations → cargo target dir), asked for its
///    `--build-sha` — the installed-node path, where there is no git tree.
///
/// NEVER skips soft: any gap (no core answering, a pre-#194 core without `buildSha`,
/// `unknown` provenance, no resolvable artifact) is an ERROR — a reboot must not print a
/// success line it cannot back with provenance ([[fallbacks-are-illegal-fail-loud]]).
/// This CLI's own build SHA, stamped at compile time by `build.rs` for the whole crate —
/// the `continuum` bin lives in `continuum-core`, so it gets the same constant the core does,
/// describing the binary you are RUNNING rather than one found on disk.
const CLI_BUILD_SHA: &str = env!("CONTINUUM_BUILD_GIT_SHA");

/// `rebuilt_cli` says whether THIS invocation replaced the installed CLI — true from
/// `reboot` (start-server.sh rebuilds + reinstalls it unless `cli_self_build` skips the
/// platform), false from a bare `deploy-verify`. It is what lets the CLI-provenance note
/// tell a HANDOFF ("the next run gets the new CLI") apart from real STALENESS, instead of
/// warning on every successful deploy.
async fn verify_deployed_build(rebuilt_cli: bool) -> Result<(), String> {
    verify_deployed_build_against(rebuilt_cli, None).await
}

async fn verify_deployed_build_against(
    rebuilt_cli: bool,
    prebuilt: Option<&PrebuiltCore>,
) -> Result<(), String> {
    let socket = socket_path();
    // The RUNNING core's provenance, from the process itself. BOUNDED: a core
    // that accepts the socket mid-boot but never answers made `continuum
    // reboot` hang for good (IntelMac's node, 50 min, 2026-09-05) — the same
    // wall-clock-forever shape as the 300 s boot watchdog, on the other side of
    // the socket. A named failure beats a silent hang.
    let reply = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        connection()
            .commands()
            .execute_value("ping", Value::Object(Default::default())),
    )
    .await
    .map_err(|_| {
        format!(
            "deploy-verify: the core on {socket} accepted the socket but did not answer `ping` \
             within 30 s — mid-boot (retry) or wedged (read boot.phase / boot.module_init)"
        )
    })?
    .map_err(|e| format!("deploy-verify: no core answering on {socket}: {e}"))?;
    let actual = reply
        .get("buildSha")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    // What the deploy SHOULD have shipped.
    let (expected, expected_source) = match prebuilt {
        Some(candidate) => (
            candidate.build_sha.clone(),
            format!(
                "prebuilt artifact {} (no rebuild)",
                candidate.path.display()
            ),
        ),
        None => match git_head_short_sha() {
            Some(head) => (head, "git HEAD of this checkout".to_string()),
            None => {
                let artifact = resolve_core_artifact()?;
                let sha = binary_build_sha(&artifact).await?;
                (sha, format!("artifact {}", artifact.display()))
            }
        },
    };

    let running_desc = describe_running_core(&socket);
    // The CLI's own provenance rides alongside the core's, on BOTH outcomes: a stale CLI
    // is relevant whether or not the core swap took.
    let cli_note = cli_staleness_note(CLI_BUILD_SHA, &expected, &expected_source, rebuilt_cli);
    match deploy_verdict(
        actual.as_deref(),
        &expected,
        &expected_source,
        &running_desc,
    ) {
        Ok(line) => {
            println!("{line}");
            if let Some(note) = cli_note {
                println!("{note}");
            }
            // The last line of a verified deploy is WHERE to look.
            println!("{}", desktop_receipt_line().await);
            Ok(())
        }
        Err(e) => Err(match cli_note {
            Some(note) => format!("{e}\n{note}"),
            None => e,
        }),
    }
}

/// Best-effort human identity of the running core for error messages: socket + pid(s) +
/// process image path where resolvable. Diagnostics ONLY — the SHA itself always comes from
/// the process over the socket, never from re-executing a path guessed here.
///
/// `socket=` is the socket THIS CLI resolved. When a listed core is provably bound
/// somewhere else, that is said too rather than left for the reader to assume the two
/// agree — this is the third site (with the two `Occupied` refusals) where a client-side
/// path was being printed as though it were the core's.
fn describe_running_core(socket: &str) -> String {
    let pids = running_core_pids();
    let mut desc = format!("socket={socket}");
    if !pids.is_empty() {
        desc.push_str(&format!(
            ", pid(s) {}",
            pids.iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(",")
        ));
        let elsewhere: Vec<String> = running_core_sockets_for(&pids)
            .into_iter()
            .filter(|(_, s)| s != socket)
            .map(|(pid, s)| format!("pid {pid} bound to {s}"))
            .collect();
        if !elsewhere.is_empty() {
            desc.push_str(&format!(" — NOT this socket: {}", elsewhere.join(", ")));
        }
    }
    if let Some(img) = running_core_binary() {
        desc.push_str(&format!(", image {}", img.display()));
    }
    desc
}

/// Resolve the deployable `continuum-core-server` artifact — the SAME order
/// `tools/scripts/install-service.sh::resolve_core_bin` uses, centralized so the CLI and the
/// service installer can never disagree: `CONTINUUM_CORE_BIN` env → installed locations
/// (`/usr/local/bin`, `~/.continuum/bin`) → cargo target dir (release, then debug; default
/// target dir `~/.continuum/cache/cargo-target`, matching start-server.sh). NEVER "next to
/// the CLI exe" — that guess is what printed "could not locate continuum-core-server" on the
/// 2026-08-01 Windows node while a stale core kept serving.
fn resolve_core_artifact() -> Result<PathBuf, String> {
    let env_bin = std::env::var("CONTINUUM_CORE_BIN").ok();
    if let Some(explicit) = &env_bin {
        let p = PathBuf::from(explicit);
        if p.is_file() {
            return Ok(p);
        }
        return Err(format!("CONTINUUM_CORE_BIN={explicit} is not a file"));
    }
    let home = home_dir()?;
    let target = std::env::var("CARGO_TARGET_DIR").ok();
    let candidates = core_artifact_candidates(&home, target.as_deref());
    candidates
        .iter()
        .find(|p| p.is_file())
        .cloned()
        .ok_or_else(|| {
            format!(
                "no continuum-core-server artifact found. Searched (install-service.sh order): {}. \
                 Build one (`continuum reboot` from the repo, or npm start) or set CONTINUUM_CORE_BIN.",
                candidates
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

/// The ordered candidate list behind [`resolve_core_artifact`] — pure (paths in, paths out)
/// so the shared resolution ORDER is pinned by a unit test against install-service.sh.
fn core_artifact_candidates(home: &str, cargo_target_dir: Option<&str>) -> Vec<PathBuf> {
    let exe = if cfg!(windows) {
        "continuum-core-server.exe"
    } else {
        "continuum-core-server"
    };
    let target = cargo_target_dir
        .map(str::to_string)
        .unwrap_or_else(|| format!("{home}/.continuum/cache/cargo-target"));
    let mut out = Vec::new();
    if !cfg!(windows) {
        out.push(PathBuf::from("/usr/local/bin").join(exe));
    }
    out.push(PathBuf::from(home).join(".continuum").join("bin").join(exe));
    out.push(PathBuf::from(&target).join("release").join(exe));
    out.push(PathBuf::from(&target).join("debug").join(exe));
    out
}

/// The manifest-declared runtime library dirs that actually EXIST under
/// `root`. Pure over the filesystem so the selection rule is testable without
/// spawning anything.
///
/// Mirrors the manifest's `runtime_path` entries: fixed-name tool dirs
/// (`tools/<tool>/bin`) and versioned CUDA trees (`cuda-*/Library/bin`). The
/// CUDA sweep reads the directory rather than shelling a glob, so it behaves
/// identically on every platform and never depends on a shell being present.
fn runtime_library_dirs(root: &std::path::Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for tool in ["cmake", "llvm"] {
        let bin = root.join("tools").join(tool).join("bin");
        if bin.is_dir() {
            dirs.push(bin);
        }
    }
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if !name.starts_with("cuda-") {
                continue;
            }
            let bin = entry.path().join("Library").join("bin");
            if bin.is_dir() {
                dirs.push(bin);
            }
        }
    }
    dirs
}

/// Prepend the manifest-declared runtime library dirs to the child's PATH.
///
/// Only dirs that EXIST are added, so a node without CUDA is unaffected and a
/// node with several CUDA majors contributes each real one. Prepended (not
/// appended) so a provisioned toolchain wins over a stray system copy — the
/// same precedence `windows-build-env.sh` applies for the scripted path.
///
/// Non-fatal by construction: if the home dir cannot be resolved there is
/// nothing to add and the child launches exactly as before. This can only add
/// paths that are already on disk under the operator's own continuum root.
fn apply_runtime_library_path(cmd: &mut std::process::Command) {
    let Ok(root) = continuum_root() else {
        return;
    };
    apply_runtime_library_env_in(cmd, &root, std::env::consts::OS);
}

/// The candidate's provenance probe and the actual launch need the same loader
/// environment. Configure only the child; never export into the calling shell.
fn apply_core_runtime_env(cmd: &mut std::process::Command) {
    for (k, v) in continuum_core::config_env::read_all() {
        cmd.env(k, v);
    }
    apply_runtime_library_path(cmd);
}

/// Direct launches retain the caller's cwd and the core's positional socket
/// contract. An explicit prebuilt path never goes through artifact discovery.
fn direct_core_command(artifact: &Path, socket: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new(artifact);
    cmd.arg(socket);
    cmd
}

/// Read the child's effective environment, including config.env assignments and
/// explicit removals. Looking only at our own environment loses launch overrides.
fn command_env(cmd: &std::process::Command, key: &str) -> Option<std::ffi::OsString> {
    match cmd.get_envs().find(|(name, _)| {
        if cfg!(windows) {
            name.as_encoded_bytes().eq_ignore_ascii_case(key.as_bytes())
        } else {
            *name == key
        }
    }) {
        Some((_, value)) => value.map(std::ffi::OsStr::to_os_string),
        None => std::env::var_os(key),
    }
}

/// The installed-library contract also applies to binary-only launches. In
/// particular, Windows otherwise finds System32's older ONNX Runtime and voice
/// initialization panics while the rest of the core reports ready (card d2698cdc).
/// Configure only the child, before spawn; never mutate a running process's env.
fn apply_runtime_library_env_in(cmd: &mut std::process::Command, root: &Path, os: &str) {
    if command_env(cmd, "ORT_DYLIB_PATH").is_none_or(|path| path.is_empty()) {
        let library_name = match os {
            "windows" => "onnxruntime.dll",
            "macos" => "libonnxruntime.dylib",
            _ => "libonnxruntime.so",
        };
        let installed = root.join("lib").join(library_name);
        let homebrew = Path::new("/opt/homebrew/lib/libonnxruntime.dylib");
        if installed.is_file() {
            cmd.env("ORT_DYLIB_PATH", installed);
        } else if os == "macos" && homebrew.is_file() {
            cmd.env("ORT_DYLIB_PATH", homebrew);
        } else if os == "windows" {
            eprintln!(
                "⚠ ONNX Runtime not provisioned at {}. Voice initialization may fail if Windows loads its system copy; install the onnxruntime module or set ORT_DYLIB_PATH.",
                installed.display()
            );
        }
    }

    let dirs = runtime_library_dirs(root);
    if dirs.is_empty() {
        return;
    }
    let existing = command_env(cmd, "PATH").unwrap_or_default();
    let joined = std::env::join_paths(dirs.into_iter().chain(std::env::split_paths(&existing)));
    match joined {
        Ok(path) => {
            cmd.env("PATH", path);
        }
        // A PATH entry containing the separator cannot be joined. Leaving PATH
        // untouched is the honest outcome: the child still launches, and on a
        // CUDA node it fails the same loud way it did before this fix rather
        // than silently inheriting a half-built PATH.
        Err(_) => {}
    }
}

/// The continuum root (`~/.continuum`) — where the deploy claim lives.
fn continuum_root() -> Result<PathBuf, String> {
    Ok(PathBuf::from(home_dir()?).join(".continuum"))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Consult the deploy claim before minting a core, and REPORT what it found.
///
/// Returns `Err` only while a deploy is genuinely in flight. See
/// [`continuum_core::runtime::deploy_claim`] for the incident: mid-build there is no core
/// answering AND the installed image is still the PREVIOUS build, so any autostart in that
/// window launches stale code, wins the socket, and defeats the deploy.
///
/// A claim whose owner is gone or that has aged out is swept and announced rather than
/// silently obeyed — a claim must never be able to wedge the machine (#355's failure mode).
fn deploy_gate(verb: &str) -> Result<(), String> {
    use continuum_core::runtime::deploy_claim::{self, DeployGate};
    let Ok(root) = continuum_root() else {
        return Ok(()); // no HOME → no claim file → nothing to honour
    };
    let claim = deploy_claim::read(&root);
    let alive = claim.as_ref().is_some_and(|c| pid_alive(c.pid));
    match deploy_claim::decide(claim.as_ref(), alive, now_ms()) {
        DeployGate::Clear => Ok(()),
        DeployGate::Abandoned { pid, age_ms, why } => {
            eprintln!(
                "⚠ sweeping an abandoned deploy claim (pid {pid}, {}s old, {why:?}) — \
                 proceeding with `{verb}`",
                age_ms / 1000
            );
            let _ = deploy_claim::clear(&root);
            Ok(())
        }
        DeployGate::InProgress {
            pid,
            age_ms,
            target_sha,
        } => Err(format!(
            "a deploy is in flight (pid {pid} shipping build {target_sha}, {}s in) and no core \
             is answering yet. Starting one now would launch the PRE-SWAP installed binary, \
             which would then hold the socket and make the deploy report the OLD build — \
             measured 2026-08-17. Wait for the deploy to finish; `{verb}` works the moment it \
             does. (If that deploy is dead, its claim is swept automatically once its process \
             exits.)",
            age_ms / 1000
        )),
    }
}

/// RAII deploy claim: published for the length of a swap, released on EVERY exit path
/// (Ok, Err, `?`, panic-unwind). A claim that leaked past its deploy would block autostarts
/// until its owner died, so the release cannot be a line at the end of the happy path.
struct DeployClaimGuard {
    root: PathBuf,
}

impl DeployClaimGuard {
    /// Best-effort by design: if the claim cannot be written the deploy still proceeds —
    /// losing the guard degrades to the old behaviour (which `deploy-verify` still catches),
    /// whereas refusing to deploy over an unwritable advisory file turns a hint into an outage.
    fn take(target_sha: &str) -> Option<Self> {
        use continuum_core::runtime::deploy_claim::{self, DeployClaim};
        let root = continuum_root().ok()?;
        let claim = DeployClaim {
            pid: std::process::id() as i32,
            started_ms: now_ms(),
            target_sha: target_sha.to_string(),
        };
        match deploy_claim::write(&root, &claim) {
            Ok(()) => Some(Self { root }),
            Err(e) => {
                eprintln!(
                    "⚠ could not publish a deploy claim ({e}) — a concurrent command could \
                     autostart a stale core during this build; deploy-verify still catches it"
                );
                None
            }
        }
    }
}

impl Drop for DeployClaimGuard {
    fn drop(&mut self) {
        let _ = continuum_core::runtime::deploy_claim::clear(&self.root);
    }
}

/// The user's home dir — `HOME` (unix) or `USERPROFILE` (Windows). Loud when absent: the
/// resolution order depends on it, and guessing would defeat the shared contract.
fn home_dir() -> Result<String, String> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| {
            "HOME/USERPROFILE unset — cannot resolve installed continuum-core-server locations \
             (set CONTINUUM_CORE_BIN explicitly)"
                .to_string()
        })
}

/// Ask an on-disk `continuum-core-server` artifact for its embedded build SHA
/// (`--build-sha`, exits before any socket/side-effect). Loud on any failure — an artifact
/// that cannot state its provenance cannot anchor a deploy receipt.
async fn binary_build_sha(artifact: &Path) -> Result<String, String> {
    let mut cmd = std::process::Command::new(artifact);
    apply_core_runtime_env(&mut cmd);
    cmd.arg("--build-sha").stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: a provenance probe has no UI.
    }
    let mut cmd = tokio::process::Command::from(cmd);
    cmd.kill_on_drop(true);
    let out = tokio::time::timeout(Duration::from_secs(30), cmd.output())
        .await
        .map_err(|_| {
            format!(
                "{} --build-sha did not answer within 30 s; provenance is unavailable",
                artifact.display()
            )
        })?
        .map_err(|e| format!("cannot run {} --build-sha: {e}", artifact.display()))?;
    if !out.status.success() {
        return Err(format!(
            "{} --build-sha exited {} — a pre-#194 artifact cannot anchor a deploy receipt; rebuild it",
            artifact.display(),
            out.status
        ));
    }
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if sha.is_empty() {
        return Err(format!(
            "{} --build-sha printed nothing — rebuild the artifact",
            artifact.display()
        ));
    }
    Ok(sha)
}

/// Short git HEAD SHA of the current checkout (matches what `build.rs` embeds), or `None`
/// when not in a git tree.
/// Record the cwd's repo checkout for the core (`modules::repo_registry`): the
/// claim-edge staging of a REPO card needs `owner/name → path`, and the core has
/// no cwd. Idempotent, silent outside a repo.
fn record_repo_checkout() {
    let out = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
    };
    // `--show-toplevel` is the WORKTREE when run inside one (a card worktree,
    // deleted on merge — IntelMac's review of #3706); `--git-common-dir` is the
    // clone's .git in both cases, and absolute so the main clone does not
    // answer a relative `.git`.
    let (Some(url), Some(common)) = (
        out(&["remote", "get-url", "origin"]),
        out(&["rev-parse", "--path-format=absolute", "--git-common-dir"]),
    ) else {
        return;
    };
    let Some(root) = continuum_core::modules::repo_registry::clone_root_from_common_dir(&common)
    else {
        return;
    };
    if let Some(repo) = continuum_core::modules::repo_registry::repo_id_from_remote(&url) {
        continuum_core::modules::repo_registry::record(&repo, &root);
    }
}

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
struct DeployConsumeOptions {}

impl DeployConsumeOptions {
    fn parse(args: impl Iterator<Item = String>) -> Result<Self, String> {
        for arg in args {
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
enum ConsumeVerdict {
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
    /// verb's). The fifth arm, mirroring `runtime::deploy_tracker::decide`.
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

fn consume_verdict(
    request_tip: Option<&str>,
    running_sha: Option<&str>,
    checkout_dirty: bool,
    build_in_flight: bool,
    prior_failures_for_tip: u32,
) -> ConsumeVerdict {
    let Some(tip) = request_tip else {
        return ConsumeVerdict::NothingOwed;
    };
    // ONE sha-equivalence rule for the fleet's deploy owner (the 7-char floor, either
    // spelling as the prefix) — the tracker's, not a second copy of it.
    if running_sha.is_some_and(|running| continuum_core::runtime::deploy_tracker::same_commit(tip, running)) {
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

fn read_consume_failures(path: &Path, tip: &str) -> u32 {
    let Ok(text) = std::fs::read_to_string(path) else { return 0 };
    let Ok(v) = serde_json::from_str::<Value>(&text) else { return 0 };
    if v.get("tip").and_then(|t| t.as_str()) != Some(tip) {
        return 0; // a different tip: the ledger is about a request that no longer stands
    }
    v.get("failures").and_then(|f| f.as_u64()).unwrap_or(0) as u32 // unwrap_or: a malformed ledger counts as no failures, never as give-up
}

fn write_consume_failures(path: &Path, tip: &str, failures: u32) {
    let body = serde_json::json!({ "tip": tip, "failures": failures });
    let _ = std::fs::write(path, body.to_string());
}

fn deploy_request_path() -> Result<PathBuf, String> {
    // The same home the tracker writes under (`resolve_continuum_root`: CONTINUUM_HOME
    // or ~/.continuum) — one resolver, so the consumer reads where the decision wrote.
    Ok(continuum_core::modules::persona_instance_manager::resolve_continuum_root()
        .join("state")
        .join("deploy-request.json"))
}

fn read_deploy_request_tip(path: &Path) -> Option<String> {
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
fn tracked_repo_dir() -> Result<PathBuf, String> {
    use continuum_core::runtime::tracked_checkout::{tracked_checkout, TRACK_REPO_DIR_KEY};
    match tracked_checkout() {
        Ok(Some(dir)) => Ok(dir),
        Ok(None) => Err(format!("deploy-consume: no checkout to deploy from (set {TRACK_REPO_DIR_KEY})")),
        Err(e) => Err(format!("deploy-consume: {e}")),
    }
}

fn git_in(repo: &Path, args: &[&str]) -> Result<String, String> {
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

async fn running_build_sha() -> Option<String> {
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

// =============================================================================
// install — converge this machine to the contract, from the binary (Joel 2026-09-18:
// "What's the repeatable process or inside an install? … Do NOT hand jack"; "it
// needs to be in our own binary here"). Arms land one at a time; each is a module
// under `continuum/`. First: the OS supervisor (`supervisor_install`).
// =============================================================================

async fn install(options: supervisor_install::InstallOptions) -> Result<(), String> {
    #[cfg(any(windows, target_os = "macos"))]
    use supervisor_install::{Arm, ArmReport};
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = options;
        return Err(format!(
            "continuum install: this platform's arms are not in the binary yet — \
             the Linux arm (`systemd --user` + linger) is owed; Windows and macOS are here. \
             Until it lands: tools/scripts/install-service.sh ({} arm).",
            std::env::consts::OS
        ));
    }
    #[cfg(target_os = "macos")]
    {
        if let (true, Some(plan), Some(sha)) = (options.elevated, options.plan.as_deref(), options.plan_sha.as_deref()) {
            for line in launchd::live::register_elevated(plan, sha)? {
                println!("{line}");
            }
            return Ok(());
        }
        let check = options.check;
        let mut reports: Vec<(Arm, ArmReport)> = Vec::new();
        let mut failed: Vec<(Arm, String)> = Vec::new();
        let mut take = |arm: Arm, r: Result<ArmReport, String>| match r {
            Ok(report) => reports.push((arm, report)),
            Err(why) => {
                println!("✗ {}: {why}", arm.name());
                failed.push((arm, why));
            }
        };
        if options.runs(Arm::Supervisor) {
            take(Arm::Supervisor, install_supervisor_macos(check, options.user).await);
        }
        if options.runs(Arm::Core) {
            take(Arm::Core, install_core(check).await);
        }
        if options.runs(Arm::Cli) {
            // The macOS CLI arm (PATH copies follow the slot's CLI) is owed: on this OS the
            // slot carries no CLI descriptor yet. Said, never counted as converged.
            if options.cli {
                return Err("install --cli: the macOS CLI arm is not in the binary yet (the slot carries no CLI descriptor on this OS)".to_string());
            }
            println!("  cli: the macOS arm is owed — skipped, not counted");
        }
        return finish_install(check, &reports, &failed);
    }
    #[cfg(windows)]
    {
        if let (true, Some(plan), Some(sha)) = (options.elevated, options.plan.as_deref(), options.plan_sha.as_deref()) {
            return supervisor_install::install_supervisor_elevated(plan, sha);
        }
        if options.user {
            return Err("install --user is the macOS agent choice; Windows registers the S4U task".to_string());
        }
        let check = options.check;
        // Every arm runs, whatever the one before it did: an arm that cannot do its
        // job is REPORTED as one remaining drift (with its reason) and the next arm
        // still runs. 2026-09-19, Joel's first run: the core arm's error ended the
        // verb before the CLI arm ever ran — the supervisor was converged, the CLI on
        // PATH stayed stale, and the line said "run from the repository".
        let mut reports: Vec<(Arm, ArmReport)> = Vec::new();
        let mut failed: Vec<(Arm, String)> = Vec::new();
        let mut take = |arm: Arm, r: Result<ArmReport, String>| match r {
            Ok(report) => reports.push((arm, report)),
            Err(why) => {
                println!("✗ {}: {why}", arm.name());
                failed.push((arm, why));
            }
        };
        // 1. The supervisor: a core can only be handed to a prepared one.
        if options.runs(Arm::Supervisor) {
            take(Arm::Supervisor, supervisor_install::install_supervisor(check, installed_cli_from_descriptor).await);
        }
        // 2. The core: the running build is the tracked checkout's HEAD, or it is built,
        //    staged into the slot and handed to the supervisor — `reboot --service`, the
        //    same path the unattended consumer takes. Hand-run, the tip is what you have.
        if options.runs(Arm::Core) {
            take(Arm::Core, install_core(check).await);
        }
        // 3. The CLI on PATH follows the slot's CLI (fresh after a stage).
        if options.runs(Arm::Cli) {
            take(Arm::Cli, install_cli(check).await);
        }
        finish_install(check, &reports, &failed)
    }
}

/// The orchestrator's verdict, one for every platform: drift summed across the arms
/// that ran, exit non-zero only on what remains; twice = "nothing changed".
#[cfg(any(windows, target_os = "macos"))]
fn finish_install(
    check: bool,
    reports: &[(supervisor_install::Arm, supervisor_install::ArmReport)],
    failed: &[(supervisor_install::Arm, String)],
) -> Result<(), String> {
    // An arm that could not run is one remaining drift with its reason (#4236) — the
    // verb's exit is the sum over every arm, never the first error.
    let remaining: usize = reports.iter().map(|(_, r)| r.drift_after).sum::<usize>() + failed.len();
    let found: usize = reports.iter().map(|(_, r)| r.drift_before).sum::<usize>() + failed.len();
    let ran = reports.len() + failed.len();
    if remaining > 0 {
        let could_not = if failed.is_empty() {
            String::new()
        } else {
            format!(
                " ({} arm(s) could not run: {})",
                failed.len(),
                failed.iter().map(|(a, _)| a.name()).collect::<Vec<_>>().join(", ")
            )
        };
        return Err(if check {
            format!("install --check: {remaining} way(s) drifted across {ran} arm(s){could_not}; `continuum install` converges them")
        } else {
            format!("install: {remaining} way(s) still drifted after running {ran} arm(s){could_not} — read the lines above")
        });
    }
    println!(
        "✓ install: {ran} arm(s) converged{}",
        if found == 0 { " — nothing changed" } else { "" }
    );
    Ok(())
}

/// The installed CLI, read out of the ContinuumCore task's description (the release
/// descriptor) — the slot's single source.
#[cfg(windows)]
fn installed_cli_from_descriptor(description: &str) -> Result<String, String> {
    let d: CoreServiceDescription = serde_json::from_str(description).map_err(|e| {
        format!("ContinuumCore is not prepared by the current installer (its description is not a release descriptor): {e}; rerun the installer")
    })?;
    Ok(d.cli)
}

/// The core arm: running build vs the TRACKED checkout's HEAD. The checkout is the
/// one the consumer deploys from (`CONTINUUM_TRACK_REPO_DIR`, else the source tree
/// this binary was built from) — never the current directory: a user types
/// `uu install` from wherever they are (Joel, from his home dir, 2026-09-19:
/// "no checkout HEAD to converge to — run from the repository" was the wrong answer).
#[cfg(any(windows, target_os = "macos"))]
async fn install_core(check: bool) -> Result<supervisor_install::ArmReport, String> {
    use supervisor_install::ArmReport;
    let repo = tracked_repo_dir().map_err(|e| format!("{e} — `uu install` converges the tracked checkout, from any directory"))?;
    let head = git_in(&repo, &["rev-parse", "--short", "HEAD"])?;
    println!("  core: checkout {} at {head}", repo.display());
    // RECORD the checkout as the fact every owner reads (card 790c6bcb): the running core
    // built from this install must track this clone, not the dir it was compiled in. Read
    // mode records nothing; a converged write records the same value it already reads.
    if !check {
        let recorded = continuum_core::config_env::read(continuum_core::runtime::tracked_checkout::TRACK_REPO_DIR_KEY);
        if recorded.as_deref() != Some(&*repo.to_string_lossy()) {
            continuum_core::runtime::tracked_checkout::record(&repo)?;
            println!("  core: recorded {} as the tracked checkout (config.env)", repo.display());
        }
    }
    // The handoff path (`reboot --service`) locates its script and registers the
    // checkout from the working directory, as the consumer does before it.
    std::env::set_current_dir(&repo).map_err(|e| format!("install: cannot enter {}: {e}", repo.display()))?;
    let running = running_build_sha().await;
    match running.as_deref() {
        Some(r) if continuum_core::runtime::deploy_tracker::same_commit(r, &head) => {
            println!("✓ core: converged — running build {r} is HEAD");
            return Ok(ArmReport::converged());
        }
        Some(r) => println!("  core: running build {r}, HEAD is {head}"),
        None => println!("  core: no core answering; HEAD is {head}"),
    }
    if check {
        println!("✗ core: drifted; `continuum install` builds HEAD, stages it and hands it to the supervisor");
        return Ok(ArmReport::read_only(1));
    }
    // Windows hands the built core to the prepared task. macOS `reboot` implies the
    // supervisor when a launchd job exists (stage + kickstart) and builds direct when
    // none does — the same idempotent answer either way.
    reboot(RebootOptions { service: cfg!(windows), ..Default::default() }).await?;
    let now = running_build_sha().await;
    match now.as_deref() {
        Some(r) if continuum_core::runtime::deploy_tracker::same_commit(r, &head) => {
            println!("✓ core: converged — running build {r} is HEAD");
            Ok(ArmReport { drift_before: 1, drift_after: 0 })
        }
        other => Err(format!(
            "install: the handoff ran but the running core reports {} against HEAD {head}",
            other.unwrap_or("nothing") // unwrap_or: None = no core answering, reported as such — never a sha
        )),
    }
}

/// The CLI arm: `~/.local/bin/{continuum,uu}.exe` are the slot's CLI, and the dir is
/// on the user's PATH.
#[cfg(windows)]
async fn install_cli(check: bool) -> Result<supervisor_install::ArmReport, String> {
    use install_cli::CliDrift;
    use supervisor_install::ArmReport;
    let core = supervisor_install::task_report(supervisor_install::CORE_TASK).await?;
    if !core.present {
        println!("  cli: no installed release (no ContinuumCore task) — nothing to follow yet");
        return Ok(ArmReport::read_only(1));
    }
    let slot_cli = PathBuf::from(installed_cli_from_descriptor(&core.description)?);
    let dir = install_cli::cli_dir(Path::new(&home_dir()?));
    let user_path = supervisor_install::powershell(
        "[Environment]::GetEnvironmentVariable('PATH','User')",
        Duration::from_secs(30),
    )
    .await?;
    let drift = install_cli::cli_drift(&slot_cli, &dir, &user_path)?;
    if drift.is_empty() {
        println!("✓ cli: converged — {} and uu on PATH are the slot's CLI ({})", dir.join("continuum.exe").display(), slot_cli.display());
        return Ok(ArmReport::converged());
    }
    for d in &drift {
        println!("  cli: {d:?}");
    }
    if check {
        println!("✗ cli: drifted; `continuum install` refreshes the copies and the user PATH");
        return Ok(ArmReport::read_only(drift.len()));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("install: cannot create {}: {e}", dir.display()))?;
    for d in &drift {
        match d {
            CliDrift::Missing(name) | CliDrift::Stale(name) => {
                let to = dir.join(install_cli::cli_file_name(name));
                install_cli::copy_with_retry(&slot_cli, &to, Duration::from_secs(10))?;
                println!("  cli: refreshed {}", to.display());
            }
            CliDrift::NotOnPath(dir) => {
                let script = format!(
                    "$d='{}'; $p=[Environment]::GetEnvironmentVariable('PATH','User'); if (-not $p) {{ $p='' }}; [Environment]::SetEnvironmentVariable('PATH', ($d + ';' + $p).TrimEnd(';'), 'User')",
                    dir.display().to_string().replace('\'', "''")
                );
                supervisor_install::powershell(&script, Duration::from_secs(30)).await?;
                println!("  cli: added {} to the user PATH (new terminals see it)", dir.display());
            }
        }
    }
    let after = install_cli::cli_drift(
        &slot_cli,
        &dir,
        &supervisor_install::powershell("[Environment]::GetEnvironmentVariable('PATH','User')", Duration::from_secs(30)).await?,
    )?;
    if !after.is_empty() {
        return Err(format!("install: the CLI still drifts after the refresh: {after:?}"));
    }
    println!("✓ cli: converged — continuum and uu on PATH are the slot's CLI");
    Ok(ArmReport { drift_before: drift.len(), drift_after: 0 })
}

/// `continuum supervisor-status [--crash-test]` — the supervision receipt as a verb
/// (a1bd8b58; Joel 2026-09-19: "careful methodical TDD and vdd" — every change ships its
/// receipt as a self-running check, not a hand test). Reads the launchd job, its pid, the
/// core's pid and what launchd's log said about the domain, and prints ONE of the four
/// verdicts in [`launchd::SupervisionVerdict`]. Exit 0 only for SUPERVISED.
///
/// `--crash-test` is the receipt itself: `kill -9` the core and time launchd's relaunch,
/// refusing success unless the core that answers is the job's own pid. It is refused when
/// no job is registered — killing an unsupervised core is not a test, it is an outage.
async fn supervisor_status(crash_test: bool) -> Result<(), String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = crash_test;
        Err("supervisor-status reads launchd; on Windows the ContinuumCore task is the supervisor (card 7b56a84b)".to_string())
    }
    #[cfg(target_os = "macos")]
    {
        use launchd::{live, supervision_verdict};
        let socket = socket_path();
        let core_pid = || live::serving_core_pid(&socket);
        let job = live::job()?;
        let job_pid = job.as_ref().and_then(|j| live::job_pid(&j.domain));
        let on_demand = live::domain_on_demand_only_recently(Duration::from_secs(15 * 60));
        let verdict: launchd::SupervisionVerdict = supervision_verdict(job.as_ref().map(|j| j.domain.clone()), job_pid, core_pid(), on_demand);
        eprintln!("{}", verdict.line());
        println!(
            "{}",
            serde_json::json!({
                "domain": job.as_ref().map(|j| j.domain.target()),
                "slot": job.as_ref().map(|j| j.slot.display().to_string()),
                "job_pid": job_pid,
                "core_pid": core_pid(),
                "domain_on_demand_only_last_15m": on_demand,
                "verdict": format!("{verdict:?}"),
                "healthy": verdict.is_healthy(),
            })
        );
        if !crash_test {
            return if verdict.is_healthy() { Ok(()) } else { Err(verdict.line()) };
        }
        let Some(job) = job else {
            return Err("--crash-test refused: no launchd job is registered — killing an unsupervised core is an outage, not a test".to_string());
        };
        let Some(victim) = core_pid() else {
            return Err("--crash-test refused: no core is answering to crash".to_string());
        };
        if job_pid != Some(victim) {
            return Err(format!(
                "--crash-test refused: launchd's pid {job_pid:?} is not the core {victim}; the core is an orphan and would not come back"
            ));
        }
        eprintln!("▶ crash test: kill -9 {victim}; waiting for {} to relaunch it", job.domain.target());
        // SAFETY: a plain signal to a pid this process just read as the supervised core.
        let rc = unsafe { libc::kill(victim as i32, libc::SIGKILL) };
        if rc != 0 {
            return Err(format!("kill -9 {victim} failed: {}", std::io::Error::last_os_error()));
        }
        let t0 = std::time::Instant::now();
        let fresh = move || core_pid().filter(|p| *p != victim);
        match live::wait_owned(&job, fresh, core_is_up, Duration::from_secs(120)).await {
            Ok(pid) => {
                let secs = t0.elapsed().as_secs();
                eprintln!("✅ healed: {} relaunched the core (pid {pid}) in {secs}s", job.domain.target());
                println!("{}", serde_json::json!({ "crash_test": "healed", "seconds": secs, "pid": pid }));
                if secs > 60 {
                    return Err(format!("healed, but in {secs}s — the receipt is 60 s"));
                }
                Ok(())
            }
            Err(why) => {
                let again = live::domain_on_demand_only_recently(Duration::from_secs(5 * 60));
                Err(format!(
                    "NOT HEALED after {}s: {why}{}",
                    t0.elapsed().as_secs(),
                    if again { " — launchd: \"pending spawn, domain in on-demand-only mode\"; install the system LaunchDaemon" } else { "" }
                ))
            }
        }
    }
}

/// The macOS supervisor arm of `continuum install` (card a1bd8b58): READ the registered
/// supervision against the contract ([`launchd::mac_drift`]), and on drift — unless
/// `--check` — WRITE: stop any core on the socket through the save rail, register with
/// the binary as the command (the system LaunchDaemon by default, one sudo, the draft
/// bound to the consent by its digest; `--user` = the agent), hand it the launch, and
/// refuse a core launchd does not own. Then read again: the report is what remains.
#[cfg(target_os = "macos")]
async fn install_supervisor_macos(check: bool, user: bool) -> Result<supervisor_install::ArmReport, String> {
    use launchd::{live, mac_drift, Domain, MacDrift};
    use supervisor_install::ArmReport;
    let socket = socket_path();
    let want = if user {
        // SAFETY: getuid has no preconditions and cannot fail.
        Domain::Gui(unsafe { libc::getuid() })
    } else {
        Domain::System
    };
    let home = home_dir()?;
    let read = || -> Result<Vec<MacDrift>, String> {
        let job = live::job()?;
        let plist = match &job {
            Some(j) => {
                let path = j.domain.plist_path(Path::new(&home));
                Some(std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?)
            }
            None => None,
        };
        let job_pid = job.as_ref().and_then(|j| live::job_pid(&j.domain));
        let on_demand = live::domain_on_demand_only_recently(Duration::from_secs(15 * 60));
        Ok(mac_drift(
            job.as_ref().zip(plist.as_deref()).map(|(j, p)| (&j.domain, p)),
            &want,
            job_pid,
            live::serving_core_pid(&socket),
            on_demand,
        ))
    };
    let before = read()?;
    if before.is_empty() {
        println!("✓ supervisor: converged — {} owns the core, binary as the command, lanes survive a kickstart", want.target());
        return Ok(ArmReport::converged());
    }
    for d in &before {
        println!("  supervisor: {d:?}");
    }
    if check {
        println!("✗ supervisor: drifted; `continuum install` registers {} and hands it the launch", want.target());
        return Ok(ArmReport::read_only(before.len()));
    }

    let artifact = resolve_core_artifact()?;
    // What launchd must carry for the exec to find its libraries: ORT and the runtime
    // library dirs, read off a Command so it is the direct launch's computation. NOT
    // config.env — the core applies that file to itself on every boot
    // (`config_env::apply_to_process`); frozen into the plist it would outlive an edit
    // until the next `install` (Fable, #4228 review).
    let mut probe = direct_core_command(&artifact, &socket);
    apply_runtime_library_path(&mut probe);
    let mut env: Vec<(String, String)> = probe
        .get_envs()
        .filter_map(|(k, v)| Some((k.to_str()?.to_string(), v?.to_str()?.to_string())))
        .collect();
    // launchd's PATH is the system's; the core finds airc and the engines on the
    // operator's. Captured at install, visible in the plist, no login shell needed.
    if let Ok(path) = std::env::var("PATH") {
        env.push(("PATH".to_string(), path));
    }
    // Whatever core holds the socket — an orphan, or the previous registration's own pid
    // — goes down through the save rail before the job is booted out, which would end it
    // without one. Like `reboot`, an unsaved module is printed, not fatal: the goal is a
    // supervised core, and refusing would leave the node down.
    if let Some(pid) = live::serving_core_pid(&socket) {
        let owned = live::job()?.and_then(|j| live::job_pid(&j.domain)) == Some(pid);
        println!(
            "▶ a core (pid {pid}) is serving {socket}{} — stopping it through the save rail before re-registering",
            if owned { " under the existing launchd job" } else { " outside launchd" }
        );
        let _ = stop_with(true).await?;
    }
    let job = live::install(want.clone(), &artifact, &socket, &env)?;
    // `resolve_core_artifact` prefers the installed slot over a fresh build, so on a node
    // that already has one this REGISTERS what is in the slot; the core arm (or `reboot`)
    // is what brings HEAD to the slot.
    if artifact.canonicalize().ok() == job.slot.canonicalize().ok() {
        println!("  artifact: the slot's own binary (the core arm brings HEAD to the slot)");
    } else {
        println!("  artifact: staged {} into the slot", artifact.display());
    }
    println!("✓ registered {} → {} {socket}", job.domain.target(), job.slot.display());
    // The system daemon was started by its bootstrap (RunAtLoad) inside the elevated half
    // — a kickstart there needs root and is not owed. The agent is kickstarted: on a gui
    // domain in on-demand-only mode bootstrap does NOT start it (measured).
    if matches!(job.domain, Domain::Gui(_)) {
        live::kickstart(&job.domain)?;
    }
    let core_pid = || live::serving_core_pid(&socket);
    match live::wait_owned(&job, core_pid, core_is_up, Duration::from_secs(5 * 60)).await {
        Ok(pid) => println!("✓ {} owns the core (pid {pid}); `continuum supervisor-status --crash-test` proves the heal", job.domain.target()),
        Err(why) => {
            let on_demand = live::domain_on_demand_only_recently(Duration::from_secs(3 * 60));
            return Err(format!(
                "registered but NOT supervised: {why}{}",
                if on_demand {
                    " — launchd: \"pending spawn, domain in on-demand-only mode\": the user agent cannot run on this Mac; run `continuum install` (system, sudo once)"
                } else {
                    ""
                }
            ));
        }
    }
    let after = read()?;
    for d in &after {
        println!("  supervisor: still {d:?}");
    }
    Ok(ArmReport { drift_before: before.len(), drift_after: after.len() })
}

/// `continuum uninstall`: unregister the job from launchd (both domains). The staged
/// binary and `~/.continuum` stay; a running core is stopped by the bootout.
async fn uninstall() -> Result<(), String> {
    #[cfg(not(target_os = "macos"))]
    {
        Err("continuum uninstall: the macOS arm is here; Windows unregisters through register-core-service.ps1 until its arm lands in this verb".to_string())
    }
    #[cfg(target_os = "macos")]
    {
        let removed = launchd::live::uninstall()?;
        if removed.is_empty() {
            println!("no launchd job registered for {}", launchd::LABEL);
        }
        for domain in removed {
            println!("✓ removed {}", domain.target());
        }
        Ok(())
    }
}

/// The consumer's log — the only place its output can land, since the scheduled task
/// has no stdout. Set once by `deploy-consume`; read by the warm build to redirect
/// the build's own output there too.
static DEPLOY_LOG: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// The parent's OWN notes: `.append(true)` is the atomic append (FILE_APPEND_DATA
/// positions every write at the end, whoever else is writing); a Rust `writeln!` is
/// fine with it. Only a CHILD needs `open_log_for_child` (Fable, #4233).
fn deploy_log_file() -> Option<std::fs::File> {
    let path = DEPLOY_LOG.get()?;
    std::fs::OpenOptions::new().create(true).append(true).open(path).ok()
}

/// A log handle a CHILD can be handed as its stdout/stderr. NOT `.append(true)`:
/// on Windows that opens the handle with FILE_APPEND_DATA and no FILE_WRITE_DATA,
/// and an MSYS bash handed such a handle as its stdout exits 1 before running a
/// line — every unattended deploy on the 5090 died in one second with an EMPTY
/// log, "warm build exited exit code: 1" and nothing else, for a day (2026-09-19).
/// Full write access, positioned at the end: the same append, in a handle every
/// child accepts.
fn open_log_for_child(path: &Path) -> std::io::Result<std::fs::File> {
    use std::io::Seek;
    let mut file = std::fs::OpenOptions::new().create(true).write(true).open(path)?;
    file.seek(std::io::SeekFrom::End(0))?;
    Ok(file)
}

/// Say it on stdout AND in the log, stamped.
fn deploy_note(line: &str) {
    println!("{line}");
    if let Some(mut f) = deploy_log_file() {
        use std::io::Write;
        let stamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
        let _ = writeln!(f, "{stamp} {line}");
    }
}

/// Whether the consumer's reboot goes through `--service`: only Windows needs the flag
/// (the supervisor handoff); macOS promotes a bare reboot to its launchd job itself, and
/// `reboot` refuses `--service` everywhere else. Pure over the OS name.
fn consumer_uses_service(os: &str) -> bool {
    os == "windows"
}

async fn deploy_consume(options: DeployConsumeOptions) -> Result<(), String> {
    let DeployConsumeOptions {} = options;
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
    let dirty = !git_in(&repo, &["status", "--porcelain", "--untracked-files=no"])?.is_empty();
    // The deploy claim is the tracker's own input (`deploy_claim::in_flight`): a live
    // owner under the ceiling blocks; an abandoned claim is swept by `reboot` itself.
    let build_in_flight = continuum_root()
        .map(|root| continuum_core::runtime::deploy_claim::in_flight(&root, now_ms()).blocks())
        .unwrap_or(false); // unwrap_or: no root = no claim file = nothing in flight
    let attempts_path = consume_attempts_path()?;
    let prior_failures = tip
        .as_deref()
        .map(|t| read_consume_failures(&attempts_path, t))
        .unwrap_or(0); // unwrap_or: no request = nothing to have failed
    let verdict = consume_verdict(
        tip.as_deref(),
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
        ConsumeVerdict::NothingOwed | ConsumeVerdict::AlreadyRunning | ConsumeVerdict::BuildInFlight => Ok(()),
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
            let attempt = async {
                git_in(&repo, &["fetch", "--quiet", "origin"])?;
                git_in(&repo, &["checkout", "--quiet", "--detach", &tip])?;
                // The warm build locates tools/scripts/start-server.sh by walking UP FROM
                // THE CWD, and a scheduled task starts in System32 — the same wall the
                // Macs' launchd tracker hit ("under launchd the cwd is /; continuum reboot
                // then finds no source"). The consumer knows the repo; it stands in it.
                std::env::set_current_dir(&repo)
                    .map_err(|e| format!("deploy-consume: cannot enter {}: {e}", repo.display()))?;
                let service = consumer_uses_service(std::env::consts::OS);
                deploy_note(&format!(
                    "▶ deploy-consume: {} at {tip} — reboot{}",
                    repo.display(),
                    if service { " --service" } else { "" }
                ));
                reboot(RebootOptions { service, ..Default::default() }).await
            }
            .await;
            match &attempt {
                Ok(()) => {
                    let _ = std::fs::remove_file(&attempts_path);
                    deploy_note(&format!("✓ deploy-consume: {tip} handed off"));
                }
                Err(why) => {
                    write_consume_failures(&attempts_path, &tip, prior_failures + 1);
                    deploy_note(&format!(
                        "deploy-consume: attempt {} of {CONSUME_MAX_ATTEMPTS} for {tip} failed: {why}",
                        prior_failures + 1
                    ));
                }
            }
            attempt
        }
    }
}

fn git_head_short_sha() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Unlike the legacy best-effort HEAD lookup, a prebuilt deploy may self-anchor
/// only outside a repository. A broken/inaccessible checkout is not an absence.
async fn prebuilt_checkout_sha(
    cwd: &Path,
    git_dir: Option<&std::ffi::OsStr>,
) -> Result<Option<String>, String> {
    let cwd = cwd
        .canonicalize()
        .map_err(|e| format!("cannot inspect prebuilt checkout {}: {e}", cwd.display()))?;
    if git_dir.is_none() && !has_git_checkout(&cwd)? {
        return Ok(None);
    }

    let mut cmd = std::process::Command::new("git");
    cmd.current_dir(&cwd)
        .args(["rev-parse", "--short", "HEAD"])
        .env_remove("GIT_DIR")
        .stdin(Stdio::null());
    if let Some(git_dir) = git_dir {
        cmd.env("GIT_DIR", git_dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut cmd = tokio::process::Command::from(cmd);
    cmd.kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(30), cmd.output())
        .await
        .map_err(|_| {
            format!(
                "prebuilt checkout HEAD lookup timed out in {}",
                cwd.display()
            )
        })?
        .map_err(|e| {
            format!(
                "cannot read prebuilt checkout HEAD in {}: {e}",
                cwd.display()
            )
        })?;
    if !output.status.success() {
        return Err(format!(
            "cannot verify prebuilt checkout HEAD in {}: git exited {}: {}",
            cwd.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim(),
        ));
    }
    let head = String::from_utf8(output.stdout)
        .map_err(|e| format!("prebuilt checkout HEAD is not UTF-8: {e}"))?;
    let head = head.trim();
    if head.is_empty() {
        return Err("prebuilt checkout HEAD lookup returned no SHA".into());
    }
    Ok(Some(head.to_owned()))
}

/// Detect repository metadata without asking Git to read HEAD. Both a normal
/// `.git` directory and a linked worktree/submodule's `.git` file count, even if
/// broken; Git must then verify the selected checkout. Bare repositories count
/// too. Metadata permission errors must not downgrade verification to standalone.
fn has_git_checkout(cwd: &Path) -> Result<bool, String> {
    fn present(path: &Path) -> Result<bool, String> {
        match std::fs::symlink_metadata(path) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(format!(
                "cannot inspect Git metadata {}: {e}",
                path.display()
            )),
        }
    }
    for ancestor in cwd.ancestors() {
        if present(&ancestor.join(".git"))?
            || (present(&ancestor.join("HEAD"))? && present(&ancestor.join("objects"))?)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Path to the executable image of the ACTUALLY-RUNNING core, resolved from its live pid.
/// DIAGNOSTICS ONLY (error-message context via [`describe_running_core`]): the deploy receipt
/// itself never re-execs this path — after a rebuild the on-disk file at the running pid's
/// path is already the NEW binary while the old image keeps serving, so exec'ing it
/// false-passes a stale deploy (the inverse of the 2026-07-25 release-leftover false alarm).
/// The running core's SHA always comes from the process itself over the socket (`ping`).
fn running_core_binary() -> Option<std::path::PathBuf> {
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
fn running_trainer_pids() -> Vec<i32> {
    processes_with_cmdline("mlx_lm")
}

/// PIDs whose command line contains `fragment` (the old `pgrep -f` behaviour, cross-platform).
/// Kept separate from [`processes_named`] deliberately: command-line matching is what finds an
/// interpreted process, but it is also what makes `pgrep -f continuum-core-server` match the shell
/// that is merely LAUNCHING the core. Use the name-based one where precision matters.
fn processes_with_cmdline(fragment: &str) -> Vec<i32> {
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
enum WaitVerdict {
    Progressing,
    Stalled,
}

fn wait_verdict(silent_for_secs: u64, stall_limit_secs: u64) -> WaitVerdict {
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
fn cpu_reading_shows_progress(reading: u64, last: u64) -> bool {
    reading != last
}

/// `ps -o cputime=` renders `[[dd-]hh:]mm:ss`. Sum right-to-left so every shape parses without
/// branching on which one it is; a day field arrives glued as `dd-hh`, so split it off.
fn cputime_to_secs(field: &str) -> u64 {
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
fn running_core_pids() -> Vec<i32> {
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
fn socket_from_core_argv(argv: &[String]) -> Option<String> {
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
fn running_core_sockets_for(pids: &[i32]) -> Vec<(i32, String)> {
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
fn bound_elsewhere_hint(bound: &[(i32, String)], socket: &str) -> Option<String> {
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
struct CoreProcessEvidence {
    core_pids: Vec<i32>,
    observed_pids: std::collections::HashSet<i32>,
}

impl CoreProcessEvidence {
    fn from_processes<'a>(
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

    fn ensure_offline(&self, recorded_pid: Option<i32>) -> std::io::Result<()> {
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

fn core_process_evidence() -> std::io::Result<CoreProcessEvidence> {
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

fn process_matches_fragment(name: &std::ffi::OsStr, exe: Option<&Path>, fragment: &str) -> bool {
    name.to_string_lossy().contains(fragment)
        || exe
            .and_then(Path::file_name)
            .is_some_and(|name| name.to_string_lossy().contains(fragment))
}

fn processes_named(fragment: &str) -> Vec<i32> {
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
fn kill_pid_tree(pid: i32) {
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
enum KillStep {
    /// This pid alone — its children are visited by the plan, the protected one spared.
    Process(i32),
    /// This pid and everything under it — no protected lane in this branch.
    Tree(i32),
}

impl KillStep {
    /// The pid this step targets — the subject whose survival decides whether the
    /// kill actually happened. A tree kill still has one root, and that root is the
    /// process whose image pins a staging slot when it refuses to die.
    fn pid(self) -> i32 {
        match self {
            Self::Process(pid) | Self::Tree(pid) => pid,
        }
    }

    #[cfg(any(windows, test))]
    fn command(self) -> std::process::Command {
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
    fn execute(self) {
        match self {
            Self::Tree(pid) => kill_pid_tree(pid),
            Self::Process(pid) => kill_pid_alone(pid),
        }
    }
}

/// Unix: TERM this pid ONLY — never its group — with the same 3 s deadline then KILL as
/// [`kill_pid_tree`]. The step a plan takes on an ancestor of a protected lane.
#[cfg(unix)]
fn kill_pid_alone(pid: i32) {
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
fn kill_plan(
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
fn keep_with_self(keep: &[i32]) -> Vec<i32> {
    let mut with_self = keep.to_vec();
    with_self.push(std::process::id() as i32);
    with_self
}

fn process_parents(sys: &sysinfo::System) -> std::collections::HashMap<i32, i32> {
    sys.processes()
        .values()
        .filter_map(|p| {
            p.parent()
                .map(|par| (p.pid().as_u32() as i32, par.as_u32() as i32))
        })
        .collect()
}

fn kill_pid_trees_preserving(roots: &[i32], keep: &[i32]) {
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
            // A KILL IS PROVEN BY THE PROCESS BEING GONE, NEVER BY THE KILLER RETURNING.
            // This arm was `let _ = step.command().output()` — exit status, stdout and
            // stderr all discarded — so `taskkill` answering "Access is denied" (the
            // target runs under the S4U task principal; the caller does not) was
            // indistinguishable from a successful kill. Downstream then removed the
            // pidfile and excluded the pid from survivors, and the only place the truth
            // surfaced was a 60 s handoff timeout that blamed a slow exit
            // (2026-09-22: Astra's core 25040 outlived two staging attempts this way).
            let target = step.pid();
            let out = step.command().output();
            // The postcondition, observed: still alive means the kill did not happen,
            // whatever the tool said. Report the tool's own words when it spoke.
            if pid_alive(target) {
                let detail = match &out {
                    Ok(o) => format!(
                        "taskkill exited {}: {}",
                        o.status,
                        String::from_utf8_lossy(&o.stderr).trim()
                    ),
                    Err(e) => format!("taskkill could not be run: {e}"),
                };
                eprintln!("▶ KILL REFUSED: pid {target} is STILL ALIVE after the kill — {detail}");
                continuum_core::probe!(
                    class = "deploy.kill.refused",
                    pid = target as u64,
                    detail = %detail,
                    "a kill returned but the process is still running — staging over its image will be denied"
                );
            }
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
fn descends_from(parents: &std::collections::HashMap<i32, i32>, pid: i32, keep: &[i32]) -> bool {
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

fn owned_engine_orphans(keep: &[i32]) -> Vec<(i32, String)> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let owned_root = home.join(".continuum").join("bin");
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
fn pid_alive(pid: i32) -> bool {
    // ONE enumerator, shared with the core's own read of the claim (`deploy_claim::in_flight`).
    continuum_core::runtime::deploy_claim::owner_alive(pid)
}

/// Spawn the pure-Rust start script detached and wait until the core answers
/// `ping`. Shared by `continuum start` (after an up-check) and `continuum reboot` (always).
///
/// `wait_for_death` is the set of core PIDs that must EXIT before we trust the
/// ping. Without it, `continuum reboot` would see the OLD core still answering on the
/// same socket and falsely report "ready" before the swap happened — a fail-loud
/// violation that would also hide a failed rebuild. `continuum start` passes `&[]`.
///
/// Returns the seconds waited. Deliberately prints NO success line — the caller owns the
/// receipt (#194): `start` may celebrate liveness, but `reboot` must verify deploy
/// provenance first and only then print its one checkmark.
/// Resolve the bash that runs the start script.
///
/// `Command::new("bash")` is WRONG on Windows: PATH lookup finds `C:\Windows\System32\bash.exe`,
/// which is the WSL launcher, not a POSIX shell. It hands the script to a Linux distro that may
/// not exist and dies with `execvpe(/bin/bash) failed: No such file or directory`. That is exactly
/// what `continuum start` has been doing here — so the core could never start on Windows, so no
/// governed command was reachable, so every long-running job got hand-rolled instead.
///
/// Order: explicit `CONTINUUM_BASH` override, then the Git-for-Windows locations, then a PATH scan
/// that SKIPS the System32 WSL shim. Fails loud and names the fix rather than falling back to a
/// bash that will not work.
fn locate_bash() -> Result<PathBuf, String> {
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
enum LaunchSource<'a> {
    /// Whatever is already installed is fine — the caller wants a live core.
    Installed,
    /// Build first. The caller is deploying source they just edited.
    FromSource,
    /// A caller-selected artifact, validated before reboot's teardown.
    Prebuilt(&'a PrebuiltCore),
}

/// The resolved launch, given the policy and what actually exists on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LaunchPlan {
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
fn plan_launch(
    policy: LaunchSource<'_>,
    env_from_source: bool,
    have_script: bool,
    have_installed: bool,
) -> LaunchPlan {
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

async fn launch_core(wait_for_death: &[i32], policy: LaunchSource<'_>) -> Result<u64, String> {
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
    let (script, server_bin) = if matches!(policy, LaunchSource::Prebuilt(_)) {
        // Already prepared: an installed override may not redirect this explicit
        // artifact — but the source script, in PREBUILT MODE, is how the artifact
        // gets its launch environment (desktop dist, power assertion, airc daemon,
        // llama-server PATH). Without it the M5 came up dark on 2026-09-16.
        (locate_start_script().ok(), None)
    } else {
        (locate_start_script().ok(), locate_core_server_binary())
    };
    let plan = plan_launch(
        policy,
        env_from_source,
        script.is_some(),
        server_bin.is_some(),
    );

    let mut cmd = match plan {
        LaunchPlan::Prebuilt => {
            let LaunchSource::Prebuilt(candidate) = policy else {
                unreachable!("only a validated prebuilt source selects this plan")
            };
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
    apply_core_runtime_env(&mut cmd);
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
            cmd.env("CONTINUUM_SKIP_SELF_BUILD", "1");
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

/// `continuum stop` — stop the running core (the detached session started by `continuum start`).
/// How long the core gets to drain, save and join before the CLI stops waiting.
///
/// `Runtime::shutdown` runs three 2s-bounded phases per module in parallel, so a healthy
/// stop is ~6s worst case; the extra room is for the response to travel back. A stop that
/// exceeds this is not assumed dead — it is assumed UNKNOWN, and the caller says so.
const GRACEFUL_STOP_BUDGET: std::time::Duration = std::time::Duration::from_secs(20);

/// What the graceful request achieved, if anything.
enum GracefulStop {
    /// The core ran the broadcast and every module's state reached disk.
    Durable(String),
    /// The core ran the broadcast and something did NOT save. The message names it.
    /// The process is still stopping; the operator needs the names, not a retry.
    Incomplete(String),
    /// No answer — a core too wedged to answer, or a response that did not arrive inside
    /// the budget. NOT the same as "it stopped": the caller must still tear the process
    /// down, and must not report a clean stop.
    NoAnswer(String),
    /// The running core PREDATES this rail: it has no `system/shutdown` verb, so it was
    /// never going to save and no fix in this binary can change that.
    ///
    /// Distinct from `NoAnswer` on purpose. Both are non-durable, but they are different
    /// facts and an operator needs to tell them apart: this one is the expected, one-time
    /// cost of the FIRST upgrade — the core being replaced was built before the rail
    /// existed — whereas `NoAnswer` is a core that HAS the capability and would not use
    /// it, which is a fault. Collapsing them would make every first rollout look like a
    /// malfunction, and every malfunction look like a rollout.
    LegacyCore(String),
    /// Nothing was listening in the first place. Distinct from `NoAnswer` on purpose: a
    /// core that never ran lost nothing, so `stop` on an already-stopped node must exit 0
    /// rather than announce a data loss that did not happen. The sweep below still runs —
    /// an unanswering socket is not proof that no process survives.
    NothingRunning,
}

impl GracefulStop {
    /// A legacy core cannot attest a final checkpoint. Keep it running until the
    /// operator can stop it explicitly and preserve/adopt its selected checkpoint.
    fn ensure_teardown_supported(&self, reboot: bool) -> Result<(), String> {
        if let (true, Self::LegacyCore(reason)) = (reboot, self) {
            return Err(format!(
                "reboot refused before teardown: {reason}. The existing core is still running. \
                 Use `continuum stop`, preserve the outgoing checkpoints, and use \
                 `continuum checkpoint inspect` / `continuum checkpoint adopt` where recovery \
                 is needed before running `continuum reboot` again."
            ));
        }
        Ok(())
    }
}

/// Ask the running core to stop ITSELF, so every module's `save_state` runs.
///
/// This is the whole point of the rail. A kill — `taskkill /F` on Windows, a signal-less
/// tree kill elsewhere — runs no module's save, so the citizens' volatile state is lost
/// on every ordinary `stop`, and the CLI reported success because a dead process is
/// indistinguishable from a cleanly stopped one when the only thing you check is whether
/// it is gone.
///
/// The request travels the same socket path `ping` uses, so there is no new transport and
/// no Windows-specific arrangement.
async fn request_graceful_stop() -> GracefulStop {
    // Ask whether anything is listening BEFORE spending the stop budget on a socket
    // nobody holds. Without this, `stop` on an already-stopped node waits the full
    // graceful budget and then reports state loss — 20 seconds to be told, wrongly, that
    // a core which never ran failed to save.
    //
    // BUT A PING TIMEOUT IS NOT PROOF OF ABSENCE. `core_is_up` returns false for a core
    // that is running and too wedged to answer — which is the case where state is most
    // likely to be lost, and reporting it as "nothing was running" would exit 0 and tell
    // the operator nothing was at stake. So absence must be corroborated by the pidfile:
    // no answer AND no pidfile is an empty node; no answer WITH a pidfile is a core that
    // would not speak, and that is `NoAnswer`. (The pidfile can also be stale, which is
    // why this decides the LABEL only — the sweep below runs either way.)
    if !core_is_up().await {
        // A MISSING PIDFILE IS NOT PROOF THAT NOTHING IS RUNNING. It was, in the first
        // version of this, and that is the same error as reading a ping timeout as an
        // empty node — one layer down. A core started outside the pidfile's owner, one
        // whose file was removed, or a second core that never wrote one, all present as
        // "no record" while still holding the socket and a citizen's unsaved state.
        //
        // So absence is only claimed when the sweep can also find no process. That is
        // what `running_core_pids` answers, and it is the same enumeration the sweep
        // below acts on — one instrument, so the label and the action cannot disagree.
        //
        // It can still be wrong in the SAFE direction: `processes_named` returning empty
        // because the enumerator itself failed reads as "nothing running". That is worth
        // naming rather than hiding — it is the residual case, and it is why the teardown
        // below runs regardless of what this decides.
        // THREE OUTCOMES, not two. Each piece of evidence can say "yes", "no", or "I
        // could not look", and only the last of those may not be rounded to "no".
        let pidfile = pidfile_for(&socket_path());
        let recorded = match std::fs::read_to_string(&pidfile) {
            Ok(c) => c.trim().parse::<i32>().is_ok(),
            // The ONLY error that means absence. A permission error, a busy file, a
            // path on a filesystem that went away — those mean we did not find out, and
            // `.ok()` used to flatten every one of them into "no record".
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => {
                return GracefulStop::NoAnswer(format!(
                    "could not read {}: {e} — whether a core is running is unknown, so this                      stop is not being called clean",
                    pidfile
                ));
            }
        };
        // ONE snapshot answers both questions, so they cannot describe different moments.
        let evidence = match core_process_evidence() {
            Ok(evidence) => evidence,
            Err(error) => {
                return GracefulStop::NoAnswer(format!(
                    "could not establish whether a core is running: {error}"
                ));
            }
        };
        let survivors = !evidence.core_pids.is_empty();
        return if recorded || survivors {
            GracefulStop::NoAnswer(
                "a core process is present but did not answer a ping".to_string(),
            )
        } else {
            // Both instruments looked, and both found nothing.
            GracefulStop::NothingRunning
        };
    }
    let conn = connection();
    let cmds = conn.commands();
    let req = cmds.execute_value("system/shutdown", Value::Object(Default::default()));
    match tokio::time::timeout(GRACEFUL_STOP_BUDGET, req).await {
        Ok(Ok(value)) => {
            let durable = value
                .get("state_is_durable")
                .and_then(Value::as_bool)
                // A response whose shape we do not recognise is not a durable stop. An
                // older core answering `system/shutdown` with something else must not be
                // read as a clean save.
                .unwrap_or(false);
            let summary = value
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or("the core answered without a summary")
                .to_string();
            if durable {
                GracefulStop::Durable(summary)
            } else {
                GracefulStop::Incomplete(summary)
            }
        }
        Ok(Err(e)) => {
            // A core that does not KNOW the verb is not a core that refused it. The
            // command surface answers an unknown name with a "no handler"-shaped error,
            // and during the first upgrade that is exactly what the outgoing core says —
            // it was built before this rail existed.
            let unknown = {
                // `e` is a ClientError, not a String — formatted first so the match is on
                // the rendered message the transport actually produced.
                let m = format!("{e}").to_lowercase();
                m.contains("unknown command")
                    || m.contains("no handler")
                    || m.contains("not found")
                    || m.contains("unsupported")
            };
            if unknown {
                GracefulStop::LegacyCore(format!(
                    "the running core has no system/shutdown verb ({e}) — it predates this                      rail, so its modules were never going to save and this stop cannot be                      called durable"
                ))
            } else {
                GracefulStop::NoAnswer(format!("the core refused the request: {e}"))
            }
        }
        Err(_) => GracefulStop::NoAnswer(format!(
            "no answer within {}s",
            GRACEFUL_STOP_BUDGET.as_secs()
        )),
    }
}

async fn stop() -> Result<(), String> {
    // The operator verb answers with its EXIT CODE. `stop` returning 0 has meant only
    // "the process is gone"; it now means "the process is gone AND every module's state
    // reached disk", which is the question anyone typing `stop` before an upgrade is
    // actually asking. A stop that could not save is a failure the shell can see.
    match stop_with(false).await? {
        // Nothing ran, so nothing was lost. Exiting non-zero here would report a data
        // loss that did not happen, and `stop` is used in scripts that would then treat a
        // clean no-op as a failure.
        GracefulStop::Durable(_) | GracefulStop::NothingRunning => Ok(()),
        GracefulStop::Incomplete(summary) => Err(format!(
            "the core stopped but its state is not durable: {summary}"
        )),
        GracefulStop::NoAnswer(why) => Err(format!(
            "the core was forced down without saving ({why}) — volatile state since the last              save-on-write is gone"
        )),
        // Also a failure, and deliberately so: the operator asked for a stop and did not
        // get a durable one. It is EXPECTED once, on the upgrade that installs the rail,
        // and the message says which it is rather than making a rollout look like a fault.
        GracefulStop::LegacyCore(why) => Err(format!(
            "the outgoing core could not stop gracefully ({why}); this is the one-time cost              of installing the shutdown rail, and it is still not a durable stop"
        )),
    }
}

/// The one teardown, parameterized by lane fate. `keep_lanes: true` is the
/// REBOOT path: a healthy llama-server about to be wanted again by the next
/// core stays up, and boot's serve-or-adopt reconcile adopts it at zero
/// relaunches when the shape matches (regression-pinned:
/// a_past_form_of_ourself_serving_enough_lanes_is_adopted_not_reaped) or
/// honestly reaps+rebuilds when it doesn't. Reaping a 20GB-resident lane just
/// to reload it 60s later made every reboot ~5min; with adoption it is the
/// core swap alone (Joel 2026-08-23: "if it's taking so long we need to fix
/// that first"). The standalone `stop` verb keeps FULL teardown — an operator
/// who says stop means everything.
async fn stop_with(keep_lanes: bool) -> Result<GracefulStop, String> {
    // ASK BEFORE KILLING. Everything below this point is a kill, and a kill runs no
    // module's `save_state` — so before it, the core gets the chance to stop itself and
    // report what reached disk. The kill still runs afterwards either way: a core that
    // answered is already exiting and the sweep finds nothing, and a core that did not
    // answer still has to go. What changes is that the operator is told which of those
    // happened instead of reading the same success line for both.
    let graceful = request_graceful_stop().await;
    // `keep_lanes` IS the reboot flag — see this function's doc: "`keep_lanes: true` is the
    // REBOOT path". Named `reboot` on the guard because that is the property it reasons about,
    // and passed `keep_lanes` because today the two callers are exactly reboot(true)/stop(false).
    // A third caller that wants lanes kept for some OTHER reason must pass the reboot-ness
    // separately rather than reuse this argument.
    graceful.ensure_teardown_supported(keep_lanes)?;
    match &graceful {
        GracefulStop::Durable(summary) => println!("core stopped gracefully — {summary}"),
        GracefulStop::Incomplete(summary) => {
            eprintln!("core stopped WITH UNSAVED STATE — {summary}")
        }
        GracefulStop::NoAnswer(why) => {
            eprintln!("no graceful stop ({why}); forcing — modules did not save")
        }
        GracefulStop::NothingRunning => {
            println!("no core is listening; sweeping for survivors")
        }
        GracefulStop::LegacyCore(why) => {
            eprintln!("FIRST UPGRADE — {why}; forcing, and its volatile state is lost")
        }
    }

    let socket = socket_path();
    let pidfile = pidfile_for(&socket);
    // Resolve identity BEFORE any tree is killed. live_lane requires a matching
    // Live registry record and a currently observable llama-server image;
    // unreadable or mismatched records are not exclusions.
    let keep: Vec<i32> = if keep_lanes {
        continuum_core::inference::lane_registry::live_lane()
            .map(|r| r.pid as i32)
            .into_iter()
            .collect()
    } else {
        Vec::new()
    };

    let mut pidfile_core: Option<i32> = None;
    if let Ok(contents) = std::fs::read_to_string(&pidfile) {
        if let Ok(pid) = contents.trim().parse::<i32>() {
            kill_pid_trees_preserving(&[pid], &keep);
            pidfile_core = Some(pid);
            println!("stopping core (pid {pid})");
        }
        let _ = std::fs::remove_file(&pidfile);
    }
    let stopped = pidfile_core.is_some();

    // The pidfile names ONE core. It is not evidence that only one is running.
    //
    // This sweep used to be gated behind `if !stopped` — a pidfile present meant
    // "handled, nothing else to look for". Measured 2026-08-14: two cores were
    // alive at once, both with /tmp/continuum-core.sock open (a debug build at
    // 23:51 and the installed release at 23:54, the second having unlinked and
    // re-bound the path). `stop` printed "stopping core (pid 70240)" — singular —
    // reaped that one, and left the other serving. The `reap_owned_orphans` sweep
    // below could not catch it either: ownership there is keyed on
    // `~/.continuum/bin`, and a core built into the project's mandated
    // CARGO_TARGET_DIR is not under that root, so a dev-built core is invisible to
    // it by construction.
    //
    // A second core on this machine is never benign — whichever one the kernel
    // hands a connection to is the one that answers, so a shipped fix can look
    // intermittently broken and an unshipped one intermittently fixed. `stop`
    // must therefore always enumerate, and must be LOUD about a survivor: an
    // extra core is evidence of a lifecycle bug, exactly as an orphan is.
    //
    // (The enumerator itself was `pkill -f continuum-core-server`, which does not
    // exist on Windows: the spawn failed, `.unwrap_or(false)` swallowed it, and
    // stop printed "no running core found" while the core was running — the same
    // shape as the `pgrep` bug documented on `processes_named` above. That fix
    // ported the finder to sysinfo and missed this sibling call site.)
    // Exclude the pidfile core: SIGTERM is asynchronous, so it is very likely
    // still in the process table on the next line. Counting it here would report
    // a SPLIT BRAIN on every ordinary stop — a false alarm on a message whose
    // whole value is that it only fires when something is genuinely wrong.
    let survivors: Vec<i32> = running_core_pids()
        .into_iter()
        .filter(|pid| Some(*pid) != pidfile_core)
        .filter(|pid| pid_alive(*pid))
        .collect();
    if survivors.is_empty() {
        if !stopped {
            println!("no running core found");
        }
    } else {
        if stopped {
            println!(
                "  SPLIT BRAIN: {} core(s) still running after the pidfile core was stopped \
                 — reaping (pid(s) {})",
                survivors.len(),
                survivors
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
        kill_pid_trees_preserving(&survivors, &keep);
        if !stopped {
            println!(
                "stopped continuum-core-server (pid(s) {})",
                survivors
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
    }

    // Engine children whose parent died without taking them down are invisible
    // to every tree reap above. Sweep them by OWNERSHIP — nothing is left
    // holding a port or VRAM once `stop` returns.
    //
    // EXCEPT the live serving lane on the reboot path. The comment that used
    // to say lanes were invisible to this sweep rotted when llama-server moved
    // under ~/.continuum/bin: measured 2026-09-01/02, EVERY reboot printed
    // "reaping orphaned llama-server — parent gone" here and then "leaving
    // serving lane(s) up for adoption" eleven lines later, over the corpse —
    // a ~15-minute model reload per reboot, the single biggest boot tax. The
    // identity-verified live lane (pidfile + is_llama_server, never a reused
    // pid) is SPARED when keep_lanes; the start rail's adopt_or_reap then
    // health-checks and adopts the warm weights.
    reap_owned_orphans(&keep);

    // The live serving lane may be a core child on Windows or already orphaned,
    // and its executable may live under `~/.continuum/bin`. Both the core-tree
    // reap and ownership sweep above must honor the same exclusion. Until this call
    // existed, `stop` left every `llama-server` running and the registry was
    // swept only on the NEXT boot — measured 2026-08-17 on the M5 as two lanes
    // resident at once (a 19 GB ephemeral 27B beside the live 14B), which
    // starved the planner into serving a 2,816-token window that cannot hold the
    // tool surface. `reboot` could not clear it either: reboot is stop + start,
    // and neither half owned lanes.
    if keep_lanes {
        println!("  leaving serving lane(s) up for adoption by the next core (reboot path)");
        let _ = std::fs::remove_file(&socket); // socket cleanup still ours — only the lane fate changed
        return Ok(graceful);
    }
    for outcome in continuum_core::inference::lane_registry::sweep_all() {
        use continuum_core::inference::lane_registry::SweepOutcome as S;
        match outcome {
            S::ReapedLive { pid, port } => {
                println!("  reaping serving lane (pid {pid}, port {port}) — live lane, this core is stopping")
            }
            S::ReapedEphemeral { pid, port } => {
                println!(
                    "  reaping serving lane (pid {pid}, port {port}) — ephemeral lane, owner gone"
                )
            }
            // A record whose pid is dead / recycled / unparseable is bookkeeping,
            // not an event: garbage-collected silently so the loud lines above
            // stay meaningful.
            S::RemovedDead { .. } | S::RemovedReused { .. } | S::RemovedUnparseable { .. } => {}
            // Unreachable under Shutdown (every role is reaped) — but matched
            // explicitly so adding a mode can never silently fall through here.
            S::LeftLive { pid } => {
                println!("  WARNING: serving lane (pid {pid}) left running by a shutdown sweep — report this")
            }
        }
    }

    let _ = std::fs::remove_file(&socket);
    Ok(graceful)
}

/// Kill every owned engine process not descended from `keep`, reporting each
/// by name and pid. Silent when there are none, loud when there are: an orphan
/// is evidence of a lifecycle bug, so it must never be reaped quietly.
fn reap_owned_orphans(keep: &[i32]) {
    let orphans = owned_engine_orphans(keep);
    if orphans.is_empty() {
        return;
    }
    for (pid, what) in &orphans {
        println!("  reaping orphaned {what} (pid {pid}) — owned by this install, parent gone");
    }
    let roots: Vec<i32> = orphans.iter().map(|(pid, _)| *pid).collect();
    kill_pid_trees_preserving(&roots, keep);
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
fn locate_core_server_binary() -> Option<PathBuf> {
    const BIN: &str = if cfg!(windows) {
        "continuum-core-server.exe"
    } else {
        "continuum-core-server"
    };

    if let Ok(explicit) = std::env::var("CONTINUUM_CORE_SERVER") {
        let p = PathBuf::from(&explicit);
        if p.is_file() {
            return Some(p);
        }
        eprintln!(
            "continuum: CONTINUUM_CORE_SERVER={explicit} is not a file — ignoring the \
             override and searching normally"
        );
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(BIN);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    if let Ok(home) = home_dir().map(PathBuf::from) {
        let candidate = home.join(".continuum").join("bin").join(BIN);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    let mut dir = std::env::current_dir().ok()?;
    loop {
        for profile in ["release", "debug"] {
            let candidate = dir.join("target").join(profile).join(BIN);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn locate_start_script() -> Result<PathBuf, String> {
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

/// The desktop display manager's port — `CONTINUUM_UI_PORT`, else the
/// documented default beside WS 8974 (http::desktop). ONE place; the
/// `desktop` verb and the start/reboot receipt both read it.
fn desktop_port() -> u16 {
    std::env::var("CONTINUUM_UI_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(8975) // unwrap_or: the display manager's documented default
}

fn desktop_url() -> String {
    format!("http://127.0.0.1:{}/", desktop_port())
}

/// Bounded (1 s) "is the greeter answering" probe — on a deploy path, so it
/// has a bound and a named outcome, never a hang.
async fn desktop_answering() -> bool {
    matches!(
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            tokio::net::TcpStream::connect(("127.0.0.1", desktop_port())),
        )
        .await,
        Ok(Ok(_))
    )
}

/// The line a verified start/reboot ends with: WHERE the desktop is. A user
/// must never have to know a port (Joel, 2026-09-05: "remembering port is
/// bush league") — the CLI says the address, and `uu desktop` opens it.
async fn desktop_receipt_line() -> String {
    if desktop_answering().await {
        format!("🖥  desktop: {}   (`uu desktop` opens it)", desktop_url())
    } else {
        format!(
            "🖥  desktop: not serving yet on :{} — the web build lands in the background; \
             `uu desktop` opens it once it does",
            desktop_port()
        )
    }
}

fn usage() -> String {
    "usage: continuum <start|reboot|stop|desktop|command> [json | --key value ...]  (uu = continuum)\n\
     \n\
     Lifecycle:\n  \
       continuum start                 build + run the headless Rust core (detached), wait until ready;\n                                       refuses if a core is running but not answering (a second core on\n                                       one socket makes results non-deterministic)\n  \
       continuum start --force         reclaim those unresponsive core(s) first, then start\n  \
       continuum reboot                rebuild + relaunch; verifies the RUNNING core's build SHA\n  \
       continuum reboot --prebuilt <path> [--service | --validate-only]\n                                       validate and launch that core without rebuilding; retains cwd\n                                       and matches checkout HEAD when run in a repository\n                                       Windows --service uses the installer's prepared task;\n                                       --validate-only checks without stopping or launching\n  \
       continuum stop                  stop the running core\n  \
       continuum deploy-verify         prove the running core's build SHA matches the deployed source\n  \
       continuum install [--check]     converge this machine: the OS supervisor (Windows: S4U at boot +\n                                       the deploy consumer, one elevation; macOS: the system LaunchDaemon,\n                                       sudo once, --user = the agent), the core (build HEAD, stage, hand\n                                       off when the running build is not HEAD), the CLI on PATH\n                                       (continuum + uu follow the slot). Each arm reads, changes only\n                                       what drifted, says so. --check reads only. Name arms with\n                                       --supervisor --core --cli. (linux arms pending)\n  \
       continuum uninstall             unregister the supervisor job (the staged binary stays)\n  \
       continuum supervisor-status [--crash-test]\n                                       who owns the running core; --crash-test = kill -9, expect a heal < 60 s\n\
     \n\
     Legacy checkpoint recovery (local; no running core required):\n  \
       continuum checkpoint inspect --source <volatile.json> --persona-id <uuid> --plan <new-file>\n                                       save an explicit digest-bound selection; no checkpoint changed\n  \
       continuum checkpoint adopt --plan <file> --legacy-writers-stopped\n                                       preserve both snapshots and adopt the selected bytes offline;\n                                       stop legacy cores and automatic launchers first; no final-flush claim\n\
     \n\
     Desktop (the core serves it; no port to remember):\n  \
       continuum desktop               open the desktop in your browser (alias: uu desktop)\n\
     \n\
     Commands (dispatch to the running core):\n  \
       continuum ping\n  \
       continuum ping --message hi                 # --key value, coerced + camelCased automatically\n  \
       continuum ping '{\"message\":\"hi\"}'           # or a single JSON object (AI / power-user path)\n  \
       continuum commands/list                     # discover commands dynamically (single source)\n  \
       continuum commands/list --filter data/\n\
     \n\
     Env: CONTINUUM_CORE_SOCKET (default /tmp/continuum-core.sock)\n     \
          CONTINUUM_START_SCRIPT (override the start script path)"
        .to_string()
}

#[cfg(test)]
mod tests {
    // what this catches (2026-09-22, Astra's Windows node): a kill step that cannot say
    // WHICH process it was asked to end. The Windows arm discarded taskkill's Output, so
    // an "Access is denied" refusal and a successful kill were the same observation —
    // and the verification that now replaces that silence needs the target pid. A
    // `pid()` that answered for only one variant would verify the wrong process for the
    // other, which is worse than not verifying: it would report a healthy kill while the
    // real target lived on and pinned the staging slot.
    #[test]
    fn a_kill_step_names_the_process_whose_survival_disproves_it() {
        use super::KillStep;
        assert_eq!(KillStep::Process(4321).pid(), 4321);
        assert_eq!(KillStep::Tree(4321).pid(), 4321, "a tree kill is still rooted at one pid");
        // The two shapes differ in BLAST RADIUS, never in subject: whichever the plan
        // picks, the pid whose survival means the kill did not happen is the same.
        assert_eq!(KillStep::Process(7).pid(), KillStep::Tree(7).pid());
    }

    // what this catches (card 82af11f5, 2026-09-19): the Windows deploy consumer's
    // verdict vocabulary — no request = nothing owed; the tip already running (short
    // sha as a prefix, git's 7-char floor) = the tracker retires it, never a reboot;
    // a dirty checkout is REFUSED (a consumer never stashes an operator's tree); only
    // a clean checkout with a different tip deploys. And the option parse: --install
    // and --uninstall are exclusive, nothing else is accepted.
    #[test]
    fn the_deploy_consumer_deploys_only_a_clean_checkout_toward_a_tip_not_running() {
        use super::{consume_verdict, ConsumeVerdict, DeployConsumeOptions};
        let v = |tip: Option<&str>, running: Option<&str>, dirty: bool| consume_verdict(tip, running, dirty, false, 0);
        assert_eq!(v(None, Some("6d8fc04de"), false), ConsumeVerdict::NothingOwed);
        assert_eq!(v(Some("6d8fc04de"), Some("6d8fc04de1234567"), false), ConsumeVerdict::AlreadyRunning);
        assert_eq!(v(Some("6d8fc04de1234567"), Some("6d8fc04de"), false), ConsumeVerdict::AlreadyRunning, "either spelling as the prefix");
        assert_eq!(
            v(Some("6d8fc0"), Some("6d8fc04de"), false),
            ConsumeVerdict::Deploy,
            "under git's 7-char floor a prefix is a coincidence, not a match"
        );
        assert_eq!(v(Some("abc1234"), Some("6d8fc04de"), true), ConsumeVerdict::RefuseDirty);
        assert_eq!(v(Some("abc1234"), Some("6d8fc04de"), false), ConsumeVerdict::Deploy);
        assert_eq!(v(Some("abc1234"), None, false), ConsumeVerdict::Deploy, "no core answering = deploy, the request stands");
        // A live deploy claim = a build in flight from an earlier tick: NEVER a second
        // reboot into it (the 10-min task vs a 50-min build). It outranks dirty and the
        // ledger because nothing about this tick should act at all.
        assert_eq!(consume_verdict(Some("abc1234"), None, false, true, 0), ConsumeVerdict::BuildInFlight);
        assert_eq!(consume_verdict(Some("abc1234"), None, true, true, 9), ConsumeVerdict::BuildInFlight);
        // A tip that would not land here is tried CONSUME_MAX_ATTEMPTS times, then left to
        // deploy.stranded — never a rebuild loop every tick until the claim ages out.
        assert_eq!(consume_verdict(Some("abc1234"), None, false, false, 2), ConsumeVerdict::Deploy);
        assert_eq!(consume_verdict(Some("abc1234"), None, false, false, 3), ConsumeVerdict::GaveUp);
        // The ledger is per tip: a new tip starts at zero.
        let dir = std::env::temp_dir().join(format!("consume-ledger-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap(); // unwrap: test fixture — a temp dir that cannot be made fails the test loudly
        let ledger = dir.join("deploy-consume-attempts.json");
        super::write_consume_failures(&ledger, "abc1234", 3);
        assert_eq!(super::read_consume_failures(&ledger, "abc1234"), 3);
        assert_eq!(super::read_consume_failures(&ledger, "def5678"), 0, "a different tip resets");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(DeployConsumeOptions::parse(["--install".to_string()].into_iter()).is_err(), "the consumer's task has ONE registrar: continuum install --supervisor");
        assert!(DeployConsumeOptions::parse(std::iter::empty()).is_ok());
        assert!(DeployConsumeOptions::parse(["--now".to_string()].into_iter()).is_err());
        // The consumer runs on every platform now (the core's actuator launches it); only
        // Windows needs `--service` — macOS promotes a bare reboot to launchd itself, and
        // `reboot` refuses the flag on Linux.
        assert!(super::consumer_uses_service("windows"));
        assert!(!super::consumer_uses_service("macos"));
        assert!(!super::consumer_uses_service("linux"));
    }

    // Regression for #3929: a first upgrade must leave the legacy core available
    // for explicit checkpoint recovery, while normal reboot outcomes may continue.
    #[test]
    fn legacy_core_requires_explicit_recovery_before_reboot_teardown() {
        use super::GracefulStop;
        let legacy = GracefulStop::LegacyCore("no shutdown handler".into());
        let error = legacy.ensure_teardown_supported(true).unwrap_err();
        assert!(
            legacy.ensure_teardown_supported(false).is_ok(),
            "explicit stop remains available"
        );
        assert!(error.contains("before teardown"));
        assert!(error.contains("checkpoint inspect"));
        assert!(error.contains("checkpoint adopt"));
        for outcome in [
            GracefulStop::NothingRunning,
            GracefulStop::Durable("saved".into()),
            GracefulStop::Incomplete("save failed".into()),
            GracefulStop::NoAnswer("timed out".into()),
        ] {
            assert!(outcome.ensure_teardown_supported(true).is_ok());
        }
    }

    // What this catches (card 9f160b78): missing/truncated process evidence and
    // a live PID-file process must never authorize offline memory replacement.
    #[test]
    fn checkpoint_offline_evidence_keeps_unknown_distinct_from_absent() {
        use super::CoreProcessEvidence;
        use std::ffi::OsStr;
        use std::path::Path;
        let own = (41, OsStr::new("continuum"), None);
        let other = (42, OsStr::new("launcher"), None);
        let evidence = CoreProcessEvidence::from_processes(41, [own, other]).unwrap();
        assert!(evidence.ensure_offline(None).is_ok());
        assert!(evidence.ensure_offline(Some(99)).is_ok());
        assert_eq!(
            evidence.ensure_offline(Some(42)).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        assert!(CoreProcessEvidence::from_processes(41, [other]).is_err());
        let truncated = (43, OsStr::new("continuum-core-"), None);
        assert!(CoreProcessEvidence::from_processes(41, [own, truncated]).is_err());
        let resolved = (
            43,
            OsStr::new("continuum-core-"),
            Some(Path::new("/bin/continuum-core-server")),
        );
        assert!(CoreProcessEvidence::from_processes(41, [own, resolved])
            .unwrap()
            .ensure_offline(None)
            .is_err());
        // The uncertain truncated name must not broaden process termination.
        assert!(!super::process_matches_fragment(
            truncated.1,
            truncated.2,
            "continuum-core-server"
        ));
        assert!(super::process_matches_fragment(
            resolved.1,
            resolved.2,
            "continuum-core-server"
        ));
        assert!(
            !super::process_matches_fragment(
                OsStr::new("python"),
                Some(Path::new("/continuum-core-server-fixtures/python")),
                "continuum-core-server"
            ),
            "parent directories must not turn an unrelated executable into a core kill target"
        );
    }

    // What this catches (card 9f160b78): inspect --plan <absent volatile.json>
    // must not write a valid plan into a Persona's actual memory file.
    #[test]
    fn inspection_plan_cannot_occupy_checkpoint_or_evidence_storage() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let source_dir = root.join("legacy").join(uuid::Uuid::new_v4().to_string());
        let persona_dir = root.join("personas").join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&persona_dir).unwrap();
        let sibling = root.join("legacy").join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&sibling).unwrap();
        let source = source_dir.join("volatile.json");
        let destination = persona_dir.join("volatile.json");
        for invalid in [
            &source,
            &destination,
            &persona_dir.join(".volatile.lock"),
            &source_dir.join("plan.json"),
            &sibling.join("volatile.json"),
        ] {
            assert!(super::checkpoint_plan_output(invalid, &source, &destination).is_err());
            assert!(
                !invalid.exists(),
                "inspection must leave managed state absent"
            );
        }
        let outside = root.join("plan.json");
        assert_eq!(
            super::checkpoint_plan_output(&outside, &source, &destination).unwrap(),
            outside
        );
    }

    // What this catches (card 9f160b78): a malformed recovery invocation must
    // never select/adopt a checkpoint or imply that legacy writers are stopped.
    #[test]
    fn checkpoint_recovery_requires_explicit_selection_and_offline_precondition() {
        let parse =
            |args: &[&str]| super::CheckpointCommand::parse(args.iter().map(|arg| arg.to_string()));
        let persona = "68d231fb-1b99-47ea-8615-14538906817a";
        assert!(matches!(
            parse(&[
                "inspect",
                "--source",
                "old/volatile.json",
                "--persona-id",
                persona,
                "--plan",
                "plan.json"
            ])
            .unwrap(),
            super::CheckpointCommand::Inspect { .. }
        ));
        assert_eq!(
            parse(&["adopt", "--plan", "plan.json", "--legacy-writers-stopped"]).unwrap(),
            super::CheckpointCommand::Adopt {
                plan: "plan.json".into()
            }
        );
        for args in [
            vec![],
            vec!["adopt", "--plan", "plan.json"],
            vec!["adopt", "--plan", "plan.json", "--force"],
            vec![
                "adopt",
                "--plan",
                "plan.json",
                "--plan",
                "other.json",
                "--legacy-writers-stopped",
            ],
            vec![
                "adopt",
                "--plan",
                "plan.json",
                "--legacy-writers-stopped",
                "--legacy-writers-stopped",
            ],
            vec![
                "inspect",
                "--source",
                "old/volatile.json",
                "--plan",
                "plan.json",
            ],
            vec![
                "inspect",
                "--source",
                "old/volatile.json",
                "--persona-id",
                "invalid",
                "--plan",
                "plan.json",
            ],
            vec![
                "inspect",
                "--source",
                "--persona-id",
                persona,
                "--plan",
                "plan.json",
            ],
        ] {
            assert!(parse(&args).is_err(), "must refuse {args:?}");
        }
    }

    // what this catches: card 67f53b63 — a missing/misspelled prebuilt path
    // must not fall through to a source reboot, and --force only changes leases.
    #[test]
    fn prebuilt_reboot_options_are_explicit_and_strict() {
        let parse = |args: &[&str]| super::RebootOptions::parse(args.iter().map(|s| s.to_string()));
        assert_eq!(parse(&[]).unwrap(), super::RebootOptions::default());
        for args in [
            vec!["--force", "--prebuilt", "release dir/core.exe"],
            vec!["--prebuilt", "release dir/core.exe", "--force"],
        ] {
            let options = parse(&args).unwrap();
            assert!(options.force);
            assert_eq!(
                options.prebuilt.as_deref(),
                Some(std::path::Path::new("release dir/core.exe"))
            );
        }
        for args in [
            vec!["--prebuilt"],
            vec!["--prebuilt", ""],
            vec!["--prebuilt", "--force"],
            vec!["--prebuit", "core.exe"],
            vec!["core.exe"],
            vec!["--force", "--force"],
            vec!["--prebuilt", "one", "--prebuilt", "two"],
            vec!["--force", "--prebuilt", "core.exe", "--from-source"],
            // `--service` ALONE is valid since card 82af11f5 (the warm build supplies the
            // artifact); it is exercised in the valid cases above on Windows.
            vec!["--service", "--service", "--prebuilt", "core.exe"],
            vec!["--validate-only"],
            vec!["--validate-only", "--force", "--prebuilt", "core.exe"],
            vec!["--validate-only", "--service", "--prebuilt", "core.exe"],
            vec![
                "--validate-only",
                "--validate-only",
                "--prebuilt",
                "core.exe",
            ],
        ] {
            assert!(
                parse(&args).is_err(),
                "invalid request must stop at parsing: {args:?}"
            );
        }
        let service = parse(&["--service", "--prebuilt", "core.exe"]);
        let validate = parse(&["--prebuilt", "core.exe", "--validate-only"]).unwrap();
        assert!(validate.validate_only);
        assert!(!validate.force && !validate.service);
        if cfg!(windows) {
            let service = service.unwrap();
            assert!(service.service);
            assert!(!service.force);
        } else {
            assert!(
                service.is_err(),
                "scheduled task deployment is Windows-only"
            );
        }
    }

    // what this catches: the normal Windows update must refuse a stale task
    // BEFORE stopping the live core, even when its description names the new image.
    #[test]
    fn service_reboot_rejects_stale_action_and_artifact_before_teardown() {
        let directory = tempfile::tempdir().unwrap();
        let directory = directory.path().canonicalize().unwrap();
        let artifact = directory.join("continuum-core-server.exe");
        let cli = directory.join("continuum.exe");
        let launcher = directory.join("run-service-hidden.ps1");
        let shell = directory.join("powershell.exe");
        let stale = directory.join("old-core.exe");
        let engine_directory = directory.join("engine-slot");
        std::fs::create_dir(&engine_directory).unwrap();
        let engine = engine_directory.join("llama-server.exe");
        for path in [&artifact, &cli, &launcher, &shell, &stale, &engine] {
            std::fs::write(path, b"installer artifact").unwrap();
        }
        let socket = directory.join("core.sock").display().to_string();
        let description = serde_json::json!({
            "artifact": artifact,
            "socket": socket,
            "cli": cli,
            "launcher": launcher,
            "engine": engine,
            "logDirectory": directory,
        });
        let arguments = format!(
            "-NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy RemoteSigned -File \"{}\" -ExecutablePath \"{}\" -CorePath \"{}\" -SocketPath \"{}\" -EnginePath \"{}\" -LogDirectory \"{}\"",
            launcher.display(), cli.display(), artifact.display(), socket, engine.display(), directory.display(),
        );
        let mut task = super::CoreServiceTask {
            description: description.to_string(),
            command: shell.display().to_string(),
            arguments: arguments.clone(),
            enabled: true,
            state: "Ready".to_string(),
        };
        let candidate = super::PrebuiltCore {
            path: artifact.canonicalize().unwrap(),
            build_sha: "123456789".to_string(),
        };
        task.validate(&candidate, &socket, &shell).unwrap();
        task.arguments = arguments.replace(
            &artifact.display().to_string(),
            &stale.display().to_string(),
        );
        assert!(task.validate(&candidate, &socket, &shell).is_err());
        task.arguments = arguments;
        assert!(task.validate(&candidate, "different.sock", &shell).is_err());
        task.enabled = false;
        assert!(task.validate(&candidate, &socket, &shell).is_err());
        task.enabled = true;
        let mut old_description = description;
        old_description["artifact"] = serde_json::json!(stale);
        task.description = old_description.to_string();
        assert!(task.validate(&candidate, &socket, &shell).is_err());
    }

    // what this catches: card 67f53b63 — provenance is checked before a
    // candidate can select the direct launch plan, including on installed nodes.
    #[tokio::test]
    async fn prebuilt_reboot_rejects_unusable_or_mismatched_candidates() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(
            super::PrebuiltCore::prepare(&tmp.path().join("absent-core"))
                .await
                .is_err()
        );
        assert!(super::PrebuiltCore::prepare(tmp.path()).await.is_err());
        let artifact = tmp.path().join("core");
        for report in ["", "unknown", "abc123", "not-a-sha", "def456a"] {
            assert!(super::PrebuiltCore::from_report(
                artifact.clone(),
                report.into(),
                Some("abc123f")
            )
            .is_err());
        }
        for report in ["", "unknown", "abc123", "not-a-sha"] {
            assert!(
                super::PrebuiltCore::from_report(artifact.clone(), report.into(), None).is_err()
            );
        }
        let ready =
            super::PrebuiltCore::from_report(artifact, "abc123f0123456789".into(), Some("abc123f"))
                .unwrap();
        assert_eq!(ready.build_sha, "abc123f0123456789");
    }

    // Regression for PR3902: failed Git inside a checkout must not become the
    // standalone self-anchor. Use real Git metadata and worktree selection;
    // process cwd/environment stay local to each command, never global test state.
    #[tokio::test]
    async fn prebuilt_checkout_lookup_distinguishes_absence_from_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        let standalone = tmp.path().join("standalone");
        let hooks = tmp.path().join("empty-hooks");
        for path in [&repo, &standalone, &hooks] {
            std::fs::create_dir(path).unwrap();
        }
        let git = |cwd: &std::path::Path, args: &[&str]| {
            let output = std::process::Command::new("git")
                .current_dir(cwd)
                .args([
                    "-c",
                    "user.name=Continuum tests",
                    "-c",
                    "user.email=tests@example.invalid",
                    "-c",
                    "commit.gpgsign=false",
                    "-c",
                    "maintenance.auto=false",
                    "-c",
                    "gc.auto=0",
                ])
                .arg("-c")
                .arg(format!("core.hooksPath={}", hooks.display()))
                .env_remove("GIT_DIR")
                .env_remove("GIT_WORK_TREE")
                .env_remove("GIT_COMMON_DIR")
                .env_remove("GIT_INDEX_FILE")
                .env_remove("GIT_OBJECT_DIRECTORY")
                .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
                .args(args)
                .output()
                .expect("Git is required for checkout provenance tests");
            assert!(
                output.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).unwrap().trim().to_string()
        };

        git(&repo, &["init", "--quiet"]);
        assert!(
            super::prebuilt_checkout_sha(&repo, None).await.is_err(),
            "a checkout with no readable HEAD cannot self-anchor"
        );
        git(
            &repo,
            &["commit", "--allow-empty", "--quiet", "-m", "fixture"],
        );
        git(&repo, &["checkout", "--detach", "--quiet", "HEAD"]);
        let head = git(&repo, &["rev-parse", "--short", "HEAD"]);
        assert_eq!(
            super::prebuilt_checkout_sha(&repo, None).await.unwrap(),
            Some(head.clone())
        );

        git(
            &repo,
            &[
                "worktree",
                "add",
                "--detach",
                "--quiet",
                "../linked",
                "HEAD",
            ],
        );
        let linked = tmp.path().join("linked");
        let nested = linked.join("src");
        std::fs::create_dir(&nested).unwrap();
        assert!(linked.join(".git").is_file());
        assert_eq!(
            super::prebuilt_checkout_sha(&nested, None).await.unwrap(),
            Some(head.clone())
        );

        // A sibling checkout must not bind a standalone invocation, but an
        // explicit GIT_DIR must bind it, including when that selection is broken.
        let absent = super::prebuilt_checkout_sha(&standalone, None)
            .await
            .unwrap();
        assert!(absent.is_none());
        assert!(super::PrebuiltCore::from_report(
            standalone.join("core"),
            "abc123f".into(),
            absent.as_deref(),
        )
        .is_ok());
        assert_eq!(
            super::prebuilt_checkout_sha(&standalone, Some(repo.join(".git").as_os_str()))
                .await
                .unwrap(),
            Some(head),
        );
        assert!(super::prebuilt_checkout_sha(
            &standalone,
            Some(tmp.path().join("missing").as_os_str())
        )
        .await
        .is_err());

        std::fs::write(linked.join(".git"), "gitdir: missing-checkout\n").unwrap();
        assert!(
            super::prebuilt_checkout_sha(&nested, None).await.is_err(),
            "a broken worktree gitfile is not a standalone installation"
        );
    }

    // what this catches: card 67f53b63 — an inherited build request, an available
    // source script or installed artifact must not redirect an explicit candidate.
    // Assert the production Command's path/argv/cwd, not a duplicate launch model.
    #[test]
    fn prebuilt_reboot_launch_retains_candidate_socket_and_cwd() {
        let candidate = super::PrebuiltCore::from_report(
            std::path::PathBuf::from("verified release/core.exe"),
            "abc123f".into(),
            None,
        )
        .unwrap();
        for have_script in [false, true] {
            for have_installed in [false, true] {
                for env_from_source in [false, true] {
                    assert_eq!(
                        super::plan_launch(
                            super::LaunchSource::Prebuilt(&candidate),
                            env_from_source,
                            have_script,
                            have_installed
                        ),
                        super::LaunchPlan::Prebuilt,
                    );
                }
            }
        }
        let cmd = super::direct_core_command(&candidate.path, "socket from caller");
        assert_eq!(cmd.get_program(), candidate.path.as_os_str());
        assert_eq!(
            cmd.get_args().collect::<Vec<_>>(),
            vec![std::ffi::OsStr::new("socket from caller")]
        );
        assert!(
            cmd.get_current_dir().is_none(),
            "inherit repository cwd and its AIRC/workspace scope"
        );
        assert!(super::deploy_verdict(
            Some("def456a"),
            &candidate.build_sha,
            "prebuilt candidate",
            "replacement core"
        )
        .is_err());
        assert!(super::deploy_verdict(
            Some("abc123f"),
            &candidate.build_sha,
            "prebuilt candidate",
            "replacement core"
        )
        .is_ok());
    }

    // Regression for card 25fadb8f: reboot --help stopped a live core. Every
    // local verb must take the help return, including when --force is present;
    // remote verbs must retain their schema-derived help path.
    #[test]
    fn lifecycle_help_precedes_actions_and_remote_help_stays_remote() {
        for command in [
            "start",
            "reboot",
            "restart",
            "boot",
            "stop",
            "desktop",
            "ui",
            "orphans",
            "deploy-verify",
            "verify",
            "checkpoint",
        ] {
            for flag in ["-h", "--help"] {
                assert!(super::local_help_requested(
                    command,
                    &["--force".into(), flag.into()]
                ));
            }
            assert!(!super::local_help_requested(command, &["--force".into()]));
        }
        for command in ["help", "-h", "--help"] {
            assert!(super::local_help_requested(command, &[]));
        }
        assert!(!super::local_help_requested(
            "serving/status",
            &["--help".into()]
        ));
    }

    // what this catches: the direct-exec launch path losing the manifest's
    // runtime library dirs. On a CUDA Windows node that loss is fatal BEFORE
    // main() — STATUS_DLL_NOT_FOUND (0xC0000135), no output, empty start log,
    // a core that "just doesn't come up" with nothing to read. Measured on the
    // 5090 2026-09-04 with a positive control both ways: the same freshly
    // built binary exits 0xC0000135 without the CUDA bin dir on PATH and runs
    // with it.
    //
    // Asserts the SELECTION RULE rather than spawning a process: only
    // existing dirs are contributed (a node without CUDA is unaffected), each
    // real cuda-* major contributes its own Library/bin, and unrelated
    // directories are never picked up.
    #[test]
    fn runtime_library_dirs_take_only_real_toolchain_paths() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir_all(root.join("tools/cmake/bin")).expect("cmake bin");
        std::fs::create_dir_all(root.join("cuda-13.2/Library/bin")).expect("cuda bin");
        // Present but NOT a runtime dir: must never be contributed.
        std::fs::create_dir_all(root.join("cuda-12.1/Library/lib")).expect("cuda lib only");
        std::fs::create_dir_all(root.join("models")).expect("models");

        let dirs = super::runtime_library_dirs(root);

        assert!(
            dirs.contains(&root.join("tools/cmake/bin")),
            "a provisioned tool's bin/ must be contributed: {dirs:?}"
        );
        assert!(
            dirs.contains(&root.join("cuda-13.2/Library/bin")),
            "a real CUDA tree's Library/bin is the dir whose absence kills the loader: {dirs:?}"
        );
        assert!(
            !dirs.iter().any(|d| d.starts_with(root.join("cuda-12.1"))),
            "a cuda-* tree with no Library/bin contributes nothing: {dirs:?}"
        );
        assert!(
            !dirs.iter().any(|d| d.ends_with("models")),
            "unrelated continuum-root dirs are never runtime library paths: {dirs:?}"
        );
    }

    // what this catches: card d2698cdc — ONNX selection must work without any
    // CUDA/toolchain directory and must select the host platform's installed ABI.
    #[test]
    fn native_launch_selects_platform_ort_without_cuda() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir(root.join("lib")).expect("lib dir");
        for (os, name) in [
            ("windows", "onnxruntime.dll"),
            ("linux", "libonnxruntime.so"),
            ("macos", "libonnxruntime.dylib"),
        ] {
            let library = root.join("lib").join(name);
            std::fs::write(&library, []).expect("installed library fixture");
            let mut cmd = std::process::Command::new("unused");
            cmd.env("ORT_DYLIB_PATH", "");
            cmd.env("PATH", root.join("operator-bin"));
            super::apply_runtime_library_env_in(&mut cmd, root, os);
            assert_eq!(
                super::command_env(&cmd, "ORT_DYLIB_PATH"),
                Some(library.into_os_string())
            );
            assert_eq!(
                super::command_env(&cmd, "PATH"),
                Some(root.join("operator-bin").into_os_string()),
                "ONNX discovery must not require or rewrite a toolchain PATH"
            );
        }
    }

    // what this catches: config.env overrides live on Command, not in the CLI's
    // environment. Preserve both an explicit ORT selection and configured PATH.
    #[test]
    fn native_launch_preserves_runtime_overrides_and_configured_path() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let cuda = root.join("cuda-13.2/Library/bin");
        std::fs::create_dir_all(&cuda).expect("cuda bin");
        let explicit_ort = root.join("operator-ort.dll");
        let explicit_path = root.join("operator-bin");
        let mut cmd = std::process::Command::new("unused");
        // Windows preserves spelling but compares environment keys without case.
        // config.env may legally use either spelling; Unix remains case-sensitive.
        let ort_key = if cfg!(windows) {
            "ort_dylib_path"
        } else {
            "ORT_DYLIB_PATH"
        };
        let path_key = if cfg!(windows) { "Path" } else { "PATH" };
        cmd.env(ort_key, &explicit_ort);
        cmd.env(path_key, &explicit_path);
        super::apply_runtime_library_env_in(&mut cmd, root, "windows");
        assert_eq!(
            super::command_env(&cmd, "ORT_DYLIB_PATH"),
            Some(explicit_ort.into_os_string()),
            "an explicit override remains authoritative even if the path is absent"
        );
        let actual = super::command_env(&cmd, "PATH").expect("child PATH");
        assert_eq!(
            std::env::split_paths(&actual).collect::<Vec<_>>(),
            vec![cuda, explicit_path],
            "prepend runtime dirs to the configured child PATH, not the parent's PATH"
        );
        cmd.env_remove(path_key);
        assert!(
            super::command_env(&cmd, "PATH").is_none(),
            "an explicit removal must not fall through to the parent's environment"
        );
    }

    use super::*;

    // what this catches (2026-09-13): a warm build attempted without headroom (starving the
    // serving core, Joel 08-23) or without a build definition; and a refusal that does not
    // name the number the operator needs.
    #[test]
    fn a_warm_build_needs_a_script_and_headroom_and_says_why_not() {
        let script = Some(PathBuf::from("/x/start-server.sh"));
        assert!(warm_build_allowed(WARM_BUILD_MIN_FREE_BYTES, script.clone()).is_ok());
        let err = warm_build_allowed(WARM_BUILD_MIN_FREE_BYTES - 1, script).unwrap_err();
        assert!(err.contains("GiB free"), "{err}");
        assert!(
            warm_build_allowed(u64::MAX, None).is_err(),
            "no script = no build definition"
        );
    }

    // what this catches (2026-09-19, the 5090's consumer): a log handle opened with
    // `.append(true)` is FILE_APPEND_DATA-only on Windows, and an MSYS bash handed it
    // as stdout exits 1 before running a line — every unattended deploy on the node
    // died in one second with an EMPTY log, and the exit code named nothing. The
    // handle a child is given must be one bash runs under and writes through, and
    // it must still append.
    #[cfg(windows)]
    #[test]
    fn the_deploy_log_handle_is_one_bash_can_run_under() {
        let dir = tempfile::tempdir().expect("tempdir");
        let log = dir.path().join("deploy.log");
        std::fs::write(&log, "seed\n").expect("seed");
        let bash = continuum_core::shell_portable::locate_bash().expect("Git bash is a build prerequisite on Windows");
        let status = std::process::Command::new(bash)
            .args(["-c", "echo from-bash"])
            .stdin(Stdio::null())
            .stdout(Stdio::from(open_log_for_child(&log).expect("open")))
            .stderr(Stdio::from(open_log_for_child(&log).expect("open")))
            .status()
            .expect("spawn");
        assert!(status.success(), "bash under the log handle exited {status}");
        let text = std::fs::read_to_string(&log).expect("read");
        assert_eq!(text.replace("\r", ""), "seed\nfrom-bash\n", "the child wrote through the handle, after the seed");
    }

    // Regression for card 571d6e0b: an old script's absent receipt, malformed
    // output, or failed build cannot turn into teardown followed by a rebuild.
    #[tokio::test]
    async fn warm_build_receipts_are_bounded_owned_and_fail_closed() {
        let receipt = WarmBuildReceipt::create().unwrap();
        let receipt_path = receipt.0.clone();
        for invalid in ["", "relative/core", "/one\n/two\n", "/one\0two"] {
            std::fs::write(&receipt.0, invalid).unwrap();
            assert!(receipt.artifact().is_err(), "accepted {invalid:?}");
        }
        std::fs::write(&receipt.0, vec![b'x'; 16 * 1024 + 1]).unwrap();
        assert!(receipt.artifact().is_err());
        let artifact = std::env::temp_dir().join("build cache 雪").join("core.exe");
        std::fs::write(&receipt.0, format!("{}\n", artifact.display())).unwrap();
        assert_eq!(receipt.artifact().unwrap(), artifact);
        drop(receipt);
        assert!(!receipt_path.exists());

        let mut failed = std::process::Command::new(locate_bash().unwrap());
        failed.args([
            "-c",
            "printf '%s\\n' /misleading/core > \"$CONTINUUM_BUILD_RECEIPT\"; exit 23",
        ]);
        let error = prepare_warm_build(failed).await.unwrap_err();
        assert!(error.contains("warm build exited"), "{error}");
        assert!(error.contains("leaving the running core untouched"));

        let mut missing = std::process::Command::new(locate_bash().unwrap());
        missing.args(["-c", "exit 0"]);
        assert!(prepare_warm_build(missing)
            .await
            .unwrap_err()
            .contains("did not report one absolute native artifact path"));

        let directory = tempfile::tempdir().unwrap();
        let unusable = directory.path().join("unusable core.exe");
        std::fs::write(&unusable, b"not an executable").unwrap();
        let mut reported = std::process::Command::new(locate_bash().unwrap());
        reported
            .args([
                "-c",
                "printf '%s\\n' \"$1\" > \"$CONTINUUM_BUILD_RECEIPT\"",
                "receipt-fixture",
            ])
            .arg(&unusable);
        let error = prepare_warm_build(reported).await.unwrap_err();
        assert!(error.contains("--build-sha"), "{error}");
    }
    use serde_json::json;

    fn ptable(pairs: &[(i32, i32)]) -> std::collections::HashMap<i32, i32> {
        pairs.iter().copied().collect()
    }

    /// The socket a running core is bound to, and the refusal line that names it.
    ///
    /// Measured 2026-09-04 on the Intel Mac: pid 83712 served
    /// `~/.continuum/intelmac-core.sock` for an hour while every client-side diagnostic
    /// reported `/tmp/continuum-core.sock` — the one path that was not the answer.
    mod bound_socket {
        use super::*;

        fn argv(parts: &[&str]) -> Vec<String> {
            parts.iter().map(|s| s.to_string()).collect()
        }

        /// what this catches: re-deriving `main.rs`'s argument rule instead of calling it.
        /// BOTH spellings of the flag must be peeled — `boot_mode.rs` accepts
        /// `--mode VALUE` as well as `--mode=VALUE`, and the space form is the one the
        /// original heuristic got wrong: "first arg not starting with `-`" returns
        /// **"fail-fast"**, a boot mode reported to the operator as a socket path and
        /// pasted into their remedy line. The equals form alone passes either way, which
        /// is why the first version of this test looked right and proved nothing.
        #[test]
        fn both_flag_spellings_are_peeled_not_just_the_equals_form() {
            assert_eq!(
                socket_from_core_argv(&argv(&[
                    "continuum-core-server",
                    "--mode",
                    "fail-fast",
                    "/tmp/a.sock",
                ])),
                Some("/tmp/a.sock".to_string()),
                "the SPACE form's value must never be reported as a socket path"
            );
            assert_eq!(
                socket_from_core_argv(&argv(&[
                    "continuum-core-server",
                    "--mode=full-citizen",
                    "/tmp/a.sock",
                ])),
                Some("/tmp/a.sock".to_string()),
                "a flag before the positional must not be mistaken for the socket"
            );
            assert_eq!(
                socket_from_core_argv(&argv(&["continuum-core-server", "/tmp/a.sock"])),
                Some("/tmp/a.sock".to_string())
            );
        }

        /// what this catches: inventing a socket for a core that passed none. Such a core
        /// resolved the path from ITS environment at launch, which this process cannot read
        /// afterwards — so the honest answer is "unknown", and a `Some(default)` here would
        /// make the hint assert a path the core may well not be on. Both flag spellings,
        /// because the space form is where a "skip the dashes" reading leaks the VALUE.
        #[test]
        fn a_core_with_no_positional_reports_unknown_rather_than_the_default() {
            assert_eq!(
                socket_from_core_argv(&argv(&["continuum-core-server", "--mode=fail-fast"])),
                None
            );
            assert_eq!(
                socket_from_core_argv(&argv(&["continuum-core-server", "--mode", "fail-fast"])),
                None,
                "the space form's value is a boot mode, not an unknown socket"
            );
        }

        /// what this catches: reporting a socket for a core that never bound one. A
        /// malformed `--mode` makes `main.rs` print its error and `exit(2)` BEFORE any
        /// socket is resolved, so such a process provably bound nothing — "unknown" is
        /// the literally correct answer, not a swallowed error. The pid is never hidden
        /// by this: the refusal lists it from `running_core_pids` either way. Only the
        /// unprovable socket claim is withheld, which is this helper's whole contract.
        #[test]
        fn a_malformed_mode_reports_unknown_because_that_core_bound_nothing() {
            assert_eq!(
                socket_from_core_argv(&argv(&[
                    "continuum-core-server",
                    "--mode",
                    "not-a-real-mode",
                    "/tmp/a.sock",
                ])),
                None,
                "a core that exits on a bad --mode never reached socket resolution"
            );
        }

        /// what this catches: the hint firing on the ordinary wedged core. A core bound to
        /// the very socket the CLI is using is genuinely not answering; telling that
        /// operator to "reach it elsewhere" would send them chasing a door that is already
        /// open, so the existing message must go out unchanged.
        #[test]
        fn no_hint_when_the_running_core_is_bound_to_the_socket_we_asked_for() {
            let bound = vec![(1, "/tmp/continuum-core.sock".to_string())];
            assert_eq!(
                bound_elsewhere_hint(&bound, "/tmp/continuum-core.sock"),
                None
            );
            assert_eq!(bound_elsewhere_hint(&[], "/tmp/continuum-core.sock"), None);
        }

        /// what this catches: a hint that reports the mismatch without the remedy. The
        /// operator's next keystroke needs the core's ACTUAL path — naming only the
        /// client's is what cost four commands and a wrong "wedged" hypothesis.
        #[test]
        fn hint_names_the_cores_path_and_the_command_that_reaches_it() {
            let bound = vec![(83712, "/home/agent/.continuum/node-core.sock".to_string())];
            let hint = bound_elsewhere_hint(&bound, "/tmp/continuum-core.sock")
                .expect("a core bound elsewhere must produce a hint");
            assert!(
                hint.contains("pid 83712 → /home/agent/.continuum/node-core.sock"),
                "must name which pid is where: {hint}"
            );
            assert!(
                hint.contains("CONTINUUM_CORE_SOCKET=/home/agent/.continuum/node-core.sock"),
                "the remedy must carry the core's path, not the client's: {hint}"
            );
            assert!(
                hint.contains("NOT wedged"),
                "with nothing on our socket, the hint's job is to rule out the wedged case: {hint}"
            );
        }

        /// what this catches: a mixed fleet hiding the outlier — AND the headline denying
        /// the other half of it. With one core on the client's socket and one elsewhere,
        /// only the unreachable core belongs in the list; but the core on our socket IS
        /// genuinely wedged, so leading with an unconditional "NOT wedged" would tell the
        /// operator their real problem does not exist. The earlier version of this test
        /// pinned the right list around exactly that false claim by never asserting on the
        /// headline at all.
        #[test]
        fn mixed_fleet_lists_only_the_outlier_and_does_not_deny_the_wedged_core() {
            let bound = vec![
                (7, "/tmp/continuum-core.sock".to_string()),
                (9, "/home/agent/.continuum/node-core.sock".to_string()),
            ];
            let hint = bound_elsewhere_hint(&bound, "/tmp/continuum-core.sock").expect("hint");
            assert!(
                hint.contains("pid 9"),
                "the unreachable core must appear: {hint}"
            );
            assert!(
                !hint.contains("pid 7"),
                "a core on the asked-for socket is not 'elsewhere': {hint}"
            );
            assert!(
                !hint.contains("NOT wedged"),
                "pid 7 IS on our socket and unresponsive — the hint must not deny it: {hint}"
            );
        }
    }

    /// The deploy watchdog: is a slow build failing, or just slow?
    mod launch_wait {
        use super::*;

        /// what this catches: a WALL-CLOCK bound standing in for a liveness check.
        ///
        /// Both of these are REAL measurements from 2026-09-05 and both deploys SUCCEEDED —
        /// 1,404s for a full CUDA rebuild on the 5090, and an Intel Mac cold build that ran
        /// past the 1,800s ceiling and brought the core up about forty minutes after the CLI
        /// had already reported failure. A verdict keyed on elapsed time calls both of those
        /// dead. Keyed on silence, both are alive, which is the only answer that is correct
        /// on both machines.
        #[test]
        fn a_long_build_that_is_still_talking_is_not_a_failure() {
            let stall = 5 * 60;
            // 23 minutes elapsed means nothing; 10s since the last log line means everything.
            assert_eq!(wait_verdict(10, stall), WaitVerdict::Progressing);
            // Even at the far end of a 40-minute build, recent output is life.
            assert_eq!(wait_verdict(stall - 1, stall), WaitVerdict::Progressing);
        }

        /// what this catches: regression for card 6c91036c — the watchdog killing a HEALTHY
        /// build and reporting it "burned no CPU" while rustc sat at 631% in its own process
        /// group. Reproduced on a real deploy of f49409fa2 on the Intel Mac.
        ///
        /// The readings here are the MEASURED ones, taken 20s apart mid-build: the group total
        /// was ~2589s while a 43-minute rustc was alive, then ~3s once that child finished its
        /// crate and cargo replaced it. Group cputime sums only members alive at that instant,
        /// so a child exiting REMOVES its accumulated time and the total drops.
        ///
        /// THIS TEST MUST DRIVE THE READING DOWN. A test that only ever feeds increasing values
        /// passes against `reading > last` — the buggy comparison — which is presumably how the
        /// defect shipped with the rest of the watchdog well covered. The descent IS the bug.
        #[test]
        fn a_cpu_reading_that_drops_is_progress_because_a_finished_child_leaves_the_group() {
            // The measured pair. Under `>` this returns false and the stall clock never resets
            // again for the life of the build.
            assert!(
                cpu_reading_shows_progress(3, 2589),
                "a group total that FELL because a long rustc finished its crate is a build \
                 making progress, not a dead one"
            );
            // Ordinary within-crate accumulation is still progress.
            assert!(cpu_reading_shows_progress(2589, 3));
            // And a build that is genuinely stuck holds the sum CONSTANT — the one reading that
            // must NOT reset the clock, or the watchdog never fires at all.
            assert!(
                !cpu_reading_shows_progress(2589, 2589),
                "an unchanged group total is the signature of a hung exec: it burns no CPU and \
                 spawns no children, and it is the only thing this watchdog exists to catch"
            );
            // A climbing total reads as progress, which is what keeps an ordinary build alive.
            // It also means a SPINNING hang is not caught by this watchdog at all: the CPU arm
            // and the log arm reset the same clock, so a spin resets it forever and only
            // MAX_WAIT_SECS bounds it. True of `>` as well — not a regression, just not a
            // guarantee. Asserted so nobody 'fixes' this into an equality check believing the
            // log arm would still cover the spin; it would not.
            assert!(cpu_reading_shows_progress(2590, 2589));
        }

        /// what this catches: the opposite failure — never giving up. A script that has said
        /// nothing for the full stall window has stopped, and the operator needs to be told
        /// that rather than left watching a spinner. The boundary is inclusive so the limit
        /// means "this long is too long", not "this long is fine".
        #[test]
        fn silence_past_the_limit_is_a_stall_and_the_boundary_is_inclusive() {
            let stall = 5 * 60;
            assert_eq!(wait_verdict(stall, stall), WaitVerdict::Stalled);
            assert_eq!(wait_verdict(stall + 1, stall), WaitVerdict::Stalled);
        }

        /// what this catches: misreading `ps -o cputime=`, which is the second progress signal
        /// and the fix for a regression I shipped — a stall clock reset ONLY by log lines
        /// condemns cargo compiling one large crate quietly, which is exactly the cold low-end
        /// build the watchdog protects. Every shape `ps` emits must parse, because a field this
        /// function silently reads as 0 turns a working build back into a false stall.
        #[test]
        fn every_ps_cputime_shape_parses() {
            assert_eq!(cputime_to_secs("0:05"), 5, "mm:ss");
            assert_eq!(cputime_to_secs("12:34"), 12 * 60 + 34);
            assert_eq!(cputime_to_secs("1:02:03"), 3600 + 2 * 60 + 3, "hh:mm:ss");
            assert_eq!(
                cputime_to_secs("2-03:04:05"),
                2 * 86_400 + 3 * 3600 + 4 * 60 + 5,
                "dd-hh:mm:ss — the day field arrives glued to the hour"
            );
            assert_eq!(
                cputime_to_secs("   7:08  "),
                7 * 60 + 8,
                "ps pads its column"
            );
        }

        /// what this catches: a garbage field silently reading as a huge or negative number and
        /// either freezing the stall clock forever or resetting it every tick. Unparseable means
        /// ZERO contribution — the caller then falls back to the log signal, which is the
        /// refuse-to-guess posture the rest of tonight's fixes landed on.
        #[test]
        fn unparseable_cputime_contributes_nothing_rather_than_guessing() {
            assert_eq!(cputime_to_secs(""), 0);
            assert_eq!(cputime_to_secs("   "), 0);
            assert_eq!(cputime_to_secs("?"), 0);
            assert_eq!(cputime_to_secs("not:a:time"), 0);
        }
    }

    mod launch_policy {
        use super::*;

        /// what this catches: `reboot` — THE deploy path — silently running a
        /// prebuilt artifact instead of the source just edited. Regression for
        /// 7e0c5469a, which made the installed binary the default for BOTH
        /// callers of `launch_core`; the reboot banner still promised "building
        /// fresh binary, then swapping" while exec'ing a month-old binary, so no
        /// edit could reach the running core at all.
        #[test]
        fn reboot_builds_from_source_even_when_an_installed_binary_exists() {
            assert_eq!(
                plan_launch(LaunchSource::FromSource, false, true, true),
                LaunchPlan::Script
            );
        }

        /// what this catches: re-breaking `start` for the no-source-tree user
        /// (the case 7e0c5469a was written for) while fixing reboot. `start`
        /// wants a RUNNING core, not a fresh one — it must never compile when an
        /// artifact is sitting right there.
        #[test]
        fn start_prefers_the_installed_binary_over_a_compile() {
            assert_eq!(
                plan_launch(LaunchSource::Installed, false, true, true),
                LaunchPlan::Installed
            );
        }

        /// what this catches: a reboot on an installed node (no checkout) either
        /// dying with "no start script" or — worse — quietly calling itself a
        /// deploy. It restarts the artifact, and the distinct variant is what
        /// forces the caller to SAY nothing was rebuilt.
        #[test]
        fn reboot_without_a_source_tree_restarts_the_artifact_and_says_so() {
            assert_eq!(
                plan_launch(LaunchSource::FromSource, false, false, true),
                LaunchPlan::InstalledWithoutRebuild
            );
        }

        /// what this catches: substituting a prebuilt binary for an explicit
        /// operator demand to compile. CONTINUUM_FROM_SOURCE asks a specific
        /// question; answering a different one silently is the fallback class
        /// this codebase forbids.
        #[test]
        fn an_explicit_from_source_request_fails_loud_with_no_script() {
            assert_eq!(
                plan_launch(LaunchSource::Installed, true, false, true),
                LaunchPlan::NoLaunchable
            );
        }

        /// what this catches: the env override being ignored on the `start` path
        /// once the policy argument existed — two ways to ask for a source build
        /// and only one honoured.
        #[test]
        fn the_env_override_still_forces_a_source_build_on_start() {
            assert_eq!(
                plan_launch(LaunchSource::Installed, true, true, true),
                LaunchPlan::Script
            );
        }

        /// what this catches: a bare machine (no artifact, no checkout) getting a
        /// launch attempt against nothing instead of one clear error.
        #[test]
        fn nothing_installed_and_no_script_is_a_loud_nothing_to_launch() {
            assert_eq!(
                plan_launch(LaunchSource::Installed, false, false, false),
                LaunchPlan::NoLaunchable
            );
            assert_eq!(
                plan_launch(LaunchSource::FromSource, false, false, false),
                LaunchPlan::NoLaunchable
            );
        }

        /// what this catches: `start` in a fresh checkout before anything is
        /// installed — the fresh-clone front door (#291). Compiling is correct
        /// here; refusing is not.
        #[test]
        fn start_in_a_checkout_with_no_installed_binary_builds() {
            assert_eq!(
                plan_launch(LaunchSource::Installed, false, true, false),
                LaunchPlan::Script
            );
        }
    }

    /// what this catches: the orphan classifier deciding to KILL a live serving
    /// lane. `reboot` reaps every owned engine process not descended from a live
    /// core, so a wrong answer here terminates a 21 GB in-service llama-server
    /// mid-request. Grandchildren count as descended — engines are spawned
    /// through intermediate processes, so a depth-1 check would flag them all.
    #[test]
    fn in_service_descendants_are_never_orphans() {
        // core 100 -> shim 200 -> engine 300
        let parents = ptable(&[(300, 200), (200, 100)]);
        assert!(
            descends_from(&parents, 300, &[100]),
            "grandchild is in service"
        );
        assert!(descends_from(&parents, 200, &[100]));
        assert!(descends_from(&parents, 100, &[100]), "the core itself");
    }

    /// what this catches: card d746a1a0 — Windows `/T` on the core killed the
    /// recorded gateway before reboot's later orphan exclusion could protect it.
    /// Inspect the commands production executes, including the orphan-sweep pass.
    #[test]
    fn windows_teardown_preserves_only_the_verified_lane_subtree() {
        let parents = ptable(&[
            (100, 1),     // core
            (200, 100),   // gateway launcher, itself expendable
            (8600, 200),  // identity-verified live lane
            (8610, 8600), // helper owned by that lane
            (300, 100),   // eye-node
            (301, 300),   // browser
            (400, 100),   // other worker
            (500, 100),   // stale/unrecorded serving process: not protected
            (501, 500),
            (900, 1), // unrelated process: never a teardown root
        ]);
        let command_args = |plan: Vec<KillStep>| {
            let mut args: Vec<Vec<String>> = plan
                .into_iter()
                .map(|kill| {
                    let cmd = kill.command();
                    assert_eq!(cmd.get_program(), "taskkill");
                    cmd.get_args()
                        .map(|arg| arg.to_str().unwrap().to_owned())
                        .collect()
                })
                .collect();
            args.sort();
            args
        };
        // Duplicate/descendant roots must not produce overlapping tree kills.
        let roots = [300, 200, 100, 100, 301, 500];
        let plan = kill_plan(&roots, &parents, &[8600]);
        assert_eq!(
            plan.first(),
            Some(&KillStep::Process(100)),
            "stop the spawning core first"
        );
        assert_eq!(
            command_args(plan),
            vec![
                vec!["/F", "/PID", "100"],
                vec!["/F", "/PID", "200"],
                vec!["/F", "/T", "/PID", "300"],
                vec!["/F", "/T", "/PID", "400"],
                vec!["/F", "/T", "/PID", "500"],
            ]
        );
        // A surviving launcher ancestor in the owned-orphan pass must ALSO be
        // killed alone, never with /T across the now-adoptable lane beneath it.
        assert_eq!(
            command_args(kill_plan(
                &[200, 300, 301, 400, 500, 501, 8600, 8610],
                &parents,
                &[8600],
            )),
            vec![
                vec!["/F", "/PID", "200"],
                vec!["/F", "/T", "/PID", "300"],
                vec!["/F", "/T", "/PID", "400"],
                vec!["/F", "/T", "/PID", "500"],
            ]
        );
        // Full stop — or no identity-verified lane record — still kills the
        // entire core tree, including the otherwise-preserved gateway.
        assert_eq!(
            command_args(kill_plan(&roots, &parents, &[])),
            vec![vec!["/F", "/T", "/PID", "100"]]
        );
        // An old fixed 64-hop ancestry limit must not turn a deeper protected
        // branch into an apparently unprotected /T target.
        let deep: std::collections::HashMap<i32, i32> =
            (1..=80).map(|pid| (pid, pid - 1)).collect();
        let plan = kill_plan(&[1], &deep, &[80]);
        assert_eq!(plan.len(), 79);
        assert!(plan
            .iter()
            .all(|kill| matches!(kill, KillStep::Process(_))));
    }

    // what this catches (IntelMac 2026-09-21 11:20Z, card 3ef0986c): the deploy consumer
    // is the CORE'S CHILD, so the tree it reaps on a reboot contains itself. With the
    // consumer in the keep set (what `keep_with_self` adds at the call site), the core
    // gets a `Process` step — it dies alone — and no step names the consumer or its
    // branch; without it, `Tree(core)` reaps the hand mid-handoff and the kickstart the
    // node is owed is never issued. Both directions: the lane keep still holds beside it.
    #[test]
    fn the_reaper_is_never_in_its_own_plan() {
        let (core, consumer, lane, eye) = (100, 200, 300, 400);
        let parents = std::collections::HashMap::from([
            (consumer, core),
            (lane, core),
            (eye, core),
            (core, 1),
        ]);
        let plan = kill_plan(&[core], &parents, &keep_with_self_for_test(&[lane], consumer));
        assert_eq!(plan.first(), Some(&KillStep::Process(core)), "the core dies alone, never as a tree");
        assert!(
            !plan.iter().any(|s| matches!(s, KillStep::Tree(p) | KillStep::Process(p) if *p == consumer)),
            "the reaper never appears in its own plan: {plan:?}"
        );
        assert!(!plan.iter().any(|s| matches!(s, KillStep::Tree(p) | KillStep::Process(p) if *p == lane)), "the lane keep still holds");
        assert!(plan.contains(&KillStep::Tree(eye)), "an unprotected sibling is still reaped: {plan:?}");
        // The pre-fix shape, for contrast: with only the lane kept, the consumer IS in the plan.
        let unprotected = kill_plan(&[core], &parents, &[lane]);
        assert!(unprotected.contains(&KillStep::Tree(consumer)), "without the self keep the hand is reaped: {unprotected:?}");
    }

    /// `keep_with_self` pins the CALLER's pid, which in a test is the test binary; this
    /// mirrors it with an explicit self so the plan can be asserted for a chosen pid.
    fn keep_with_self_for_test(keep: &[i32], self_pid: i32) -> Vec<i32> {
        let mut with_self = keep.to_vec();
        with_self.push(self_pid);
        with_self
    }

    /// what this catches: the actual BIGMAMA leak. llama-server pid 37148 ran
    /// 24h past its dead parent, holding 127.0.0.1:8090, so every persona that
    /// resolved a chat model through that port got an EMBEDDING model. Its
    /// parent is absent from the table entirely (dead), which must read as
    /// "orphan", not as "unknown, leave it alone".
    /// what this catches (card 59052747, 2026-09-20): the Unix executor of the kill plan
    /// — until now `let _ = keep;` and a group kill per root, so the plan's `Process(core)`
    /// never existed on macOS/Linux and the live lane died with the core on every reboot.
    /// Real processes, one group: a leader and its child; a plan that protects the child
    /// must leave it alive after the leader is gone, and the same plan with nothing
    /// protected must take the whole group (the plain `stop`).
    #[cfg(unix)]
    #[test]
    fn a_protected_child_survives_its_leaders_kill_on_unix() {
        use std::os::unix::process::CommandExt;
        use std::process::{Command, Stdio};
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
        fn alive(pid: i32) -> bool {
            unsafe { libc::kill(pid, 0) == 0 }
        }
        fn group(protect: bool) -> (i32, i32) {
            // A leader that execs nothing but waits on one child in ITS group.
            let mut leader = Command::new("sh")
                .args(["-c", "sleep 30 & wait"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .process_group(0)
                .spawn()
                .expect("sh spawns"); // test: the shell exists on every unix CI box
            let leader_pid = leader.id() as i32;
            // Find the child (the sleep) under the leader.
            let child = (0..50)
                .find_map(|_| {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    let mut sys = System::new();
                    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
                    process_parents(&sys)
                        .into_iter()
                        .find(|(_, parent)| *parent == leader_pid)
                        .map(|(child, _)| child)
                })
                .expect("the leader's child appears"); // test: `sleep` forks within 2.5 s
            let mut sys = System::new();
            sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
            let parents = process_parents(&sys);
            let keep: Vec<i32> = if protect { vec![child] } else { vec![] };
            let plan = kill_plan(&[leader_pid], &parents, &keep);
            assert_eq!(plan, vec![if protect { KillStep::Process(leader_pid) } else { KillStep::Tree(leader_pid) }]);
            for step in plan {
                step.execute();
            }
            let _ = leader.wait();
            std::thread::sleep(std::time::Duration::from_millis(200));
            (leader_pid, child)
        }
        let (leader, child) = group(true);
        assert!(!alive(leader), "the leader is gone");
        assert!(alive(child), "the protected child in the leader's group survives its kill");
        unsafe { libc::kill(child, libc::SIGKILL) };
        let (leader, child) = group(false);
        assert!(!alive(leader));
        assert!(!alive(child), "nothing protected: the group goes with the leader (plain stop)");
    }

    #[test]
    fn a_process_whose_parent_is_gone_is_an_orphan() {
        let parents = ptable(&[(37148, 37856)]); // 37856 itself not present = dead
        assert!(!descends_from(&parents, 37148, &[37920]));
        // And with no core running at all, nothing is in service.
        assert!(!descends_from(&parents, 37148, &[]));
    }

    /// what this catches: a HANG in `continuum reboot`. A pid table can present
    /// a cycle — pid reuse during a racing scan, or a parent pointer into a
    /// descendant — and an unbounded ancestor walk would spin forever inside the
    /// verb whose entire job is recovering from unknown failures.
    #[test]
    fn a_cyclic_parent_chain_terminates() {
        let parents = ptable(&[(1, 2), (2, 3), (3, 1)]);
        assert!(
            !descends_from(&parents, 1, &[999]),
            "must terminate, not hang"
        );
        // A cycle that CONTAINS a kept pid still resolves as in-service.
        assert!(descends_from(&parents, 1, &[3]));
    }

    /// what this catches: pid 0 / self-parent as chain terminators. Both
    /// platforms present them, and treating either as a real hop would walk a
    /// bogus ancestor or loop.
    #[test]
    fn root_sentinels_terminate_the_chain() {
        assert!(!descends_from(&ptable(&[(5, 0)]), 5, &[7]));
        assert!(!descends_from(&ptable(&[(5, 5)]), 5, &[7]));
    }

    // what this catches: the PROCEDURAL param adapter — one generic rule for all
    // commands, no per-command switch. Covers the three forms + coercion +
    // kebab/snake→camelCase + bare flags. This is the CLI edge of "meet humans/AIs
    // in the middle at every interface" (Joel 2026-06-21).
    #[test]
    fn params_adapt_procedurally_from_args() {
        // No schema available → generic camelCase normalization (pre-schema behavior).
        let no_schema: &[String] = &[];

        // 1. nothing → empty object
        assert_eq!(params_from_args("ping", &[], no_schema).unwrap(), json!({}));

        // 2. positional JSON verbatim (the AI / tool-call path)
        assert_eq!(
            params_from_args("ping", &[r#"{"message":"hi"}"#.to_string()], no_schema).unwrap(),
            json!({ "message": "hi" })
        );

        // 3. --key value with coercion: string stays string, number→number,
        //    bool→bool; keys camelCased from kebab/snake.
        let p = params_from_args(
            "ping",
            &[
                "--message".into(),
                "hi".into(),
                "--round-trip-ms".into(),
                "5".into(),
                "--enabled".into(),
                "true".into(),
            ],
            no_schema,
        )
        .unwrap();
        assert_eq!(
            p,
            json!({ "message": "hi", "roundTripMs": 5, "enabled": true }),
            "coerced + camelCased automatically, one generic rule"
        );

        // bare flag (no value) → true
        assert_eq!(
            params_from_args("ping", &["--verbose".into()], no_schema).unwrap(),
            json!({ "verbose": true })
        );

        // `--key=value` form (muscle memory) — split on first `=`, NOT a junk key.
        assert_eq!(
            params_from_args("ping", &["--filter=data/".into()], no_schema).unwrap(),
            json!({ "filter": "data/" }),
            "--key=value splits correctly (regression: was {{\"filter=data/\": true}})"
        );
        assert_eq!(
            params_from_args("ping", &["--round-trip-ms=5".into()], no_schema).unwrap(),
            json!({ "roundTripMs": 5 }),
            "--key=value coerces + camelCases"
        );

        // a non-flag, non-JSON arg is a clear error (not silently swallowed)
        assert!(params_from_args("ping", &["oops".into()], no_schema).is_err());
    }

    // what this catches: snake_case Rust-native command fields (e.g. cognition/eval's
    // `persona_id`) must be invokable by flag. continuum used to blanket-camelCase every key,
    // so `--persona_id` became `personaId` and the server rejected it with
    // `missing field persona_id`. With the command's schema known, any spelling of a
    // schema field canonicalizes to the exact field name; flags NOT in the schema
    // (base fields, schemaless commands) keep the legacy camelCase normalization.
    // regression for the 2026-06-25 continuum flag bug.
    #[test]
    fn flags_canonicalize_to_schema_field_names() {
        let canonical = vec!["persona_id".to_string(), "eval_set".to_string()];
        // every separator/case spelling of a schema field → the exact field name
        for spelling in [
            "--persona_id",
            "--persona-id",
            "--personaId",
            "--PERSONA_ID",
        ] {
            let p = params_from_args("ping", &[spelling.into(), "abc".into()], &canonical).unwrap();
            assert_eq!(p, json!({ "persona_id": "abc" }), "{spelling} → persona_id");
        }
        // `--key=value` form canonicalizes too
        assert_eq!(
            params_from_args("ping", &["--eval-set=x.jsonl".into()], &canonical).unwrap(),
            json!({ "eval_set": "x.jsonl" })
        );
        // a flag NOT in a KNOWN schema is refused, not coerced (see the unknown-flag
        // test below). Identity fields like userId are injected by the connection —
        // no command's schema carries them (349/349 on 2026-09-20) — so there is no
        // base-field carve-out to preserve here.
        assert!(
            params_from_args("ping", &["--room-id".into(), "r1".into()], &canonical).is_err(),
            "non-schema flag against a known schema → refused"
        );
        // with no schema at all, everything is legacy camelCase
        assert_eq!(
            params_from_args("ping", &["--persona-id".into(), "abc".into()], &[]).unwrap(),
            json!({ "personaId": "abc" }),
            "no schema → legacy camelCase (pre-schema behavior preserved)"
        );
    }

    // what this catches: a typo'd flag being silently coerced into a junk param the
    // command ignores, so the caller gets a successful-looking answer to a question
    // they did not ask. Measured 2026-09-05: `continuum ping --nonsense-flag`
    // returned a healthy pong, exit 0. The ladder is the point: refuse ONLY when the
    // schema is known; with no schema we cannot prove a flag is unknown, so the
    // camelCase guess must stand or every schemaless command becomes uncallable.
    // regression for #3724.
    #[test]
    fn an_unknown_flag_is_refused_by_name_only_when_the_schema_is_known() {
        let schema = &["message".to_string()];
        let err = params_from_args("ping", &["--nonsense-flag".into()], schema)
            .expect_err("a flag the schema does not have must be refused, not coerced");
        assert!(err.contains("--nonsense-flag"), "names the offending flag: {err}");
        assert!(err.contains("`ping`"), "names the command: {err}");
        assert!(err.contains("--message"), "lists what IS accepted: {err}");
        // every arg shape refuses the same way: `--k=v` and `--k v`, not just bare
        assert!(params_from_args("ping", &["--nonsense=1".into()], schema).is_err());
        assert!(params_from_args("ping", &["--nonsense".into(), "1".into()], schema).is_err());

        // A real field still parses, and still canonicalizes across separators.
        assert_eq!(
            params_from_args("ping", &["--message".into(), "hi".into()], schema).unwrap(),
            json!({ "message": "hi" })
        );

        // NO schema → cannot prove unknown → the pre-schema behaviour is preserved.
        let no_schema: &[String] = &[];
        assert_eq!(
            params_from_args("ping", &["--nonsense-flag".into()], no_schema).unwrap(),
            json!({ "nonsenseFlag": true }),
            "without a schema an unrecognised flag must NOT be refused"
        );
    }

    #[test]
    fn camel_case_normalizes_kebab_and_snake() {
        assert_eq!(to_camel_case("round-trip-ms"), "roundTripMs");
        assert_eq!(to_camel_case("round_trip_ms"), "roundTripMs");
        assert_eq!(to_camel_case("message"), "message");
        // round-trips with the display direction
        assert_eq!(camel_to_kebab("roundTripMs"), "round-trip-ms");
        assert_eq!(camel_to_kebab("message"), "message");
    }

    // what this catches: the CLI help adapter renders a command's manual in the
    // bash paradigm from the SAME schema the AI adapter reads as a tool spec — flags
    // (camelCase property → --kebab), types, descriptions, required markers. "The
    // instructions manual matches the paradigm" (Joel 2026-06-21), single source.
    #[test]
    fn help_renders_schema_as_bash_flags() {
        let info = json!({
            "name": "ping",
            "description": "Health check.",
            "paramsSchema": {
                "type": "object",
                "properties": {
                    "message": { "type": "string", "description": "Echoed back." },
                    "roundTripMs": { "type": "integer" }
                },
                "required": ["message"]
            }
        });
        let help = render_cli_help("ping", &info);
        assert!(help.contains("ping — Health check."), "header: {help}");
        assert!(help.contains("--message"), "flag from property: {help}");
        assert!(help.contains("Echoed back."), "prop description: {help}");
        assert!(help.contains("(required)"), "required marker: {help}");
        assert!(help.contains("--round-trip-ms"), "camel→kebab flag: {help}");
        assert!(help.contains("<integer>"), "type label: {help}");
    }

    // what this catches (#194): the artifact resolution ORDER is a shared contract with
    // tools/scripts/install-service.sh::resolve_core_bin — installed locations before the
    // cargo target dir (release before debug), default target dir under ~/.continuum/cache.
    // A drift here means the CLI verifies a different binary than the service installer
    // deploys — and NOTHING may resolve "next to the CLI exe" (the guess that printed
    // "could not locate continuum-core-server" while a 2-day-old core kept serving).
    #[test]
    fn artifact_resolution_order_matches_install_service() {
        let c = core_artifact_candidates("/home/u", None);
        let shown: Vec<String> = c.iter().map(|p| p.display().to_string()).collect();
        #[cfg(not(windows))]
        assert_eq!(
            shown,
            vec![
                "/usr/local/bin/continuum-core-server",
                "/home/u/.continuum/bin/continuum-core-server",
                "/home/u/.continuum/cache/cargo-target/release/continuum-core-server",
                "/home/u/.continuum/cache/cargo-target/debug/continuum-core-server",
            ],
            "order is the install-service.sh contract"
        );
        #[cfg(windows)]
        {
            assert!(shown
                .iter()
                .all(|p| p.ends_with("continuum-core-server.exe")));
            assert!(
                !shown.iter().any(|p| p.starts_with("/usr/local/bin")),
                "no unix-only install location on Windows"
            );
        }
        // explicit CARGO_TARGET_DIR overrides the default cache location
        let c = core_artifact_candidates("/home/u", Some("/tgt"));
        assert!(
            c.iter().any(|p| p.starts_with("/tgt/release"))
                && c.iter().any(|p| p.starts_with("/tgt/debug")),
            "CARGO_TARGET_DIR is honored: {c:?}"
        );
    }
}
