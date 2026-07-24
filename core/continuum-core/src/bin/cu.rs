//! `cu` — the pure-Rust Continuum CLI: the ONE surface for both lifecycle and
//! commands. Replaces the legacy Node `./jtag` and the bare start scripts.
//!
//! ```text
//! cu start            # build + run the headless Rust core (detached), wait until ready
//! cu reboot           # rebuild + relaunch, replacing any running core (~0 downtime)
//!                     # refuses while training (mlx_lm) is live — `--force` overrides
//! cu stop             # stop the running core
//! cu ping             # dispatch a command to the running core
//! cu ping '{"message":"hi"}'
//! cu data/list '{"collection":"users"}'
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
use continuum_core::runtime::core_ipc_transport::CoreIpcTransport;
use serde_json::Value;

const DEFAULT_CORE_SOCKET: &str = "/tmp/continuum-core.sock";
/// Where `cu start` records the detached core's PID so `cu stop` can find it.
fn pidfile_for(socket: &str) -> String {
    format!("{socket}.pid")
}
fn start_logfile() -> String {
    "/tmp/continuum-core-start.log".to_string()
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("cu: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let first = args.next().ok_or_else(usage)?;
    match first.as_str() {
        "-h" | "--help" | "help" => {
            eprintln!("{}", usage());
            Ok(())
        }
        "start" => start().await,
        "reboot" | "restart" => {
            let force = args.any(|a| a == "--force");
            reboot(force).await
        }
        "stop" => stop().await,
        // Anything else is a command name. `--help`/`-h` renders the manual in the
        // CLI's paradigm (bash flags), adapted from the SAME schema the AI gets as
        // a tool spec. Otherwise dispatch, params adapted procedurally.
        command => {
            // Meet the operator's dialect: `cu read_file ...` / `cu code_read ...`
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

/// `cu <command> --help` — the CLI adapter for the command's manual: query the
/// live registry (`commands/list`) for the command's description + params schema,
/// then render it as bash usage. Same single source the AI tool adapter reads;
/// only the rendering differs by paradigm ("the manual matches the paradigm").
async fn help_for(command: &str) -> Result<(), String> {
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
        .ok_or_else(|| format!("unknown command `{command}` (try: cu commands/list)"))?;
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
        "Usage: cu {command} [--flag value ...]   (or a single JSON object)\n"
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

/// camelCase → kebab-case for display (`roundTripMs` → `round-trip-ms`). cu's
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
    std::env::var("CONTINUUM_CORE_SOCKET").unwrap_or_else(|_| DEFAULT_CORE_SOCKET.to_string())
}

/// Dispatch a single command to the running core through the uniform Connection.
async fn dispatch(command: &str, args: Vec<String>) -> Result<(), String> {
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
/// flag (regression: cu used to blanket-camelCase every key, turning `--persona_id`
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
    let field = |raw: &str| -> String {
        canon_by_norm
            .get(&normalize_key(raw))
            .map(|c| (*c).to_string())
            .unwrap_or_else(|| to_camel_case(raw))
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

/// Is a core already answering on the socket? A real ping round-trip, not just a
/// socket-file existence check (a stale socket file lies).
async fn core_is_up() -> bool {
    matches!(
        connection()
            .commands()
            .execute_value("ping", Value::Object(Default::default()))
            .await,
        Ok(_)
    )
}

/// `cu start` — build + run the headless Rust core (detached), wait until it
/// answers `ping`. Idempotent: a no-op if a core is already up.
async fn start() -> Result<(), String> {
    let socket = socket_path();

    if core_is_up().await {
        println!("core already running (socket={socket})");
        return Ok(());
    }

    launch_core(&[]).await
}

/// `cu reboot` — rebuild + relaunch the core, replacing any running instance.
/// Unlike `cu start` this never no-ops on an up core: start-server.sh builds the
/// fresh binary first (old core keeps serving), then stops the old core and
/// execs the new one (~0 downtime). This is the canonical operator
/// rebuild-after-edit verb — one command, no manual kill dance
/// ([[validate-via-pure-rust-not-npm-jtag]]).
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
             with `cu reboot --force` if losing the run is acceptable.",
            trainers
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(",")
        ));
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
    // Snapshot the running core PIDs up front so launch_core can wait for them to
    // actually exit before trusting the new core's ping (same socket, both answer).
    let old = running_core_pids();
    if old.is_empty() {
        println!("▶ no core running — starting fresh (socket={socket})");
    } else {
        println!(
            "▶ rebooting core (socket={socket}, replacing pid(s) {}) — building fresh binary, then swapping",
            old.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(",")
        );
    }
    launch_core(&old).await
}

/// PIDs of live training runs (`mlx_lm` trainers spawned by the MLX adapter) —
/// the reboot guard's evidence. Same pgrep shape as [`running_core_pids`].
fn running_trainer_pids() -> Vec<i32> {
    pgrep("mlx_lm")
}

/// PIDs of every running `continuum-core-server`, via `pgrep` (pure unix, no
/// Node). Empty on no match or if pgrep is unavailable.
fn running_core_pids() -> Vec<i32> {
    pgrep("continuum-core-server")
}

fn pgrep(pattern: &str) -> Vec<i32> {
    std::process::Command::new("pgrep")
        .args(["-f", pattern])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter_map(|l| l.trim().parse::<i32>().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// True if `pid` is still alive. Unix: signal 0 is the canonical liveness probe.
/// Windows has no signals — query the task list for the pid.
fn pid_alive(pid: i32) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: kill(pid, 0) sends no signal; it only checks existence/permission.
        unsafe { libc::kill(pid, 0) == 0 }
    }
    #[cfg(windows)]
    {
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
}

/// Spawn the pure-Rust start script detached and wait until the core answers
/// `ping`. Shared by `cu start` (after an up-check) and `cu reboot` (always).
///
/// `wait_for_death` is the set of core PIDs that must EXIT before we trust the
/// ping. Without it, `cu reboot` would see the OLD core still answering on the
/// same socket and falsely report "ready" before the swap happened — a fail-loud
/// violation that would also hide a failed rebuild. `cu start` passes `&[]`.
async fn launch_core(wait_for_death: &[i32]) -> Result<(), String> {
    let socket = socket_path();
    let script = locate_start_script()?;
    let logfile = start_logfile();
    let log = std::fs::File::create(&logfile)
        .map_err(|e| format!("cannot open start log {logfile}: {e}"))?;
    let log_err = log
        .try_clone()
        .map_err(|e| format!("cannot clone start log handle: {e}"))?;

    println!("▶ starting core via {} (log: {logfile})", script.display());

    // Spawn the pure-Rust start script in its OWN session (setsid) so it survives
    // `cu` exiting — a detached daemon, not a child tied to this process.
    let mut cmd = std::process::Command::new("bash");
    cmd.arg(&script)
        .env("CONTINUUM_CORE_SOCKET", &socket)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err));
    // Detach so the core outlives this CLI invocation. Unix: setsid() in the
    // forked child before exec, off cu's session/controlling terminal. Windows:
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
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
    }
    let child = cmd
        .spawn()
        .map_err(|e| format!("failed to spawn {}: {e}", script.display()))?;

    // Record the PID so `cu stop` can find the detached process group.
    let pidfile = pidfile_for(&socket);
    let _ = std::fs::write(&pidfile, child.id().to_string());

    // Wait until the core answers ping AND every old core PID has exited. The
    // first build (cargo) can take minutes; poll generously, then fail loud with
    // the log tail rather than hang forever. The death-check is what makes a
    // reboot's success signal honest: the ping must come from the NEW core.
    for i in 0..150 {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let old_still_alive = wait_for_death.iter().any(|p| pid_alive(*p));
        if !old_still_alive && core_is_up().await {
            println!("✅ core ready (socket={socket}) after ~{}s", (i + 1) * 2);
            return Ok(());
        }
    }
    Err(format!(
        "core did not become ready within 300s. Last log lines:\n{}",
        tail(&logfile, 20)
    ))
}

/// `cu stop` — stop the running core (the detached session started by `cu start`).
async fn stop() -> Result<(), String> {
    let socket = socket_path();
    let pidfile = pidfile_for(&socket);

    let mut stopped = false;
    if let Ok(contents) = std::fs::read_to_string(&pidfile) {
        if let Ok(pid) = contents.trim().parse::<i32>() {
            // Reap the core + its children. Unix: signal the whole process group
            // (negative pid) — the start script's setsid made the core a group
            // leader. Windows: taskkill /T kills the process tree.
            #[cfg(unix)]
            unsafe {
                libc::kill(-pid, libc::SIGTERM);
                libc::kill(pid, libc::SIGTERM);
            }
            #[cfg(windows)]
            {
                let _ = std::process::Command::new("taskkill")
                    .args(["/F", "/T", "/PID", &pid.to_string()])
                    .output();
            }
            stopped = true;
            println!("stopping core (pid {pid})");
        }
        let _ = std::fs::remove_file(&pidfile);
    }

    if !stopped {
        // No pidfile (started another way). Fall back to a targeted pkill — still
        // pure unix, no Node.
        let killed = std::process::Command::new("pkill")
            .args(["-f", "continuum-core-server"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if killed {
            println!("stopped continuum-core-server (pkill)");
        } else {
            println!("no running core found");
        }
    }

    let _ = std::fs::remove_file(&socket);
    Ok(())
}

/// Find `tools/scripts/start-server.sh`: an explicit `CONTINUUM_START_SCRIPT`
/// override, else walk up from the cwd until the repo's script is found.
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
                 cwd. Run `cu start` from inside the repo, or set CONTINUUM_START_SCRIPT."
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
    use super::*;
    use serde_json::json;

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
    // `persona_id`) must be invokable by flag. cu used to blanket-camelCase every key,
    // so `--persona_id` became `personaId` and the server rejected it with
    // `missing field persona_id`. With the command's schema known, any spelling of a
    // schema field canonicalizes to the exact field name; flags NOT in the schema
    // (base fields, schemaless commands) keep the legacy camelCase normalization.
    // regression for the 2026-06-25 cu flag bug.
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
}

fn usage() -> String {
    "usage: cu <start|reboot|stop|command> [json | --key value ...]\n\
     \n\
     Lifecycle:\n  \
       cu start                 build + run the headless Rust core (detached), wait until ready\n  \
       cu reboot                rebuild + relaunch, replacing any running core (~0 downtime)\n  \
       cu stop                  stop the running core\n\
     \n\
     Commands (dispatch to the running core):\n  \
       cu ping\n  \
       cu ping --message hi                 # --key value, coerced + camelCased automatically\n  \
       cu ping '{\"message\":\"hi\"}'           # or a single JSON object (AI / power-user path)\n  \
       cu commands/list                     # discover commands dynamically (single source)\n  \
       cu commands/list --filter data/\n\
     \n\
     Env: CONTINUUM_CORE_SOCKET (default /tmp/continuum-core.sock)\n     \
          CONTINUUM_START_SCRIPT (override the start script path)"
        .to_string()
}
