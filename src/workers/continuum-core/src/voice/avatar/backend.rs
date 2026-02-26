//! RenderBackend trait — factory interface for format-specific avatar renderers.
//!
//! Follows the TTS adapter convention: name/description/initialize/create.
//! Each backend handles one or more model formats and produces AvatarRenderer instances.

use super::frame::AvatarConfig;
use super::frame_analysis::{self, FrameAnalysis};
use super::renderer::AvatarRenderer;
use super::types::AvatarModel;
use thiserror::Error;
use ts_rs::TS;

/// Avatar rendering errors.
#[derive(Error, Debug)]
pub enum AvatarError {
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Backend not initialized: {0}")]
    NotInitialized(String),

    #[error("Render failed: {0}")]
    RenderFailed(String),

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Model format — determines which backend handles the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/voice/ModelFormat.ts")]
#[serde(rename_all = "snake_case")]
pub enum ModelFormat {
    /// VRM 0.x (VRoid Studio, 52 morph targets, 83 joints)
    Vrm0x,
    /// VRM 1.0 (newer spec, 169 joints — currently unsupported by Bevy skinning)
    Vrm1,
    /// Generic glTF 2.0 / GLB
    Gltf,
    /// Live2D Cubism (.moc3 or sprite-sheet compositing)
    #[serde(rename = "live2d")]
    Live2D,
    /// Pre-rendered sprite sheet (grid of animation frames)
    SpriteSheet,
    /// SVG vector avatar
    Svg,
    /// Static image (PNG/JPG/WebP fallback)
    StaticImage,
}

impl ModelFormat {
    /// Infer format from filename extension.
    pub fn from_filename(path: &str) -> Option<Self> {
        let lower = path.to_lowercase();
        if lower.ends_with(".vrm") {
            // VRM version detection would need file parsing;
            // default to 0.x since that's what our catalog uses.
            Some(Self::Vrm0x)
        } else if lower.ends_with(".glb") || lower.ends_with(".gltf") {
            Some(Self::Gltf)
        } else if lower.ends_with(".moc3") {
            Some(Self::Live2D)
        } else if lower.ends_with(".svg") {
            Some(Self::Svg)
        } else if lower.ends_with(".png") || lower.ends_with(".jpg")
            || lower.ends_with(".jpeg") || lower.ends_with(".webp") {
            Some(Self::StaticImage)
        } else {
            None
        }
    }
}

/// Render backend — factory for format-specific renderers.
///
/// Follows the TTS adapter convention: name/description/initialize.
/// Each backend supports one or more `ModelFormat` values and creates
/// `AvatarRenderer` instances for specific models.
pub trait RenderBackend: Send + Sync {
    /// Backend name (e.g., "procedural", "bevy_3d", "live2d")
    fn name(&self) -> &'static str;

    /// Human-readable description
    fn description(&self) -> &'static str;

    /// Model formats this backend can render.
    fn supported_formats(&self) -> &[ModelFormat];

    /// Whether the backend is ready to create renderers.
    fn is_initialized(&self) -> bool;

    /// Initialize the backend (load GPU resources, etc.)
    fn initialize(&mut self) -> Result<(), AvatarError>;

    /// Create a renderer for a specific model + config.
    fn create_renderer(
        &self,
        model: &AvatarModel,
        config: &AvatarConfig,
    ) -> Result<Box<dyn AvatarRenderer>, AvatarError>;

    /// Health check: create a renderer, render a test frame, analyze it.
    /// Returns the frame analysis with a health verdict.
    /// Default implementation renders 1 frame and runs frame_analysis::analyze().
    fn health_check(
        &self,
        model: &AvatarModel,
        config: &AvatarConfig,
    ) -> Result<FrameAnalysis, AvatarError> {
        let mut renderer = self.create_renderer(model, config)?;
        let frame = renderer.render_frame();
        Ok(frame_analysis::analyze(&frame))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_format_from_filename() {
        assert_eq!(ModelFormat::from_filename("vroid-female.vrm"), Some(ModelFormat::Vrm0x));
        assert_eq!(ModelFormat::from_filename("scene.glb"), Some(ModelFormat::Gltf));
        assert_eq!(ModelFormat::from_filename("scene.gltf"), Some(ModelFormat::Gltf));
        assert_eq!(ModelFormat::from_filename("model.moc3"), Some(ModelFormat::Live2D));
        assert_eq!(ModelFormat::from_filename("icon.svg"), Some(ModelFormat::Svg));
        assert_eq!(ModelFormat::from_filename("avatar.png"), Some(ModelFormat::StaticImage));
        assert_eq!(ModelFormat::from_filename("avatar.jpg"), Some(ModelFormat::StaticImage));
        assert_eq!(ModelFormat::from_filename("avatar.webp"), Some(ModelFormat::StaticImage));
        assert_eq!(ModelFormat::from_filename("unknown.xyz"), None);
    }

    #[test]
    fn test_model_format_case_insensitive() {
        assert_eq!(ModelFormat::from_filename("Model.VRM"), Some(ModelFormat::Vrm0x));
        assert_eq!(ModelFormat::from_filename("SCENE.GLB"), Some(ModelFormat::Gltf));
        assert_eq!(ModelFormat::from_filename("Avatar.PNG"), Some(ModelFormat::StaticImage));
    }
}
