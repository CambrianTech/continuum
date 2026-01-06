/**
 * Inference gRPC Server with Candle LLM Backend
 *
 * Real model inference with Llama 3.2 3B for quality matching Ollama.
 * Loads model on startup, generates text via gRPC streaming.
 */

use tonic::transport::Server;
use log::info;

mod lora;
mod model;
mod grpc;

pub mod inference {
    tonic::include_proto!("inference");
}

use inference::inference_server::InferenceServer;
use grpc::InferenceService;
use model::load_default_model;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    let addr = "127.0.0.1:50051".parse()?;

    let model_id = std::env::var("INFERENCE_MODEL_ID")
        .unwrap_or_else(|_| "unsloth/Llama-3.2-3B-Instruct".to_string());

    info!("===========================================");
    info!("  Inference gRPC Server (Candle + Llama)");
    info!("  Model: {}", model_id);
    info!("  Listening on: {}", addr);
    info!("===========================================");

    let model_state = match load_default_model() {
        Ok(state) => {
            info!("✅ Model ready for inference");
            Some(state)
        }
        Err(e) => {
            info!("❌ Failed to load model: {}", e);
            info!("   Server will start but return errors");
            None
        }
    };

    let service = InferenceService::new(model_state);

    Server::builder()
        .add_service(InferenceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
