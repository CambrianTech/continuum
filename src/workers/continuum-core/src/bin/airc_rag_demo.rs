//! airc_rag_demo — integration: hit the live airc daemon, run the
//! L1 RAG layer (AircRagSource + FlexboxRagBudgetAdapter), print
//! what the substrate would actually feed a model with real messages.
//!
//! Joel (2026-05-31): "Unit is one thing. Integration is everything."
//!
//! Run with:
//!     cargo run --bin airc_rag_demo --features metal,accelerate
//!
//! Behavior:
//! - Discovers the airc daemon socket (auto-install if missing)
//! - Attaches a demo persona at ~/.continuum/personas/rag-demo/airc/
//! - Joins the airc daemon's default channel (whatever `airc room`
//!   currently points at)
//! - Wraps the Airc handle in an AircRagSource
//! - Runs FlexboxRagBudgetAdapter at THREE context-window sizes
//!   (4_096 tiny-local, 32_768 mid-local, 200_000 cloud-tier) so the
//!   operator can SEE the variability thesis from
//!   docs/architecture/EVERY-MODEL-INCLUDED-VIA-L1-BUDGET.md
//!   produce different deliveries against the same source
//! - Captures each turn's allocation + deliveries into a JSONL
//!   trace under ~/.continuum/personas/rag-demo/rag-traces/ so the
//!   run is replayable later
//! - Prints the result in human-readable form
//!
//! Skips gracefully when:
//! - The airc daemon isn't reachable (prints actionable remedy)
//! - The current scope has no default room (prints `airc room <name>`
//!   remedy)
//! - The persona has no messages yet (prints "send some messages
//!   through `airc msg` and re-run")

use std::path::PathBuf;
use std::sync::Arc;

use continuum_core::airc::{discover_airc_socket, discover_default_channel};
use continuum_core::persona::airc_source::{AircRagSource, AircTranscriptReader};
use continuum_core::persona::rag_budget::{
    FlexboxRagBudgetAdapter, RagBudgetAdapter, RagContext, RagSource, RagSourceBudget,
    ReservedTokens, ResolutionPreference,
};
use continuum_core::persona::rag_capture::{
    JsonlRagCaptureSink, RagCaptureEvent, RagCaptureSink, RecordingRagSource,
};

const DEFAULT_AGENT_NAME: &str = "rag-demo";

/// Choose which persona to attach as. `CONTINUUM_PERSONA=Paige`
/// flips the demo from "synthetic seeded rag-demo" to "attach as a
/// real living persona and show me what their RAG actually looks
/// like" — answering Joel's directive (2026-05-31): "Look at the rag
/// for a real room or persona. AIs are gonna need to analyze what's
/// getting fed into a persona."
fn persona_name() -> String {
    std::env::var("CONTINUUM_PERSONA").unwrap_or_else(|_| DEFAULT_AGENT_NAME.to_string())
}

/// Don't pollute a real persona's transcript with synthetic seed
/// messages. Skip seeding when the caller explicitly asks for a
/// real persona via CONTINUUM_PERSONA.
fn should_seed_messages() -> bool {
    std::env::var("CONTINUUM_PERSONA").is_err()
}

/// One profile: a synthetic "what context window am I pretending to
/// have" + the source budget that goes with it.
struct ContextProfile {
    name: &'static str,
    context_window: u32,
    reserved_system: u32,
    reserved_completion: u32,
    /// Floor + max for the airc source. Larger contexts mean larger
    /// allowances; the no-clipping doctrine means a tighter context
    /// drops items whole rather than partial-include.
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .with_writer(std::io::stderr)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    println!("=== airc_rag_demo ===");
    println!();

    // 1. Discover the airc daemon.
    let socket_path = match discover_airc_socket().await {
        Ok(p) => p,
        Err(e) => {
            println!("⚠️  Cannot reach the airc daemon: {e}");
            println!("    Remedy: install airc (`brew install cambriantech/cambrian/airc`) and run");
            println!("    `airc join` to bring up the local daemon, then re-run this demo.");
            return Ok(());
        }
    };
    println!("✓ airc daemon discovered at {}", socket_path.display());

    // 2. Discover the scope's default channel.
    let default_channel = match discover_default_channel().await {
        Ok(uuid) => uuid,
        Err(e) => {
            println!("⚠️  Cannot determine your scope's default room: {e}");
            println!("    Remedy: run `airc room <name>` to subscribe this scope to a room");
            println!("    (e.g., `airc room continuum`), then re-run.");
            return Ok(());
        }
    };
    println!("✓ default channel resolved: {default_channel}");

    // 3. Attach the persona via airc-lib (default `rag-demo`;
    //    override with CONTINUUM_PERSONA=<name> to attach as a real
    //    living persona like Paige).
    let agent = persona_name();
    let root = continuum_root();
    let home = root
        .join("personas")
        .join(&agent)
        .join("airc");
    tokio::fs::create_dir_all(&home).await?;
    let airc = match airc_lib::Airc::attach_as(home.clone(), &agent, socket_path.clone())
        .await
    {
        Ok(a) => a,
        Err(e) => {
            println!("⚠️  attach_as failed: {e}");
            println!("    Remedy: check that ~/.continuum/personas/{agent}/airc is writable + airc-lib is current.");
            return Ok(());
        }
    };
    let persona_id = airc.peer_id().as_uuid();
    println!("✓ persona attached: name={agent} peer_id={}", airc.peer_id());

    // Join the discovered room so page_recent has something to read.
    let _ = airc
        .join(&default_channel.to_string())
        .await
        .map_err(|e| format!("join failed: {e}"))?;
    println!("✓ joined room {default_channel}");

    // Seed a few self-messages ONLY for the synthetic rag-demo persona.
    // Real personas (CONTINUUM_PERSONA set) keep their transcript clean.
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

    // 4. Build the AircRagSource around the live Airc.
    let reader: Arc<dyn AircTranscriptReader> = Arc::new(airc);
    let airc_source = AircRagSource::new(persona_id, reader).with_fetch_limit(100);

    // 5. Capture sink: JSONL under the persona's home.
    let traces_dir = root.join("personas").join(&agent).join("rag-traces");
    tokio::fs::create_dir_all(&traces_dir).await?;
    let trace_path = traces_dir.join("demo-run.jsonl");
    let sink: Arc<dyn RagCaptureSink> =
        Arc::new(JsonlRagCaptureSink::open(trace_path.clone())?);
    let recorded_source = RecordingRagSource::new(airc_source, sink.clone());

    println!("✓ capture trace: {}", trace_path.display());
    println!();

    // 6. Run the three profiles + print results.
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let ctx = RagContext::for_persona(persona_id, now_ms);
    let adapter = FlexboxRagBudgetAdapter::new();

    for profile in PROFILES {
        println!("─── profile: {} ───", profile.name);
        println!("  context_window = {} tokens", profile.context_window);
        let reserved = ReservedTokens {
            system: profile.reserved_system,
            completion: profile.reserved_completion,
        };
        let budgets = vec![RagSourceBudget {
            source_id: "airc".to_string(),
            priority: 10,
            floor_tokens: profile.airc_floor,
            min_tokens: profile.airc_floor,
            max_tokens: profile.airc_max,
            required: true,
        }];

        // Emit a TurnStart so the trace can be grouped properly.
        let turn_id = uuid::Uuid::new_v4();
        let mut ctx_for_turn = ctx.clone();
        ctx_for_turn.substrate.turn_id = Some(turn_id);
        sink.record(RagCaptureEvent::TurnStart {
            captured_at_ms: now_ms,
            persona_id,
            turn_id: Some(turn_id),
            context_window: profile.context_window,
            reserved,
            source_budgets: budgets.clone(),
            context: ctx_for_turn.clone(),
        });

        let allocation = adapter.allocate(&ctx_for_turn, profile.context_window, reserved, &budgets);
        let airc_alloc = &allocation.allocations[0];
        println!(
            "  airc allocation: {} tokens (state: {:?})",
            airc_alloc.allocated_tokens, airc_alloc.state,
        );
        if allocation.escalation_needed {
            println!("  ⚠️  ESCALATION NEEDED — required source under-provisioned");
        }
        sink.record(RagCaptureEvent::BudgetAllocated {
            captured_at_ms: now_ms,
            persona_id,
            turn_id: Some(turn_id),
            allocation: allocation.clone(),
        });

        let delivery = recorded_source
            .deliver(
                &ctx_for_turn,
                airc_alloc.allocated_tokens,
                ResolutionPreference::Raw,
            )
            .await;

        println!(
            "  delivered {} items, {} tokens used ({} continuation)",
            delivery.items.len(),
            delivery.tokens_used,
            if delivery.continuation.is_some() {
                "with"
            } else {
                "no"
            },
        );

        if delivery.items.is_empty() {
            println!("  (no items — room empty for this persona, or all events were non-text)");
        } else {
            // Deep introspection: dump per-item content + scoring +
            // metadata so an AI (or human) can honestly look at what
            // would be fed into the prompt. Joel (2026-05-31): "AIs
            // are gonna need to analyze what's getting fed into a
            // persona. Also why replay was important. That way we
            // can take an honest look at each prompt."
            let preview_count = delivery.items.len().min(5);
            for (i, item) in delivery.items.iter().take(preview_count).enumerate() {
                let score = item
                    .metadata
                    .get("score")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let lamport = item
                    .metadata
                    .get("lamport")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let peer_short = item
                    .metadata
                    .get("peer_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.chars().take(8).collect::<String>())
                    .unwrap_or_else(|| "????".to_string());
                let occurred_at_ms = item
                    .metadata
                    .get("occurred_at_ms")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let age_s = if occurred_at_ms > 0 && now_ms > occurred_at_ms {
                    (now_ms - occurred_at_ms) / 1_000
                } else {
                    0
                };
                let snippet: String = item.content.chars().take(120).collect();
                println!(
                    "    [{i:>2}] tokens={:>4} score={:.3} lamport={lamport:<5} peer={peer_short} age={age_s}s",
                    item.tokens, score
                );
                println!(
                    "         │ {}{}",
                    snippet.replace('\n', " ⏎ "),
                    if item.content.chars().count() > 120 { " …" } else { "" }
                );
            }
            if delivery.items.len() > preview_count {
                println!(
                    "    … ({} more items)",
                    delivery.items.len() - preview_count
                );
            }
        }

        sink.record(RagCaptureEvent::TurnEnd {
            captured_at_ms: now_ms,
            persona_id,
            turn_id: Some(turn_id),
        });
        println!();
    }

    println!("=== done ===");
    println!("Trace written to {}", trace_path.display());
    println!(
        "Replay with: ReplayRagSource::from_captures(\"airc\", {persona_id}, read_jsonl_captures(path)?)"
    );

    Ok(())
}
