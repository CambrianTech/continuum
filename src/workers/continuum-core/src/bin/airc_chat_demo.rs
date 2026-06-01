//! airc_chat_demo — proves the substrate's end-to-end persona
//! response loop against a live airc daemon.
//!
//! Joel (2026-05-31): "We really need to prove persona and rag work.
//! That this can respond in airc chats."
//!
//! This binary IS that proof. It:
//!
//! 1. Discovers the local airc daemon + the scope's default room.
//! 2. Attaches as the demo persona (default `Paige`, configurable
//!    via `CONTINUUM_PERSONA`).
//! 3. Joins the default room.
//! 4. Polls airc for new transcript events on every tick (every
//!    `CONTINUUM_CHAT_DEMO_POLL_MS` ms; default 3000).
//! 5. For each new chat message NOT from Paige herself:
//!    a. Builds a `RagInspectionRequest` for her.
//!    b. Calls `inspect_persona_rag_with_inference` — the L1 RAG
//!       layer surfaces her recent transcript, the
//!       HeuristicInferenceAdapter generates a deterministic
//!       response, the result captures the model response.
//!    c. Posts the response text back via `airc.say(...)`.
//! 6. Prints the live trace to stdout — what came in, what RAG
//!    delivered, what Paige said back.
//!
//! Run from the operator's shell against the live airc daemon:
//!
//!     cargo run --bin airc_chat_demo --features metal,accelerate
//!
//! Then in another shell or via the chat widget, send a message to
//! the same room — Paige replies via the heuristic adapter within
//! one poll tick. Stop with Ctrl-C.
//!
//! ### What this proves
//!
//! - The substrate's RAG layer + inference chain + airc round-trip
//!   work end-to-end on the operator's actual hardware.
//! - The heuristic adapter ([[inference-is-an-adapter-always-in-the-loop]])
//!   produces a deterministic, observable response without needing
//!   a GGUF or cloud key.
//! - Swapping the heuristic adapter for a real LlamaCppAdapter (or
//!   AircRemoteInferenceAdapter routing to a grid peer) is a
//!   one-line config change — the surrounding code doesn't shift.
//!
//! ### What this is NOT
//!
//! - Not the production persona-cognition path. The substrate's
//!   real `PersonaAircRuntime` will wire an inbound pump that
//!   triggers `cognition::generate_response` (task #112 refactors
//!   it through the handle store). This demo is the proof that
//!   the wire shape works end-to-end; production wiring is a
//!   focused follow-up.
//! - Not a multi-persona test. ONE persona, ONE room. The
//!   coordinator + lane multiplexing tests cover the N-persona
//!   case; this demo focuses on the chat round-trip.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use continuum_core::ai::adapter::AIProviderAdapter;
use continuum_core::ai::heuristic_adapter::HeuristicInferenceAdapter;
use continuum_core::airc::{discover_airc_socket, discover_default_channel};
use continuum_core::persona::airc_source::AircTranscriptReader;
use continuum_core::persona::rag_inspect::{
    inspect_persona_rag_with_inference, RagInspectionRequest,
};

const DEFAULT_AGENT_NAME: &str = "Paige";
const DEFAULT_POLL_MS: u64 = 3_000;
const PAGE_RECENT_LIMIT: usize = 25;

fn persona_name() -> String {
    std::env::var("CONTINUUM_PERSONA").unwrap_or_else(|_| DEFAULT_AGENT_NAME.to_string())
}

fn poll_interval() -> Duration {
    let ms = std::env::var("CONTINUUM_CHAT_DEMO_POLL_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(DEFAULT_POLL_MS);
    Duration::from_millis(ms)
}

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .with_writer(std::io::stderr)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    println!("=== airc_chat_demo ===");
    println!("Proving substrate end-to-end: airc → RAG → inference → airc.");
    println!();

    // 1. Discover the airc daemon + default room.
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

    // 2. Attach the persona.
    let agent = persona_name();
    let root = continuum_root();
    let home = root.join("personas").join(&agent).join("airc");
    tokio::fs::create_dir_all(&home).await?;
    let airc = match airc_lib::Airc::attach_as(home.clone(), &agent, socket_path.clone()).await {
        Ok(a) => a,
        Err(e) => {
            println!("⚠️  attach_as failed: {e}");
            return Ok(());
        }
    };
    let persona_id = airc.peer_id().as_uuid();
    let my_peer_id_str = airc.peer_id().to_string();
    let my_peer_id_short: String = my_peer_id_str.chars().take(8).collect();
    println!(
        "✓ persona attached: name={agent} peer_id={} (short={})",
        airc.peer_id(),
        my_peer_id_short
    );

    // 3. Join the default room.
    let _ = airc
        .join(&default_channel.to_string())
        .await
        .map_err(|e| format!("join failed: {e}"))?;
    println!("✓ joined room {default_channel}");

    // 4. Build the heuristic adapter — substrate's deterministic
    //    proof-of-life inference. Replace with LlamaCppAdapter or
    //    AircRemoteInferenceAdapter via config when ready.
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    println!("✓ heuristic adapter ready: {}", adapter.provider_id());
    println!();

    // 5. Wrap the Airc handle as an AircTranscriptReader so the
    //    RAG layer can read recent transcript events. Arc-shared
    //    so we can keep using the underlying handle for `say`.
    let airc_arc = Arc::new(airc);
    let reader: Arc<dyn AircTranscriptReader> = airc_arc.clone();

    // 6. Track the highest lamport we've already responded to.
    //    Prevents replying to the same message twice + avoids
    //    replying to messages from before this binary started.
    let mut last_lamport_seen: u64 = airc_arc
        .page_recent(PAGE_RECENT_LIMIT)
        .await
        .map(|events| events.iter().map(|e| e.lamport).max().unwrap_or(0))
        .unwrap_or(0);
    println!(
        "✓ starting from lamport={} (responding only to messages received AFTER this).",
        last_lamport_seen
    );
    println!();
    println!("Listening for chats. Send a message in the same room to test.");
    println!("Stop with Ctrl-C.");
    println!();

    let poll = poll_interval();
    loop {
        tokio::time::sleep(poll).await;

        // Pull recent events; in production we'd subscribe to the
        // airc event stream, but page_recent + lamport-tracking is
        // sufficient for the demo's proof-of-life.
        let events = match airc_arc.page_recent(PAGE_RECENT_LIMIT).await {
            Ok(events) => events,
            Err(e) => {
                eprintln!("page_recent failed: {e}");
                continue;
            }
        };

        // Process oldest → newest so a burst of messages gets
        // answered in order.
        let mut sorted = events;
        sorted.sort_by_key(|e| e.lamport);

        for event in sorted {
            if event.lamport <= last_lamport_seen {
                continue;
            }
            // Skip messages from Paige herself (avoid loop).
            if event.peer_id.as_uuid() == persona_id {
                last_lamport_seen = event.lamport.max(last_lamport_seen);
                continue;
            }
            // Skip non-text messages.
            let Some(body) = &event.body else {
                last_lamport_seen = event.lamport.max(last_lamport_seen);
                continue;
            };
            let Some(text) = body.as_text() else {
                last_lamport_seen = event.lamport.max(last_lamport_seen);
                continue;
            };

            let from_peer_short: String = event.peer_id.to_string().chars().take(8).collect();
            println!("─── inbound (lamport={}) ───", event.lamport);
            println!("  from={from_peer_short}");
            println!("  text={text}");

            // Build a RAG inspection request scoped to Paige.
            let mut req =
                RagInspectionRequest::defaults_for(persona_id, agent.clone(), now_ms());
            req.airc_fetch_limit = PAGE_RECENT_LIMIT;

            // Run the chained inspection: RAG layer surfaces recent
            // transcript → heuristic adapter generates response →
            // captured in model_response.
            let inspection = match inspect_persona_rag_with_inference(
                &req,
                reader.clone(),
                Some(adapter.clone()),
            )
            .await
            {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("  inspect_persona_rag_with_inference failed: {e}");
                    last_lamport_seen = event.lamport.max(last_lamport_seen);
                    continue;
                }
            };

            let mr = match inspection.model_response {
                Some(mr) => mr,
                None => {
                    println!("  (no model_response — RAG-only path; nothing to post)");
                    last_lamport_seen = event.lamport.max(last_lamport_seen);
                    continue;
                }
            };

            println!("  RAG delivered {} items", inspection.deliveries[0].items.len());
            println!("  model={} tokens_in={} tokens_out={}",
                mr.model, mr.input_tokens, mr.output_tokens);

            // Post the response back to airc.
            match airc_arc.say(&mr.response_text).await {
                Ok(event_id) => {
                    println!("  ✓ posted reply (event_id={event_id})");
                    println!("    reply: {}", mr.response_text);
                }
                Err(e) => {
                    eprintln!("  airc.say failed: {e}");
                }
            }
            println!();

            last_lamport_seen = event.lamport.max(last_lamport_seen);
        }
    }
}
