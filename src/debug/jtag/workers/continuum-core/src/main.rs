/// Continuum Core Server - Unix Socket IPC Server
///
/// Rust-first architecture for concurrent AI persona system.
/// Provides VoiceOrchestrator and PersonaInbox via Unix socket IPC.
///
/// Usage: continuum-core-server <socket-path> <logger-socket-path>
/// Example: continuum-core-server /tmp/continuum-core.sock /tmp/jtag-logger-worker.sock

use continuum_core::{init_logger, start_server};
use std::env;

fn main() -> std::io::Result<()> {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <socket-path> <logger-socket-path>", args[0]);
        eprintln!("Example: {} /tmp/continuum-core.sock /tmp/jtag-logger-worker.sock", args[0]);
        std::process::exit(1);
    }

    let socket_path = &args[1];
    let logger_socket_path = &args[2];

    // Initialize logger
    match init_logger(logger_socket_path) {
        Ok(_) => println!("✅ Logger initialized"),
        Err(e) => {
            eprintln!("❌ Failed to initialize logger: {e}");
            eprintln!("   (Server will continue without logging)");
        }
    }

    println!("🦀 Continuum Core Server starting...");
    println!("   Socket: {socket_path}");
    println!("   Logger: {logger_socket_path}");

    // Start IPC server (blocks here, event-driven)
    start_server(socket_path)?;

    Ok(())
}
