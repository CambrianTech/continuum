//! `cu` — the pure-Rust Continuum CLI: the ONE surface for both lifecycle and
//! commands. Replaces the legacy Node `./jtag` and the bare start scripts.
//!
//! ```text
//! cu start            # build + run the headless Rust core (detached), wait until ready
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
        "stop" => stop().await,
        // Anything else is a command name. `--help`/`-h` renders the manual in the
        // CLI's paradigm (bash flags), adapted from the SAME schema the AI gets as
        // a tool spec. Otherwise dispatch, params adapted procedurally.
        command => {
            let rest: Vec<String> = args.collect();
            if rest.iter().any(|a| a == "--help" || a == "-h") {
                help_for(&command.to_string()).await
            } else {
                dispatch(&command.to_string(), rest).await
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
    let desc = info.get("description").and_then(|d| d.as_str()).unwrap_or("");
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
    match schema.and_then(|s| s.get("properties")).and_then(|p| p.as_object()) {
        Some(props) if !props.is_empty() => {
            out.push_str("\nParams:\n");
            for (name, spec) in props {
                let ty = schema_type_str(spec);
                let pdesc = spec.get("description").and_then(|d| d.as_str()).unwrap_or("");
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
/// `"type":["string","null"]` for optionals, or an unschematized field).
fn schema_type_str(spec: &Value) -> String {
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
    let params = params_from_args(&args)?;
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
/// Schema-AWARE coercion/validation (knowing each field's exact type) lands when
/// the registry exposes param JSON schemas via `commands/list`; until then this
/// generic coercion covers the common cases without any per-command code.
fn params_from_args(args: &[String]) -> Result<Value, String> {
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

    let mut map = serde_json::Map::new();
    let mut i = 0;
    while i < args.len() {
        let raw_key = args[i].strip_prefix("--").ok_or_else(|| {
            format!(
                "expected `--key value` or a single JSON object, got `{}`",
                args[i]
            )
        })?;
        let key = to_camel_case(raw_key);
        // A value follows unless the next arg is another flag (or there is none).
        let has_value = args.get(i + 1).is_some_and(|n| !n.starts_with("--"));
        if has_value {
            let raw = &args[i + 1];
            let val = serde_json::from_str::<Value>(raw).unwrap_or_else(|_| Value::String(raw.clone()));
            map.insert(key, val);
            i += 2;
        } else {
            map.insert(key, Value::Bool(true));
            i += 1;
        }
    }
    Ok(Value::Object(map))
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
    // SAFETY: setsid() in the forked child before exec — detaches from cu's
    // session/controlling terminal so the core outlives this CLI invocation.
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let child = cmd
        .spawn()
        .map_err(|e| format!("failed to spawn {}: {e}", script.display()))?;

    // Record the PID so `cu stop` can find the detached process group.
    let pidfile = pidfile_for(&socket);
    let _ = std::fs::write(&pidfile, child.id().to_string());

    // Wait until the core answers ping. The first build (cargo) can take minutes;
    // poll generously, then fail loud with the log tail rather than hang forever.
    for i in 0..150 {
        tokio::time::sleep(Duration::from_secs(2)).await;
        if core_is_up().await {
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
            // Signal the whole process group (negative pid) — the start script's
            // setsid made the core a group leader, so this reaps cargo + the core.
            unsafe {
                libc::kill(-pid, libc::SIGTERM);
                libc::kill(pid, libc::SIGTERM);
            }
            stopped = true;
            println!("sent SIGTERM to core (pid {pid})");
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
        // 1. nothing → empty object
        assert_eq!(params_from_args(&[]).unwrap(), json!({}));

        // 2. positional JSON verbatim (the AI / tool-call path)
        assert_eq!(
            params_from_args(&[r#"{"message":"hi"}"#.to_string()]).unwrap(),
            json!({ "message": "hi" })
        );

        // 3. --key value with coercion: string stays string, number→number,
        //    bool→bool; keys camelCased from kebab/snake.
        let p = params_from_args(&[
            "--message".into(),
            "hi".into(),
            "--round-trip-ms".into(),
            "5".into(),
            "--enabled".into(),
            "true".into(),
        ])
        .unwrap();
        assert_eq!(
            p,
            json!({ "message": "hi", "roundTripMs": 5, "enabled": true }),
            "coerced + camelCased automatically, one generic rule"
        );

        // bare flag (no value) → true
        assert_eq!(
            params_from_args(&["--verbose".into()]).unwrap(),
            json!({ "verbose": true })
        );

        // a non-flag, non-JSON arg is a clear error (not silently swallowed)
        assert!(params_from_args(&["oops".into()]).is_err());
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
    "usage: cu <start|stop|command> [json | --key value ...]\n\
     \n\
     Lifecycle:\n  \
       cu start                 build + run the headless Rust core (detached), wait until ready\n  \
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
