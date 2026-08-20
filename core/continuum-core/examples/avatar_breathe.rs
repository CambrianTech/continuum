//! First light for the avatar renderer — watch an avatar breathe.
//!
//! Boots the headless Bevy renderer, loads a persona's VRM, and captures a
//! *sequence* of frames over a few seconds so the idle breathing animation is
//! observable: the spine bone rides a slow sine wave (`bevy_renderer::animation::
//! breathing`, PORTRAIT_PROFILE ~0.8 Hz), so the avatar's silhouette rises and
//! falls. A single still (what `avatar/snapshot` grabs) can't show this — motion
//! needs a strip of frames.
//!
//! This is the same allocate → drain-frames path the snapshot module uses
//! (`modules/avatar.rs::capture_snapshot`), extended from one frame to a strip.
//! It doubles as a rig sanity check: breathing silently no-ops if the VRM's
//! spine bone matches none of the naming schemes in `breathing.rs`, so a flat
//! centroid signal below is a real "the rig didn't wire up" signal, not noise.
//!
//! Run (from anywhere — it relocates CWD to the repo's `tools/` itself):
//!   export CARGO_TARGET_DIR="$HOME/.continuum/cache/cargo-target"
//!   cargo run --example avatar_breathe --features metal,accelerate -- [identity]
//!
//! Frames land in `~/.continuum/avatars/breathe/<identity>/frame-NNN.png`.

use continuum_core::live::avatar::catalog::avatar_model_path;
use continuum_core::live::avatar::frame::{AvatarConfig, RgbaFrame};
use continuum_core::live::avatar::render_loop::allocate_bevy_slot;
use continuum_core::live::avatar::selection::select_avatar_by_identity;
use std::time::{Duration, Instant};

/// How long to observe, and which frames to keep. Overridable via env for
/// bring-up (a cold 17 MB VRM decode + GPU upload can take >6s before the
/// slot goes active and the first frame reads back).
fn observe_secs() -> u64 {
    std::env::var("BREATHE_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20)
}
/// Frames before this are model-load / initial-black; skip them (matches the
/// snapshot module's 30-frame warmup window).
fn warmup_frames() -> u32 {
    std::env::var("BREATHE_WARMUP")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30)
}
/// Keep every Nth post-warmup frame as a PNG (15 fps → every 3rd ≈ 5/sec).
const KEEP_EVERY: u32 = 3;
/// Cap the frames retained for the GIF so a long observe window can't balloon
/// memory (90 kept frames ≈ 18s of loop at 5/sec, ~80 MB of RGBA — plenty).
const GIF_MAX_FRAMES: usize = 90;

fn main() -> Result<(), String> {
    let identity = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "asha".to_string());

    // Both the catalog (`avatar_model_path`) and Bevy's AssetServer resolve
    // `models/avatars/<file>.vrm` relative to CWD. The VRMs live under the
    // repo's `tools/`, so anchor there — CARGO_MANIFEST_DIR is core/continuum-core.
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .ok_or("cannot locate repo root from CARGO_MANIFEST_DIR")?;
    let tools_dir = repo_root.join("tools");
    std::env::set_current_dir(&tools_dir)
        .map_err(|e| format!("set CWD to {}: {e}", tools_dir.display()))?;

    let model = select_avatar_by_identity(&identity);
    let vrm_path = avatar_model_path(model.filename);
    if !vrm_path.exists() {
        return Err(format!(
            "VRM not found: {} (CWD={})\n\
             Expected the repo's avatar VRMs at tools/models/avatars/*.vrm.",
            vrm_path.display(),
            tools_dir.display()
        ));
    }

    println!(
        "🫁 avatar_breathe: identity='{identity}' model='{}' ({:?})",
        model.filename, model.voice_profile.gender
    );
    println!("   VRM: {}", vrm_path.display());

    let out_dir = dirs_home()
        .join(".continuum/avatars/breathe")
        .join(&identity);
    std::fs::create_dir_all(&out_dir)
        .map_err(|e| format!("create out dir {}: {e}", out_dir.display()))?;

    let config = AvatarConfig {
        identity: identity.clone(),
        display_name: identity.clone(),
        width: 640,
        height: 360,
        fps: 15.0,
        vrm_model_path: Some(vrm_path.to_string_lossy().to_string()),
        preference: Default::default(),
    };

    let observe = Duration::from_secs(observe_secs());
    let warmup = warmup_frames();
    println!(
        "   booting Bevy + loading VRM (observing {}s; VRM cold-load can take >6s)…",
        observe.as_secs()
    );
    let allocation = allocate_bevy_slot(config)?;

    let mut received = 0u32;
    let mut kept = 0u32;
    // (frame index, vertical centroid of the avatar silhouette). The centroid
    // riding up and down IS the breath — printed as a sparkline at the end.
    let mut signal: Vec<(u32, f32)> = Vec::new();
    // Retained RGBA buffers for the animated GIF (breath is sub-pixel in a
    // still; only a moving loop makes it legible). Capped so a long observe
    // window can't balloon memory.
    let mut gif_frames: Vec<(Vec<u8>, u32, u32)> = Vec::new();
    let start = Instant::now();
    let mut next_beat = Duration::from_secs(1);

    while start.elapsed() < observe {
        while let Ok(frame) = allocation.frame_rx.try_recv() {
            received += 1;
            if received <= warmup {
                continue;
            }
            if (received - warmup) % KEEP_EVERY != 0 {
                continue;
            }
            let (w, h) = infer_dims(&frame)?;
            if let Some(centroid) = silhouette_centroid(&frame.data, w, h) {
                signal.push((received, centroid));
            }
            let path = out_dir.join(format!("frame-{kept:03}.png"));
            save_png(&frame.data, w, h, &path)?;
            if gif_frames.len() < GIF_MAX_FRAMES {
                gif_frames.push((frame.data.clone(), w, h));
            }
            kept += 1;
        }
        if start.elapsed() >= next_beat {
            eprintln!(
                "   … {:>2}s: {received} frames received, {kept} kept",
                start.elapsed().as_secs()
            );
            next_beat += Duration::from_secs(1);
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    // SlotGuard drops here (RAII) → model unloads, slot returns to the pool.
    drop(allocation);

    if kept == 0 {
        return Err(format!(
            "no frames kept ({received} received in {}ms) — renderer never produced a usable frame",
            start.elapsed().as_millis()
        ));
    }

    println!(
        "\n✅ kept {kept} frames ({received} total) → {}",
        out_dir.display()
    );

    // Assemble the retained frames into a looping GIF — the only way the
    // sub-pixel breath is actually watchable.
    let gif_path = out_dir.join("breathe.gif");
    match save_gif(&gif_frames, &gif_path) {
        Ok(()) => println!("🎞️  breathing loop → {}", gif_path.display()),
        Err(e) => eprintln!("⚠️  gif encode skipped: {e}"),
    }

    report_breath(&signal);
    Ok(())
}

/// Encode the retained RGBA frames into a single looping GIF. Breathing moves
/// the silhouette by ~2px frame-to-frame, invisible in a still; the loop is
/// what makes it read as a live idle animation. Uses the `image` crate's gif
/// codec (in the default feature set), so no external ffmpeg/imagemagick.
fn save_gif(frames: &[(Vec<u8>, u32, u32)], path: &std::path::Path) -> Result<(), String> {
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Frame};

    if frames.is_empty() {
        return Err("no frames retained".to_string());
    }
    let file =
        std::fs::File::create(path).map_err(|e| format!("create {}: {e}", path.display()))?;
    let mut encoder = GifEncoder::new(std::io::BufWriter::new(file));
    encoder
        .set_repeat(Repeat::Infinite)
        .map_err(|e| format!("set repeat: {e}"))?;
    // ~15 fps kept-frame cadence was every 3rd of 15 → 5/sec → 200ms/frame.
    let delay = Delay::from_numer_denom_ms(200, 1);
    for (rgba, w, h) in frames {
        let buf = image::RgbaImage::from_raw(*w, *h, rgba.clone())
            .ok_or("invalid frame dimensions for gif buffer")?;
        encoder
            .encode_frame(Frame::from_parts(buf, 0, 0, delay))
            .map_err(|e| format!("encode frame: {e}"))?;
    }
    Ok(())
}

/// Vertical centroid (0.0 = top, 1.0 = bottom) of the avatar silhouette:
/// the mean row of every pixel that differs from the solid backdrop (sampled
/// from the top-left corner). As the spine breathes, the silhouette's mass
/// shifts by a sub-pixel amount frame-to-frame — the centroid makes it legible.
fn silhouette_centroid(rgba: &[u8], w: u32, h: u32) -> Option<f32> {
    if rgba.len() < 4 || rgba.len() != (w * h * 4) as usize {
        return None;
    }
    let (br, bg, bb) = (rgba[0], rgba[1], rgba[2]);
    let mut row_sum: f64 = 0.0;
    let mut count: u64 = 0;
    for y in 0..h {
        let row = (y * w * 4) as usize;
        for x in 0..w {
            let i = row + (x * 4) as usize;
            let dr = rgba[i].abs_diff(br) as u32;
            let dg = rgba[i + 1].abs_diff(bg) as u32;
            let db = rgba[i + 2].abs_diff(bb) as u32;
            if dr + dg + db > 24 {
                row_sum += y as f64;
                count += 1;
            }
        }
    }
    if count == 0 {
        return None;
    }
    Some((row_sum / count as f64) as f32 / h as f32)
}

/// Print the breathing signal as a sparkline + peak-to-peak amplitude so the
/// motion is legible in the terminal even before opening the PNGs.
fn report_breath(signal: &[(u32, f32)]) {
    if signal.len() < 2 {
        println!("⚠️  too few frames to read a breathing signal");
        return;
    }
    let vals: Vec<f32> = signal.iter().map(|(_, v)| *v).collect();
    let min = vals.iter().cloned().fold(f32::INFINITY, f32::min);
    let max = vals.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let span = (max - min).max(f32::EPSILON);
    let bars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let spark: String = vals
        .iter()
        .map(|v| {
            let t = ((v - min) / span * (bars.len() - 1) as f32).round() as usize;
            bars[t.min(bars.len() - 1)]
        })
        .collect();
    println!("🫁 breath (silhouette centroid over time):");
    println!("   {spark}");
    println!(
        "   peak-to-peak {:.4} of frame height across {} frames{}",
        max - min,
        vals.len(),
        if max - min < 1e-4 {
            "  ⚠️ flat — spine bone may not have matched the rig"
        } else {
            "  ✅ moving — the avatar is breathing"
        }
    );
}

/// Readback resolution can differ from requested (mirrors snapshot's inference).
fn infer_dims(frame: &RgbaFrame) -> Result<(u32, u32), String> {
    let pixels = frame.data.len() / 4;
    if (frame.width * frame.height) as usize == pixels {
        return Ok((frame.width, frame.height));
    }
    let w = (pixels as f64).sqrt() as u32;
    let h = pixels as u32 / w.max(1);
    if (w * h) as usize == pixels {
        Ok((w, h))
    } else {
        Err(format!(
            "cannot determine frame dims: {} bytes ({pixels} px), reported {}x{}",
            frame.data.len(),
            frame.width,
            frame.height
        ))
    }
}

fn save_png(rgba: &[u8], w: u32, h: u32, path: &std::path::Path) -> Result<(), String> {
    let img = image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(w, h, rgba.to_vec())
        .ok_or("invalid frame dimensions for image buffer")?;
    img.save(path)
        .map_err(|e| format!("save {}: {e}", path.display()))
}

fn dirs_home() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}
