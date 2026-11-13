//! What each native code verb's own result says it did (ONE-ACT 6b): every native tool
//! projects its outcome (`natives_without_an_outcome` = 0).
//!
//! The failure this ends: `code/edit` and `code/write` projected nothing, so a refused edit
//! (`WriteResult { success: false, error: Some(..) }`) rendered as a success in her receipt
//! (Benchy, the M5, 2026-11-05 02:20-02:54Z on 0b2854327: one of five writes landed).

use super::*;
use crate::sdk_codegen::ProjectsOutcome;

impl ProjectsOutcome for CodeRead {
    fn outcome(output: &ReadResult) -> ToolVerdict {
        ToolVerdict::of_success(output.success)
    }
}

impl ProjectsOutcome for CodeWrite {
    fn outcome(output: &WriteResult) -> ToolVerdict {
        ToolVerdict::of_success(output.success)
    }
}

impl ProjectsOutcome for CodeEdit {
    fn outcome(output: &WriteResult) -> ToolVerdict {
        ToolVerdict::of_success(output.success)
    }
}

impl ProjectsOutcome for CodeList {
    /// `success` decides: a zero-match glob lands `success: true` with a note in `error`.
    fn outcome(output: &ListResult) -> ToolVerdict {
        ToolVerdict::of_success(output.success)
    }
}

impl ProjectsOutcome for CodeTree {
    fn outcome(output: &TreeResult) -> ToolVerdict {
        ToolVerdict::of_success(output.success)
    }
}

impl ProjectsOutcome for CodeSearch {
    fn outcome(output: &SearchResult) -> ToolVerdict {
        ToolVerdict::of_success(output.success)
    }
}

crate::register_outcome!(CodeRead);
crate::register_outcome!(CodeWrite);
crate::register_outcome!(CodeEdit);
crate::register_outcome!(CodeList);
crate::register_outcome!(CodeTree);
crate::register_outcome!(CodeSearch);
