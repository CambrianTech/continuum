//! continuum-cli — rust CLI client, first slice (task #143).
//!
//! ## Today
//!
//! One subcommand (`metrics`) that dispatches `runtime/metrics/all` at a
//! continuum-core-server peer and pretty-prints the JSON response. The
//! purpose of THIS slice is to prove the seam: substrate-first
//! architecture, no Node middleware, no JTAG-daemon IPC dance.
//!
//! ## Tomorrow
//!
//! Every `./jtag <command>` migrates to a `ctm` subcommand that calls
//! `Connection::commands().execute()`. Each migration is a small slice —
//! we don't rewrite the world at once. As subcommands land, the Node
//! `./jtag` binary shrinks, eventually leaving only its install
//! footprint behind.
//!
//! ## Identity + peer discovery
//!
//! The CLI runs as the human's airc citizen (per
//! `[[personas-are-citizens-airc-is-identity-provider]]`). It opens the
//! user's airc home (default `$HOME/.airc`, overridable via `--home` or
//! `CONTINUUM_AIRC_HOME`) and targets the substrate's peer UUID via
//! `--peer` / `CONTINUUM_PEER_ID`. Auto-discovery of the local
//! continuum-core-server (so the operator doesn't have to type a UUID)
//! is a follow-up slice — pending an `airc ipc-endpoint`-style
//! lookup or a `~/.continuum/peer.json` discovery file.

use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use airc_lib::Airc;
use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use continuum_client::Connection;
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(
    name = "ctm",
    about = "continuum CLI — substrate client via continuum-client",
    long_about = "Run substrate commands directly against a continuum-core-server\n\
                  over airc IPC. Successor to ./jtag; same commands, no Node middleware."
)]
struct Cli {
    /// airc home directory (default: $HOME/.airc).
    #[arg(long, env = "CONTINUUM_AIRC_HOME", global = true)]
    home: Option<PathBuf>,

    /// Target substrate peer UUID. Find it via `airc status` on the
    /// machine running continuum-core-server. Required for any command
    /// that talks to the substrate — `--help` works without it.
    #[arg(long, env = "CONTINUUM_PEER_ID", global = true)]
    peer: Option<Uuid>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Fetch runtime metrics for every registered module.
    Metrics,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Honest startup probe before anything else.
    tracing_subscriber::fmt()
        .with_target(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("CONTINUUM_CLI_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let cli = Cli::parse();

    let home = match cli.home {
        Some(p) => p,
        None => default_airc_home()?,
    };

    let peer = cli.peer.ok_or_else(|| {
        anyhow!(
            "--peer is required (or set CONTINUUM_PEER_ID). Find it via `airc status` on the \
             machine running continuum-core-server."
        )
    })?;

    tracing::debug!(?home, "opening airc home");
    let airc = Airc::open(&home)
        .await
        .with_context(|| format!("open airc home at {}", home.display()))?;

    let conn = Connection::connect(Arc::new(airc), peer);

    match cli.command {
        Command::Metrics => run_metrics(conn).await,
    }
}

async fn run_metrics(
    conn: Connection<continuum_client::AircIpcTransport>,
) -> Result<()> {
    let result: serde_json::Value = conn
        .commands()
        .execute("runtime/metrics/all", serde_json::json!({}))
        .await
        .map_err(|e| anyhow!("dispatch runtime/metrics/all: {e}"))?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

/// `$HOME/.airc`, or an error if `$HOME` isn't set. Mirrors what
/// airc-lib does internally so the CLI doesn't fall back to a system
/// path the user didn't expect.
fn default_airc_home() -> Result<PathBuf> {
    let home = env::var_os("HOME")
        .ok_or_else(|| anyhow!("$HOME is unset; pass --home explicitly"))?;
    Ok(PathBuf::from(home).join(".airc"))
}
