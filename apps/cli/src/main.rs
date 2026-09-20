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

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use continuum_client::{attach_local_substrate, AircIpcTransport, Connection};
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

    /// Target substrate peer UUID. Defaults to the LOCAL substrate,
    /// auto-discovered from the airc daemon's Status RPC. Pass
    /// explicitly (or set CONTINUUM_PEER_ID) to target a REMOTE grid
    /// peer instead.
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
    /// the inference will transparently run on a remote peer (the
    /// operator's GPU-rich grid host, for example); the CLI doesn't
    /// know or care.
    Generate {
        /// User-side prompt. Becomes a single user message in the
        /// TextGenerationRequest.
        #[arg(long)]
        prompt: String,

        /// Model name to dispatch to. Optional — but the substrate's
        /// selector refuses a request with NEITHER model NOR provider
        /// ([[no-fallbacks-ever]]: no silent default), so pass this or
        /// `--provider`. A model-only request must match an adapter's
        /// registered model prefix on the target.
        #[arg(long)]
        model: Option<String>,
        /// Provider id to route to (e.g. `llama-server`, `docker-model-runner`,
        /// `anthropic`). The substrate's `AdapterRegistry::select` accepts an
        /// explicit provider with NO model and hard-refuses model-only requests
        /// whose name matches no adapter prefix ([[no-fallbacks-ever]]) — so a
        /// grid consumer with no local registry needs this to make the one
        /// request a remote substrate will serve. `local` is the sentinel for
        /// "best local GPU adapter on the target". Card 94179b72.
        #[arg(long, env = "CONTINUUM_PROVIDER")]
        provider: Option<String>,

        /// Cap on generated tokens (`maxTokens` on the request). Without it the
        /// lane decides, and a measurement that asked for "one word" can get
        /// 124 tokens — wall-clock then measures decode length, not the wire.
        /// Card ddd7a7cf.
        #[arg(long)]
        max_tokens: Option<u32>,

        /// Sampling temperature (`temperature` on the request). `0` makes
        /// repeated measurement runs comparable.
        #[arg(long)]
        temperature: Option<f32>,

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

    // Attach through the RUNNING airc daemon — `Airc::open` is owner-mode
    // (no daemon, no routes: every dispatch dies at the command deadline;
    // the 2026-08-27 grid-smoke 0/3 was exactly that). Socket + local
    // substrate peer are auto-discovered; --peer overrides for remote
    // grid targets.
    tracing::debug!(?home, "attaching to local substrate");
    let attachment = attach_local_substrate(home.clone(), "ctm", cli.peer)
        .await
        .with_context(|| format!("attach to substrate from airc home {}", home.display()))?;
    let peer = attachment.substrate_peer;

    // 120s deadline: the default 30s fits control commands but not a real
    // generation on a contended lane (grid-smoke's ai/generate row measured
    // 10s for 16 tokens; 128 tokens blew 30s). Refusals still return in ms —
    // the deadline is only the ceiling.
    let conn = Connection::new(
        AircIpcTransport::new(attachment.airc, peer)
            .with_deadline(std::time::Duration::from_secs(120)),
    );

    match cli.command {
        Command::Metrics => run_metrics(conn).await,
        Command::Generate {
            prompt,
            model,
            provider,
            max_tokens,
            temperature,
            json,
        } => run_generate(conn, prompt, model, provider, max_tokens, temperature, json).await,
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
    provider: Option<String>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
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
    if let Some(p) = provider {
        params["provider"] = serde_json::Value::String(p);
    }
    if let Some(n) = max_tokens {
        params["maxTokens"] = serde_json::Value::from(n);
    }
    if let Some(t) = temperature {
        params["temperature"] = serde_json::Value::from(t);
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
        let text = result.get("text").and_then(|v| v.as_str()).ok_or_else(|| {
            anyhow!(
                "substrate `ai/generate` response missing required `text` field — \
                 substrate-side contract violation, not a CLI presentation problem"
            )
        })?;
        println!("{text}");
        let model = result
            .get("model")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("substrate response missing required `model` field"))?;
        let provider = result
            .get("provider")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("substrate response missing required `provider` field"))?;
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
    let home =
        env::var_os("HOME").ok_or_else(|| anyhow!("$HOME is unset; pass --home explicitly"))?;
    // ctm gets its OWN scope, never the operator's `~/.airc`. Two reasons,
    // both learned the hard way (2026-08-27 grid-smoke 0/3): a shared home
    // would inherit the operator's CURRENT ROOM — request/reply frames are
    // stamped with the sender's current-room channel, so a scope parked in
    // #academy talks past a substrate living in #general and every dispatch
    // deadlines; and steering the shared scope's room to fix that would
    // mutate the operator's own airc state. A fresh dedicated scope lands
    // in #general (the substrate's commons) by airc's own default.
    Ok(PathBuf::from(home)
        .join(".continuum")
        .join("ctm")
        .join("airc"))
}
