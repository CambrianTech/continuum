//! One block layer for every memory tier (docs/architecture/UNIFIED-PAGING.md).
//!
//! - [`TierKind`]: what kind of memory a region is in, detected per node, never listed.
//! - [`Region`]: a run of memory a tier gave out, RAII: its memory is returned when it drops.
//! - [`Block`]: what a consumer holds: its own key, its region, where its bytes are now, and pins.
//! - [`link`]: how a block's bytes move between tiers: a pointer, the OS's paging, one DMA of one block.
//! - [`streamer`]: the one SAGE component that moves blocks, soonest demanded first, on its own worker.
//!
//! A block's bytes are the consumer's native layout. Nothing here reads, copies or encodes them.

mod block;
mod instances;
mod region;
mod tier;
pub mod link;
pub mod streamer;

pub use block::{Block, Pin, Placement};
pub use instances::{Instance, Reclaimed};
pub use region::{Backing, Region, RegionRefusal};
pub use tier::TierKind;
