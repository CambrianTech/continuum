/// Shared Rust modules for all JTAG workers
///
/// This contains common code used across all Rust workers:
/// - JTAGProtocol: Universal packet format (JSON for control)
/// - BinaryProtocol: Zero-copy binary format (for data payloads)
/// - GpuAllocator: Centralized GPU memory management
/// - LoggerClient: Structured logging to JTAG log daemon
/// - Common utilities and shared types

pub mod jtag_protocol;
pub mod binary_protocol;
pub mod gpu_allocator;
pub mod logger_client;

// Re-export commonly used types for convenience
pub use jtag_protocol::{JTAGErrorType, JTAGRequest, JTAGResponse};

// Re-export binary protocol types
pub use binary_protocol::{
    write_binary_payload, write_embeddings, write_frame, write_single_embedding, BinaryHeader,
    DataType, FrameFormat,
};

// Re-export GPU allocator
pub use gpu_allocator::{get_gpu_allocator, AllocationRequest, AllocationResult, GpuAllocator, GpuStatus};
