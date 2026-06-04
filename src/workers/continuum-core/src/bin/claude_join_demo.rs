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

use continuum_core::airc::{
    discover_airc_socket, discover_default_channel, discover_default_room_name,
};
use continuum_core::context::{ClaudeContext, ClaudeMetadata, Context};
use std::path::PathBuf;
use std::process;

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
            eprintln!("⚠️  Cannot reach the airc daemon: {e}");
            eprintln!("    Remedy: install + run `airc join`.");
            process::exit(2);
        }
    };
    println!("✓ airc daemon discovered at {}", socket_path.display());

    let default_channel = match discover_default_channel().await {
        Ok(uuid) => uuid,
        Err(e) => {
            eprintln!("⚠️  Cannot determine default room: {e}");
            eprintln!("    Remedy: run `airc room <name>`.");
            process::exit(2);
        }
    };
    println!("✓ default channel resolved: {default_channel}");

    let env_room = std::env::var("CONTINUUM_ROOM").ok();
    let room_name = match env_room {
        Some(name) => name,
        None => match discover_default_room_name().await {
            Ok(name) => name,
            Err(e) => {
                eprintln!("⚠️  Cannot determine default room name: {e}");
                eprintln!("    Remedy: run `airc room <name>` so the daemon knows the canonical name,");
                eprintln!("            or set CONTINUUM_ROOM=<name> explicitly.");
                process::exit(2);
            }
        },
    };
    println!("✓ room name resolved: {room_name}");

    // 2. Bootstrap the Claude context. ClaudeContext::bootstrap does
    //    the home-mkdir + airc-lib attach_as + Airc::join (by NAME,
    //    not UUID-as-string, per the recurring hazard documented in
    //    PersonaAircRuntime::bootstrap) + Identity construction.
    let root = continuum_root();
    let label = instance_label();
    let ctx = match ClaudeContext::bootstrap(
        &root,
        &label,
        socket_path.clone(),
        default_channel,
        Some(&room_name),
        ClaudeMetadata {
            model_id: Some("claude-opus-4-7".to_string()),
        },
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("⚠️  ClaudeContext::bootstrap failed: {e}");
            process::exit(2);
        }
    };
    let identity = ctx.identity();
    println!(
        "✓ ClaudeContext ready: agent_name={} peer_id={} kind={:?} source={:?}",
        identity.agent_name, identity.id, identity.kind, identity.source
    );

    // 3. Bootstrap already joined the room by NAME (correct channel
    //    derivation). Compose + post the message via the Context's
    //    airc handle.

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
            Ok(())
        }
        Err(e) => {
            eprintln!("⚠️  say failed: {e}");
            process::exit(2);
        }
    }
}
