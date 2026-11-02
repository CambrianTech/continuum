//! Serving lanes' admission and ledger.
//!
//! The serving lanes, their permits and the model-call ledger (in-flight gauges,
//! prompt samples, measured waits) are serving facts, so they live with the engine;
//! the mind's own turn bookkeeping is `cognition::turn_ledger`, which calls down here.


mod lanes;
mod metrics;
mod progress;
mod serving_lane_permit;
#[cfg(test)]
mod tests;

pub use lanes::*;
pub use metrics::*;
pub use progress::{LaneWait, LaneProgress};
pub use serving_lane_permit::ServingLanePermit;
