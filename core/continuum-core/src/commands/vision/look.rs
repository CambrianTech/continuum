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
    /// An image file (png/jpg/gif/webp/bmp), as for code/read. Or give url.
    #[serde(default)]
    #[ts(optional)]
    pub file_path: Option<String>,
    /// An http(s) page, e.g. your dev server at http://localhost:5173.
    #[serde(default)]
    #[ts(optional)]
    pub url: Option<String>,
    /// Optional: what to focus on ("count the shapes", "read the chart title").
    /// Omit for a general description.
    #[serde(default)]
    #[ts(optional)]
    pub focus: Option<String>,
}

/// What to look at: exactly one of a file or a page. A page is http(s) only: the renderer
/// runs as the core, and a `file://` url would let a look reach files outside her workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
enum LookAt {
    File(String),
    Page(String),
}

/// A page the renderer may load: http(s) on this machine only. The renderer runs as the
/// core, so any other host (an internal endpoint, cloud metadata at 169.254.169.254) would
/// be reached with the core's network position (Cormac on #4539).
fn is_loopback_page(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://").or_else(|| url.strip_prefix("https://")) else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default(); // unwrap_or_default: split always yields a first piece
    // userinfo ("user@host") is refused outright: it is how a host is disguised
    if authority.contains('@') {
        return false;
    }
    let host = match authority.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or_default(), // unwrap_or_default: as above
        None => authority.split(':').next().unwrap_or_default(), // unwrap_or_default: as above
    };
    matches!(host.to_ascii_lowercase().as_str(), "localhost" | "127.0.0.1" | "::1")
}

impl TryFrom<&VisionLookParams> for LookAt {
    type Error = CommandError;

    fn try_from(p: &VisionLookParams) -> Result<Self, Self::Error> {
        match (p.file_path.as_deref().map(str::trim), p.url.as_deref().map(str::trim)) {
            (Some(file), None) if !file.is_empty() => Ok(LookAt::File(file.to_string())),
            (None, Some(url)) if is_loopback_page(url) => Ok(LookAt::Page(url.to_string())),
            (None, Some(url)) => Err(CommandError::Invalid(format!(
                "vision/look: url '{url}' must be an http(s) page on this machine (localhost, 127.0.0.1 or [::1], e.g. your dev server); for a file use file_path, for a public page web/fetch"
            ))),
            (Some(_), Some(_)) => Err(CommandError::Invalid("vision/look: give file_path OR url, not both".into())),
            (None, None) | (Some(_), None) => Err(CommandError::Invalid(
                "vision/look: give file_path (an image in your workspace) or url (an http(s) page)".into(),
            )),
        }
    }
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
    run(this, ctx, p) => {
        // A page is rendered first (the SAME capture a builder uses, scoped to her), then
        // looked at exactly like a file: one sight, whatever she is looking at.
        let file_path = match LookAt::try_from(&p)? {
            LookAt::File(file) => file,
            LookAt::Page(url) => {
                let shot = crate::commands::interface::capture::Capture
                    .run(ctx, crate::commands::interface::capture::CaptureParams {
                        target: "web".into(),
                        url: Some(url),
                        ..Default::default()
                    })
                    .await?;
                shot.path
            }
        };
        let path = std::path::Path::new(&file_path);
        let Some(mime) = mime_for(path) else {
            return Err(CommandError::Invalid(format!(
                "vision/look: '{file_path}' does not look like an image file \
                 (png/jpg/gif/webp/bmp)"
            )));
        };
        let meta = std::fs::metadata(path).map_err(|e| {
            CommandError::Invalid(format!("vision/look: cannot read '{file_path}': {e}"))
        })?;
        if meta.len() > MAX_IMAGE_BYTES {
            return Err(CommandError::Invalid(format!(
                "vision/look: '{}' is {} bytes — larger than the {}MB cap",
                file_path,
                meta.len(),
                MAX_IMAGE_BYTES / (1024 * 1024)
            )));
        }
        let bytes = std::fs::read(path).map_err(|e| {
            CommandError::Invalid(format!("vision/look: cannot read '{file_path}': {e}"))
        })?;
        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes); // boundary: vision model API takes base64 image payloads on the wire

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

    // what this catches: a look at a PAGE that could reach host files (file:// runs as the
    // core, outside her workspace) or another host from the core's network position (cloud
    // metadata, an internal endpoint, a userinfo-disguised host), or an ambiguous call
    // silently picking one target. Exactly one of file_path or a loopback http(s) url.
    #[test]
    fn a_look_is_one_file_or_one_http_page() {
        let at = |file: Option<&str>, url: Option<&str>| {
            LookAt::try_from(&VisionLookParams { file_path: file.map(Into::into), url: url.map(Into::into), focus: None })
        };
        assert_eq!(at(Some("shot.png"), None).ok(), Some(LookAt::File("shot.png".into())));
        assert_eq!(at(None, Some("http://localhost:5173")).ok(), Some(LookAt::Page("http://localhost:5173".into())));
        assert!(at(None, Some("file:///Users/x/.ssh/id_rsa")).is_err(), "a file url never reaches the renderer");
        assert!(at(None, Some("http://[::1]:5173/app")).is_ok());
        for outside in [
            "http://169.254.169.254/latest/meta-data",
            "https://example.com",
            "http://localhost@evil.example/",
            "http://10.0.0.5:8080",
        ] {
            assert!(at(None, Some(outside)).is_err(), "{outside} is not this machine");
        }
        assert!(at(Some("a.png"), Some("http://x")).is_err(), "both is ambiguous");
        assert!(at(None, None).is_err());
    }
}
