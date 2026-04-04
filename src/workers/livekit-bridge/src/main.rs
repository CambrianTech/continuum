//! LiveKit Bridge — transport adapter between continuum-core and LiveKit WebRTC SFU.
//!
//! This binary contains ALL LiveKit/webrtc-sys code. It is the ONLY process that
//! links webrtc-sys. continuum-core links ort (ONNX Runtime) but NOT webrtc-sys.
//! This eliminates the C++ protobuf symbol conflict that caused runtime deadlocks.
//!
//! Architecture:
//!   continuum-core ←→ [Unix Socket IPC] ←→ livekit-bridge ←→ [WebRTC] ←→ LiveKit Server
//!
//! The bridge:
//!   - Receives commands from core (join room, speak, publish video, etc.)
//!   - Manages LiveKit room connections for AI persona agents
//!   - Streams audio frames from human participants back to core for VAD/STT
//!   - Publishes TTS audio and avatar video frames from core into LiveKit rooms
//!
//! Usage: livekit-bridge <socket-path> [--livekit-url <url>]
//! Example: livekit-bridge /root/.continuum/sockets/livekit-bridge.sock

mod agent;
mod server;

use std::env;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_writer(std::io::stderr)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <socket-path> [--livekit-url <url>]", args[0]);
        std::process::exit(1);
    }

    let socket_path = &args[1];
    let livekit_url = args
        .iter()
        .position(|a| a == "--livekit-url")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            env::var("LIVEKIT_URL").unwrap_or_else(|_| "ws://localhost:7880".to_string())
        });

    info!("🌉 LiveKit Bridge starting...");
    info!("   IPC Socket: {}", socket_path);
    info!("   LiveKit URL: {}", livekit_url);

    server::run(socket_path, &livekit_url).await?;

    Ok(())
}
