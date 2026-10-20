//! SWE-bench: the repo-test grader, in Rust.
//!
//! A SWE-bench instance is a real GitHub issue plus the repo at the commit before its fix. A
//! solution is a patch. The protocol is fixed and small:
//!
//! ```text
//! clone @ base_commit -> apply model_patch -> apply test_patch -> run tests
//! RESOLVED  iff  every FAIL_TO_PASS passes AND every PASS_TO_PASS passes
//! ```
//!
//! **Why this is Rust and not a script.** Benchmarks are the curriculum spine — they feed
//! training signal, eval integrity, and the paper claim — so every benchmark operation is a
//! command with a handle and events, exactly like a model download
//! ([[benchmark-infra-is-substrate-commands-handles-events-never-bash]]). The Python harness
//! this replaces cost, in one morning: a zsh word-splitting bug that silently ran the whole
//! instance list as a single bogus id, a 60-minute blind poll loop whose output was buffered
//! so a dead dispatch looked identical to a working one, and no liveness signal at all. None
//! of those are Python's fault in principle and all of them are what hand-rolled harnesses
//! actually do in practice.
//!
//! **The one thing that stays Python is the SUBJECT.** flask's and sympy's test suites are
//! pytest; running them is what the benchmark IS, no more a dependency of ours than `rustc`
//! is for `humaneval-rs`. We never write Python — we invoke the artifact's own tests and read
//! the verdicts. `uv` (itself a Rust binary) builds the per-instance environment.
//!
//! Three hazards are load-bearing here, each learned the expensive way today:
//!
//! 1. **The gate.** FAIL_TO_PASS must FAIL on the pristine tree. Eight runs were scored
//!    against a clone left at HEAD where the fix already existed, so 0 and 1 meant the same
//!    thing ([[the-swe-workspace-was-never-at-the-base-commit-the-bug-was-not-there]]).
//! 2. **Id shape.** sympy's FAIL_TO_PASS entries are bare function names, not pytest node
//!    ids. Feeding those to pytest as paths yields "file or directory not found", which scores
//!    as a failure and mis-scored GOLD on three instances whose environments were fine.
//! 3. **Era.** Both the dependency graph AND the interpreter have one. 2014 code does
//!    `from collections import Mapping`, deleted in 3.10; no pin rescues that.

use crate::time::EpochMs;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

mod swe_instance;
pub use swe_instance::*;
mod fetch_hf_rows;
pub use fetch_hf_rows::*;
mod diff_target_paths;
pub use diff_target_paths::*;
mod build_env_from_scratch;
pub use build_env_from_scratch::*;
mod log_parser;
pub use log_parser::*;
mod split_node_outcome;
pub use split_node_outcome::*;
mod grade;
pub use grade::*;

#[cfg(test)]
mod tests;
