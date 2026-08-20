//! Re-export shim: the BanditPlanController moved to the
//! `expert-pager-policy` leaf crate (windows-msvc driver requirement).
pub use expert_pager_policy::controller::*;
