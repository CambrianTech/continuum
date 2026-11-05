//! What each native perception verb's own result says it did (ONE-ACT 6b): every native tool
//! projects its outcome (`natives_without_an_outcome` = 0). Each reports a failed
//! observation as data (`success: false`), which must read as a failure.

use super::hot_edit::{HotEditCommand, HotEditResult};
use super::interact::{InteractCommand, InteractResult};
use super::look::{LookCommand, LookResult};
use super::{ObserveCommand, ObserveResult};
use crate::sdk_codegen::{ProjectsOutcome, ToolVerdict};

impl ProjectsOutcome for ObserveCommand {
    fn outcome(output: &ObserveResult) -> ToolVerdict {
        ToolVerdict::of_success(output.success)
    }
}

impl ProjectsOutcome for LookCommand {
    fn outcome(output: &LookResult) -> ToolVerdict {
        ToolVerdict::of_success(output.success)
    }
}

/// An edit's result is the observation after it: the edit landed when that observation did.
impl ProjectsOutcome for HotEditCommand {
    fn outcome(output: &HotEditResult) -> ToolVerdict {
        ToolVerdict::of_success(output.observation.success)
    }
}

impl ProjectsOutcome for InteractCommand {
    fn outcome(output: &InteractResult) -> ToolVerdict {
        ToolVerdict::of_success(output.observation.success)
    }
}

crate::register_outcome!(ObserveCommand);
crate::register_outcome!(LookCommand);
crate::register_outcome!(HotEditCommand);
crate::register_outcome!(InteractCommand);
