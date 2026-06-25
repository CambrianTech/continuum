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
//! Config (via `config.env`, the single owner):
//!   FORGE_CUSTODIAN_ADDR  — bind addr (default `127.0.0.1:8899`)
//!   FORGE_LLAMA_CPP_DIR   — dir holding `convert_lora_to_gguf.py`
//!                           (default `~/.unsloth/llama.cpp`)
//!   FORGE_PYTHON          — python interpreter with the converter's deps
//!                           (default `python3`)

use axum::{
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
use std::process::Command;

#[tokio::main]
async fn main() {
    // The default lives in the contract (DEFAULT_CUSTODIAN_ADDR) so the client
    // connects to exactly where this binary binds.
    let addr = config_env::read("FORGE_CUSTODIAN_ADDR")
        .unwrap_or_else(|| DEFAULT_CUSTODIAN_ADDR.to_string());

    let app = Router::new()
        .route(ROUTE_HEALTH, get(health))
        .route(ROUTE_GGUF_LORA, post(gguf_lora_handler));

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[forge-custodian] FATAL: cannot bind {addr}: {e}");
            std::process::exit(1);
        }
    };
    eprintln!("[forge-custodian] listening on http://{addr} (POST /api/export/export/gguf-lora)");
    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("[forge-custodian] FATAL: server error: {e}");
        std::process::exit(1);
    }
}

async fn health() -> impl IntoResponse {
    // The shared contract type — carries `contract_version` so a (grid) client
    // can refuse a custodian it can't speak to before POSTing a body it can't parse.
    Json(HealthResponse::ok_gguf_lora())
}

async fn gguf_lora_handler(Json(req): Json<GgufLoraRequest>) -> impl IntoResponse {
    eprintln!(
        "[forge-custodian] gguf-lora: checkpoint={} base={} outtype={} -> {}",
        req.checkpoint, req.base_model_id, req.outtype, req.save_directory
    );
    // Heavy work (safetensors transpose + python subprocess) off the async
    // reactor thread.
    let result = tokio::task::spawn_blocking(move || convert_gguf_lora(&req)).await;
    match result {
        Ok(Ok(envelope)) => Json(envelope),
        Ok(Err(msg)) => {
            eprintln!("[forge-custodian] conversion FAILED: {msg}");
            Json(ExportResult {
                success: false,
                message: msg,
                details: json!({}),
            })
        }
        Err(join_err) => {
            let msg = format!("conversion task panicked: {join_err}");
            eprintln!("[forge-custodian] {msg}");
            Json(ExportResult {
                success: false,
                message: msg,
                details: json!({}),
            })
        }
    }
}

/// Run the two-step conversion: MLX→PEFT (Rust) then PEFT→GGUF (python).
/// Fail-loud on every missing precondition — no silent skip, no partial output.
fn convert_gguf_lora(req: &GgufLoraRequest) -> Result<ExportResult, String> {
    let checkpoint = Path::new(&req.checkpoint);
    let mlx_safetensors = resolve_mlx_adapter(checkpoint)?;
    let mlx_config: Value = read_json(&checkpoint.join("adapter_config.json"))?;
    let (rank, alpha) = parse_mlx_lora_params(&mlx_config)?;

    let save_dir = Path::new(&req.save_directory);
    let peft_dir = save_dir.join("peft");
    let conv = mlx_adapters_to_peft(&mlx_safetensors, &peft_dir, &req.base_model_id, rank, alpha)?;

    let base_dir = resolve_hf_base_dir(&req.base_model_id)?;
    let converter = llama_cpp_converter()?;
    let python = config_env::read("FORGE_PYTHON").unwrap_or_else(|| "python3".to_string());

    let name = checkpoint
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "gene".to_string());
    let outfile = save_dir.join(format!("{name}.gguf"));

    let output = Command::new(&python)
        .arg(&converter)
        .arg(&peft_dir)
        .arg("--base")
        .arg(&base_dir)
        .arg("--outtype")
        .arg(&req.outtype)
        .arg("--outfile")
        .arg(&outfile)
        .output()
        .map_err(|e| format!("spawn {python} {}: {e}", converter.display()))?;

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
        }),
    })
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
fn llama_cpp_converter() -> Result<PathBuf, String> {
    let dir = config_env::read("FORGE_LLAMA_CPP_DIR").unwrap_or_else(|| {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{home}/.unsloth/llama.cpp")
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

fn read_json(path: &Path) -> Result<Value, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("parse {}: {e}", path.display()))
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
}
