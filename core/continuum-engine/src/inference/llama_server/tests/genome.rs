//! Genome regression fixtures.
#![cfg(test)]
use super::*;
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
    *process.served_adapters.lock().unwrap() = vec!["/genes/a.gguf".into(), "/genes/b.gguf".into()]; // JUSTIFIED: test-owned lock, never poisoned
    let dir = tempfile::tempdir().expect("test: tempdir");
    let got = process
        .converge_genome(dir.path(), &target_with_adapters("coder-14b", &["/genes/c.gguf"]))
        .await
        .expect("test: the engine answered");
    assert_eq!(got, GenomeInPlace::Busy);
    assert_eq!(
        *process.served_adapters.lock().unwrap(), // JUSTIFIED: test-owned lock, never poisoned
        vec!["/genes/b.gguf".to_string()],
        "A was freed and B still held: the record says exactly that"
    );
    assert_eq!(loads.load(Ordering::SeqCst), 0, "no new gene joins ahead of a refused retire");
    server.abort();
}
