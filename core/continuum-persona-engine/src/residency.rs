//! Where a persona's bytes are: residency tiers behind one contract.

mod execute;
mod fence;
mod tier;

pub use execute::{Completion, Execute};
pub use fence::{Carries, Direction, Fence, FenceTag};
pub use tier::{Native, Tier, TierRefusal};
