//! ProceduralRenderer — colored circle on dark background (zero dependencies).
//!
//! Used as fallback when Bevy/VRM is not available.

use crate::clog_info;
use crate::voice::avatar::backend::{RenderBackend, AvatarError, ModelFormat};
use crate::voice::avatar::frame::{RgbaFrame, AvatarConfig};
use crate::voice::avatar::renderer::AvatarRenderer;
use crate::voice::avatar::types::AvatarModel;

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
        clog_info!(
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
// Procedural rendering helpers
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

// =============================================================================
// ProceduralBackend — RenderBackend implementation (universal fallback)
// =============================================================================

/// Backend factory for the procedural renderer.
/// Always available, supports all formats as a fallback (renders identity-colored circle).
pub struct ProceduralBackend {
    initialized: bool,
}

impl ProceduralBackend {
    pub fn new() -> Self {
        Self { initialized: false }
    }
}

impl RenderBackend for ProceduralBackend {
    fn name(&self) -> &'static str { "procedural" }

    fn description(&self) -> &'static str {
        "CPU-rendered colored circle (zero-dependency fallback)"
    }

    fn supported_formats(&self) -> &[ModelFormat] {
        // Procedural can render "something" for any format — it ignores the model
        // and just draws a colored circle based on identity.
        &[ModelFormat::StaticImage, ModelFormat::Svg, ModelFormat::SpriteSheet]
    }

    fn is_initialized(&self) -> bool { self.initialized }

    fn initialize(&mut self) -> Result<(), AvatarError> {
        self.initialized = true;
        Ok(())
    }

    fn create_renderer(
        &self,
        _model: &AvatarModel,
        config: &AvatarConfig,
    ) -> Result<Box<dyn AvatarRenderer>, AvatarError> {
        Ok(Box::new(ProceduralRenderer::new(config.clone())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
