//! Integration test (Contract C, Pass 3): boot the REAL `forge-custodian` binary
//! and drive it with the REAL [`ForgeCustodianHttp`] client over loopback HTTP.
//!
//! Unit tests cover the pure pieces (job-id, deadline-kill, param parsing) and the
//! client's URL/handshake logic against fakes. This proves the two HALVES meet:
//!   - the daemon binds where the contract says and serves `/health` (R4),
//!   - the real client's contract-version handshake passes end to end (the whole
//!     anti-drift point of single-sourcing `forge::protocol`),
//!   - `/health` honestly reports `ready` + the configured capacity (R4),
//!   - SIGTERM drains and exits cleanly (R5 graceful shutdown).
//!
//! Isolation: the child runs under a temp `$HOME`, so it reads a throwaway
//! `config.env` and never touches the operator's real config or HF cache. A stub
//! `convert_lora_to_gguf.py` is dropped under the temp home so readiness probes
//! true without needing real llama.cpp.

// unix-only integration target (#304): dials the core UNIX IPC socket /
// sends unix signals. Windows checks compile it to empty; the lib +
// unit tests are the windows-supported surface today.
#![cfg(unix)]

use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant, SystemTime};

use continuum_core::forge::custodian_client::{ForgeCustodian, ForgeCustodianHttp};
use continuum_core::forge::protocol::{CAPABILITY_GGUF_LORA, CONTRACT_VERSION};

/// A unique temp dir under the system temp root (no external tempdir crate).
fn unique_tmp(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("fc-it-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Grab a free loopback port by binding to :0, then releasing it. Small TOCTOU
/// window, acceptable for a test; the daemon re-binds it immediately.
fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    l.local_addr().expect("local_addr").port()
}

fn write_file(path: &PathBuf, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent");
    }
    let mut f = std::fs::File::create(path).expect("create file");
    f.write_all(contents.as_bytes()).expect("write file");
}

#[tokio::test]
async fn custodian_daemon_boots_serves_health_and_shuts_down_gracefully() {
    let home = unique_tmp("home");
    let port = free_port();
    let addr = format!("127.0.0.1:{port}");

    // Throwaway config.env: bind addr + an advertised capacity of 3 slots.
    write_file(
        &home.join(".continuum/config.env"),
        &format!("FORGE_CUSTODIAN_ADDR={addr}\nFORGE_MAX_CONCURRENT=3\n"),
    );
    // Stub converter so readiness probes true (we never invoke it in this test).
    write_file(
        &home.join(".unsloth/llama.cpp/convert_lora_to_gguf.py"),
        "# stub for readiness probe\n",
    );

    let exe = env!("CARGO_BIN_EXE_forge-custodian");
    let mut child = Command::new(exe)
        .env("HOME", &home)
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .expect("spawn forge-custodian");

    // Poll /health via the REAL client until the daemon is up (or time out).
    let client =
        ForgeCustodianHttp::with_base_url(format!("http://{addr}"), reqwest::Client::new());
    let deadline = Instant::now() + Duration::from_secs(15);
    let health = loop {
        match client.health().await {
            Ok(h) => break h,
            Err(_) if Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
            Err(e) => {
                let _ = child.kill();
                panic!("custodian never became healthy within 15s: {e}");
            }
        }
    };

    // R4: honest health — contract handshake + capability + readiness + capacity.
    assert_eq!(
        health.contract_version, CONTRACT_VERSION,
        "served contract version must match what the client compiles against"
    );
    assert_eq!(health.capability, CAPABILITY_GGUF_LORA);
    assert!(health.ready, "stub converter present ⇒ ready=true");
    assert_eq!(health.slots_total, 3, "FORGE_MAX_CONCURRENT=3 advertised");
    assert_eq!(
        health.slots_available, 3,
        "no conversion in flight ⇒ all slots free"
    );

    // The whole anti-drift point: the real client's handshake passes over the wire.
    let confirmed = client
        .ensure_contract()
        .await
        .expect("ensure_contract must pass against a same-version daemon");
    assert_eq!(confirmed.contract_version, CONTRACT_VERSION);

    // R5: SIGTERM → graceful drain → clean exit within a bound.
    let pid = child.id();
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    let exit_deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        match child.try_wait().expect("try_wait") {
            Some(s) => break s,
            None if Instant::now() < exit_deadline => {
                std::thread::sleep(Duration::from_millis(100))
            }
            None => {
                let _ = child.kill();
                panic!("custodian did not exit within 5s of SIGTERM (graceful shutdown stuck)");
            }
        }
    };
    assert!(
        status.success(),
        "graceful shutdown must exit 0, got {status:?}"
    );

    let _ = std::fs::remove_dir_all(&home);
}
