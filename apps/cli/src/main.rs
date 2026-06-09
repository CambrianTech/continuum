//! continuum-cli — rust CLI client (task #143).
//!
//! ## Subcommands
//!
//! - `metrics`    — `runtime/metrics/all` against the substrate, pretty-printed.
//! - `generate`   — `ai/generate` against the substrate, prints the response text.
//! - `grid-smoke` — coverage battery across grid-shipped substrate commands.
//!                   Single-hop in v1; multi-hop composition in v2.
//!
//! Every subcommand dispatches a substrate command via `Connection` /
//! `CommandClient`. Same wire path the `airc_ipc_roundtrip` integration
//! test pins.
//!
//! ## Identity + peer discovery
//!
//! The CLI runs as the human's airc citizen (per
//! `[[personas-are-citizens-airc-is-identity-provider]]`). It opens the
//! user's airc home (default `$HOME/.airc`, overridable via `--home` or
//! `CONTINUUM_AIRC_HOME`) and targets the substrate's peer UUID via
//! `--peer` / `CONTINUUM_PEER_ID`. Auto-discovery is pending — see
//! task #143 follow-ups.

use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use airc_lib::Airc;
use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use continuum_client::{AircIpcTransport, Connection};
use uuid::Uuid;

mod grid_smoke;

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

    /// Run inference at the substrate. Dispatches `ai/generate` with the
    /// supplied prompt as a single user message; prints the response text
    /// (or the full JSON via --json).
    ///
    /// If the substrate has an AircRemoteInferenceAdapter registered,
    /// the inference will transparently run on a remote peer (e.g., the
    /// operator's 5090); the CLI doesn't know or care.
    Generate {
        /// User-side prompt. Becomes a single user message in the
        /// TextGenerationRequest.
        #[arg(long)]
        prompt: String,

        /// Model name to dispatch to. Optional; the substrate's adapter
        /// selector picks a default when omitted.
        #[arg(long)]
        model: Option<String>,

        /// Print the raw JSON response instead of just the text field.
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Run the grid-smoke battery against the target peer. Dispatches
    /// each spec in `grid_smoke::default_battery()` and reports
    /// pass/fail + wall-clock latency per row. Exits nonzero if any
    /// row fails, so CI / scripts can gate on the report directly.
    ///
    /// v1 is single-hop only (caller → target peer → caller).
    /// Multi-hop composition (M → A → B → C), fan-out, and mixed-
    /// modality chains land in v2 when probe-sink trace ingestion is
    /// wired.
    GridSmoke,
}

// CLI is a one-shot binary: parse args, open airc, fire one command,
// exit. The `current_thread` flavor avoids spinning N worker threads for
// a single round-trip (R1 follow-up from PR #1559 review).
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
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
        Command::Generate { prompt, model, json } => run_generate(conn, prompt, model, json).await,
        Command::GridSmoke => grid_smoke::run(conn, peer).await,
    }
}

async fn run_metrics(conn: Connection<AircIpcTransport>) -> Result<()> {
    let result: serde_json::Value = conn
        .commands()
        .execute("runtime/metrics/all", serde_json::json!({}))
        .await
        .map_err(|e| anyhow!("dispatch runtime/metrics/all: {e}"))?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

async fn run_generate(
    conn: Connection<AircIpcTransport>,
    prompt: String,
    model: Option<String>,
    json: bool,
) -> Result<()> {
    // Construct the minimum-viable TextGenerationRequest shape. The
    // substrate's `ai/generate` handler accepts a JSON object matching
    // continuum-core::ai::types::TextGenerationRequest (camelCase). We
    // intentionally build the JSON inline so the CLI doesn't need to
    // dev-depend on continuum-core types — only the wire shape.
    //
    // ChatMessage.content is `#[serde(untagged)] enum MessageContent {
    //   Text(String), Parts(Vec<ContentPart>) }`. Pass the prompt as a
    //   plain string so it matches the `Text(String)` arm. The
    //   substrate's parse_request handles either shape; the string
    //   form is what its own legacy `prompt`-param path produces, so
    //   it's already exercised end-to-end and the safest wire choice.
    let mut params = serde_json::json!({
        "messages": [
            {
                "role": "user",
                "content": prompt,
            }
        ],
    });
    if let Some(m) = model {
        params["model"] = serde_json::Value::String(m);
    }

    let result: serde_json::Value = conn
        .commands()
        .execute("ai/generate", params)
        .await
        .map_err(|e| anyhow!("dispatch ai/generate: {e}"))?;

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        // Pretty-print the response text + a sparse footer with
        // model/provider/usage so the operator can see WHO answered.
        // Per [[no-fallbacks-ever]] + R2 review on PR #1561: every
        // field below is REQUIRED on TextGenerationResponse (`text:
        // String`, `model: String`, `provider: String`, `usage:
        // UsageMetrics` — non-Option in the substrate's typed
        // definition). A defensive `unwrap_or` here would silently
        // mask a substrate-side contract violation as a fake string;
        // surface a typed error instead so substrate bugs get caught
        // loudly.
        let text = result
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!(
                "substrate `ai/generate` response missing required `text` field — \
                 substrate-side contract violation, not a CLI presentation problem"
            ))?;
        println!("{text}");
        let model = result.get("model").and_then(|v| v.as_str()).ok_or_else(|| {
            anyhow!("substrate response missing required `model` field")
        })?;
        let provider = result.get("provider").and_then(|v| v.as_str()).ok_or_else(|| {
            anyhow!("substrate response missing required `provider` field")
        })?;
        let total = result
            .get("usage")
            .and_then(|u| u.get("totalTokens"))
            .and_then(|v| v.as_u64())
            .ok_or_else(|| {
                anyhow!("substrate response missing required `usage.totalTokens` field")
            })?;
        eprintln!("\n--- model={model} provider={provider} total_tokens={total} ---");
    }
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
