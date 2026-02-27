//! Avatar type definitions — model metadata, style categories, voice profiles.

use super::backend::ModelFormat;
use ts_rs::TS;

/// Available avatar model catalog entry.
///
/// Static entries use `&'static str` fields (compile-time catalog).
/// Dynamic entries from filesystem discovery use `DynamicAvatarModel` instead.
#[derive(Debug, Clone)]
pub struct AvatarModel {
    /// Unique identifier for this model
    pub id: &'static str,
    /// Human-readable name
    pub name: &'static str,
    /// Filename in models/avatars/ directory
    pub filename: &'static str,
    /// Style category
    pub style: AvatarStyle,
    /// Voice characteristics this model matches best
    pub voice_profile: VoiceProfile,
}

/// Dynamic avatar model — discovered at runtime from filesystem + manifest.
/// Owns its strings (unlike AvatarModel which uses `&'static str`).
#[derive(Debug, Clone)]
pub struct DynamicAvatarModel {
    /// Unique identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Filename (relative to models/avatars/)
    pub filename: String,
    /// Detected or declared model format
    pub format: ModelFormat,
    /// Style category
    pub style: AvatarStyle,
    /// Voice profile for matching
    pub voice_profile: VoiceProfile,
    /// Freeform tags for filtering (e.g., "fantasy", "sci-fi", "casual")
    pub tags: Vec<String>,
}

/// Avatar art style categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/voice/AvatarStyle.ts")]
#[serde(rename_all = "snake_case")]
pub enum AvatarStyle {
    /// Anime VRoid-style (high detail, full blend shapes, 35-50k triangles)
    Anime,
    /// Stylized 3D (100Avatars / Polygonal Mind — clean, geometric)
    Stylized,
    /// Western cartoon style (exaggerated proportions, bright colors)
    Cartoon,
    /// Pixel art (retro, low-res sprites)
    Pixel,
    /// Photorealistic or near-realistic 3D
    Realistic,
    /// Simple/abstract (geometric shapes, minimal detail)
    Minimalist,
}

impl AvatarStyle {
    /// Parse from string (for TOML manifest parsing).
    pub fn from_str_loose(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "anime" => Self::Anime,
            "stylized" => Self::Stylized,
            "cartoon" => Self::Cartoon,
            "pixel" => Self::Pixel,
            "realistic" => Self::Realistic,
            "minimalist" | "minimal" => Self::Minimalist,
            _ => Self::Stylized, // safe default for unknown styles
        }
    }
}

/// Voice characteristics for matching TTS voice → avatar model.
#[derive(Debug, Clone, Copy)]
pub struct VoiceProfile {
    /// Expected pitch range: Low (<145Hz), Mid (145-190Hz), High (>190Hz)
    pub pitch: PitchRange,
    /// Perceived gender for avatar selection
    pub gender: AvatarGender,
    /// Perceived energy level
    pub energy: EnergyLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PitchRange { Low, Mid, High }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/voice/AvatarGender.ts")]
#[serde(rename_all = "snake_case")]
pub enum AvatarGender { Male, Female }

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnergyLevel { Calm, Moderate, Energetic }

/// Catalog entry for the browser widget — serializable subset of DynamicAvatarModel.
/// Sent to the browser so users can see available models and make selections.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/voice/AvatarCatalogEntry.ts")]
pub struct AvatarCatalogEntry {
    /// Unique model identifier
    pub id: String,
    /// Human-readable display name
    pub name: String,
    /// Model format (vrm, gltf, live2d, etc.)
    pub format: ModelFormat,
    /// Art style category
    pub style: AvatarStyle,
    /// Perceived gender
    pub gender: AvatarGender,
    /// Freeform tags for filtering
    #[serde(default)]
    pub tags: Vec<String>,
}

impl From<&DynamicAvatarModel> for AvatarCatalogEntry {
    fn from(m: &DynamicAvatarModel) -> Self {
        Self {
            id: m.id.clone(),
            name: m.name.clone(),
            format: m.format,
            style: m.style,
            gender: m.voice_profile.gender,
            tags: m.tags.clone(),
        }
    }
}

/// User preference for avatar selection (sent from browser widget).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/voice/AvatarPreference.ts")]
pub struct AvatarPreference {
    /// Preferred art style (None = any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<AvatarStyle>,
    /// Preferred specific model ID (highest priority if set)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    /// Exclude these tags
    #[serde(default)]
    pub exclude_tags: Vec<String>,
}
