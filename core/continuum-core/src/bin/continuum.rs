//! `continuum` — the pure-Rust Continuum CLI: the ONE surface for both lifecycle and
//! commands. Replaces the legacy Node `./jtag` and the bare start scripts.
//!
//! ```text
//! continuum start            # build + run the headless Rust core (detached), wait until ready
//!                     # refuses if a core is running but not answering — `--force` reclaims it
//! continuum reboot           # stop everything, rebuild on a free machine, relaunch
//!                     # refuses while training (mlx_lm) is live — `--force` overrides
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
//!
//! Env: `CONTINUUM_CORE_SOCKET` (default `/tmp/continuum-core.sock`),
//! `CONTINUUM_START_SCRIPT` (override the start script path).

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use continuum_client::Connection;
use continuum_core::runtime::core_bind_guard::BindDecision;
use continuum_core::runtime::core_ipc_transport::CoreIpcTransport;
use continuum_core::runtime::deploy_provenance::{
    cli_self_build, cli_staleness_note, deploy_verdict, CliSelfBuild,
};
use serde_json::Value;

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
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let first = args.next().ok_or_else(usage)?;
    // Every CLI run from inside a repo records that checkout for the core
    // (repo-card staging reads it); the first deploy after #3706 would otherwise
    // start with an empty registry until the next `start`/`reboot`.
    record_repo_checkout();
    match first.as_str() {
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
        "reboot" | "restart" => {
            let force = args.any(|a| a == "--force");
            reboot(force).await
        }
        // The typed boot plan (BOOT-IS-A-TYPED-PLAN.md, slice 1): deterministic
        // runtime bring-up of an ALREADY-BUILT binary — lane adopt-or-reap,
        // transport, core launch + #194 verify, optional Beside rails — one
        // receipt row per step. The dev-time source build stays with
        // `reboot`/the script until slice 2 migrates it.
        "boot" => {
            use continuum_core::boot_plan::Outcome;
            let mut receipt = continuum_core::boot_plan::run_before_phase();
            if !receipt.ok {
                return Err("boot plan: a REQUIRED step failed (see rows above)".into());
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
                return Err("boot plan: core launch/verify failed".into());
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
                help_for(&command).await
            } else {
                dispatch(&command, rest).await
            }
        }
    }
}

/// `continuum <command> --help` — the CLI adapter for the command's manual: query the
/// live registry (`commands/list`) for the command's description + params schema,
/// then render it as bash usage. Same single source the AI tool adapter reads;
/// only the rendering differs by paradigm ("the manual matches the paradigm").
async fn help_for(command: &str) -> Result<(), String> {
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

/// Dispatch a single command to the core through the uniform Connection, STARTING the
/// core first if nothing is answering.
///
/// Why auto-start is infrastructure and not convenience: every governed long-running
/// operation lives behind a command (`models/pull` resumes, is content-addressed,
/// journals to `~/.continuum/progress/`, emits progress on the bus). None of that is
/// reachable when the core is down, so "the core isn't running" turns into a bare
/// `nohup <downloader> &` — which has no ledger, no resume, no progress, and dies
/// silently leaving an empty directory. That has now happened to two separate
/// multi-hour model pulls (K3, then V4-Flash IQ1_S) and cost days.
///
/// The governed path must be the path of LEAST resistance or it does not get used.
/// One ping decides; `continuum start` is already idempotent and waits for ready.
/// Set `CONTINUUM_NO_AUTOSTART=1` where spawning a core is not acceptable (CI, probes)
/// — it then fails with the reason rather than silently doing nothing.
async fn dispatch(command: &str, args: Vec<String>) -> Result<(), String> {
    ensure_core_running(command).await?;
    let canonical = canonical_param_names(command).await;
    let params = params_from_args(&args, &canonical)?;
    let result = connection()
        .commands()
        .execute_value(command, params)
        .await
        .map_err(|e| format!("{command}: {e}"))?;
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
/// Flags NOT in the schema — base `CommandParams` like userId, or commands that
/// expose no schema (`canonical` empty) — fall back to the generic camelCase
/// normalization, which is correct for the camelCase wire fields and is the
/// pre-schema behavior, so nothing regresses.
///
/// Schema-AWARE coercion/validation (knowing each field's exact type) is the next
/// step on the same `commands/list` schema; this canonicalizes the KEY today.
fn params_from_args(args: &[String], canonical: &[String]) -> Result<Value, String> {
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
    let field = |raw: &str| -> String {
        let norm = normalize_key(raw);
        if let Some(c) = canon_by_norm.get(&norm) {
            return (*c).to_string();
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
                    return (*c).to_string();
                }
            }
        }
        to_camel_case(raw)
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
            map.insert(field(k), coerce(v));
            i += 1;
            continue;
        }
        // A value follows unless the next arg is another flag (or there is none).
        let has_value = args.get(i + 1).is_some_and(|n| !n.starts_with("--"));
        if has_value {
            map.insert(field(raw_key), coerce(&args[i + 1]));
            i += 2;
        } else {
            map.insert(field(raw_key), Value::Bool(true));
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

/// Make sure a core is answering before dispatching, launching one if not. See the
/// `dispatch` doc for why this is load-bearing rather than a nicety.
///
/// Announces on stderr (never stdout — stdout is the command's JSON result and stays
/// machine-parseable) so an operator who typed one command and got a 60s pause knows
/// exactly what is happening instead of assuming it hung.
async fn ensure_core_running(command: &str) -> Result<(), String> {
    // Same reclaim-or-refuse guard `start` uses, minus the reclaim: an implicit
    // autostart exists to get the caller a live core, and killing someone else's
    // core is never part of that errand. Occupied therefore always refuses here.
    match bind_decision().await {
        BindDecision::AlreadyServing { .. } => return Ok(()),
        BindDecision::Occupied { pids } => {
            let socket = socket_path();
            return Err(format!(
                "`{command}` needs a running core, and {} core process(es) are running \
                 (pid(s) {}) but NONE is answering on {}. Starting another would bind a \
                 second core on the same socket — whichever one the kernel hands a \
                 connection to would answer, so results would be non-deterministic. \
                 Either wait (a core that is still booting answers shortly), or clear it \
                 with `continuum stop`.{}",
                pids.len(),
                pids.iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
                socket,
                bound_elsewhere_hint(&running_core_sockets_for(&pids), &socket).unwrap_or_default()
            ));
        }
        BindDecision::Free => {}
    }
    // A deploy in flight is the one state the bind guard above cannot see: mid-build no
    // core answers and no core pid exists, which reads as "safe to start" and is exactly
    // when starting is wrong.
    deploy_gate(command)?;
    if std::env::var("CONTINUUM_NO_AUTOSTART").is_ok_and(|v| v != "0") {
        return Err(format!(
            "no core is answering on {} and CONTINUUM_NO_AUTOSTART is set, so `{command}` \
             cannot be dispatched. Start one with `continuum start`.",
            socket_path()
        ));
    }
    eprintln!("▶ no core running — starting one for `{command}` (continuum start)");
    let secs = launch_core(&[], LaunchSource::Installed)
        .await
        .map_err(|e| {
            format!("`{command}` needs a running core and one could not be started: {e}")
        })?;
    eprintln!("✅ core ready after ~{secs}s — dispatching `{command}`");
    Ok(())
}

/// Gather the two observations [`BindDecision`] is a function of — a real `ping`
/// round-trip and the core process table — and hand them to the shared truth table.
///
/// The observation half lives here because it is platform- and transport-shaped; the
/// DECISION half lives in the lib so it is unit-tested by the `--lib` CI gate. Both
/// `start` and `ensure_core_running` go through this one seam, which is the point:
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

    match bind_decision().await {
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
            let secs = launch_core(&pids, LaunchSource::Installed).await?;
            println!("✅ core ready (socket={socket}) after ~{secs}s");
            return Ok(());
        }
    }

    let secs = launch_core(&[], LaunchSource::Installed).await?;
    println!("✅ core ready (socket={socket}) after ~{secs}s");
    Ok(())
}

/// `continuum reboot` — stop + rebuild + relaunch the core.
/// Unlike `continuum start` this never no-ops on an up core: it runs the FULL
/// `stop` teardown first (core, split-brains, orphans, serving lanes), builds
/// the fresh binary on a machine that is no longer serving a model, then
/// launches. This is the canonical operator rebuild-after-edit verb — one
/// command, no manual kill dance ([[validate-via-pure-rust-not-npm-jtag]]).
async fn reboot(force: bool) -> Result<(), String> {
    let socket = socket_path();
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
            "benchmark run(s) in flight ({}) — a reboot kills them mid-drive; the round \
             survives and the boot resume re-fires each killed solve (rejoining its room \
             and workspace) once serving + citizens are back. Wait for them to finish, or \
             rerun with `continuum reboot --force` to take the restart-and-resume path.",
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
    } else {
        println!(
            "▶ rebooting core (socket={socket}): stopping pid(s) {} FIRST, then building on a free machine",
            old.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(",")
        );
    }
    stop_with(true).await?;
    // `launch_core`'s `wait_for_death` on `old` is now trivially satisfied —
    // kept as the honesty check that the stop actually took.
    // Publish the claim for the WHOLE build+swap. Held until this function returns, so a
    // concurrent `continuum <verb>` refuses instead of autostarting the pre-swap installed
    // image and stealing the socket (the DEPLOY MISMATCH measured 2026-08-17).
    let _deploy_claim =
        DeployClaimGuard::take(git_head_short_sha().as_deref().unwrap_or("unknown"));
    let secs = launch_core(&old, LaunchSource::FromSource).await?;
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
    let rebuilt_cli = locate_start_script().is_ok()
        && matches!(cli_self_build(std::env::consts::OS), CliSelfBuild::Rebuild);
    verify_deployed_build(rebuilt_cli).await
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
    let (expected, expected_source) = match git_head_short_sha() {
        Some(head) => (head, "git HEAD of this checkout".to_string()),
        None => {
            let artifact = resolve_core_artifact()?;
            let sha = binary_build_sha(&artifact)?;
            (sha, format!("artifact {}", artifact.display()))
        }
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
    let dirs = runtime_library_dirs(&root);
    if dirs.is_empty() {
        return;
    }
    let existing = std::env::var_os("PATH").unwrap_or_default();
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
fn binary_build_sha(artifact: &Path) -> Result<String, String> {
    let out = std::process::Command::new(artifact)
        .arg("--build-sha")
        .output()
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
    let Some(root) = continuum_core::modules::repo_registry::clone_root_from_common_dir(&common) else {
        return;
    };
    if let Some(repo) = continuum_core::modules::repo_registry::repo_id_from_remote(&url) {
        continuum_core::modules::repo_registry::record(&repo, &root);
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
fn processes_named(fragment: &str) -> Vec<i32> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    sys.processes()
        .values()
        .filter(|p| {
            p.name().to_string_lossy().contains(fragment)
                || p.exe()
                    .map(|e| e.to_string_lossy().contains(fragment))
                    .unwrap_or(false)
        })
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
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .output();
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
/// termination — is testable without live processes. The hop bound is the
/// load-bearing part: a pid table can present a CYCLE (pid reuse during a
/// racing scan, or a reparent to a descendant), and an unbounded walk would
/// hang `reboot` forever. Bounded, an unresolvable chain answers "not
/// descended", which is the safe direction only because the caller pairs it
/// with an ownership test — we never kill something we do not own.
fn descends_from(parents: &std::collections::HashMap<i32, i32>, pid: i32, keep: &[i32]) -> bool {
    const MAX_HOPS: usize = 64;
    let mut current = pid;
    for _ in 0..MAX_HOPS {
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
    let parents: std::collections::HashMap<i32, i32> = sys
        .processes()
        .values()
        .filter_map(|p| {
            p.parent()
                .map(|par| (p.pid().as_u32() as i32, par.as_u32() as i32))
        })
        .collect();

    sys.processes()
        .values()
        .filter(|p| {
            p.exe()
                .map(|exe| exe.starts_with(&owned_root))
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
    // Same enumerator as processes_named, for the same reason: one implementation beats a
    // per-OS pair where one arm shells out to a tool the other platform does not have. The old
    // Windows arm also matched the pid as a SUBSTRING of tasklist's whole output, so pid 42 read
    // as alive whenever any pid containing "42" existed.
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let target = Pid::from_u32(pid as u32);
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[target]),
        true,
        ProcessRefreshKind::nothing(),
    );
    sys.process(target).is_some()
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
enum LaunchSource {
    /// Whatever is already installed is fine — the caller wants a live core.
    Installed,
    /// Build first. The caller is deploying source they just edited.
    FromSource,
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
    /// Nothing to run.
    NoLaunchable,
}

/// Pure resolution so the policy is testable without a filesystem, a build, or a
/// process. Every branch below cost a real outage or a false success line at some
/// point; the table in the tests is the record of which.
fn plan_launch(
    policy: LaunchSource,
    env_from_source: bool,
    have_script: bool,
    have_installed: bool,
) -> LaunchPlan {
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

async fn launch_core(wait_for_death: &[i32], policy: LaunchSource) -> Result<u64, String> {
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
    let script = locate_start_script().ok();
    let server_bin = locate_core_server_binary();
    let plan = plan_launch(
        policy,
        env_from_source,
        script.is_some(),
        server_bin.is_some(),
    );

    let mut cmd = match plan {
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
            // borrow: the spawn-failure diagnostic below reports which binary it
            // tried, so `server_bin` has to outlive this arm.
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
            let mut c = std::process::Command::new(bin);
            // THE SOCKET PATH IS A POSITIONAL ARGUMENT, and this call site is the
            // only one that ever forgot it. `main.rs` requires argv[1] and exits 1
            // with its usage text when it is missing — so from the moment the
            // direct-exec path landed, every `start`/`reboot` on a machine with an
            // installed binary died in ~2s having printed "Usage:" into the start
            // log. The env var below is set too (and `endpoint_paths::core_socket`
            // now honours it server-side), but argv is the binary's documented
            // contract and is what `ps` shows an operator.
            c.arg(&socket);
            c
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
    for (k, v) in continuum_core::config_env::read_all() {
        cmd.env(k, v);
    }
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
    apply_runtime_library_path(&mut cmd);
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
    cmd.env("CONTINUUM_CORE_SOCKET", &socket)
        .stdin(Stdio::null())
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
    {
        use std::os::windows::process::CommandExt;
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
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| match &server_bin {
        Some(bin) => format!("failed to spawn the core server {}: {e}", bin.display()),
        None => format!(
            "failed to spawn the source-build start script via bash: {e}. The start script \
             is bash; on Windows that needs bash on PATH (Git Bash). Installing \
             continuum-core-server avoids the script entirely."
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
        let stalled_for = last_progress_at.elapsed().as_secs();
        if matches!(wait_verdict(stalled_for, STALL_SECS), WaitVerdict::Stalled) {
            return Err(format!(
                "the start script has emitted nothing for {stalled_for}s (still running, {}s \
                 elapsed) — it is stalled, not slow.\n{}",
                (i + 1) * TICK_SECS,
                start_log_report(&logfile)
            ));
        }
        // The start script EXITING is the fast, certain answer, and not checking for it was the
        // defect: a script that died in 200ms was indistinguishable from one still doing a
        // multi-minute cargo build, so every startup failure cost the full 300s and then reported
        // a log tail. Observed shape: 300s wait, empty log, no cause -- which is how "the core
        // never starts on this box" stayed invisible long enough to make hand-rolled downloads
        // feel like the only option.
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!(
                "the start script exited ({status}) after ~{}s without the core coming up.\n{}",
                (i + 1) * 2,
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
async fn stop() -> Result<(), String> {
    stop_with(false).await
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
async fn stop_with(keep_lanes: bool) -> Result<(), String> {
    let socket = socket_path();
    let pidfile = pidfile_for(&socket);

    let mut pidfile_core: Option<i32> = None;
    if let Ok(contents) = std::fs::read_to_string(&pidfile) {
        if let Ok(pid) = contents.trim().parse::<i32>() {
            kill_pid_tree(pid);
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
        for pid in &survivors {
            kill_pid_tree(*pid);
        }
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
    let keep: Vec<i32> = if keep_lanes {
        continuum_core::inference::lane_registry::live_lane()
            .map(|r| r.pid as i32)
            .into_iter()
            .collect()
    } else {
        Vec::new()
    };
    reap_owned_orphans(&keep);

    // Serving lanes are NOT descended from any core we just reaped (the daemon
    // spawns them detached) and are NOT under `~/.continuum/bin`, so neither the
    // tree kill nor the ownership sweep above can see them. Until this call
    // existed, `stop` left every `llama-server` running and the registry was
    // swept only on the NEXT boot — measured 2026-08-17 on the M5 as two lanes
    // resident at once (a 19 GB ephemeral 27B beside the live 14B), which
    // starved the planner into serving a 2,816-token window that cannot hold the
    // tool surface. `reboot` could not clear it either: reboot is stop + start,
    // and neither half owned lanes.
    if keep_lanes {
        println!("  leaving serving lane(s) up for adoption by the next core (reboot path)");
        let _ = std::fs::remove_file(&socket); // socket cleanup still ours — only the lane fate changed
        return Ok(());
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
    Ok(())
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
        kill_pid_tree(*pid);
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

    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
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

#[cfg(test)]
mod tests {
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

    use super::*;
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
            assert_eq!(bound_elsewhere_hint(&bound, "/tmp/continuum-core.sock"), None);
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
            assert!(hint.contains("pid 9"), "the unreachable core must appear: {hint}");
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

    /// what this catches: the actual BIGMAMA leak. llama-server pid 37148 ran
    /// 24h past its dead parent, holding 127.0.0.1:8090, so every persona that
    /// resolved a chat model through that port got an EMBEDDING model. Its
    /// parent is absent from the table entirely (dead), which must read as
    /// "orphan", not as "unknown, leave it alone".
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
        assert_eq!(params_from_args(&[], no_schema).unwrap(), json!({}));

        // 2. positional JSON verbatim (the AI / tool-call path)
        assert_eq!(
            params_from_args(&[r#"{"message":"hi"}"#.to_string()], no_schema).unwrap(),
            json!({ "message": "hi" })
        );

        // 3. --key value with coercion: string stays string, number→number,
        //    bool→bool; keys camelCased from kebab/snake.
        let p = params_from_args(
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
            params_from_args(&["--verbose".into()], no_schema).unwrap(),
            json!({ "verbose": true })
        );

        // `--key=value` form (muscle memory) — split on first `=`, NOT a junk key.
        assert_eq!(
            params_from_args(&["--filter=data/".into()], no_schema).unwrap(),
            json!({ "filter": "data/" }),
            "--key=value splits correctly (regression: was {{\"filter=data/\": true}})"
        );
        assert_eq!(
            params_from_args(&["--round-trip-ms=5".into()], no_schema).unwrap(),
            json!({ "roundTripMs": 5 }),
            "--key=value coerces + camelCases"
        );

        // a non-flag, non-JSON arg is a clear error (not silently swallowed)
        assert!(params_from_args(&["oops".into()], no_schema).is_err());
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
            let p = params_from_args(&[spelling.into(), "abc".into()], &canonical).unwrap();
            assert_eq!(p, json!({ "persona_id": "abc" }), "{spelling} → persona_id");
        }
        // `--key=value` form canonicalizes too
        assert_eq!(
            params_from_args(&["--eval-set=x.jsonl".into()], &canonical).unwrap(),
            json!({ "eval_set": "x.jsonl" })
        );
        // a flag NOT in the schema falls back to camelCase (base fields — no regression)
        assert_eq!(
            params_from_args(&["--room-id".into(), "r1".into()], &canonical).unwrap(),
            json!({ "roomId": "r1" }),
            "non-schema flag → legacy camelCase"
        );
        // with no schema at all, everything is legacy camelCase
        assert_eq!(
            params_from_args(&["--persona-id".into(), "abc".into()], &[]).unwrap(),
            json!({ "personaId": "abc" }),
            "no schema → legacy camelCase (pre-schema behavior preserved)"
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
       continuum reboot                rebuild + relaunch, replacing any running core (~0 downtime);\n                                       verifies the RUNNING core's build SHA before reporting success\n  \
       continuum stop                  stop the running core\n  \
       continuum deploy-verify         prove the running core's build SHA matches the deployed source\n\
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
