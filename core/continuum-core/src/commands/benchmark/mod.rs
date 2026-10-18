//! `benchmark/*` — first-class benchmark collections, managed like the model catalog. A
//! declarative catalog of known benchmarks (add one = one row, same as a `ModelSpec`) plus
//! `benchmark/list` to see them and `benchmark/dispatch` to adapt one INTO the work board as
//! claimable cards. There is no divergent "run a benchmark" verb: every benchmark goes through
//! the natural kanban loop — citizens claim a card and solve it with their own hands, and the
//! result is graded from the artifacts they produce. This lives in Rust, on the DynCommand
//! registry, so it is
//! ON-GRID: discoverable, persona-callable, and manageable by the daemons — unlike the
//! toolchain-free `benchmarks/coder/oneshot_opponent.py` script, whose ONLY job is letting an
//! OUTSIDER replicate our numbers against their own `/v1` without our stack. Operational
//! benchmarking is Rust; the replication convenience is the lone edge script.

pub(crate) mod credit_recovery;

mod catalog;
mod list;
mod record;
mod dispatch_types;
mod dispatch_cards;
mod dispatch_prepare;
mod dispatch;
mod swe_grade;
mod swe_setup;
mod runs;
mod scoreboard;
mod validate;
mod rounds;
mod fetch;

pub use self::catalog::*;
pub use self::list::*;
pub use self::record::*;
pub use self::dispatch_types::*;
pub(crate) use self::dispatch_cards::*;
pub(crate) use self::dispatch_prepare::*;
#[cfg(test)]
use self::dispatch::publish_citizen_deck;
pub use self::swe_grade::*;
pub use self::swe_setup::*;
pub use self::runs::*;
pub use self::scoreboard::*;
pub use self::validate::*;
pub use self::rounds::*;
pub use self::fetch::*;

#[cfg(test)]
mod tests;
