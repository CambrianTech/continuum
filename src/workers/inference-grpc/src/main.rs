use log::info;
/**
 * Inference gRPC Server with Candle LLM Backend
 *
 * Supports both full-precision (BF16) and quantized (GGUF) models.
 * Configuration via ~/.continuum/config.env:
 *   INFERENCE_MODE=auto|quantized|bf16  (default: auto)
 */
use std::fs;
use std::path::PathBuf;
use tonic::transport::Server;

mod adapter_registry;
mod grpc;
mod lora;
mod model;
mod priority_queue;
mod quantized_model;
mod worker_pool;

pub mod inference {
    tonic::include_proto!("inference");
}

use grpc::InferenceService;
use inference::inference_server::InferenceServer;
use model::load_default_model;
use worker_pool::WorkerPool;

/// Resolve the inference worker-pool size.
///
/// Source of truth, in order:
///
/// 1. **`INFERENCE_WORKERS` environment variable** — the channel a
///    supervising continuum-core sets at process spawn based on its
///    PressureBroker lease. When set, that value is the policy and
///    inference-grpc uses it verbatim. No floor, no ceiling — supervisor
///    knows the live hardware + memory pressure better than this binary
///    does. Invalid integer in the env var is a configuration bug:
///    return Err with the bad value named (no silent default).
///
/// 2. **No env var set** — Continuum-core wasn't the spawner (direct
///    `cargo run`, integration test, docker exec). Fall back to the
///    physical CPU count from `num_cpus`. CPU count is hardware-derived,
///    not hardcoded; one worker per physical core is the most
///    conservative "make use of the box" default. Caller sees a single
///    info log line announcing the fallback.
///
/// What this fn DOES NOT do anymore (deletion targets from CBAR-PIECE-8
/// + vhsm-d1f4 audit pass 1):
///
/// - **No more `~/.continuum/config.env` parsing.** Static-config-file
///   reads violate the dynamic / broker-owned-concurrency rule. If a
///   user wants to override, they pass `INFERENCE_WORKERS` as an env
///   var on the process line; no file-on-disk side channel.
/// - **No more `clamp(1, 4)` / `clamp(1, 8)` ceilings.** Hardcoded
///   ceilings prevent the supervisor from sizing the pool for a
///   Blackwell with 128GB RAM (capped at 4 workers, same as a 16GB
///   MacBook Air). Removed entirely — supervisor sets the ceiling, this
///   binary doesn't.
/// - **No more `2GB-per-worker` magic constant.** Per-worker footprint
///   depends on the model + quantization + context window; a fixed
///   number is wrong for every model that isn't a 7B Q4_K_M. Calculation
///   was wrong; deleted.
/// - **No more `Default: 2 workers` fallback** — silent default was
///   the exact "guess and silently degrade" anti-pattern vhsm-d1f4
///   called out. Fallback is now `num_cpus::get_physical()` (hardware-
///   probed, never zero) with an info log so the operator can see what
///   was picked.
///
/// Returns `Result` so the supervisor can see the typed reason when
/// INFERENCE_WORKERS is invalid; `main` propagates the error to abort
/// startup instead of silently launching with a wrong pool size.
fn resolve_num_workers() -> Result<usize, String> {
    match std::env::var("INFERENCE_WORKERS") {
        Ok(value) => {
            let n: usize = value.parse().map_err(|e| {
                format!(
                    "INFERENCE_WORKERS={value:?} is not a valid usize: {e}. \
                     The supervising continuum-core (or whoever set this) sent a bad value. \
                     Fix the source or unset to fall back to physical CPU count."
                )
            })?;
            if n == 0 {
                return Err(
                    "INFERENCE_WORKERS=0 — zero workers means zero concurrent inference. \
                     Pool size must be >= 1."
                        .into(),
                );
            }
            info!("  Workers: {n} (from INFERENCE_WORKERS env, supervisor-set)");
            Ok(n)
        }
        Err(_) => {
            let n = num_cpus::get_physical().max(1);
            info!(
                "  Workers: {n} (INFERENCE_WORKERS not set; fell back to \
                 num_cpus::get_physical(). Continuum-core supervisor should set \
                 INFERENCE_WORKERS based on its PressureBroker lease — see CBAR-PIECE-8)"
            );
            Ok(n)
        }
    }
}

#[cfg(test)]
mod resolve_num_workers_tests {
    use super::resolve_num_workers;

    /// Save+restore env around a test so concurrent runs don't poison
    /// each other. INFERENCE_WORKERS is process-global so tests cannot
    /// run in parallel against it — `cargo test --test-threads=1` is
    /// the contract. (Documented per CLAUDE.md FEEDBACK rule on
    /// env-mutating tests.)
    fn with_env<F: FnOnce()>(key: &str, value: Option<&str>, f: F) {
        let prev = std::env::var(key).ok();
        // SAFETY: tests run serial via --test-threads=1 for env mutations.
        unsafe {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
        f();
        unsafe {
            match prev {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }

    /// What this catches: INFERENCE_WORKERS=8 returns 8 (no clamp, no
    /// default). Replaces the prior clamp(1,8) ceiling — supervisor's
    /// value must pass through verbatim.
    #[test]
    fn env_var_passes_through_verbatim() {
        with_env("INFERENCE_WORKERS", Some("8"), || {
            assert_eq!(resolve_num_workers().unwrap(), 8);
        });
    }

    /// What this catches: INFERENCE_WORKERS=64 returns 64. The prior
    /// hardcoded clamp(1, 8) would have capped this at 8 on a Blackwell
    /// rig with the headroom to actually run 64 concurrent workers.
    /// Pins the no-ceiling guarantee explicitly.
    #[test]
    fn large_env_value_not_capped() {
        with_env("INFERENCE_WORKERS", Some("64"), || {
            assert_eq!(resolve_num_workers().unwrap(), 64);
        });
    }

    /// What this catches: INFERENCE_WORKERS=0 returns Err — zero
    /// workers means zero concurrent inference, which is a config bug
    /// the caller surely didn't mean. Refuse rather than launch with a
    /// dead pool.
    #[test]
    fn env_var_zero_returns_err() {
        with_env("INFERENCE_WORKERS", Some("0"), || {
            let result = resolve_num_workers();
            assert!(result.is_err());
            assert!(result.unwrap_err().contains("0"));
        });
    }

    /// What this catches: INFERENCE_WORKERS=not-a-number returns Err
    /// with the bad value named. Operator sees what was set so they can
    /// fix the source. Silent fallback to 2 (the old behavior) would
    /// hide the bad config.
    #[test]
    fn env_var_invalid_returns_err_with_value_named() {
        with_env("INFERENCE_WORKERS", Some("not-a-number"), || {
            let result = resolve_num_workers();
            assert!(result.is_err());
            let msg = result.unwrap_err();
            assert!(msg.contains("not-a-number"), "value name missing: {msg}");
        });
    }

    /// What this catches: INFERENCE_WORKERS unset → fallback to
    /// num_cpus::get_physical(), clamped >=1. No silent default-2;
    /// hardware-derived. Confirms the fallback never returns 0.
    #[test]
    fn unset_env_falls_back_to_physical_cpus() {
        with_env("INFERENCE_WORKERS", None, || {
            let result = resolve_num_workers();
            assert!(result.is_ok());
            let n = result.unwrap();
            assert!(n >= 1, "fallback must be >=1, got {n}");
            // Should match num_cpus on this test host
            assert_eq!(n, num_cpus::get_physical().max(1));
        });
    }

    /// What this catches: empty env var (`INFERENCE_WORKERS=`) returns
    /// Err with the empty value named. Empty != unset — empty is a
    /// shell-script bug where someone wrote `INFERENCE_WORKERS=` with
    /// nothing after. Refuse rather than silently fallback (the user
    /// MEANT to set something).
    #[test]
    fn empty_env_var_returns_err() {
        with_env("INFERENCE_WORKERS", Some(""), || {
            let result = resolve_num_workers();
            assert!(result.is_err());
        });
    }

    /// What this catches: INFERENCE_WORKERS=1 (the minimum valid)
    /// passes through. Edge case at the lower boundary.
    #[test]
    fn env_var_one_passes() {
        with_env("INFERENCE_WORKERS", Some("1"), || {
            assert_eq!(resolve_num_workers().unwrap(), 1);
        });
    }

    /// What this catches: negative env value returns Err (parse fails
    /// for usize). Defensive — shell scripts that compute the value
    /// could underflow to a negative number; this catches.
    #[test]
    fn negative_env_value_returns_err() {
        with_env("INFERENCE_WORKERS", Some("-1"), || {
            let result = resolve_num_workers();
            assert!(result.is_err());
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum InferenceMode {
    Auto,      // BF16 first (full LoRA), fallback to quantized
    Quantized, // Force quantized (fast startup, no LoRA)
    BF16,      // Force BF16 (full LoRA support)
}

impl InferenceMode {
    fn from_config() -> Self {
        // Load from ~/.continuum/config.env
        let config_path = dirs::home_dir()
            .map(|h| h.join(".continuum/config.env"))
            .unwrap_or_else(|| PathBuf::from(".continuum/config.env"));

        if let Ok(content) = fs::read_to_string(&config_path) {
            for line in content.lines() {
                let line = line.trim();
                if line.starts_with("INFERENCE_MODE=") {
                    let value = line.strip_prefix("INFERENCE_MODE=").unwrap_or("auto");
                    return match value.to_lowercase().as_str() {
                        "quantized" | "gguf" | "q4" => InferenceMode::Quantized,
                        "bf16" | "full" | "fp16" => InferenceMode::BF16,
                        _ => InferenceMode::Auto,
                    };
                }
            }
        }

        // Default to Auto (BF16 for full LoRA support)
        InferenceMode::Auto
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    let addr = "127.0.0.1:50051".parse()?;
    let mode = InferenceMode::from_config();

    info!("===========================================");
    info!("  Inference gRPC Server (Candle + Llama)");
    info!("  Mode: {mode:?}");
    info!("  Listening on: {addr}");
    info!("===========================================");

    // Determine number of workers for concurrent inference. Source: env
    // var INFERENCE_WORKERS (supervisor-set) or num_cpus fallback. See
    // resolve_num_workers' docstring for the deletion-of-hardcoded-ceilings
    // rationale. Hard-fails on invalid env value instead of silent default.
    let num_workers = resolve_num_workers()?;

    // Load model based on mode
    // Default: worker pool with quantized models for concurrent inference
    let service = match mode {
        InferenceMode::Auto | InferenceMode::Quantized => {
            // Try to create worker pool for concurrent quantized inference
            info!("🏭 Creating worker pool with {num_workers} quantized models...");

            match WorkerPool::new(num_workers).await {
                Ok(pool) => {
                    info!("✅ Worker pool ready ({num_workers} concurrent inference slots)");
                    InferenceService::new_with_pool(pool)
                }
                Err(e) => {
                    info!("⚠️ Worker pool failed: {e}");
                    info!("🔄 Falling back to single quantized instance...");

                    // Fallback to single instance
                    match quantized_model::load_default_quantized() {
                        Ok(state) => {
                            info!("✅ Single quantized instance ready");
                            InferenceService::new_with_quantized(None, Some(state))
                        }
                        Err(e2) => {
                            info!("⚠️ Quantized unavailable: {e2}");
                            info!("🔄 Falling back to BF16...");
                            match load_default_model() {
                                Ok(state) => {
                                    info!("✅ BF16 model ready");
                                    InferenceService::new_with_quantized(Some(state), None)
                                }
                                Err(e3) => {
                                    info!("❌ All modes failed: {e3}");
                                    InferenceService::new_with_quantized(None, None)
                                }
                            }
                        }
                    }
                }
            }
        }
        InferenceMode::BF16 => {
            // BF16 mode for LoRA support - single instance
            info!("📦 Loading BF16 model (forced, LoRA support)...");
            match load_default_model() {
                Ok(state) => {
                    info!("✅ BF16 model ready");
                    InferenceService::new_with_quantized(Some(state), None)
                }
                Err(e) => {
                    info!("❌ Failed to load BF16: {e}");
                    InferenceService::new_with_quantized(None, None)
                }
            }
        }
    };

    Server::builder()
        .add_service(InferenceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
