//! Model management handlers
//!
//! Handles model loading, unloading, and listing operations.

use log::info;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};

use crate::inference::{
    ListModelsRequest, ListModelsResponse, LoadModelRequest, LoadModelResponse, ModelInfo,
    UnloadModelRequest, UnloadModelResponse,
};
use crate::model::{load_model_by_id, ModelState};

/// Load a model by ID
pub async fn handle_load_model(
    request: Request<LoadModelRequest>,
    state: &Arc<RwLock<Option<ModelState>>>,
) -> Result<Response<LoadModelResponse>, Status> {
    let req = request.into_inner();
    let model_id = req.model_id;

    info!("📥 LoadModel: {model_id}");
    let start = Instant::now();

    let result = tokio::task::spawn_blocking(move || load_model_by_id(&model_id)).await;

    match result {
        Ok(Ok(new_state)) => {
            let load_time_ms = start.elapsed().as_millis() as i64;

            let mut state = state.write().await;
            *state = Some(new_state);

            info!("✅ Model loaded in {load_time_ms}ms");
            Ok(Response::new(LoadModelResponse {
                success: true,
                error: String::new(),
                load_time_ms,
                memory_bytes: 0,
            }))
        }
        Ok(Err(e)) => {
            info!("❌ Failed to load model: {e}");
            Ok(Response::new(LoadModelResponse {
                success: false,
                error: e.to_string(),
                load_time_ms: 0,
                memory_bytes: 0,
            }))
        }
        Err(e) => {
            info!("❌ Load task failed: {e}");
            Ok(Response::new(LoadModelResponse {
                success: false,
                error: format!("Task join error: {e}"),
                load_time_ms: 0,
                memory_bytes: 0,
            }))
        }
    }
}

/// Unload the current model
pub async fn handle_unload_model(
    _request: Request<UnloadModelRequest>,
    state: &Arc<RwLock<Option<ModelState>>>,
) -> Result<Response<UnloadModelResponse>, Status> {
    info!("📤 UnloadModel");

    let mut state = state.write().await;
    if state.is_some() {
        *state = None;
        info!("✅ Model unloaded");
        Ok(Response::new(UnloadModelResponse {
            success: true,
            error: String::new(),
        }))
    } else {
        Ok(Response::new(UnloadModelResponse {
            success: false,
            error: "No model loaded".to_string(),
        }))
    }
}

/// List loaded models
pub async fn handle_list_models(
    _request: Request<ListModelsRequest>,
    state: &Arc<RwLock<Option<ModelState>>>,
) -> Result<Response<ListModelsResponse>, Status> {
    let state = state.read().await;

    let models = if let Some(ref model_state) = *state {
        vec![ModelInfo {
            model_id: model_state.model_id.clone(),
            loaded: true,
            memory_bytes: 0,
            dtype: format!("{:?}", model_state.dtype),
        }]
    } else {
        vec![]
    };

    Ok(Response::new(ListModelsResponse { models }))
}
