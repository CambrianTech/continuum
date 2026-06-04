//! claude_join_demo — proves Slice 3 of #142 end-to-end.
//!
//! A Claude Code session boots a `ClaudeContext`, gets its OWN
//! airc identity (keypair under `~/.continuum/claudes/<label>/airc/`),
//! joins the operator's current room, posts a single "Claude
//! entered the grid" message signed by THAT keypair, and exits.
//!
//! After running, the operator can verify via `airc inbox` that the
//! message was authored by a DIFFERENT peer_id than the host's —
//! the visible payoff of the Identity + Context + Slice-1B work:
//! every actor instance now has its own substrate identity, not the
//! host's.
//!
//! ## Run
//!
//! ```bash
//! cargo run --bin claude_join_demo --features metal,accelerate
//! ```
//!
//! Env vars:
//! - `CONTINUUM_ROOT` — defaults to `~/.continuum`
//! - `CONTINUUM_CLAUDE_LABEL` — instance label (default
//!   `"default"`). Different labels produce different identities;
//!   same label across runs resumes the same keypair.
//! - `CONTINUUM_ROOM` — room to join (default `"continuum"`)

use continuum_core::airc::{discover_airc_socket, discover_default_channel};
use continuum_core::context::{ClaudeContext, ClaudeMetadata, Context};
use std::path::PathBuf;

fn continuum_root() -> PathBuf {
    if let Ok(root) = std::env::var("CONTINUUM_ROOT") {
        return PathBuf::from(root);
    }
    dirs::home_dir()
        .expect("home directory")
        .join(".continuum")
}

fn instance_label() -> String {
    std::env::var("CONTINUUM_CLAUDE_LABEL").unwrap_or_else(|_| "default".to_string())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .with_writer(std::io::stderr)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    println!("=== claude_join_demo ===");
    println!("Proving Slice 3 of #142: Claude as a first-class substrate citizen.");
    println!();

    // 1. Discover the airc daemon + the operator's current room.
    let socket_path = match discover_airc_socket().await {
        Ok(p) => p,
        Err(e) => {
            println!("⚠️  Cannot reach the airc daemon: {e}");
            println!("    Remedy: install + run `airc join`.");
            return Ok(());
        }
    };
    println!("✓ airc daemon discovered at {}", socket_path.display());

    let default_channel = match discover_default_channel().await {
        Ok(uuid) => uuid,
        Err(e) => {
            println!("⚠️  Cannot determine default room: {e}");
            println!("    Remedy: run `airc room <name>`.");
            return Ok(());
        }
    };
    println!("✓ default channel resolved: {default_channel}");

    // 2. Bootstrap the Claude context. ClaudeContext::bootstrap does
    //    the home-mkdir + airc-lib attach_as + Identity construction.
    let root = continuum_root();
    let label = instance_label();
    let ctx = match ClaudeContext::bootstrap(
        &root,
        &label,
        socket_path.clone(),
        default_channel,
        ClaudeMetadata {
            model_id: Some("claude-opus-4-7".to_string()),
        },
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            println!("⚠️  ClaudeContext::bootstrap failed: {e}");
            return Ok(());
        }
    };
    let identity = ctx.identity();
    println!(
        "✓ ClaudeContext ready: agent_name={} peer_id={} kind={:?} source={:?}",
        identity.agent_name, identity.id, identity.kind, identity.source
    );

    // 3. Join the operator's room. Same name-derives-channel
    //    discipline as airc_chat_demo — join by NAME, not by UUID-
    //    as-string, so the channel matches what `airc room` reports.
    //    Reach the join through the underlying Arc<Airc> via the
    //    citizen's say/subscribe — the demo just needs to land a
    //    message in the same channel the operator's reading.
    //
    //    Note: `Airc::join` lives on `airc_lib::Airc`, not on the
    //    AircCitizen trait surface. For a polished bootstrap path
    //    we'd plumb a `join(&str)` method through; for THIS demo we
    //    rely on the fact that airc-lib's `attach_as` already
    //    associates the daemon with the home's default channel for
    //    publish purposes — the operator's room name resolves via
    //    daemon-side state set by `airc room`.
    //
    //    A polished follow-up adds an explicit room.join() through
    //    the Context trait so this isn't bin-specific knowledge.

    let claude_short: String = identity.id.to_string().chars().take(8).collect();
    let message = format!(
        "Claude-Opus-4.7-{ts} entered the grid as {short} (peer_id {full})",
        ts = now_ms(),
        short = claude_short,
        full = identity.id,
    );

    println!();
    println!("Publishing: {message}");

    match ctx.airc().say(&message).await {
        Ok(event_id) => {
            println!("✓ posted event_id={event_id}");
            println!();
            println!("Verify: `airc inbox --limit 5` should show the message authored");
            println!("by peer_id={} — a DIFFERENT peer_id than the host's.", identity.id);
        }
        Err(e) => {
            println!("⚠️  say failed: {e}");
        }
    }

    Ok(())
}
