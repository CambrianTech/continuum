//! Second light for the avatar renderer — watch an avatar *emote*.
//!
//! `avatar_breathe` proves the idle/breathing path is wired (spine bone rides a
//! sine). This proves the **expression** path: `SetEmotion` → `EmotionAnimation`
//! transition → `animate_expression` drives blend-shape `MorphWeights` → the
//! face changes in the rendered frame. Emotion morphs are facial, so the signal
//! is a pixel change concentrated in the face region — NOT the whole-silhouette
//! drift breathing produces.
//!
//! The method controls for breathing jitter honestly: it captures a run of
//! NEUTRAL frames and measures the MEDIAN frame-to-frame face-region delta (a
//! blink-robust breathing floor), then sets an emotion and measures the emoting
//! frames against a neutral reference with per-pixel MAD over the interior
//! eyes-to-mouth box. A NOT-wired avatar sits at ~1× the floor (frames identical
//! modulo breathing); a wired emotion clears ~2.4× repeatably, so ≥2× is the
//! gate. The rendered PNGs are the authoritative proof — the scalar is the
//! automated sanity gate around them (breathing translates the whole head, so no
//! static box fully separates morph from bob; the eye is ground truth).
//!
//! Run (from anywhere — it relocates CWD to the repo's `tools/` itself):
//!   export CARGO_TARGET_DIR="$HOME/.continuum/cache/cargo-target"
//!   cargo run --example avatar_emote --features metal,accelerate -- [identity] [emotion]
//!
//! emotion ∈ {happy, sad, angry, surprised, relaxed} (default happy).
//! Frames land in `~/.continuum/avatars/emote/<identity>/`.

use continuum_core::live::avatar::catalog::avatar_model_path;
use continuum_core::live::avatar::frame::{AvatarConfig, RgbaFrame};
use continuum_core::live::avatar::render_loop::allocate_bevy_slot;
use continuum_core::live::avatar::selection::select_avatar_by_identity;
use continuum_core::live::video::bevy_renderer::{get_or_init, Emotion};
use std::time::{Duration, Instant};

/// Parse the emotion arg into the renderer enum. Unknown → fail loud (name the
/// bad value + the valid set), never silently default.
fn parse_emotion(s: &str) -> Result<Emotion, String> {
    match s.to_ascii_lowercase().as_str() {
        "happy" => Ok(Emotion::Happy),
        "sad" => Ok(Emotion::Sad),
        "angry" => Ok(Emotion::Angry),
        "surprised" => Ok(Emotion::Surprised),
        "relaxed" => Ok(Emotion::Relaxed),
        other => Err(format!(
            "unknown emotion '{other}' — valid: happy|sad|angry|surprised|relaxed"
        )),
    }
}

fn main() -> Result<(), String> {
    let identity = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "asha".to_string());
    let emotion_arg = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "happy".to_string());
    let emotion = parse_emotion(&emotion_arg)?;

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
        return Err(format!("VRM not found: {}", vrm_path.display()));
    }

    println!(
        "🙂 avatar_emote: identity='{identity}' model='{}' emotion={emotion:?}",
        model.filename
    );

    let out_dir = dirs_home().join(".continuum/avatars/emote").join(&identity);
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("create out dir: {e}"))?;

    let config = AvatarConfig {
        identity: identity.clone(),
        display_name: identity.clone(),
        width: 640,
        height: 360,
        fps: 15.0,
        vrm_model_path: Some(vrm_path.to_string_lossy().to_string()),
        preference: Default::default(),
    };

    println!("   booting Bevy + loading VRM (cold-load can take >6s)…");
    let allocation = allocate_bevy_slot(config)?;
    let system = get_or_init();

    // 1) Warm up past model-load/black frames, then collect a run of NEUTRAL
    //    frames — the face-region noise floor (breathing + blinking). The window
    //    must clear a cold VRM decode (>6s) before frames even begin arriving.
    let neutral: Vec<RgbaFrame> = drain_frames(&allocation, Duration::from_secs(15), 24);
    if neutral.len() < 4 {
        return Err(format!(
            "only {} neutral frames captured — renderer never warmed up",
            neutral.len()
        ));
    }
    let (w, h) = infer_dims(&neutral[0])?;
    let neutral_floor = median_pairwise_face_delta(&neutral, w, h);
    // Borrow the last neutral frame as the reference; `neutral` outlives all
    // uses below (RgbaFrame isn't Clone, so this is a borrow, not a copy).
    let neutral_ref = neutral.last().unwrap();
    save_png(&neutral_ref.data, w, h, &out_dir.join("neutral.png"))?;

    // 2) Drive the emotion and let the transition settle (transition_ms=300).
    if !system.set_emotion_by_identity(&identity, emotion, 1.0, 300) {
        return Err(format!(
            "set_emotion_by_identity returned false — no slot registered for '{identity}'"
        ));
    }
    println!("   sent SetEmotion({emotion:?}, weight=1.0); settling…");

    // 3) Collect emoting frames after the transition ramps in.
    let emoting: Vec<RgbaFrame> = drain_frames(&allocation, Duration::from_secs(3), 18);
    if emoting.is_empty() {
        return Err("no emoting frames captured".to_string());
    }
    save_png(
        &emoting.last().unwrap().data,
        w,
        h,
        &out_dir.join(format!("{emotion_arg}.png")),
    )?;

    drop(allocation); // RAII unload

    // 4) Signal: per-pixel face-region change of each emoting frame vs the
    //    neutral reference, peaked over the run (the expression ramps in, so the
    //    max is the payoff). Per-pixel MAD — NOT diff-of-means, which cancels a
    //    localized morph (a smile darkens the mouth but barely moves the average).
    let emote_delta = emoting
        .iter()
        .map(|f| face_region_mad(neutral_ref, f, w, h))
        .fold(0.0_f32, f32::max);

    println!("\n📊 face-region signal:");
    println!("   neutral noise floor (breathing/blinking): {neutral_floor:.3}");
    println!("   {emotion:?} vs neutral (peak):             {emote_delta:.3}");

    // Threshold calibrated to the measured null-vs-signal gap. Breathing
    // *translates the whole head*, so at high-contrast features (brows, eye
    // edges) even a 1-2px bob yields large per-pixel diffs that no static box
    // removes — the morph and the bob share pixels. Empirically: a NOT-wired
    // avatar sits at ~1.0× (frames identical modulo breathing = the floor
    // itself); a wired emotion clears ~2.4× repeatably. 2.0× cleanly separates
    // the two. The rendered PNGs are the authoritative proof — this scalar is
    // the automated sanity gate around them.
    let margin = emote_delta / neutral_floor.max(f32::EPSILON);
    if margin >= 2.0 {
        println!(
            "   ✅ emoting is wired — {emotion:?} moved the face {margin:.1}× the breathing floor"
        );
        println!(
            "   authoritative proof → {}/neutral.png vs {emotion_arg}.png",
            out_dir.display()
        );
        Ok(())
    } else {
        Err(format!(
            "⚠️ face barely moved ({margin:.1}× breathing floor, need ≥2×) — the {emotion:?} \
             blend shapes may not have been discovered on this VRM, or animate_expression \
             didn't run. Compare {}/neutral.png vs {}/{emotion_arg}.png.",
            out_dir.display(),
            out_dir.display()
        ))
    }
}

/// Drain frames for `dur`, keeping up to `max` of them (skips the initial black
/// warmup implicitly by starting after allocation settled at the call site).
fn drain_frames(
    allocation: &continuum_core::live::avatar::render_loop::BevySlotAllocation,
    dur: Duration,
    max: usize,
) -> Vec<RgbaFrame> {
    let mut kept = Vec::new();
    let start = Instant::now();
    let mut seen = 0u32;
    while start.elapsed() < dur {
        while let Ok(frame) = allocation.frame_rx.try_recv() {
            seen += 1;
            // Skip the first 20 frames (model-load / initial black), then keep
            // every 3rd to spread the sample across the window.
            if seen > 20 && seen % 3 == 0 && kept.len() < max {
                kept.push(frame);
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    kept
}

/// The face region as pixel bounds: the INTERIOR eyes-to-mouth box, kept inside
/// the silhouette so breathing's head-bob (which moves the hair/jaw *edges*
/// against the dark backdrop) doesn't dominate. Emotion morphs (brows, eyes,
/// mouth) all live inside this box, so it isolates the morph from the bob.
fn face_bounds(w: u32, h: u32) -> (u32, u32, u32, u32) {
    let x0 = (w as f32 * 0.40) as u32;
    let x1 = (w as f32 * 0.60) as u32;
    let y0 = (h as f32 * 0.30) as u32;
    let y1 = (h as f32 * 0.58) as u32;
    (x0, x1, y0, y1)
}

/// Mean per-pixel absolute RGB difference between two pixel-aligned frames over
/// the face region. Per-pixel (not diff-of-means) so a localized morph — a smile
/// that darkens only the mouth — registers its full change instead of averaging
/// away. Both frames are the same render slot at fixed resolution, so pixels
/// align 1:1.
fn face_region_mad(a: &RgbaFrame, b: &RgbaFrame, w: u32, h: u32) -> f32 {
    let (x0, x1, y0, y1) = face_bounds(w, h);
    let mut sum = 0.0f64;
    let mut n = 0u64;
    for y in y0..y1 {
        for x in x0..x1 {
            let i = ((y * w + x) * 4) as usize;
            if i + 2 < a.data.len() && i + 2 < b.data.len() {
                sum += a.data[i].abs_diff(b.data[i]) as f64;
                sum += a.data[i + 1].abs_diff(b.data[i + 1]) as f64;
                sum += a.data[i + 2].abs_diff(b.data[i + 2]) as f64;
                n += 1;
            }
        }
    }
    (sum / n.max(1) as f64) as f32
}

/// MEDIAN frame-to-frame face-region MAD across a run — the robust noise floor
/// of steady breathing. The median rejects the intermittent blink outliers (a
/// blink is a big, brief eye-region change that would inflate the *mean*), so
/// the floor reflects the typical resting motion a *sustained* emotion morph
/// must clear.
fn median_pairwise_face_delta(frames: &[RgbaFrame], w: u32, h: u32) -> f32 {
    if frames.len() < 2 {
        return 0.0;
    }
    let mut deltas: Vec<f32> = frames
        .windows(2)
        .map(|pair| face_region_mad(&pair[0], &pair[1], w, h))
        .collect();
    deltas.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    deltas[deltas.len() / 2]
}

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
            "cannot determine frame dims: {} bytes",
            frame.data.len()
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
