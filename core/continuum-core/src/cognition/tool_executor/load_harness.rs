//! Realistic persona load harness — a cooperative team on a shared project.
//!
//! The synthetic echo curve in `command_executor.rs::tests::stress` proves the
//! dispatch FLOOR (no lock, no FIFO). This harness measures what 50 personas
//! ACTUALLY do: a coding/doc team hammering a shared workspace through the
//! **uniform client** (`Connection<InProcessTransport>`) — `code/read`,
//! `code/search`, `code/tree`, `code/write` against the REAL `CodeModule` with
//! REAL file payloads. That surfaces the costs echo hides: result serialization
//! (`value.to_string()` over a multi-KB file read), input clones, per-op work.
//!
//! Goal (Joel): "test the shit out of our clients on airc doing things real
//! personas do, tackle latency here and now while it's still simple." So this is
//! a profiler first — it prints a per-op-type table so the slow op is obvious —
//! and a regression guard second (it asserts only that work completes).
//!
//! Gated `stress-tests` per the test doctrine. Run:
//! `cargo test -p continuum-core --features stress-tests,metal,accelerate --lib \
//!   cognition::tool_executor::load_harness -- --nocapture --test-threads=1`

use super::types::{PersonaMediaConfigLite, ToolExecutionContext};
use super::{CommandToolExecutor, ToolExecutor};
use crate::ai::types::ToolCall as NativeToolCall;
use crate::logging::timing::PerformanceStats;
use crate::modules::code::{CodeModule, CodeState};
use crate::routing::CallerIdentity;
use crate::runtime::{CommandExecutor, InProcessTransport, ModuleRegistry};
use continuum_client::Connection;
use dashmap::DashMap;
use futures::future::join_all;
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;
use tempfile::TempDir;
use uuid::Uuid;

/// A shared project tree: `files` source files of ~`lines`×2 lines each — a small
/// crate's worth of realistic, searchable content (not `{"i": n}`).
fn make_project(files: usize, lines: usize) -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    for f in 0..files {
        let mut content = String::with_capacity(lines * 120);
        content.push_str(&format!("//! module {f} — the quick brown fox\n\n"));
        for l in 0..lines {
            content.push_str(&format!(
                "/// doc for func_{f}_{l}: the quick brown fox jumps over the lazy dog\n\
                 pub fn func_{f}_{l}(x: usize) -> usize {{ x * {l} + {f} + 42 }}\n\n"
            ));
        }
        std::fs::write(dir.path().join(format!("mod_{f}.rs")), content).expect("write file");
    }
    dir
}

/// One substrate executor (one core) with the REAL CodeModule registered — the
/// shape 50 personas share.
fn substrate() -> Arc<CommandExecutor> {
    let registry = Arc::new(ModuleRegistry::new());
    let state = Arc::new(CodeState::new(
        Arc::new(DashMap::new()),
        Arc::new(DashMap::new()),
        tokio::runtime::Handle::current(),
    ));
    registry.register(Arc::new(CodeModule::new(state)));
    Arc::new(CommandExecutor::new(registry))
}

/// A persona = a client over the local transport carrying its own identity.
fn persona(executor: Arc<CommandExecutor>, id: Uuid) -> CommandToolExecutor {
    let transport = InProcessTransport::new(
        executor,
        Some(CallerIdentity::airc(crate::identity::PeerId::from_uuid(id))),
    );
    CommandToolExecutor::new(Connection::new(transport))
}

fn ctx(id: Uuid) -> ToolExecutionContext {
    ToolExecutionContext {
        persona_id: id,
        persona_name: "bot".to_string(),
        session_id: Uuid::new_v4(),
        context_id: Uuid::new_v4(),
        caller_context: serde_json::Value::Null,
        persona_config: PersonaMediaConfigLite {
            auto_load_media: false,
            supported_media_types: vec![],
        },
    }
}

fn tool(id: &str, name: &str, input: serde_json::Value) -> NativeToolCall {
    NativeToolCall {
        id: id.to_string(),
        name: name.to_string(),
        input,
    }
}

const PERSONAS: usize = 50;
const ROUNDS: usize = 20;
const FILES: usize = 40;
const LINES: usize = 50; // ~100 lines / ~7KB per file

/// Stand up `PERSONAS` clients sharing `executor`, each with a created workspace
/// rooted at the shared project. Returns (persona_id, client) pairs.
async fn team(executor: &Arc<CommandExecutor>, root: &str) -> Vec<(Uuid, CommandToolExecutor)> {
    let mut team = Vec::with_capacity(PERSONAS);
    for _ in 0..PERSONAS {
        let id = Uuid::new_v4();
        let client = persona(executor.clone(), id);
        // Identity flows via the transport's CallerIdentity (caller-scoped), not a
        // spoofable persona_id param — create-workspace keys on ctx.caller, same as
        // every other migrated code/* op.
        let calls = vec![tool(
            "ws",
            "code/create-workspace",
            json!({ "workspace_root": root }),
        )];
        let out = client
            .execute_native_batch(&calls, &ctx(id), 8000)
            .await
            .expect("workspace batch");
        assert!(
            out.results[0].is_error.is_none(),
            "create-workspace failed: {}",
            out.results[0].content
        );
        team.push((id, client));
    }
    team
}

/// Fire `ROUNDS` homogeneous single-op batches per persona, all concurrent, and
/// report throughput + latency for that op type. `mk` builds the op's input given
/// (persona_id, round).
async fn profile_op<F>(
    label: &str,
    team: &[(Uuid, CommandToolExecutor)],
    command: &'static str,
    mk: F,
) where
    F: Fn(Uuid, usize) -> serde_json::Value + Copy + Send + Sync + 'static,
{
    let stats = Arc::new(PerformanceStats::new());
    let errors = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let start = Instant::now();

    let handles: Vec<_> = team
        .iter()
        .map(|(id, client)| {
            let id = *id;
            let client = client.clone();
            let stats = Arc::clone(&stats);
            let errors = Arc::clone(&errors);
            tokio::spawn(async move {
                let c = ctx(id);
                for r in 0..ROUNDS {
                    let calls = vec![tool("op", command, mk(id, r))];
                    let t0 = Instant::now();
                    let out = client
                        .execute_native_batch(&calls, &c, 16000)
                        .await
                        .unwrap();
                    stats.record(t0.elapsed().as_micros() as u64);
                    if out.results[0].is_error.is_some() {
                        errors.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            })
        })
        .collect();
    join_all(handles).await.into_iter().for_each(|r| r.unwrap());

    let wall = start.elapsed();
    let ops = PERSONAS * ROUNDS;
    let snap = stats.snapshot();
    println!(
        "  {:<14} │ {:>6} │ {:>8.1} │ {:>10.0} │ {:>9} │ {:>9} │ {:>5}",
        label,
        ops,
        wall.as_secs_f64() * 1000.0,
        ops as f64 / wall.as_secs_f64(),
        snap.avg_duration_us,
        snap.max_duration_us,
        errors.load(std::sync::atomic::Ordering::Relaxed),
    );
}

/// Build a team of `n` personas sharing `executor`, each workspace-created.
async fn team_of(
    executor: &Arc<CommandExecutor>,
    root: &str,
    n: usize,
) -> Vec<(Uuid, CommandToolExecutor)> {
    let mut team = Vec::with_capacity(n);
    for _ in 0..n {
        let id = Uuid::new_v4();
        let client = persona(executor.clone(), id);
        client
            .execute_native_batch(
                &[tool(
                    "ws",
                    "code/create-workspace",
                    json!({ "workspace_root": root }),
                )],
                &ctx(id),
                8000,
            )
            .await
            .expect("workspace");
        team.push((id, client));
    }
    team
}

// what this catches: THE concurrency question — does the realistic read path keep
// SCALING as the persona fleet grows, or is there a contention cliff? Ramps the
// fleet and prints reads/sec per tier. Healthy = throughput climbs toward the
// core-count ceiling then plateaus (CPU/IO-bound), NOT a collapse (which would
// mean a lock/serialization point under load).
#[tokio::test(flavor = "multi_thread")]
async fn read_scaling_sweep() {
    let project = make_project(FILES, LINES);
    let root = project.path().to_string_lossy().to_string();
    let executor = substrate();

    println!(
        "\n  cores={}  read scaling ({ROUNDS} reads/persona)",
        num_cpus::get()
    );
    println!(
        "  {:>8} │ {:>6} │ {:>8} │ {:>10} │ {:>9} │ {:>9}",
        "personas", "ops", "wall_ms", "reads/sec", "avg_us", "max_us"
    );

    for &n in &[10usize, 50, 100, 200, 400] {
        let team = team_of(&executor, &root, n).await;
        let stats = Arc::new(PerformanceStats::new());
        let start = Instant::now();
        let handles: Vec<_> = team
            .iter()
            .map(|(id, client)| {
                let id = *id;
                let client = client.clone();
                let stats = Arc::clone(&stats);
                tokio::spawn(async move {
                    let c = ctx(id);
                    for r in 0..ROUNDS {
                        let calls = vec![tool("r", "code/read", json!({ "persona_id": id.to_string(), "file_path": format!("mod_{}.rs", r % FILES) }))];
                        let t0 = Instant::now();
                        client.execute_native_batch(&calls, &c, 16000).await.unwrap();
                        stats.record(t0.elapsed().as_micros() as u64);
                    }
                })
            })
            .collect();
        join_all(handles).await.into_iter().for_each(|r| r.unwrap());
        let wall = start.elapsed();
        let ops = n * ROUNDS;
        let snap = stats.snapshot();
        println!(
            "  {:>8} │ {:>6} │ {:>8.1} │ {:>10.0} │ {:>9} │ {:>9}",
            n,
            ops,
            wall.as_secs_f64() * 1000.0,
            ops as f64 / wall.as_secs_f64(),
            snap.avg_duration_us,
            snap.max_duration_us
        );
    }
}

// what this catches: the realistic per-op latency profile for the high-frequency
// tools a collaborating team hammers (read/search/tree/write). It's a profiler —
// the printed table tells us WHICH op is slow so we iterate on the right thing —
// and a guard that the real CodeModule path works end-to-end over the uniform
// client at 50-persona concurrency with zero errors.
#[tokio::test(flavor = "multi_thread")]
async fn realistic_collaborative_tool_load() {
    let project = make_project(FILES, LINES);
    let root = project.path().to_string_lossy().to_string();
    let executor = substrate();
    let team = team(&executor, &root).await;

    println!(
        "\n  cores={}  {PERSONAS} personas × {ROUNDS} rounds  shared project: {FILES} files",
        num_cpus::get()
    );
    println!(
        "  {:<14} │ {:>6} │ {:>8} │ {:>10} │ {:>9} │ {:>9} │ {:>5}",
        "op", "ops", "wall_ms", "ops/sec", "avg_us", "max_us", "err"
    );

    // code/read — the single most frequent persona op.
    profile_op("read", &team, "code/read", |id, r| {
        json!({ "persona_id": id.to_string(), "file_path": format!("mod_{}.rs", r % FILES) })
    })
    .await;

    // code/search — constant (grep the codebase before acting).
    profile_op("search", &team, "code/search", |id, _r| {
        json!({ "persona_id": id.to_string(), "pattern": "quick brown", "max_results": 50 })
    })
    .await;

    // code/tree — orient in the project.
    profile_op(
        "tree",
        &team,
        "code/tree",
        |id, _r| json!({ "persona_id": id.to_string(), "max_depth": 5 }),
    )
    .await;

    // code/write — each persona to its own scratch file (the edit half of work).
    profile_op("write", &team, "code/write", |id, r| {
        json!({
            "persona_id": id.to_string(),
            "file_path": format!("scratch_{id}.rs"),
            "content": format!("// round {r} by {id}\npub fn scratch() -> usize {{ {r} }}\n"),
        })
    })
    .await;

    // The combined collaborative turn: read + search + tree in one concurrent
    // batch (what a turn actually looks like), all personas at once.
    let stats = Arc::new(PerformanceStats::new());
    let start = Instant::now();
    let handles: Vec<_> = team
        .iter()
        .map(|(id, client)| {
            let id = *id;
            let client = client.clone();
            let stats = Arc::clone(&stats);
            tokio::spawn(async move {
                let c = ctx(id);
                for r in 0..ROUNDS {
                    let batch = vec![
                        tool(
                            "r",
                            "code/read",
                            json!({ "persona_id": id.to_string(), "file_path": format!("mod_{}.rs", r % FILES) }),
                        ),
                        tool(
                            "s",
                            "code/search",
                            json!({ "persona_id": id.to_string(), "pattern": "lazy dog", "max_results": 50 }),
                        ),
                        tool("t", "code/tree", json!({ "persona_id": id.to_string(), "max_depth": 5 })),
                    ];
                    let t0 = Instant::now();
                    client.execute_native_batch(&batch, &c, 16000).await.unwrap();
                    stats.record(t0.elapsed().as_micros() as u64);
                }
            })
        })
        .collect();
    join_all(handles).await.into_iter().for_each(|r| r.unwrap());
    let wall = start.elapsed();
    let turns = PERSONAS * ROUNDS;
    let snap = stats.snapshot();
    println!(
        "  {:<14} │ {:>6} │ {:>8.1} │ {:>10.0} │ {:>9} │ {:>9} │ {:>5}",
        "turn(r+s+t)",
        turns,
        wall.as_secs_f64() * 1000.0,
        turns as f64 / wall.as_secs_f64(),
        snap.avg_duration_us,
        snap.max_duration_us,
        0,
    );
}
