//! Shared Rust resource admission, and the serving lanes' admission and ledger.
//!
//! This is the small lease gate that every expensive subsystem can use
//! while the substrate governor becomes the process-wide allocator:
//! inference, training, rendering, audio, TTS, STT, classifiers, RAG,
//! and background work. Callers submit typed resource policy; the gate
//! admits or denies before work starts and returns an RAII guard that
//! releases the lease on every exit path.
//!
//! The serving lanes, their permits and the model-call ledger (in-flight gauges,
//! prompt samples, measured waits) are serving facts, so they live with the engine;
//! the mind's own turn bookkeeping is `cognition::turn_ledger`, which calls down here.

mod gate;
mod lane_handoff;
mod lanes;
mod metrics;
mod progress;
mod serving_lane_permit;
#[cfg(test)]
mod tests;

pub use gate::*;
pub(crate) use lane_handoff::take_serving_lane;
pub use lane_handoff::with_serving_lane;
pub use lanes::*;
pub use metrics::*;
pub use progress::{LaneProgress, LaneWait};
pub use serving_lane_permit::ServingLanePermit;
