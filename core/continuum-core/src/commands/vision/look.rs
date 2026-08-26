//! `vision/look` — a citizen LOOKS at an image file with her own eyes.
//!
//! The sensory architecture promises every persona sight regardless of base
//! model, and rooms deliver it for chat attachments — but a citizen working in a
//! WORKSPACE had no way to look at an image file at all: `cognition/vision-describe`
//! is `Internal` (host-invoked), so a task like "open look.png and tell me what
//! the chart says" was structurally impossible. This verb closes that gap as an
//! `AiSafe` toolbelt act: read the file, run it through the SAME description
//! bridge live rooms use (the vision sidecar / best available vision model), and
//! return what her eyes report. It is also what makes an input-side VISION
//! benchmark honest — the gym measures see-then-answer through the real bridge,
//! never a harness translating on her behalf (`vision-qa`, plan follow-on to the
//! 2026-08-26 sight restoration).

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::cognition::vision_describe::{
    describe_image, VisionDescribeOptions, VisionDescribeRequest,
};
use crate::runtime::{CommandExecutor, LateBound};
use crate::sdk_codegen::CommandError;

/// Refuse anything that is not plausibly an image, BEFORE spending a vision
/// generate on it. Extension-keyed: honest and cheap; a mislabeled file comes
/// back as a garbage description, which the citizen can see and say.
fn mime_for(path: &std::path::Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => Some("image/png"),
        Some("jpg" | "jpeg") => Some("image/jpeg"),
        Some("gif") => Some("image/gif"),
        Some("webp") => Some("image/webp"),
        Some("bmp") => Some("image/bmp"),
        _ => None,
    }
}

/// Images larger than this are refused rather than shipped to the describer —
/// a screenshot is hundreds of KB; tens of MB is a mistake, not a picture.
const MAX_IMAGE_BYTES: u64 = 12 * 1024 * 1024;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/vision/VisionLookParams.ts")]
pub struct VisionLookParams {
    /// Path to the image file to look at (png/jpg/gif/webp/bmp), as you would
    /// pass it to code/read.
    pub file_path: String,
    /// Optional: what to focus on ("count the shapes", "read the chart title").
    /// Omit for a general description.
    #[serde(default)]
    #[ts(optional)]
    pub focus: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/vision/VisionLookResult.ts")]
pub struct VisionLookResult {
    /// What your eyes report about the image.
    pub description: String,
    /// Which vision model looked (attribution for the receipt).
    pub model: String,
}

crate::action_command! {
    /// LOOK at an image file with your eyes and get a description of what it
    /// shows. Use for any image in your workspace — screenshots, charts,
    /// diagrams, photos. Pass `focus` to direct your attention (e.g. "count the
    /// red shapes"). This runs the image through your own vision system.
    pub struct VisionLook { executor_slot: Arc<LateBound<CommandExecutor>> }
    name: "vision/look",
    access: AiSafe,
    native: true,
    params: VisionLookParams,
    output: VisionLookResult,
    run(this, _ctx, p) => {
        let path = std::path::Path::new(&p.file_path);
        let Some(mime) = mime_for(path) else {
            return Err(CommandError::Invalid(format!(
                "vision/look: '{}' does not look like an image file \
                 (png/jpg/gif/webp/bmp)",
                p.file_path
            )));
        };
        let meta = std::fs::metadata(path).map_err(|e| {
            CommandError::Invalid(format!("vision/look: cannot read '{}': {e}", p.file_path))
        })?;
        if meta.len() > MAX_IMAGE_BYTES {
            return Err(CommandError::Invalid(format!(
                "vision/look: '{}' is {} bytes — larger than the {}MB cap",
                p.file_path,
                meta.len(),
                MAX_IMAGE_BYTES / (1024 * 1024)
            )));
        }
        let bytes = std::fs::read(path).map_err(|e| {
            CommandError::Invalid(format!("vision/look: cannot read '{}': {e}", p.file_path))
        })?;
        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

        let executor = this
            .executor_slot
            .require()
            .map_err(CommandError::Internal)?;
        let req = VisionDescribeRequest {
            base64_data: b64,
            mime_type: mime.to_string(),
            options: VisionDescribeOptions {
                prompt: p.focus.map(|f| {
                    format!(
                        "Describe this image accurately and concretely. Pay particular \
                         attention to: {f}. State counts, colors and shapes exactly as \
                         they appear."
                    )
                }),
                ..Default::default()
            },
        };
        let described = describe_image(req, executor)
            .await
            .map_err(CommandError::Internal)?;
        let Some(d) = described else {
            // No vision model available is an infra ABSENCE — say so loudly
            // rather than returning an empty "description" she might act on.
            return Err(CommandError::Internal(
                "vision/look: no vision-capable model is available right now — \
                 your eyes are offline; retry after serving settles"
                    .into(),
            ));
        };
        Ok(VisionLookResult {
            description: d.description,
            model: d.model_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: non-image paths refused typed BEFORE a vision generate
    // is spent, and the mime map covering the formats the describers accept.
    #[test]
    fn non_images_are_refused_and_mimes_map() {
        assert!(mime_for(std::path::Path::new("a/chart.png")).is_some());
        assert!(mime_for(std::path::Path::new("shot.JPG")).is_some());
        assert!(mime_for(std::path::Path::new("notes.txt")).is_none());
        assert!(mime_for(std::path::Path::new("Makefile")).is_none());
    }
}
