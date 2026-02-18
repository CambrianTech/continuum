//! Status and health handlers
//!
//! Provides server health checks and statistics.

use log::info;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};

use crate::inference::{
    PingRequest, PingResponse, PriorityStats as ProtoPriorityStats, StatusRequest, StatusResponse,
};
use crate::lora::LoadedAdapter;
use crate::model::ModelState;
use crate::worker_pool::WorkerPool;

use super::service::ServerStats;

/// Health check ping
pub async fn handle_ping(
    _request: Request<PingRequest>,
    state: &Arc<RwLock<Option<ModelState>>>,
) -> Result<Response<PingResponse>, Status> {
    let state = state.read().await;
    let model_loaded = state.is_some();

    Ok(Response::new(PingResponse {
        message: if model_loaded {
            "pong (model loaded)".to_string()
        } else {
            "pong (no model)".to_string()
        },
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64,
    }))
}

/// Server status with statistics
pub async fn handle_status(
    _request: Request<StatusRequest>,
    state: &Arc<RwLock<Option<ModelState>>>,
    adapters: &Arc<RwLock<Vec<LoadedAdapter>>>,
    worker_pool: &Option<Arc<WorkerPool>>,
    stats: &Arc<ServerStats>,
) -> Result<Response<StatusResponse>, Status> {
    let state = state.read().await;
    let adapters = adapters.read().await;

    let current_model = state
        .as_ref()
        .map(|s| s.model_id.clone())
        .unwrap_or_default();

    let active_adapters: Vec<String> = adapters
        .iter()
        .filter(|a| a.active)
        .map(|a| a.adapter_id.clone())
        .collect();

    // Use pool stats when available (more accurate - tracks actual worker activity)
    // Fall back to gRPC-level stats when no pool
    let (requests_completed, requests_pending) = if let Some(pool) = worker_pool {
        let (completed, pending, tokens, time_ms) = pool.stats();
        // Log additional metrics for observability
        if completed > 0 {
            let tokens_per_sec = if time_ms > 0 {
                (tokens as f64 / time_ms as f64) * 1000.0
            } else {
                0.0
            };
            info!(
                "Pool stats: {completed} requests, {tokens} tokens, {tokens_per_sec:.1} tok/s"
            );
        }
        (completed as i32, pending as i32)
    } else {
        (
            stats.requests_completed.load(Ordering::SeqCst) as i32,
            stats.requests_pending.load(Ordering::SeqCst) as i32,
        )
    };

    Ok(Response::new(StatusResponse {
        healthy: state.is_some() || worker_pool.is_some(),
        current_model,
        memory_used_bytes: 0,
        memory_total_bytes: 0,
        requests_pending,
        requests_completed,
        active_adapters,
        priority_stats: Some(ProtoPriorityStats {
            hot_completed: 0,
            hot_avg_wait_ms: 0.0,
            warm_completed: 0,
            warm_avg_wait_ms: 0.0,
            bg_completed: 0,
            bg_avg_wait_ms: 0.0,
        }),
    }))
}
