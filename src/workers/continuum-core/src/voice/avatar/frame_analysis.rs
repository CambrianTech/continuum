//! Frame analysis — runtime quality checks for rendered avatar frames.
//!
//! Detects degenerate renders: all-black, all-background, distorted geometry
//! (broken bilateral symmetry), insufficient coverage, flickering.
//! Works on any RgbaFrame regardless of backend.

use super::frame::RgbaFrame;

/// Analysis results for a single frame.
#[derive(Debug, Clone)]
pub struct FrameAnalysis {
    /// Fraction of pixels that differ from the background color [0.0, 1.0]
    pub coverage: f32,
    /// Bilateral symmetry score [0.0, 1.0] — 1.0 = perfectly symmetric
    /// Distorted skinning breaks symmetry of humanoid models.
    pub symmetry: f32,
    /// Number of distinct colors in the frame (sampled, not exhaustive)
    pub color_diversity: u32,
    /// Whether the frame is entirely one color (degenerate)
    pub is_solid: bool,
    /// Whether the frame appears to be just the dark background
    pub is_background_only: bool,
    /// Average foreground/background transitions per scanline.
    /// Smooth silhouette (healthy avatar) ≈ 2-4 transitions.
    /// Jagged/exploding mesh (broken render) ≈ 10-30+ transitions.
    /// This is the primary signal for detecting untextured or distorted geometry.
    pub edge_roughness: f32,
    /// Number of foreground pixels that are nearly white (R,G,B all > 240).
    /// High ratio of white-to-foreground indicates missing textures.
    pub white_ratio: f32,
}

/// Health verdict for a rendered frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthVerdict {
    /// Frame looks like a properly textured, well-formed avatar.
    Healthy,
    /// Frame is empty / background only — model failed to load.
    Empty,
    /// Frame is solid color — degenerate render.
    Degenerate,
    /// Frame has content but jagged edges — broken geometry or missing textures.
    BrokenGeometry,
    /// Frame renders but is mostly white — missing textures.
    MissingTextures,
}

impl FrameAnalysis {
    /// Determine health verdict from the analysis metrics.
    pub fn verdict(&self) -> HealthVerdict {
        if self.is_background_only {
            return HealthVerdict::Empty;
        }
        if self.is_solid {
            return HealthVerdict::Degenerate;
        }
        // Mostly white foreground = missing textures
        if self.white_ratio > 0.7 && self.color_diversity < 10 {
            return HealthVerdict::MissingTextures;
        }
        // Jagged boundary = exploding mesh / broken geometry.
        // Healthy humanoid silhouette: 2-6 transitions per scanline.
        // Complex models (detailed hair, accessories): 8-15 transitions — still correct.
        // Truly broken geometry: 10-30+ transitions with FEW colors and LOW symmetry.
        //
        // Key insight: broken skinning scatters triangles → chaotic edges with few colors.
        // A well-textured model with detailed hair → many edges but RICH colors and symmetry.
        if self.edge_roughness > 8.0 {
            let well_textured = self.color_diversity > 100;
            let reasonably_symmetric = self.symmetry > 0.70;
            // Well-textured + symmetric = detailed model, not broken.
            // Missing either signal = genuine broken geometry.
            if !(well_textured && reasonably_symmetric) {
                return HealthVerdict::BrokenGeometry;
            }
        }
        HealthVerdict::Healthy
    }

    /// Is this frame healthy enough to show to users?
    pub fn is_healthy(&self) -> bool {
        self.verdict() == HealthVerdict::Healthy
    }
}

/// The dark background color used by both Bevy clear and procedural renderer.
const BG_R: u8 = 26;
const BG_G: u8 = 26;
const BG_B: u8 = 46;

/// Tolerance for "is this pixel background?" — accounts for anti-aliasing edges.
const BG_TOLERANCE: u8 = 10;

fn is_background_pixel(r: u8, g: u8, b: u8) -> bool {
    r.abs_diff(BG_R) <= BG_TOLERANCE
        && g.abs_diff(BG_G) <= BG_TOLERANCE
        && b.abs_diff(BG_B) <= BG_TOLERANCE
}

/// Analyze a rendered frame for quality issues.
pub fn analyze(frame: &RgbaFrame) -> FrameAnalysis {
    let total_pixels = (frame.width * frame.height) as usize;
    if total_pixels == 0 || frame.data.len() < total_pixels * 4 {
        return FrameAnalysis {
            coverage: 0.0,
            symmetry: 0.0,
            color_diversity: 0,
            is_solid: true,
            is_background_only: true,
            edge_roughness: 0.0,
            white_ratio: 0.0,
        };
    }

    let w = frame.width as usize;
    let h = frame.height as usize;

    // --- Coverage: fraction of non-background pixels ---
    let mut non_bg = 0usize;
    let first_r = frame.data[0];
    let first_g = frame.data[1];
    let first_b = frame.data[2];
    let mut all_same = true;

    // Color diversity: sample every 8th pixel, collect into a small set
    let mut color_set = std::collections::HashSet::new();

    for i in 0..total_pixels {
        let off = i * 4;
        let (r, g, b) = (frame.data[off], frame.data[off + 1], frame.data[off + 2]);

        if !is_background_pixel(r, g, b) {
            non_bg += 1;
        }
        if r != first_r || g != first_g || b != first_b {
            all_same = false;
        }
        // Sample every 8th pixel for diversity (full enumeration is expensive)
        if i % 8 == 0 {
            // Quantize to 6-bit per channel for reasonable bucket size
            let key = ((r >> 2) as u32) << 12
                | ((g >> 2) as u32) << 6
                | (b >> 2) as u32;
            color_set.insert(key);
        }
    }

    let coverage = non_bg as f32 / total_pixels as f32;

    // --- White ratio: fraction of foreground pixels that are nearly white ---
    // Missing textures render as white geometry against the dark background.
    let mut white_fg = 0usize;
    for i in 0..total_pixels {
        let off = i * 4;
        let (r, g, b) = (frame.data[off], frame.data[off + 1], frame.data[off + 2]);
        if !is_background_pixel(r, g, b) && r > 240 && g > 240 && b > 240 {
            white_fg += 1;
        }
    }
    let white_ratio = if non_bg > 0 { white_fg as f32 / non_bg as f32 } else { 0.0 };

    // --- Edge roughness: avg foreground/background transitions per scanline ---
    // A smooth humanoid silhouette has ~2 transitions per row (left edge, right edge).
    // Jagged/exploding mesh has many more as the boundary zigzags wildly.
    let mut total_transitions = 0u64;
    let mut counted_rows = 0u64;
    // Sample every other row for performance
    let row_step_rough = 2.max(h / 128);
    for y in (0..h).step_by(row_step_rough) {
        let mut transitions = 0u32;
        let mut prev_fg = false;
        for x in 0..w {
            let off = (y * w + x) * 4;
            let (r, g, b) = (frame.data[off], frame.data[off + 1], frame.data[off + 2]);
            let fg = !is_background_pixel(r, g, b);
            if fg != prev_fg {
                transitions += 1;
            }
            prev_fg = fg;
        }
        total_transitions += transitions as u64;
        counted_rows += 1;
    }
    let edge_roughness = if counted_rows > 0 {
        total_transitions as f32 / counted_rows as f32
    } else {
        0.0
    };

    // --- Bilateral symmetry: compare left half to mirrored right half ---
    // Sample rows to keep cost O(height) not O(width*height)
    let half_w = w / 2;
    let mut sym_matches = 0u64;
    let mut sym_total = 0u64;
    // Sample every 4th row, every 2nd column in the left half
    let row_step = 4.max(h / 64); // at most 64 rows sampled
    let col_step = 2.max(half_w / 64); // at most 64 cols per row

    for y in (0..h).step_by(row_step) {
        for x in (0..half_w).step_by(col_step) {
            let mirror_x = w - 1 - x;
            let left = (y * w + x) * 4;
            let right = (y * w + mirror_x) * 4;

            let dr = frame.data[left].abs_diff(frame.data[right]) as u32;
            let dg = frame.data[left + 1].abs_diff(frame.data[right + 1]) as u32;
            let db = frame.data[left + 2].abs_diff(frame.data[right + 2]) as u32;
            let diff = dr + dg + db;

            sym_total += 1;
            // Pixels within tolerance count as symmetric
            if diff < 30 {
                sym_matches += 1;
            }
        }
    }

    let symmetry = if sym_total > 0 {
        sym_matches as f32 / sym_total as f32
    } else {
        0.0
    };

    FrameAnalysis {
        coverage,
        symmetry,
        color_diversity: color_set.len() as u32,
        is_solid: all_same,
        is_background_only: coverage < 0.01,
        edge_roughness,
        white_ratio,
    }
}

/// Check if two consecutive frames differ significantly (flickering detection).
/// Returns the fraction of pixels that changed [0.0, 1.0].
pub fn frame_difference(a: &RgbaFrame, b: &RgbaFrame) -> f32 {
    if a.width != b.width || a.height != b.height {
        return 1.0; // Different dimensions = completely different
    }
    let total = (a.width * a.height) as usize;
    if total == 0 { return 0.0; }

    let mut changed = 0usize;
    for i in 0..total {
        let off = i * 4;
        let dr = a.data[off].abs_diff(b.data[off]) as u32;
        let dg = a.data[off + 1].abs_diff(b.data[off + 1]) as u32;
        let db = a.data[off + 2].abs_diff(b.data[off + 2]) as u32;
        if dr + dg + db > 15 {
            changed += 1;
        }
    }
    changed as f32 / total as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice::avatar::backends::ProceduralRenderer;
    use crate::voice::avatar::renderer::AvatarRenderer;
    use crate::voice::avatar::frame::AvatarConfig;

    fn make_procedural_frame(identity: &str, w: u32, h: u32) -> RgbaFrame {
        let config = AvatarConfig {
            identity: identity.into(),
            width: w,
            height: h,
            fps: 10.0,
            ..Default::default()
        };
        let mut renderer = ProceduralRenderer::new(config);
        renderer.render_frame()
    }

    #[test]
    fn test_procedural_frame_has_content() {
        let frame = make_procedural_frame("test-agent", 320, 240);
        let analysis = analyze(&frame);

        assert!(!analysis.is_solid, "Procedural frame should not be solid color");
        assert!(!analysis.is_background_only, "Procedural frame should have visible content");
        assert!(analysis.coverage > 0.1, "Circle should cover >10% of frame, got {:.1}%", analysis.coverage * 100.0);
        assert!(analysis.coverage < 0.9, "Circle should not cover >90% of frame, got {:.1}%", analysis.coverage * 100.0);
    }

    #[test]
    fn test_procedural_frame_is_symmetric() {
        let frame = make_procedural_frame("symmetric-test", 320, 240);
        let analysis = analyze(&frame);

        // A centered circle should be highly symmetric
        assert!(analysis.symmetry > 0.85,
            "Centered circle should be symmetric, got {:.2}", analysis.symmetry);
    }

    #[test]
    fn test_procedural_frame_color_diversity() {
        let frame = make_procedural_frame("color-test", 320, 240);
        let analysis = analyze(&frame);

        // Circle + background = at least 2 distinct color regions
        assert!(analysis.color_diversity >= 2,
            "Should have at least 2 color regions (circle + bg), got {}", analysis.color_diversity);
    }

    #[test]
    fn test_procedural_frame_deterministic() {
        let frame_a = make_procedural_frame("determinism-test", 160, 120);
        let frame_b = make_procedural_frame("determinism-test", 160, 120);

        // Same identity → same frame (static image)
        let diff = frame_difference(&frame_a, &frame_b);
        assert!(diff < 0.001,
            "Same identity should produce identical frames, got {:.1}% different", diff * 100.0);
    }

    #[test]
    fn test_different_identities_different_colors() {
        let frame_a = make_procedural_frame("alice", 160, 120);
        let frame_b = make_procedural_frame("bob", 160, 120);

        let diff = frame_difference(&frame_a, &frame_b);
        assert!(diff > 0.05,
            "Different identities should produce different frames, got {:.1}% different", diff * 100.0);
    }

    #[test]
    fn test_solid_frame_detection() {
        let mut data = vec![0u8; 4 * 4 * 4];
        for pixel in data.chunks_exact_mut(4) {
            pixel[0] = 100; pixel[1] = 100; pixel[2] = 100; pixel[3] = 255;
        }
        let frame = RgbaFrame { width: 4, height: 4, data };
        let analysis = analyze(&frame);
        assert!(analysis.is_solid, "Uniform color frame should be detected as solid");
    }

    #[test]
    fn test_background_only_detection() {
        // Frame filled with background color
        let mut data = vec![0u8; 4 * 4 * 4];
        for pixel in data.chunks_exact_mut(4) {
            pixel[0] = BG_R;
            pixel[1] = BG_G;
            pixel[2] = BG_B;
            pixel[3] = 255;
        }
        let frame = RgbaFrame { width: 4, height: 4, data };
        let analysis = analyze(&frame);
        assert!(analysis.is_background_only, "All-background frame should be detected");
    }

    #[test]
    fn test_empty_frame() {
        let frame = RgbaFrame { width: 0, height: 0, data: vec![] };
        let analysis = analyze(&frame);
        assert!(analysis.is_solid);
        assert!(analysis.is_background_only);
        assert_eq!(analysis.coverage, 0.0);
    }

    #[test]
    fn test_procedural_frame_low_edge_roughness() {
        // A smooth circle has ~2 transitions per scanline (enter + exit)
        let frame = make_procedural_frame("smooth-test", 320, 240);
        let analysis = analyze(&frame);
        assert!(analysis.edge_roughness < 6.0,
            "Smooth circle should have low edge roughness, got {:.1}", analysis.edge_roughness);
        assert!(analysis.is_healthy(),
            "Procedural circle should be healthy, verdict={:?}", analysis.verdict());
    }

    #[test]
    fn test_exploding_mesh_detected() {
        // Simulate exploding mesh: alternating foreground/background every few pixels
        // across each scanline — produces high edge_roughness like the demon image.
        let w = 128u32;
        let h = 96u32;
        let mut data = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let off = ((y * w + x) * 4) as usize;
                // Create jagged spikes: foreground for ~3px, background for ~3px, repeat
                let spike = ((x + y * 7) % 6) < 3;
                // Only in the central region (like a head-sized area)
                let in_region = x > w / 4 && x < 3 * w / 4 && y > h / 4 && y < 3 * h / 4;
                if spike && in_region {
                    data[off] = 255; data[off + 1] = 255; data[off + 2] = 255; // white fg
                } else {
                    data[off] = BG_R; data[off + 1] = BG_G; data[off + 2] = BG_B;
                }
                data[off + 3] = 255;
            }
        }
        let frame = RgbaFrame { width: w, height: h, data };
        let analysis = analyze(&frame);
        assert!(analysis.edge_roughness > 8.0,
            "Jagged spiky pattern should have high edge roughness, got {:.1}", analysis.edge_roughness);
        assert!(!analysis.is_healthy(),
            "Exploding mesh should NOT be healthy, verdict={:?}", analysis.verdict());
    }

    #[test]
    fn test_white_untextured_model_detected() {
        // Simulate untextured model: smooth white oval on dark background
        let w = 64u32;
        let h = 64u32;
        let mut data = vec![0u8; (w * h * 4) as usize];
        let cx = w as f32 / 2.0;
        let cy = h as f32 / 2.0;
        let rx = w as f32 / 3.0;
        let ry = h as f32 / 2.5;
        for y in 0..h {
            for x in 0..w {
                let off = ((y * w + x) * 4) as usize;
                let dx = (x as f32 - cx) / rx;
                let dy = (y as f32 - cy) / ry;
                if dx * dx + dy * dy < 1.0 {
                    // All white — no texture
                    data[off] = 255; data[off + 1] = 255; data[off + 2] = 255;
                } else {
                    data[off] = BG_R; data[off + 1] = BG_G; data[off + 2] = BG_B;
                }
                data[off + 3] = 255;
            }
        }
        let frame = RgbaFrame { width: w, height: h, data };
        let analysis = analyze(&frame);
        assert!(analysis.white_ratio > 0.9,
            "Untextured white model should have high white ratio, got {:.2}", analysis.white_ratio);
        assert_eq!(analysis.verdict(), HealthVerdict::MissingTextures,
            "Smooth white shape should be MissingTextures, got {:?}", analysis.verdict());
    }

    #[test]
    fn test_healthy_textured_model() {
        // Simulate a textured avatar: varied colors, smooth boundary
        let w = 64u32;
        let h = 64u32;
        let mut data = vec![0u8; (w * h * 4) as usize];
        let cx = w as f32 / 2.0;
        let cy = h as f32 / 2.0;
        let r = w as f32 / 3.0;
        for y in 0..h {
            for x in 0..w {
                let off = ((y * w + x) * 4) as usize;
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                if dx * dx + dy * dy < r * r {
                    // Skin tone + varied colors (simulate hair, eyes, skin)
                    data[off] = (180 + (y % 40) as u8).min(240);     // R varies
                    data[off + 1] = (140 + (x % 30) as u8).min(220); // G varies
                    data[off + 2] = (120 + ((x + y) % 20) as u8).min(200); // B varies
                } else {
                    data[off] = BG_R; data[off + 1] = BG_G; data[off + 2] = BG_B;
                }
                data[off + 3] = 255;
            }
        }
        let frame = RgbaFrame { width: w, height: h, data };
        let analysis = analyze(&frame);
        assert!(analysis.is_healthy(),
            "Textured smooth model should be healthy. roughness={:.1}, white={:.2}, diversity={}, verdict={:?}",
            analysis.edge_roughness, analysis.white_ratio, analysis.color_diversity, analysis.verdict());
    }

    #[test]
    fn test_detailed_model_not_false_positive() {
        // Directly test the verdict logic with metrics matching vroid-hairsample-male:
        // roughness=13.7, colors=647, symmetry=0.88 → should be HEALTHY.
        // The old threshold (roughness > 8.0 alone) would false-positive this.
        let detailed_model = FrameAnalysis {
            coverage: 0.33,
            symmetry: 0.88,
            color_diversity: 647,
            is_solid: false,
            is_background_only: false,
            edge_roughness: 13.7,
            white_ratio: 0.07,
        };
        assert_eq!(detailed_model.verdict(), HealthVerdict::Healthy,
            "Detailed model (roughness={:.1}, colors={}, sym={:.2}) should be HEALTHY",
            detailed_model.edge_roughness, detailed_model.color_diversity, detailed_model.symmetry);

        // Truly broken geometry: high roughness + low colors + low symmetry
        let broken_model = FrameAnalysis {
            coverage: 0.25,
            symmetry: 0.55,
            color_diversity: 30,
            is_solid: false,
            is_background_only: false,
            edge_roughness: 15.8,
            white_ratio: 0.15,
        };
        assert_eq!(broken_model.verdict(), HealthVerdict::BrokenGeometry,
            "Broken geometry (roughness={:.1}, colors={}, sym={:.2}) should be BrokenGeometry",
            broken_model.edge_roughness, broken_model.color_diversity, broken_model.symmetry);

        // Edge case: high roughness + high colors but LOW symmetry = still broken
        // (asymmetric chaos with varied colors from random triangle faces)
        let chaotic_model = FrameAnalysis {
            coverage: 0.40,
            symmetry: 0.50,
            color_diversity: 300,
            is_solid: false,
            is_background_only: false,
            edge_roughness: 20.0,
            white_ratio: 0.10,
        };
        assert_eq!(chaotic_model.verdict(), HealthVerdict::BrokenGeometry,
            "Chaotic asymmetric model should still be BrokenGeometry");

        // Edge case: high roughness + low colors but HIGH symmetry = still broken
        // (symmetric exploding mesh with uniform color)
        let uniform_broken = FrameAnalysis {
            coverage: 0.30,
            symmetry: 0.90,
            color_diversity: 20,
            is_solid: false,
            is_background_only: false,
            edge_roughness: 12.0,
            white_ratio: 0.05,
        };
        assert_eq!(uniform_broken.verdict(), HealthVerdict::BrokenGeometry,
            "Uniform-color high-roughness model should still be BrokenGeometry");
    }

    #[test]
    fn test_half_and_half_symmetry() {
        // Left half white, right half black → low symmetry
        let w = 32u32;
        let h = 16u32;
        let mut data = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let off = ((y * w + x) * 4) as usize;
                if x < w / 2 {
                    data[off] = 255; data[off+1] = 255; data[off+2] = 255;
                } else {
                    data[off] = 0; data[off+1] = 0; data[off+2] = 0;
                }
                data[off + 3] = 255;
            }
        }
        let frame = RgbaFrame { width: w, height: h, data };
        let analysis = analyze(&frame);
        assert!(analysis.symmetry < 0.3,
            "Left-white/right-black should have low symmetry, got {:.2}", analysis.symmetry);
    }
}
