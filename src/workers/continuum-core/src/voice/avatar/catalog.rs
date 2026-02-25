//! Avatar model catalog — static entries, filesystem discovery, and manifest parsing.
//!
//! The catalog is the single source of truth for available avatar models.
//! Static AVATAR_CATALOG provides the compile-time baseline.
//! AvatarCatalog adds runtime filesystem discovery with optional .toml manifests.

use std::path::{Path, PathBuf};
use crate::{clog_info, clog_warn};
use super::backend::ModelFormat;
use super::types::*;

/// The 8 VRM 0.x avatar models in the static (compile-time) catalog.
/// All models are VRoid Studio quality (35-50k triangles, 52 morph targets, 83 joints).
/// Sources: VRoid Studio CC0 (OpenGameArt).
/// Voice-to-avatar matching uses pitch + gender + energy to select the best fit.
///
/// NOTE: VRM 1.0 models (169 joints, VRMC_vrm extension) are excluded from the catalog.
/// Bevy 0.18's glTF loader cannot correctly skin models with >128 joints — they render
/// as exploding geometry. Only VRM 0.x models (83 joints) work in the headless pipeline.
///
/// Gender distribution (static): 7 Female, 1 Male
pub const AVATAR_CATALOG: &[AvatarModel] = &[
    AvatarModel {
        id: "vroid-female-base",
        name: "Base Female",
        filename: "vroid-female-base.vrm",
        style: AvatarStyle::Anime,
        voice_profile: VoiceProfile { pitch: PitchRange::High, gender: AvatarGender::Female, energy: EnergyLevel::Moderate },
    },
    AvatarModel {
        id: "vroid-male-base",
        name: "Base Male",
        filename: "vroid-male-base.vrm",
        style: AvatarStyle::Anime,
        voice_profile: VoiceProfile { pitch: PitchRange::Low, gender: AvatarGender::Male, energy: EnergyLevel::Moderate },
    },
    AvatarModel {
        id: "vroid-sakurada",
        name: "Sakurada Fumiriya",
        filename: "vroid-sakurada.vrm",
        style: AvatarStyle::Anime,
        voice_profile: VoiceProfile { pitch: PitchRange::High, gender: AvatarGender::Female, energy: EnergyLevel::Energetic },
    },
    AvatarModel {
        id: "vroid-shino",
        name: "Sendagaya Shino",
        filename: "vroid-shino.vrm",
        style: AvatarStyle::Anime,
        voice_profile: VoiceProfile { pitch: PitchRange::High, gender: AvatarGender::Female, energy: EnergyLevel::Calm },
    },
    AvatarModel {
        id: "vroid-darkness",
        name: "Darkness",
        filename: "vroid-darkness.vrm",
        style: AvatarStyle::Anime,
        voice_profile: VoiceProfile { pitch: PitchRange::Mid, gender: AvatarGender::Female, energy: EnergyLevel::Calm },
    },
    AvatarModel {
        id: "vroid-sample-d",
        name: "Sample D",
        filename: "vroid-sample-d.vrm",
        style: AvatarStyle::Anime,
        voice_profile: VoiceProfile { pitch: PitchRange::High, gender: AvatarGender::Female, energy: EnergyLevel::Moderate },
    },
    AvatarModel {
        id: "vroid-sample-e",
        name: "Sample E",
        filename: "vroid-sample-e.vrm",
        style: AvatarStyle::Anime,
        voice_profile: VoiceProfile { pitch: PitchRange::Mid, gender: AvatarGender::Female, energy: EnergyLevel::Energetic },
    },
    AvatarModel {
        id: "vroid-sample-f",
        name: "Sample F",
        filename: "vroid-sample-f.vrm",
        style: AvatarStyle::Anime,
        voice_profile: VoiceProfile { pitch: PitchRange::Mid, gender: AvatarGender::Female, energy: EnergyLevel::Moderate },
    },
];

/// Models directory relative to the working directory (src/).
const MODELS_DIR: &str = "models/avatars";

/// Recognized model file extensions for filesystem discovery.
const MODEL_EXTENSIONS: &[&str] = &[
    "vrm", "glb", "gltf", "moc3", "svg", "png", "jpg", "jpeg", "webp",
];

/// Get the filesystem path for an avatar model.
pub fn avatar_model_path(filename: &str) -> PathBuf {
    PathBuf::from(MODELS_DIR).join(filename)
}

// =============================================================================
// TOML manifest — optional sidecar file for model metadata
// =============================================================================

/// TOML manifest for a model file. Placed alongside the model as `<name>.toml`.
///
/// Example `vroid-female-base.toml`:
/// ```toml
/// id = "vroid-female-base"
/// name = "Base Female"
/// style = "anime"
/// gender = "female"
/// pitch = "high"
/// energy = "moderate"
/// tags = ["vroid", "cc0"]
/// ```
#[derive(Debug, serde::Deserialize)]
pub struct ModelManifest {
    pub id: Option<String>,
    pub name: Option<String>,
    pub style: Option<String>,
    pub gender: Option<String>,
    pub pitch: Option<String>,
    pub energy: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl ModelManifest {
    /// Parse a manifest file. Returns None if the file doesn't exist or can't be parsed.
    pub fn load(manifest_path: &Path) -> Option<Self> {
        let content = std::fs::read_to_string(manifest_path).ok()?;
        match toml::from_str(&content) {
            Ok(manifest) => Some(manifest),
            Err(e) => {
                clog_warn!("Failed to parse manifest {}: {}", manifest_path.display(), e);
                None
            }
        }
    }

    /// Convert gender string to AvatarGender.
    fn parse_gender(&self) -> AvatarGender {
        match self.gender.as_deref().unwrap_or("female").to_lowercase().as_str() {
            "male" | "m" => AvatarGender::Male,
            _ => AvatarGender::Female,
        }
    }

    /// Convert pitch string to PitchRange.
    fn parse_pitch(&self) -> PitchRange {
        match self.pitch.as_deref().unwrap_or("mid").to_lowercase().as_str() {
            "low" | "l" => PitchRange::Low,
            "high" | "h" => PitchRange::High,
            _ => PitchRange::Mid,
        }
    }

    /// Convert energy string to EnergyLevel.
    fn parse_energy(&self) -> EnergyLevel {
        match self.energy.as_deref().unwrap_or("moderate").to_lowercase().as_str() {
            "calm" | "low" => EnergyLevel::Calm,
            "energetic" | "high" => EnergyLevel::Energetic,
            _ => EnergyLevel::Moderate,
        }
    }
}

// =============================================================================
// AvatarCatalog — dynamic runtime catalog with filesystem discovery
// =============================================================================

/// Check if a .vrm file is VRM 1.0 (has VRMC_vrm extension in GLB JSON).
/// VRM 1.0 models can't be rendered by Bevy's glTF loader (exploding geometry).
fn is_vrm1_file(path: &Path) -> bool {
    use std::io::Read;
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    // GLB header: magic(4) + version(4) + length(4)
    let mut header = [0u8; 12];
    if file.read_exact(&mut header).is_err() { return false; }
    let magic = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    if magic != 0x46546C67 { return false; } // Not GLB
    // JSON chunk header: length(4) + type(4)
    let mut chunk_header = [0u8; 8];
    if file.read_exact(&mut chunk_header).is_err() { return false; }
    let chunk_length = u32::from_le_bytes([chunk_header[0], chunk_header[1], chunk_header[2], chunk_header[3]]) as usize;
    let chunk_type = u32::from_le_bytes([chunk_header[4], chunk_header[5], chunk_header[6], chunk_header[7]]);
    if chunk_type != 0x4E4F534A { return false; } // Not JSON chunk
    let mut json_data = vec![0u8; chunk_length];
    if file.read_exact(&mut json_data).is_err() { return false; }
    let json_str = match std::str::from_utf8(&json_data) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let root: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return false,
    };
    root.get("extensions")
        .and_then(|e| e.get("VRMC_vrm"))
        .is_some()
}

/// File extension priority for deduplication (higher = preferred).
/// .glb is preferred because Bevy loads it natively without VRM-specific conversion.
fn ext_priority(ext: &str) -> u8 {
    match ext {
        "glb" => 3,
        "gltf" => 2,
        "vrm" => 1,
        _ => 0,
    }
}

/// Infer gender from model stem name using known VRoid naming conventions.
/// VRoid Sample R/T/V/X/Z are distinct characters with known genders.
fn infer_gender_from_stem(stem: &str) -> AvatarGender {
    match stem {
        "vroid-sample-r" => AvatarGender::Female,
        "vroid-sample-t" => AvatarGender::Male,
        "vroid-sample-v" => AvatarGender::Female,
        "vroid-sample-x" => AvatarGender::Male,
        "vroid-sample-z" => AvatarGender::Male,
        _ => super::gender::gender_from_identity(stem),
    }
}

/// Convert a stem like "vroid-sample-r" to a human-readable "Sample R".
fn humanize_stem(stem: &str) -> String {
    let name = stem.strip_prefix("vroid-").unwrap_or(stem);
    name.split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => format!("{}{}", c.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Dynamic catalog that combines static entries with filesystem-discovered models.
///
/// On construction, scans `models/avatars/` for model files. For each file,
/// looks for a `.toml` manifest sidecar. Falls back to inferring metadata
/// from filename and format.
#[derive(Debug)]
pub struct AvatarCatalog {
    models: Vec<DynamicAvatarModel>,
}

impl AvatarCatalog {
    /// Build a catalog from filesystem discovery.
    ///
    /// Scans `models/avatars/` for recognized model files. Deduplicates by stem name,
    /// preferring `.glb` over `.vrm` (Bevy loads glTF/GLB natively without conversion).
    ///
    /// For known models (matching static catalog by stem), inherits voice profile metadata.
    /// For unknown models, infers gender from the static catalog or defaults.
    pub fn discover() -> Self {
        let models_dir = Path::new(MODELS_DIR);

        if !models_dir.exists() {
            clog_info!("🎭 Avatar catalog: {} not found, using static catalog only", MODELS_DIR);
            return Self::from_static();
        }

        let entries = match std::fs::read_dir(models_dir) {
            Ok(entries) => entries,
            Err(e) => {
                clog_warn!("🎭 Failed to read {}: {}, using static catalog", MODELS_DIR, e);
                return Self::from_static();
            }
        };

        // Collect all candidate files, keyed by stem for deduplication.
        // When both foo.vrm and foo.glb exist, prefer .glb (Bevy-native via GltfPlugin).
        // VRM 1.0 models are filtered out below (Bevy can't render them correctly).
        use std::collections::HashMap;
        let mut candidates: HashMap<String, (PathBuf, String)> = HashMap::new(); // stem → (path, ext)

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() { continue; }

            let ext = path.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_default();

            if !MODEL_EXTENSIONS.contains(&ext.as_str()) { continue; }

            // Skip non-model formats (images, SVG) for now — those need different backends
            if !matches!(ext.as_str(), "vrm" | "glb" | "gltf") { continue; }

            let stem = path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            // Prefer .glb over .vrm (Bevy loads glTF/GLB natively)
            if let Some(existing) = candidates.get(&stem) {
                let existing_priority = ext_priority(&existing.1);
                let new_priority = ext_priority(&ext);
                if new_priority <= existing_priority {
                    continue; // existing is better or equal
                }
            }
            candidates.insert(stem, (path, ext));
        }

        // Build static catalog lookup for metadata inheritance
        let static_by_stem: HashMap<&str, &AvatarModel> = AVATAR_CATALOG.iter()
            .filter_map(|m| {
                Path::new(m.filename).file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| (s, m))
            })
            .collect();

        let mut models = Vec::new();
        for (stem, (path, _ext)) in &candidates {
            let filename = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();

            let format = match ModelFormat::from_filename(&filename) {
                Some(f) => f,
                None => continue,
            };

            // Skip VRM 1.0 models — Bevy 0.18 can't render them correctly (exploding geometry).
            // Check the .vrm file for VRMC_vrm extension marker.
            if matches!(format, ModelFormat::Vrm0x | ModelFormat::Gltf) {
                let vrm_path = path.with_extension("vrm");
                if vrm_path.exists() && is_vrm1_file(&vrm_path) {
                    clog_info!("🎭 Skipping VRM 1.0 model '{}' (Bevy can't render >128 joints)", stem);
                    continue;
                }
            }

            // Check for sidecar manifest
            let manifest_path = path.with_extension("toml");
            let manifest = ModelManifest::load(&manifest_path);

            let model = if let Some(ref m) = manifest {
                // Manifest provides full metadata
                DynamicAvatarModel {
                    id: m.id.clone().unwrap_or_else(|| stem.clone()),
                    name: m.name.clone().unwrap_or_else(|| stem.clone()),
                    filename: filename.clone(),
                    format,
                    style: m.style.as_deref().map(AvatarStyle::from_str_loose)
                        .unwrap_or(AvatarStyle::Stylized),
                    voice_profile: VoiceProfile {
                        pitch: m.parse_pitch(),
                        gender: m.parse_gender(),
                        energy: m.parse_energy(),
                    },
                    tags: m.tags.clone(),
                }
            } else if let Some(static_model) = static_by_stem.get(stem.as_str()) {
                // Known model from static catalog — inherit metadata
                DynamicAvatarModel {
                    id: static_model.id.to_string(),
                    name: static_model.name.to_string(),
                    filename: filename.clone(),
                    format,
                    style: static_model.style,
                    voice_profile: static_model.voice_profile,
                    tags: Vec::new(),
                }
            } else {
                // Unknown model — infer from stem name and assign alternating gender
                // for better distribution when no metadata is available.
                let gender = infer_gender_from_stem(stem);
                DynamicAvatarModel {
                    id: stem.clone(),
                    name: humanize_stem(stem),
                    filename: filename.clone(),
                    format,
                    style: AvatarStyle::Anime, // VRoid models are anime-style
                    voice_profile: VoiceProfile {
                        pitch: PitchRange::Mid,
                        gender,
                        energy: EnergyLevel::Moderate,
                    },
                    tags: vec!["discovered".to_string()],
                }
            };

            models.push(model);
        }

        // Sort by id for deterministic ordering
        models.sort_by(|a, b| a.id.cmp(&b.id));

        clog_info!("🎭 Avatar catalog: discovered {} models from filesystem", models.len());
        Self { models }
    }

    /// Build from the static AVATAR_CATALOG (fallback when no models/ directory).
    pub fn from_static() -> Self {
        let models = AVATAR_CATALOG.iter().map(|m| DynamicAvatarModel {
            id: m.id.to_string(),
            name: m.name.to_string(),
            filename: m.filename.to_string(),
            format: ModelFormat::from_filename(m.filename).unwrap_or(ModelFormat::Vrm0x),
            style: m.style,
            voice_profile: m.voice_profile,
            tags: Vec::new(),
        }).collect();
        Self { models }
    }

    /// All models in the catalog.
    pub fn all(&self) -> &[DynamicAvatarModel] {
        &self.models
    }

    /// Filter models by criteria.
    pub fn filter(
        &self,
        format: Option<ModelFormat>,
        style: Option<AvatarStyle>,
        gender: Option<AvatarGender>,
    ) -> Vec<&DynamicAvatarModel> {
        self.models.iter().filter(|m| {
            if let Some(f) = format { if m.format != f { return false; } }
            if let Some(s) = style { if m.style != s { return false; } }
            if let Some(g) = gender { if m.voice_profile.gender != g { return false; } }
            true
        }).collect()
    }

    /// Find a model by ID.
    pub fn by_id(&self, id: &str) -> Option<&DynamicAvatarModel> {
        self.models.iter().find(|m| m.id == id)
    }

    /// Number of models in the catalog.
    pub fn len(&self) -> usize {
        self.models.len()
    }

    /// Whether the catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_avatar_catalog_has_8_models() {
        assert_eq!(AVATAR_CATALOG.len(), 8);
    }

    #[test]
    fn test_avatar_model_path() {
        let path = avatar_model_path("vroid-female-base.vrm");
        assert_eq!(path.to_str().unwrap(), "models/avatars/vroid-female-base.vrm");
    }

    #[test]
    fn test_all_models_have_filenames() {
        for model in AVATAR_CATALOG {
            assert!(model.filename.ends_with(".vrm"), "Model {} missing .vrm extension", model.id);
            assert!(!model.id.is_empty());
            assert!(!model.name.is_empty());
        }
    }

    #[test]
    fn test_static_catalog_conversion() {
        let catalog = AvatarCatalog::from_static();
        assert_eq!(catalog.len(), 8);
        assert!(catalog.by_id("vroid-female-base").is_some());
        assert!(catalog.by_id("nonexistent").is_none());
    }

    #[test]
    fn test_catalog_filter_by_gender() {
        let catalog = AvatarCatalog::from_static();
        let males = catalog.filter(None, None, Some(AvatarGender::Male));
        assert_eq!(males.len(), 1);
        assert_eq!(males[0].id, "vroid-male-base");

        let females = catalog.filter(None, None, Some(AvatarGender::Female));
        assert_eq!(females.len(), 7);
    }

    #[test]
    fn test_catalog_filter_by_style() {
        let catalog = AvatarCatalog::from_static();
        let anime = catalog.filter(None, Some(AvatarStyle::Anime), None);
        assert_eq!(anime.len(), 8); // all are anime

        let cartoon = catalog.filter(None, Some(AvatarStyle::Cartoon), None);
        assert_eq!(cartoon.len(), 0); // none are cartoon
    }

    #[test]
    fn test_catalog_filter_by_format() {
        let catalog = AvatarCatalog::from_static();
        let vrm = catalog.filter(Some(ModelFormat::Vrm0x), None, None);
        assert_eq!(vrm.len(), 8); // all are VRM 0.x

        let live2d = catalog.filter(Some(ModelFormat::Live2D), None, None);
        assert_eq!(live2d.len(), 0);
    }

    #[test]
    fn test_catalog_combined_filter() {
        let catalog = AvatarCatalog::from_static();
        let result = catalog.filter(
            Some(ModelFormat::Vrm0x),
            Some(AvatarStyle::Anime),
            Some(AvatarGender::Female),
        );
        assert_eq!(result.len(), 7); // 7 female anime VRM models
    }

    #[test]
    fn test_manifest_parsing() {
        let toml_str = r#"
id = "test-model"
name = "Test Model"
style = "cartoon"
gender = "male"
pitch = "low"
energy = "energetic"
tags = ["test", "unit-test"]
"#;
        let manifest: ModelManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.id.as_deref(), Some("test-model"));
        assert_eq!(manifest.name.as_deref(), Some("Test Model"));
        assert_eq!(manifest.parse_gender(), AvatarGender::Male);
        assert_eq!(manifest.parse_pitch(), PitchRange::Low);
        assert_eq!(manifest.parse_energy(), EnergyLevel::Energetic);
        assert_eq!(manifest.tags.len(), 2);
    }

    #[test]
    fn test_manifest_defaults() {
        let toml_str = r#"
id = "minimal"
"#;
        let manifest: ModelManifest = toml::from_str(toml_str).unwrap();
        // Missing fields → defaults
        assert_eq!(manifest.parse_gender(), AvatarGender::Female);
        assert_eq!(manifest.parse_pitch(), PitchRange::Mid);
        assert_eq!(manifest.parse_energy(), EnergyLevel::Moderate);
        assert!(manifest.tags.is_empty());
    }

    #[test]
    fn test_avatar_style_from_str_loose() {
        assert_eq!(AvatarStyle::from_str_loose("anime"), AvatarStyle::Anime);
        assert_eq!(AvatarStyle::from_str_loose("ANIME"), AvatarStyle::Anime);
        assert_eq!(AvatarStyle::from_str_loose("cartoon"), AvatarStyle::Cartoon);
        assert_eq!(AvatarStyle::from_str_loose("pixel"), AvatarStyle::Pixel);
        assert_eq!(AvatarStyle::from_str_loose("realistic"), AvatarStyle::Realistic);
        assert_eq!(AvatarStyle::from_str_loose("minimal"), AvatarStyle::Minimalist);
        assert_eq!(AvatarStyle::from_str_loose("minimalist"), AvatarStyle::Minimalist);
        // Unknown → Stylized (safe default)
        assert_eq!(AvatarStyle::from_str_loose("unknown"), AvatarStyle::Stylized);
    }
}
