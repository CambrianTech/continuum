//! gRPC Service Module
//!
//! Modular implementation of the Inference gRPC service.
//!
//! Structure:
//! - service.rs  - InferenceService struct and constructors
//! - generate.rs - Text generation handler
//! - model.rs    - Model management handlers
//! - adapter.rs  - LoRA adapter handlers
//! - genome.rs   - Multi-adapter stacking handler
//! - status.rs   - Health and status handlers

mod adapter;
mod generate;
mod genome;
mod model;
pub mod service;
mod status;

use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use crate::inference::inference_server::Inference;
use crate::inference::{
    ApplyGenomeRequest, ApplyGenomeResponse, DownloadAdapterRequest, DownloadAdapterResponse,
    GenerateRequest, GenerateResponse, ListAdaptersRequest, ListAdaptersResponse,
    ListModelsRequest, ListModelsResponse, LoadAdapterRequest, LoadAdapterResponse,
    LoadModelRequest, LoadModelResponse, PingRequest, PingResponse, StatusRequest, StatusResponse,
    UnloadAdapterRequest, UnloadAdapterResponse, UnloadModelRequest, UnloadModelResponse,
};

pub use service::InferenceService;

#[tonic::async_trait]
impl Inference for InferenceService {
    // ========================================================================
    // Health
    // ========================================================================

    async fn ping(&self, request: Request<PingRequest>) -> Result<Response<PingResponse>, Status> {
        status::handle_ping(request, &self.state).await
    }

    // ========================================================================
    // Generation
    // ========================================================================

    type GenerateStream = ReceiverStream<Result<GenerateResponse, Status>>;

    async fn generate(
        &self,
        request: Request<GenerateRequest>,
    ) -> Result<Response<Self::GenerateStream>, Status> {
        let has_adapters = !self.adapters.read().await.is_empty();
        generate::handle_generate(
            request,
            &self.worker_pool,
            &self.state,
            &self.quantized_state,
            &self.stats,
            has_adapters,
        )
        .await
    }

    // ========================================================================
    // Model Management
    // ========================================================================

    async fn load_model(
        &self,
        request: Request<LoadModelRequest>,
    ) -> Result<Response<LoadModelResponse>, Status> {
        model::handle_load_model(request, &self.state).await
    }

    async fn unload_model(
        &self,
        request: Request<UnloadModelRequest>,
    ) -> Result<Response<UnloadModelResponse>, Status> {
        model::handle_unload_model(request, &self.state).await
    }

    async fn list_models(
        &self,
        request: Request<ListModelsRequest>,
    ) -> Result<Response<ListModelsResponse>, Status> {
        model::handle_list_models(request, &self.state).await
    }

    // ========================================================================
    // LoRA Adapter Management
    // ========================================================================

    async fn load_adapter(
        &self,
        request: Request<LoadAdapterRequest>,
    ) -> Result<Response<LoadAdapterResponse>, Status> {
        adapter::handle_load_adapter(request, self).await
    }

    async fn unload_adapter(
        &self,
        request: Request<UnloadAdapterRequest>,
    ) -> Result<Response<UnloadAdapterResponse>, Status> {
        adapter::handle_unload_adapter(request, &self.adapters).await
    }

    async fn list_adapters(
        &self,
        request: Request<ListAdaptersRequest>,
    ) -> Result<Response<ListAdaptersResponse>, Status> {
        adapter::handle_list_adapters(request, &self.adapters).await
    }

    async fn download_adapter(
        &self,
        request: Request<DownloadAdapterRequest>,
    ) -> Result<Response<DownloadAdapterResponse>, Status> {
        adapter::handle_download_adapter(request, self).await
    }

    // ========================================================================
    // Genome (Multi-Adapter Stacking)
    // ========================================================================

    async fn apply_genome(
        &self,
        request: Request<ApplyGenomeRequest>,
    ) -> Result<Response<ApplyGenomeResponse>, Status> {
        genome::handle_apply_genome(request, self).await
    }

    // ========================================================================
    // Server Status
    // ========================================================================

    async fn status(
        &self,
        request: Request<StatusRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        status::handle_status(
            request,
            &self.state,
            &self.adapters,
            &self.worker_pool,
            &self.stats,
        )
        .await
    }
}
