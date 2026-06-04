//! agent_join_demo — proves Slice 4 of #142 end-to-end.
//!
//! An external AI agent session (Claude / Codex / Gemini / future
//! provider) boots an `AgentContext`, gets its OWN airc identity
//! (keypair under
//! `~/.continuum/citizens/agents/<provider>/<label>/airc/`), joins
//! the operator's current room, posts a "agent entered the grid"
//! message signed by THAT keypair, and exits.
//!
//! After running, the operator verifies via `airc inbox` that the
//! message was authored by a DIFFERENT peer_id than the host's —
//! the visible payoff of the Identity + Context + Slice-1B + Slice-4
//! work: every actor instance has its own substrate identity in a
//! symmetric directory layout.
//!
//! ## Run
//!
//! ```bash
//! # Default: provider=claude, label=default
//! cargo run --bin agent_join_demo --features metal,accelerate
//!
//! # Codex session:
//! CONTINUUM_AGENT_PROVIDER=codex cargo run --bin agent_join_demo
//!
//! # Gemini session named "joel-mac":
//! CONTINUUM_AGENT_PROVIDER=gemini CONTINUUM_AGENT_LABEL=joel-mac \
//!   cargo run --bin agent_join_demo
//! ```
//!
//! Env vars:
//! - `CONTINUUM_ROOT` — defaults to `~/.continuum`
//! - `CONTINUUM_AGENT_PROVIDER` — provider slug (default `"claude"`).
//!   Lowercase. Carried as the `agent_provider` field on the
//!   Identity row.
//! - `CONTINUUM_AGENT_LABEL` — instance label (default `"default"`).
//!   Different labels produce different identities; same label
//!   across runs resumes.
//! - `CONTINUUM_ROOM` — room to join. If unset, discovered from the
//!   airc daemon's current default-room name.

use continuum_core::airc::{
    discover_airc_socket, discover_default_channel, discover_default_room_name,
};
use continuum_core::context::{AgentContext, AgentMetadata, Context};
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

fn agent_provider() -> String {
    std::env::var("CONTINUUM_AGENT_PROVIDER").unwrap_or_else(|_| "claude".to_string())
}

fn instance_label() -> String {
    std::env::var("CONTINUUM_AGENT_LABEL").unwrap_or_else(|_| "default".to_string())
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

    println!("=== agent_join_demo ===");
    println!(
        "Proving Slice 4 of #142: external AI agents (Claude / Codex / Gemini / ...) as"
    );
    println!("first-class substrate citizens under the symmetric citizens/agents layout.");
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

    // 2. Bootstrap the Agent context. AgentContext::bootstrap does
    //    the home-mkdir + airc-lib attach_as + Airc::join (by NAME,
    //    not UUID-as-string, per the recurring hazard documented in
    //    PersonaAircRuntime::bootstrap) + Identity construction.
    let root = continuum_root();
    let provider = agent_provider();
    let label = instance_label();
    let model_id = match provider.as_str() {
        "claude" => Some("claude-opus-4-7".to_string()),
        "codex" => Some("gpt-codex".to_string()),
        "gemini" => Some("gemini-2.5".to_string()),
        _ => None,
    };
    let ctx = match AgentContext::bootstrap(
        &root,
        &provider,
        &label,
        socket_path.clone(),
        default_channel,
        Some(&room_name),
        AgentMetadata { model_id },
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("⚠️  AgentContext::bootstrap failed: {e}");
            process::exit(2);
        }
    };
    let identity = ctx.identity();
    println!(
        "✓ AgentContext ready: provider={} agent_name={} peer_id={} kind={:?} source={:?}",
        identity.agent_provider.as_deref().unwrap_or("?"),
        identity.agent_name,
        identity.id,
        identity.kind,
        identity.source
    );

    // 3. Bootstrap already joined the room by NAME (correct channel
    //    derivation). Compose + post the message via the Context's
    //    airc handle.

    let peer_short: String = identity.id.to_string().chars().take(8).collect();
    let message = format!(
        "{provider}-{label}-{ts} entered the grid as {short} (peer_id {full})",
        ts = now_ms(),
        short = peer_short,
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
