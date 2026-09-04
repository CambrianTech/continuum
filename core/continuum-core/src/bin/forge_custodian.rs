//! `forge-custodian` — the continuum-owned forge custodian sidecar.
//!
//! This process implements the ONE capability unsloth-studio lacks: exporting a
//! trained LoRA as a **GGUF LoRA adapter** — the pageable gene `llama-server
//! --lora` loads and the genome pages in/out (memory
//! `model-endpoint-fabric-adapter-router`, slice 1 supply side).
//!
//! It is the custodian half of task #32's "1a": continuum-core (the organism)
//! POSTs `GenomeFormat::GgufLora` → `/api/export/export/gguf-lora`; this sidecar
//! runs the conversion and OWNS the produced bytes. The organism never runs the
//! converter (memory `fallbacks-are-illegal-fail-loud` boundary: byte custody
//! stays custodian-side).
//!
//! The conversion is two steps, the first now pure Rust:
//!   1. MLX `adapters.safetensors` → HF PEFT dir — [`mlx_adapters_to_peft`]
//!      (no python; see memory `no-python-in-rs-files`).
//!   2. PEFT dir → GGUF LoRA — llama.cpp's `convert_lora_to_gguf.py`, spawned as
//!      a subprocess (the converter is python and not ours to rewrite; spawning
//!      python3 is the sanctioned last resort).
//!
//! Why a separate process and not an in-core command: the conversion is a
//! byte-owning custodian responsibility, and (memory `serving-daemon-architecture`
//! / task #25) the substrate decomposes into composable processes. This is the
//! first member of the forge-custodian fleet the forge daemon will route over.
//!
//! ## Hardening (Contract C, Pass 3) — a resilient daemon, not a happy-path script
//! This is the seam a future grid router negotiates work across, so its advertised
//! capacity must be HONEST and its failure modes BOUNDED:
//!   - **R3 — bounds.** A finite pool of conversion slots ([`MAX_CONCURRENT`],
//!     env `FORGE_MAX_CONCURRENT`): a saturated custodian rejects fast and loud
//!     (`503`) so the router routes elsewhere, never queues unbounded. Each
//!     conversion runs under a wall-clock deadline ([`CONVERT_TIMEOUT_SECS`], env
//!     `FORGE_CONVERT_TIMEOUT_SECS`): a wedged python subprocess is KILLED, never
//!     holds a slot forever. (These two are deployment-tunable — conversion cost
//!     scales with model size × hardware — unlike substrate cognition thresholds,
//!     which must never be env-tuned.)
//!   - **R4 — honest `/health`.** Reports `ready` (its converter tooling resolves)
//!     and live `slots_total`/`slots_available` so a router scores it before
//!     dispatching. A live-but-not-ready custodian advertises `ready=false`.
//!   - **R5 — graceful shutdown.** SIGINT/SIGTERM stops accepting and lets axum
//!     drain in-flight conversions before exit (no orphaned half-written genes).
//!   - **R6 — content-addressed idempotency.** The output filename embeds a job id
//!     `= hash(weights ⊕ base ⊕ outtype)`; an identical re-POST (at-least-once grid
//!     delivery) returns the existing artifact instead of redoing GB of work, and
//!     a differing request can never silently overwrite another's gene.
//!
//! Config (via `config.env`, the single owner):
//!   FORGE_CUSTODIAN_ADDR        — bind addr (default `127.0.0.1:8899`)
//!   FORGE_MAX_CONCURRENT        — concurrent conversion slots (default 1)
//!   FORGE_CONVERT_TIMEOUT_SECS  — per-conversion wall-clock deadline (default 1800)
//!   FORGE_LLAMA_CPP_DIR         — dir holding `convert_lora_to_gguf.py`
//!                                 (default `~/.unsloth/llama.cpp`)
//!   FORGE_PYTHON                — python interpreter with the converter's deps
//!                                 (default `python3`)

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use continuum_core::config_env;
use continuum_core::forge::lora_convert::mlx_adapters_to_peft;
// The wire contract is single-sourced in `forge::protocol` so this server and
// the core-side client can never drift (see that module's docs). This binary is
// the SERVER half; it imports the SAME request/response/health types a client
// imports.
use continuum_core::forge::protocol::{
    ExportResult, GgufLoraRequest, HealthResponse, DEFAULT_CUSTODIAN_ADDR, ROUTE_GGUF_LORA,
    ROUTE_HEALTH,
};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

/// Default concurrent conversion slots. A conversion is a GPU/CPU transpose plus
/// a python subprocess; on a single box more than a couple in flight just thrash.
/// The router reads the free-slot count from `/health` and routes elsewhere when
/// we're saturated, so this is an HONEST capacity advertisement (R3), not a silent
/// drop. Deployment-tunable via `FORGE_MAX_CONCURRENT`.
const MAX_CONCURRENT: usize = 1;

/// Default per-conversion wall-clock deadline. The python converter is external
/// and can hang; a wedged subprocess must not hold a slot forever (R3). Tunable
/// via `FORGE_CONVERT_TIMEOUT_SECS` because conversion time scales with model
/// size × hardware.
const CONVERT_TIMEOUT_SECS: u64 = 1800;

/// Shared custodian state — the conversion-slot pool plus the resolved bounds.
/// Cloned into every handler via axum `State` (cheap: `Arc` + small `Copy` fields).
#[derive(Clone)]
struct CustodianState {
    /// The finite conversion-slot pool. `try_acquire` ⇒ reject-fast when saturated.
    slots: Arc<Semaphore>,
    /// Advertised total slots (the permit count at boot) — for honest `/health`.
    slots_total: usize,
    /// Per-conversion wall-clock deadline handed to the blocking converter.
    convert_timeout: Duration,
}

impl CustodianState {
    fn from_config() -> Self {
        let slots_total = config_env::read("FORGE_MAX_CONCURRENT")
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|&n| n > 0)
            .unwrap_or(MAX_CONCURRENT);
        let secs = config_env::read("FORGE_CONVERT_TIMEOUT_SECS")
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|&n| n > 0)
            .unwrap_or(CONVERT_TIMEOUT_SECS);
        Self {
            slots: Arc::new(Semaphore::new(slots_total)),
            slots_total,
            convert_timeout: Duration::from_secs(secs),
        }
    }
}

#[tokio::main]
async fn main() {
    // The default lives in the contract (DEFAULT_CUSTODIAN_ADDR) so the client
    // connects to exactly where this binary binds.
    let addr = config_env::read("FORGE_CUSTODIAN_ADDR")
        .unwrap_or_else(|| DEFAULT_CUSTODIAN_ADDR.to_string());

    let state = CustodianState::from_config();

    let app = Router::new()
        .route(ROUTE_HEALTH, get(health))
        .route(ROUTE_GGUF_LORA, post(gguf_lora_handler))
        .with_state(state.clone());

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[forge-custodian] FATAL: cannot bind {addr}: {e}");
            std::process::exit(1);
        }
    };
    eprintln!(
        "[forge-custodian] listening on http://{addr} (POST {ROUTE_GGUF_LORA}) \
         slots={} timeout={}s ready={}",
        state.slots_total,
        state.convert_timeout.as_secs(),
        converter_ready()
    );
    // R5: stop accepting + drain in-flight on SIGINT/SIGTERM.
    if let Err(e) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        eprintln!("[forge-custodian] FATAL: server error: {e}");
        std::process::exit(1);
    }
    eprintln!("[forge-custodian] shutdown complete");
}

/// R5: resolve when the process is asked to stop. axum then stops accepting new
/// connections and waits for in-flight handlers (a running conversion) to finish
/// before `serve` returns — no orphaned, half-written gene.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(e) => eprintln!("[forge-custodian] cannot install SIGTERM handler: {e}"),
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
    eprintln!("[forge-custodian] shutdown signal received — draining in-flight conversions");
}

/// R4: liveness + capability + contract version + the readiness/capacity detail a
/// router scores against. `ready` reflects whether the conversion tooling actually
/// resolves; the slot counts are live (free permits right now).
async fn health(State(state): State<CustodianState>) -> impl IntoResponse {
    Json(HealthResponse::gguf_lora(
        converter_ready(),
        state.slots_total as u32,
        state.slots.available_permits() as u32,
    ))
}

async fn gguf_lora_handler(
    State(state): State<CustodianState>,
    Json(req): Json<GgufLoraRequest>,
) -> impl IntoResponse {
    // R3: bound concurrency. `try_acquire` so a saturated custodian fails FAST and
    // LOUD (503) rather than queueing unbounded — the router then routes the
    // idempotent job elsewhere. The permit releases when `_permit` drops at the
    // end of the handler (after the blocking task joins).
    let _permit = match state.slots.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            let msg = format!(
                "custodian at capacity ({} slots in use) — retry or route to another node",
                state.slots_total
            );
            eprintln!("[forge-custodian] REJECT (busy): {msg}");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ExportResult {
                    success: false,
                    message: msg,
                    details: json!({ "busy": true, "slots_total": state.slots_total }),
                }),
            );
        }
    };

    eprintln!(
        "[forge-custodian] gguf-lora: checkpoint={} base={} outtype={} -> {}",
        req.checkpoint, req.base_model_id, req.outtype, req.save_directory
    );
    // Heavy work (safetensors transpose + python subprocess) off the async
    // reactor thread; the deadline bounds the subprocess (R3).
    let timeout = state.convert_timeout;
    let result = tokio::task::spawn_blocking(move || convert_gguf_lora(&req, timeout)).await;
    match result {
        Ok(Ok(envelope)) => (StatusCode::OK, Json(envelope)),
        Ok(Err(msg)) => {
            eprintln!("[forge-custodian] conversion FAILED: {msg}");
            (
                StatusCode::OK,
                Json(ExportResult {
                    success: false,
                    message: msg,
                    details: json!({}),
                }),
            )
        }
        Err(join_err) => {
            let msg = format!("conversion task panicked: {join_err}");
            eprintln!("[forge-custodian] {msg}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ExportResult {
                    success: false,
                    message: msg,
                    details: json!({}),
                }),
            )
        }
    }
}

/// Run the two-step conversion: MLX→PEFT (Rust) then PEFT→GGUF (python).
/// Fail-loud on every missing precondition — no silent skip, no partial output.
/// `timeout` bounds the python subprocess (R3); the output path is content-
/// addressed so an identical re-request short-circuits (R6).
fn convert_gguf_lora(req: &GgufLoraRequest, timeout: Duration) -> Result<ExportResult, String> {
    let checkpoint = Path::new(&req.checkpoint);
    let mlx_safetensors = resolve_mlx_adapter(checkpoint)?;
    let config_path = checkpoint.join("adapter_config.json");
    let config_bytes =
        std::fs::read(&config_path).map_err(|e| format!("read {}: {e}", config_path.display()))?;
    let mlx_config: Value = serde_json::from_slice(&config_bytes)
        .map_err(|e| format!("parse {}: {e}", config_path.display()))?;
    let (rank, alpha) = parse_mlx_lora_params(&mlx_config)?;

    // R6: content-addressed job id over (weights ⊕ base ⊕ outtype). The output
    // name embeds it so an identical re-POST resolves to the same path and a
    // differing request can never silently clobber another gene.
    let job = job_id(
        &mlx_safetensors,
        &config_bytes,
        &req.base_model_id,
        &req.outtype,
    )?;
    let save_dir = Path::new(&req.save_directory);
    let name = checkpoint
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "gene".to_string());
    let outfile = save_dir.join(format!("{name}-{job}.gguf"));

    // R6: idempotent short-circuit — identical inputs already produced this gene.
    if let Ok(meta) = std::fs::metadata(&outfile) {
        if meta.len() > 0 {
            eprintln!(
                "[forge-custodian] idempotent hit for job {job}: {}",
                outfile.display()
            );
            return Ok(ExportResult {
                success: true,
                message: format!("idempotent: reused existing GGUF LoRA (job {job})"),
                details: json!({
                    "output": outfile.to_string_lossy(),
                    "job_id": job,
                    "idempotent": true,
                }),
            });
        }
    }

    let peft_dir = save_dir.join(format!("peft-{job}"));
    let conv = mlx_adapters_to_peft(&mlx_safetensors, &peft_dir, &req.base_model_id, rank, alpha)?;

    let base_dir = resolve_hf_base_dir(&req.base_model_id)?;
    let converter = llama_cpp_converter()?;
    let python = config_env::read("FORGE_PYTHON").unwrap_or_else(|| "python3".to_string());

    let mut cmd = Command::new(&python);
    cmd.arg(&converter)
        .arg(&peft_dir)
        .arg("--base")
        .arg(&base_dir)
        .arg("--outtype")
        .arg(&req.outtype)
        .arg("--outfile")
        .arg(&outfile);

    let output = run_with_deadline(cmd, timeout).map_err(|e| {
        format!(
            "convert_lora_to_gguf.py ({python} {}): {e}",
            converter.display()
        )
    })?;

    if !output.status.success() {
        return Err(format!(
            "convert_lora_to_gguf.py failed (status {}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    if !outfile.exists() {
        return Err(format!(
            "converter reported success but {} does not exist",
            outfile.display()
        ));
    }

    Ok(ExportResult {
        success: true,
        message: format!(
            "wrote GGUF LoRA ({} tensors, r={})",
            conv.tensor_count, conv.rank
        ),
        details: json!({
            "output": outfile.to_string_lossy(),
            "peft_dir": peft_dir.to_string_lossy(),
            "tensor_count": conv.tensor_count,
            "rank": conv.rank,
            "lora_alpha": alpha,
            "target_modules": conv.target_modules,
            "base_model_dir": base_dir.to_string_lossy(),
            "job_id": job,
            "idempotent": false,
        }),
    })
}

/// Spawn `cmd` and wait up to `timeout`, draining stdout/stderr in helper threads
/// (so a full pipe can't deadlock the wait). On deadline the child is KILLED and a
/// loud error returned — a wedged converter never holds its slot forever (R3).
fn run_with_deadline(mut cmd: Command, timeout: Duration) -> Result<std::process::Output, String> {
    use std::io::Read;

    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn: {e}"))?;

    // Drain the pipes concurrently — a converter that writes more than a pipe
    // buffer would otherwise block forever on write while we wait on exit.
    let mut child_stdout = child.stdout.take().expect("stdout piped");
    let mut child_stderr = child.stderr.take().expect("stderr piped");
    let out_h = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = child_stdout.read_to_end(&mut b);
        b
    });
    let err_h = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = child_stderr.read_to_end(&mut b);
        b
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait().map_err(|e| format!("wait child: {e}"))? {
            Some(status) => break status,
            None => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("exceeded {}s deadline — killed", timeout.as_secs()));
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    };

    let stdout = out_h.join().unwrap_or_default();
    let stderr = err_h.join().unwrap_or_default();
    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

/// Content-addressed job id (R6): `sha256(weights-metadata ⊕ config ⊕ base ⊕
/// outtype)`, first 16 hex chars. Uses the adapter file's length + mtime rather
/// than its (possibly GB) bytes — a retrain rewrites the file, changing both — and
/// the full (tiny) `adapter_config.json` bytes, which carry rank/scale/targets.
fn job_id(
    safetensors: &Path,
    config_bytes: &[u8],
    base: &str,
    outtype: &str,
) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let meta = std::fs::metadata(safetensors)
        .map_err(|e| format!("stat {}: {e}", safetensors.display()))?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut h = Sha256::new();
    h.update(meta.len().to_le_bytes());
    h.update(mtime.to_le_bytes());
    h.update(config_bytes);
    h.update(b"\x00");
    h.update(base.as_bytes());
    h.update(b"\x00");
    h.update(outtype.as_bytes());
    let digest = h.finalize();

    let mut hex = String::with_capacity(16);
    for byte in digest.iter().take(8) {
        hex.push_str(&format!("{byte:02x}"));
    }
    Ok(hex)
}

/// R4 readiness: can the custodian actually do work right now? The custodian-
/// specific required tool is llama.cpp's `convert_lora_to_gguf.py`; if it doesn't
/// resolve, the custodian is alive but NOT ready and a router must not pick it.
fn converter_ready() -> bool {
    llama_cpp_converter().is_ok()
}

/// Find the MLX adapter weights in a checkpoint dir. Prefers the canonical
/// `adapters.safetensors`; fails loud if absent (no guessing among step files).
fn resolve_mlx_adapter(checkpoint: &Path) -> Result<PathBuf, String> {
    let canonical = checkpoint.join("adapters.safetensors");
    if canonical.exists() {
        return Ok(canonical);
    }
    Err(format!(
        "no adapters.safetensors in checkpoint {} — not an MLX LoRA checkpoint?",
        checkpoint.display()
    ))
}

/// Extract `(rank, lora_alpha)` from an `mlx_lm.lora` `adapter_config.json`.
/// MLX stores `lora_parameters.{rank, scale}`; PEFT's `lora_alpha == scale *
/// rank` (the proof: scale 20 × r 8 = alpha 160). Both are REQUIRED — a missing
/// field fails loud rather than defaulting (a wrong scale silently poisons the
/// page-in dial).
fn parse_mlx_lora_params(cfg: &Value) -> Result<(usize, u32), String> {
    let params = cfg
        .get("lora_parameters")
        .ok_or("MLX adapter_config.json missing `lora_parameters`")?;
    let rank = params
        .get("rank")
        .and_then(|v| v.as_u64())
        .ok_or("MLX adapter_config.json missing numeric `lora_parameters.rank`")?
        as usize;
    let scale = params
        .get("scale")
        .and_then(|v| v.as_f64())
        .ok_or("MLX adapter_config.json missing numeric `lora_parameters.scale`")?;
    let alpha = (scale * rank as f64).round() as u32;
    Ok((rank, alpha))
}

/// Resolve a HuggingFace model id to its local snapshot dir in the HF cache
/// (`~/.cache/huggingface/hub/models--<org>--<name>/snapshots/<hash>/`). The
/// converter needs the base's `config.json` for its architecture. Fail loud if
/// the base isn't cached locally (no network pull from a byte custodian).
fn resolve_hf_base_dir(model_id: &str) -> Result<PathBuf, String> {
    let home = std::env::var("HOME").map_err(|_| "HOME not set".to_string())?;
    let repo = format!("models--{}", model_id.replace('/', "--"));
    let snapshots = Path::new(&home)
        .join(".cache/huggingface/hub")
        .join(&repo)
        .join("snapshots");
    let entries = std::fs::read_dir(&snapshots).map_err(|e| {
        format!(
            "base model {model_id} not in HF cache ({}): {e}",
            snapshots.display()
        )
    })?;
    for entry in entries.flatten() {
        let dir = entry.path();
        if dir.join("config.json").exists() {
            return Ok(dir);
        }
    }
    Err(format!(
        "no snapshot with config.json for base {model_id} under {}",
        snapshots.display()
    ))
}

/// Locate llama.cpp's `convert_lora_to_gguf.py`. Fail loud if absent — this is
/// the custodian's required tool, not something to silently skip.
///
/// Default is the llama.cpp WE vendor (`core/vendor/llama.cpp` — our fork),
/// resolved from the crate dir; the custodian is a training-time tool that runs
/// where the source tree exists. We do NOT borrow `~/.unsloth` anymore (unsloth
/// is excised; the convert script is ours, in the submodule). `FORGE_LLAMA_CPP_DIR`
/// overrides for deployments that stage the script elsewhere.
fn llama_cpp_converter() -> Result<PathBuf, String> {
    let dir = config_env::read("FORGE_LLAMA_CPP_DIR").unwrap_or_else(|| {
        // CARGO_MANIFEST_DIR = <repo>/core/continuum-core → ../vendor/llama.cpp.
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../vendor/llama.cpp")
            .to_string_lossy()
            .into_owned()
    });
    let path = Path::new(&dir).join("convert_lora_to_gguf.py");
    if path.exists() {
        Ok(path)
    } else {
        Err(format!(
            "convert_lora_to_gguf.py not found at {} (set FORGE_LLAMA_CPP_DIR)",
            path.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the MLX→PEFT alpha derivation (alpha = scale * rank)
    // and rank extraction from the real mlx_lm.lora adapter_config.json schema.
    // The proof artifact had rank=8, scale=20 → alpha=160; if this regresses the
    // forged gene gets the wrong page-in scale and every LIFT is silently off.
    #[test]
    fn parses_mlx_lora_params_from_real_schema() {
        let cfg = json!({
            "fine_tune_type": "lora",
            "lora_parameters": { "rank": 8, "dropout": 0.0, "scale": 20.0 },
            "model": "unsloth/Qwen2.5-0.5B-Instruct"
        });
        let (rank, alpha) = parse_mlx_lora_params(&cfg).unwrap();
        assert_eq!(rank, 8);
        assert_eq!(alpha, 160, "alpha = scale * rank = 20 * 8");
    }

    // what this catches: a missing scale fails loud (never defaults) — a wrong
    // page-in scale silently corrupts the gene.
    #[test]
    fn missing_scale_fails_loud() {
        let cfg = json!({ "lora_parameters": { "rank": 8 } });
        let err = parse_mlx_lora_params(&cfg).expect_err("missing scale must error");
        assert!(err.contains("scale"), "got: {err}");
    }

    // what this catches: the request default for outtype is f16 (quantizing a
    // small LoRA buys little; f16 preserves the trained signal).
    #[test]
    fn outtype_defaults_to_f16() {
        let req: GgufLoraRequest = serde_json::from_value(json!({
            "checkpoint": "/c", "save_directory": "/s", "base_model_id": "b"
        }))
        .unwrap();
        assert_eq!(req.outtype, "f16");
    }

    // what this catches (R6): the job id is content-addressed — stable for
    // identical inputs (so a re-POST short-circuits) and DIFFERENT when the base
    // or outtype changes (so a differing request can't clobber another's gene).
    #[test]
    fn job_id_is_content_addressed_and_stable() {
        let dir = std::env::temp_dir().join(format!("fc-jobid-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let weights = dir.join("adapters.safetensors");
        std::fs::write(&weights, b"trained-weights").unwrap();
        let cfg = br#"{"lora_parameters":{"rank":8,"scale":20.0}}"#;

        let a = job_id(&weights, cfg, "base-x", "f16").unwrap();
        let b = job_id(&weights, cfg, "base-x", "f16").unwrap();
        assert_eq!(a, b, "identical inputs must yield the same job id");
        assert_eq!(a.len(), 16, "job id is 16 hex chars (8 bytes)");

        let diff_base = job_id(&weights, cfg, "base-y", "f16").unwrap();
        assert_ne!(a, diff_base, "a different base must change the job id");
        let diff_outtype = job_id(&weights, cfg, "base-x", "q8_0").unwrap();
        assert_ne!(
            a, diff_outtype,
            "a different outtype must change the job id"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches (R3): the deadline KILLS a subprocess that outlives it and
    // returns a loud error — a wedged converter never holds its slot forever.
    #[test]
    fn run_with_deadline_kills_overrunning_child() {
        let mut cmd = Command::new("sleep");
        cmd.arg("30");
        let started = Instant::now();
        let err = run_with_deadline(cmd, Duration::from_millis(300))
            .expect_err("a 30s sleep must trip a 300ms deadline");
        assert!(err.contains("deadline"), "got: {err}");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "must return promptly after the deadline, not wait out the sleep"
        );
    }

    // what this catches (R3): a fast child completes normally through the deadline
    // path — the bound doesn't break the happy path, and stdout is captured.
    #[test]
    fn run_with_deadline_captures_fast_child() {
        let mut cmd = Command::new("echo");
        cmd.arg("hello-custodian");
        let out = run_with_deadline(cmd, Duration::from_secs(5)).expect("echo is fast");
        assert!(out.status.success());
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            "hello-custodian"
        );
    }
}
