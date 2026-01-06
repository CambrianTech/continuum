//! GPU Memory Allocator - Centralized GPU resource management
//!
//! Single source of truth for GPU memory across all workers.
//! Handles allocation, deallocation, and pressure-based eviction.
//!
//! Architecture:
//! - One GpuAllocator per system (singleton pattern via lazy_static)
//! - Clients request/release memory by name (adapter_id)
//! - Automatic LRU eviction when pressure is high
//! - Query interface for TypeScript coordination

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Memory allocation record
#[derive(Debug, Clone)]
pub struct Allocation {
    /// Unique identifier (adapter name, model id, etc.)
    pub id: String,
    /// Owner identifier (persona id, worker id)
    pub owner: String,
    /// Allocated size in MB
    pub size_mb: u64,
    /// When this allocation was last used
    pub last_used: Instant,
    /// Priority (higher = less likely to evict)
    pub priority: f32,
    /// Time to load this allocation (for paging optimization)
    pub load_time_ms: u64,
    /// Type of allocation (model, adapter, embedding)
    pub alloc_type: AllocationType,
}

/// Type of GPU allocation (for paging strategy)
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum AllocationType {
    /// Base model - expensive to load, keep resident
    Model,
    /// LoRA adapter - cheap to load, can page freely
    Adapter,
    /// Embedding model - medium cost
    Embedding,
    /// Other/unknown
    Other,
}

/// GPU memory status
#[derive(Debug, Clone, serde::Serialize)]
pub struct GpuStatus {
    /// Total GPU memory in MB
    pub total_mb: u64,
    /// Currently allocated in MB
    pub allocated_mb: u64,
    /// Available memory in MB
    pub available_mb: u64,
    /// Memory pressure (0.0 - 1.0)
    pub pressure: f32,
    /// Number of active allocations
    pub allocation_count: usize,
}

/// Allocation request
#[derive(Debug)]
pub struct AllocationRequest {
    pub id: String,
    pub owner: String,
    pub size_mb: u64,
    pub priority: f32,
    /// Time it took to load (for paging optimization)
    pub load_time_ms: Option<u64>,
    /// Type of allocation
    pub alloc_type: Option<AllocationType>,
}

/// Allocation result
#[derive(Debug)]
pub enum AllocationResult {
    /// Allocation succeeded
    Granted,
    /// Allocation denied - need to evict first
    NeedEviction { suggested_victims: Vec<String> },
    /// Allocation denied - not enough memory even after eviction
    Denied { reason: String },
}

/// GPU Memory Allocator
///
/// Thread-safe, centralized GPU memory manager.
/// Uses interior mutability for safe concurrent access.
pub struct GpuAllocator {
    /// Total GPU memory in MB (detected or configured)
    total_mb: u64,
    /// Current allocations by id
    allocations: Mutex<HashMap<String, Allocation>>,
    /// High water mark for pressure-based eviction (0.0 - 1.0)
    eviction_threshold: f32,
}

impl GpuAllocator {
    /// Create a new GPU allocator
    ///
    /// # Arguments
    /// * `total_mb` - Total GPU memory in MB (0 = auto-detect)
    /// * `eviction_threshold` - Pressure level that triggers eviction suggestions (default 0.8)
    pub fn new(total_mb: u64, eviction_threshold: f32) -> Self {
        let actual_total = if total_mb == 0 {
            Self::detect_gpu_memory()
        } else {
            total_mb
        };

        Self {
            total_mb: actual_total,
            allocations: Mutex::new(HashMap::new()),
            eviction_threshold,
        }
    }

    /// Detect available GPU memory (Metal on macOS, CUDA info on Linux)
    fn detect_gpu_memory() -> u64 {
        // TODO: Use metal-rs or cuda-sys to query actual GPU memory
        // For now, default to 8GB (common for Apple Silicon)
        8192
    }

    /// Request a memory allocation
    pub fn allocate(&self, request: AllocationRequest) -> AllocationResult {
        let mut allocations = self.allocations.lock().unwrap();

        // Check if already allocated (update last_used)
        if let Some(existing) = allocations.get_mut(&request.id) {
            existing.last_used = Instant::now();
            return AllocationResult::Granted;
        }

        let current_used: u64 = allocations.values().map(|a| a.size_mb).sum();
        let after_alloc = current_used + request.size_mb;

        // Check if we have space
        if after_alloc <= self.total_mb {
            allocations.insert(
                request.id.clone(),
                Allocation {
                    id: request.id,
                    owner: request.owner,
                    size_mb: request.size_mb,
                    last_used: Instant::now(),
                    priority: request.priority,
                    load_time_ms: request.load_time_ms.unwrap_or(0),
                    alloc_type: request.alloc_type.unwrap_or(AllocationType::Other),
                },
            );
            return AllocationResult::Granted;
        }

        // Need to evict - find candidates
        let needed = after_alloc - self.total_mb;
        let victims = self.find_eviction_candidates(&allocations, needed, &request.owner);

        if victims.is_empty() {
            AllocationResult::Denied {
                reason: format!(
                    "Need {}MB but only {}MB available, no evictable allocations",
                    request.size_mb,
                    self.total_mb - current_used
                ),
            }
        } else {
            AllocationResult::NeedEviction {
                suggested_victims: victims,
            }
        }
    }

    /// Release an allocation
    pub fn release(&self, id: &str) -> Option<Allocation> {
        let mut allocations = self.allocations.lock().unwrap();
        allocations.remove(id)
    }

    /// Find eviction candidates using LRU + priority + load cost
    ///
    /// Eviction score formula (higher = more evictable):
    /// score = (age_seconds * type_weight) / (priority * 10 * reload_cost_weight)
    ///
    /// Where:
    /// - type_weight: Adapter=2.0, Embedding=1.0, Model=0.5, Other=1.0
    ///   (prefer evicting adapters - cheap to reload)
    /// - reload_cost_weight: 1 + (load_time_ms / 1000)
    ///   (avoid evicting things that took long to load)
    fn find_eviction_candidates(
        &self,
        allocations: &HashMap<String, Allocation>,
        needed_mb: u64,
        exclude_owner: &str,
    ) -> Vec<String> {
        let mut candidates: Vec<_> = allocations
            .values()
            .filter(|a| a.owner != exclude_owner && a.priority < 0.9)
            .collect();

        // Sort by eviction score (higher = more evictable)
        candidates.sort_by(|a, b| {
            let score_a = Self::calculate_eviction_score(a);
            let score_b = Self::calculate_eviction_score(b);
            score_b.partial_cmp(&score_a).unwrap()
        });

        // Collect enough to free needed memory
        let mut victims = Vec::new();
        let mut freed = 0u64;

        for candidate in candidates {
            if freed >= needed_mb {
                break;
            }
            victims.push(candidate.id.clone());
            freed += candidate.size_mb;
        }

        victims
    }

    /// Calculate eviction score for an allocation
    /// Higher score = more likely to evict
    fn calculate_eviction_score(alloc: &Allocation) -> f32 {
        let age_seconds = alloc.last_used.elapsed().as_secs_f32();

        // Type weight: prefer evicting adapters (cheap to reload)
        let type_weight = match alloc.alloc_type {
            AllocationType::Adapter => 2.0,   // Prefer evicting (cheap)
            AllocationType::Embedding => 1.0, // Neutral
            AllocationType::Model => 0.3,     // Avoid evicting (expensive)
            AllocationType::Other => 1.0,     // Neutral
        };

        // Reload cost weight: avoid evicting things that took long to load
        let reload_cost_weight = 1.0 + (alloc.load_time_ms as f32 / 1000.0);

        // Score = (age * type_weight) / (priority * 10 * reload_cost)
        (age_seconds * type_weight) / (alloc.priority * 10.0 * reload_cost_weight)
    }

    /// Get current GPU status
    pub fn status(&self) -> GpuStatus {
        let allocations = self.allocations.lock().unwrap();
        let allocated: u64 = allocations.values().map(|a| a.size_mb).sum();

        GpuStatus {
            total_mb: self.total_mb,
            allocated_mb: allocated,
            available_mb: self.total_mb.saturating_sub(allocated),
            pressure: allocated as f32 / self.total_mb as f32,
            allocation_count: allocations.len(),
        }
    }

    /// Get all allocations (for debugging)
    pub fn list_allocations(&self) -> Vec<Allocation> {
        let allocations = self.allocations.lock().unwrap();
        allocations.values().cloned().collect()
    }

    /// Check if pressure is above eviction threshold
    pub fn should_evict(&self) -> bool {
        self.status().pressure >= self.eviction_threshold
    }

    /// Touch an allocation (update last_used without reallocating)
    pub fn touch(&self, id: &str) -> bool {
        let mut allocations = self.allocations.lock().unwrap();
        if let Some(alloc) = allocations.get_mut(id) {
            alloc.last_used = Instant::now();
            true
        } else {
            false
        }
    }

    /// Get paging statistics by allocation type
    pub fn paging_stats(&self) -> PagingStats {
        let allocations = self.allocations.lock().unwrap();

        let mut stats = PagingStats::default();

        for alloc in allocations.values() {
            match alloc.alloc_type {
                AllocationType::Model => {
                    stats.model_count += 1;
                    stats.model_mb += alloc.size_mb;
                    stats.model_load_time_ms += alloc.load_time_ms;
                }
                AllocationType::Adapter => {
                    stats.adapter_count += 1;
                    stats.adapter_mb += alloc.size_mb;
                    stats.adapter_load_time_ms += alloc.load_time_ms;
                }
                AllocationType::Embedding => {
                    stats.embedding_count += 1;
                    stats.embedding_mb += alloc.size_mb;
                }
                AllocationType::Other => {
                    stats.other_count += 1;
                    stats.other_mb += alloc.size_mb;
                }
            }
        }

        // Calculate average load times
        if stats.model_count > 0 {
            stats.avg_model_load_ms = stats.model_load_time_ms / stats.model_count as u64;
        }
        if stats.adapter_count > 0 {
            stats.avg_adapter_load_ms = stats.adapter_load_time_ms / stats.adapter_count as u64;
        }

        stats
    }
}

/// Statistics for paging optimization
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct PagingStats {
    /// Number of base models loaded
    pub model_count: usize,
    /// Total MB used by models
    pub model_mb: u64,
    /// Total load time for models (ms)
    pub model_load_time_ms: u64,
    /// Average model load time (ms)
    pub avg_model_load_ms: u64,

    /// Number of LoRA adapters loaded
    pub adapter_count: usize,
    /// Total MB used by adapters
    pub adapter_mb: u64,
    /// Total load time for adapters (ms)
    pub adapter_load_time_ms: u64,
    /// Average adapter load time (ms)
    pub avg_adapter_load_ms: u64,

    /// Number of embedding models loaded
    pub embedding_count: usize,
    /// Total MB used by embeddings
    pub embedding_mb: u64,

    /// Other allocations
    pub other_count: usize,
    /// Total MB used by other
    pub other_mb: u64,
}

// Global singleton
lazy_static::lazy_static! {
    /// Global GPU allocator instance
    pub static ref GPU_ALLOCATOR: Arc<GpuAllocator> = Arc::new(
        GpuAllocator::new(0, 0.8) // Auto-detect memory, 80% eviction threshold
    );
}

/// Get the global GPU allocator
pub fn get_gpu_allocator() -> Arc<GpuAllocator> {
    GPU_ALLOCATOR.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_allocation() {
        let allocator = GpuAllocator::new(1000, 0.8);

        let result = allocator.allocate(AllocationRequest {
            id: "adapter-1".to_string(),
            owner: "persona-1".to_string(),
            size_mb: 100,
            priority: 0.5,
            load_time_ms: Some(500),
            alloc_type: Some(AllocationType::Adapter),
        });

        assert!(matches!(result, AllocationResult::Granted));

        let status = allocator.status();
        assert_eq!(status.allocated_mb, 100);
        assert_eq!(status.available_mb, 900);
    }

    #[test]
    fn test_eviction_prefers_adapters_over_models() {
        let allocator = GpuAllocator::new(200, 0.8);

        // Load a model (expensive to reload - 7000ms)
        allocator.allocate(AllocationRequest {
            id: "base-model".to_string(),
            owner: "persona-1".to_string(),
            size_mb: 100,
            priority: 0.5,
            load_time_ms: Some(7000),
            alloc_type: Some(AllocationType::Model),
        });

        // Load an adapter (cheap to reload - 200ms)
        allocator.allocate(AllocationRequest {
            id: "lora-adapter".to_string(),
            owner: "persona-1".to_string(),
            size_mb: 100,
            priority: 0.5,
            load_time_ms: Some(200),
            alloc_type: Some(AllocationType::Adapter),
        });

        // Request more than available - should suggest evicting adapter, not model
        let result = allocator.allocate(AllocationRequest {
            id: "new-adapter".to_string(),
            owner: "persona-2".to_string(),
            size_mb: 50,
            priority: 0.7,
            load_time_ms: Some(300),
            alloc_type: Some(AllocationType::Adapter),
        });

        match result {
            AllocationResult::NeedEviction { suggested_victims } => {
                // Should suggest the adapter (cheap to reload), not the model
                assert!(suggested_victims.contains(&"lora-adapter".to_string()));
                assert!(!suggested_victims.contains(&"base-model".to_string()));
            }
            _ => panic!("Expected NeedEviction"),
        }
    }

    #[test]
    fn test_paging_stats() {
        let allocator = GpuAllocator::new(10000, 0.8);

        // Add a model
        allocator.allocate(AllocationRequest {
            id: "llama-3b".to_string(),
            owner: "shared".to_string(),
            size_mb: 3000,
            priority: 0.9,
            load_time_ms: Some(7500),
            alloc_type: Some(AllocationType::Model),
        });

        // Add some adapters
        allocator.allocate(AllocationRequest {
            id: "code-adapter".to_string(),
            owner: "persona-1".to_string(),
            size_mb: 100,
            priority: 0.5,
            load_time_ms: Some(200),
            alloc_type: Some(AllocationType::Adapter),
        });
        allocator.allocate(AllocationRequest {
            id: "chat-adapter".to_string(),
            owner: "persona-2".to_string(),
            size_mb: 100,
            priority: 0.5,
            load_time_ms: Some(180),
            alloc_type: Some(AllocationType::Adapter),
        });

        let stats = allocator.paging_stats();
        assert_eq!(stats.model_count, 1);
        assert_eq!(stats.model_mb, 3000);
        assert_eq!(stats.avg_model_load_ms, 7500);
        assert_eq!(stats.adapter_count, 2);
        assert_eq!(stats.adapter_mb, 200);
        assert_eq!(stats.avg_adapter_load_ms, 190); // (200 + 180) / 2
    }
}
