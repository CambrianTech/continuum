/// Continuum Core Server - Unified Modular Rust Runtime
///
/// Rust-first architecture for concurrent AI persona system.
/// Provides via Unix socket IPC:
/// - VoiceOrchestrator and PersonaInbox
/// - DataModule (ORM operations via ORMRustClient)
/// - EmbeddingModule (fastembed vector generation)
/// - SearchModule (BM25, TF-IDF, vector search)
/// - LoggerModule (structured logging)
/// - LiveKit WebRTC agent for live audio/video
///
/// Usage: continuum-core-server <socket-path>
/// Example: continuum-core-server /tmp/continuum-core.sock

use continuum_core::start_server;
use continuum_core::voice::livekit_agent::LiveKitAgentManager;
use continuum_core::memory::{ModuleBackedEmbeddingProvider, PersonaMemoryManager};
use std::env;
use std::sync::Arc;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

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

    info!("🦀 Continuum Core Server starting...");
    info!("   IPC Socket: {socket_path}");

    // Create LiveKit agent manager — routes audio/video through LiveKit WebRTC SFU.
    // Handles speak-in-call, inject-audio, ambient, and video track publishing.
    // URL resolved from config.env secrets (LIVEKIT_URL) with dev fallback.
    let livekit_manager = Arc::new(LiveKitAgentManager::new());
    info!("🔊 LiveKit agent manager ready (URL: {})", livekit_manager.url());

    // Initialize Hippocampus memory subsystem with shared embedding provider.
    // Uses EmbeddingModule's MODEL_CACHE for ONE fastembed model across entire runtime.
    // Model loads lazily on first embed call (~100ms), then ~5ms per embed.
    info!("🧠 Initializing Hippocampus with shared embedding provider...");
    let embedding_provider: Arc<dyn continuum_core::memory::EmbeddingProvider> =
        Arc::new(ModuleBackedEmbeddingProvider::default_model());
    info!("✅ Hippocampus ready: {} ({}D, shared with EmbeddingModule)",
        embedding_provider.name(), embedding_provider.dimensions());
    let memory_manager = Arc::new(PersonaMemoryManager::new(embedding_provider));

    // Capture tokio runtime handle for async operations from IPC thread
    let rt_handle = tokio::runtime::Handle::current();

    // Start IPC server in background thread FIRST (creates socket immediately)
    let ipc_livekit_manager = livekit_manager.clone();
    let ipc_memory_manager = memory_manager.clone();
    let ipc_handle = std::thread::spawn(move || {
        if let Err(e) = start_server(&socket_path, ipc_livekit_manager, rt_handle, ipc_memory_manager) {
            tracing::error!("❌ IPC server error: {}", e);
        }
    });

    // Give IPC server time to create socket (satisfies start-workers.sh check)
    std::thread::sleep(std::time::Duration::from_millis(100));

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

    // Server is ready — wait for IPC thread (runs until process exits)
    info!("✅ Continuum Core Server fully started");
    let _ = ipc_handle.join();

    Ok(())
}
