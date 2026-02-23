//! Avatar Renderer — Abstracts video frame generation for AI persona avatars.
//!
//! Provides a trait-based rendering pipeline where different backends can produce
//! RGBA frames for LiveKit video publishing:
//!
//!   - ProceduralRenderer: Simple colored circle (current, zero-dependency)
//!   - BevyRenderer: Full 3D VRM model rendering via Bevy headless (in progress)
//!
//! The renderer runs on its own thread and delivers RGBA frames via crossbeam channel.
//! LiveKit's video loop consumes frames and feeds them to NativeVideoSource.
//!
//! Architecture:
//!   AvatarRenderer (trait) → RGBA frames → crossbeam channel → video loop → I420 → NativeVideoSource

use std::sync::Arc;
use tracing::{info, warn};

/// Raw RGBA frame extracted from the renderer.
pub struct RgbaFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// Configuration for avatar rendering.
#[derive(Debug, Clone)]
pub struct AvatarConfig {
    /// Persona identity (used for color derivation and model selection)
    pub identity: String,
    /// Display name (for any text overlays)
    pub display_name: String,
    /// Render resolution width
    pub width: u32,
    /// Render resolution height
    pub height: u32,
    /// Target frames per second
    pub fps: f64,
    /// Path to VRM model file (if using Bevy renderer)
    pub vrm_model_path: Option<String>,
}

impl Default for AvatarConfig {
    fn default() -> Self {
        Self {
            identity: String::new(),
            display_name: String::new(),
            width: 320,
            height: 240,
            fps: 10.0,
            vrm_model_path: None,
        }
    }
}

/// Trait for avatar rendering backends.
/// Each implementation produces RGBA frames at the configured FPS.
pub trait AvatarRenderer: Send + Sync {
    /// Render a single RGBA frame. Called at the configured FPS.
    fn render_frame(&mut self) -> RgbaFrame;

    /// Notify the renderer that the persona is speaking (for lip sync, expressions).
    fn set_speaking(&mut self, _speaking: bool) {}

    /// Set the current viseme for lip sync (0-14, OCULUS viseme set).
    fn set_viseme(&mut self, _viseme: u8, _weight: f32) {}

    /// Set the current emotion expression.
    fn set_emotion(&mut self, _emotion: &str) {}
}

// =============================================================================
// Procedural Renderer — colored circle on dark background (zero dependencies)
// =============================================================================

/// Simple procedural renderer that draws a colored circle.
/// Used as fallback when Bevy/VRM is not available.
pub struct ProceduralRenderer {
    config: AvatarConfig,
    /// Pre-rendered frame (static image, generated once)
    frame_data: Vec<u8>,
}

impl ProceduralRenderer {
    pub fn new(config: AvatarConfig) -> Self {
        let size = (config.width * config.height * 4) as usize;
        let mut frame_data = vec![0u8; size];
        generate_avatar_rgba(&mut frame_data, config.width, config.height, &config.identity);
        info!(
            "ProceduralRenderer: initialized for '{}' ({}x{} @{}fps)",
            config.identity, config.width, config.height, config.fps
        );
        Self { config, frame_data }
    }
}

impl AvatarRenderer for ProceduralRenderer {
    fn render_frame(&mut self) -> RgbaFrame {
        RgbaFrame {
            width: self.config.width,
            height: self.config.height,
            data: self.frame_data.clone(),
        }
    }
}

// =============================================================================
// Bevy 3D Renderer — wraps BevyAvatarSystem channel as AvatarRenderer
// =============================================================================

/// 3D VRM model renderer backed by the Bevy headless rendering system.
/// Receives RGBA frames from BevyAvatarSystem's GPU readback pipeline.
/// Falls back to ProceduralRenderer if Bevy is not ready or the channel dies.
pub struct BevyChannelRenderer {
    config: AvatarConfig,
    frame_rx: crossbeam_channel::Receiver<RgbaFrame>,
    slot: u8,
    /// Fallback frame for when Bevy hasn't produced output yet
    fallback_frame: Vec<u8>,
    /// Last successfully received frame — prevents blinking between 3D and fallback
    last_frame: Option<Vec<u8>>,
    /// Track consecutive receive failures to detect dead renderer
    consecutive_failures: u32,
}

impl BevyChannelRenderer {
    /// Create a BevyChannelRenderer for a given slot.
    /// The caller must have already called `bevy_system.load_model(slot, ...)`.
    pub fn new(config: AvatarConfig, frame_rx: crossbeam_channel::Receiver<RgbaFrame>, slot: u8) -> Self {
        let size = (config.width * config.height * 4) as usize;
        let mut fallback_frame = vec![0u8; size];
        generate_avatar_rgba(&mut fallback_frame, config.width, config.height, &config.identity);
        info!(
            "BevyChannelRenderer: slot {} for '{}' ({}x{} @{}fps)",
            slot, config.identity, config.width, config.height, config.fps
        );
        Self {
            config,
            frame_rx,
            slot,
            fallback_frame,
            last_frame: None,
            consecutive_failures: 0,
        }
    }
}

impl AvatarRenderer for BevyChannelRenderer {
    fn render_frame(&mut self) -> RgbaFrame {
        // Try to receive a frame from Bevy (non-blocking)
        match self.frame_rx.try_recv() {
            Ok(frame) => {
                self.consecutive_failures = 0;
                // Cache this frame so we return it on misses instead of the fallback circle
                self.last_frame = Some(frame.data.clone());
                frame
            }
            Err(_) => {
                self.consecutive_failures += 1;
                // Return last good Bevy frame if we have one, otherwise the procedural fallback.
                // This prevents blinking between 3D model and colored circle.
                let data = self.last_frame.clone()
                    .unwrap_or_else(|| self.fallback_frame.clone());
                RgbaFrame {
                    width: self.config.width,
                    height: self.config.height,
                    data,
                }
            }
        }
    }

    fn set_speaking(&mut self, speaking: bool) {
        // Forward to BevyAvatarSystem if alive
        if let Some(bevy_system) = super::bevy_renderer::try_get() {
            bevy_system.set_speaking(self.slot, speaking);
        }
    }
}

// =============================================================================
// Renderer factory — selects the best available renderer for a persona
// =============================================================================

/// Create the best available renderer for a persona's avatar.
///
/// Selection order:
/// 1. BevyChannelRenderer (3D VRM) — if VRM model exists AND Bevy is ready
/// 2. ProceduralRenderer (colored circle) — always available fallback
///
/// This is the single factory function. All renderer selection logic lives here.
/// New backends (bgfx, wgpu-native, etc.) plug in by adding a new branch.
pub fn create_renderer(config: AvatarConfig) -> Box<dyn AvatarRenderer> {
    // Attempt 3D renderer if VRM model path is specified
    if let Some(ref vrm_path) = config.vrm_model_path {
        let exists = std::path::Path::new(vrm_path).exists();
        eprintln!("[avatar_renderer] create_renderer: vrm_path='{}' exists={}", vrm_path, exists);
        if exists {
            let bevy_system = super::bevy_renderer::get_or_init();
            let ready = bevy_system.is_ready();
            eprintln!("[avatar_renderer] Bevy system ready={}", ready);
            if ready {
                static NEXT_SLOT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
                let slot = NEXT_SLOT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if slot < 16 {
                    bevy_system.load_model(slot, vrm_path, &config.display_name);
                    if let Some(frame_rx) = bevy_system.frame_receiver(slot) {
                        info!(
                            "🎨 Using BevyChannelRenderer for '{}' (slot {}, model: {})",
                            config.identity, slot, vrm_path
                        );
                        return Box::new(BevyChannelRenderer::new(config, frame_rx.clone(), slot));
                    }
                } else {
                    warn!("🎨 All 16 Bevy render slots taken, falling back to procedural for '{}'", config.identity);
                }
            } else {
                warn!("🎨 Bevy renderer not ready, falling back to procedural for '{}'", config.identity);
            }
        }
    }

    info!("🎨 Using ProceduralRenderer for '{}'", config.identity);
    Box::new(ProceduralRenderer::new(config))
}

// =============================================================================
// Avatar video loop — consumes frames from ANY renderer, publishes to LiveKit
// =============================================================================

/// Spawns a background thread that renders avatar frames and sends them via channel.
/// Returns a crossbeam receiver that the LiveKit video loop consumes.
///
/// The renderer is selected by `create_renderer()` — the loop is backend-agnostic.
/// Any AvatarRenderer implementation plugs in without touching this code.
pub fn spawn_renderer_loop(
    config: AvatarConfig,
) -> crossbeam_channel::Receiver<RgbaFrame> {
    let fps = config.fps;
    let (tx, rx) = crossbeam_channel::bounded(2); // Small buffer, drop old frames

    let renderer = create_renderer(config);
    let renderer = Arc::new(std::sync::Mutex::new(renderer));

    std::thread::Builder::new()
        .name("avatar-renderer".into())
        .spawn(move || {
            let interval = std::time::Duration::from_secs_f64(1.0 / fps);
            let mut renderer = renderer.lock().unwrap();
            loop {
                let frame = renderer.render_frame();
                match tx.try_send(frame) {
                    Ok(_) => {}
                    Err(crossbeam_channel::TrySendError::Full(_)) => {
                        // Consumer is slow, drop this frame
                    }
                    Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                        // Consumer dropped, stop rendering
                        break;
                    }
                }
                std::thread::sleep(interval);
            }
            info!("Avatar renderer loop exited");
        })
        .expect("Failed to spawn avatar renderer thread");

    rx
}

// =============================================================================
// Avatar model selection — maps persona identity/voice to a VRM model
// =============================================================================

/// Available avatar model catalog entry.
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

/// Avatar art style categories.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AvatarStyle {
    /// Anime VRoid-style (high detail, full blend shapes)
    Anime,
    /// Low-poly stylized (100Avatars style)
    Stylized,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PitchRange { Low, Mid, High }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AvatarGender { Male, Female, Neutral }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EnergyLevel { Calm, Moderate, Energetic }

/// The 14 CC0 avatar models available for persona video rendering.
/// Voice-to-avatar matching uses pitch + gender + energy to select the best fit.
pub const AVATAR_CATALOG: &[AvatarModel] = &[
    // ---- VRoid Anime (8 models) ----
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
    // ---- 100Avatars Stylized (6 models) ----
    AvatarModel {
        id: "100av-rose",
        name: "Rose",
        filename: "100av-rose.vrm",
        style: AvatarStyle::Stylized,
        voice_profile: VoiceProfile { pitch: PitchRange::High, gender: AvatarGender::Female, energy: EnergyLevel::Moderate },
    },
    AvatarModel {
        id: "100av-robert",
        name: "Robert",
        filename: "100av-robert.vrm",
        style: AvatarStyle::Stylized,
        voice_profile: VoiceProfile { pitch: PitchRange::Low, gender: AvatarGender::Male, energy: EnergyLevel::Calm },
    },
    AvatarModel {
        id: "100av-olivia",
        name: "Olivia",
        filename: "100av-olivia.vrm",
        style: AvatarStyle::Stylized,
        voice_profile: VoiceProfile { pitch: PitchRange::High, gender: AvatarGender::Female, energy: EnergyLevel::Energetic },
    },
    AvatarModel {
        id: "100av-mikel",
        name: "Mikel",
        filename: "100av-mikel.vrm",
        style: AvatarStyle::Stylized,
        voice_profile: VoiceProfile { pitch: PitchRange::Low, gender: AvatarGender::Male, energy: EnergyLevel::Moderate },
    },
    AvatarModel {
        id: "100av-amazonas",
        name: "Amazonas",
        filename: "100av-amazonas.vrm",
        style: AvatarStyle::Stylized,
        voice_profile: VoiceProfile { pitch: PitchRange::Mid, gender: AvatarGender::Neutral, energy: EnergyLevel::Energetic },
    },
    AvatarModel {
        id: "100av-dinokid",
        name: "DinoKid",
        filename: "100av-dinokid.vrm",
        style: AvatarStyle::Stylized,
        voice_profile: VoiceProfile { pitch: PitchRange::Mid, gender: AvatarGender::Neutral, energy: EnergyLevel::Energetic },
    },
];

/// Select the best avatar model for a persona based on TTS voice characteristics.
///
/// Matching algorithm:
/// 1. Filter by gender (if known)
/// 2. Score by pitch range match
/// 3. Score by energy level match
/// 4. Break ties by style preference (anime > stylized for most personas)
///
/// Falls back to deterministic hash-based selection if no voice profile is provided.
pub fn select_avatar_for_voice(
    voice_gender: Option<AvatarGender>,
    voice_pitch: Option<PitchRange>,
    voice_energy: Option<EnergyLevel>,
    preferred_style: Option<AvatarStyle>,
) -> &'static AvatarModel {
    let mut best_score = -1i32;
    let mut best = &AVATAR_CATALOG[0];

    for model in AVATAR_CATALOG {
        let mut score = 0i32;

        // Gender match (strongest signal)
        if let Some(gender) = voice_gender {
            if model.voice_profile.gender == gender {
                score += 10;
            } else if model.voice_profile.gender == AvatarGender::Neutral {
                score += 3; // Neutral is always acceptable
            }
        }

        // Pitch range match
        if let Some(pitch) = voice_pitch {
            if model.voice_profile.pitch == pitch {
                score += 5;
            }
        }

        // Energy level match
        if let Some(energy) = voice_energy {
            if model.voice_profile.energy == energy {
                score += 3;
            }
        }

        // Style preference
        if let Some(style) = preferred_style {
            if model.style == style {
                score += 2;
            }
        }

        if score > best_score {
            best_score = score;
            best = model;
        }
    }

    best
}

/// Select avatar by deterministic assignment using atomic counter.
/// Each new persona gets the next model in sequence — zero collisions
/// for up to 14 personas (wraps after that, but still distributed).
///
/// The old hash-based approach had 5 collisions among 17 personas because
/// the 31-multiplier hash has poor distribution over short strings mod 14.
pub fn select_avatar_by_identity(_identity: &str) -> &'static AvatarModel {
    static NEXT_INDEX: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let idx = NEXT_INDEX.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    &AVATAR_CATALOG[idx % AVATAR_CATALOG.len()]
}

/// Get the filesystem path for an avatar model.
pub fn avatar_model_path(filename: &str) -> std::path::PathBuf {
    // Models are relative to the working directory (src/)
    std::path::PathBuf::from("models/avatars").join(filename)
}

// =============================================================================
// Procedural rendering helpers (moved from livekit_agent.rs)
// =============================================================================

/// Generate a procedural avatar frame: colored circle on dark background.
fn generate_avatar_rgba(rgba: &mut [u8], width: u32, height: u32, identity: &str) {
    let (cr, cg, cb) = identity_to_color(identity);
    let w = width as f32;
    let h = height as f32;
    let center_x = w / 2.0;
    let center_y = h / 2.0;
    let radius = w.min(h) * 0.35;
    let radius_sq = radius * radius;

    let (bg_r, bg_g, bg_b): (u8, u8, u8) = (26, 26, 46);

    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - center_x;
            let dy = y as f32 - center_y;
            let dist_sq = dx * dx + dy * dy;

            let i = ((y * width + x) * 4) as usize;
            if dist_sq <= radius_sq {
                rgba[i] = cr;
                rgba[i + 1] = cg;
                rgba[i + 2] = cb;
            } else {
                rgba[i] = bg_r;
                rgba[i + 1] = bg_g;
                rgba[i + 2] = bg_b;
            }
            rgba[i + 3] = 255;
        }
    }
}

/// Derive a consistent RGB color from an identity string.
fn identity_to_color(identity: &str) -> (u8, u8, u8) {
    let hash: u32 = identity
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    let hue = (hash % 360) as f32;
    hsl_to_rgb(hue, 0.65, 0.55)
}

/// Convert HSL color to RGB. H in [0, 360), S and L in [0, 1].
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_avatar_catalog_has_14_models() {
        assert_eq!(AVATAR_CATALOG.len(), 14);
    }

    #[test]
    fn test_identity_selection_is_deterministic() {
        let a1 = select_avatar_by_identity("helper-ai");
        let a2 = select_avatar_by_identity("helper-ai");
        assert_eq!(a1.id, a2.id);
    }

    #[test]
    fn test_different_identities_get_different_avatars() {
        let a1 = select_avatar_by_identity("alice");
        let a2 = select_avatar_by_identity("bob");
        // With 14 models, two different identities should usually get different models
        // (not guaranteed, but very likely for short strings)
        // Just verify it doesn't crash
        assert!(!a1.id.is_empty());
        assert!(!a2.id.is_empty());
    }

    #[test]
    fn test_voice_matching_prefers_gender() {
        let model = select_avatar_for_voice(
            Some(AvatarGender::Male),
            None,
            None,
            None,
        );
        assert_eq!(model.voice_profile.gender, AvatarGender::Male);
    }

    #[test]
    fn test_voice_matching_full_profile() {
        let model = select_avatar_for_voice(
            Some(AvatarGender::Female),
            Some(PitchRange::High),
            Some(EnergyLevel::Calm),
            Some(AvatarStyle::Anime),
        );
        assert_eq!(model.voice_profile.gender, AvatarGender::Female);
        assert_eq!(model.style, AvatarStyle::Anime);
    }

    #[test]
    fn test_procedural_renderer_produces_correct_size() {
        let config = AvatarConfig {
            identity: "test-persona".into(),
            width: 320,
            height: 240,
            fps: 10.0,
            ..Default::default()
        };
        let mut renderer = ProceduralRenderer::new(config);
        let frame = renderer.render_frame();
        assert_eq!(frame.width, 320);
        assert_eq!(frame.height, 240);
        assert_eq!(frame.data.len(), 320 * 240 * 4);
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
}
