// jemalloc: returns memory to OS aggressively instead of hoarding pages.
// macOS system allocator fragments badly under Bevy's 15fps readback churn
// (14 slots × 921KB per frame) — RSS grows to 30-40GB and never shrinks.
// jemalloc's dirty page purging returns freed memory within seconds.
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use continuum_core::live::transport::livekit_agent::LiveKitAgentManager;
use continuum_core::memory::{ModuleBackedEmbeddingProvider, PersonaMemoryManager};
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
use std::env;
use std::sync::Arc;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

/// Install signal handlers that kill all sentinel process groups on shutdown.
/// This prevents orphaned training processes from eating memory after npm stop.
fn install_shutdown_handlers() {
    // SIGTERM (from npm stop / kill / system-stop.sh)
    tokio::spawn(async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sig.recv().await;
            eprintln!("[continuum-core] SIGTERM — killing sentinel process groups");
            continuum_core::modules::sentinel::shutdown_all_sentinels();
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            std::process::exit(0);
        }
    });

    // SIGINT (Ctrl+C)
    tokio::spawn(async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
        {
            sig.recv().await;
            eprintln!("[continuum-core] SIGINT — killing sentinel process groups");
            continuum_core::modules::sentinel::shutdown_all_sentinels();
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            std::process::exit(0);
        }
    });
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::WARN)
        .with_writer(std::io::stderr)
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
    info!(
        "🔊 LiveKit agent manager ready (URL: {})",
        livekit_manager.url()
    );

    // Initialize Hippocampus memory subsystem with shared embedding provider.
    // Uses EmbeddingModule's MODEL_CACHE for ONE fastembed model across entire runtime.
    // Model loads lazily on first embed call (~100ms), then ~5ms per embed.
    info!("🧠 Initializing Hippocampus with shared embedding provider...");
    let embedding_provider: Arc<dyn continuum_core::memory::EmbeddingProvider> =
        Arc::new(ModuleBackedEmbeddingProvider::default_model());
    info!(
        "✅ Hippocampus ready: {} ({}D, shared with EmbeddingModule)",
        embedding_provider.name(),
        embedding_provider.dimensions()
    );
    let memory_manager = Arc::new(PersonaMemoryManager::new(embedding_provider));

    // Capture tokio runtime handle for async operations from IPC thread
    let rt_handle = tokio::runtime::Handle::current();

    // Start IPC server in background thread FIRST (creates socket immediately)
    let ipc_livekit_manager = livekit_manager.clone();
    let ipc_memory_manager = memory_manager.clone();
    let ipc_handle = std::thread::spawn(move || {
        if let Err(e) = start_server(
            &socket_path,
            ipc_livekit_manager,
            rt_handle,
            ipc_memory_manager,
        ) {
            tracing::error!("❌ IPC server error: {}", e);
        }
    });

    // Give IPC server time to create socket (satisfies start-workers.sh check)
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Start memory pressure monitor — own task, non-blocking, crash-proof.
    // Polls every 2s, publishes via watch channel. Modules subscribe to react.
    // Start with empty reporters — Bevy might not be ready yet (race condition).
    let pressure_monitor =
        continuum_core::system_resources::MemoryPressureMonitor::start(Vec::new());

    // Delayed reporter registration: wait for Bevy to finish initializing,
    // then register its memory reporter. Retries every 2s for up to 30s.
    let pm_clone = pressure_monitor.clone();
    tokio::spawn(async move {
        for attempt in 0..15 {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            if let Some(bevy) = continuum_core::live::video::bevy_renderer::try_get() {
                let reporter = Arc::new(
                    continuum_core::live::video::memory_reporter::BevyMemoryReporter::new(
                        bevy.memory_stats.clone(),
                        bevy.command_sender(),
                    ),
                );
                pm_clone.add_reporter(reporter);
                info!("🧠 Bevy memory reporter registered (attempt {})", attempt + 1);
                return;
            }
        }
        tracing::warn!("🧠 Bevy memory reporter NOT registered after 30s — Bevy may not be running");
    });

    // Initialize TTS/STT in background (non-blocking - happens after startup)
    tokio::spawn(async {
        // Initialize STT registry and adapters
        continuum_core::live::audio::stt::init_registry();
        match continuum_core::live::audio::stt::initialize().await {
            Ok(_) => {
                info!("✅ STT adapter initialized successfully");
            }
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
        continuum_core::live::audio::tts::init_registry();
        match continuum_core::live::audio::tts::initialize().await {
            Ok(_) => {
                info!("✅ TTS adapter initialized successfully");
            }
            Err(e) => {
                tracing::warn!(
                    "⚠️  TTS adapter not available: {}. TTS will use fallback (silence).",
                    e
                );
                tracing::warn!(
                    "   Download Piper ONNX from https://huggingface.co/rhasspy/piper-voices"
                );
                tracing::warn!("   Place in: models/piper/");
            }
        }
    });

    // Install signal handlers BEFORE declaring ready — ensures cleanup on any exit path
    install_shutdown_handlers();

    // Server is ready — wait for IPC thread (runs until process exits)
    info!("✅ Continuum Core Server fully started");
    let _ = ipc_handle.join();

    Ok(())
}
