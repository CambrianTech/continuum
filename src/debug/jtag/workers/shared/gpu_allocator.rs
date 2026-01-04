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

    /// Find eviction candidates using LRU + priority
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
        // Score = age_seconds / (priority * 10)
        candidates.sort_by(|a, b| {
            let score_a = a.last_used.elapsed().as_secs_f32() / (a.priority * 10.0);
            let score_b = b.last_used.elapsed().as_secs_f32() / (b.priority * 10.0);
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
        });

        assert!(matches!(result, AllocationResult::Granted));

        let status = allocator.status();
        assert_eq!(status.allocated_mb, 100);
        assert_eq!(status.available_mb, 900);
    }

    #[test]
    fn test_eviction_suggestion() {
        let allocator = GpuAllocator::new(200, 0.8);

        // Fill up memory
        allocator.allocate(AllocationRequest {
            id: "adapter-1".to_string(),
            owner: "persona-1".to_string(),
            size_mb: 100,
            priority: 0.3,
        });
        allocator.allocate(AllocationRequest {
            id: "adapter-2".to_string(),
            owner: "persona-1".to_string(),
            size_mb: 100,
            priority: 0.5,
        });

        // Request more than available
        let result = allocator.allocate(AllocationRequest {
            id: "adapter-3".to_string(),
            owner: "persona-2".to_string(),
            size_mb: 50,
            priority: 0.7,
        });

        match result {
            AllocationResult::NeedEviction { suggested_victims } => {
                // Should suggest adapter-1 (lower priority)
                assert!(suggested_victims.contains(&"adapter-1".to_string()));
            }
            _ => panic!("Expected NeedEviction"),
        }
    }
}
