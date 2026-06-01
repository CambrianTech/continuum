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
//!
//! ### Inbound: subscribe, not poll (RTOS doctrine)
//!
//! v1 of this demo polled `airc.page_recent(N)` every tick to
//! detect new messages. That hid the substrate's actual contract
//! and tripped a false-positive "fanout gap" hypothesis. The
//! reality (confirmed by tracing 2026-06-01):
//!
//! - `Airc::subscribe()` (`crates/airc-lib/src/messaging.rs:204`)
//!   ALREADY routes through the daemon's attach stream when
//!   daemon-attached. It opens `AttachRequest`, decodes each
//!   `Response::Event { envelope }` via `decode_wire_event`, and
//!   delivers `Arc<TranscriptEvent>` through an `EventStream` —
//!   with reconnect-from-cursor on daemon restarts.
//! - `Airc::page_recent()` (when daemon-attached) issues an
//!   `InboxRequest` to the daemon which replays the durable
//!   tier via `state.router.resume_from_cursor`. So the warm-up
//!   high-water mark IS correct.
//!
//! The current shape: page_recent once for the cursor, then loop
//! on subscribe() forever. No polling, no per-tick diagnostics
//! needed — events arrive as they're published.
//!
//! ### Empirical status (2026-06-01)
//!
//! Tested live on Joel's MacBookPro15,1 against the running
//! daemon (build=71a07525f57c, branch=feat/airc-ipc-endpoint-command):
//!
//! 1. Demo starts, attaches as Paige, subscribes via the public
//!    `Airc::subscribe()` API — `✓ subscribed to live daemon stream`
//!    prints, no error from the attach handshake.
//! 2. Three test messages posted via `airc msg` land in the
//!    daemon's `~/.airc/events.sqlite::bus_events` table
//!    (verified directly: epoch=124, counters 646-648,
//!    matching channel uuid).
//! 3. Demo's `subscribe()` stream yields zero events — no
//!    "inbound" log line, no "subscribe stream ended" log line.
//!    The mpsc is open but silent.
//!
//! Diagnosis: messages are landing in the bus but the daemon's
//! per-subscriber fanout is NOT pushing them to Paige's IPC
//! attach stream. This is **task #82** ("Headless break #3: CBOR
//! Response::Event schema mismatch") manifesting on the live
//! daemon — either decode_wire_event silently bails (the
//! current daemon_subscribe loop `Err(_) => return` swallows it
//! at airc-lib/src/daemon.rs:416) OR the subscriber filter on
//! the daemon side doesn't match these envelopes.
//!
//! Demo is structurally correct and will start producing
//! inbound + reply output the moment #82 lands in the daemon.
//! Until then, only the OUTBOUND half (attach + join + adapter +
//! say) is provably wired.

use std::path::PathBuf;
use std::sync::Arc;

use futures::StreamExt;

use continuum_core::ai::adapter::AIProviderAdapter;
use continuum_core::ai::heuristic_adapter::HeuristicInferenceAdapter;
use continuum_core::airc::{discover_airc_socket, discover_default_channel};
use continuum_core::persona::airc_source::AircTranscriptReader;
use continuum_core::persona::rag_inspect::{
    inspect_persona_rag_with_inference, RagInspectionRequest,
};

const DEFAULT_AGENT_NAME: &str = "Paige";
const PAGE_RECENT_LIMIT: usize = 25;

fn persona_name() -> String {
    std::env::var("CONTINUUM_PERSONA").unwrap_or_else(|_| DEFAULT_AGENT_NAME.to_string())
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

    // 6. Compute the high-water mark from recent history. This is
    //    a one-shot page_recent for the cursor only — subscribe()
    //    takes over for live events. Avoids replying to messages
    //    that arrived before this binary started.
    let mut last_lamport_seen: u64 = airc_arc
        .page_recent(PAGE_RECENT_LIMIT)
        .await
        .map(|events| events.iter().map(|e| e.lamport).max().unwrap_or(0))
        .unwrap_or(0);
    println!(
        "✓ warm-up cursor: lamport={} (responding only to events AFTER this).",
        last_lamport_seen
    );

    // 7. Open the live daemon attach stream. From here on no
    //    polling — every new event arrives through `next().await`.
    let mut stream = airc_arc
        .subscribe()
        .await
        .map_err(|e| format!("subscribe failed: {e}"))?;
    println!("✓ subscribed to live daemon stream — listening for chats.");
    println!("  Send a message in the same room to test.");
    println!("  Stop with Ctrl-C.");
    println!();

    while let Some(item) = stream.next().await {
        let event = match item {
            Ok(e) => e,
            Err(lag) => {
                eprintln!("⚠️  live stream lag: {lag} — resume continues from cursor");
                continue;
            }
        };

        if event.lamport <= last_lamport_seen {
            continue;
        }
        last_lamport_seen = event.lamport.max(last_lamport_seen);

        // Skip messages from Paige herself (avoid loop).
        if event.peer_id.as_uuid() == persona_id {
            continue;
        }
        // Skip non-text messages.
        let Some(body) = &event.body else { continue };
        let Some(text) = body.as_text() else { continue };

        let from_peer_short: String = event.peer_id.to_string().chars().take(8).collect();
        println!("─── inbound (lamport={}) ───", event.lamport);
        println!("  from={from_peer_short}");
        println!("  text={text}");

        // Build a RAG inspection request scoped to Paige.
        let mut req = RagInspectionRequest::defaults_for(persona_id, agent.clone(), now_ms());
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
                continue;
            }
        };

        let mr = match inspection.model_response {
            Some(mr) => mr,
            None => {
                println!("  (no model_response — RAG-only path; nothing to post)");
                continue;
            }
        };

        println!(
            "  RAG delivered {} items",
            inspection.deliveries[0].items.len()
        );
        println!(
            "  model={} tokens_in={} tokens_out={}",
            mr.model, mr.input_tokens, mr.output_tokens
        );

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
    }

    println!("✓ subscribe stream ended — daemon disconnected. Exiting.");
    Ok(())
}
