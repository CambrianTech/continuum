//! airc_rag_demo — integration: hit the live airc daemon, run the L1
//! RAG layer via the canonical `inspect_persona_rag` library function,
//! print what the substrate would actually feed a model with real
//! messages.
//!
//! Joel (2026-05-31): "Unit is one thing. Integration is everything."
//! ...and follow-up: "This is the differentiator between a complex
//! guess and an intentional brain. If we have observability and
//! replay at any stage, we can iterate, improve, add complexity..."
//!
//! Run with:
//!     cargo run --bin airc_rag_demo --features metal,accelerate
//!
//! Or attach as a real persona:
//!     CONTINUUM_PERSONA=Paige cargo run --bin airc_rag_demo --features metal,accelerate
//!
//! The introspection rationale (per-item score, lamport, peer-id,
//! age, content preview) is computed by `persona::rag_inspect`. This
//! binary is a thin CLI: discover daemon → attach → call library →
//! print. That's the same path the future ServiceModule will take.

use std::path::PathBuf;
use std::sync::Arc;

use continuum_core::airc::{discover_airc_socket, discover_default_channel};
use continuum_core::persona::airc_source::AircTranscriptReader;
use continuum_core::persona::rag_budget::ReservedTokens;
use continuum_core::persona::rag_inspect::{inspect_persona_rag, RagInspection, RagInspectionRequest};

const DEFAULT_AGENT_NAME: &str = "rag-demo";

fn persona_name() -> String {
    std::env::var("CONTINUUM_PERSONA").unwrap_or_else(|_| DEFAULT_AGENT_NAME.to_string())
}

fn should_seed_messages() -> bool {
    std::env::var("CONTINUUM_PERSONA").is_err()
}

/// One profile = a synthetic context window + per-source budget knobs.
/// Same shape `RagInspectionRequest` takes, with a display name layered
/// on top so the demo can group output by tier.
struct ContextProfile {
    name: &'static str,
    context_window: u32,
    reserved_system: u32,
    reserved_completion: u32,
    airc_floor: u32,
    airc_max: u32,
}

const PROFILES: &[ContextProfile] = &[
    ContextProfile {
        name: "tiny-local (4k)",
        context_window: 4_096,
        reserved_system: 200,
        reserved_completion: 800,
        airc_floor: 100,
        airc_max: 2_000,
    },
    ContextProfile {
        name: "mid-local (32k)",
        context_window: 32_768,
        reserved_system: 400,
        reserved_completion: 4_000,
        airc_floor: 500,
        airc_max: 20_000,
    },
    ContextProfile {
        name: "cloud-tier (200k)",
        context_window: 200_000,
        reserved_system: 500,
        reserved_completion: 8_000,
        airc_floor: 2_000,
        airc_max: 150_000,
    },
];

fn continuum_root() -> PathBuf {
    if let Ok(root) = std::env::var("CONTINUUM_ROOT") {
        return PathBuf::from(root);
    }
    dirs::home_dir()
        .expect("home directory")
        .join(".continuum")
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn print_inspection(profile: &ContextProfile, inspection: &RagInspection) {
    println!("─── profile: {} ───", profile.name);
    println!("  context_window = {} tokens", profile.context_window);

    if let Some(alloc) = inspection.allocation.allocations.first() {
        println!(
            "  airc allocation: {} tokens (state: {:?})",
            alloc.allocated_tokens, alloc.state
        );
    }
    if inspection.allocation.escalation_needed {
        println!("  ⚠️  ESCALATION NEEDED — required source under-provisioned");
    }

    let delivery = match inspection.deliveries.first() {
        Some(d) => d,
        None => {
            println!("  (no deliveries)");
            println!();
            return;
        }
    };

    println!(
        "  delivered {} items, {} tokens used ({} continuation)",
        delivery.items.len(),
        delivery.tokens_used,
        if delivery.has_continuation { "with" } else { "no" },
    );

    if delivery.items.is_empty() {
        println!("  (no items — room empty for this persona, or all events were non-text)");
    } else {
        let preview_count = delivery.items.len().min(5);
        for item in delivery.items.iter().take(preview_count) {
            println!(
                "    [{:>2}] tokens={:>4} score={:.3} lamport={:<5} peer={} age={}s",
                item.index, item.tokens, item.score, item.lamport, item.peer_id_prefix, item.age_s
            );
            println!(
                "         │ {}{}",
                item.content_preview.replace('\n', " ⏎ "),
                if item.content_preview.chars().count() >= continuum_core::persona::rag_inspect::CONTENT_PREVIEW_CHARS {
                    " …"
                } else {
                    ""
                }
            );
        }
        if delivery.items.len() > preview_count {
            println!("    … ({} more items)", delivery.items.len() - preview_count);
        }
    }
    println!();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .with_writer(std::io::stderr)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    println!("=== airc_rag_demo ===");
    println!();

    let socket_path = match discover_airc_socket().await {
        Ok(p) => p,
        Err(e) => {
            println!("⚠️  Cannot reach the airc daemon: {e}");
            println!("    Remedy: install airc and run `airc join` to bring up the local daemon, then re-run this demo.");
            return Ok(());
        }
    };
    println!("✓ airc daemon discovered at {}", socket_path.display());

    let default_channel = match discover_default_channel().await {
        Ok(uuid) => uuid,
        Err(e) => {
            println!("⚠️  Cannot determine your scope's default room: {e}");
            println!("    Remedy: run `airc room <name>` to subscribe this scope to a room, then re-run.");
            return Ok(());
        }
    };
    println!("✓ default channel resolved: {default_channel}");

    let agent = persona_name();
    let root = continuum_root();
    let home = root.join("personas").join(&agent).join("airc");
    tokio::fs::create_dir_all(&home).await?;
    let airc = match airc_lib::Airc::attach_as(home.clone(), &agent, socket_path.clone()).await {
        Ok(a) => a,
        Err(e) => {
            println!("⚠️  attach_as failed: {e}");
            println!("    Remedy: check that ~/.continuum/personas/{agent}/airc is writable + airc-lib is current.");
            return Ok(());
        }
    };
    let persona_id = airc.peer_id().as_uuid();
    println!("✓ persona attached: name={agent} peer_id={}", airc.peer_id());

    let _ = airc
        .join(&default_channel.to_string())
        .await
        .map_err(|e| format!("join failed: {e}"))?;
    println!("✓ joined room {default_channel}");

    if should_seed_messages() {
        let seed_lines = [
            "rag-demo: integration smoke — turn 1",
            "rag-demo: substrate L1 budget over real airc transcript",
            "rag-demo: no-clipping doctrine respected by source",
            "rag-demo: capture trace written for replay",
        ];
        for line in seed_lines.iter() {
            let _ = airc.say(line).await;
        }
        println!("✓ seeded {} self-messages", seed_lines.len());
    } else {
        println!("✓ real persona — no synthetic seeding (transcript stays clean)");
    }

    let traces_dir = root.join("personas").join(&agent).join("rag-traces");
    tokio::fs::create_dir_all(&traces_dir).await?;
    let trace_path = traces_dir.join("demo-run.jsonl");
    // Truncate prior trace so this run starts clean. The append-mode
    // sink will recreate it.
    let _ = tokio::fs::remove_file(&trace_path).await;

    println!("✓ capture trace: {}", trace_path.display());
    println!();

    let reader: Arc<dyn AircTranscriptReader> = Arc::new(airc);
    let now = now_ms();

    for profile in PROFILES {
        let mut req = RagInspectionRequest::defaults_for(persona_id, agent.clone(), now);
        req.context_window = profile.context_window;
        req.reserved = ReservedTokens {
            system: profile.reserved_system,
            completion: profile.reserved_completion,
        };
        req.airc_floor = profile.airc_floor;
        req.airc_max = profile.airc_max;
        req.trace_path = Some(trace_path.clone());

        let inspection = inspect_persona_rag(&req, reader.clone()).await?;
        print_inspection(profile, &inspection);
    }

    println!("=== done ===");
    println!("Trace written to {}", trace_path.display());
    println!(
        "Replay with: ReplayRagSource::from_captures(\"airc\", {persona_id}, read_jsonl_captures(path)?)"
    );

    Ok(())
}
