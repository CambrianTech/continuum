/// Shared Rust modules for all JTAG workers
///
/// This contains common code used across all Rust workers:
/// - JTAGProtocol: Universal packet format
/// - Common utilities
/// - Shared types

pub mod jtag_protocol;

// Re-export commonly used types for convenience
pub use jtag_protocol::{
    JTAGRequest,
    JTAGResponse,
    JTAGErrorType,
    // Legacy aliases
    WorkerRequest,
    WorkerResponse,
    ErrorType,
};
