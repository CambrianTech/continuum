//! Unified paging primitive — the foundation that every paged resource
//! in the system builds on.
//!
//! Same shape used by:
//!   - LoRA adapters (genome registry adopts this)
//!   - KV cache pages (PagedAttention via vllm-metal handles natively)
//!   - MoE expert weights (when MoE forge ships)
//!   - Model weights (multiple loaded models per host)
//!   - Embedding vectors (content-addressed dedup)
//!   - Memory recall (TieredMemoryCache reformulation)
//!
//! The hand-rolled implementations across these consumers diverged on
//! pressure interpretation, eviction, single-flight handling. This is
//! the shared shape they should all adopt.
//!
//! See: docs/architecture/UNIFIED-PAGING.md

pub mod adapter;
pub mod broker;
pub mod pool;

pub use adapter::ResourcePoolAdapter;
pub use broker::{
    BrokerConfig, BrokerSnapshot, PoolView, PressureAlert, PressureBroker, PressureSource,
    PressureTier, ReliefReport,
};
pub use pool::{
    lru_priority, size_weighted_lru, EvictionPriority, PagedResourcePool, PinHandle, PoolConfig,
    PoolEntry, PoolEntryView, PoolStats, ResourceError, ResourcePool, ResourcePoolEntry, Sizer,
};
