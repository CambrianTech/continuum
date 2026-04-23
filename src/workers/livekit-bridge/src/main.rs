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

    // Parse command line arguments. argv[1] is the IPC socket path (positional)
    // — but intercept flag-like values FIRST so `--version` and `--help` don't
    // get treated as a socket path. Without this, `livekit-bridge --version`
    // boots trying to bind "/--version" as the socket path, hanging on a
    // connection that never arrives. Same failure mode as continuum-core-server
    // before a79bd56f0 fixed that; Carl runs `docker pull` then tries --version
    // to verify the image works, and gets a hang instead of a version string.
    let args: Vec<String> = env::args().collect();
    if args.len() >= 2 {
        match args[1].as_str() {
            "-V" | "--version" | "version" => {
                println!("livekit-bridge {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "-h" | "--help" | "help" => {
                println!("Usage: {} <socket-path> [--livekit-url <url>]", args[0]);
                println!("Example: {} /tmp/livekit-bridge.sock", args[0]);
                println!();
                println!("Flags:");
                println!("  -V, --version           Print version and exit");
                println!("  -h, --help              Print this help and exit");
                println!("      --livekit-url URL   LiveKit server URL (default ws://localhost:7880, or $LIVEKIT_URL)");
                std::process::exit(0);
            }
            _ => {}
        }
    }
    if args.len() < 2 {
        eprintln!("Usage: {} <socket-path> [--livekit-url <url>]", args[0]);
        eprintln!("Try `{} --help` for more.", args[0]);
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
