//! vision-qa — OUR input-side vision benchmark: SEE an image, answer a question.
//!
//! The tier public vision benchmarks lack (they grade a raw VLM's forward pass,
//! not a CITIZEN using her sensory system): each task stages a generated PNG
//! into the workspace, the persona must LOOK at it with her own eyes
//! (`vision/look` — the same description bridge every citizen's sight runs
//! through) and answer an objective question graded by held-out substring.
//! Measures the WHOLE sensory loop: tool-act → sidecar describe → reason →
//! answer. A citizen whose eyes are broken scores zero here — which is exactly
//! the honest signal (the 2026-08-26 sightless-sidecar bug would have been
//! caught by this gym on day one).
//!
//! CONTAMINATION-FREE BY CONSTRUCTION ([[humaneval-is-prohibited...]]): the
//! images are generated deterministically in this file — colored shapes on
//! white, counts ≤ 4, six unambiguous colors — so no model has ever seen them,
//! and the oracle (`expect`) is derived from the same parameters and held out
//! of the prompt. Deliberately EASY for real eyes: this gym measures that the
//! sensory pipeline works end to end, not vision acuity; harder tiers (charts,
//! rendered tables, screenshots) stack on the same generator seam later.

use std::sync::OnceLock;

/// The six colors the describer models name reliably. (Color words are also the
/// grading substrings, so they must be unambiguous common words.)
const COLORS: [([u8; 3], &str); 6] = [
    ([220, 30, 30], "red"),
    ([30, 60, 220], "blue"),
    ([25, 160, 60], "green"),
    ([240, 200, 20], "yellow"),
    ([140, 40, 180], "purple"),
    ([245, 130, 20], "orange"),
];

const SHAPES: [&str; 3] = ["circle", "square", "triangle"];

/// Tiny deterministic LCG — no rand dependency, byte-stable gyms forever.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % bound
    }
}

/// One placed shape: kind index, color index, cell (0..4 in a 2×2 grid).
struct Placed {
    shape: usize,
    color: usize,
    cell: usize,
}

fn render_png(shapes: &[Placed]) -> Vec<u8> {
    const W: u32 = 320;
    const H: u32 = 320;
    let mut img = image::RgbaImage::from_pixel(W, H, image::Rgba([255, 255, 255, 255]));
    for p in shapes {
        let (cx, cy) = match p.cell {
            0 => (80i32, 80i32),
            1 => (240, 80),
            2 => (80, 240),
            _ => (240, 240),
        };
        let col = COLORS[p.color].0;
        let px = image::Rgba([col[0], col[1], col[2], 255]);
        let r = 55i32;
        for y in (cy - r)..(cy + r) {
            for x in (cx - r)..(cx + r) {
                if x < 0 || y < 0 || x >= W as i32 || y >= H as i32 {
                    continue;
                }
                let (dx, dy) = (x - cx, y - cy);
                let inside = match p.shape {
                    0 => dx * dx + dy * dy <= r * r,                  // circle
                    1 => dx.abs() <= r - 8 && dy.abs() <= r - 8,      // square
                    _ => {
                        // upright triangle: apex at top, base at bottom
                        let fy = (dy + r) as f32 / (2 * r) as f32;    // 0 top → 1 bottom
                        dy.abs() <= r && (dx.abs() as f32) <= fy * r as f32
                    }
                };
                if inside {
                    img.put_pixel(x as u32, y as u32, px);
                }
            }
        }
    }
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)
        .expect("in-memory PNG encode cannot fail");
    out.into_inner()
}

/// The generated gym as JSONL — one build per process, byte-stable across runs.
pub fn vision_qa_jsonl() -> &'static str {
    static GYM: OnceLock<String> = OnceLock::new();
    GYM.get_or_init(|| {
        use base64::Engine as _;
        let mut rng = Lcg(0x5EE_C1712E4 ^ 0x2026_08_26);
        let mut rows = Vec::new();
        for i in 0..16u32 {
            // 2–4 shapes, all colors distinct, all cells distinct.
            let n = 2 + rng.next(3);
            let mut cells = vec![0usize, 1, 2, 3];
            let mut colors: Vec<usize> = (0..COLORS.len()).collect();
            let mut placed = Vec::new();
            for _ in 0..n {
                let cell = cells.remove(rng.next(cells.len()));
                let color = colors.remove(rng.next(colors.len()));
                placed.push(Placed {
                    shape: rng.next(SHAPES.len()),
                    color,
                    cell,
                });
            }
            // Question type rotates; each is objective against the placement.
            let (question, expect) = match i % 3 {
                0 => {
                    // color of a uniquely-shaped item (ensure uniqueness)
                    let target = placed[rng.next(placed.len())].shape;
                    if placed.iter().filter(|p| p.shape == target).count() != 1 {
                        // fall back to counting when not unique
                        (
                            "How many shapes are in the image?".to_string(),
                            n.to_string(),
                        )
                    } else {
                        let p = placed.iter().find(|p| p.shape == target).unwrap();
                        (
                            format!("What color is the {}?", SHAPES[p.shape]),
                            COLORS[p.color].1.to_string(),
                        )
                    }
                }
                1 => (
                    "How many shapes are in the image?".to_string(),
                    n.to_string(),
                ),
                _ => {
                    let p = &placed[rng.next(placed.len())];
                    (
                        format!("What shape is the {} one?", COLORS[p.color].1),
                        SHAPES[p.shape].to_string(),
                    )
                }
            };
            let png = render_png(&placed);
            let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
            let row = serde_json::json!({
                "id": format!("vision-qa-{i:02}"),
                "prompt": format!(
                    "Your workspace contains an image at .gymtool/look.png. LOOK at it \
                     with your vision/look tool (file_path=\".gymtool/look.png\", \
                     focus=\"the shapes, their colors, and how many there are\") — do not \
                     guess without looking. Then answer this question about the image: \
                     {question} Reply with just the answer."
                ),
                "expect": expect,
                "needs_tools": true,
                "setup_shell": format!(
                    "mkdir -p .gymtool && printf '%s' '{b64}' | openssl base64 -d -A \
                     > .gymtool/look.png"
                ),
            });
            rows.push(row.to_string());
        }
        rows.join("\n")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the gym must be deterministic (byte-stable across
    // calls/processes — grading history is only comparable against the same
    // battery), every row must parse as an EvalTask with a real image and a
    // held-out oracle, and the oracle must never leak into the prompt.
    #[test]
    fn gym_is_deterministic_parseable_and_oracle_held_out() {
        let a = vision_qa_jsonl();
        let b = vision_qa_jsonl();
        assert!(std::ptr::eq(a, b) || a == b, "one build per process, stable");
        let mut n = 0;
        for line in a.lines() {
            let task: crate::cognition::eval::EvalTask =
                serde_json::from_str(line).expect("row parses as EvalTask");
            assert!(!task.expect.is_empty(), "oracle present");
            assert!(
                !task
                    .prompt
                    .to_ascii_lowercase()
                    .contains(&format!("answer: {}", task.expect)),
                "oracle must not leak into the prompt"
            );
            let setup = task.setup_shell.expect("stages the image");
            assert!(setup.contains("look.png") && setup.contains("openssl base64"));
            assert_eq!(task.needs_tools, Some(true), "seeing is an ACT — tools offered");
            n += 1;
        }
        assert_eq!(n, 16);
    }

    // what this catches: the renderer actually paints the declared shapes — a
    // blank or single-color PNG would grade every citizen zero and read as a
    // capability failure instead of a generator bug.
    #[test]
    fn rendered_pngs_contain_the_declared_colors() {
        let placed = vec![
            Placed { shape: 0, color: 0, cell: 0 },
            Placed { shape: 1, color: 1, cell: 3 },
        ];
        let png = render_png(&placed);
        let img = image::load_from_memory(&png).expect("valid png").to_rgba8();
        let has = |rgb: [u8; 3]| {
            img.pixels()
                .any(|p| p.0[0] == rgb[0] && p.0[1] == rgb[1] && p.0[2] == rgb[2])
        };
        assert!(has(COLORS[0].0), "red circle painted");
        assert!(has(COLORS[1].0), "blue square painted");
        assert!(has([255, 255, 255]), "white background");
    }
}
