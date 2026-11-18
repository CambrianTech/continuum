//! Where a persona's bytes are: residency tiers behind one contract.

mod fence;
mod tier;

pub use fence::Fence;
pub use tier::{Tier, TierRefusal};
