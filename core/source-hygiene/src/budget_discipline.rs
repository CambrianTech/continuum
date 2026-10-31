//! **Every resource is spent through one budget: VRAM, RAM, disk and a mind's context.**
//!
//! The contract is `docs/architecture/ONE-BUDGET.md`. One authority measures each resource
//! and grants it to typed claims; one allocator (the flexbox, priority/floor/min/max/grow)
//! sizes every grant; a consumer reconciles to its grant and never sizes itself. Joel,
//! 2026-10-30: "Everything must follow a governor or a rag budgeter as a consumer or it will
//! inevitably fail."
//!
//! These rules count the ways around that contract. Each is judged two ways (`ratchet.rs`):
//! the count never rises over the base tree, and a file the change touches ends with none
//! (`SOURCE_HYGIENE_TOUCHED`), the same discipline as the 1,000-line gate. That second
//! judgement is what drives the counts to zero instead of holding them.
//!
//! There is no exemption comment. A site is allowed only by being an OWNER, named here by
//! its repository path: the allocator, the authority's probes, the store. Everything else
//! is a consumer and takes its size from a grant.
//!
//! These are lexical rules, a backstop. The lasting enforcement is the type: once claims
//! and grants exist, a cache, ring, lane or file class is constructed from its grant and an
//! unbudgeted one does not compile.

mod probes;
mod self_sized;
mod writes;

#[cfg(test)]
mod tests;

pub use probes::PrivateProbes;
pub use self_sized::SelfSized;
pub use writes::WritesOutsideAClass;

use crate::SourceFile;

/// Does this constant's name say it is a duration or a rate? That is `time_bounds`' class,
/// never `SelfSized`'s; one predicate, so the two rules cannot disagree. A unit is a SUFFIX
/// (`_MS` must not read `MAX_MSGS` as a time).
pub(crate) fn is_duration_name(name: &str) -> bool {
    const UNITS: &[&str] = &["_MS", "_US", "_NS", "_SECS", "_SECONDS", "_MINUTES", "_HOURS", "_PER_S", "_TPS", "_HZ"];
    const WORDS: &[&str] = &["TIMEOUT", "INTERVAL", "CADENCE", "DEADLINE"];
    UNITS.iter().any(|u| name.ends_with(u) || name.contains(&format!("{u}_"))) || WORDS.iter().any(|w| name.contains(w))
}

/// Upstream code ported verbatim carries upstream's numbers; it is not ours to budget.
fn is_vendored(file: &SourceFile) -> bool {
    file.repo.split('/').any(|part| part == "vendored")
}

/// Does `file` live under one of `owners` (repository path prefixes)?
fn owned_by(file: &SourceFile, owners: &[&str]) -> bool {
    owners.iter().any(|owner| file.repo.starts_with(owner))
}
