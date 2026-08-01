//! Re-export shim: the GGML_MOE_PLAN_FILE writer moved to the
//! `expert-pager-policy` leaf crate (windows-msvc driver requirement).
pub use expert_pager_policy::plan_file::*;
