//! Live2D sprite-sheet renderer — 2D compositing backend for Live2D-style avatars.
//!
//! Validates that RenderBackend + AvatarRenderer handles 2D CPU compositing
//! (maximally different from Bevy 3D GPU rendering — outlier B).
//!
//! Approach: sprite-sheet mode with parameter-driven frame selection.
//! A sprite atlas contains pre-rendered frames organized in a grid:
//!   - Rows = viseme states (idle, A, E, I, O, U, etc.)
//!   - Columns = expression variants (neutral, happy, sad, angry, surprised)
//!
//! At render time, the current viseme + emotion selects a cell from the grid.
//! The cell is cropped, scaled to the target resolution, and delivered as RgbaFrame.
//!
//! No Cubism SDK dependency — pure image compositing. Can swap to real Cubism SDK
//! later by implementing the same RenderBackend trait.

use crate::clog_info;
use crate::voice::avatar::backend::{RenderBackend, AvatarError, ModelFormat};
use crate::voice::avatar::frame::{RgbaFrame, AvatarConfig};
use crate::voice::avatar::renderer::AvatarRenderer;
use crate::voice::avatar::types::AvatarModel;

// =============================================================================
// Sprite atlas layout
// =============================================================================

/// Number of viseme rows in the sprite atlas.
/// Row 0 = idle, rows 1-6 = visemes (A, E, I, O, U, neutral-talk).
const VISEME_ROWS: usize = 7;

/// Number of expression columns in the sprite atlas.
/// Col 0 = neutral, col 1 = happy, col 2 = sad, col 3 = angry, col 4 = surprised.
const EXPRESSION_COLS: usize = 5;

/// Map emotion string to column index.
fn emotion_to_col(emotion: &str) -> usize {
    match emotion.to_lowercase().as_str() {
        "happy" | "joy" => 1,
        "sad" | "sorrow" => 2,
        "angry" | "anger" => 3,
        "surprised" | "surprise" => 4,
        _ => 0, // neutral
    }
}

/// Map viseme (0-14 OCULUS set) to row index.
/// Groups the 15 OCULUS visemes into our 7 rows.
fn viseme_to_row(viseme: u8) -> usize {
    match viseme {
        0 => 0,         // sil → idle
        1..=3 => 1,     // PP, FF, TH → A-like
        4..=5 => 2,     // DD, kk → E-like
        6..=8 => 3,     // iH, aa, ou → I-like
        9..=11 => 4,    // oh, oo, schwa → O-like
        12..=14 => 5,   // er, w, r → U-like
        _ => 6,         // fallback → neutral-talk
    }
}

// =============================================================================
// Live2DRenderer — sprite-sheet compositing
// =============================================================================

/// 2D sprite-sheet avatar renderer.
///
/// Holds the sprite atlas in memory as raw RGBA pixels.
/// Each render_frame() call extracts the appropriate cell based on
/// current viseme + emotion state.
pub struct Live2DRenderer {
    config: AvatarConfig,
    /// Sprite atlas: raw RGBA pixel data (atlas_width × atlas_height × 4 bytes)
    atlas_data: Vec<u8>,
    /// Atlas width in pixels (needed for row-stride calculation in extract_cell)
    atlas_width: u32,
    /// Individual cell size in the atlas
    cell_width: u32,
    cell_height: u32,
    /// Current viseme state (0 = idle)
    current_viseme: u8,
    /// Current emotion column index
    current_emotion_col: usize,
    /// Whether persona is speaking (affects idle vs talk animation)
    speaking: bool,
}

impl Live2DRenderer {
    /// Create a renderer from a pre-loaded sprite atlas.
    ///
    /// Atlas must be organized as a grid: VISEME_ROWS × EXPRESSION_COLS cells.
    /// Each cell has uniform dimensions derived from atlas size / grid size.
    pub fn from_atlas(
        config: AvatarConfig,
        atlas_data: Vec<u8>,
        atlas_width: u32,
        atlas_height: u32,
    ) -> Result<Self, AvatarError> {
        let cell_width = atlas_width / EXPRESSION_COLS as u32;
        let cell_height = atlas_height / VISEME_ROWS as u32;

        if cell_width == 0 || cell_height == 0 {
            return Err(AvatarError::RenderFailed(
                format!("Atlas too small: {}x{} for {}x{} grid",
                    atlas_width, atlas_height, EXPRESSION_COLS, VISEME_ROWS)
            ));
        }

        let expected_bytes = (atlas_width * atlas_height * 4) as usize;
        if atlas_data.len() < expected_bytes {
            return Err(AvatarError::RenderFailed(
                format!("Atlas data too short: {} bytes, expected {}",
                    atlas_data.len(), expected_bytes)
            ));
        }

        clog_info!(
            "Live2DRenderer: initialized for '{}' (atlas {}x{}, cell {}x{}, target {}x{})",
            config.identity, atlas_width, atlas_height,
            cell_width, cell_height, config.width, config.height
        );

        Ok(Self {
            config,
            atlas_data,
            atlas_width,
            cell_width,
            cell_height,
            current_viseme: 0,
            current_emotion_col: 0,
            speaking: false,
        })
    }

    /// Extract a cell from the atlas and scale to target resolution.
    fn extract_cell(&self, row: usize, col: usize) -> Vec<u8> {
        let src_x = (col as u32) * self.cell_width;
        let src_y = (row as u32) * self.cell_height;
        let tw = self.config.width;
        let th = self.config.height;
        let mut output = vec![0u8; (tw * th * 4) as usize];

        // Nearest-neighbor scaling from cell to target resolution
        for dy in 0..th {
            for dx in 0..tw {
                let sx = src_x + (dx * self.cell_width / tw);
                let sy = src_y + (dy * self.cell_height / th);

                let src_off = ((sy * self.atlas_width + sx) * 4) as usize;
                let dst_off = ((dy * tw + dx) * 4) as usize;

                if src_off + 3 < self.atlas_data.len() {
                    output[dst_off]     = self.atlas_data[src_off];
                    output[dst_off + 1] = self.atlas_data[src_off + 1];
                    output[dst_off + 2] = self.atlas_data[src_off + 2];
                    output[dst_off + 3] = self.atlas_data[src_off + 3];
                }
            }
        }

        output
    }
}

impl AvatarRenderer for Live2DRenderer {
    fn render_frame(&mut self) -> RgbaFrame {
        let row = if self.speaking {
            viseme_to_row(self.current_viseme)
        } else {
            0 // idle row when not speaking
        };
        let col = self.current_emotion_col;

        let data = self.extract_cell(row, col);
        RgbaFrame {
            width: self.config.width,
            height: self.config.height,
            data,
        }
    }

    fn set_speaking(&mut self, speaking: bool) {
        self.speaking = speaking;
        if !speaking {
            self.current_viseme = 0; // reset to idle
        }
    }

    fn set_viseme(&mut self, viseme: u8, _weight: f32) {
        self.current_viseme = viseme;
    }

    fn set_emotion(&mut self, emotion: &str) {
        self.current_emotion_col = emotion_to_col(emotion);
    }
}

// =============================================================================
// Live2DBackend — RenderBackend implementation
// =============================================================================

/// Backend factory for Live2D sprite-sheet rendering.
/// Handles .moc3 (future: real Cubism SDK) and sprite-sheet formats.
pub struct Live2DBackend {
    initialized: bool,
}

impl Live2DBackend {
    pub fn new() -> Self {
        Self { initialized: false }
    }
}

impl RenderBackend for Live2DBackend {
    fn name(&self) -> &'static str { "live2d" }

    fn description(&self) -> &'static str {
        "2D sprite-sheet compositing for Live2D-style avatars"
    }

    fn supported_formats(&self) -> &[ModelFormat] {
        &[ModelFormat::Live2D, ModelFormat::SpriteSheet]
    }

    fn is_initialized(&self) -> bool { self.initialized }

    fn initialize(&mut self) -> Result<(), AvatarError> {
        // No external dependencies needed for sprite-sheet mode
        self.initialized = true;
        Ok(())
    }

    fn create_renderer(
        &self,
        model: &AvatarModel,
        config: &AvatarConfig,
    ) -> Result<Box<dyn AvatarRenderer>, AvatarError> {
        let atlas_path = super::super::catalog::avatar_model_path(model.filename);
        if !atlas_path.exists() {
            return Err(AvatarError::ModelNotFound(
                format!("Sprite atlas not found: {}", atlas_path.display())
            ));
        }

        // Load the atlas image
        // For now, we support raw RGBA files (width/height encoded in filename or manifest).
        // A real implementation would use image crate to decode PNG/WebP.
        let atlas_data = std::fs::read(&atlas_path)
            .map_err(|e| AvatarError::IoError(e))?;

        // Infer atlas dimensions from file size assuming square-ish atlas
        // with EXPRESSION_COLS × VISEME_ROWS cells.
        // Each cell should be roughly the target resolution.
        let cell_w = config.width;
        let cell_h = config.height;
        let atlas_width = cell_w * EXPRESSION_COLS as u32;
        let atlas_height = cell_h * VISEME_ROWS as u32;
        let expected = (atlas_width * atlas_height * 4) as usize;

        if atlas_data.len() < expected {
            return Err(AvatarError::RenderFailed(
                format!("Atlas file too small: {} bytes, expected {} for {}x{} atlas",
                    atlas_data.len(), expected, atlas_width, atlas_height)
            ));
        }

        let renderer = Live2DRenderer::from_atlas(
            config.clone(),
            atlas_data,
            atlas_width,
            atlas_height,
        )?;

        clog_info!(
            "🎭 Live2DBackend: created renderer for '{}' (atlas: {})",
            config.identity, atlas_path.display()
        );
        Ok(Box::new(renderer))
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a test atlas: EXPRESSION_COLS × VISEME_ROWS grid of solid-colored cells.
    /// Each cell gets a unique color based on (row, col) so we can verify correct extraction.
    fn make_test_atlas(cell_w: u32, cell_h: u32) -> (Vec<u8>, u32, u32) {
        let atlas_w = cell_w * EXPRESSION_COLS as u32;
        let atlas_h = cell_h * VISEME_ROWS as u32;
        let mut data = vec![0u8; (atlas_w * atlas_h * 4) as usize];

        for row in 0..VISEME_ROWS {
            for col in 0..EXPRESSION_COLS {
                let r = ((row * 37 + col * 73) % 256) as u8;
                let g = ((row * 53 + col * 17) % 256) as u8;
                let b = ((row * 97 + col * 41) % 256) as u8;

                let cell_x = col as u32 * cell_w;
                let cell_y = row as u32 * cell_h;

                for dy in 0..cell_h {
                    for dx in 0..cell_w {
                        let off = (((cell_y + dy) * atlas_w + (cell_x + dx)) * 4) as usize;
                        data[off] = r;
                        data[off + 1] = g;
                        data[off + 2] = b;
                        data[off + 3] = 255;
                    }
                }
            }
        }

        (data, atlas_w, atlas_h)
    }

    fn cell_color(row: usize, col: usize) -> (u8, u8, u8) {
        let r = ((row * 37 + col * 73) % 256) as u8;
        let g = ((row * 53 + col * 17) % 256) as u8;
        let b = ((row * 97 + col * 41) % 256) as u8;
        (r, g, b)
    }

    #[test]
    fn test_live2d_renderer_produces_correct_size() {
        let (atlas, aw, ah) = make_test_atlas(32, 32);
        let config = AvatarConfig {
            identity: "test".into(),
            width: 32,
            height: 32,
            fps: 10.0,
            ..Default::default()
        };
        let mut renderer = Live2DRenderer::from_atlas(config, atlas, aw, ah).unwrap();
        let frame = renderer.render_frame();
        assert_eq!(frame.width, 32);
        assert_eq!(frame.height, 32);
        assert_eq!(frame.data.len(), 32 * 32 * 4);
    }

    #[test]
    fn test_live2d_idle_frame_is_row0_col0() {
        let (atlas, aw, ah) = make_test_atlas(16, 16);
        let config = AvatarConfig {
            identity: "test".into(),
            width: 16,
            height: 16,
            fps: 10.0,
            ..Default::default()
        };
        let mut renderer = Live2DRenderer::from_atlas(config, atlas, aw, ah).unwrap();

        // Not speaking → idle (row 0, col 0 = neutral)
        let frame = renderer.render_frame();
        let (er, eg, eb) = cell_color(0, 0);
        assert_eq!(frame.data[0], er, "Expected R={}, got {}", er, frame.data[0]);
        assert_eq!(frame.data[1], eg, "Expected G={}, got {}", eg, frame.data[1]);
        assert_eq!(frame.data[2], eb, "Expected B={}, got {}", eb, frame.data[2]);
    }

    #[test]
    fn test_live2d_viseme_changes_row() {
        let (atlas, aw, ah) = make_test_atlas(16, 16);
        let config = AvatarConfig {
            identity: "test".into(),
            width: 16,
            height: 16,
            fps: 10.0,
            ..Default::default()
        };
        let mut renderer = Live2DRenderer::from_atlas(config, atlas, aw, ah).unwrap();

        renderer.set_speaking(true);
        renderer.set_viseme(1, 1.0); // PP → row 1

        let frame = renderer.render_frame();
        let (er, eg, eb) = cell_color(1, 0);
        assert_eq!(frame.data[0], er, "Viseme 1 should select row 1");
        assert_eq!(frame.data[1], eg);
        assert_eq!(frame.data[2], eb);
    }

    #[test]
    fn test_live2d_emotion_changes_column() {
        let (atlas, aw, ah) = make_test_atlas(16, 16);
        let config = AvatarConfig {
            identity: "test".into(),
            width: 16,
            height: 16,
            fps: 10.0,
            ..Default::default()
        };
        let mut renderer = Live2DRenderer::from_atlas(config, atlas, aw, ah).unwrap();

        renderer.set_emotion("happy"); // col 1
        renderer.set_speaking(true);
        renderer.set_viseme(0, 1.0); // sil → row 0 (idle) but speaking

        let frame = renderer.render_frame();
        let (er, eg, eb) = cell_color(0, 1);
        assert_eq!(frame.data[0], er, "Emotion 'happy' should select col 1");
        assert_eq!(frame.data[1], eg);
        assert_eq!(frame.data[2], eb);
    }

    #[test]
    fn test_live2d_combined_viseme_emotion() {
        let (atlas, aw, ah) = make_test_atlas(16, 16);
        let config = AvatarConfig {
            identity: "test".into(),
            width: 16,
            height: 16,
            fps: 10.0,
            ..Default::default()
        };
        let mut renderer = Live2DRenderer::from_atlas(config, atlas, aw, ah).unwrap();

        renderer.set_speaking(true);
        renderer.set_viseme(9, 1.0);      // oh → row 4 (O-like)
        renderer.set_emotion("surprised"); // col 4

        let frame = renderer.render_frame();
        let (er, eg, eb) = cell_color(4, 4);
        assert_eq!(frame.data[0], er, "Row 4 col 4 expected");
        assert_eq!(frame.data[1], eg);
        assert_eq!(frame.data[2], eb);
    }

    #[test]
    fn test_live2d_speaking_false_resets_to_idle() {
        let (atlas, aw, ah) = make_test_atlas(16, 16);
        let config = AvatarConfig {
            identity: "test".into(),
            width: 16,
            height: 16,
            fps: 10.0,
            ..Default::default()
        };
        let mut renderer = Live2DRenderer::from_atlas(config, atlas, aw, ah).unwrap();

        renderer.set_speaking(true);
        renderer.set_viseme(6, 1.0); // row 3
        renderer.set_emotion("sad"); // col 2

        // Stop speaking → should go back to row 0
        renderer.set_speaking(false);

        let frame = renderer.render_frame();
        let (er, eg, eb) = cell_color(0, 2); // row 0 (idle), col 2 (sad emotion persists)
        assert_eq!(frame.data[0], er, "After stop speaking, should be row 0");
        assert_eq!(frame.data[1], eg);
        assert_eq!(frame.data[2], eb);
    }

    #[test]
    fn test_live2d_scaling() {
        // Atlas cells are 16x16, but render target is 32x32 (upscale)
        let (atlas, aw, ah) = make_test_atlas(16, 16);
        let config = AvatarConfig {
            identity: "test".into(),
            width: 32,
            height: 32,
            fps: 10.0,
            ..Default::default()
        };
        let mut renderer = Live2DRenderer::from_atlas(config, atlas, aw, ah).unwrap();
        let frame = renderer.render_frame();
        assert_eq!(frame.width, 32);
        assert_eq!(frame.height, 32);
        assert_eq!(frame.data.len(), 32 * 32 * 4);

        // Upscaled from uniform cell → should still be uniform color
        let (er, eg, eb) = cell_color(0, 0);
        assert_eq!(frame.data[0], er);
        assert_eq!(frame.data[1], eg);
        assert_eq!(frame.data[2], eb);
    }

    #[test]
    fn test_live2d_backend_supported_formats() {
        let backend = Live2DBackend::new();
        let formats = backend.supported_formats();
        assert!(formats.contains(&ModelFormat::Live2D));
        assert!(formats.contains(&ModelFormat::SpriteSheet));
        assert!(!formats.contains(&ModelFormat::Vrm0x));
    }

    #[test]
    fn test_live2d_backend_initialize() {
        let mut backend = Live2DBackend::new();
        assert!(!backend.is_initialized());
        backend.initialize().unwrap();
        assert!(backend.is_initialized());
    }

    #[test]
    fn test_viseme_mapping_coverage() {
        // All 15 OCULUS visemes should map to valid rows
        for v in 0..=14 {
            let row = viseme_to_row(v);
            assert!(row < VISEME_ROWS, "Viseme {} mapped to invalid row {}", v, row);
        }
        // Out of range → neutral-talk row
        assert_eq!(viseme_to_row(255), 6);
    }

    #[test]
    fn test_emotion_mapping() {
        assert_eq!(emotion_to_col("neutral"), 0);
        assert_eq!(emotion_to_col("happy"), 1);
        assert_eq!(emotion_to_col("joy"), 1);
        assert_eq!(emotion_to_col("sad"), 2);
        assert_eq!(emotion_to_col("angry"), 3);
        assert_eq!(emotion_to_col("surprised"), 4);
        assert_eq!(emotion_to_col("HAPPY"), 1); // case insensitive
        assert_eq!(emotion_to_col("unknown"), 0); // fallback
    }

    #[test]
    fn test_atlas_too_small_error() {
        let config = AvatarConfig {
            identity: "test".into(),
            width: 16,
            height: 16,
            fps: 10.0,
            ..Default::default()
        };
        // Too few bytes
        let result = Live2DRenderer::from_atlas(config, vec![0; 100], 10, 10);
        assert!(result.is_err());
    }
}
