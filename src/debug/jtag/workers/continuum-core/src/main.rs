/// Continuum Core Server - Combined IPC + WebSocket Voice Server
///
/// Rust-first architecture for concurrent AI persona system.
/// Provides:
/// - VoiceOrchestrator and PersonaInbox via Unix socket IPC
/// - WebSocket call server for live audio (replaces streaming-core)
///
/// Usage: continuum-core-server <socket-path> <logger-socket-path>
/// Example: continuum-core-server /tmp/continuum-core.sock /tmp/jtag-logger-worker.sock

use continuum_core::{init_logger, start_server};
use std::env;
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
    if args.len() < 3 {
        eprintln!("Usage: {} <socket-path> <logger-socket-path>", args[0]);
        eprintln!("Example: {} /tmp/continuum-core.sock /tmp/jtag-logger-worker.sock", args[0]);
        std::process::exit(1);
    }

    let socket_path = args[1].clone();
    let logger_socket_path = &args[2];

    // Initialize logger
    match init_logger(logger_socket_path) {
        Ok(_) => info!("✅ Logger initialized"),
        Err(e) => {
            eprintln!("❌ Failed to initialize logger: {e}");
            eprintln!("   (Server will continue without logging)");
        }
    }

    info!("🦀 Continuum Core Server starting...");
    info!("   IPC Socket: {socket_path}");
    info!("   Logger: {logger_socket_path}");

    // Start IPC server in background thread FIRST (creates socket immediately)
    let ipc_handle = std::thread::spawn(move || {
        if let Err(e) = start_server(&socket_path) {
            tracing::error!("❌ IPC server error: {}", e);
        }
    });

    // Give IPC server time to create socket (satisfies start-workers.sh check)
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Start WebSocket call server for live audio
    let call_port = get_call_server_port();
    let call_addr = format!("127.0.0.1:{call_port}");
    info!("🎙️  Call WebSocket server starting on ws://{call_addr}");
    let call_server_handle = tokio::spawn(async move {
        if let Err(e) = continuum_core::voice::call_server::start_call_server(&call_addr).await {
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
