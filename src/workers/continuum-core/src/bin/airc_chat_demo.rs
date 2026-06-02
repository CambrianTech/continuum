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

use continuum_core::ai::adapter::AIProviderAdapter;
use continuum_core::airc::{discover_airc_socket, discover_default_channel};
use continuum_core::inference::LlamaCppAdapter;
use continuum_core::modules::persona_instance_manager::PersonaInstanceInfo;
use continuum_core::persona::airc_persona_conversation::AircPersonaConversation;
use continuum_core::persona::airc_runtime::PersonaAircRuntime;
use continuum_core::persona::airc_source::AircTranscriptReader;
use continuum_core::persona::identity_provider::PersonaIdentitySource;
use continuum_core::persona::role_template::RoleId;
use continuum_core::persona::service_loop::{serve_persona_loop, ServeOptions};
use continuum_core::persona::supervisor::HostedPersona;

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

    // 3. Join the room by NAME — not UUID.
    //
    // `Airc::join(name)` (airc-lib/src/airc.rs:914) calls
    // `ChannelName::new(name)` which derives a fresh channel UUID
    // from the name. Passing a uuid-shaped string as the "name"
    // creates a brand-new channel whose UUID does NOT match the
    // intended room — the subscription registers on the wrong
    // channel and the fan-out misses every publish. Card 800ce5bd
    // empirically caught this: Paige's subscribe landed on shard 15
    // / channel 5d33e2a7 (derived from the uuid string),
    // while `airc msg` published to channel 11c1a7ac with
    // subscribers_before=0. Use the actual room name; the canonical
    // continuum room is "continuum" (matches what `airc room`
    // reports for the same scope).
    let room_name = std::env::var("CONTINUUM_ROOM").unwrap_or_else(|_| "continuum".to_string());
    let room = airc
        .join(&room_name)
        .await
        .map_err(|e| format!("join failed: {e}"))?;
    println!(
        "✓ joined room {room_name} → channel {} (discovered uuid was {default_channel})",
        room.channel
    );

    // 4. Build the LlamaCppAdapter pointing at the LCD local GGUF.
    //    Per [[no-fallbacks-ever]] + [[no-if-statements-use-llms-for-
    //    cognition]] + [[lcd-model-qwen25-05b-and-foundry-lora]] —
    //    real cognition only. Heuristic adapter is cfg-gated out of
    //    production (#128) and the binary explicitly uses
    //    LlamaCppAdapter so there's no fallback path that could land
    //    on a fake. On Intel Mac without working Metal, build with
    //    `--features llama/mac-cpu-only` and run with n_gpu_layers=0
    //    via the LLM_GGUF_PATH-pointed local file.
    let gguf_path = std::env::var("LLM_GGUF_PATH").unwrap_or_else(|_| {
        // Default: the LCD inference target — Qwen2.5-0.5B-Instruct
        // Q4_K_M, ~468 MiB, plain attention, candle-trainable
        // safetensors sibling available for foundry LoRA work.
        format!(
            "{}/.continuum/genome/models/qwen2.5-0.5b-instruct/qwen2.5-0.5b-instruct-q4_k_m.gguf",
            dirs::home_dir()
                .expect("home directory")
                .display()
        )
    });
    let gguf_pathbuf = PathBuf::from(&gguf_path);
    if !gguf_pathbuf.exists() {
        println!(
            "⚠️  GGUF not found at {gguf_path}. \
             Substrate hard-errors per [[no-fallbacks-ever]] — fix the path \
             via LLM_GGUF_PATH or download the LCD model."
        );
        return Ok(());
    }
    let n_gpu_layers: i32 = std::env::var("LLM_N_GPU_LAYERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let context_length: usize = std::env::var("LLM_CONTEXT_LENGTH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2048);
    println!(
        "✓ loading LCD model: {gguf_path} (n_gpu_layers={n_gpu_layers}, context={context_length})"
    );
    // Build a PersonaInferenceProfile and construct the adapter via
    // the intent-driven API per [[intent-driven-api-not-hot-patches]].
    // Pre-#133 this was a hand-tuned chain (with_model_id +
    // with_context_length + hardcoded n_ubatch); post-#133 the profile
    // is the source of truth for every inference knob and the
    // PersonaSpawnerModule (#121) will eventually be the producer.
    //
    // Demo binary builds the profile from env vars for now because
    // the spawner doesn't exist yet (#133 slice 5). Substrate-managed
    // personas will get fully-resolved profiles from
    // role_template + hw_tier_descriptor + model_meta — no env vars,
    // no ad-hoc string constants.
    use continuum_core::persona::hw_tier_descriptor::HwTierCategory;
    use continuum_core::persona::inference_profile::{
        PersonaInferenceProfile, SamplingProfile,
    };
    let profile = PersonaInferenceProfile {
        persona_id,
        persona_name: agent.clone(),
        model_id: "continuum-ai/qwen2.5-0.5b-instruct-GGUF".to_string(),
        gguf_local_path: Some(gguf_pathbuf),
        // Compat: works everywhere — Intel Mac + AMD discrete falls
        // here per the post-#129 LCD doctrine.
        tier_category: HwTierCategory::Compat,
        tier_id: "mac_intel_metal_discrete".to_string(),
        context_length: context_length as u32,
        // n_ubatch=512 covers the realistic 200-500 token RAG-built
        // persona prompts observed during #130. Substrate default
        // matches; profile carries it explicitly so the spawner can
        // tune per role/tier later.
        n_ubatch: 512,
        n_batch: context_length as u32,
        n_seq_max: 1,
        n_gpu_layers,
        sampling: SamplingProfile::chat_defaults(),
        // Adapter falls through to the model_registry row's chat_template
        // when None — the registry already carries qwen2.5's chatml.
        chat_template: None,
        // Defense-in-depth — registry row has these too.
        stop_sequences: vec!["<|im_end|>".to_string(), "<|endoftext|>".to_string()],
    };
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(
        LlamaCppAdapter::for_persona(&profile).map_err(|e| {
            format!("LlamaCppAdapter::for_persona failed: {e}")
        })?,
    );
    println!("✓ real-cognition adapter ready: {}", adapter.provider_id());
    println!();

    // 5. Wrap the Airc handle in a PersonaAircRuntime via the
    //    `from_attached` constructor (avoids `bootstrap`'s join-by-
    //    uuid-as-string path that the demo deliberately works around
    //    via join-by-name above).
    let airc_arc = Arc::new(airc);
    let reader: Arc<dyn AircTranscriptReader> = airc_arc.clone();
    let runtime = Arc::new(PersonaAircRuntime::from_attached(
        persona_id,
        agent.clone(),
        home.clone(),
        airc_arc.clone(),
        room.channel,
        PersonaIdentitySource::FreshlyMinted,
    ));

    // 6. Hand off to the substrate-managed service loop. The demo
    //    binary stops doing the work itself — `serve_persona_loop`
    //    (from #133 slice 10) owns the subscribe + inbound filter +
    //    RAG + inference + say cycle. The same call is what slice 12
    //    will fire from headless `continuum-core` boot for every
    //    persona the spawner planned.
    let hosted = HostedPersona {
        role: RoleId::Helper,
        identity: PersonaInstanceInfo {
            persona_id,
            agent_name: agent.clone(),
            peer_id: persona_id,
            home: home.clone(),
            default_room: room.channel.as_uuid(),
            source: PersonaIdentitySource::FreshlyMinted,
        },
        profile: profile.clone(),
        adapter,
        // PersonaAircRuntime impls AircCitizen — Arc auto-coerces.
        runtime: runtime.clone(),
    };
    let mut conversation = AircPersonaConversation::new(runtime);

    println!("✓ handed off to substrate-managed serve_persona_loop.");
    println!("  Send a message in the same room to test.");
    println!("  Stop with Ctrl-C.");
    println!();

    let outcome = serve_persona_loop(
        &hosted,
        &mut conversation,
        reader,
        ServeOptions {
            page_recent_limit: PAGE_RECENT_LIMIT,
            rag_fetch_limit: PAGE_RECENT_LIMIT,
            now_ms,
        },
    )
    .await
    .map_err(|e| format!("serve_persona_loop failed: {e}"))?;

    println!(
        "✓ loop ended: replied={} skipped={} errored={}",
        outcome.turns_replied, outcome.turns_skipped, outcome.turns_errored
    );
    Ok(())
}
