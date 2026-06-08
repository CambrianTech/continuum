// jemalloc: returns memory to OS aggressively instead of hoarding pages.
// macOS system allocator fragments badly under Bevy's 15fps readback churn
// (14 slots × 921KB per frame) — RSS grows to 30-40GB and never shrinks.
// jemalloc's dirty page purging returns freed memory within seconds.
//
// MALLOC_CONF: dirty_decay_ms=1000 (purge freed pages after 1s instead of default 10s),
// muzzy_decay_ms=2000 (return muzzy pages after 2s instead of default 10s).
// On a 32GB machine with 15 bursty personas, the default 10s window accumulates
// 3-5GB of "owned unmapped memory" that inflates RSS.
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[allow(non_upper_case_globals)]
#[unsafe(export_name = "malloc_conf")]
pub static malloc_conf: &[u8] = b"dirty_decay_ms:1000,muzzy_decay_ms:2000\0";

use continuum_core::live::transport::bridge_client::LiveKitAgentManager;
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
use continuum_core::routing::{install_probe_tracing, ProbeTracingConfig};
use continuum_core::start_server;
use std::env;
use std::sync::Arc;
use tracing::info;

/// Install signal handlers that kill all sentinel process groups on shutdown.
/// This prevents orphaned training processes from eating memory after npm stop.
///
/// Exit semantics: we use `libc::_exit` (the syscall) instead of
/// `std::process::exit` (which runs C++ static destructors via
/// `__cxa_finalize_ranges`). Reason: the process holds raw pointers to
/// llama.cpp objects (Model, Context, LoraAdapter, MtmdContext) whose Rust
/// `Drop` impls call `llama_*_free` from libllama. If those drops race with
/// libllama's own static destructors during atexit teardown, we double-free
/// and SIGABRT. The crash signature is:
///   `tokio-rt-worker → __cxa_finalize_ranges → continuum-core destructor → abort()`
///
/// `_exit` skips all atexit handlers + Rust drops + libc cleanup → kernel
/// reclaims memory + closes FDs + unmaps mmaps. Buffered stdout would be
/// lost, but tracing writes to stderr per-line and we eprintln! the
/// shutdown message before exiting, so no diagnostic loss in practice.
///
/// The `Drop` impls remain correct for normal lifetime — model unload,
/// context swap, etc. We're only short-circuiting the process-exit path.
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
            unsafe { libc::_exit(0) };
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
            unsafe { libc::_exit(0) };
        }
    });
}

/// Short human-readable description of each `BootMode` — surfaces
/// in the boot banner so the operator can see at a glance what the
/// substrate is expected to support in this run.
/// Register a BevyMemoryReporter with the pressure monitor once the
/// Bevy renderer's ready edge fires. Pulled out of the inline async
/// task so the subscribe-and-await loop reads as one signal-shaped
/// block. Called at most once per process lifetime; a Bevy restart
/// would re-fire the edge but the existing reporter stays valid.
fn register_bevy_reporter(
    pressure_monitor: &std::sync::Arc<continuum_core::system_resources::MemoryPressureMonitor>,
) {
    let bevy = match continuum_core::live::video::bevy_renderer::try_get() {
        Some(b) => b,
        None => {
            // Ready edge fired but renderer absent — race during shutdown.
            tracing::warn!(
                "🧠 Bevy ready edge fired but try_get returned None; skipping reporter"
            );
            return;
        }
    };
    let reporter = std::sync::Arc::new(
        continuum_core::live::video::memory_reporter::BevyMemoryReporter::new(
            bevy.memory_stats.clone(),
            bevy.command_sender(),
        ),
    );
    pressure_monitor.add_reporter(reporter);
    tracing::info!("🧠 Bevy memory reporter registered via ready edge");
}

fn boot_mode_description(mode: continuum_core::runtime::BootMode) -> &'static str {
    use continuum_core::runtime::BootMode;
    match mode {
        BootMode::FullCitizen => "hosts personas via AIRC; requires AIRC Healthy",
        BootMode::InferenceOnly => "no persona hosting; allows degraded AIRC",
        BootMode::FailFast => "strictest — refuses any degraded capability",
    }
}

// ORT panic-filter intentionally NOT in A.2.1 per reviewer #2's
// BLOCKING finding: shipping the panic trace muter without the
// `🔊 Voice: ready / 🔇 Voice: unavailable` indicator leaves the
// operator with ZERO signal about voice subsystem state on default
// `FullCitizen` boot. Per [[substrate-is-a-good-citizen-on-the-host]]
// "speak clearly when degraded," the filter and the indicator
// must ship together — both land in A.2.2 alongside the
// `libloading::Library::new("libonnxruntime.dylib")` dlopen probe.

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Substrate-canonical tracing stack: UriCapture + ProbeRouter +
    // optional JsonlProbeFileSink + fmt-to-stderr governed by
    // RUST_LOG (default `info`). See
    // docs/architecture/RTOS-DEBUGGER-PROBES.md.
    //
    // Env-coupling lives at exactly one seam: `from_env(...)`. The
    // installer takes only typed values — tests construct config
    // directly without racing `std::env::set_var`. Setting
    // `CONTINUUM_PROBE_FILE=/tmp/probes.jsonl` lights up the JSONL
    // sink with zero binary changes; the boot log below reports
    // the path so the operator sees it landed.
    let probe_install = install_probe_tracing(ProbeTracingConfig::from_env("info"))?;
    if let Some(ref path) = probe_install.probe_log_path {
        // Use println so it appears even when RUST_LOG filters out
        // info-level tracing events — the operator who just set the
        // env var SHOULD see this confirmation.
        eprintln!("[continuum-core-server] probes landing at {}", path.display());
    }
    // Per `[[never-redirect-substrate-stderr]]`: the substrate now
    // owns its tracing fmt-layer log persistence via a rolling-file
    // sink under `~/.continuum/logs/` (or `$CONTINUUM_LOG_DIR`).
    // Surfacing the path at boot prevents the "where are my logs?"
    // confusion for anyone migrating off the (forbidden)
    // `npm start 2>&1 | tee /tmp/server.log` pattern.
    if let Some(ref dir) = probe_install.log_dir {
        eprintln!(
            "[continuum-core-server] logs landing at {}/continuum-core-server.YYYY-MM-DD.log (rolling daily, retention 7)",
            dir.display()
        );
    }
    // CRITICAL: hold the non-blocking writer's WorkerGuard for the
    // process lifetime. Dropping it flushes + shuts down the
    // background writer thread; we MUST keep it alive so tail-of-
    // process log lines reach disk. Underscore-prefixed binding
    // keeps Rust quiet about "unused" while still binding (vs.
    // `let _ = ...` which drops immediately).
    let _log_writer_guard = probe_install.fmt_writer_guard;

    // ORT panic-filter deferred to A.2.2 (lands together with the
    // libonnxruntime dlopen probe + 🔊/🔇 voice subsystem indicator
    // per reviewer #2's BLOCKING finding — muting the only signal
    // without pairing the indicator is the [[substrate-is-a-good-
    // citizen-on-the-host]] violation we're refusing to ship in
    // A.2.1).

    // Parse command line arguments. argv[1] is the IPC socket path (positional)
    // — but intercept flag-like values FIRST so `--version` and `--help` don't
    // get treated as a socket path. Without this, `continuum-core-server
    // --version` boots the server with "/--version" as the socket path
    // and prints "IPC Socket: --version" — confusing for anyone trying to
    // verify the binary works (Carl's first instinct after `docker pull`).
    let raw_args: Vec<String> = env::args().collect();

    // A.2: peel off `--mode=<value>` (or `--mode <value>`) BEFORE
    // positional parsing so it can appear anywhere in argv. The
    // operator's intent (FullCitizen / InferenceOnly / FailFast)
    // is now an explicit input instead of a heuristic from disk
    // contents (Slice A's R2#2 BLOCK).
    let (boot_mode, args) = match continuum_core::runtime::extract_boot_mode(raw_args) {
        Ok(parsed) => parsed,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };

    if args.len() >= 2 {
        match args[1].as_str() {
            "-V" | "--version" | "version" => {
                println!("continuum-core-server {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "-h" | "--help" | "help" => {
                println!("Usage: {} [--mode=<MODE>] <socket-path>", args[0]);
                println!("Example: {} /tmp/continuum-core.sock", args[0]);
                println!();
                println!("Flags:");
                println!("  --mode=<MODE>    Boot mode: full-citizen (default), inference-only, fail-fast");
                println!("  -V, --version    Print version and exit");
                println!("  -h, --help       Print this help and exit");
                println!();
                println!("Modes:");
                println!("  full-citizen     (default) hosts personas via AIRC; requires AIRC Healthy");
                println!("  inference-only   no persona hosting; allows degraded AIRC");
                println!("  fail-fast        strictest; refuses any degraded capability");
                std::process::exit(0);
            }
            _ => {}
        }
    }
    if args.len() < 2 {
        eprintln!("Usage: {} [--mode=<MODE>] <socket-path>", args[0]);
        eprintln!("Example: {} /tmp/continuum-core.sock", args[0]);
        eprintln!("Try `{} --help` for more.", args[0]);
        std::process::exit(1);
    }

    let socket_path = args[1].clone();

    info!("🦀 Continuum Core Server starting...");
    info!("   IPC Socket: {socket_path}");
    info!("   Boot mode:  {} ({})", boot_mode.label(), boot_mode_description(boot_mode));

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

    // Start memory pressure monitor — own task, non-blocking, crash-proof.
    // Polls every 2s, publishes via watch channel. Modules subscribe to react.
    // Start with empty reporters — Bevy might not be ready yet (race condition).
    // Created BEFORE IPC server so it can be wired into SystemResourceModule.
    let pressure_monitor =
        continuum_core::system_resources::MemoryPressureMonitor::start(Vec::new());

    // Start disk pressure monitor — same RTOS shape as memory (own task,
    // watch channel, atomic gate, 100 ms reporter timeout + quarantine).
    // Polls every 30 s. The disk-guard slop that triggered the
    // CONCURRENCY-STYLE-GUIDE was a synchronous main-thread probe with
    // env-tunable thresholds; this is the right shape (task #88).
    // Future PR: register as a ResourcePool with PressureBroker so disk
    // pressure participates in cross-resource tier-relief.
    let _disk_pressure_monitor =
        continuum_core::system_resources::DiskPressureMonitor::start(Vec::new());

    // Start IPC server in background thread FIRST (creates socket immediately).
    // The thread publishes `ipc::subscribe_ready()`'s watch once the Unix
    // socket is bound + chmod'd, so main advances on a real signal instead
    // of a 100 ms guess. Per docs/architecture/CONCURRENCY-STYLE-GUIDE.md:
    // signals replace races. Same primitive Bevy renderer + every future
    // ServiceModule's `ready_edge()` uses — one mechanism across the board.
    let ipc_livekit_manager = livekit_manager.clone();
    let ipc_memory_manager = memory_manager.clone();
    let ipc_pressure_monitor = pressure_monitor.clone();
    let mut ipc_ready_rx = continuum_core::ipc::subscribe_ready();
    let ipc_handle = std::thread::spawn(move || {
        if let Err(e) = start_server(
            &socket_path,
            ipc_livekit_manager,
            rt_handle,
            ipc_memory_manager,
            ipc_pressure_monitor,
            boot_mode,
        ) {
            tracing::error!("❌ IPC server error: {}", e);
            // A.2: a `start_server` Err means the substrate refused
            // to boot in a degraded state ([[no-fallbacks-ever]]). The
            // operator's repair is in the error message. Exit non-zero
            // so init systems / orchestrators see the failure instead
            // of an idle-but-alive process.
            unsafe { libc::_exit(1) };
        }
    });

    // Wait for the Unix socket to be bound + world-rw chmod'd. The IPC
    // thread already libc::_exit(1)s on bind failure, so a watch that
    // never transitions means the process is on its way down — main's
    // await will resolve to Err and we fall through to a clean exit
    // with the IPC thread's exit code already in flight.
    if *ipc_ready_rx.borrow_and_update() {
        // fast path
    } else {
        loop {
            if ipc_ready_rx.changed().await.is_err() {
                tracing::error!("❌ IPC ready watch closed before bind — see prior error");
                return Ok(());
            }
            if *ipc_ready_rx.borrow_and_update() {
                break;
            }
        }
    }

    // Delayed reporter registration: subscribe to the Bevy renderer's
    // ready edge and register the memory reporter on the first false→true
    // transition. Replaces the prior 15-attempt 30s sleep-poll loop with
    // a real signal — per docs/architecture/CONCURRENCY-STYLE-GUIDE.md.
    //
    // If Bevy never starts (headless boot with no avatar consumers), the
    // task simply parks on `rx.changed()` for the process lifetime — zero
    // CPU. The 30s give-up window from the old code was misleading anyway:
    // it never timed out the actual Bevy init, only this task's patience.
    let pm_clone = pressure_monitor.clone();
    tokio::spawn(async move {
        let mut rx = continuum_core::live::video::bevy_renderer::subscribe_ready();
        // Fast path: bevy already up before we subscribed.
        if *rx.borrow_and_update() {
            register_bevy_reporter(&pm_clone);
            return;
        }
        loop {
            if rx.changed().await.is_err() {
                // Sender dropped — bevy module shutdown for process exit.
                return;
            }
            if *rx.borrow_and_update() {
                register_bevy_reporter(&pm_clone);
                return;
            }
        }
    });

    // Initialize TTS/STT in background (non-blocking - happens after startup)
    // Wrapped in catch_unwind because ORT panics (not errors) when libonnxruntime.dylib
    // is missing. A missing TTS/STT model must NEVER crash the entire server.
    //
    // CONTINUUM_SKIP_STT=1 skips STT init entirely. whisper-rs vendors its own
    // ggml and races our llama crate's ggml backend registry on Metal — the
    // loser gets NULL back from ggml_backend_metal_init and segfaults inside
    // whisper_kv_cache_init. Skipping until the ggml registries are unified
    // or whisper-rs is taught about our backend.
    let skip_stt = std::env::var("CONTINUUM_SKIP_STT").ok().as_deref() == Some("1");
    let skip_tts = std::env::var("CONTINUUM_SKIP_TTS").ok().as_deref() == Some("1");
    tokio::spawn(async move {
        let result = tokio::task::spawn(async move {
            if skip_stt {
                tracing::warn!("⏭️  STT init skipped (CONTINUUM_SKIP_STT=1)");
            } else {
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
            } // end !skip_stt

            if skip_tts {
                tracing::warn!("⏭️  TTS init skipped (CONTINUUM_SKIP_TTS=1)");
            } else {
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
            } // end !skip_tts
        }).await;

        if let Err(e) = result {
            // ORT panics when libonnxruntime.dylib is missing — catch it here
            // instead of letting it poison the tokio runtime
            tracing::error!(
                "⚠️  TTS/STT initialization panicked (ORT dylib missing?): {:?}",
                e
            );
            tracing::error!(
                "   Voice features disabled. Install libonnxruntime or set ORT_DYLIB_PATH."
            );
        }
    });

    // Install signal handlers BEFORE declaring ready — ensures cleanup on any exit path
    install_shutdown_handlers();

    // Server is ready — wait for IPC thread (runs until process exits)
    info!("✅ Continuum Core Server fully started");
    let _ = ipc_handle.join();

    Ok(())
}
