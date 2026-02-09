/// Continuum Core Server - Unified Modular Rust Runtime
///
/// Rust-first architecture for concurrent AI persona system.
/// Provides via Unix socket IPC:
/// - VoiceOrchestrator and PersonaInbox
/// - DataModule (ORM operations via ORMRustClient)
/// - EmbeddingModule (fastembed vector generation)
/// - SearchModule (BM25, TF-IDF, vector search)
/// - LoggerModule (structured logging)
/// - WebSocket call server for live audio
///
/// Usage: continuum-core-server <socket-path>
/// Example: continuum-core-server /tmp/continuum-core.sock
///
/// NOTE: LoggerModule is now internal (Phase 4a). External logger socket no longer required.

use continuum_core::{start_server, CallManager};
use continuum_core::memory::{EmbeddingProvider, FastEmbedProvider, PersonaMemoryManager};
use std::env;
use std::sync::Arc;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

/// Get WebSocket call server port from environment or default
fn get_call_server_port() -> u16 {
    std::env::var("CONTINUUM_CORE_WS_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(50053)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <socket-path>", args[0]);
        eprintln!("Example: {} /tmp/continuum-core.sock", args[0]);
        std::process::exit(1);
    }

    let socket_path = args[1].clone();

    // LoggerModule is now internal (Phase 4a) - no external socket needed.
    // Rust-side logging uses tracing (FmtSubscriber above).
    // TypeScript clients send log/write commands to this server's IPC socket.
    info!("🦀 Continuum Core Server starting...");
    info!("   IPC Socket: {socket_path}");
    info!("   LoggerModule: internal (Phase 4a unified runtime)");

    // Create shared CallManager — used by BOTH the IPC server and WebSocket call server.
    // This enables voice/speak-in-call: TypeScript sends text → Rust synthesizes → injects
    // directly into the call mixer → audio streams to browsers via WebSocket.
    // Audio never leaves the Rust process.
    let call_manager = Arc::new(CallManager::new());

    // Initialize Hippocampus memory subsystem — inline embedding for query vectors.
    // Rust is a pure compute engine. Memory data comes from the TS ORM via IPC.
    // Embedding model loads once (~100ms), then ~5ms per embed (no IPC hop).
    info!("🧠 Initializing Hippocampus embedding provider...");
    let embedding_provider: Arc<dyn continuum_core::memory::EmbeddingProvider> = match FastEmbedProvider::new() {
        Ok(provider) => {
            info!("✅ Hippocampus embedding ready: {} ({}D)", provider.name(), provider.dimensions());
            Arc::new(provider)
        }
        Err(e) => {
            tracing::error!("❌ Failed to load embedding model: {}", e);
            tracing::error!("   Memory operations will not have semantic search.");
            tracing::error!("   Ensure fastembed model cache is available.");
            std::process::exit(1);
        }
    };
    let memory_manager = Arc::new(PersonaMemoryManager::new(embedding_provider));

    // Capture tokio runtime handle for IPC thread to call async CallManager methods
    let rt_handle = tokio::runtime::Handle::current();

    // Start IPC server in background thread FIRST (creates socket immediately)
    let ipc_call_manager = call_manager.clone();
    let ipc_memory_manager = memory_manager.clone();
    let ipc_handle = std::thread::spawn(move || {
        if let Err(e) = start_server(&socket_path, ipc_call_manager, rt_handle, ipc_memory_manager) {
            tracing::error!("❌ IPC server error: {}", e);
        }
    });

    // Give IPC server time to create socket (satisfies start-workers.sh check)
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Start WebSocket call server for live audio (shares the same CallManager)
    let call_port = get_call_server_port();
    let call_addr = format!("127.0.0.1:{call_port}");
    info!("🎙️  Call WebSocket server starting on ws://{call_addr}");
    let ws_call_manager = call_manager.clone();
    let call_server_handle = tokio::spawn(async move {
        if let Err(e) = continuum_core::voice::call_server::start_call_server(&call_addr, ws_call_manager).await {
            tracing::error!("❌ Call server error: {}", e);
        }
    });

    // Initialize TTS/STT in background (non-blocking - happens after startup)
    tokio::spawn(async {
        // Initialize STT registry and adapters
        continuum_core::voice::stt::init_registry();
        match continuum_core::voice::stt::initialize().await {
            Ok(_) => info!("✅ STT adapter initialized successfully"),
            Err(e) => {
                tracing::warn!(
                    "⚠️  STT adapter not available: {}. STT will return errors until model is loaded.",
                    e
                );
                tracing::warn!("   Download ggml-base.en.bin from https://huggingface.co/ggerganov/whisper.cpp/tree/main");
                tracing::warn!("   Place in: models/whisper/ggml-base.en.bin");
            }
        }

        // Initialize TTS registry and adapters
        continuum_core::voice::tts::init_registry();
        match continuum_core::voice::tts::initialize().await {
            Ok(_) => info!("✅ TTS adapter initialized successfully"),
            Err(e) => {
                tracing::warn!(
                    "⚠️  TTS adapter not available: {}. TTS will use fallback (silence).",
                    e
                );
                tracing::warn!("   Download Piper ONNX from https://huggingface.co/rhasspy/piper-voices");
                tracing::warn!("   Place in: models/piper/");
            }
        }
    });

    // Wait for call server (the primary voice service)
    info!("✅ Continuum Core Server fully started");
    let _ = call_server_handle.await;

    // If call server exits, join IPC thread
    let _ = ipc_handle.join();

    Ok(())
}
