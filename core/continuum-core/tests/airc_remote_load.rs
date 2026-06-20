//! Baseline load measurement for the FULL airc remote path — the *before*
//! number for the body-opaque command-envelope optimization.
//!
//! The in-process path (`InProcessTransport`) is already proven fast + scaling
//! (see `cognition::tool_executor::load_harness`). This harness measures the
//! REMOTE path that mobile / cli / cross-grid personas ride: a real command,
//! driven by `continuum_client::Connection<AircIpcTransport>`, all the way
//! through airc and the substrate's REAL handler chain —
//!
//!   client encode (AircCommandRequest → Body::Json, build_headers)
//!     → airc wire (TwoAircLoopback, real sockets)
//!       → CommandRequestHandler::parse_envelope  (decode the wrapper from the body)
//!         → process_request → CommandExecutor::execute  (the real dispatch + gate)
//!           → send_reply (AircCommandResponse → wire)
//!             → client decode
//!
//! This is the exact chain the body-opaque change targets: today `params` is
//! deep-walked into an `AircCommandRequest` wrapper on encode and walked back
//! out on decode, and `path`/`kind`/`env` ride in BOTH headers and body. The
//! optimization makes the body the opaque `params` and routes on headers only.
//! Run this before + after to prove the win.
//!
//! Gated `stress-tests` (real sockets + multi-task). Run:
//! `cargo test -p continuum-core --features stress-tests,metal,accelerate \
//!   --test airc_remote_load -- --nocapture --test-threads=1`
#![cfg(feature = "stress-tests")]

use std::any::Any;
use std::sync::Arc;
use std::time::{Duration, Instant};

use airc_test_fixtures::TwoAircLoopback;
use continuum_airc_protocol::{COMMAND_REQUEST_BODY_HINT, HEADER_CONTINUUM_BODY_HINT};
use continuum_client::Connection;
use continuum_core::routing::CommandRequestHandler;
use continuum_core::runtime::{
    CommandExecutor, CommandResult, ModuleConfig, ModuleContext, ModulePriority, ModuleRegistry,
    ServiceModule,
};
use futures::stream::StreamExt;
use serde_json::{json, Value};

/// Echoes its params back — a cheap command so the measurement isolates the
/// TRANSPORT + envelope cost (encode/wire/decode), which is what body-opaque
/// optimizes, rather than command work.
struct EchoModule;

#[async_trait::async_trait]
impl ServiceModule for EchoModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "echo",
            priority: ModulePriority::Normal,
            command_prefixes: &["bench/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }
    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }
    async fn handle_command(&self, _command: &str, params: Value) -> Result<CommandResult, String> {
        Ok(CommandResult::Json(params))
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Spawn the REAL substrate command path on `peer_a`: subscribe, filter command
/// frames by body-hint, and run `parse_envelope → process_request → send_reply`
/// per request (each on its own task, so the substrate side is concurrent — the
/// realistic shape).
async fn spawn_substrate(handler: Arc<CommandRequestHandler>, peer_a: Arc<airc_lib::Airc>) {
    let self_id = peer_a.peer_id();
    tokio::spawn(async move {
        let mut stream = peer_a.subscribe().await.expect("peer_a subscribe");
        while let Some(event) = stream.next().await {
            let event = match event {
                Ok(e) => e,
                Err(_) => continue,
            };
            if event.peer_id == self_id {
                continue;
            }
            let is_command = event
                .headers
                .get(HEADER_CONTINUUM_BODY_HINT)
                .map(|h| h == COMMAND_REQUEST_BODY_HINT)
                .unwrap_or(false);
            if !is_command {
                continue;
            }
            let handler = Arc::clone(&handler);
            tokio::spawn(async move {
                if let Ok(parsed) = CommandRequestHandler::parse_envelope(&event) {
                    let response = handler.process_request(&parsed).await;
                    let _ = handler.send_reply(&parsed, &response).await;
                }
            });
        }
    });
}

/// Merge latency samples into (avg_us, max_us).
fn stats(samples: &[u64]) -> (u64, u64) {
    if samples.is_empty() {
        return (0, 0);
    }
    let sum: u64 = samples.iter().sum();
    let max = *samples.iter().max().unwrap();
    (sum / samples.len() as u64, max)
}

// what this catches: the BASELINE throughput/latency of the full airc remote
// command path, per client-fleet size. This is the measurement we optimize
// against — re-run after the body-opaque envelope change and compare. It also
// guards that the full chain (client → wire → real handler → executor → reply)
// works concurrently across many client connections with zero errors.
#[tokio::test(flavor = "multi_thread")]
async fn airc_remote_command_path_baseline() {
    const ROUNDS: usize = 20;
    let tiers = [1usize, 10, 50];

    let loop_back = TwoAircLoopback::new().await.expect("loopback");
    // peer_a = substrate with a REAL executor + EchoModule registered.
    let registry = Arc::new(ModuleRegistry::new());
    registry.register(Arc::new(EchoModule));
    let executor = Arc::new(CommandExecutor::new(registry));
    let handler = CommandRequestHandler::new(Arc::clone(loop_back.peer_a()), executor);
    spawn_substrate(Arc::clone(&handler), Arc::clone(loop_back.peer_a())).await;
    // Let the subscribe filter arm before clients fire.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let peer_b = Arc::clone(loop_back.peer_b());
    let peer_a_id = loop_back.peer_a_id();

    println!("\n  FULL airc remote path (loopback sockets), {ROUNDS} reqs/client");
    println!(
        "  {:>7} │ {:>6} │ {:>8} │ {:>10} │ {:>9} │ {:>9} │ {:>5}",
        "clients", "ops", "wall_ms", "ops/sec", "avg_us", "max_us", "err"
    );

    for &n in &tiers {
        let start = Instant::now();
        let handles: Vec<_> = (0..n)
            .map(|_| {
                let conn = Connection::connect(Arc::clone(&peer_b), peer_a_id);
                tokio::spawn(async move {
                    let mut samples = Vec::with_capacity(ROUNDS);
                    let mut errors = 0usize;
                    for r in 0..ROUNDS {
                        let t0 = Instant::now();
                        let res: Result<Value, _> = conn
                            .commands()
                            .execute("bench/echo", json!({ "r": r, "msg": "hello team" }))
                            .await;
                        samples.push(t0.elapsed().as_micros() as u64);
                        if res.is_err() {
                            errors += 1;
                        }
                    }
                    (samples, errors)
                })
            })
            .collect();

        let mut all = Vec::with_capacity(n * ROUNDS);
        let mut errors = 0usize;
        for h in futures::future::join_all(handles).await {
            let (samples, errs) = h.expect("client task");
            all.extend(samples);
            errors += errs;
        }
        let wall = start.elapsed();
        let ops = n * ROUNDS;
        let (avg, max) = stats(&all);
        println!(
            "  {:>7} │ {:>6} │ {:>8.1} │ {:>10.0} │ {:>9} │ {:>9} │ {:>5}",
            n,
            ops,
            wall.as_secs_f64() * 1000.0,
            ops as f64 / wall.as_secs_f64(),
            avg,
            max,
            errors
        );
        assert_eq!(errors, 0, "all remote ops completed at clients={n}");
    }
}
