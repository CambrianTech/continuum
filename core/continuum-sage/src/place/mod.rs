//! SAGE's placement contract, once for everything finite that is shared (SAGE.md, "Ebb and flow";
//! UNIFIED-PAGING.md, slice 3a). Bytes on a tier, minds awake on a node, models hot, participants
//! rendered full: each is a place holding a typed quantity, and each consumer asks the same way.
//!
//! - **Prepare**: one claim across every place it needs, all of it or a wait. A wait is never a
//!   refusal; it names its shortfall, which is the demand the placement law makes room for.
//! - **Consume**: the consumer spends its granted claim from its own atomics and never waits, so a
//!   claim can be spent inside an engine step.
//! - **Commit or unwind**: what was consumed becomes held, what was not is returned; a claim that will
//!   not be spent returns everything.
//! - **Release**: a holding returns only when its owner's allocator has released it.
//! - **Never here**: refused only past a place's proven capacity, never for pressure.
//!
//! The ledger is `ledger`; the [`Placer`] is the component that owns one per domain (`component`). Its law
//! as a `decide::Policy` use case, and its consumers, come next on this contract.

mod claim;
mod composite;
mod flex;
mod holding;
mod levels;
mod component;
mod core;
mod ledger;
mod quantity;
mod resident;

pub use claim::{Claim, ClaimId, Generation, Need, Owner};
pub use component::{key_of, Act, AskId, Asks, Domain, Placer, Shortfall, Status};
pub use core::PlacerCore;
pub use ledger::{Accounts, Answer, Ledger, NeverNow, Refusal, ShrinkTo, Untaken};
pub use composite::{Composite, Priority};
pub use flex::{Flex, Flexbox, Landed, Share, Shared};
pub use holding::{Answered, Asked, Holding, Landing, Receipt, Ticket};
pub use levels::{Facts, Fixed, Moved, Resize, ResizeTo, Resizing};
pub use quantity::{Bytes, Quantity, Slots, Tokens};
pub use resident::{Cost, Given, GrowGrant, Report, Requires, Resident, Row, Sharing, ShrinkAsk, Varies, Way, Ways};

#[cfg(test)]
mod tests;
