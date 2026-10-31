//! Concrete disk eviction pools — wire (2) of task #155.
//!
//! The 2026-07-13 incident, part two: the eviction ECONOMY was already
//! live (PressureBrokerModule ticks `relieve()` every 5 s at boot, and
//! `DiskPressureMonitor` registers as a signal-only pool), but no pool
//! that OWNS deletable disk content was ever registered — so the broker
//! spent the incident emitting the designed "disk hot AND nobody owns
//! the eviction" zero-byte alerts while cargo-target grew to 363 GB.
//! On an operator's machine that's an inconvenience; on a public user's
//! machine it trashes their system. This module is the first concrete
//! owner; each cache class from `standard_tracked_dirs` gets one over
//! time (genome-models and hf-hub need reference-aware LRU and come
//! later; cargo-target is pure derived artifact and comes first).
//!
//! ## Safety invariants (each pinned by a test)
//!
//! 1. **Never race a live build.** Eviction takes non-blocking exclusive
//!    flocks on cargo's own lock files (`<root>/.cargo-lock`,
//!    `<root>/{debug,release}/.cargo-lock`) before touching anything; any lock we
//!    can't get means a build is in flight → free 0 bytes and let the
//!    broker retry next tick.
//! 2. **Never leave the root.** The eviction ladder is a fixed list of
//!    `root.join(...)` subpaths — no pattern matching, no following
//!    symlinks or Windows reparse points out of the tree. Validation and removal
//!    stay under the same Cargo custody locks.
//! 3. **Derived artifacts only.** The ladder deletes incremental state,
//!    test binaries, then the debug tree — all reproducible by the next
//!    `cargo build`. Standard release incremental state is also reclaimable;
//!    release build inputs require an explicit producer static-input declaration.
//!    Generic deps/build and top-level installer inputs remain retained and reported. Custom profiles
//!    and target-triple subtrees are not inferred from arbitrary directory names.

use crate::system_resources::hands_lease::PersonaHandsLease;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::paging::pool::{ResourcePool, ResourcePoolEntry};

use super::disk_reporters::{dir_size_bytes, TrackedDir};

// The tests gate themselves (`#![cfg(test)]` in tests.rs), so the scanner reads the file as test code.
mod tests;

// The cache-class eviction decisions the tests check (`#![cfg(test)]` in the file).
mod eviction_decisions;

mod default_cargo_target_budget_bytes;
pub use default_cargo_target_budget_bytes::*;
mod cargo_target_pool;
pub use cargo_target_pool::*;
mod worktree_targets_pool;
pub use worktree_targets_pool::{WorktreeEvictionMode, WorktreeTargetsPool};
