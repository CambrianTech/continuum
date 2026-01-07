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

pub mod inference {
    tonic::include_proto!("inference");
}

use grpc::InferenceService;
use inference::inference_server::InferenceServer;
use model::load_default_model;

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

    // Load model based on mode
    // Default: quantized (fast startup, auto-switches to BF16 when LoRA is needed)
    let (model_state, quantized_state) = match mode {
        InferenceMode::Auto | InferenceMode::Quantized => {
            // Quantized: fast startup (~2s), low memory (~2GB)
            // Auto-switches to BF16 when LoRA adapters are requested
            info!("📦 Loading quantized model (Q4_K_M)...");
            match quantized_model::load_default_quantized() {
                Ok(state) => {
                    info!("✅ Quantized ready (2s load, auto-switches to BF16 for LoRA)");
                    (None, Some(state))
                }
                Err(e) => {
                    info!("⚠️  Quantized unavailable: {e}");
                    info!("🔄 Falling back to BF16...");
                    match load_default_model() {
                        Ok(state) => {
                            info!("✅ BF16 model ready (~14s load)");
                            (Some(state), None)
                        }
                        Err(e2) => {
                            info!("❌ Both modes failed: {e2}");
                            (None, None)
                        }
                    }
                }
            }
        }
        InferenceMode::BF16 => {
            info!("📦 Loading BF16 model (forced)...");
            match load_default_model() {
                Ok(state) => {
                    info!("✅ BF16 model ready");
                    (Some(state), None)
                }
                Err(e) => {
                    info!("❌ Failed to load BF16: {e}");
                    (None, None)
                }
            }
        }
    };

    let service = InferenceService::new_with_quantized(model_state, quantized_state);

    Server::builder()
        .add_service(InferenceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
