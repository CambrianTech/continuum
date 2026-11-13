//! `cognition/vision-describe` — describe an image via a vision-capable model (typed, dep-holding).
//!
//! The sensory bridge that lets a non-vision-native persona "see": selects a vision-capable
//! model from the registry, builds the describe prompt, dispatches a multimodal `ai/generate`,
//! and returns the parsed description. Migrated from `system/vision/VisionInferenceProvider.ts`
//! (continuum#1276) — the TS file becomes a thin shim over this IPC.
//!
//! Capture shape: unlike the state-holding cognition commands, this one holds the module's
//! shared late-bound [`CommandExecutor`](crate::runtime::CommandExecutor) slot (an
//! `Arc<LateBound<CommandExecutor>>`), because `describe_image` re-enters the command bus to
//! run `ai/generate`. Same pattern as the `chat/*` family
//! ([`crate::commands::chat`]). The slot is installed by
//! [`CognitionModule::install_executor`](crate::modules::cognition::CognitionModule); calling
//! before install fails loud (`CommandError::Internal` with the slot's diagnostic name)
//! rather than dispatching into a null executor.
//!
//! Output note: `Option<VisionDescription>` — `None` means no vision-capable model is
//! registered on this deploy (the caller's vision pipeline interprets the null and bridges via
//! its own fallback path). This is the free fn's existing contract, preserved verbatim; a
//! provider/inference failure inside `describe_image` is an `Err`, surfaced here as
//! `CommandError::Internal`, NOT collapsed into `None`.
//!
//! `access: Internal` — host-driven sensory IPC (the media-preprocess path calls it), not a
//! persona toolbelt verb.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::cognition::vision_describe::{describe_image, VisionDescribeRequest, VisionDescription};
use crate::runtime::{CommandExecutor, LateBound};
use crate::sdk_codegen::CommandError;

/// Result of `cognition/vision-describe`: the parsed description, or `None`.
///
/// A NAMED wrapper around `Option<VisionDescription>` — the command-schema
/// validator ([`crate::sdk_codegen`]) rejects a bare `Option<T>` output because
/// an inline `T | null` has no named TS type to `export_to`, and one such command
/// panics the whole `command_registry()` walk. `description == None` preserves the
/// free fn's contract: no vision-capable model is registered on this deploy (the
/// caller's vision pipeline bridges via its own fallback); a provider/inference
/// failure is still an `Err`, never collapsed into `None`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/VisionDescribeResult.ts"
)]
pub struct VisionDescribeResult {
    pub description: Option<VisionDescription>,
}

crate::action_command! {
    /// Describe an image with a vision-capable model. Given base64 image bytes + MIME type
    /// and optional knobs (preferred model/provider, max length, object/color/text detection,
    /// prompt override), selects a vision model, runs a multimodal generate, and returns the
    /// description (or null when no vision model is available). Host-invoked sensory bridge;
    /// not a persona toolbelt verb.
    pub struct VisionDescribe { executor_slot: Arc<LateBound<CommandExecutor>> }
    name: "cognition/vision-describe",
    access: Internal,
    params: VisionDescribeRequest,
    output: VisionDescribeResult,
    run(this, _ctx, req) => {
        // describe_image re-enters the bus to run ai/generate, so it needs the executor.
        // Fail loud if the module hasn't installed it yet (diagnostic name from LateBound).
        let executor = this
            .executor_slot
            .require()
            .map_err(CommandError::Internal)?;

        let description = describe_image(req, executor)
            .await
            .map_err(CommandError::Internal)?;
        Ok(VisionDescribeResult { description })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. vision-describe is the host-driven
    // sensory bridge IPC, so it is Internal — registered and grid-routable, never a
    // remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(VisionDescribe::NAME, "cognition/vision-describe");
        assert_eq!(VisionDescribe::ACCESS, AccessLevel::Internal);
    }
}
