//! Where a persona's bytes are: residency tiers behind one contract.

mod fence;
mod tier;

pub use fence::{Direction, Fence, FenceTag};
pub use tier::{Native, Tier, TierRefusal};
