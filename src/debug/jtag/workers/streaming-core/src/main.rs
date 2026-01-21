//! Streaming Core gRPC Service
//!
//! Exposes pipeline management and voice services via gRPC for TypeScript clients.
//! Handle-based: start returns handle, events flow on separate channel.

use std::collections::HashMap;
use std::sync::Arc;
use streaming_core::{
    call_server, EventBus, Handle, Pipeline, PipelineBuilder, PipelineState, StreamEvent,
};
use tokio::sync::RwLock;
use tonic::{transport::Server, Status};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

// Voice service (gRPC) - import from library
#[cfg(feature = "grpc")]
use streaming_core::voice_service::VoiceServiceImpl;

/// Get gRPC port from environment or default
fn get_grpc_port() -> u16 {
    std::env::var("STREAMING_CORE_GRPC_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(50052)
}

/// Get WebSocket call server port from environment or default
fn get_call_server_port() -> u16 {
    std::env::var("STREAMING_CORE_WS_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(50053)
}

/// Active pipelines managed by the service
#[allow(dead_code)]
struct PipelineManager {
    pipelines: RwLock<HashMap<String, Arc<RwLock<Pipeline>>>>,
    event_bus: Arc<EventBus>,
}

#[allow(dead_code)]
impl PipelineManager {
    fn new() -> Self {
        Self {
            pipelines: RwLock::new(HashMap::new()),
            event_bus: Arc::new(EventBus::new(4096)),
        }
    }

    async fn create_voice_chat(&self) -> Handle {
        let pipeline = PipelineBuilder::new(self.event_bus.clone()).voice_chat();
        let handle = pipeline.handle();

        self.pipelines
            .write()
            .await
            .insert(handle.to_string(), Arc::new(RwLock::new(pipeline)));

        handle
    }

    async fn create_ivr(&self, stream_sid: String) -> Handle {
        let pipeline = PipelineBuilder::new(self.event_bus.clone()).ivr(stream_sid);
        let handle = pipeline.handle();

        self.pipelines
            .write()
            .await
            .insert(handle.to_string(), Arc::new(RwLock::new(pipeline)));

        handle
    }

    async fn create_image_gen(&self) -> Handle {
        let pipeline = PipelineBuilder::new(self.event_bus.clone()).image_gen();
        let handle = pipeline.handle();

        self.pipelines
            .write()
            .await
            .insert(handle.to_string(), Arc::new(RwLock::new(pipeline)));

        handle
    }

    async fn start(&self, handle_str: &str) -> Result<(), Status> {
        let pipelines = self.pipelines.read().await;
        let pipeline = pipelines
            .get(handle_str)
            .ok_or_else(|| Status::not_found("Pipeline not found"))?
            .clone();

        drop(pipelines); // Release read lock before starting

        let mut pipeline = pipeline.write().await;
        pipeline
            .start()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(())
    }

    async fn cancel(&self, handle_str: &str) -> Result<(), Status> {
        let pipelines = self.pipelines.read().await;
        let pipeline = pipelines
            .get(handle_str)
            .ok_or_else(|| Status::not_found("Pipeline not found"))?
            .clone();

        drop(pipelines);

        let mut pipeline = pipeline.write().await;
        pipeline
            .cancel()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(())
    }

    #[allow(dead_code)]
    async fn get_state(&self, handle_str: &str) -> Result<PipelineState, Status> {
        let pipelines = self.pipelines.read().await;
        let pipeline = pipelines
            .get(handle_str)
            .ok_or_else(|| Status::not_found("Pipeline not found"))?
            .clone();

        drop(pipelines); // Release read lock

        let state = pipeline.read().await.state();
        Ok(state)
    }

    #[allow(dead_code)]
    fn subscribe_events(&self, handle: Handle) -> tokio::sync::broadcast::Receiver<StreamEvent> {
        self.event_bus.subscribe_handle(handle)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("Starting streaming-core service");

    let _manager = Arc::new(PipelineManager::new());

    // Start WebSocket call server for live audio
    let call_port = get_call_server_port();
    let call_addr = format!("127.0.0.1:{}", call_port);
    info!("Call WebSocket server starting on ws://{}", call_addr);
    let call_server_handle = tokio::spawn(async move {
        if let Err(e) = call_server::start_call_server(&call_addr).await {
            tracing::error!("Call server error: {}", e);
        }
    });

    // Start gRPC server with voice service
    #[cfg(feature = "grpc")]
    {
        let grpc_port = get_grpc_port();
        let addr = format!("127.0.0.1:{}", grpc_port).parse()?;
        info!("Voice gRPC service listening on {}", addr);

        let voice_service = VoiceServiceImpl::new();

        // Run gRPC server and call server concurrently
        tokio::select! {
            result = Server::builder()
                .add_service(voice_service.into_server())
                .serve(addr) => {
                if let Err(e) = result {
                    tracing::error!("gRPC server error: {}", e);
                }
            }
            _ = call_server_handle => {
                tracing::warn!("Call server stopped");
            }
        }
    }

    // Fallback if gRPC not enabled
    #[cfg(not(feature = "grpc"))]
    {
        info!("gRPC feature not enabled, running WebSocket call server only");
        let _ = call_server_handle.await;
    }

    Ok(())
}

// Simple test to verify the architecture works
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pipeline_manager() {
        let manager = PipelineManager::new();

        // Create a pipeline
        let handle = manager.create_voice_chat().await;
        assert!(!handle.to_string().is_empty());

        // Check state
        let state = manager.get_state(&handle.to_string()).await.unwrap();
        assert_eq!(state, PipelineState::Idle);
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let manager = PipelineManager::new();
        let handle = manager.create_voice_chat().await;

        // Subscribe to events
        let _receiver = manager.subscribe_events(handle);

        // Events would flow when pipeline runs
    }
}
