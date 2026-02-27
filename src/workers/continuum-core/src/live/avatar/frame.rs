//! Frame types and rendering configuration.
//!
//! RgbaFrame is the universal output currency of all avatar renderers.
//! AvatarConfig and ResolutionTier control rendering parameters.

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
    /// User preference for avatar selection (style, specific model, tag exclusions)
    pub preference: super::types::AvatarPreference,
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
            preference: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
