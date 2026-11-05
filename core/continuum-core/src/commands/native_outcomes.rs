//! What each of core's native tools says its result means (ONE-ACT 6b): every native tool
//! projects its outcome (`natives_without_an_outcome` = 0, pinned in
//! `test_env::registry_contract`).

use super::catalog::CommandsList;
use super::help::CommandsHelp;
use super::vision::look::{VisionLook, VisionLookResult};
use super::web::{fetch::WebFetch, search::WebSearch, WebFetchResult, WebSearchResult};
use crate::interface::{ScreenshotCommand, ScreenshotResult};
use crate::sdk_codegen::{ProjectsOutcome, ToolVerdict};

/// A capture that failed reports it as data (`success: false`).
impl ProjectsOutcome for ScreenshotCommand {
    fn outcome(output: &ScreenshotResult) -> ToolVerdict {
        ToolVerdict::of_success(output.success)
    }
}

// Declared `native: true` through `action_command!` (which submits each projection); their
// failures are all refusals (`Err`), so a result each returns is the work done.
continuum_kernel::projects_success_on_return! {
    WebFetch => WebFetchResult,
    WebSearch => WebSearchResult,
    VisionLook => VisionLookResult,
}

crate::register_outcome!(ScreenshotCommand);
crate::register_outcome!(CommandsList, refusals_only);
crate::register_outcome!(CommandsHelp, refusals_only);
