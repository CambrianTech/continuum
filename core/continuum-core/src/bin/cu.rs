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
        // Anything else is a command name → dispatch it to the running core.
        command => dispatch(&command.to_string(), args.next()).await,
    }
}

fn socket_path() -> String {
    std::env::var("CONTINUUM_CORE_SOCKET").unwrap_or_else(|_| DEFAULT_CORE_SOCKET.to_string())
}

/// Dispatch a single command to the running core through the uniform Connection.
async fn dispatch(command: &str, raw_params: Option<String>) -> Result<(), String> {
    let params: Value = match raw_params {
        Some(raw) => serde_json::from_str(&raw)
            .map_err(|e| format!("invalid JSON params: {e}\n(got: {raw})"))?,
        None => Value::Object(Default::default()),
    };
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

fn usage() -> String {
    "usage: cu <start|stop|command> [json-params]\n\
     \n\
     Lifecycle:\n  \
       cu start                 build + run the headless Rust core (detached), wait until ready\n  \
       cu stop                  stop the running core\n\
     \n\
     Commands (dispatch to the running core):\n  \
       cu ping\n  \
       cu ping '{\"message\":\"hi\"}'\n  \
       cu data/list '{\"collection\":\"users\"}'\n\
     \n\
     Env: CONTINUUM_CORE_SOCKET (default /tmp/continuum-core.sock)\n     \
          CONTINUUM_START_SCRIPT (override the start script path)"
        .to_string()
}
