//! Where a persona's bytes are: residency tiers behind one contract.

mod execute;
mod fence;
mod learn;
mod tier;

pub use execute::{Completion, Execute};
pub use fence::{Direction, Fence, FenceTag};
pub use learn::Learns;
pub use tier::{Native, Tier, TierRefusal};
