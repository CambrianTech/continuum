/// Shared Rust modules for all JTAG workers
///
/// This contains common code used across all Rust workers:
/// - JTAGProtocol: Universal packet format (JSON for control)
/// - BinaryProtocol: Zero-copy binary format (for data payloads)
/// - Common utilities
/// - Shared types

pub mod jtag_protocol;
pub mod binary_protocol;

// Re-export commonly used types for convenience
pub use jtag_protocol::{JTAGErrorType, JTAGRequest, JTAGResponse};

// Re-export binary protocol types
pub use binary_protocol::{
    write_binary_payload, write_embeddings, write_frame, write_single_embedding, BinaryHeader,
    DataType, FrameFormat,
};
