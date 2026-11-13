//! Re-export shim: the learned-decay scoring + online decay bandit
//! moved to the `expert-pager-policy` LEAF crate so the RUN-2 driver
//! builds on windows-msvc (continuum-core's unix-socket surface cannot
//! compile there). One source of truth; same module path as before.
pub use expert_pager_policy::decay::*;
