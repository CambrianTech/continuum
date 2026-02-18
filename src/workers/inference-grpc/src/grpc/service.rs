//! InferenceService - Core service struct and constructors
//!
//! The main gRPC service implementation supporting:
//! - Worker Pool (quantized) - Multiple model instances for concurrent inference
//! - Single Instance (BF16) - For LoRA adapter support

use log::info;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::lora::LoadedAdapter;
use crate::model::ModelState;
use crate::quantized_model::QuantizedModelState;
use crate::worker_pool::WorkerPool;

/// Server statistics tracking
pub struct ServerStats {
    pub requests_completed: AtomicU64,
    pub requests_pending: AtomicU64,
}

impl ServerStats {
    pub fn new() -> Self {
        Self {
            requests_completed: AtomicU64::new(0),
            requests_pending: AtomicU64::new(0),
        }
    }

    pub fn inc_pending(&self) {
        self.requests_pending.fetch_add(1, Ordering::SeqCst);
    }

    pub fn dec_pending(&self) {
        self.requests_pending.fetch_sub(1, Ordering::SeqCst);
    }

    pub fn inc_completed(&self) {
        self.requests_completed.fetch_add(1, Ordering::SeqCst);
    }
}

impl Default for ServerStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Main gRPC service struct
/// Supports both full-precision (BF16) and quantized (GGUF Q4) models
pub struct InferenceService {
    /// Full-precision model state (BF16) - for LoRA support
    pub state: Arc<RwLock<Option<ModelState>>>,
    /// Quantized model state (GGUF Q4_K_M) - single instance fallback
    pub quantized_state: Arc<RwLock<Option<QuantizedModelState>>>,
    /// Worker pool for concurrent quantized inference
    pub worker_pool: Option<Arc<WorkerPool>>,
    /// Request statistics
    pub stats: Arc<ServerStats>,
    /// Loaded LoRA adapters
    pub adapters: Arc<RwLock<Vec<LoadedAdapter>>>,
}

impl InferenceService {
    /// Create service with full-precision model only (for LoRA support)
    #[allow(dead_code)]
    pub fn new(state: Option<ModelState>) -> Self {
        Self {
            state: Arc::new(RwLock::new(state)),
            quantized_state: Arc::new(RwLock::new(None)),
            worker_pool: None,
            stats: Arc::new(ServerStats::new()),
            adapters: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Create service with single quantized model (legacy, fallback)
    pub fn new_with_quantized(
        state: Option<ModelState>,
        quantized: Option<QuantizedModelState>,
    ) -> Self {
        Self {
            state: Arc::new(RwLock::new(state)),
            quantized_state: Arc::new(RwLock::new(quantized)),
            worker_pool: None,
            stats: Arc::new(ServerStats::new()),
            adapters: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Create service with worker pool for concurrent quantized inference
    ///
    /// This is the recommended mode for high-throughput scenarios.
    /// Falls back to BF16 single-instance when LoRA adapters are needed.
    pub fn new_with_pool(pool: WorkerPool) -> Self {
        let num_workers = pool.num_workers;
        info!("🏭 InferenceService using worker pool ({num_workers} workers)");
        Self {
            state: Arc::new(RwLock::new(None)),
            quantized_state: Arc::new(RwLock::new(None)),
            worker_pool: Some(Arc::new(pool)),
            stats: Arc::new(ServerStats::new()),
            adapters: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Check if we're in quantized mode (pool or single instance)
    pub async fn is_quantized(&self) -> bool {
        self.worker_pool.is_some() || self.quantized_state.read().await.is_some()
    }

    /// Auto-switch from quantized to BF16 mode for LoRA support
    ///
    /// Returns Ok(()) if switch successful or already in BF16 mode.
    /// Returns Err(message) if switch failed.
    pub async fn ensure_bf16_mode(&self) -> Result<(), String> {
        if !self.is_quantized().await {
            return Ok(()); // Already in BF16 mode
        }

        info!("🔄 Auto-switching from quantized to BF16 mode...");

        // Unload quantized model
        {
            let mut q_state = self.quantized_state.write().await;
            *q_state = None;
        }

        // Load BF16 model
        let load_result = tokio::task::spawn_blocking(crate::model::load_default_model).await;

        match load_result {
            Ok(Ok(new_state)) => {
                let mut state = self.state.write().await;
                *state = Some(new_state);
                info!("✅ Switched to BF16 mode");
                Ok(())
            }
            Ok(Err(e)) => {
                let msg = format!("Failed to switch to BF16 mode: {e}");
                info!("❌ {msg}");
                Err(msg)
            }
            Err(e) => {
                let msg = format!("Mode switch task failed: {e}");
                info!("❌ {msg}");
                Err(msg)
            }
        }
    }
}
