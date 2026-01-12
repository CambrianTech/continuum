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
mod quantized_model;
mod worker_pool;

pub mod inference {
    tonic::include_proto!("inference");
}

use grpc::InferenceService;
use inference::inference_server::InferenceServer;
use model::load_default_model;
use worker_pool::WorkerPool;

/// Get number of inference workers from config or auto-detect
fn get_num_workers() -> usize {
    // Load from ~/.continuum/config.env
    let config_path = dirs::home_dir()
        .map(|h| h.join(".continuum/config.env"))
        .unwrap_or_else(|| PathBuf::from(".continuum/config.env"));

    if let Ok(content) = fs::read_to_string(&config_path) {
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("INFERENCE_WORKERS=") {
                if let Some(value) = line.strip_prefix("INFERENCE_WORKERS=") {
                    if let Ok(n) = value.parse::<usize>() {
                        return n.clamp(1, 8); // Clamp to 1-8
                    }
                }
            }
        }
    }

    // Auto-detect: use available memory / 2GB per worker, max 4
    // Each quantized model uses ~2GB
    let sys_info = sys_info::mem_info();
    if let Ok(mem) = sys_info {
        let total_gb = mem.total as f64 / (1024.0 * 1024.0);
        let workers = ((total_gb - 4.0) / 2.0).floor() as usize; // Reserve 4GB for system
        return workers.clamp(1, 4); // 1-4 workers
    }

    // Default: 2 workers
    2
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

    // Determine number of workers for concurrent inference
    let num_workers = get_num_workers();
    info!("  Workers: {num_workers} (INFERENCE_WORKERS env or auto-detected)");

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
