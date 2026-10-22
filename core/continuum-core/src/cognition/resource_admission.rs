//! Shared Rust resource admission.
//!
//! This is the small lease gate that every expensive subsystem can use
//! while the substrate governor becomes the process-wide allocator:
//! inference, training, rendering, audio, TTS, STT, classifiers, RAG,
//! and background work. Callers submit typed resource policy; the gate
//! admits or denies before work starts and returns an RAII guard that
//! releases the lease on every exit path.


mod gate;
mod lanes;
mod metrics;
mod serving_lane_permit;
mod turns;
#[cfg(test)]
mod tests;

pub use gate::*;
pub use lanes::*;
pub use metrics::*;
pub use serving_lane_permit::ServingLanePermit;
pub use turns::*;
