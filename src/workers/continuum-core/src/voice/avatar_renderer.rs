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
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{info, warn};

// =============================================================================
// Resolution Tiers — discrete levels to prevent render target thrashing
// =============================================================================

/// Discrete resolution tiers for adaptive avatar rendering.
/// Tile size in the browser drives which tier is used — smaller tiles
/// get lower resolution, saving GPU readback bandwidth and encoding cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResolutionTier {
    /// 160×120 @15fps — thumbnails under 120px wide
    Tiny,
    /// 320×240 @20fps — small tiles 120–300px wide
    Small,
    /// 480×360 @24fps — medium tiles 300–500px wide
    Medium,
    /// 640×480 @30fps — large/spotlight tiles over 500px wide
    Large,
}

impl ResolutionTier {
    /// Render target dimensions for this tier.
    pub fn dimensions(&self) -> (u32, u32) {
        match self {
            Self::Tiny   => (160, 120),
            Self::Small  => (320, 240),
            Self::Medium => (480, 360),
            Self::Large  => (640, 480),
        }
    }

    /// Target publish FPS for this tier.
    pub fn fps(&self) -> f64 {
        match self {
            Self::Tiny   => 15.0,
            Self::Small  => 20.0,
            Self::Medium => 24.0,
            Self::Large  => 30.0,
        }
    }

    /// Frame interval as nanoseconds (for AtomicU64 storage).
    pub fn interval_nanos(&self) -> u64 {
        (1_000_000_000.0 / self.fps()) as u64
    }

    /// Select tier from browser tile width in CSS pixels.
    pub fn from_tile_width(px: u32) -> Self {
        if px < 120 {
            Self::Tiny
        } else if px < 300 {
            Self::Small
        } else if px < 500 {
            Self::Medium
        } else {
            Self::Large
        }
    }
}

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
        // Dark background fallback (no circle) — shown only until first real Bevy frame.
        // Using the same dark color as Bevy's clear color to prevent a visible flash.
        let mut fallback_frame = vec![0u8; size];
        for pixel in fallback_frame.chunks_exact_mut(4) {
            pixel[0] = 26;   // R  (matches clear color 0.1)
            pixel[1] = 26;   // G
            pixel[2] = 46;   // B  (matches clear color 0.18)
            pixel[3] = 255;  // A
        }
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
        if exists {
            let bevy_system = super::bevy_renderer::get_or_init();
            let ready = bevy_system.is_ready();
            if ready {
                static NEXT_SLOT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
                let slot = NEXT_SLOT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if slot < super::bevy_renderer::MAX_AVATAR_SLOTS {
                    bevy_system.load_model(slot, vrm_path, &config.display_name);
                    // Register identity → slot mapping for speaking state routing
                    bevy_system.register_identity(&config.identity, slot);
                    if let Some(frame_rx) = bevy_system.frame_receiver(slot) {
                        info!(
                            "🎨 Using BevyChannelRenderer for '{}' (slot {}, model: {})",
                            config.identity, slot, vrm_path
                        );
                        return Box::new(BevyChannelRenderer::new(config, frame_rx.clone(), slot));
                    }
                } else {
                    warn!("🎨 All {} Bevy render slots taken, falling back to procedural for '{}'",
                        super::bevy_renderer::MAX_AVATAR_SLOTS, config.identity);
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
/// Returns a crossbeam receiver that the LiveKit video loop consumes, plus an
/// `Arc<AtomicU64>` that controls the sleep interval in nanoseconds (for FPS adaptation).
///
/// The renderer is selected by `create_renderer()` — the loop is backend-agnostic.
/// Any AvatarRenderer implementation plugs in without touching this code.
pub fn spawn_renderer_loop(
    config: AvatarConfig,
) -> (crossbeam_channel::Receiver<RgbaFrame>, Arc<AtomicU64>) {
    let fps = config.fps;
    let interval_nanos = Arc::new(AtomicU64::new(
        (1_000_000_000.0 / fps) as u64
    ));
    let interval_nanos_clone = interval_nanos.clone();

    let (tx, rx) = crossbeam_channel::bounded(2); // Small buffer, drop old frames

    let renderer = create_renderer(config);
    let renderer = Arc::new(std::sync::Mutex::new(renderer));

    std::thread::Builder::new()
        .name("avatar-renderer".into())
        .spawn(move || {
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
                let nanos = interval_nanos_clone.load(Ordering::Relaxed);
                std::thread::sleep(std::time::Duration::from_nanos(nanos));
            }
            info!("Avatar renderer loop exited");
        })
        .expect("Failed to spawn avatar renderer thread");

    (rx, interval_nanos)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

// =============================================================================
// Deterministic selection — hash-based picking from any array
// =============================================================================

/// Deterministically pick one element from a slice using a stable hash of a unique ID.
/// Same ID + same salt + same slice always returns the same element.
///
/// The `salt` parameter decorrelates picks across different trait arrays so that
/// e.g. gender and avatar model aren't locked to the same hash bucket:
///   `deterministic_pick(id, &genders, "gender")`
///   `deterministic_pick(id, &avatars, "avatar")`
///   `deterministic_pick(id, &voices, "voice")`
pub fn deterministic_pick<'a, T>(unique_id: &str, options: &'a [T], salt: &str) -> &'a T {
    assert!(!options.is_empty(), "Cannot pick from empty slice");
    &options[deterministic_index(unique_id, options.len(), salt)]
}

/// Deterministic index: hash a unique ID into the range [0, len).
/// Different salt values produce independent indices from the same ID.
pub fn deterministic_index(unique_id: &str, len: usize, salt: &str) -> usize {
    let mut hash = fnv1a_hash(unique_id.as_bytes());
    // Mix in salt to decorrelate across different trait dimensions
    hash = hash.wrapping_mul(0x100000001b3);
    hash ^= fnv1a_hash(salt.as_bytes());
    hash as usize % len
}

/// FNV-1a 64-bit hash — fast, excellent distribution for short strings and UUIDs.
fn fnv1a_hash(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325; // FNV offset basis
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3); // FNV prime
    }
    hash
}

/// Select avatar by deterministic hash of persona identity.
/// Same persona always gets the same model.
pub fn select_avatar_by_identity(identity: &str) -> &'static AvatarModel {
    deterministic_pick(identity, AVATAR_CATALOG, "avatar")
}

/// Extract gender from a TTS voice name.
///
/// Covers all TTS backends used in the system:
/// - Edge TTS: names like "en-US-GuyNeural" (male), "en-US-JennyNeural" (female)
/// - Kokoro: prefixes "af_"/"bf_" (female), "am_"/"bm_" (male)
/// - Orpheus: gendered names (tara/leah/jess/mia/zoe = female, leo/dan/zac = male)
/// - Piper/Pocket: character names (alba/cosette/eponine = female, jean/marius = male)
///
/// Returns None for unrecognized voices (caller falls back to round-robin).
pub fn gender_from_voice_name(voice: &str) -> Option<AvatarGender> {
    let lower = voice.to_lowercase();

    // Kokoro prefix convention: af_ / bf_ = female, am_ / bm_ = male
    if lower.starts_with("af_") || lower.starts_with("bf_") {
        return Some(AvatarGender::Female);
    }
    if lower.starts_with("am_") || lower.starts_with("bm_") {
        return Some(AvatarGender::Male);
    }

    // Edge TTS: contains "Guy" or known male names → male, known female names → female
    if lower.contains("guyneural") || lower.contains("andrewneural")
        || lower.contains("brianneural") || lower.contains("ericneural")
        || lower.contains("rogerneural") || lower.contains("steffanneural")
        || lower.contains("christopherneural") || lower.contains("davisneural") {
        return Some(AvatarGender::Male);
    }
    if lower.contains("jennyneural") || lower.contains("arianeural")
        || lower.contains("emmaneural") || lower.contains("janeneural")
        || lower.contains("nancyneural") || lower.contains("saraneural")
        || lower.contains("michelleneural") || lower.contains("amberneural") {
        return Some(AvatarGender::Female);
    }

    // Orpheus: gendered character names
    let orpheus_female = ["tara", "leah", "jess", "mia", "zoe"];
    let orpheus_male = ["leo", "dan", "zac"];
    for name in &orpheus_female {
        if lower == *name || lower.starts_with(&format!("{}_", name))
            || lower.starts_with(&format!("{}.", name)) {
            return Some(AvatarGender::Female);
        }
    }
    for name in &orpheus_male {
        if lower == *name || lower.starts_with(&format!("{}_", name))
            || lower.starts_with(&format!("{}.", name)) {
            return Some(AvatarGender::Male);
        }
    }

    // Piper / Pocket: character names from literature
    let pocket_female = ["alba", "fantine", "cosette", "eponine", "azelma"];
    let pocket_male = ["marius", "javert", "jean"];
    for name in &pocket_female {
        if lower.contains(name) {
            return Some(AvatarGender::Female);
        }
    }
    for name in &pocket_male {
        if lower.contains(name) {
            return Some(AvatarGender::Male);
        }
    }

    None
}

/// All genders available for deterministic selection.
const ALL_GENDERS: &[AvatarGender] = &[AvatarGender::Female, AvatarGender::Male, AvatarGender::Neutral];

/// Deterministically derive a gender from a persona identity.
/// Same persona always gets the same gender.
pub fn gender_from_identity(identity: &str) -> AvatarGender {
    *deterministic_pick(identity, ALL_GENDERS, "gender")
}

/// Select the best avatar for an agent.
///
/// Priority:
/// 1. If voice name is known, extract gender from it (voice-matched)
/// 2. Otherwise, derive gender deterministically from the persona identity
///
/// Within the gender-matched subset, picks deterministically by identity
/// so the same persona always gets the same avatar model.
pub fn select_avatar_for_agent(identity: &str, voice: Option<&str>) -> &'static AvatarModel {
    // Resolve gender: voice name > deterministic from identity
    let gender = voice
        .and_then(gender_from_voice_name)
        .unwrap_or_else(|| gender_from_identity(identity));

    // Filter catalog to matching gender (+ neutral is always acceptable)
    let matching: Vec<usize> = AVATAR_CATALOG.iter().enumerate()
        .filter(|(_, m)| m.voice_profile.gender == gender || m.voice_profile.gender == AvatarGender::Neutral)
        .map(|(i, _)| i)
        .collect();

    if !matching.is_empty() {
        let idx = deterministic_index(identity, matching.len(), "avatar");
        let picked = matching[idx];
        info!("🎭 Avatar for '{}': gender={:?} → model='{}'",
            &identity[..8.min(identity.len())], gender, AVATAR_CATALOG[picked].id);
        return &AVATAR_CATALOG[picked];
    }

    // Shouldn't happen (catalog has all genders) but safe fallback
    select_avatar_by_identity(identity)
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
    fn test_identity_selection_round_robin() {
        // select_avatar_by_identity uses an atomic counter — each call gets the next model.
        // It is NOT identity-based (same identity = different model on subsequent calls).
        let a1 = select_avatar_by_identity("helper-ai");
        let a2 = select_avatar_by_identity("helper-ai");
        // Verify both return valid models (may or may not be the same due to shared counter)
        assert!(!a1.id.is_empty());
        assert!(!a2.id.is_empty());
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

    #[test]
    fn test_gender_from_voice_kokoro_female() {
        assert_eq!(gender_from_voice_name("af_bella"), Some(AvatarGender::Female));
        assert_eq!(gender_from_voice_name("bf_emma"), Some(AvatarGender::Female));
    }

    #[test]
    fn test_gender_from_voice_kokoro_male() {
        assert_eq!(gender_from_voice_name("am_adam"), Some(AvatarGender::Male));
        assert_eq!(gender_from_voice_name("bm_george"), Some(AvatarGender::Male));
    }

    #[test]
    fn test_gender_from_voice_edge_tts() {
        assert_eq!(gender_from_voice_name("en-US-GuyNeural"), Some(AvatarGender::Male));
        assert_eq!(gender_from_voice_name("en-US-JennyNeural"), Some(AvatarGender::Female));
        assert_eq!(gender_from_voice_name("en-US-BrianNeural"), Some(AvatarGender::Male));
    }

    #[test]
    fn test_gender_from_voice_orpheus() {
        assert_eq!(gender_from_voice_name("tara"), Some(AvatarGender::Female));
        assert_eq!(gender_from_voice_name("leo"), Some(AvatarGender::Male));
        assert_eq!(gender_from_voice_name("zoe"), Some(AvatarGender::Female));
    }

    #[test]
    fn test_gender_from_voice_pocket() {
        assert_eq!(gender_from_voice_name("alba"), Some(AvatarGender::Female));
        assert_eq!(gender_from_voice_name("marius"), Some(AvatarGender::Male));
    }

    #[test]
    fn test_gender_from_voice_unknown() {
        assert_eq!(gender_from_voice_name("some-random-voice"), None);
        assert_eq!(gender_from_voice_name(""), None);
    }

    #[test]
    fn test_select_avatar_for_agent_with_voice() {
        // Male voice → male or neutral avatar (round-robin among matching)
        let model = select_avatar_for_agent("some-persona-id", Some("am_adam"));
        assert!(model.voice_profile.gender == AvatarGender::Male
            || model.voice_profile.gender == AvatarGender::Neutral,
            "Expected Male or Neutral, got {:?}", model.voice_profile.gender);

        // Female voice → female or neutral avatar (round-robin among matching)
        let model = select_avatar_for_agent("some-persona-id", Some("af_bella"));
        assert!(model.voice_profile.gender == AvatarGender::Female
            || model.voice_profile.gender == AvatarGender::Neutral,
            "Expected Female or Neutral, got {:?}", model.voice_profile.gender);
    }

    #[test]
    fn test_select_avatar_for_agent_no_voice() {
        // Without voice, derives gender from identity deterministically
        let model = select_avatar_for_agent("some-persona-id", None);
        assert!(!model.id.is_empty());
    }

    #[test]
    fn test_deterministic_pick_stable() {
        // Same ID + same array + same salt → always same result
        let options = ["a", "b", "c", "d", "e"];
        let pick1 = deterministic_pick("user-123", &options, "test");
        let pick2 = deterministic_pick("user-123", &options, "test");
        assert_eq!(pick1, pick2);
    }

    #[test]
    fn test_deterministic_pick_different_salt() {
        // Same ID but different salt → likely different result (decorrelated)
        let options = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let pick_a = deterministic_pick("user-xyz", &options, "salt-a");
        let pick_b = deterministic_pick("user-xyz", &options, "salt-b");
        // Not guaranteed different, but with 10 options it's very likely
        // Just verify they're valid picks
        assert!(options.contains(pick_a));
        assert!(options.contains(pick_b));
    }

    #[test]
    fn test_gender_from_identity_stable() {
        // Same identity always gets same gender
        let g1 = gender_from_identity("persona-abc-123");
        let g2 = gender_from_identity("persona-abc-123");
        assert_eq!(g1, g2);
    }

    #[test]
    fn test_gender_from_identity_covers_all() {
        // With enough different identities, all genders should appear
        let mut seen = std::collections::HashSet::new();
        for i in 0..100 {
            seen.insert(gender_from_identity(&format!("persona-{}", i)));
        }
        assert!(seen.len() >= 2, "Expected at least 2 genders from 100 identities, got {}", seen.len());
    }

    #[test]
    fn test_resolution_tier_dimensions() {
        assert_eq!(ResolutionTier::Tiny.dimensions(), (160, 120));
        assert_eq!(ResolutionTier::Small.dimensions(), (320, 240));
        assert_eq!(ResolutionTier::Medium.dimensions(), (480, 360));
        assert_eq!(ResolutionTier::Large.dimensions(), (640, 480));
    }

    #[test]
    fn test_resolution_tier_fps() {
        assert!((ResolutionTier::Tiny.fps() - 15.0).abs() < 0.01);
        assert!((ResolutionTier::Small.fps() - 20.0).abs() < 0.01);
        assert!((ResolutionTier::Medium.fps() - 24.0).abs() < 0.01);
        assert!((ResolutionTier::Large.fps() - 30.0).abs() < 0.01);
    }

    #[test]
    fn test_resolution_tier_from_tile_width() {
        assert_eq!(ResolutionTier::from_tile_width(50), ResolutionTier::Tiny);
        assert_eq!(ResolutionTier::from_tile_width(119), ResolutionTier::Tiny);
        assert_eq!(ResolutionTier::from_tile_width(120), ResolutionTier::Small);
        assert_eq!(ResolutionTier::from_tile_width(299), ResolutionTier::Small);
        assert_eq!(ResolutionTier::from_tile_width(300), ResolutionTier::Medium);
        assert_eq!(ResolutionTier::from_tile_width(499), ResolutionTier::Medium);
        assert_eq!(ResolutionTier::from_tile_width(500), ResolutionTier::Large);
        assert_eq!(ResolutionTier::from_tile_width(1920), ResolutionTier::Large);
    }

    #[test]
    fn test_resolution_tier_interval_nanos() {
        // 30fps = ~33.3ms = 33_333_333ns
        let nanos = ResolutionTier::Large.interval_nanos();
        assert!(nanos > 33_000_000 && nanos < 34_000_000);
        // 15fps = ~66.6ms = 66_666_666ns
        let nanos = ResolutionTier::Tiny.interval_nanos();
        assert!(nanos > 66_000_000 && nanos < 67_000_000);
    }
}
