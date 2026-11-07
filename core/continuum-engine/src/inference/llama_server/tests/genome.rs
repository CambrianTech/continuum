//! Genome regression fixtures.
#![cfg(test)]
use super::*;

#[cfg(windows)]
#[tokio::test]
async fn review_failed_other_binary_probe_preserves_the_first_binary_answer() {
    let key_a = "review-successful-engine-a";
    let key_b = "review-failed-engine-b";
    let help = || {
        let mut command = tokio::process::Command::new("cmd.exe");
        command.args(["/D", "/C", "echo --train-dir PATH"]);
        command
    };
    ensure_engine_kv_support_recorded(key_a, help()).await;
    assert!(engine_accepts_train_dir(key_a), "fixture binary A advertises train-dir");
    let missing = tempfile::tempdir().expect("fixture directory");
    ensure_engine_kv_support_recorded(
        key_b,
        tokio::process::Command::new(missing.path().join("absent-engine.exe")),
    ).await;
    ensure_engine_kv_support_recorded(key_a, help()).await;
    assert!(engine_accepts_train_dir(key_a), "returning to A must retain or re-probe A, never inherit the failed B record");
}
// what this catches (Codex on #4467): two retires where the engine frees the first and
// refuses the second (a turn holds it) left the core advertising BOTH, though the engine
// had freed one. The served set must name exactly what the engine holds after a busy
// stop, and the new gene must not have been loaded ahead of the refused retire.
#[tokio::test]
async fn a_busy_second_retire_leaves_the_served_set_naming_what_the_engine_holds() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let loads = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&loads);
    let app = axum::Router::new()
        .route(
            "/lora-adapters",
            axum::routing::get(|| async {
                axum::Json(serde_json::json!([
                    {"id": 0, "path": "/genes/a.gguf"},
                    {"id": 1, "path": "/genes/b.gguf"}
                ]))
            }),
        )
        .route(
            "/lora-adapters/unload",
            axum::routing::post(|axum::Json(body): axum::Json<serde_json::Value>| async move {
                if body["id"] == 0 {
                    axum::http::StatusCode::OK
                } else {
                    axum::http::StatusCode::SERVICE_UNAVAILABLE
                }
            }),
        )
        .route(
            "/lora-adapters/load",
            axum::routing::post(move || {
                seen.fetch_add(1, Ordering::SeqCst);
                async { axum::http::StatusCode::OK }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
    let root = format!("http://{}", listener.local_addr().expect("test: addr"));
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let process = LlamaServerProcess::with_root(root);
    *process.served_adapters.lock().unwrap() = Some(vec!["/genes/a.gguf".into(), "/genes/b.gguf".into()]); // JUSTIFIED: test-owned lock, never poisoned
    let dir = tempfile::tempdir().expect("test: tempdir");
    let got = process
        .converge_genome(dir.path(), &target_with_adapters("coder-14b", &["/genes/c.gguf"]))
        .await
        .expect("test: the engine answered");
    assert_eq!(got, GenomeInPlace::Busy);
    assert_eq!(
        *process.served_adapters.lock().unwrap(), // JUSTIFIED: test-owned lock, never poisoned
        Some(vec!["/genes/b.gguf".to_string()]),
        "A was freed and B still held: the record says exactly that"
    );
    assert_eq!(loads.load(Ordering::SeqCst), 0, "no new gene joins ahead of a refused retire");
    server.abort();
}

// what this catches (IntelMac 2026-11-07, deploy of 417b65ed3: from=0 to=7): a core adopting a
// live engine it never launched reading the engine's adapter set as empty, so the reconcile saw a
// gene change and relaunched an engine serving 3 adapters cold. A fresh process reads the engine's
// own report once, maps a launch path and a staged name back to the target's genes, and records
// it; a malformed report is an error, never an empty set.
#[tokio::test]
async fn a_fresh_process_reads_an_adopted_engines_genes_from_its_report() {
    let staged = gene_stage_name(std::path::Path::new("/genes/b.gguf")).expect("test: stageable");
    let report = serde_json::json!([
        {"id": 0, "path": "/genes/a.gguf"},
        {"id": 1, "path": format!("/cache/engine-train/{staged}")}
    ]);
    let app = axum::Router::new()
        .route("/lora-adapters", axum::routing::get(move || { let report = report.clone(); async move { axum::Json(report) } }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
    let root = format!("http://{}", listener.local_addr().expect("test: addr"));
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let fresh = LlamaServerProcess::with_root(root);
    let target = target_with_adapters("coder-14b", &["/genes/a.gguf", "/genes/b.gguf"]);
    let held = fresh.active_adapters(&target).await.expect("test: the engine reported its set");
    assert_eq!(held, target.adapter_paths(), "the adopted engine holds the target's genes: no relaunch");
    assert_eq!(*fresh.served_adapters.lock().unwrap(), Some(held), "read once, then recorded"); // JUSTIFIED: test-owned lock, never poisoned
    server.abort();

    let app = axum::Router::new()
        .route("/lora-adapters", axum::routing::get(|| async { axum::Json(serde_json::json!({"not": "an array"})) }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
    let root = format!("http://{}", listener.local_addr().expect("test: addr"));
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let fresh = LlamaServerProcess::with_root(root);
    assert!(fresh.active_adapters(&target).await.is_err(), "a malformed report is an error, never an empty set");
    assert_eq!(*fresh.served_adapters.lock().unwrap(), None, "and nothing is recorded"); // JUSTIFIED: test-owned lock, never poisoned
    server.abort();
}
